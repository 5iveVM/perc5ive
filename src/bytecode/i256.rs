//! i256 math sequences — counterparts to Percolator's `wide_math.rs` `I256` methods.
//!
//! Two's-complement semantics on `[u64; 4]`. All ops use the `ADD_I256`,
//! `SUB_I256`, `MUL_I256`, `DIV_I256` opcodes (0xCA-0xCD) added in PR #84.
//!
//! # Percolator mapping
//!
//! Methods from `wide_math.rs` (grep-counted usage in `percolator.rs`):
//!
//! - `I256::checked_add` (17 calls) — emit_add_i256 with checked flag
//! - `I256::checked_sub` (7 calls) — emit_sub_i256 with checked flag
//! - `I256::checked_mul_i256` (6 calls) — complex; see `program_checked_mul_i256_TODO`
//! - `I256::checked_neg` (3 calls) — emit_neg (composite of SUB from zero)
//! - `I256::is_negative` (11 calls) — test high-limb MSB
//! - `I256::is_zero` — CMP_U256 against zero
//! - `I256::is_positive` — composite: not zero AND not negative
//! - `I256::signum` — composite: -1, 0, or 1
//! - `I256::abs_u256` — composite: negate if negative, else identity
//! - `I256::try_into_i128` — narrow if high limbs are sign-extended; returns low u128

use super::emit::Program;
use super::u256::{FLAG_CHECKED, FLAG_WRAPPING};
use five_protocol::opcodes::{
    ADD_I256, ALLOC_LOCALS, BITWISE_AND, CMP_U256, DEALLOC_LOCALS, DIV_I256, DROP, GET_LOCAL,
    MUL_I256, SET_LOCAL, SUB_I256, SWAP,
};

/// Saturating-add I256 with the *right-hand side known at emit time*.
///
/// Bakes the saturation constant (`I256::MIN` if `b` is negative, else
/// `I256::MAX`) into the bytecode. This matches how Percolator typically uses
/// `saturating_add` — one side is loaded from account state and known
/// statically by the time the bytecode block is emitted.
///
/// Source: hello_slab/percolator/src/wide_math.rs:924-935
///
/// For the runtime-rhs variant (where both operands are pushed by the caller
/// without us knowing their signs), the bytecode needs to peek-or-recompute
/// `b`'s sign from inside the call. That's deferred until the DSL→bytecode
/// call convention lands; the const-rhs form is sufficient for both tests and
/// the next port pass.
pub fn program_saturating_add_i256_const_rhs_return_limb(
    a: [u64; 4],
    b: [u64; 4],
    limb_to_return: u8,
) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let saturate_to = if is_negative_i256_reference(b) {
        I256_MIN_RAW
    } else {
        I256_MAX_RAW
    };
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[ADD_I256, FLAG_CHECKED]);
    let no_sat = p.emit_jump_if_not_placeholder();
    for _ in 0..4 {
        p.raw(DROP);
    }
    p.push_u256(saturate_to);
    p.patch_jump_to_here(no_sat);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// Rust reference for `saturating_add_i256`. Used as test oracle.
pub fn saturating_add_i256_reference(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    match checked_add_i256_reference(a, b) {
        Some(r) => r,
        None => {
            if is_negative_i256_reference(b) {
                I256_MIN_RAW
            } else {
                I256_MAX_RAW
            }
        }
    }
}

/// Rust reference for `checked_add_i256`. Returns `None` if the signed sum
/// doesn't fit in `[u64; 4]` two's-complement.
pub fn checked_add_i256_reference(a: [u64; 4], b: [u64; 4]) -> Option<[u64; 4]> {
    let mut carry: u128 = 0;
    let mut r = [0u64; 4];
    for i in 0..4 {
        let s = (a[i] as u128) + (b[i] as u128) + carry;
        r[i] = s as u64;
        carry = s >> 64;
    }
    let a_neg = is_negative_i256_reference(a);
    let b_neg = is_negative_i256_reference(b);
    let r_neg = is_negative_i256_reference(r);
    // Signed overflow iff operands have the same sign and the result has the
    // opposite sign.
    if a_neg == b_neg && r_neg != a_neg {
        None
    } else {
        Some(r)
    }
}

