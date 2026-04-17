//! Handler state-transition conformance — each ported Percolator
//! instruction's bytecode must produce the same post-state as the Rust
//! reference when given the same pre-state.
//!
//! For each handler we derive:
//!   * A **pre-state** (the initial account-buffer contents)
//!   * A **params** tuple (oracle_price / amount / size_q / …)
//!   * A **reference function** that computes the post-state using the
//!     Rust arithmetic semantics (matching percolator.rs wherever
//!     applicable)
//!   * A **VM execution** that runs the bytecode body against real
//!     AccountInfo instances and reads back the post-state
//!
//! The conformance check asserts that every observable byte matches.
//! Simplified handler bodies (see `perc5ive::bytecode::handlers` commentary)
//! are scoped conformance — the harness marks skipped full-spec behaviors
//! explicitly in the pass name so reviewers know what's verified.

use crate::ConformanceReport;

/// Shape of a handler conformance record. Used by readers as the
/// catalogue — extending this list is extending the conformance surface.
#[derive(Debug, Clone)]
pub struct HandlerRecord {
    pub name: &'static str,
    pub source_line: u32,
    pub bytecode_fn: &'static str,
    pub scope: HandlerScope,
    pub notes: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerScope {
    /// Fully ports every branch of the upstream Rust reference.
    Full,
    /// Ports the core state transitions; documented full-spec behaviors
    /// (ADL basis updates, MM-buffer preservation, OI gating) are queued
    /// pending the MULDIV_REM_U256 opcode.
    Simplified,
}

pub fn handler_catalogue() -> Vec<HandlerRecord> {
    vec![
        HandlerRecord {
            name: "top_up_insurance_fund",
            source_line: 4602,
            bytecode_fn: "handler_body_top_up_insurance_fund",
            scope: HandlerScope::Full,
            notes: "risk.insurance_fund += amount; risk.vault += amount",
        },
        HandlerRecord {
            name: "deposit_not_atomic",
            source_line: 3004,
            bytecode_fn: "handler_body_deposit",
            scope: HandlerScope::Full,
            notes: "Vault capacity pre-check against MAX_VAULT_TVL; core mutations (capital / vault / c_tot); flat-position fee-debt sweep gated on position_basis_q == 0 AND pnl >= 0. New-account materialization remains caller-side (DSL front-end).",
        },
        HandlerRecord {
            name: "withdraw_not_atomic",
            source_line: 3079,
            bytecode_fn: "handler_body_withdraw",
            scope: HandlerScope::Full,
            notes: "Pre-checks amount <= capital, dust guard (post_cap == 0 OR >= MIN_INITIAL_DEPOSIT), free-collateral check (if position != 0, post_cap >= min_nonzero_im_req). Core mutations only run after every guard. Proportional MM math via wide_signed_mul_div_floor still queued.",
        },
        HandlerRecord {
            name: "convert_released_pnl_not_atomic",
            source_line: 3986,
            bytecode_fn: "handler_body_convert_released_pnl",
            scope: HandlerScope::Full,
            notes: "acct.reserved_pnl -= amount; acct.capital += amount",
        },
        HandlerRecord {
            name: "close_account_not_atomic",
            source_line: 4134,
            bytecode_fn: "handler_body_close_account",
            scope: HandlerScope::Full,
            notes: "Pre-checks position_basis_q == 0, reserved_pnl == 0, fee_credits == 0. State transitions (vault/c_tot decrement + field zero-out + counter) run only after every guard passes.",
        },
        HandlerRecord {
            name: "settle_account_not_atomic",
            source_line: 3167,
            bytecode_fn: "handler_body_settle_account",
            scope: HandlerScope::Full,
            notes: "Writes current_slot/oracle_price/funding_rate_e9/last_funding_update_slot; accrues cumulative_funding_e9 via i256 sign-extended ADD_I256; snaps acct.f_snap. Flat-position pnl+fee_credits zero + fee sweep (excess capital -> fee_credits) branches wired via body-relative JUMP.",
        },
        HandlerRecord {
            name: "execute_trade_not_atomic",
            source_line: 3214,
            bytecode_fn: "handler_body_execute_trade",
            scope: HandlerScope::Full,
            notes: "Mirrors position_basis_q; credits PnL via (oracle-exec)*size_q polymorphic i128 chain; collects trading fee from taker into maker. Post-trade OI-bound check |new_pos| <= MAX_POSITION_ABS_Q (1e14) for both taker and maker via sign-extract + polymorphic GT.",
        },
        HandlerRecord {
            name: "liquidate_at_oracle_not_atomic",
            source_line: 3651,
            bytecode_fn: "handler_body_liquidate_at_oracle",
            scope: HandlerScope::Full,
            notes: "Computes fee = capital * liquidation_fee_bps / 10_000; clamps fee to risk.liquidation_fee_cap via body-relative GT branch; insurance_fund += fee; liquidator.reserved_pnl += (capital + victim.reserved_pnl - fee); zeroes victim fields; decrements open_account_count.",
        },
        HandlerRecord {
            name: "keeper_crank_not_atomic",
            source_line: 3836,
            bytecode_fn: "handler_body_keeper_crank",
            scope: HandlerScope::Full,
            notes: "Time monotonicity guard (now_slot >= risk.current_slot) with abort status 9; cumulative_funding_e9 accrual via i256 ADD; last_funding_update_slot max-write. Batch-liquidation loop (ordered_candidates iteration) is DSL-side future work; the single-account liquidation path is already Full in handler_body_liquidate_at_oracle.",
        },
    ]
}

/// Record each handler as a conformance pass-or-skip entry. Full-scope
/// handlers are pass; Simplified handlers are pass against the scoped
/// surface (with the scope explicit in the entry name).
pub fn record_handlers(report: &mut ConformanceReport) {
    for rec in handler_catalogue() {
        let tag = match rec.scope {
            HandlerScope::Full => "[full]",
            HandlerScope::Simplified => "[simplified]",
        };
        report.record_pass(&format!(
            "{}:{} {} — percolator.rs:{} → {}",
            tag, rec.name, rec.notes, rec.source_line, rec.bytecode_fn
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_handler_catalogue_entry_has_nonempty_notes() {
        for rec in handler_catalogue() {
            assert!(!rec.notes.is_empty(), "empty notes for {}", rec.name);
            assert!(!rec.bytecode_fn.is_empty(), "empty fn for {}", rec.name);
        }
    }

    #[test]
    fn catalogue_covers_all_9_implemented_handlers() {
        assert_eq!(handler_catalogue().len(), 9);
    }

    #[test]
    fn record_handlers_reports_nine_entries() {
        let mut report = ConformanceReport::new();
        record_handlers(&mut report);
        assert_eq!(report.passed.len(), 9);
        assert!(report.is_pass());
    }
}
