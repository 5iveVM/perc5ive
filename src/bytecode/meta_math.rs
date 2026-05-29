//! Meta-specific math for the percolator-meta genesis/fair-launch port.
//!
//! Source of truth: `hello_slab/percolator-meta/program/src/lib.rs`
//! (oracle clone, pinned SHA recorded in commit messages). All values are
//! u64 — percolator-meta's genesis ledger has no u128/i128 fields, so the
//! `five-dsl-compiler` u128 LHS-assignment regression does not apply here.
//!
//! Two forms per primitive:
//!   * A Rust **reference** that mirrors the upstream Rust 1:1 — the
//!     conformance oracle and the model used by the lifecycle test.
//!   * (for the one non-trivial primitive, vote weight) a hand-written
//!     **bytecode program** so the VM result can be checked bit-exact against
//!     the reference via `MitoVM::execute_direct`.
//!
//! The genesis handler bodies in `meta_handlers.rs` emit the same opcode
//! sequences inline; this module isolates the math so it is independently
//! testable.

use super::emit::Program;
use five_protocol::opcodes::{ADD, EQ, LT, MUL, RETURN_VALUE, SHIFT_RIGHT};

// =============================================================================
// Rust references — mirror percolator-meta/program/src/lib.rs exactly
// =============================================================================

/// Time-weighted vote power: `floor(log2(age)) * staked`.
///
/// Mirrors `genesis_vote_weight` (lib.rs:511). `age` is the position's age in
/// slots at vote time. Younger than 2 slots (log2 == 0) or zero stake has no
/// weight, so there is monotonic pressure to deposit earlier.
pub fn genesis_vote_weight(staked: u64, age: u64) -> u64 {
    if staked == 0 || age < 2 {
        return 0;
    }
    (age.ilog2() as u64).saturating_mul(staked)
}

/// Kickstart capital split: `insurance = floor(total/2)`, `backing = total -
/// insurance`. Mirrors `process_kickstart_genesis_market` (lib.rs:2652-2653).
pub fn kickstart_split(total_deposited: u64) -> (u64, u64) {
    let insurance = total_deposited / 2;
    let backing = total_deposited.saturating_sub(insurance);
    (insurance, backing)
}

/// Post-finalization recoverable principal, pro-rata against the vault's health
/// ratio. Mirrors `genesis_recoverable_principal` (lib.rs:881-899).
///
/// If the vault is solvent (`vault_balance >= outstanding`) the depositor
/// recovers their full remaining principal; otherwise they recover
/// `floor(remaining * vault_balance / outstanding)`. `outstanding == 0` with a
/// nonzero claim is a corrupt-state error upstream — modelled here as `None`.
pub fn genesis_recoverable_principal(
    remaining_principal: u64,
    vault_balance: u64,
    outstanding_principal: u64,
) -> Option<u64> {
    if remaining_principal == 0 {
        return Some(0);
    }
    if outstanding_principal == 0 {
        return None;
    }
    if vault_balance >= outstanding_principal {
        return Some(remaining_principal);
    }
    // u128 intermediate to avoid overflow, exactly as upstream.
    let scaled =
        (remaining_principal as u128) * (vault_balance as u128) / (outstanding_principal as u128);
    Some(scaled as u64)
}

/// Genesis distribution approval test. Mirrors the two gates in
/// `process_genesis_mint_reward` (lib.rs:2419-2426):
///   1. weighted majority: `yes_votes > no_votes`;
///   2. principal quorum: `voted_principal > outstanding_principal / 2`.
/// Exactly-half principal fails the quorum (strict `>`).
pub fn distribution_approved(
    yes_votes: u64,
    no_votes: u64,
    voted_principal: u64,
    outstanding_principal: u64,
) -> bool {
    yes_votes > no_votes && voted_principal > outstanding_principal / 2
}

// =============================================================================
// Bytecode — standalone scripts for VM conformance
// =============================================================================

