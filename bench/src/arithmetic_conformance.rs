//! Arithmetic conformance — compares Rust reference implementations against
//! the bytecode-driven VM execution for every u256 / i256 / i128 op that
//! perc5ive ships.
//!
//! Intentionally starts with a small, high-signal probe set: spec edge
//! cases (MIN / MAX / carry-chain probes) and regression anchors.

use crate::ConformanceReport;
use five_vm_mito::{MitoVM, Value};
use perc5ive::bytecode::u256::{
    program_add_u256_return_limb, program_sub_u256_return_limb,
    saturating_add_u256_reference, saturating_sub_u256_reference,
};

/// Probe vectors common to all U256 conformance checks.
/// Covers: zeros, single-limb, carry-into-limb-1, full saturation, and an
/// asymmetric mid-range value.
pub const U256_PROBES: &[[u64; 4]] = &[
    [0, 0, 0, 0],
    [1, 0, 0, 0],
    [u64::MAX, 0, 0, 0],
    [0, 1, 0, 0],
    [u64::MAX, u64::MAX, 0, 0],
    [u64::MAX, u64::MAX, u64::MAX, 0],
    [0xDEAD_BEEF, 0xCAFE_F00D, 0x1234_5678, 0xAAAA_AAAA],
];

/// Reference U256 addition — matches the semantics of the VM's ADD_U256
/// in wrapping mode (no overflow signalling).
pub fn add_u256_wrapping_reference(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut out = [0u64; 4];
    let mut carry: u128 = 0;
    for i in 0..4 {
        let s = a[i] as u128 + b[i] as u128 + carry;
        out[i] = s as u64;
        carry = s >> 64;
    }
    out
}

/// Reference U256 subtraction — matches ADD_U256 wrapping semantics.
pub fn sub_u256_wrapping_reference(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut out = [0u64; 4];
    let mut borrow: i128 = 0;
    for i in 0..4 {
        let d = a[i] as i128 - b[i] as i128 - borrow;
        if d < 0 {
            out[i] = (d + (1i128 << 64)) as u64;
            borrow = 1;
        } else {
            out[i] = d as u64;
            borrow = 0;
        }
    }
    out
}

fn run_return_u64(bytecode: &[u8]) -> u64 {
    let result = MitoVM::execute_direct(bytecode, &[], &[])
        .expect("vm: execute_direct should succeed");
    match result {
        Some(Value::U64(v)) => v,
        other => panic!("vm: expected U64 return, got {:?}", other),
    }
}

/// Run each limb of `program_add_u256_return_limb` and verify against the
/// reference. Four VM invocations per probe pair (one per limb).
pub fn add_u256_vm_conformance(report: &mut ConformanceReport) {
    for (i, a) in U256_PROBES.iter().enumerate() {
        for (j, b) in U256_PROBES.iter().enumerate() {
            let expected = add_u256_wrapping_reference(*a, *b);
            for limb_idx in 0..4u8 {
                let bytecode = program_add_u256_return_limb(*a, *b, limb_idx);
                let vm_result = run_return_u64(&bytecode);
                let name = format!(
                    "vm_add_u256_wrapping_matches_reference[{},{},limb{}]",
                    i, j, limb_idx
                );
                if vm_result == expected[limb_idx as usize] {
                    report.record_pass(&name);
                } else {
                    report.record_fail(
                        &name,
                        &format!(
                            "vm={:#x} reference={:#x} a={:?} b={:?}",
                            vm_result, expected[limb_idx as usize], a, b
                        ),
                    );
                }
            }
        }
    }
}

/// Run each limb of `program_sub_u256_return_limb` and verify.
pub fn sub_u256_vm_conformance(report: &mut ConformanceReport) {
    for (i, a) in U256_PROBES.iter().enumerate() {
        for (j, b) in U256_PROBES.iter().enumerate() {
            let expected = sub_u256_wrapping_reference(*a, *b);
            for limb_idx in 0..4u8 {
                let bytecode = program_sub_u256_return_limb(*a, *b, limb_idx);
                let vm_result = run_return_u64(&bytecode);
                let name = format!(
                    "vm_sub_u256_wrapping_matches_reference[{},{},limb{}]",
                    i, j, limb_idx
                );
                if vm_result == expected[limb_idx as usize] {
                    report.record_pass(&name);
                } else {
                    report.record_fail(
                        &name,
                        &format!(
                            "vm={:#x} reference={:#x} a={:?} b={:?}",
                            vm_result, expected[limb_idx as usize], a, b
                        ),
                    );
                }
            }
        }
    }
}

