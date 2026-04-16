//! End-to-end tests for the u256 bytecode sequences.
//!
//! Each test builds a `.five` binary via `perc5ive::bytecode::u256`, executes it
//! through `MitoVM::execute_direct`, and checks the returned value against a
//! native reference. Coverage targets every Percolator-facing op: ADD / SUB /
//! MUL / DIV (both wrapping and checked), CMP, MULDIV (the 512-bit-intermediate
//! hotspot), plus is_zero + saturating-reference sanity checks.
//!
//! If any test fails, the issue is either in the bytecode sequence or in the
//! VM opcode handler — the Rust references here are correct by construction.

use five_vm_mito::{MitoVM, Value};
use perc5ive::bytecode::u256::{
    program_add_u256_checked_return_overflow, program_add_u256_return_limb, program_cmp_u256,
    program_div_u256_return_limb, program_is_zero_u256,
    program_mul_u256_checked_return_overflow, program_mul_u256_return_limb,
    program_muldiv_u256_return_limb, program_saturating_add_u256_return_limb,
    program_saturating_sub_u256_return_limb, program_sub_u256_checked_return_underflow,
    program_sub_u256_return_limb, program_try_into_u128_lo,
    saturating_add_u256_reference, saturating_sub_u256_reference,
};

fn u128_as_u256(v: u128) -> [u64; 4] {
    [v as u64, (v >> 64) as u64, 0, 0]
}

fn u128_limbs(v: u128) -> [u64; 2] {
    [v as u64, (v >> 64) as u64]
}

fn run_u64(bytecode: Vec<u8>) -> u64 {
    let result = MitoVM::execute_direct(&bytecode, &[], &[])
        .expect("VM execution should succeed");
    match result {
        Some(Value::U64(v)) => v,
        Some(Value::Bool(b)) => b as u64,
        Some(other) => panic!("expected Value::U64, got {:?}", other),
        None => panic!("expected a return value, got None"),
    }
}

fn run_all_limbs<F>(program_fn: F, expected: [u64; 4])
where
    F: Fn(u8) -> Vec<u8>,
{
    for limb in 0..4u8 {
        assert_eq!(
            run_u64(program_fn(limb)),
            expected[limb as usize],
            "limb {} mismatch",
            limb
        );
    }
}

// =============================================================================
// ADD_U256
// =============================================================================

#[test]
fn add_u256_carries_across_all_limbs() {
    let a = [u64::MAX, u64::MAX, u64::MAX, 0];
    let b = [1u64, 0, 0, 0];
    run_all_limbs(|l| program_add_u256_return_limb(a, b, l), [0, 0, 0, 1]);
}

#[test]
fn add_u256_u128_range_matches_native() {
    for &(a, b) in &[
        (0u128, 0u128),
        (1, 1),
        (u64::MAX as u128, u64::MAX as u128),
        (u128::MAX / 2, u128::MAX / 2),
        (1_000_000_000_000_000_000u128, 999_999_999_999_999_999u128),
    ] {
        let expected = a.wrapping_add(b);
        let [elo, ehi] = u128_limbs(expected);
        assert_eq!(run_u64(program_add_u256_return_limb(u128_as_u256(a), u128_as_u256(b), 0)), elo);
        assert_eq!(run_u64(program_add_u256_return_limb(u128_as_u256(a), u128_as_u256(b), 1)), ehi);
    }
}

#[test]
fn add_u256_checked_reports_overflow() {
    assert_eq!(run_u64(program_add_u256_checked_return_overflow([u64::MAX; 4], [1, 0, 0, 0])), 1);
    assert_eq!(run_u64(program_add_u256_checked_return_overflow([1, 2, 3, 4], [5, 6, 7, 8])), 0);
}

// =============================================================================
// SUB_U256
// =============================================================================

#[test]
fn sub_u256_basic() {
    let a = [100u64, 0, 0, 0];
    let b = [30u64, 0, 0, 0];
    run_all_limbs(|l| program_sub_u256_return_limb(a, b, l), [70, 0, 0, 0]);
}

#[test]
fn sub_u256_borrow_chain() {
    // 2^128 - 1 = (u128::MAX, 0, 0)
    let a = [0u64, 0, 1, 0];
    let b = [1u64, 0, 0, 0];
    run_all_limbs(
        |l| program_sub_u256_return_limb(a, b, l),
        [u64::MAX, u64::MAX, 0, 0],
    );
}

