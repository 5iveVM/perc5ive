//! End-to-end tests for the i128 bytecode sequences.
//!
//! Each op maps 1:1 to an `I128_*` opcode (0xC6-0xC9). Tests cover the basic
//! arithmetic plus the Rust references for `fee_debt_u128_checked` and
//! `floor_div_signed_conservative_i128` which Percolator uses directly.

use five_vm_mito::{MitoVM, Value};
use perc5ive::bytecode::i128::{
    fee_debt_u128_checked_reference, floor_div_signed_conservative_i128_reference,
    i128_from_limbs, i128_to_limbs, program_add_i128_checked_return_overflow,
    program_add_i128_return_limb, program_div_i128_return_limb, program_mul_i128_return_limb,
    program_sub_i128_return_limb,
};

fn run_u64(bytecode: Vec<u8>) -> u64 {
    let result = MitoVM::execute_direct(&bytecode, &[], &[])
        .expect("VM execution should succeed");
    match result {
        Some(Value::U64(v)) => v,
        Some(Value::Bool(b)) => b as u64,
        Some(other) => panic!("expected U64/Bool, got {:?}", other),
        None => panic!("expected a return value"),
    }
}

fn run_i128(program: impl Fn(u8) -> Vec<u8>) -> i128 {
    let lo = run_u64(program(0));
    let hi = run_u64(program(1));
    i128_from_limbs([lo, hi])
}

// =============================================================================
// ADD_I128
// =============================================================================

#[test]
fn add_i128_small() {
    assert_eq!(run_i128(|l| program_add_i128_return_limb(5, 3, l)), 8);
    assert_eq!(run_i128(|l| program_add_i128_return_limb(-5, 3, l)), -2);
    assert_eq!(run_i128(|l| program_add_i128_return_limb(-5, -3, l)), -8);
}

#[test]
fn add_i128_wraps_at_max() {
    // i128::MAX + 1 = i128::MIN (wrapping)
    assert_eq!(run_i128(|l| program_add_i128_return_limb(i128::MAX, 1, l)), i128::MIN);
}

#[test]
fn add_i128_checked_reports_overflow() {
    assert_eq!(run_u64(program_add_i128_checked_return_overflow(i128::MAX, 1)), 1);
    assert_eq!(run_u64(program_add_i128_checked_return_overflow(0, 0)), 0);
    assert_eq!(run_u64(program_add_i128_checked_return_overflow(i128::MIN, -1)), 1);
}

// =============================================================================
// SUB_I128
// =============================================================================

#[test]
fn sub_i128_small() {
    assert_eq!(run_i128(|l| program_sub_i128_return_limb(10, 3, l)), 7);
    assert_eq!(run_i128(|l| program_sub_i128_return_limb(-5, -3, l)), -2);
    assert_eq!(run_i128(|l| program_sub_i128_return_limb(3, 10, l)), -7);
}

// =============================================================================
// MUL_I128
// =============================================================================

#[test]
fn mul_i128_small() {
    assert_eq!(run_i128(|l| program_mul_i128_return_limb(6, 7, l)), 42);
    assert_eq!(run_i128(|l| program_mul_i128_return_limb(-6, 7, l)), -42);
    assert_eq!(run_i128(|l| program_mul_i128_return_limb(-6, -7, l)), 42);
}

// =============================================================================
// DIV_I128
// =============================================================================

#[test]
fn div_i128_truncates_toward_zero() {
    assert_eq!(run_i128(|l| program_div_i128_return_limb(10, 3, l)), 3);
    assert_eq!(run_i128(|l| program_div_i128_return_limb(-10, 3, l)), -3);
    assert_eq!(run_i128(|l| program_div_i128_return_limb(10, -3, l)), -3);
    assert_eq!(run_i128(|l| program_div_i128_return_limb(-10, -3, l)), 3);
}

// =============================================================================
// Reference functions (Rust-side, for use from the eventual DSL port)
// =============================================================================

#[test]
fn fee_debt_reference_zero_when_nonnegative() {
    assert_eq!(fee_debt_u128_checked_reference(0), 0);
    assert_eq!(fee_debt_u128_checked_reference(100), 0);
    assert_eq!(fee_debt_u128_checked_reference(i128::MAX), 0);
}

#[test]
fn fee_debt_reference_positive_magnitude_for_negative() {
    assert_eq!(fee_debt_u128_checked_reference(-1), 1);
    assert_eq!(fee_debt_u128_checked_reference(-42), 42);
    // i128::MIN needs special handling: -MIN wraps, should be 2^127.
    assert_eq!(fee_debt_u128_checked_reference(i128::MIN), 1u128 << 127);
}

#[test]
fn floor_div_signed_conservative_i128_matches_spec() {
    // Positive numerator: integer division.
    assert_eq!(floor_div_signed_conservative_i128_reference(10, 3), 3);
    // Negative numerator: floor (rounds down toward -infinity).
    assert_eq!(floor_div_signed_conservative_i128_reference(-10, 3), -4);
    // Exact division works both ways.
    assert_eq!(floor_div_signed_conservative_i128_reference(9, 3), 3);
    assert_eq!(floor_div_signed_conservative_i128_reference(-9, 3), -3);
    // Zero numerator.
    assert_eq!(floor_div_signed_conservative_i128_reference(0, 5), 0);
}

#[test]
fn i128_limbs_roundtrip() {
    for v in [0i128, 1, -1, 42, -42, i128::MAX, i128::MIN, 1i128 << 100] {
        let limbs = i128_to_limbs(v);
        assert_eq!(i128_from_limbs(limbs), v);
    }
}