/// Saturating-reference sanity — makes sure the saturating_* helpers in
/// `perc5ive::bytecode::u256` clamp to 0 / MAX consistently.
pub fn saturating_add_u256_conformance(report: &mut ConformanceReport) {
    // MAX + 1 saturates to MAX.
    let max = [u64::MAX; 4];
    let one = [1, 0, 0, 0];
    let name = "saturating_add_u256_max_plus_one_clamps_to_max".to_string();
    if saturating_add_u256_reference(max, one) == max {
        report.record_pass(&name);
    } else {
        report.record_fail(&name, "did not clamp to MAX");
    }

    // Small + small does not saturate.
    let a = [10, 0, 0, 0];
    let b = [20, 0, 0, 0];
    let expected = [30, 0, 0, 0];
    let name = "saturating_add_u256_small_plus_small_matches_sum".to_string();
    if saturating_add_u256_reference(a, b) == expected {
        report.record_pass(&name);
    } else {
        report.record_fail(&name, "did not produce 30");
    }
}

pub fn saturating_sub_u256_conformance(report: &mut ConformanceReport) {
    // Zero - one clamps to zero.
    let zero = [0u64; 4];
    let one = [1, 0, 0, 0];
    let name = "saturating_sub_u256_zero_minus_one_clamps_to_zero".to_string();
    if saturating_sub_u256_reference(zero, one) == zero {
        report.record_pass(&name);
    } else {
        report.record_fail(&name, "did not clamp to 0");
    }

    // Big - small is normal subtraction.
    let a = [100, 0, 0, 0];
    let b = [40, 0, 0, 0];
    let expected = [60, 0, 0, 0];
    let name = "saturating_sub_u256_normal_subtraction".to_string();
    if saturating_sub_u256_reference(a, b) == expected {
        report.record_pass(&name);
    } else {
        report.record_fail(&name, "did not produce 60");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_u256_vm_conformance_all_pass() {
        let mut report = ConformanceReport::new();
        add_u256_vm_conformance(&mut report);
        assert!(
            report.is_pass(),
            "{}: {:?}",
            report.summary(),
            report.failed
        );
    }

    #[test]
    fn sub_u256_vm_conformance_all_pass() {
        let mut report = ConformanceReport::new();
        sub_u256_vm_conformance(&mut report);
        assert!(
            report.is_pass(),
            "{}: {:?}",
            report.summary(),
            report.failed
        );
    }

    #[test]
    fn saturating_add_u256_conformance_all_pass() {
        let mut report = ConformanceReport::new();
        saturating_add_u256_conformance(&mut report);
        assert!(
            report.is_pass(),
            "{}: {:?}",
            report.summary(),
            report.failed
        );
    }

    #[test]
    fn saturating_sub_u256_conformance_all_pass() {
        let mut report = ConformanceReport::new();
        saturating_sub_u256_conformance(&mut report);
        assert!(
            report.is_pass(),
            "{}: {:?}",
            report.summary(),
            report.failed
        );
    }

    #[test]
    fn add_u256_reference_carry_chain() {
        let a = [u64::MAX, 0, 0, 0];
        let b = [1, 0, 0, 0];
        let sum = add_u256_wrapping_reference(a, b);
        assert_eq!(sum, [0, 1, 0, 0]);
    }

    #[test]
    fn add_u256_reference_wraps_on_max() {
        let a = [u64::MAX; 4];
        let b = [1, 0, 0, 0];
        let sum = add_u256_wrapping_reference(a, b);
        assert_eq!(sum, [0, 0, 0, 0]); // wrap to zero
    }
}