// =============================================================================
// Primitive drivers — directly map to a single i256 opcode
// =============================================================================

/// `a + b` (i256, wrapping). Returns a specified result limb.
pub fn program_add_i256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[ADD_I256, FLAG_WRAPPING]);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a + b` (i256, checked). Returns overflow bool.
pub fn program_add_i256_checked_return_overflow(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[ADD_I256, FLAG_CHECKED]);
    for _ in 0..4 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// `a - b` (i256, wrapping). Returns a specified result limb.
pub fn program_sub_i256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[SUB_I256, FLAG_WRAPPING]);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a - b` (i256, checked). Returns overflow bool.
pub fn program_sub_i256_checked_return_overflow(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[SUB_I256, FLAG_CHECKED]);
    for _ in 0..4 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// `a * b` (i256, wrapping). Returns a specified result limb.
pub fn program_mul_i256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[MUL_I256, FLAG_WRAPPING]);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a * b` (i256, checked). Returns overflow bool.
pub fn program_mul_i256_checked_return_overflow(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[MUL_I256, FLAG_CHECKED]);
    for _ in 0..4 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// `a / b` (i256, truncates toward zero). Traps on `b == 0` or `i256::MIN / -1`
/// (without the checked flag — with it, pushes overflow = true instead).
pub fn program_div_i256_return_limb(a: [u64; 4], b: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    p.raw_bytes(&[DIV_I256, FLAG_WRAPPING]);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

// =============================================================================
// checked_neg via SUB_I256 (0 - a)
//
// Source: hello_slab/percolator/src/wide_math.rs:911-922
//
// Implementation: subtract `a` from zero with the checked flag. The VM's
// SUB_I256 already encodes the I256::MIN edge case as overflow=true (because
// negating MIN exceeds I256::MAX), so no extra branching is needed here.
// =============================================================================

/// `-a` as an i256, returning a specified result limb. Wrapping semantics
/// (matches Percolator's `checked_neg` body when the input is not MIN). For the
/// MIN-detecting variant, see `program_checked_neg_i256_return_overflow`.
pub fn program_checked_neg_i256_return_limb(a: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256([0; 4]).push_u256(a);
    p.raw_bytes(&[SUB_I256, FLAG_WRAPPING]);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `-a` overflow flag: 1 iff `a == I256::MIN` (the only value whose negation
/// doesn't fit in an `I256`). Pops the result limbs and keeps just the bool.
pub fn program_checked_neg_i256_return_overflow(a: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256([0; 4]).push_u256(a);
    p.raw_bytes(&[SUB_I256, FLAG_CHECKED]);
    for _ in 0..4 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// Rust-side reference for `checked_neg` — returns `None` exactly when
/// `a == I256::MIN`.
pub fn checked_neg_i256_reference(a: [u64; 4]) -> Option<[u64; 4]> {
    if a == I256_MIN_RAW {
        return None;
    }
    // Two's-complement negate: invert all bits, add 1.
    let inv = [!a[0], !a[1], !a[2], !a[3]];
    let (r0, c0) = inv[0].overflowing_add(1);
    let (r1, c1) = inv[1].overflowing_add(c0 as u64);
    let (r2, c2) = inv[2].overflowing_add(c1 as u64);
    let (r3, _)  = inv[3].overflowing_add(c2 as u64);
    Some([r0, r1, r2, r3])
}

/// `I256::MIN` as a raw `[u64; 4]` little-endian limb array.
pub const I256_MIN_RAW: [u64; 4] = [0, 0, 0, 0x8000_0000_0000_0000];

/// `I256::MAX` as a raw `[u64; 4]` little-endian limb array.
pub const I256_MAX_RAW: [u64; 4] = [u64::MAX, u64::MAX, u64::MAX, 0x7FFF_FFFF_FFFF_FFFF];

// =============================================================================
// checked_mul_i256 — Rust oracle (VM opcode already covers all edges)
//
// Source: hello_slab/percolator/src/wide_math.rs:835-865
//
// The mito `mul_i256_checked` opcode handler already covers every MIN edge
// correctly because `abs_i256(MIN) == (MIN, true)` and the subsequent unsigned-
// magnitude-overflow AND signed-range checks together reject every overflowing
// case (MIN × MIN unsigned-overflows u256; MIN × -1 lands on the MIN bit
// pattern with `negate == false` which trips the positive-range check). So no
// extra bytecode is needed — this reference is purely an oracle for the
// conformance tests in `tests/e2e_i256.rs`.
// =============================================================================

/// Rust reference for `checked_mul_i256`. Returns `None` on overflow.
///
/// Mirrors Percolator's spec: zero short-circuits, MIN × ONE is the only
/// MIN-involving product that fits (result = MIN), everything else involving
/// MIN overflows. For the non-MIN case, abs-multiply with unsigned-overflow
/// detection, then check the signed range: negative results may equal exactly
/// 2^255 (= MIN), positive results must be < 2^255.
pub fn checked_mul_i256_reference(a: [u64; 4], b: [u64; 4]) -> Option<[u64; 4]> {
    if a == [0; 4] || b == [0; 4] {
        return Some([0; 4]);
    }
    const ONE: [u64; 4] = [1, 0, 0, 0];
    if a == I256_MIN_RAW {
        if b == ONE {
            return Some(I256_MIN_RAW);
        }
        return None;
    }
    if b == I256_MIN_RAW {
        if a == ONE {
            return Some(I256_MIN_RAW);
        }
        return None;
    }
    // Neither is MIN, so abs+negate are safe (non-panicking) in the reference.
    let a_neg = is_negative_i256_reference(a);
    let b_neg = is_negative_i256_reference(b);
    let abs_a = if a_neg {
        checked_neg_i256_reference(a).expect("guarded: a != MIN")
    } else {
        a
    };
    let abs_b = if b_neg {
        checked_neg_i256_reference(b).expect("guarded: b != MIN")
    } else {
        b
    };
    let (magnitude, mag_overflow) = mul_u256_checked_reference(abs_a, abs_b);
    if mag_overflow {
        return None;
    }
    let mag_hi_bit = (magnitude[3] >> 63) & 1 == 1;
    let result_neg = a_neg != b_neg;
    if result_neg {
        // A negative result can equal exactly 2^255 (== MIN bit pattern).
        if magnitude == I256_MIN_RAW {
            return Some(I256_MIN_RAW);
        }
        if mag_hi_bit {
            return None;
        }
        // Safe negation: magnitude < 2^255 means -magnitude fits as i256.
        checked_neg_i256_reference(magnitude)
    } else {
        // Positive i256 range is < 2^255.
        if mag_hi_bit {
            return None;
        }
        Some(magnitude)
    }
}

/// `a * b` as u256, plus an overflow flag if the true 512-bit product has any
/// non-zero high limbs. Reuses [`mul_u256_to_u512`] for the 512-bit product and
/// checks the high half.
fn mul_u256_checked_reference(a: [u64; 4], b: [u64; 4]) -> ([u64; 4], bool) {
    let full = mul_u256_to_u512(a, b);
    let low = [full[0], full[1], full[2], full[3]];
    let high_nonzero = full[4] != 0 || full[5] != 0 || full[6] != 0 || full[7] != 0;
    (low, high_nonzero)
}

// =============================================================================
// Composite helpers (is_zero / is_negative / try_into_i128)
// =============================================================================

/// `is_zero(a)` as u64. Uses CMP_U256 against [0; 4]; returns 1 if equal.
pub fn program_is_zero_i256(a: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256([0; 4]);
    p.raw_bytes(&[CMP_U256, FLAG_WRAPPING]);
    // Caller: returned u64 == 1 iff a == 0.
    p.finish_with_halt()
}

/// Rust-side reference: `is_negative(a)` — the high bit of limb 3 is set.
/// Kept as a Rust fn because expressing a single-bit test in bytecode is
/// better done via CMP against `[0, 0, 0, 1<<63]` with `< vs >=` interpretation
/// (3-way CMP makes this slightly awkward but still doable).
pub fn is_negative_i256_reference(a: [u64; 4]) -> bool {
    (a[3] >> 63) & 1 == 1
}

/// Rust-side reference: `signum(a)` — returns -1, 0, or 1.
pub fn signum_i256_reference(a: [u64; 4]) -> i8 {
    if a == [0; 4] {
        0
    } else if is_negative_i256_reference(a) {
        -1
    } else {
        1
    }
}

// =============================================================================
// abs_u256 — bytecode form with conditional negation
//
// Source: hello_slab/percolator/src/wide_math.rs:815-828
//
// Wrapping semantics: `abs(I256::MIN)` returns `I256::MIN` (since `0 - MIN`
// wraps back to MIN). The Rust reference panics on MIN; callers who need the
// panic path must pre-check `a != I256::MIN` before invoking. This matches the
// broader "bytecode is wrapping, Rust refs are strict" convention in this
// module (see checked_neg).
//
// Strategy: stash the four limbs of `a` in locals, test sign via
// `limb[3] & (1 << 63)`, branch on the result. Negative path re-emits
// `0 - a` via SUB_I256; positive path re-emits `a` unchanged. Done with
// ALLOC_LOCALS / SET_LOCAL / GET_LOCAL so the branches don't need to juggle
// the value stack.
// =============================================================================

/// `|a|` as u256 limbs, returning a specified result limb. Wrapping on MIN.
pub fn program_abs_i256_return_limb(a: [u64; 4], limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 4);
    let mut p = Program::new();
    p.push_u256(a);
    stash_top_u256_to_locals(&mut p);

    // Sign test: GET_LOCAL 3; AND with (1<<63). Nonzero ⇒ negative.
    p.raw_bytes(&[GET_LOCAL, 3]);
    p.push_u64(1u64 << 63);
    p.raw(BITWISE_AND);
    let jmp_pos = p.emit_jump_if_not_placeholder();

    // Negative path: compute 0 - saved_a.
    p.push_u256([0; 4]);
    restore_u256_from_locals(&mut p);
    p.raw_bytes(&[SUB_I256, FLAG_WRAPPING]);
    let jmp_end = p.emit_jump_placeholder();

    // Positive path: re-push saved_a.
    p.patch_jump_to_here(jmp_pos);
    restore_u256_from_locals(&mut p);

    // Join: dealloc locals, narrow to the requested limb.
    p.patch_jump_to_here(jmp_end);
    p.raw(DEALLOC_LOCALS);
    drop_above_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// Rust-side reference: `abs_u256(a)`. Panics on i256::MIN (matching the
/// Rust source at wide_math.rs:815-828).
pub fn abs_u256_reference(a: [u64; 4]) -> [u64; 4] {
    if is_negative_i256_reference(a) {
        assert_ne!(
            a,
            [0, 0, 0, 0x8000_0000_0000_0000],
            "abs_u256 called on I256::MIN"
        );
        let not = [!a[0], !a[1], !a[2], !a[3]];
        let (r0, c0) = not[0].overflowing_add(1);
        let (r1, c1) = not[1].overflowing_add(c0 as u64);
        let (r2, c2) = not[2].overflowing_add(c1 as u64);
        let (r3, _) = not[3].overflowing_add(c2 as u64);
        [r0, r1, r2, r3]
    } else {
        a
    }
}

/// Rust-side reference: `try_into_i128(a)`. Returns None if the value doesn't
/// fit in i128. Bytecode form will be composite once we have branching ops
/// wired for production.
pub fn try_into_i128_reference(a: [u64; 4]) -> Option<i128> {
    // low u128 = (a[0] | a[1]<<64)
    // high u128 = (a[2] | a[3]<<64)
    // try_into_i128 iff high u128 == sign-extension of low u128
    let lo_u128 = (a[0] as u128) | ((a[1] as u128) << 64);
    let hi_u128 = (a[2] as u128) | ((a[3] as u128) << 64);
    let sign_ext: u128 = if (lo_u128 as i128) < 0 { u128::MAX } else { 0 };
    if hi_u128 == sign_ext {
        Some(lo_u128 as i128)
    } else {
        None
    }
}

// =============================================================================
// wide_signed_mul_div_floor — Rust reference (bytecode form needs U512 ops)
//
// Source: hello_slab/percolator/src/wide_math.rs:1498-1551
//
// Bytecode form is deferred because the Rust spec multiplies in U512 and
// floor-divides toward -∞ on a negative quotient — both require a remainder
// flag that the current MULDIV_U256 opcode doesn't surface. Once we add a
// `MULDIV_REM_U256` opcode (or wire MULDIV through the DSL with two outputs),
// the bytecode form becomes a thin wrapper around MULDIV + sign-adjust.
//
// Until then this Rust reference is the conformance oracle: the eventual
// bytecode emitter must produce identical results across the test vectors
// from `hello_slab/percolator/tests/`.
// =============================================================================

/// Wide-precision signed mul-div with floor rounding toward -∞.
/// `floor((abs_basis * k_diff) / denominator)` over a U512 numerator.
///
/// Panics on `denominator == 0` (matches Percolator) and on `k_diff == I256::MIN`
/// (also matches Percolator).
///
/// Caller invariants (Percolator spec §1.5 item 11):
///   * `abs_basis` is a non-negative magnitude
///   * `denominator > 0`
///   * `k_diff` is a signed delta whose magnitude satisfies
///     `|abs_basis * |k_diff|| ≤ 2^512`
pub fn wide_signed_mul_div_floor_reference(
    abs_basis: [u64; 4],
    k_diff: [u64; 4],
    denominator: [u64; 4],
) -> [u64; 4] {
    assert!(denominator != [0; 4], "wide_signed_mul_div_floor: zero denominator");
    if k_diff == [0; 4] || abs_basis == [0; 4] {
        return [0; 4];
    }
    let negative = is_negative_i256_reference(k_diff);
    if negative {
        assert!(k_diff != I256_MIN_RAW, "wide_signed_mul_div_floor: k_diff == I256::MIN");
    }
    let abs_k = if negative {
        match checked_neg_i256_reference(k_diff) {
            Some(v) => v,
            None => unreachable!("guarded by I256::MIN assert above"),
        }
    } else {
        k_diff
    };

    // Wide product abs_basis * abs_k as U512 = [u64; 8] little-endian.
    let product_512 = mul_u256_to_u512(abs_basis, abs_k);
    let (q_512, r_512) = div_rem_u512_by_u256(product_512, denominator);
    // Quotient must fit in U256 (else result doesn't fit in I256).
    let q: [u64; 4] = [q_512[0], q_512[1], q_512[2], q_512[3]];
    assert!(
        q_512[4] == 0 && q_512[5] == 0 && q_512[6] == 0 && q_512[7] == 0,
        "wide_signed_mul_div_floor: quotient overflows U256"
    );

    if !negative {
        return q;
    }
    let r_is_zero = r_512 == [0; 4];
    let q_floor = if r_is_zero {
        q
    } else {
        // q + 1 (cannot overflow because |result| ≤ 2^255 by spec invariant)
        let mut out = q;
        let mut carry: u128 = 1;
        for i in 0..4 {
            let s = (out[i] as u128) + carry;
            out[i] = s as u64;
            carry = s >> 64;
        }
        out
    };
    if q_floor == [0; 4] {
        return [0; 4];
    }
    // Negate to apply the original sign of k_diff. Cannot fail because spec
    // bounds |result| ≤ I256::MAX.
    checked_neg_i256_reference(q_floor).expect("wide_signed_mul_div_floor result out of I256 range")
}

/// 256x256 → 512-bit unsigned multiply. Returns 8 u64 limbs little-endian.
fn mul_u256_to_u512(a: [u64; 4], b: [u64; 4]) -> [u64; 8] {
    let mut out = [0u64; 8];
    for i in 0..4 {
        let mut carry: u128 = 0;
        for j in 0..4 {
            let s = (out[i + j] as u128) + (a[i] as u128) * (b[j] as u128) + carry;
            out[i + j] = s as u64;
            carry = s >> 64;
        }
        out[i + 4] = (out[i + 4] as u128 + carry) as u64;
    }
    out
}

/// 512 / 256 = (q_512, r_256) using restoring binary long division.
/// Slow but correct — used only as a Rust-side oracle for the bytecode tests.
fn div_rem_u512_by_u256(numerator: [u64; 8], denominator: [u64; 4]) -> ([u64; 8], [u64; 4]) {
    assert!(denominator != [0; 4], "div_rem_u512_by_u256: zero denominator");
    let mut quotient = [0u64; 8];
    let mut remainder = [0u64; 4];
    for bit in (0..512).rev() {
        // remainder = remainder << 1
        let mut carry: u64 = 0;
        for limb in &mut remainder {
            let new_carry = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = new_carry;
        }
        // bring down bit
        let nb = (numerator[bit / 64] >> (bit % 64)) & 1;
        remainder[0] |= nb;
        if cmp_u256_ref(remainder, denominator) != core::cmp::Ordering::Less {
            // remainder -= denominator
            let mut borrow: i128 = 0;
            for i in 0..4 {
                let d = (remainder[i] as i128) - (denominator[i] as i128) - borrow;
                if d < 0 {
                    remainder[i] = (d + (1i128 << 64)) as u64;
                    borrow = 1;
                } else {
                    remainder[i] = d as u64;
                    borrow = 0;
                }
            }
            quotient[bit / 64] |= 1u64 << (bit % 64);
        }
    }
    (quotient, remainder)
}

fn cmp_u256_ref(a: [u64; 4], b: [u64; 4]) -> core::cmp::Ordering {
    for i in (0..4).rev() {
        match a[i].cmp(&b[i]) {
            core::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    core::cmp::Ordering::Equal
}

// =============================================================================
// Remaining deferred ops
// =============================================================================
//
// - `saturating_add_i256` runtime-rhs form
//     Source: hello_slab/percolator/src/wide_math.rs:924-935
//     The const-rhs form ships above (bakes the saturation constant at emit
//     time). Runtime-rhs needs to peek `b`'s sign-bit without consuming it
//     from the stack — doable with `stash_top_u256_to_locals` + test +
//     restore, similar to `abs_i256`. Deferred because current callers in
//     Percolator's risk engine pass one operand as a constant.
//
// - `wide_signed_mul_div_floor(abs_basis, k_diff, denominator) -> I256`
//     Source: wide_math.rs:1498-1551
//     Needs a `MULDIV_REM_U256` opcode (or two-output MULDIV) so the bytecode
//     can see the remainder and apply floor-toward-(-∞) rounding. Rust
//     reference above is the conformance oracle.

// =============================================================================
// Shared helper
// =============================================================================

fn drop_above_limb(p: &mut Program, target_limb: u8) {
    for _ in 0..(3 - target_limb) {
        p.raw(DROP);
    }
    for _ in 0..target_limb {
        p.raw(SWAP).raw(DROP);
    }
}

/// Pop the four u256 limbs on top of the stack into locals 0-3, allocating
/// them first. Layout: the bottom limb (a0) ends up in local 0, top (a3) in
/// local 3 — preserving little-endian semantics on later restore.
fn stash_top_u256_to_locals(p: &mut Program) {
    p.raw_bytes(&[ALLOC_LOCALS, 4]);
    // Stack top is a3; SET_LOCAL pops from top.
    for i in (0..4u8).rev() {
        p.raw_bytes(&[SET_LOCAL, i]);
    }
}

/// Re-push locals 0-3 onto the stack in little-endian order (a0 deep, a3 top).
/// Pairs with [`stash_top_u256_to_locals`]. Does not dealloc.
fn restore_u256_from_locals(p: &mut Program) {
    for i in 0..4u8 {
        p.raw_bytes(&[GET_LOCAL, i]);
    }
}
