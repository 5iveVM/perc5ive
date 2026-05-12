//! Bounty-hunt differential harness.
//!
//! Three-way differential between (a) `aeyakovenko/percolator` Rust
//! reference, (b) the 5ive DSL port in `perc5ive::bytecode`, (c) the BPF
//! wrapper `aeyakovenko/percolator-prog` loaded via litesvm. A probe is one
//! adversarial input run through every leg; any pairwise divergence in
//! observable state is a candidate finding.
//!
//! Session 2 deliverable: infrastructure only. Real probe payloads are
//! filled in during Session 3 (the hunt). The Session 1 target set
//! (T1–T4) has a stub here so the CLI accepts `--target` for each and the
//! harness is wired end-to-end.

pub mod bpf_runner;
pub mod probe;
pub mod sanity;
pub mod t1_funding;
pub mod t2_conservation;
pub mod targets;

pub use bpf_runner::{BpfRunner, BpfRunnerError};
pub use probe::{Divergence, ImplLeg, Probe, ProbeOutcome};
pub use targets::{Target, run_target};
