//! End-to-end tests for the i256 bytecode sequences.
//!
//! Signed 256-bit arithmetic is two's-complement on `[u64; 4]`. These tests
//! compare the bytecode output against the reference functions in
//! `perc5ive::bytecode::i256`, which in turn shadow Percolator's Rust source.

use five_vm_mito::{MitoVM, Value};
use perc5ive::bytecode::i256::{
    abs_u256_reference, checked_add_i256_reference, checked_mul_i256_reference,
    checked_neg_i256_reference, is_negative_i256_reference, program_abs_i256_return_limb,
    program_add_i256_checked_return_overflow, program_add_i256_return_limb,
    program_checked_neg_i256_return_limb, program_checked_neg_i256_return_overflow,
    program_div_i256_return_limb, program_is_zero_i256,
    program_mul_i256_checked_return_overflow, program_mul_i256_return_limb,
    program_saturating_add_i256_const_rhs_return_limb, program_sub_i256_checked_return_overflow,
    program_sub_i256_return_limb, saturating_add_i256_reference, signum_i256_reference,
    try_into_i128_reference, wide_signed_mul_div_floor_reference, I256_MAX_RAW, I256_MIN_RAW,
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

/// Negate an i256 via !x + 1 (two's complement).
fn negate(v: [u64; 4]) -> [u64; 4] {
    let not = [!v[0], !v[1], !v[2], !v[3]];
    let (r0, c0) = not[0].overflowing_add(1);
    let (r1, c1) = not[1].overflowing_add(c0 as u64);
    let (r2, c2) = not[2].overflowing_add(c1 as u64);
    let (r3, _) = not[3].overflowing_add(c2 as u64);
    [r0, r1, r2, r3]
}

fn i256_min() -> [u64; 4] {
    [0, 0, 0, 0x8000_0000_0000_0000]
}

fn i256_max() -> [u64; 4] {
    [u64::MAX, u64::MAX, u64::MAX, 0x7FFF_FFFF_FFFF_FFFF]
}

// =============================================================================
// ADD_I256
// =============================================================================

#[test]
fn add_i256_wraps_like_u256_at_bit_level() {
    // -1 + 1 = 0
    let r0 = run_u64(program_add_i256_return_limb([u64::MAX; 4], [1, 0, 0, 0], 0));
    let r1 = run_u64(program_add_i256_return_limb([u64::MAX; 4], [1, 0, 0, 0], 1));
    let r2 = run_u64(program_add_i256_return_limb([u64::MAX; 4], [1, 0, 0, 0], 2));
    let r3 = run_u64(program_add_i256_return_limb([u64::MAX; 4], [1, 0, 0, 0], 3));
    assert_eq!([r0, r1, r2, r3], [0; 4]);
}

#[test]
fn add_i256_checked_detects_positive_overflow() {
    // MAX + 1 overflows.
    assert_eq!(run_u64(program_add_i256_checked_return_overflow(i256_max(), [1, 0, 0, 0])), 1);
}

#[test]
fn add_i256_checked_detects_negative_overflow() {
    // MIN + (-1) overflows.
    assert_eq!(run_u64(program_add_i256_checked_return_overflow(i256_min(), [u64::MAX; 4])), 1);
}

#[test]
fn add_i256_checked_no_false_positive() {
    // Both sides small positive: no overflow.
    assert_eq!(run_u64(program_add_i256_checked_return_overflow([5, 0, 0, 0], [7, 0, 0, 0])), 0);
}

// =============================================================================
// SUB_I256
// =============================================================================

#[test]
fn sub_i256_basic() {
    // 5 - 3 = 2
    let r0 = run_u64(program_sub_i256_return_limb([5, 0, 0, 0], [3, 0, 0, 0], 0));
    assert_eq!(r0, 2);
    // 0 - 1 = -1 (limb layout all-ones)
    let r3 = run_u64(program_sub_i256_return_limb([0; 4], [1, 0, 0, 0], 3));
    assert_eq!(r3, u64::MAX);
}

#[test]
fn sub_i256_checked_overflow() {
    // MIN - 1 overflows.
    assert_eq!(run_u64(program_sub_i256_checked_return_overflow(i256_min(), [1, 0, 0, 0])), 1);
}

// =============================================================================
// MUL_I256
// =============================================================================

#[test]
fn mul_i256_sign_handling() {
    // -2 * -3 = 6
    let neg_two = negate([2, 0, 0, 0]);
    let neg_three = negate([3, 0, 0, 0]);
    let r0 = run_u64(program_mul_i256_return_limb(neg_two, neg_three, 0));
    assert_eq!(r0, 6);
    // 2 * -3 = -6
    let r0 = run_u64(program_mul_i256_return_limb([2, 0, 0, 0], neg_three, 0));
    assert_eq!(r0, negate([6, 0, 0, 0])[0]);
}

#[test]
fn mul_i256_checked_detects_overflow() {
    // 2^128 * 2^128 = 2^256, overflows i256 (positive-positive out of range).
    let big = [0u64, 0, 1, 0];
    assert_eq!(run_u64(program_mul_i256_checked_return_overflow(big, big)), 1);
}

// =============================================================================
// DIV_I256
// =============================================================================

#[test]
fn div_i256_truncates_toward_zero() {
    // 10 / 3 = 3
    assert_eq!(run_u64(program_div_i256_return_limb([10, 0, 0, 0], [3, 0, 0, 0], 0)), 3);
    // -10 / 3 = -3 (truncate toward zero, matching native i128 semantics)
    let r0 = run_u64(program_div_i256_return_limb(negate([10, 0, 0, 0]), [3, 0, 0, 0], 0));
    assert_eq!(r0, negate([3, 0, 0, 0])[0]);
}

// =============================================================================
// Composite helpers
// =============================================================================

#[test]
fn is_zero_i256() {
    // program_is_zero returns 0/1/2 = lt/eq/gt from CMP against [0;4].
    // Zero: cmp(0, 0) == 1.
    assert_eq!(run_u64(program_is_zero_i256([0; 4])), 1);
    // Positive: cmp(5, 0) == 2.
    assert_eq!(run_u64(program_is_zero_i256([5, 0, 0, 0])), 2);
}

// =============================================================================
// Rust references (compile + semantic smoke tests)
// =============================================================================

#[test]
fn is_negative_reference_matches_high_bit() {
    assert!(!is_negative_i256_reference([0; 4]));
    assert!(!is_negative_i256_reference([1, 2, 3, 4]));
    assert!(is_negative_i256_reference(negate([1, 0, 0, 0])));
    assert!(is_negative_i256_reference(i256_min()));
}

#[test]
fn signum_reference_returns_tri_state() {
    assert_eq!(signum_i256_reference([0; 4]), 0);
    assert_eq!(signum_i256_reference([1, 0, 0, 0]), 1);
    assert_eq!(signum_i256_reference(negate([1, 0, 0, 0])), -1);
}

#[test]
fn abs_u256_reference_flips_negatives() {
    assert_eq!(abs_u256_reference([0; 4]), [0; 4]);
    assert_eq!(abs_u256_reference([5, 0, 0, 0]), [5, 0, 0, 0]);
    assert_eq!(abs_u256_reference(negate([5, 0, 0, 0])), [5, 0, 0, 0]);
}

#[test]
#[should_panic(expected = "abs_u256 called on I256::MIN")]
fn abs_u256_reference_panics_on_min() {
    let _ = abs_u256_reference(i256_min());
}

#[test]
fn try_into_i128_reference_roundtrip() {
    for v in [0i128, 1, -1, 42, -42, i128::MAX, i128::MIN] {
        let u = v as u128;
        let sign_ext = if v < 0 { u128::MAX } else { 0 };
        let limbs = [u as u64, (u >> 64) as u64, sign_ext as u64, (sign_ext >> 64) as u64];
        assert_eq!(try_into_i128_reference(limbs), Some(v));
    }
    // Out-of-range: high limbs are not sign-extension.
    assert_eq!(try_into_i128_reference([1, 2, 3, 4]), None);
}

// =============================================================================
// checked_neg_i256 — bytecode + reference
// =============================================================================

fn run_neg_limb(a: [u64; 4]) -> [u64; 4] {
    let mut out = [0u64; 4];
    for i in 0..4 {
        out[i] = run_u64(program_checked_neg_i256_return_limb(a, i as u8));
    }
    out
}

#[test]
fn checked_neg_i256_negates_positive() {
    assert_eq!(run_neg_limb([5, 0, 0, 0]), negate([5, 0, 0, 0]));
    assert_eq!(run_neg_limb([42, 0, 0, 0]), negate([42, 0, 0, 0]));
}

#[test]
fn checked_neg_i256_negates_negative_back_to_positive() {
    assert_eq!(run_neg_limb(negate([7, 0, 0, 0])), [7, 0, 0, 0]);
}

#[test]
fn checked_neg_i256_zero_is_zero() {
    assert_eq!(run_neg_limb([0; 4]), [0; 4]);
}

#[test]
fn checked_neg_i256_overflow_only_on_min() {
    // Non-MIN: overflow flag = 0
    assert_eq!(run_u64(program_checked_neg_i256_return_overflow([5, 0, 0, 0])), 0);
    assert_eq!(run_u64(program_checked_neg_i256_return_overflow([0; 4])), 0);
    assert_eq!(run_u64(program_checked_neg_i256_return_overflow(I256_MAX_RAW)), 0);
    // MIN: overflow flag = 1
    assert_eq!(run_u64(program_checked_neg_i256_return_overflow(I256_MIN_RAW)), 1);
}

#[test]
fn checked_neg_reference_matches_spec() {
    assert_eq!(checked_neg_i256_reference([5, 0, 0, 0]), Some(negate([5, 0, 0, 0])));
    assert_eq!(checked_neg_i256_reference([0; 4]), Some([0; 4]));
    assert_eq!(checked_neg_i256_reference(I256_MIN_RAW), None);
    assert_eq!(checked_neg_i256_reference(I256_MAX_RAW), Some(negate(I256_MAX_RAW)));
}

// =============================================================================
// saturating_add_i256 (const-rhs bytecode + reference)
// =============================================================================

fn run_sat_add_limb(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut out = [0u64; 4];
    for i in 0..4 {
        out[i] = run_u64(program_saturating_add_i256_const_rhs_return_limb(a, b, i as u8));
    }
    out
}

#[test]
fn saturating_add_i256_normal_path() {
    assert_eq!(run_sat_add_limb([5, 0, 0, 0], [3, 0, 0, 0]), [8, 0, 0, 0]);
    assert_eq!(run_sat_add_limb([5, 0, 0, 0], negate([3, 0, 0, 0])), [2, 0, 0, 0]);
}

#[test]
fn saturating_add_i256_overflows_to_max_when_rhs_positive() {
    // I256::MAX + 1 → should saturate to I256::MAX
    assert_eq!(run_sat_add_limb(I256_MAX_RAW, [1, 0, 0, 0]), I256_MAX_RAW);
}

#[test]
fn saturating_add_i256_overflows_to_min_when_rhs_negative() {
    // I256::MIN + (-1) → should saturate to I256::MIN
    let neg_one = negate([1, 0, 0, 0]);
    assert_eq!(run_sat_add_limb(I256_MIN_RAW, neg_one), I256_MIN_RAW);
}

#[test]
fn saturating_add_reference_matches() {
    assert_eq!(saturating_add_i256_reference([5, 0, 0, 0], [3, 0, 0, 0]), [8, 0, 0, 0]);
    assert_eq!(saturating_add_i256_reference(I256_MAX_RAW, [1, 0, 0, 0]), I256_MAX_RAW);
    let neg_one = negate([1, 0, 0, 0]);
    assert_eq!(saturating_add_i256_reference(I256_MIN_RAW, neg_one), I256_MIN_RAW);
}

#[test]
fn checked_add_reference_detects_overflow() {
    assert_eq!(checked_add_i256_reference([5, 0, 0, 0], [3, 0, 0, 0]), Some([8, 0, 0, 0]));
    assert_eq!(checked_add_i256_reference(I256_MAX_RAW, [1, 0, 0, 0]), None);
    assert_eq!(checked_add_i256_reference(I256_MIN_RAW, negate([1, 0, 0, 0])), None);
    // Mixed signs never overflow
    assert!(checked_add_i256_reference(I256_MAX_RAW, negate([1, 0, 0, 0])).is_some());
}

// =============================================================================
// wide_signed_mul_div_floor (Rust reference only — bytecode form deferred)
// =============================================================================

#[test]
fn wide_signed_mul_div_floor_zero_inputs() {
    assert_eq!(
        wide_signed_mul_div_floor_reference([0; 4], [42, 0, 0, 0], [7, 0, 0, 0]),
        [0; 4]
    );
    assert_eq!(
        wide_signed_mul_div_floor_reference([42, 0, 0, 0], [0; 4], [7, 0, 0, 0]),
        [0; 4]
    );
}

#[test]
fn wide_signed_mul_div_floor_positive_truncates() {
    // (10 * 7) / 3 = 23 (truncate)
    assert_eq!(
        wide_signed_mul_div_floor_reference([10, 0, 0, 0], [7, 0, 0, 0], [3, 0, 0, 0]),
        [23, 0, 0, 0]
    );
}

#[test]
fn wide_signed_mul_div_floor_exact_division() {
    // (10 * 6) / 3 = 20, no remainder ⇒ floor == truncate
    assert_eq!(
        wide_signed_mul_div_floor_reference([10, 0, 0, 0], [6, 0, 0, 0], [3, 0, 0, 0]),
        [20, 0, 0, 0]
    );
}

#[test]
fn wide_signed_mul_div_floor_negative_floor_rounds_down() {
    // (10 * -7) / 3 = -23.33 → floor = -24
    let neg7 = negate([7, 0, 0, 0]);
    assert_eq!(
        wide_signed_mul_div_floor_reference([10, 0, 0, 0], neg7, [3, 0, 0, 0]),
        negate([24, 0, 0, 0])
    );
}

#[test]
fn wide_signed_mul_div_floor_negative_exact_no_extra() {
    // (10 * -6) / 3 = -20, exact ⇒ no floor adjustment
    let neg6 = negate([6, 0, 0, 0]);
    assert_eq!(
        wide_signed_mul_div_floor_reference([10, 0, 0, 0], neg6, [3, 0, 0, 0]),
        negate([20, 0, 0, 0])
    );
}

#[test]
fn wide_signed_mul_div_floor_uses_512_bit_intermediate() {
    // abs_basis = 2^128, k_diff = 2^128 (positive), denom = 2^129 → 2^127
    let basis = [0u64, 0, 1, 0]; // 2^128
    let k = [0u64, 0, 1, 0]; // 2^128
    let denom = [0u64, 0, 2, 0]; // 2^129
    assert_eq!(
        wide_signed_mul_div_floor_reference(basis, k, denom),
        [0, 1u64 << 63, 0, 0] // 2^127
    );
}

#[test]
#[should_panic(expected = "zero denominator")]
fn wide_signed_mul_div_floor_panics_on_zero_denom() {
    wide_signed_mul_div_floor_reference([10, 0, 0, 0], [5, 0, 0, 0], [0; 4]);
}

// =============================================================================
// abs_i256 — conditional negate via locals + JUMP_IF_NOT
// =============================================================================

fn run_abs(a: [u64; 4], limb: u8) -> u64 {
    run_u64(program_abs_i256_return_limb(a, limb))
}

#[test]
fn abs_positive_is_identity_all_limbs() {
    let a: [u64; 4] = [0x1111, 0x2222, 0x3333, 0x0000_4444];
    for limb in 0..4u8 {
        assert_eq!(run_abs(a, limb), a[limb as usize]);
    }
}

#[test]
fn abs_zero_all_limbs() {
    for limb in 0..4u8 {
        assert_eq!(run_abs([0; 4], limb), 0);
    }
}

#[test]
fn abs_negative_small_matches_negate() {
    // -5 in i256 = negate([5,0,0,0])
    let minus_five = negate([5, 0, 0, 0]);
    // abs(-5) == 5
    assert_eq!(run_abs(minus_five, 0), 5);
    assert_eq!(run_abs(minus_five, 1), 0);
    assert_eq!(run_abs(minus_five, 2), 0);
    assert_eq!(run_abs(minus_five, 3), 0);
}

#[test]
fn abs_negative_large_crosses_limb_boundary() {
    // -(2^128 + 7) has nonzero limbs at 0 and 2 after negation.
    let v: [u64; 4] = [7, 0, 1, 0];
    let minus_v = negate(v);
    let expected = abs_u256_reference(minus_v);
    for limb in 0..4u8 {
        assert_eq!(run_abs(minus_v, limb), expected[limb as usize]);
    }
    assert_eq!(expected, v);
}

#[test]
fn abs_i256_max_is_identity() {
    let expected = abs_u256_reference(I256_MAX_RAW);
    for limb in 0..4u8 {
        assert_eq!(run_abs(I256_MAX_RAW, limb), expected[limb as usize]);
    }
}

#[test]
fn abs_i256_min_wraps_to_min_in_bytecode() {
    // The Rust reference panics on MIN; the bytecode chooses wrapping instead,
    // producing MIN again (because `0 - MIN` overflows in two's complement).
    // This test pins that semantics — callers with strict requirements must
    // pre-check `a != MIN`.
    let min_bytecode_result: [u64; 4] = [
        run_abs(I256_MIN_RAW, 0),
        run_abs(I256_MIN_RAW, 1),
        run_abs(I256_MIN_RAW, 2),
        run_abs(I256_MIN_RAW, 3),
    ];
    assert_eq!(min_bytecode_result, I256_MIN_RAW);
}

#[test]
fn abs_negative_just_past_zero() {
    // -1 = all-ones 256-bit. abs(-1) = 1.
    let minus_one: [u64; 4] = [u64::MAX; 4];
    assert_eq!(run_abs(minus_one, 0), 1);
    assert_eq!(run_abs(minus_one, 1), 0);
    assert_eq!(run_abs(minus_one, 2), 0);
    assert_eq!(run_abs(minus_one, 3), 0);
}


#[test]
#[should_panic(expected = "k_diff == I256::MIN")]
fn wide_signed_mul_div_floor_panics_on_min_k_diff() {
    wide_signed_mul_div_floor_reference([10, 0, 0, 0], I256_MIN_RAW, [3, 0, 0, 0]);
}

// =============================================================================
// checked_mul_i256 — MIN edges + non-MIN boundary against oracle
//
// The VM's MUL_I256 CHECKED opcode is expected to handle every MIN-involving
// edge case on its own (because abs(MIN) == MIN in two's-complement, and the
// downstream unsigned-overflow AND signed-range checks catch each overflowing
// path). These tests pin that behavior by comparing against the Rust reference.
// =============================================================================

/// Run `checked_mul_i256` for `a * b` on the VM, returning `Ok(limbs)` when the
/// VM reports no overflow and `Err(())` when it does. Each successful run also
/// verifies the wrapping VM returns the same limbs (sanity: they must agree
/// when the checked variant reports no overflow).
fn run_checked_mul_i256(a: [u64; 4], b: [u64; 4]) -> Result<[u64; 4], ()> {
    let overflow = run_u64(program_mul_i256_checked_return_overflow(a, b));
    if overflow == 1 {
        return Err(());
    }
    let l0 = run_u64(program_mul_i256_return_limb(a, b, 0));
    let l1 = run_u64(program_mul_i256_return_limb(a, b, 1));
    let l2 = run_u64(program_mul_i256_return_limb(a, b, 2));
    let l3 = run_u64(program_mul_i256_return_limb(a, b, 3));
    Ok([l0, l1, l2, l3])
}

fn assert_vm_matches_checked_mul_oracle(a: [u64; 4], b: [u64; 4], label: &str) {
    let oracle = checked_mul_i256_reference(a, b);
    let vm = run_checked_mul_i256(a, b);
    match (oracle, vm) {
        (None, Err(())) => {}
        (Some(r), Ok(v)) => assert_eq!(
            v, r,
            "{}: VM produced {:?} but oracle expected {:?}",
            label, v, r
        ),
        (None, Ok(v)) => panic!(
            "{}: oracle reports overflow but VM returned {:?} with overflow=0",
            label, v
        ),
        (Some(r), Err(())) => panic!(
            "{}: oracle returns {:?} but VM flagged overflow",
            label, r
        ),
    }
}

#[test]
fn checked_mul_i256_zero_times_min_is_zero() {
    assert_vm_matches_checked_mul_oracle([0; 4], I256_MIN_RAW, "0 × MIN");
    assert_vm_matches_checked_mul_oracle(I256_MIN_RAW, [0; 4], "MIN × 0");
}

#[test]
fn checked_mul_i256_min_times_one_is_min() {
    assert_vm_matches_checked_mul_oracle(I256_MIN_RAW, [1, 0, 0, 0], "MIN × 1");
    assert_vm_matches_checked_mul_oracle([1, 0, 0, 0], I256_MIN_RAW, "1 × MIN");
}

#[test]
fn checked_mul_i256_min_times_minus_one_overflows() {
    // -MIN = 2^255 doesn't fit in i256.
    let minus_one: [u64; 4] = [u64::MAX; 4];
    assert_vm_matches_checked_mul_oracle(I256_MIN_RAW, minus_one, "MIN × -1");
    assert_vm_matches_checked_mul_oracle(minus_one, I256_MIN_RAW, "-1 × MIN");
}

#[test]
fn checked_mul_i256_min_times_two_overflows() {
    // abs(MIN) × 2 = 2^256, unsigned-overflows.
    assert_vm_matches_checked_mul_oracle(I256_MIN_RAW, [2, 0, 0, 0], "MIN × 2");
    assert_vm_matches_checked_mul_oracle([2, 0, 0, 0], I256_MIN_RAW, "2 × MIN");
}

#[test]
fn checked_mul_i256_min_times_minus_two_overflows() {
    let minus_two = negate([2, 0, 0, 0]);
    assert_vm_matches_checked_mul_oracle(I256_MIN_RAW, minus_two, "MIN × -2");
    assert_vm_matches_checked_mul_oracle(minus_two, I256_MIN_RAW, "-2 × MIN");
}

#[test]
fn checked_mul_i256_min_times_min_overflows() {
    assert_vm_matches_checked_mul_oracle(I256_MIN_RAW, I256_MIN_RAW, "MIN × MIN");
}

#[test]
fn checked_mul_i256_min_times_max_overflows() {
    assert_vm_matches_checked_mul_oracle(I256_MIN_RAW, I256_MAX_RAW, "MIN × MAX");
    assert_vm_matches_checked_mul_oracle(I256_MAX_RAW, I256_MIN_RAW, "MAX × MIN");
}

#[test]
fn checked_mul_i256_negative_result_exactly_2_to_255_is_min() {
    // 2^128 × 2^127 = 2^255 (unsigned). With one side negative, the signed
    // result is -2^255 = MIN, which is representable — the only case where a
    // magnitude exactly equal to 2^255 is legal.
    let pos_2_128: [u64; 4] = [0, 0, 1, 0];
    let pos_2_127: [u64; 4] = [0, 0x8000_0000_0000_0000, 0, 0];
    let neg_2_128 = negate(pos_2_128);
    assert_vm_matches_checked_mul_oracle(neg_2_128, pos_2_127, "-2^128 × 2^127");
    assert_vm_matches_checked_mul_oracle(pos_2_127, neg_2_128, "2^127 × -2^128");
}

#[test]
fn checked_mul_i256_positive_result_exactly_2_to_255_overflows() {
    // 2^128 × 2^127 = 2^255 with both sides positive → overflows MAX=2^255-1.
    let pos_2_128: [u64; 4] = [0, 0, 1, 0];
    let pos_2_127: [u64; 4] = [0, 0x8000_0000_0000_0000, 0, 0];
    assert_vm_matches_checked_mul_oracle(pos_2_128, pos_2_127, "2^128 × 2^127");
}

#[test]
fn checked_mul_i256_small_signed_cases_match_oracle() {
    // Sanity: the common small-magnitude cases still agree end-to-end.
    let three = [3u64, 0, 0, 0];
    let four = [4u64, 0, 0, 0];
    let neg_three = negate(three);
    let neg_four = negate(four);
    assert_vm_matches_checked_mul_oracle(three, four, "3 × 4");
    assert_vm_matches_checked_mul_oracle(neg_three, four, "-3 × 4");
    assert_vm_matches_checked_mul_oracle(three, neg_four, "3 × -4");
    assert_vm_matches_checked_mul_oracle(neg_three, neg_four, "-3 × -4");
}

#[test]
fn checked_mul_i256_max_times_max_overflows() {
    // MAX^2 is ≈ 2^510; unsigned-overflows u256.
    assert_vm_matches_checked_mul_oracle(I256_MAX_RAW, I256_MAX_RAW, "MAX × MAX");
}

#[test]
fn checked_mul_i256_max_times_minus_one_matches_neg_max() {
    let minus_one: [u64; 4] = [u64::MAX; 4];
    assert_vm_matches_checked_mul_oracle(I256_MAX_RAW, minus_one, "MAX × -1");
    // Oracle result: -MAX = [1, 0, 0, 0x8000...1] — the limb just above MIN.
    // The key property is that MAX × -1 fits (unlike MIN × -1 which overflows).
    let expected = negate(I256_MAX_RAW);
    assert_eq!(
        checked_mul_i256_reference(I256_MAX_RAW, minus_one),
        Some(expected)
    );
}
