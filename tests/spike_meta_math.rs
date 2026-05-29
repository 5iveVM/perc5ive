//! Phase 0, Spike 2 — the one meta-specific math primitive that isn't a
//! trivial add/sub: the time-weighted vote `floor(log2(age)) * staked`.
//!
//! Confirms the hand-written `program_genesis_vote_weight` bytecode matches the
//! Rust reference bit-exact across >1k inputs when run on the real
//! `MitoVM::execute_direct`. Params-only (no accounts), so this exercises the
//! mono scalar-param pathway end-to-end: `staked → LOAD_PARAM_1`,
//! `age → LOAD_PARAM_2`.

use five_protocol::types;
use five_vm_mito::{MitoVM, Value};
use perc5ive::bytecode::meta_math::{genesis_vote_weight, program_genesis_vote_weight};

/// Encode the mono typed-param envelope for two u64 scalars (staked, age).
fn input_two_u64(func_idx: u32, a: u64, b: u64) -> Vec<u8> {
    let mut input = Vec::new();
    input.extend_from_slice(&func_idx.to_le_bytes());
    input.extend_from_slice(&2u32.to_le_bytes());
    input.push(types::U64);
    input.extend_from_slice(&a.to_le_bytes());
    input.push(types::U64);
    input.extend_from_slice(&b.to_le_bytes());
    input
}

fn run_vote_weight(script: &[u8], staked: u64, age: u64) -> u64 {
    let input = input_two_u64(0, staked, age);
    let result = MitoVM::execute_direct(
        script,
        &input,
        &[],
        &[0u8; 32],
        &mut five_vm_mito::StackStorage::new(),
    )
    .expect("VM accepts vote-weight script")
    .expect("vote-weight returns a value");
    match result {
        Value::U64(v) => v,
        Value::U128(v) => v as u64,
        other => panic!("expected U64 weight, got {other:?}"),
    }
}

#[test]
fn vote_weight_bytecode_matches_reference_edge_cases() {
    let script = program_genesis_vote_weight();
    let cases = [
        (0u64, 0u64),
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 4),
        (1, 1023),
        (1, 1024),
        (1_000, 2),
        (1_000, 1_000_000),
        (0, u64::MAX),
        (7, u64::MAX),
        (1, u64::MAX),
    ];
    for (staked, age) in cases {
        let got = run_vote_weight(&script, staked, age);
        let want = genesis_vote_weight(staked, age);
        assert_eq!(got, want, "vote_weight({staked}, {age}) VM={got} ref={want}");
    }
}

#[test]
fn vote_weight_bytecode_matches_reference_over_1k_inputs() {
    let script = program_genesis_vote_weight();
    // Deterministic spread of (staked, age) pairs covering every log2 bucket
    // from age=2 (k=1) up to age≈2^40, with a range of stake magnitudes. A
    // small LCG keeps it dependency-free and reproducible.
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        seed
    };
    let mut checked = 0u32;
    for _ in 0..1_200 {
        let staked = next() % 1_000_000_000; // up to ~1e9 base units
        // age spread across 0..2^44 so floor(log2) lands in many buckets,
        // including the <2 zero-weight region.
        let age = next() % (1u64 << 44);
        let got = run_vote_weight(&script, staked, age);
        let want = genesis_vote_weight(staked, age);
        assert_eq!(got, want, "vote_weight({staked}, {age}) VM={got} ref={want}");
        checked += 1;
    }
    // Plus every exact power-of-two boundary, where floor(log2) steps.
    for k in 1..50u32 {
        let age = 1u64 << k;
        for staked in [1u64, 3, 1_000, 999_999] {
            let got = run_vote_weight(&script, staked, age);
            let want = genesis_vote_weight(staked, age);
            assert_eq!(got, want, "pow2 vote_weight({staked}, {age})");
            // and age-1 (just below the boundary)
            let got_lo = run_vote_weight(&script, staked, age - 1);
            let want_lo = genesis_vote_weight(staked, age - 1);
            assert_eq!(got_lo, want_lo, "pow2-1 vote_weight({staked}, {})", age - 1);
            checked += 2;
        }
    }
    assert!(checked >= 1_000, "spike must check >=1k inputs, did {checked}");
}
