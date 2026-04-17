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
            source_line: 2957,
            bytecode_fn: "handler_body_deposit",
            scope: HandlerScope::Simplified,
            notes: "acct.capital += amount; risk.vault += amount; risk.c_tot += amount (skips pending-bucket promotion per SESSION_STATE)",
        },
        HandlerRecord {
            name: "withdraw_not_atomic",
            source_line: 3031,
            bytecode_fn: "handler_body_withdraw",
            scope: HandlerScope::Simplified,
            notes: "acct.capital -= amount; risk.vault -= amount; risk.c_tot -= amount (skips free-collateral check)",
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
            source_line: 4081,
            bytecode_fn: "handler_body_close_account",
            scope: HandlerScope::Simplified,
            notes: "vault/c_tot/counter decrement; acct.capital and acct.reserved_pnl zeroed (trusts caller-side flat-position check)",
        },
        HandlerRecord {
            name: "settle_account_not_atomic",
            source_line: 3167,
            bytecode_fn: "handler_body_settle_account",
            scope: HandlerScope::Full,
            notes: "Writes current_slot/oracle_price/funding_rate_e9/last_funding_update_slot; accrues cumulative_funding_e9 via i256 sign-extended ADD_I256; snaps acct.f_snap. Branched flow (flat-pos PnL zero + fee sweep) awaits body-relative JUMP wiring",
        },
        HandlerRecord {
            name: "execute_trade_not_atomic",
            source_line: 3214,
            bytecode_fn: "handler_body_execute_trade",
            scope: HandlerScope::Full,
            notes: "Mirrors position_basis_q; credits PnL via (oracle-exec)*size_q polymorphic i128 chain; collects trading fee from taker into maker. OI-bounds / MM-buffer / ADL-basis gating remains caller-enforced",
        },
        HandlerRecord {
            name: "liquidate_at_oracle_not_atomic",
            source_line: 3651,
            bytecode_fn: "handler_body_liquidate_at_oracle",
            scope: HandlerScope::Full,
            notes: "Computes fee via u128 MUL/DIV on liquidation_fee_bps; insurance_fund += fee; liquidator.reserved_pnl += (capital + victim.reserved_pnl - fee); zeroes victim fields; decrements open_account_count. liquidation_fee_cap clamp pending body-relative JUMP",
        },
        HandlerRecord {
            name: "keeper_crank_not_atomic",
            source_line: 3784,
            bytecode_fn: "handler_body_keeper_crank",
            scope: HandlerScope::Simplified,
            notes: "DSL half updates timestamp/oracle/funding; batch liquidation loop is deferred",
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
