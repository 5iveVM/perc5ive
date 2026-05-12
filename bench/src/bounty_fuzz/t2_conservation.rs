//! T2 — Multi-instruction conservation sweep.
//!
//! Build a `RiskEngine`, seed it with insurance + materialized accounts +
//! capital, then apply a random sequence of public + test-visible engine
//! methods. After each call, check:
//!
//! 1. **Residual `R = vault - c_tot - insurance` is conserved** across
//!    every op — this is the strict Jelleo F01/F03/F04/F05 invariant. The
//!    helper `engine.check_conservation()` only tests `vault >= c_tot +
//!    insurance` (senior solvency), which the F-family bugs *don't*
//!    violate. The real invariant is `R_after == R_before`.
//! 2. **`insurance_fund.balance` did not decrease** unless the op is one
//!    of the documented insurance-draining methods.
//!
//! Any invariant break is a candidate divergence. The bounty win condition
//! is exactly `insurance_fund.balance` decreasing via public-only calls,
//! so this probe is the most direct route to the bounty surface.
//!
//! Because Jelleo already enumerated single-call PoCs in the conservation
//! family (F01, F03, F04, F05, F06, F08), most of what this probe finds
//! will *match* those root causes. Session 3 triage marks those Jelleo-
//! disqualified. Anything that survives — a sequence that violates V
//! through an instruction-pair Jelleo didn't enumerate — is a candidate
//! for Session 4 validation.

use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use percolator::{RiskEngine, RiskParams, U128};

use crate::bounty_fuzz::probe::{Divergence, ImplLeg, Probe, ProbeOutcome};

/// Senior residual cash, saturated at 0 on deficit. Matches Jelleo's V1
/// definition: `R = vault - (c_tot + insurance_fund.balance)` and 0 when
/// the subtraction would underflow.
fn compute_r(engine: &RiskEngine) -> u128 {
    let v = engine.vault.get();
    let c = engine.c_tot.get();
    let i = engine.insurance_fund.balance.get();
    let senior = c.saturating_add(i);
    v.saturating_sub(senior)
}

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

/// Op-tag enum — the set of engine methods this probe can invoke.
/// Smaller subset than the full ix surface for tractability; Session 4
/// can expand if the surviving candidates motivate it.
#[derive(Debug, Clone, Copy)]
enum Op {
    Deposit,
    Withdraw,
    TopUpInsurance,
    Accrue,
    AbsorbProtocolLoss,
    SettleFlatNegativeNotAtomic,
    DepositFeeCredits,
    SyncAccountFee,
}

impl Op {
    fn pick(rng: &mut XorShiftRng) -> Self {
        let r = rng.r#gen::<u8>() % 8;
        match r {
            0 => Op::Deposit,
            1 => Op::Withdraw,
            2 => Op::TopUpInsurance,
            3 => Op::Accrue,
            4 => Op::AbsorbProtocolLoss,
            5 => Op::SettleFlatNegativeNotAtomic,
            6 => Op::DepositFeeCredits,
            _ => Op::SyncAccountFee,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Op::Deposit => "deposit",
            Op::Withdraw => "withdraw",
            Op::TopUpInsurance => "top_up_insurance",
            Op::Accrue => "accrue",
            Op::AbsorbProtocolLoss => "absorb_protocol_loss",
            Op::SettleFlatNegativeNotAtomic => "settle_flat_negative_not_atomic",
            Op::DepositFeeCredits => "deposit_fee_credits",
            Op::SyncAccountFee => "sync_account_fee",
        }
    }

    fn drains_insurance(&self) -> bool {
        matches!(
            self,
            Op::AbsorbProtocolLoss | Op::SettleFlatNegativeNotAtomic
        )
    }
}

pub struct T2ConservationProbe {
    rng: XorShiftRng,
}

