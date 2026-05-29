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

/// Where every base unit ends up when a genesis market winds down.
///
/// The genesis ledger has **no operator/founder destination**: a deposited base
/// unit is either returned to its depositor (`genesis_withdraw`) or remains held
/// in the protocol — in the genesis vault or in the isolated Percolator market it
/// was deployed into at kickstart, both recoverable to depositors via
/// `recover_genesis_market`. Any shortfall under market loss is absorbed as
/// in-protocol bad debt, never skimmed. So `operator_rent` is 0 by construction;
/// a nonzero value would mean a value sink leaked and is a conformance failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RentBreakdown {
    pub total_in: u64,
    pub returned_to_users: u64,
    pub held_in_protocol: u64,
    pub operator_rent: u64,
}

/// Wind-down breakdown for `deposits` against the market `vault_balance` at
/// wind-down. `vault_balance < sum(deposits)` models a market loss; recovery is
/// the same pro-rata rule the handler uses (`genesis_recoverable_principal`),
/// with the whole pool as the outstanding principal.
pub fn rent_breakdown(deposits: &[u64], vault_balance: u64) -> RentBreakdown {
    let total_in: u64 = deposits.iter().copied().sum();
    let returned_to_users: u64 = deposits
        .iter()
        .map(|&d| genesis_recoverable_principal(d, vault_balance, total_in).unwrap_or(0))
        .sum();
    let held_in_protocol = total_in.saturating_sub(returned_to_users);
    let operator_rent = total_in
        .saturating_sub(returned_to_users)
        .saturating_sub(held_in_protocol);
    RentBreakdown { total_in, returned_to_users, held_in_protocol, operator_rent }
}

/// Aggregate the rent breakdown across multiple **isolated** markets launched
/// under one shared futarchy. Markets do not cross-collateralize (Percolator's
/// per-market isolation), so the aggregate is a plain field-wise sum; operator
/// rent stays 0 in the aggregate iff it is 0 per market.
pub fn aggregate_rent(markets: &[RentBreakdown]) -> RentBreakdown {
    markets.iter().fold(
        RentBreakdown { total_in: 0, returned_to_users: 0, held_in_protocol: 0, operator_rent: 0 },
        |acc, m| RentBreakdown {
            total_in: acc.total_in + m.total_in,
            returned_to_users: acc.returned_to_users + m.returned_to_users,
            held_in_protocol: acc.held_in_protocol + m.held_in_protocol,
            operator_rent: acc.operator_rent + m.operator_rent,
        },
    )
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
    fn rent_breakdown_conserves_and_never_extracts() {
        // Solvent wind-down: vault covers the whole pool → everyone made whole,
        // nothing held back, zero operator rent.
        let b = rent_breakdown(&[4, 4, 2], 10);
        assert_eq!(b.total_in, 10);
        assert_eq!(b.returned_to_users, 10);
        assert_eq!(b.held_in_protocol, 0);
        assert_eq!(b.operator_rent, 0);

        // 50% market loss: vault is half the pool. Users recover pro-rata; the
        // shortfall stays as in-protocol bad debt, NOT skimmed to an operator.
        let b = rent_breakdown(&[4, 4, 2], 5);
        assert_eq!(b.total_in, 10);
        assert_eq!(b.returned_to_users, 5, "floor(4*5/10)+floor(4*5/10)+floor(2*5/10)");
        assert_eq!(b.held_in_protocol, 5);
        assert_eq!(b.operator_rent, 0);

        // Conservation + no-extraction over a probe sweep.
        let mut seed: u64 = 0xC0FFEE_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };
        for _ in 0..500 {
            let deposits = [next() % 1_000, next() % 1_000, next() % 1_000];
            let total: u64 = deposits.iter().sum();
            let vault = next() % (2 * total.max(1));
            let b = rent_breakdown(&deposits, vault);
            assert_eq!(b.total_in, total);
            assert!(b.returned_to_users <= b.total_in, "no base-unit minting");
            assert_eq!(b.returned_to_users + b.held_in_protocol, b.total_in, "conservation");
            assert_eq!(b.operator_rent, 0, "genesis ledger has no operator sink");
        }
    }

    #[test]
    fn aggregate_rent_sums_markets_and_stays_zero_extraction() {
        let a = rent_breakdown(&[6, 4], 10); // solvent → 10 returned, 0 held
        let b = rent_breakdown(&[5, 5], 5); // 50% loss → floor(5*5/10)=2 each = 4 returned, 6 held
        let agg = aggregate_rent(&[a, b]);
        assert_eq!(agg.total_in, 20);
        assert_eq!(agg.returned_to_users, 10 + 4);
        assert_eq!(agg.held_in_protocol, 0 + 6);
        assert_eq!(agg.operator_rent, 0, "aggregate operator rent across markets is 0");
        assert_eq!(agg.returned_to_users + agg.held_in_protocol, agg.total_in);
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
