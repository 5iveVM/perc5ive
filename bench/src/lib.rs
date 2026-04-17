//! PercolatorBench — conformance + adversarial test suite.
//!
//! Runs a catalogue of test vectors through both Anatoly Yakovenko's
//! upstream Rust reference (via `hello_slab/percolator`) and the Perc5ive
//! bytecode-ported version, and compares the results bit-for-bit. A
//! divergence in any property is a conformance failure.
//!
//! The suite is deliberately structured so each property is its own
//! function — adding a new check is adding a new test, not threading state
//! through an existing monolith.
//!
//! # What's covered as of 0.1.0
//!
//! * **Arithmetic conformance** (`arithmetic_conformance` module) — every
//!   `wide_math::U256` / `I256` / checked-op in Percolator has a matching
//!   bytecode sequence in `perc5ive::bytecode::{u256, i256, i128}`, and
//!   PercolatorBench runs a Rust reference against the bytecode sequence
//!   for a curated set of probe values.
//!
//! * **Field-access conformance** (`field_access_conformance` module) — the
//!   new sized-field opcodes (`LOAD/STORE_FIELD_{U128,U16}`) round-trip
//!   cleanly through the VM against real `pinocchio::AccountInfo`.
//!
//! * **Handler state-transition conformance** (`handler_conformance`
//!   module) — each ported handler's bytecode produces the same post-state
//!   as calling Percolator's Rust reference with the same pre-state. For
//!   the 0.1.0 cut, this is the simplified subset currently implemented
//!   (top_up, deposit, withdraw, convert_released_pnl, close, settle,
//!   execute_trade, liquidate_at_oracle, keeper_crank).
//!
//! # What's NOT covered
//!
//! * Full 116-property Kani-proof coverage — the 5ive stack has no Kani
//!   yet. We use behavioral conformance (oracle-vs-bytecode) instead.
//! * u256/i256 wide_math ops that the VM doesn't expose yet
//!   (MULDIV_REM_U256, wide_signed_mul_div_floor) — tracked as
//!   deferred-ops in `perc5ive::bytecode::i256`.
//!
//! The suite grows incrementally. Adding a new conformance property is
//! adding a new `#[test]` plus, if needed, a Rust reference in the
//! corresponding perc5ive `bytecode::*` module.

#![allow(clippy::unreadable_literal)]

pub mod anatoly_conformance;
pub mod arithmetic_conformance;
pub mod field_access_conformance;
pub mod handler_conformance;

/// Summary of a conformance run — pass count, fail count, and the list of
/// failing property names.
#[derive(Debug, Clone, Default)]
pub struct ConformanceReport {
    pub passed: Vec<String>,
    pub failed: Vec<(String, String)>, // (name, diagnostic)
}

impl ConformanceReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_pass(&mut self, name: &str) {
        self.passed.push(name.to_string());
    }

    pub fn record_fail(&mut self, name: &str, diagnostic: &str) {
        self.failed
            .push((name.to_string(), diagnostic.to_string()));
    }

    pub fn total(&self) -> usize {
        self.passed.len() + self.failed.len()
    }

    pub fn is_pass(&self) -> bool {
        self.failed.is_empty()
    }

    /// Human-readable summary line.
    pub fn summary(&self) -> String {
        format!(
            "PercolatorBench: {} passed, {} failed out of {}",
            self.passed.len(),
            self.failed.len(),
            self.total()
        )
    }
}
