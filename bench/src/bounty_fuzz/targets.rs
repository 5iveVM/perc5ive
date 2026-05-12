//! Target stubs for Session 1's prioritized bug-class targets.
//!
//! Each `Target` variant has a stub probe in Session 2 — enough to register
//! the target with the CLI, run a configurable probe count, and emit
//! valid JSONL output. Session 3 replaces these stubs with adversarial
//! probe payloads that actually exercise the corresponding bug class.
//!
//! ## Targets (see `BOUNTY_HUNT_PLAN.md` Session 1 output for the why)
//!
//! - `t1_funding` — `accrue_market_to` cumulative-funding divergence
//! - `t2_conservation` — multi-instruction `V = vault - c_tot - insurance`
//!   invariant under realistic instruction sequences
//! - `t3_margin` — IM/MM via `wide_signed_mul_div_floor_from_k_pair`
//! - `t4_riskbuffer` — wrapper-only RiskBuffer state beyond PR #91's three
//!   root causes (BPF leg only — no DSL analog)
//! - `sanity` — deterministic Rust-only comparison, used to verify the
//!   harness plumbing before any bounty probe is trusted

use std::str::FromStr;

use crate::bounty_fuzz::probe::{Probe, ProbeOutcome};
use crate::bounty_fuzz::sanity::SanityProbe;
use crate::bounty_fuzz::t1_funding::T1FundingProbe;
use crate::bounty_fuzz::t2_conservation::T2ConservationProbe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Sanity,
    T1Funding,
    T2Conservation,
    T3Margin,
    T4RiskBuffer,
}

impl Target {
    pub fn as_str(&self) -> &'static str {
        match self {
            Target::Sanity => "sanity",
            Target::T1Funding => "t1_funding",
            Target::T2Conservation => "t2_conservation",
            Target::T3Margin => "t3_margin",
            Target::T4RiskBuffer => "t4_riskbuffer",
        }
    }
}

impl FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sanity" => Ok(Target::Sanity),
            "t1_funding" | "t1" => Ok(Target::T1Funding),
            "t2_conservation" | "t2" => Ok(Target::T2Conservation),
            "t3_margin" | "t3" => Ok(Target::T3Margin),
            "t4_riskbuffer" | "t4" => Ok(Target::T4RiskBuffer),
            other => Err(format!(
                "unknown target `{}` — valid: sanity, t1_funding, t2_conservation, t3_margin, t4_riskbuffer",
                other
            )),
        }
    }
}

/// Session-2 stub probe: emits zero divergences but reports the target
/// name and probe id so the harness wiring can be exercised. Replaced in
/// Session 3 with the real payload.
struct StubProbe {
    target: &'static str,
}

impl Probe for StubProbe {
    fn target(&self) -> &'static str {
        self.target
    }

    fn run(&mut self, probe_id: u64, _seed: u64) -> ProbeOutcome {
        ProbeOutcome {
            probe_id,
            divergences: Vec::new(),
        }
    }
}

/// Dispatch — given a target, build the right probe, run it `n` times
/// against `seed`, return the outcomes. Stub targets return `n` clean
/// outcomes; `Target::Sanity` runs the real sanity probe.
pub fn run_target(target: Target, n: u64, seed: u64) -> Vec<ProbeOutcome> {
    let mut outcomes = Vec::with_capacity(n as usize);

    match target {
        Target::Sanity => {
            let mut probe = SanityProbe::with_seed(seed);
            for i in 0..n {
                outcomes.push(probe.run(i, seed));
            }
        }
        Target::T1Funding => {
            let mut probe = T1FundingProbe::with_seed(seed);
            for i in 0..n {
                outcomes.push(probe.run(i, seed));
            }
        }
        Target::T2Conservation => {
            let mut probe = T2ConservationProbe::with_seed(seed);
            for i in 0..n {
                outcomes.push(probe.run(i, seed));
            }
        }
        Target::T3Margin | Target::T4RiskBuffer => {
            // T3 has no separate-impl to differ against post-mono (DSL
            // dropped the multiprecision K-pair helper); T4 needs BPF
            // instruction-builders that Session 2 didn't implement. Both
            // remain stubs in Session 3.
            let mut probe = StubProbe {
                target: target.as_str(),
            };
            for i in 0..n {
                outcomes.push(probe.run(i, seed));
            }
        }
    }

    outcomes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_targets_parse() {
        assert_eq!(Target::from_str("sanity"), Ok(Target::Sanity));
        assert_eq!(Target::from_str("t1"), Ok(Target::T1Funding));
        assert_eq!(Target::from_str("t1_funding"), Ok(Target::T1Funding));
        assert_eq!(Target::from_str("t2_conservation"), Ok(Target::T2Conservation));
        assert_eq!(Target::from_str("t3_margin"), Ok(Target::T3Margin));
        assert_eq!(Target::from_str("t4_riskbuffer"), Ok(Target::T4RiskBuffer));
        assert!(Target::from_str("bogus").is_err());
    }

    #[test]
    fn stub_targets_emit_zero_divergences() {
        // T1/T2 are now real probes; T3/T4 stay stubbed pending Session 4.
        for t in [Target::T3Margin, Target::T4RiskBuffer] {
            let outcomes = run_target(t, 100, 0xABCD);
            assert_eq!(outcomes.len(), 100);
            assert!(outcomes.iter().all(|o| o.is_clean()));
        }
    }

    #[test]
    fn sanity_target_runs_1000_with_zero_divergences() {
        let outcomes = run_target(Target::Sanity, 1000, 0xC0FFEE);
        assert_eq!(outcomes.len(), 1000);
        let dirty = outcomes.iter().filter(|o| !o.is_clean()).count();
        assert_eq!(dirty, 0, "sanity probe should be deterministic-clean");
    }
}