#[test]
fn sub_u256_wraps_on_underflow() {
    // 0 - 1 wraps to u256::MAX
    run_all_limbs(
        |l| program_sub_u256_return_limb([0; 4], [1, 0, 0, 0], l),
        [u64::MAX; 4],
    );
}

#[test]
fn sub_u256_checked_underflow() {
    assert_eq!(run_u64(program_sub_u256_checked_return_underflow([0; 4], [1, 0, 0, 0])), 1);
    assert_eq!(run_u64(program_sub_u256_checked_return_underflow([100, 0, 0, 0], [1, 0, 0, 0])), 0);
}

// =============================================================================
// MUL_U256
// =============================================================================

#[test]
fn mul_u256_small() {
    let a = [7u64, 0, 0, 0];
    let b = [6u64, 0, 0, 0];
    run_all_limbs(|l| program_mul_u256_return_limb(a, b, l), [42, 0, 0, 0]);
}

#[test]
fn mul_u256_u128_range_matches_native() {
    for &(a, b) in &[
        (2u128, 3u128),
        (u64::MAX as u128, 2),
        (1u128 << 60, 1u128 << 60),
        (u64::MAX as u128, u64::MAX as u128),
    ] {
        let expected: u128 = a.checked_mul(b).expect("fits in u128 for this case");
        let [elo, ehi] = u128_limbs(expected);
        assert_eq!(run_u64(program_mul_u256_return_limb(u128_as_u256(a), u128_as_u256(b), 0)), elo);
        assert_eq!(run_u64(program_mul_u256_return_limb(u128_as_u256(a), u128_as_u256(b), 1)), ehi);
    }
}

#[test]
fn mul_u256_checked_high_limb_overflow() {
    // a = 2^192, b = 2^64 ⇒ product = 2^256, overflows u256.
    let a = [0u64, 0, 0, 1];
    let b = [0u64, 1, 0, 0];
    assert_eq!(run_u64(program_mul_u256_checked_return_overflow(a, b)), 1);
    // Small multiplications don't overflow.
    assert_eq!(run_u64(program_mul_u256_checked_return_overflow([3, 0, 0, 0], [4, 0, 0, 0])), 0);
}

// =============================================================================
// DIV_U256
// =============================================================================

#[test]
fn div_u256_small() {
    let a = [100u64, 0, 0, 0];
    let b = [3u64, 0, 0, 0];
    run_all_limbs(|l| program_div_u256_return_limb(a, b, l), [33, 0, 0, 0]);
}

#[test]
fn div_u256_u128_range_matches_native() {
    for &(a, b) in &[
        (100u128, 3u128),
        (u128::MAX, 2),
        (u64::MAX as u128, 7),
        (1_000_000_000_000u128, 1_000u128),
    ] {
        let expected = a / b;
        let [elo, ehi] = u128_limbs(expected);
        assert_eq!(run_u64(program_div_u256_return_limb(u128_as_u256(a), u128_as_u256(b), 0)), elo);
        assert_eq!(run_u64(program_div_u256_return_limb(u128_as_u256(a), u128_as_u256(b), 1)), ehi);
    }
}

// =============================================================================
// CMP_U256
// =============================================================================

#[test]
fn cmp_u256_total_ordering() {
    let small = [1u64, 0, 0, 0];
    let big = [0u64, 0, 0, 1];
    assert_eq!(run_u64(program_cmp_u256(small, big)), 0);
    assert_eq!(run_u64(program_cmp_u256(big, small)), 2);
    assert_eq!(run_u64(program_cmp_u256(big, big)), 1);
}

#[test]
fn cmp_u256_high_limb_decides() {
    // Even if lower limbs differ wildly, a difference in the top limb dominates.
    let a = [u64::MAX, u64::MAX, u64::MAX, 1];
    let b = [0u64, 0, 0, 2];
    assert_eq!(run_u64(program_cmp_u256(a, b)), 0, "high-limb lt");
}

// =============================================================================
// MULDIV_U256 (the Percolator hotspot)
// =============================================================================

