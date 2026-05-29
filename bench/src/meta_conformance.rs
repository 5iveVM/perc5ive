//! PercolatorBench — percolator-meta genesis / futarchy conformance (Phase 6).
//!
//! Confirms the perc5ive genesis math matches its Rust reference, the same way
//! the risk-engine modules conform: the one non-trivial primitive (the
//! time-weighted vote weight) is checked **bytecode-vs-reference bit-exact** on
//! the real VM, and the remaining ledger arithmetic (kickstart split, quorum +
//! majority gate, pro-rata-under-loss recovery) is checked against its
//! reference's stated invariants over a probe set.
//!
//! References live in `perc5ive::bytecode::meta_math` and mirror
//! `hello_slab/percolator-meta/program/src/lib.rs` (pinned SHA in commits). The
//! handler bodies in `meta_handlers.rs` emit the same opcode sequences inline,
//! so a reference match witnesses the bytecode.

use crate::ConformanceReport;
use five_protocol::types;
use five_vm_mito::{MitoVM, StackStorage, Value};
use perc5ive::bytecode::meta_math::{
    distribution_approved, genesis_recoverable_principal, genesis_vote_weight, kickstart_split,
    program_genesis_vote_weight,
};

/// Run `program_genesis_vote_weight` on the real VM for `(staked, age)`.
fn vm_vote_weight(script: &[u8], staked: u64, age: u64) -> u64 {
    let mut input = Vec::new();
    input.extend_from_slice(&0u32.to_le_bytes()); // func idx
    input.extend_from_slice(&2u32.to_le_bytes()); // 2 scalars
    input.push(types::U64);
    input.extend_from_slice(&staked.to_le_bytes());
    input.push(types::U64);
    input.extend_from_slice(&age.to_le_bytes());
    match MitoVM::execute_direct(script, &input, &[], &[0u8; 32], &mut StackStorage::new())
        .expect("vote-weight script runs")
        .expect("returns a value")
    {
        Value::U64(v) => v,
        Value::U128(v) => v as u64,
        other => panic!("unexpected vote-weight return {other:?}"),
    }
}

/// Full meta conformance run, returning a [`ConformanceReport`].
pub fn run() -> ConformanceReport {
    let mut r = ConformanceReport::new();
    let script = program_genesis_vote_weight();

    // --- vote weight: bytecode vs reference, every log2 bucket + boundaries ---
    let mut vote_ok = true;
    let mut seed: u64 = 0x1234_5678_9abc_def0;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        seed
    };
    for _ in 0..400 {
        let staked = next() % 1_000_000_000;
        let age = next() % (1u64 << 40);
        if vm_vote_weight(&script, staked, age) != genesis_vote_weight(staked, age) {
            vote_ok = false;
        }
    }
    for k in 1..48u32 {
        let age = 1u64 << k;
        for staked in [1u64, 7, 1_000, 999_983] {
            for a in [age - 1, age] {
                if vm_vote_weight(&script, staked, a) != genesis_vote_weight(staked, a) {
                    vote_ok = false;
                }
            }
        }
    }
    if vote_ok {
        r.record_pass("vote_weight_bytecode_matches_reference");
    } else {
        r.record_fail("vote_weight_bytecode_matches_reference", "VM != reference");
    }

    // --- kickstart split conserves the pool and floors insurance ---
    let mut split_ok = true;
    for total in [0u64, 1, 2, 100, 101, u64::MAX, 7_777_777] {
        let (ins, back) = kickstart_split(total);
        if ins != total / 2 || ins.wrapping_add(back) != total {
            split_ok = false;
        }
    }
    if split_ok {
        r.record_pass("kickstart_split_conserves_and_floors");
    } else {
        r.record_fail("kickstart_split_conserves_and_floors", "split invariant broken");
    }

    // --- distribution approval: majority AND strict principal quorum ---
    let approval_cases = [
        ((10, 5, 501, 1000), true),  // majority + quorum
        ((10, 5, 500, 1000), false), // exactly half fails strict quorum
        ((5, 5, 900, 1000), false),  // tie is not a majority
        ((4, 5, 900, 1000), false),  // minority
        ((1, 0, 1, 0), true),        // zero outstanding -> any yes principal clears
    ];
    let mut approval_ok = true;
    for ((yes, no, vp, out), want) in approval_cases {
        if distribution_approved(yes, no, vp, out) != want {
            approval_ok = false;
        }
    }
    if approval_ok {
        r.record_pass("distribution_majority_and_strict_quorum");
    } else {
        r.record_fail("distribution_majority_and_strict_quorum", "gate mismatch");
    }

    // --- recoverable principal: full when solvent, pro-rata under loss ---
    let recover_cases = [
        ((100, 1000, 1000), Some(100)),
        ((100, 2000, 1000), Some(100)),
        ((100, 500, 1000), Some(50)),
        ((0, 500, 1000), Some(0)),
        ((100, 500, 0), None),
    ];
    let mut recover_ok = true;
    for ((rem, vault, out), want) in recover_cases {
        if genesis_recoverable_principal(rem, vault, out) != want {
            recover_ok = false;
        }
    }
    if recover_ok {
        r.record_pass("recoverable_principal_solvent_and_lossy");
    } else {
        r.record_fail("recoverable_principal_solvent_and_lossy", "recovery mismatch");
    }

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_conformance_run_is_all_green() {
        let report = run();
        assert!(
            report.is_pass(),
            "meta conformance failures: {:?}\n{}",
            report.failed,
            report.summary()
        );
        // 4 properties: vote-weight (bytecode), split, quorum, recovery.
        assert_eq!(report.total(), 4, "{}", report.summary());
    }
}
