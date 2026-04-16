//! u256 math sequences — the direct counterparts to Percolator's `wide_math.rs`.
//!
//! The Path 3 convention (see `MULTIPRECISION_DSL_DECISION.md`): each helper in
//! this module is an **emitter** — it appends opcode bytes to a [`Program`]
//! builder — or a **driver** — it builds a complete test program that bakes in
//! operands and runs to a single-u64 return.
//!
//! Drivers are used for unit tests against a native Rust reference.
//! Emitters are the composable primitives for the real Percolator port, which
//! will push operands from account data rather than baking them in.
//!
//! Every op has the same stack contract as the underlying VM opcode:
//!
//! | Op | Pops | Pushes |
//! |----|------|--------|
//! | ADD/SUB/MUL/DIV u256 wrapping | 8 u64 limbs (a0..a3, b0..b3) | 4 u64 limbs (r0..r3) |
//! | same, checked (flag=1) | 8 u64 limbs | 4 u64 limbs + 1 bool |
//! | CMP u256 | 8 u64 limbs | 1 u64 (0=lt, 1=eq, 2=gt) |
//! | MULDIV u256 | 12 u64 limbs (a, b, c) | 4 u64 limbs |
//!
//! The `program_*` drivers in this file all return a single u64 via `HALT`.
//! When multiple result limbs are possible, a `_return_limb` variant takes a
//! `limb_to_return ∈ 0..=3` argument and drops the other limbs.
//!
//! # Percolator mapping
//!
//! These cover the u256 operations actually used by
//! `hello_slab/percolator/src/percolator.rs` (counted by grep, 2026-04-15):
//!
//! - `U256::checked_add` / `checked_sub` / `checked_mul` / `checked_div` / `checked_rem`
//! - `U256::saturating_add` / `saturating_sub`
//! - `U256::is_zero` / `try_into_u128`
//! - `mul_div_floor_u256_with_rem` / `div_rem_u256`
//! - `ceil_div_positive_checked`
//!
//! Operations that require conditionals (e.g. `saturating_add` needs a
//! branch on overflow) are more than two bytes of bytecode and use the
//! `emit_*` API to compose jump + arithmetic opcodes.

use super::emit::Program;
use five_protocol::opcodes::{ADD_U256, DROP, SUB_U256, SWAP};

// =============================================================================
// Flags
// =============================================================================

pub const FLAG_WRAPPING: u8 = 0x00;
pub const FLAG_CHECKED: u8 = 0x01;

// =============================================================================
// Simple drivers — each builds a complete single-return test program
// =============================================================================