#[test]
fn muldiv_u256_scaled_integer_math() {
    let scale: u128 = 1_000_000_000_000_000_000; // 1e18
    let position: u128 = 500_000_000_000_000_000; // 5e17
    let ratio: u128 = 750_000_000_000_000_000; // 7.5e17
    let expected: u128 = position.wrapping_mul(ratio).wrapping_div(scale);
    let [elo, ehi] = u128_limbs(expected);
    assert_eq!(
        run_u64(program_muldiv_u256_return_limb(
            u128_as_u256(position),
            u128_as_u256(ratio),
            u128_as_u256(scale),
            0
        )),
        elo
    );
    assert_eq!(
        run_u64(program_muldiv_u256_return_limb(
            u128_as_u256(position),
            u128_as_u256(ratio),
            u128_as_u256(scale),
            1
        )),
        ehi
    );
}

#[test]
fn muldiv_u256_uses_512_bit_intermediate() {
    // (2^128 * 2^128) / 2^129 = 2^127 ⇒ [0, 1<<63, 0, 0].
    let a = [0u64, 0, 1, 0];
    let b = [0u64, 0, 1, 0];
    let c = [0u64, 0, 2, 0];
    run_all_limbs(
        |l| program_muldiv_u256_return_limb(a, b, c, l),
        [0, 1u64 << 63, 0, 0],
    );
}

// =============================================================================
// Composite helpers
// =============================================================================

#[test]
fn is_zero_u256_detects_zero() {
    assert_eq!(run_u64(program_is_zero_u256([0; 4])), 1, "cmp(0,0)=eq");
    assert_eq!(run_u64(program_is_zero_u256([1, 0, 0, 0])), 2, "cmp(1,0)=gt");
}

#[test]
fn try_into_u128_lo_returns_low_limb() {
    let v = u128::MAX / 2;
    let limbs = u128_as_u256(v);
    assert_eq!(run_u64(program_try_into_u128_lo(limbs)), limbs[0]);
}

// =============================================================================
// Saturating references
// =============================================================================

#[test]
fn saturating_add_reference_clamps() {
    assert_eq!(saturating_add_u256_reference([u64::MAX; 4], [1, 0, 0, 0]), [u64::MAX; 4]);
    assert_eq!(saturating_add_u256_reference([3, 0, 0, 0], [4, 0, 0, 0]), [7, 0, 0, 0]);
}

#[test]
fn saturating_sub_reference_clamps() {
    assert_eq!(saturating_sub_u256_reference([0; 4], [1, 0, 0, 0]), [0; 4]);
    assert_eq!(saturating_sub_u256_reference([10, 0, 0, 0], [3, 0, 0, 0]), [7, 0, 0, 0]);
}

// =============================================================================
// Saturating bytecode programs (use JUMP_IF_NOT under the hood)
// =============================================================================

#[test]
fn saturating_add_u256_normal_path_returns_sum() {
    let a = u128_as_u256(123);
    let b = u128_as_u256(456);
    run_all_limbs(
        |l| program_saturating_add_u256_return_limb(a, b, l),
        u128_as_u256(579),
    );
}

#[test]
fn saturating_add_u256_overflow_clamps_to_max() {
    run_all_limbs(
        |l| program_saturating_add_u256_return_limb([u64::MAX; 4], [1, 0, 0, 0], l),
        [u64::MAX; 4],
    );
}

#[test]
fn saturating_add_u256_overflow_at_high_limb() {
    // a = 2^192, b = 2^192. Sum's limb 3 wraps from 2 to 0... wait, 2 fits.
    // Use a where limb 3 = u64::MAX so any addition there overflows.
    let a = [0u64, 0, 0, u64::MAX];
    let b = [0u64, 0, 0, 1];
    run_all_limbs(
        |l| program_saturating_add_u256_return_limb(a, b, l),
        [u64::MAX; 4],
    );
}

#[test]
fn saturating_sub_u256_normal_path_returns_diff() {
    let a = u128_as_u256(1000);
    let b = u128_as_u256(250);
    run_all_limbs(
        |l| program_saturating_sub_u256_return_limb(a, b, l),
        u128_as_u256(750),
    );
}

#[test]
fn saturating_sub_u256_underflow_clamps_to_zero() {
    run_all_limbs(
        |l| program_saturating_sub_u256_return_limb([0; 4], [1, 0, 0, 0], l),
        [0; 4],
    );
}