/// Build a standalone `.five` script whose function 0 computes
/// `genesis_vote_weight(staked, age)` and returns it.
///
/// Calling convention (mono): scalar params compact into `params[1..]`, so
/// `staked` (first scalar) is `LOAD_PARAM_1` and `age` (second scalar) is
/// `LOAD_PARAM_2`. Locals: slot 0 = `age_work`, slot 1 = `k` (the log2 count).
///
/// The loop is `while age_work >= 2 { age_work >>= 1; k += 1 }`, which leaves
/// `k = floor(log2(age))` for `age >= 1`. Guards short-circuit `staked == 0`
/// and `age < 2` to a zero return, matching the reference.
pub fn program_genesis_vote_weight() -> Vec<u8> {
    let mut p = Program::new();
    p.emit_alloc_locals(2);

    // if staked == 0 → return 0
    p.emit_load_param(1);
    p.push_u64(0);
    p.raw(EQ);
    let to_zero_a = p.emit_jump_if_placeholder();

    // if age < 2 → return 0
    p.emit_load_param(2);
    p.push_u64(2);
    p.raw(LT);
    let to_zero_b = p.emit_jump_if_placeholder();

    // age_work = age; k = 0
    p.emit_load_param(2);
    p.emit_set_local(0);
    p.push_u64(0);
    p.emit_set_local(1);

    // loop: while age_work >= 2 { age_work >>= 1; k += 1 }
    let loop_start = p.current_absolute_offset();
    p.emit_get_local(0);
    p.push_u64(2);
    p.raw(LT); // age_work < 2 ?
    let to_break = p.emit_jump_if_placeholder();
    // age_work >>= 1
    p.emit_get_local(0);
    p.push_u64(1);
    p.raw(SHIFT_RIGHT);
    p.emit_set_local(0);
    // k += 1
    p.emit_get_local(1);
    p.push_u64(1);
    p.raw(ADD);
    p.emit_set_local(1);
    p.emit_jump_to(loop_start);

    // break: return k * staked
    p.patch_jump_to_here(to_break);
    p.emit_get_local(1);
    p.emit_load_param(1);
    p.raw(MUL);
    p.raw(RETURN_VALUE);

    // zero return
    p.patch_jump_to_here(to_zero_a);
    p.patch_jump_to_here(to_zero_b);
    p.push_u64(0);
    p.raw(RETURN_VALUE);

    p.finish_with_halt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vote_weight_reference_matches_spec_examples() {
        // age < 2 → 0 regardless of stake.
        assert_eq!(genesis_vote_weight(1_000, 0), 0);
        assert_eq!(genesis_vote_weight(1_000, 1), 0);
        // floor(log2(2)) = 1
        assert_eq!(genesis_vote_weight(1_000, 2), 1_000);
        assert_eq!(genesis_vote_weight(1_000, 3), 1_000);
        // floor(log2(4)) = 2
        assert_eq!(genesis_vote_weight(1_000, 4), 2_000);
        // floor(log2(1023)) = 9, floor(log2(1024)) = 10
        assert_eq!(genesis_vote_weight(1, 1023), 9);
        assert_eq!(genesis_vote_weight(1, 1024), 10);
        // zero stake → 0
        assert_eq!(genesis_vote_weight(0, 1_000_000), 0);
    }

    #[test]
    fn kickstart_split_is_50_50_floor() {
        assert_eq!(kickstart_split(0), (0, 0));
        assert_eq!(kickstart_split(1), (0, 1)); // floor(1/2)=0 insurance, backing 1
        assert_eq!(kickstart_split(100), (50, 50));
        assert_eq!(kickstart_split(101), (50, 51));
        let (i, b) = kickstart_split(u64::MAX);
        assert_eq!(i + b, u64::MAX, "split must conserve the pool");
    }

    #[test]
    fn recoverable_principal_solvent_and_lossy() {
        // Solvent vault → full remaining.
        assert_eq!(genesis_recoverable_principal(100, 1_000, 1_000), Some(100));
        assert_eq!(genesis_recoverable_principal(100, 2_000, 1_000), Some(100));
        // 50% loss → pro-rata half.
        assert_eq!(genesis_recoverable_principal(100, 500, 1_000), Some(50));
        // Zero claim → zero.
        assert_eq!(genesis_recoverable_principal(0, 500, 1_000), Some(0));
        // Corrupt: nonzero claim with zero outstanding.
        assert_eq!(genesis_recoverable_principal(100, 500, 0), None);
    }

    #[test]
    fn approval_requires_majority_and_strict_quorum() {
        // majority + quorum (>half of 1000 = >500).
        assert!(distribution_approved(10, 5, 501, 1_000));
        // exactly half principal fails the strict quorum.
        assert!(!distribution_approved(10, 5, 500, 1_000));
        // no weighted majority.
        assert!(!distribution_approved(5, 5, 900, 1_000));
        assert!(!distribution_approved(4, 5, 900, 1_000));
    }
}