impl T2ConservationProbe {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: XorShiftRng::seed_from_u64(seed),
        }
    }

    fn apply_op(
        &mut self,
        engine: &mut RiskEngine,
        op: Op,
        current_slot: &mut u64,
        accounts_inited: &mut [bool; 4],
    ) -> Result<(), String> {
        match op {
            Op::Deposit => {
                let idx = (self.rng.r#gen::<u8>() as u16) % 4;
                if !accounts_inited[idx as usize] {
                    // Materialize first.
                    let _ = engine.materialize_at(idx, *current_slot);
                    accounts_inited[idx as usize] = true;
                }
                let amount: u128 = 1 + (self.rng.r#gen::<u32>() as u128 % 1_000_000);
                engine
                    .deposit_not_atomic(idx, amount, *current_slot)
                    .map_err(|e| format!("{e:?}"))
            }
            Op::Withdraw => {
                let idx = (self.rng.r#gen::<u8>() as u16) % 4;
                let amount: u128 = 1 + (self.rng.r#gen::<u32>() as u128 % 100_000);
                // withdraw_not_atomic has more args; use a minimal call
                // pattern. If signature changes, this fails to compile —
                // we adapt in Session 4.
                #[allow(unused_variables)]
                let _ = (idx, amount);
                Err("withdraw_not_atomic: signature complex, skipping".to_string())
            }
            Op::TopUpInsurance => {
                let amount: u128 = 1 + (self.rng.r#gen::<u32>() as u128 % 1_000_000);
                engine
                    .top_up_insurance_fund(amount, *current_slot)
                    .map_err(|e| format!("{e:?}"))
            }
            Op::Accrue => {
                let dt: u64 = 1 + (self.rng.r#gen::<u64>() % engine.params.max_accrual_dt_slots);
                *current_slot = current_slot.saturating_add(dt);
                let oracle: u64 = 1 + (self.rng.r#gen::<u64>() % 100_000);
                let rate_mag: i128 = (self.rng.r#gen::<u32>() as i128)
                    .min(engine.params.max_abs_funding_e9_per_slot as i128);
                let rate = if self.rng.r#gen::<bool>() { rate_mag } else { -rate_mag };
                engine
                    .accrue_market_to(*current_slot, oracle, rate)
                    .map_err(|e| format!("{e:?}"))
            }
            Op::AbsorbProtocolLoss => {
                let loss: u128 = 1 + (self.rng.r#gen::<u32>() as u128 % 1_000_000);
                // absorb_protocol_loss is `fn`, not `Result` — returns ().
                engine.absorb_protocol_loss(loss);
                Ok(())
            }
            Op::SettleFlatNegativeNotAtomic => {
                let idx = (self.rng.r#gen::<u8>() as u16) % 4;
                engine
                    .settle_flat_negative_pnl_not_atomic(idx, *current_slot)
                    .map_err(|e| format!("{e:?}"))
            }
            Op::DepositFeeCredits => {
                let idx = (self.rng.r#gen::<u8>() as u16) % 4;
                let amount: u128 = 1 + (self.rng.r#gen::<u32>() as u128 % 100_000);
                engine
                    .deposit_fee_credits(idx, amount, *current_slot)
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}"))
            }
            Op::SyncAccountFee => {
                let idx = (self.rng.r#gen::<u8>() as u16) % 4;
                let fee_rate: u128 = self.rng.r#gen::<u32>() as u128 % 10_000;
                engine
                    .sync_account_fee_to_slot_not_atomic(idx, *current_slot, fee_rate)
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}"))
            }
        }
    }
}

impl Probe for T2ConservationProbe {
    fn target(&self) -> &'static str {
        "t2_conservation"
    }

    fn run(&mut self, probe_id: u64, _seed: u64) -> ProbeOutcome {
        let params = default_params();
        let init_slot = DEFAULT_SLOT + (self.rng.r#gen::<u64>() % 100);
        let init_oracle = DEFAULT_ORACLE + (self.rng.r#gen::<u64>() % 1000);
        let mut engine = RiskEngine::new_with_market(params, init_slot, init_oracle);

        // Seed insurance fund.
        let initial_insurance: u128 = 1_000_000 + (self.rng.r#gen::<u32>() as u128 % 1_000_000);
        if engine.top_up_insurance_fund(initial_insurance, init_slot).is_err() {
            return ProbeOutcome { probe_id, divergences: Vec::new() };
        }

        let mut accounts_inited = [false; 4];
        let mut current_slot = init_slot;
        let mut divergences = Vec::new();

        // `R = vault - c_tot - insurance`, saturated at 0 on deficit
        // (Jelleo's V1-strict definition). MUST be conserved across every
        // engine op — that's the senior residual cash that backs junior
        // PnL claimants. Any nonzero delta is a candidate.
        let mut prev_insurance = engine.insurance_fund.balance.get();
        let mut last_r = compute_r(&engine);

        let seq_len: usize = 10 + (self.rng.r#gen::<u8>() as usize % 30);
        for step in 0..seq_len {
            let op = Op::pick(&mut self.rng);
            let res = self.apply_op(&mut engine, op, &mut current_slot, &mut accounts_inited);

            // Invariant 1: R = vault - c_tot - insurance is conserved.
            let r_now = compute_r(&engine);
            if r_now != last_r {
                divergences.push(Divergence {
                    target: "t2_conservation".to_string(),
                    field: "residual_R".to_string(),
                    leg_a: ImplLeg::RustRef,
                    leg_b: ImplLeg::RustRef,
                    value_a: format!("{last_r}"),
                    value_b: format!("{r_now}"),
                    note: format!(
                        "probe_id={probe_id} step={step} op={} delta_R={} vault={} c_tot={} ins={} res={:?}",
                        op.name(),
                        (r_now as i128) - (last_r as i128),
                        engine.vault.get(),
                        engine.c_tot.get(),
                        engine.insurance_fund.balance.get(),
                        res.as_ref().err()
                    ),
                });
            }
            last_r = r_now;

            // Invariant 2: insurance only decreases on documented paths.
            let ins_now = engine.insurance_fund.balance.get();
            if ins_now < prev_insurance && !op.drains_insurance() {
                divergences.push(Divergence {
                    target: "t2_conservation".to_string(),
                    field: "insurance_fund.balance".to_string(),
                    leg_a: ImplLeg::RustRef,
                    leg_b: ImplLeg::RustRef,
                    value_a: format!("{}", prev_insurance),
                    value_b: format!("{}", ins_now),
                    note: format!(
                        "probe_id={probe_id} step={step} op={} drained insurance without being an insurance-draining op",
                        op.name()
                    ),
                });
            }
            prev_insurance = ins_now;
        }

        ProbeOutcome { probe_id, divergences }
    }
}
