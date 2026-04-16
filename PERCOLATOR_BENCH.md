# PercolatorBench — conformance + adversarial test suite

**Status:** infrastructure co-component of Perc5ive submission. Ships as open-source standalone repo.
**Priority:** co-component (NOT headline) — fuzz/audit cluster has Gecko Fuzz + Excalead as prior prize winners

---

## What PercolatorBench is

An open-source test harness that verifies any Percolator-compatible implementation (or derivative) satisfies the spec's 39 security properties + 55+ mandatory test properties (§0 and §10-11 of Percolator spec v12.17.0).

Runs against:
- Anatoly's upstream Rust implementation (as a self-check artifact we publish)
- Any 5ive DSL Perc5ive market (our own submissions)
- HaidarIDK/PERColator web version
- Any future Percolator-derivative implementation

Output: conformance report per implementation, pass/fail on each property, with diagnostic traces for failures.

## Why it matters strategically

1. **Ecosystem-alignment signal** — we give something back to Percolator rather than extracting
2. **Foundation-grant-eligible** — security infrastructure matches Solana Foundation STRIDE priorities
3. **Social-proof artifact** — "we ran our bench against Anatoly's implementation, 100% pass" is a tweet worth having
4. **Defensive moat** — future Percolator-derivatives run our bench; we become the de-facto conformance authority
5. **Fallback submission** — if markets under-perform, PercolatorBench alone is still a credible infrastructure submission

## Why NOT the headline

Copilot shows direct competitors with prize-winner status:
- **Gecko Fuzz** (Renaissance PRIZE) — decentralized fuzzing via crowd-sourced compute
- **Excalead** (Cypherpunk PRIZE) — automated smart contract audits with AI + formal verification
- SolSab Fuzzer (Cypherpunk) — Trident-based for Sablier
- Wybe (Breakout) — accessible formal verification via Dijkstra predicates

Two prize winners in the fuzz/audit cluster means judges have already crowned this space. Ship PercolatorBench as co-component, not primary pitch.

---

## Architecture

### Components
1. **Property catalog** — JSON manifest of all 39 + 55 properties from Percolator spec v12.17.0
2. **Rust runner** — runs properties against Rust-compiled Percolator implementations using Kani-style symbolic execution + concrete fuzzing
3. **5ive DSL runner** — runs properties against Perc5ive markets using 5ive's native test framework
4. **Adversarial scenario library** — pre-built stress tests (oracle lag, crash cascade, concurrency fuzz, etc.)
5. **Report generator** — Markdown + JSON outputs per implementation
6. **CI integration** — GitHub Action template that any Percolator-fork can drop in

### Repository layout
```
percolator-bench/
├── spec/
│   ├── v12_17_0/
│   │   ├── properties.jsonl        # 39 + 55 properties, machine-readable
│   │   ├── properties.md           # human-readable spec reference
│   │   └── CHANGELOG.md            # track spec version drift
├── runners/
│   ├── rust/
│   │   ├── Cargo.toml
│   │   └── src/                    # symbolic + concrete harness
│   └── five-dsl/
│       ├── five.toml
│       └── src/                    # 5ive-native property runner
├── scenarios/
│   ├── oracle_lag_400ms.v          # Pyth-lag stress
│   ├── crash_cascade.v             # multi-slab liquidation cascade
│   ├── concurrency_fuzz.v          # simultaneous ops on same account
│   ├── insurance_drain.v           # adversarial LP-side attack
│   └── phantom_dust.v              # same-epoch truncation fuzz
├── reports/
│   ├── aeyakovenko-percolator-master-2026-04-15.md  # our self-check artifact
│   └── perc5ive-sov-2026-05-01.md                   # our market's report
├── github-action/
│   └── percolator-bench.yml         # drop-in CI workflow
└── README.md                        # how to use
```

---

## Property catalog (excerpted from Percolator spec v12.17.0)

### §0 — Security goals (39 properties)

Selected highlights:
- **P1.** Conservation: `V >= C_tot + I` always
- **P2.** Protected principal: zero-position accounts cannot have principal reduced by others' insolvency
- **P3.** ADL eligibility: explicit protocol-state-driven
- **P4.** Oracle-manipulation safety: short-lived price distortions cannot yield immediately-withdrawable profit
- **P7.** Reserve strictness: fresh reserve MUST NOT inherit elapsed time from older scheduled bucket
- **P9.** Fee neutrality: strict risk-reducing comparisons use actual `fee_equity_impact_i`, not nominal
- **P14.** Atomic execution: every top-level instruction rolls back fully on failure
- **P22.** Permissionless liveness: no global scan, canonical order, or manual intervention required
- **P27.** Phantom-dust bounds: same-epoch settlement truncation tracked
- **P39.** Max-safe flat conversion: widened comparison prevents liquidation from lossy conversions under `h < 1`

(Full list in `spec/v12_17_0/properties.md`. Every property has a unique ID, a formal predicate, and a test scenario.)

### §10-11 — Mandatory test catalog (55+ properties)

Covers conservation across all 12 instructions, warmup exactness, state invariants per transition.

---

## Adversarial scenario library

### Oracle lag attack
Pyth feed lags 400ms during a 15% price drop. Verify:
- No user can withdraw profit that was only possible due to the lag
- Two-bucket warmup quarantines the profit in `pending` until promotion
- After promotion, withdrawal respects haircut `h`

