//! T1 — `accrue_market_to` adversarial input probe.
//!
//! Strategy: chain N `accrue_market_to(slot, oracle_price, funding_rate_e9)`
//! calls against a single `RiskEngine` with mixed positive/negative funding
//! rates, asymmetric OI (long vs short), and slot deltas spanning the
//! documented bounds. After each call, capture
//! `(cum_funding_long_e18, cum_funding_short_e18, last_market_slot,
//! oi_eff_long_q, oi_eff_short_q)` and check internal invariants:
//!
//! 1. **`check_conservation()` holds**: `vault >= c_tot + insurance_fund.balance`.
//!    The accrual path can credit/debit OI via funding payments — if any
//!    bookkeeping is wrong, this fails.
//! 2. **Insurance monotone-non-decreasing**: `accrue_market_to` itself
//!    should NOT debit insurance (only liquidation / settle paths do).
//!    Any decrease is a candidate.
//! 3. **`accrue_market_to` returns Ok or a known error**: an unexpected
//!    panic is a candidate.
//!
//! No DSL leg in this Session — the mono port dropped the multiprecision
//! opcodes that `accrue_market_to`'s internal e9/e18/q14 fixed-point
//! relies on, and `perc5ive::bytecode` doesn't expose the helper. So this
//! is engine-direct only: we're looking for invariant violations that
//! Jelleo's per-call PoCs didn't enumerate, not a cross-impl differential.

use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use percolator::{RiskEngine, RiskParams, U128};

use crate::bounty_fuzz::probe::{Divergence, ImplLeg, Probe, ProbeOutcome};

fn compute_r(engine: &RiskEngine) -> u128 {
    let v = engine.vault.get();
    let c = engine.c_tot.get();
    let i = engine.insurance_fund.balance.get();
    v.saturating_sub(c.saturating_add(i))
}

const MAX_TRADE_SIZE_Q: u64 = 1_000_000;
const DEFAULT_ORACLE: u64 = 1_000;
const DEFAULT_SLOT: u64 = 100;

fn default_params() -> RiskParams {
    RiskParams {
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        max_trading_fee_bps: 10,
        max_accounts: percolator::MAX_ACCOUNTS as u64,
        liquidation_fee_bps: 100,
        liquidation_fee_cap: U128::new(1_000_000),
        min_liquidation_abs: U128::new(0),
        min_nonzero_mm_req: 10,
        min_nonzero_im_req: 11,
        h_min: 0,
        h_max: 100,
        resolve_price_deviation_bps: 1000,
        max_accrual_dt_slots: 100,
        max_abs_funding_e9_per_slot: 10_000,
        min_funding_lifetime_slots: 10_000_000,
        max_active_positions_per_side: percolator::MAX_ACCOUNTS as u64,
        max_price_move_bps_per_slot: 3,
    }
}

pub struct T1FundingProbe {
    rng: XorShiftRng,
}

impl T1FundingProbe {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: XorShiftRng::seed_from_u64(seed),
        }
    }
}

impl Probe for T1FundingProbe {
    fn target(&self) -> &'static str {
        "t1_funding"
    }

    fn run(&mut self, probe_id: u64, _seed: u64) -> ProbeOutcome {
        let params = default_params();
        let init_oracle: u64 = 1 + (self.rng.r#gen::<u64>() % DEFAULT_ORACLE.saturating_mul(10));
        let init_slot: u64 = DEFAULT_SLOT + (self.rng.r#gen::<u64>() % 100);

        let mut engine = RiskEngine::new_with_market(params, init_slot, init_oracle);

        // Seed insurance so any decrease is observable.
        let initial_insurance: u128 = 1_000_000 + (self.rng.r#gen::<u32>() as u128 % 1_000_000);
        if engine.top_up_insurance_fund(initial_insurance, init_slot).is_err() {
            // Skip this probe — the random init values rejected the topup.
            return ProbeOutcome { probe_id, divergences: Vec::new() };
        }

        let mut divergences = Vec::new();
        let mut last_insurance = engine.insurance_fund.balance.get();
        let mut last_r = compute_r(&engine);
        let mut current_slot = init_slot;

        // Chain N accrue_market_to calls with adversarial inputs.
        let chain_len: usize = 5 + (self.rng.r#gen::<u8>() as usize % 16);
        for step in 0..chain_len {
            // Random dt within the documented cap.
            let dt: u64 = 1 + (self.rng.r#gen::<u64>() % engine.params.max_accrual_dt_slots);
            current_slot = current_slot.saturating_add(dt);

            // Random oracle price within (0, MAX_ORACLE_PRICE].
            let oracle_price: u64 = 1 + (self.rng.r#gen::<u64>() % (init_oracle * 100));

            // Funding rate signed; magnitude up to params.max_abs_funding_e9_per_slot.
            let rate_mag: i128 = (self.rng.r#gen::<u32>() as i128)
                .min(engine.params.max_abs_funding_e9_per_slot as i128);
            let funding_rate_e9: i128 = if self.rng.r#gen::<bool>() { rate_mag } else { -rate_mag };

            // Run.
            let res = engine.accrue_market_to(current_slot, oracle_price, funding_rate_e9);

            // Check invariants. Residual R must be conserved across
            // every accrue_market_to call (funding payments shuffle OI
            // accounting but should not touch vault/c_tot/insurance
            // bookkeeping in a way that moves R).
            let r_now = compute_r(&engine);
            if r_now != last_r {
                divergences.push(Divergence {
                    target: "t1_funding".to_string(),
                    field: "residual_R".to_string(),
                    leg_a: ImplLeg::RustRef,
                    leg_b: ImplLeg::RustRef,
                    value_a: format!("{last_r}"),
                    value_b: format!("{r_now}"),
                    note: format!(
                        "probe_id={probe_id} step={step} dt={dt} oracle={oracle_price} rate_e9={funding_rate_e9} delta_R={} res={:?}",
                        (r_now as i128) - (last_r as i128),
                        res.as_ref().err()
                    ),
                });
            }
            last_r = r_now;
            let ins_now = engine.insurance_fund.balance.get();
            if ins_now < last_insurance {
                divergences.push(Divergence {
                    target: "t1_funding".to_string(),
                    field: "insurance_fund.balance".to_string(),
                    leg_a: ImplLeg::RustRef,
                    leg_b: ImplLeg::RustRef,
                    value_a: format!("{}", last_insurance),
                    value_b: format!("{}", ins_now),
                    note: format!(
                        "probe_id={probe_id} step={step} accrue_market_to drained insurance",
                    ),
                });
            }
            last_insurance = ins_now;

            // If the call errored, stop chaining — further steps won't tell us more.
            if res.is_err() {
                break;
            }
        }

        let _ = MAX_TRADE_SIZE_Q; // reserved for future K-pair probes
        ProbeOutcome { probe_id, divergences }
    }
}
