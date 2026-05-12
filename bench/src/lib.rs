//! PercolatorBench — conformance + adversarial test suite.
//!
//! Runs a catalogue of test vectors through both Anatoly Yakovenko's
//! upstream Rust reference (via `hello_slab/percolator`) and the Perc5ive
//! bytecode-ported version, and compares the results bit-for-bit. A
//! divergence in any property is a conformance failure.
//!
//! # Modules
//!
//! * `bounty_fuzz` — three-way differential harness against the deployed
//!   percolator-prog BPF wrapper. Built in bounty hunt Session 2.
//!
//! * `field_access_conformance`, `handler_conformance` — documentation
//!   modules tracking what the pre-mono port covered. The actual
//!   round-trip tests live in the top-level `perc5ive/tests/` directory.
//!
//! * `anatoly_conformance`, `arithmetic_conformance` (legacy, behind
//!   `legacy_u256` feature) — pre-mono u256/i256/i128 bytecode conformance
//!   tests. The mono port dropped those multiprecision opcodes; these
//!   modules are retained for historical reference and re-enabled only if
//!   the multiprecision DSL surface is re-introduced.

#![allow(clippy::unreadable_literal)]

pub mod bounty_fuzz;
pub mod field_access_conformance;
pub mod handler_conformance;

#[cfg(feature = "legacy_u256")]
pub mod anatoly_conformance;
#[cfg(feature = "legacy_u256")]
pub mod arithmetic_conformance;

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
