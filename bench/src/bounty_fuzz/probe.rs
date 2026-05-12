//! Probe / Divergence types — the unit of bug-candidate output.
//!
//! A `Probe` carries the input vector and the recorded output from each
//! implementation leg. A `Divergence` is a pairwise mismatch on a named
//! observable. Probes serialize to JSON lines (one probe per line) so a
//! long fuzzing run streams to `bench/fuzz_results/<timestamp>.jsonl`
//! without buffering, and downstream triage scripts read line-by-line.

use serde::{Deserialize, Serialize};

/// Which implementation produced a value. Used in `Divergence` to name the
/// pair that disagreed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImplLeg {
    /// `aeyakovenko/percolator` Rust reference library.
    RustRef,
    /// 5ive DSL port (`perc5ive`).
    Dsl,
    /// `aeyakovenko/percolator-prog` BPF wrapper via litesvm.
    Bpf,
}

impl ImplLeg {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImplLeg::RustRef => "rust_ref",
            ImplLeg::Dsl => "dsl",
            ImplLeg::Bpf => "bpf",
        }
    }
}

/// A single divergence — one observable field disagreeing between two
/// legs on a specific probe input. Stringly-typed so each target
/// generator can stamp its own field names without forcing a closed
/// observable enum on the harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Divergence {
    /// Stable target name, e.g. `t1_funding_accrual`.
    pub target: String,
    /// Field name within the target — `cum_funding_long_e18`, `mm_req`, etc.
    pub field: String,
    /// First leg in the pairwise comparison.
    pub leg_a: ImplLeg,
    /// Second leg in the pairwise comparison.
    pub leg_b: ImplLeg,
    /// String-serialized values so we can carry u128 / i128 / nested types
    /// without committing to a single numeric representation in the JSONL.
    pub value_a: String,
    pub value_b: String,
    /// Free-form note from the target generator (e.g. the input seed, the
    /// instruction sequence, or the K-pair coordinates).
    pub note: String,
}

/// Outcome of running one probe across one target — either "all legs agreed"
/// or "here are the divergences".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeOutcome {
    pub probe_id: u64,
    pub divergences: Vec<Divergence>,
}

impl ProbeOutcome {
    pub fn is_clean(&self) -> bool {
        self.divergences.is_empty()
    }
}

/// A `Probe` is the per-target trait object — each target implements
/// `Probe::run` to produce a `ProbeOutcome` for one input id. The harness
/// orchestrates the loop; targets supply the input generation + comparison
/// logic.
pub trait Probe {
    /// Stable target identifier — must match `--target <name>` on the CLI.
    fn target(&self) -> &'static str;

    /// Generate one probe at index `i` and return the divergences. The seed
    /// is the harness-wide PRNG seed; targets can mix in `i` to produce
    /// reproducible per-probe input vectors.
    fn run(&mut self, probe_id: u64, seed: u64) -> ProbeOutcome;
}
