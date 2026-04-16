//! i128 math sequences — counterparts to Percolator's `i128.rs` (the BPF-safe
//! 128-bit wrapper).
//!
//! Uses the `ADD_I128`, `SUB_I128`, `MUL_I128`, `DIV_I128` opcodes (0xC6-0xC9)
//! from PR #84. Each i128 is represented as two u64 limbs (lo, hi) on the stack,
//! the same convention `five_vm_mito::handlers::multiprecision::pop_two_i128`
//! expects.
//!
//! # Percolator mapping
//!
//! `hello_slab/percolator/src/i128.rs` provides `checked_add`, `checked_sub`,
//! `checked_mul`, `checked_div` on a `#[repr(transparent)] I128(i128)` wrapper.
//! Each of these maps 1:1 to an `I128_*` opcode with `FLAG_CHECKED`.
//!
//! Special conversions (`fee_debt_u128_checked`,
//! `floor_div_signed_conservative_i128`) that need post-opcode conditional
//! logic are stubbed with Rust-reference implementations below and documented
//! for follow-up bytecode ports once branching helpers are in place.

use super::emit::Program;
use super::u256::{FLAG_CHECKED, FLAG_WRAPPING};
use five_protocol::opcodes::{ADD_I128, DIV_I128, DROP, MUL_I128, SUB_I128, SWAP};

/// Split a native i128 into two u64 limbs (lo, hi) using its two's-complement bits.
pub fn i128_to_limbs(v: i128) -> [u64; 2] {
    let u = v as u128;
    [u as u64, (u >> 64) as u64]
}

/// Combine two u64 limbs back into a native i128 (raw bit reinterpretation).
pub fn i128_from_limbs(limbs: [u64; 2]) -> i128 {
    ((limbs[0] as u128) | ((limbs[1] as u128) << 64)) as i128
}

/// Build a test program that computes `a + b` (i128, wrapping) and returns
/// the specified result limb.
pub fn program_add_i128_return_limb(a: i128, b: i128, limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 2);
    let mut p = Program::new();
    push_i128(&mut p, a);
    push_i128(&mut p, b);
    p.raw_bytes(&[ADD_I128, FLAG_WRAPPING]);
    drop_above_i128_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a + b` (i128, checked). Returns the overflow bool.
pub fn program_add_i128_checked_return_overflow(a: i128, b: i128) -> Vec<u8> {
    let mut p = Program::new();
    push_i128(&mut p, a);
    push_i128(&mut p, b);
    p.raw_bytes(&[ADD_I128, FLAG_CHECKED]);
    // Stack after: [r_lo, r_hi, overflow_bool]. Drop the 2 result limbs, keep bool.
    for _ in 0..2 {
        p.raw(SWAP).raw(DROP);
    }
    p.finish_with_halt()
}

/// `a - b` (i128, wrapping).
pub fn program_sub_i128_return_limb(a: i128, b: i128, limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 2);
    let mut p = Program::new();
    push_i128(&mut p, a);
    push_i128(&mut p, b);
    p.raw_bytes(&[SUB_I128, FLAG_WRAPPING]);
    drop_above_i128_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a * b` (i128, wrapping).
pub fn program_mul_i128_return_limb(a: i128, b: i128, limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 2);
    let mut p = Program::new();
    push_i128(&mut p, a);
    push_i128(&mut p, b);
    p.raw_bytes(&[MUL_I128, FLAG_WRAPPING]);
    drop_above_i128_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

/// `a / b` (i128, truncates toward zero). Traps on `b == 0` or `i128::MIN / -1`.
pub fn program_div_i128_return_limb(a: i128, b: i128, limb_to_return: u8) -> Vec<u8> {
    assert!(limb_to_return < 2);
    let mut p = Program::new();
    push_i128(&mut p, a);
    push_i128(&mut p, b);
    p.raw_bytes(&[DIV_I128, FLAG_WRAPPING]);
    drop_above_i128_limb(&mut p, limb_to_return);
    p.finish_with_halt()
}

// =============================================================================
// Rust references for non-trivial percolator helpers
// =============================================================================

/// Rust-side reference for `wide_math::fee_debt_u128_checked(fee_credits: i128) -> u128`.
///
/// From `wide_math.rs:1481-1497`:
/// `if fee_credits >= 0 { 0 } else { -fee_credits as u128 }`
/// with a special-case for `i128::MIN` to avoid the `-MIN` overflow.
pub fn fee_debt_u128_checked_reference(fee_credits: i128) -> u128 {
    if fee_credits >= 0 {
        0
    } else if fee_credits == i128::MIN {
        // -i128::MIN = 2^127 which fits in u128
        1u128 << 127
    } else {
        (-fee_credits) as u128
    }
}

/// Rust-side reference for `wide_math::floor_div_signed_conservative_i128(n, d) -> i128`.
///
/// Floor division for signed numerator over unsigned denominator. Spec
/// wide_math.rs:1391-1415: `if n >= 0 { n / d } else { -((-n + d - 1) / d) }`
/// with careful handling for i128::MIN.
pub fn floor_div_signed_conservative_i128_reference(n: i128, d: u128) -> i128 {
    assert!(d > 0, "denominator must be positive");
    if n >= 0 {
        (n as u128 / d) as i128
    } else if n == i128::MIN {
        // -MIN = 2^127, adjusted by (d-1) for ceiling, then negated.
        let pos_mag: u128 = 1u128 << 127;
        let adjusted = pos_mag + (d - 1);
        -((adjusted / d) as i128)
    } else {
        let pos_mag = (-n) as u128;
        let adjusted = pos_mag + (d - 1);
        -((adjusted / d) as i128)
    }
}

// =============================================================================
// Stack marshalling helpers
// =============================================================================

fn push_i128(p: &mut Program, v: i128) {
    let limbs = i128_to_limbs(v);
    p.push_u64(limbs[0]);
    p.push_u64(limbs[1]);
}

/// After an i128-producing op, two result limbs are on the stack with r_hi on
/// top. This helper drops the limb above the target and any below it, leaving
/// `target_limb` (0 = lo, 1 = hi) on top so `HALT` returns it.
fn drop_above_i128_limb(p: &mut Program, target_limb: u8) {
    for _ in 0..(1 - target_limb) {
        p.raw(DROP);
    }
    for _ in 0..target_limb {
        p.raw(SWAP).raw(DROP);
    }
}