/// `a + b` (wrapping). Returns a specified result limb. `limb_to_return` ∈ `0..=3`.
pub fn program_add_u256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_add_u256(FLAG_WRAPPING);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a + b` (checked). Returns the overflow bool (0 = no overflow, 1 = overflow).
pub fn program_add_u256_checked_return_overflow(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_add_u256(FLAG_CHECKED);
    // Stack after: [r0, r1, r2, r3, overflow_bool]. Drop the limbs, keep the bool.
    for _ in 0..4 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// `a - b` (wrapping). Returns a specified result limb.
pub fn program_sub_u256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_sub_u256(FLAG_WRAPPING);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a - b` (checked). Returns the underflow bool.
pub fn program_sub_u256_checked_return_underflow(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_sub_u256(FLAG_CHECKED);
    for _ in 0..4 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// `a * b` (wrapping). Returns a specified result limb.
pub fn program_mul_u256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_mul_u256(FLAG_WRAPPING);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a * b` (checked). Returns overflow bool.
pub fn program_mul_u256_checked_return_overflow(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_mul_u256(FLAG_CHECKED);
    for _ in 0..4 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// `a / b` quotient. Panics the VM on `b == 0`. Returns a specified quotient limb.
pub fn program_div_u256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_div_u256(FLAG_WRAPPING);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `(a * b) / c`. Returns a specified result limb.
pub fn program_muldiv_u256_return_limb(
    a: [u64; 4],
    b: [u64; 4],
    c: [u64; 4],
    limb_to_return: u8,
) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a)
        .push_u256(b)
        .push_u256(c)
        .emit_muldiv_u256(FLAG_WRAPPING);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `cmp(a, b)` as `u64`: 0 = lt, 1 = eq, 2 = gt.
pub fn program_cmp_u256(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b).emit_cmp_u256(FLAG_WRAPPING);
    p.finish_with_halt()
}

/// `is_zero(a)` as `u64` (1 if zero, 0 otherwise).
///
/// Emitted as `CMP_U256(a, ZERO) == 1` via subtraction:
/// `1 - abs(1 - cmp)` would work with arithmetic, but simpler: push expected,
/// CMP, and return whether result equals 1. Since we need a boolean, the
/// caller can compare the CMP result against 1 themselves — this program
/// returns the raw CMP result for the pattern `cmp == 1`.
pub fn program_is_zero_u256(a: [u64; 4]) -> Vec<u8> {
    program_cmp_u256(a, [0u64; 4])
    // Caller interprets: returned value == 1 ⇔ a is zero.
}

/// `try_into_u128(a)` — returns the low limb (`a[0]`) and leaves high limbs
/// discarded. Callers check externally whether `a[2]` and `a[3]` are zero; if
/// so, the u64 returned here plus a second program for `a[1]` together form
/// the u128.
///
/// A more compositional solution once function-calling is wired will combine
/// check + extract in one routine returning `(Option<u128>)` via account
/// writes.
pub fn program_try_into_u128_lo(a: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u64(a[0]);
    p.finish_with_halt()
}

// =============================================================================
// Saturating variants — bytecode form via JUMP_IF on the checked-overflow flag
//
// `saturating_add(a, b)` semantics (Percolator wide_math.rs):
//   match a.checked_add(b) { Some(r) => r, None => U256::MAX }
//
// Stack shape after `ADD_U256 CHECKED` is `[r0, r1, r2, r3, overflow]`. We pop
// `overflow` via `JUMP_IF_NOT`, branch to the "no-saturate" path on success,
// otherwise drop the result limbs and push the saturation constant.
// =============================================================================

/// `saturating_add(a, b)` returning a specified result limb (0..=3).
/// Saturates the full 256-bit result to `U256::MAX` on overflow.
pub fn program_saturating_add_u256_return_limb(
    a: [u64; 4],
    b: [u64; 4],
    limb_to_return: u8,
) -> Vec<u8> {
    assert!(limb_to_return < 4);
    saturating_program(a, b, ADD_U256, [u64::MAX; 4], limb_to_return)
}

/// `saturating_sub(a, b)` returning a specified result limb (0..=3).
/// Saturates to `[0; 4]` on underflow.
pub fn program_saturating_sub_u256_return_limb(
    a: [u64; 4],
    b: [u64; 4],
    limb_to_return: u8,
) -> Vec<u8> {
    assert!(limb_to_return < 4);
    saturating_program(a, b, SUB_U256, [0u64; 4], limb_to_return)
}

fn saturating_program(
    a: [u64; 4],
    b: [u64; 4],
    op: u8,
    saturate_to: [u64; 4],
    limb_to_return: u8,
) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    // Run the checked op: stack ends as [r0, r1, r2, r3, overflow_or_underflow]
    p.raw_bytes(&[op, FLAG_CHECKED]);
    // JUMP_IF_NOT pops the bool; if false (no overflow) skip the saturate branch.
    let no_sat = p.emit_jump_if_not_placeholder();
    // Overflow / underflow path: drop the four wrapping result limbs, push
    // saturation constant in the same low-to-high order.
    for _ in 0..4 {
        p.raw(DROP);
    }
    p.push_u256(saturate_to);
    // Both paths converge here with [r0, r1, r2, r3] on the stack.
    p.patch_jump_to_here(no_sat);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

// =============================================================================
// (Original) Saturating variants — Rust references kept as oracles
// =============================================================================
//
// NOTE: implementing saturating via a true branch requires JUMP_IF / JUMP
// opcodes. For the current scaffolding we expose a pure-Rust fallback that
// the `bytecode/link.rs` layer will call when producing a merged binary. This
// mirrors how the Rust reference's `saturating_add` is implemented as
// `match self.checked_add(rhs) { Some(v) => v, None => MAX }`.
//
// Once the architectural decision for account-offset-based intrinsics lands,
// saturating ops will become a 2-step bytecode sequence: checked op, then a
// conditional store that writes MAX/MIN on overflow. For now each bytecode
// emitter here returns `None`, signaling callers to fall back to the Rust
// implementation. The `program_saturating_*_reference` helpers are kept in
// this module so tests can exercise the target semantics unchanged.

/// Rust reference for `saturating_add_u256`. Used to unit-test the target
/// semantics until the branching bytecode lands.
pub fn saturating_add_u256_reference(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let sum = overflowing_add_u256_ref(a, b);
    if sum.1 {
        [u64::MAX; 4]
    } else {
        sum.0
    }
}

/// Rust reference for `saturating_sub_u256`. Saturates to `[0; 4]` on underflow.
pub fn saturating_sub_u256_reference(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let diff = overflowing_sub_u256_ref(a, b);
    if diff.1 {
        [0u64; 4]
    } else {
        diff.0
    }
}

fn overflowing_add_u256_ref(a: [u64; 4], b: [u64; 4]) -> ([u64; 4], bool) {
    let mut carry: u128 = 0;
    let mut r = [0u64; 4];
    for i in 0..4 {
        let s = (a[i] as u128) + (b[i] as u128) + carry;
        r[i] = s as u64;
        carry = s >> 64;
    }
    (r, carry != 0)
}

fn overflowing_sub_u256_ref(a: [u64; 4], b: [u64; 4]) -> ([u64; 4], bool) {
    let mut borrow: i128 = 0;
    let mut r = [0u64; 4];
    for i in 0..4 {
        let d = (a[i] as i128) - (b[i] as i128) - borrow;
        let (val, bw) = if d < 0 {
            ((d + (1i128 << 64)) as u64, 1i128)
        } else {
            (d as u64, 0i128)
        };
        r[i] = val;
        borrow = bw;
    }
    (r, borrow != 0)
}

// =============================================================================
// Shared helper: extract a single limb from a 4-limb stack result
// =============================================================================

/// After a u256-producing op, the four result limbs are on the stack with r3 on
/// top. This helper drops the limbs above `target_limb` and the limbs below it,
/// leaving `target_limb` alone on top so `HALT` returns it.
///
/// For `target_limb = 3`: drop nothing above, drop 3 limbs below (r0..r2).
/// For `target_limb = 0`: drop 3 limbs above (r3..r1), leaving r0 on top.
fn drop_above_limb(p: &mut Program, target_limb: u8) {
    // Drop limbs above the target (they sit on top of the stack).
    for _ in 0..(3 - target_limb) {
        p.raw(DROP);
    }
    // At this point target_limb is on top, with `target_limb` lower limbs below.
    // Bubble the target up by swapping past each lower limb, then drop each.
    for _ in 0..target_limb {
        p.raw(SWAP).raw(DROP);
    }
}