### Multi-slab liquidation cascade
Three-slab topology with positions on Slab A solvent, Slab B at-margin, Slab C underwater. Cascade Slab C liquidation. Verify:
- Insurance fund absorbs Slab C shortfall correctly
- Slab A positions unaffected
- Slab B doesn't get pulled into cascade unless cross-margin threshold reached
- Conservation `V >= C_tot + I` holds throughout

### Concurrency fuzz
Simultaneous `deposit + trade + liquidate` on same position across 3 consecutive slots. Verify:
- All ops finalize in some serializable order
- Final state matches one of the serial executions
- No double-spend possible

### Insurance drain adversarial
Malicious LP attempts to drain insurance fund by repeated open-close-liquidate cycles. Verify:
- Insurance fund growth is monotonic during normal ops
- Adversary cannot reduce insurance fund below initial seed amount
- Protocol fees route to insurance correctly

### Phantom-dust bounds
Same-epoch settlement truncation leaves sub-POS_SCALE residual. Verify:
- Residual tracked in per-account state
- Not available for withdrawal until next epoch
- Conservation preserved across epoch boundary

---

## Build scope

### Must-ship (for Perc5ive submission)
- [ ] Property catalog file (`spec/v12_17_0/properties.jsonl`) — mechanical translation of spec
- [ ] At least 10 adversarial scenarios (above 5 + 5 more)
- [ ] Rust runner harness working against Anatoly's upstream
- [ ] 5ive DSL runner harness working against Perc5ive markets
- [ ] Report generator producing Markdown output
- [ ] First published report: `aeyakovenko/percolator` master branch conformance
- [ ] First published report: Perc5ive Sov conformance (must pass 100% of core properties or disclose)

### Nice-to-have (post-hackathon)
- [ ] GitHub Action template for drop-in CI
- [ ] Kani integration for the Rust runner (formal verification layer)
- [ ] Live dashboard showing conformance across known Percolator forks

### Out of scope
- Formal verification of 5ive DSL (Kani doesn't target 5ive bytecode)
- Automated fuzzing infrastructure (Gecko Fuzz's space; we stay property-based)
- Security audit services (Excalead's space)

---

## Distribution strategy

- License: **MIT** (more permissive than Percolator's Apache-2.0 to maximize fork adoption)
- Repo name: `percolator-bench` (neutral, not 5ive-branded)
- Maintainership: open to any Percolator-fork contributor
- Governance: loose at first; revisit if ecosystem adoption materializes

Publish the `aeyakovenko/percolator` conformance report on day-of-submission as a social artifact:

> *"We ran PercolatorBench against @aeyakovenko's master branch. 100% pass on conservation, 100% on warmup, 100% on atomicity. Link to full report."*

This makes PercolatorBench look like a validator of Anatoly's work, not a competitor.

---

## Competitive positioning

Gecko Fuzz and Excalead occupy the generalized-fuzzing / automated-audit spaces. PercolatorBench is narrower and more useful:

| Project | What it does | Overlap with PercolatorBench |
|---------|--------------|------------------------------|
| Gecko Fuzz | Decentralized fuzzing infrastructure for any Solana program | HIGH if positioned as "Percolator-fuzzer"; LOW if positioned as property-based conformance |
| Excalead | Automated smart contract audits via AI + formal verification | LOW — different workflow (pre-launch audit vs ongoing conformance) |
| Wybe | Accessible formal verification for smart contracts | LOW — different method (Dijkstra predicates vs property-based testing) |
| SolSab Fuzzer | Trident fuzzer for specific program | LOW — specific to Sablier Lockup |

**Position PercolatorBench as a property-based conformance suite for Percolator derivatives, not a generic fuzzer.** That keeps us out of the Gecko Fuzz lane and into a niche we own.

---

## Risk matrix

| Risk | Severity | Mitigation |
|------|---------|------------|
| Spec version drift — Anatoly ships v13 and invalidates our properties | MEDIUM | Version-pin the spec in `properties.jsonl`; publish new version alongside upstream changes |
| Kani integration for Rust runner too hard in hackathon window | LOW | Use concrete fuzzing + invariant assertions as MVP; Kani is nice-to-have |
| Nobody uses PercolatorBench | LOW-MED | Seed adoption by running against top-5 Percolator forks ourselves and publishing reports |
| Anatoly perceives it as critique | LOW | Open PR to upstream with conformance badge in README; frame as community contribution |

---

## Timeline

- Week 1 (Apr 22-28): property catalog + 5 adversarial scenarios + first upstream report
- Week 2 (Apr 29-May 5): 5ive DSL runner + full 10 scenarios + Sov conformance report
- Week 3 (May 6-10): report generator + documentation + open-source publish
- Week 4 (May 11): submission + tweet thread featuring the Anatoly conformance report

---

## Open questions

1. Should PercolatorBench live under the 5ive org or under a neutral `percolator-bench` org?
2. How do we handle property translation between Rust-semantics and 5ive-DSL-semantics (different primitive types)?
3. Should we publish conformance reports without asking fork authors' permission? (Yes for Anatoly's upstream; YES for HaidarIDK's public fork; ASK for private forks)
4. Can PercolatorBench serve as the basis for a Stride-partner application post-hackathon?

Resolve during Week 1 kickoff.
