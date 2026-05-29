# Percolator Spec Reference — v12.17.0 primitives for Perc5ive implementation

This document is a derivative of `aeyakovenko/percolator/spec.md` v12.17.0 (confirmed verbatim via WebFetch 2026-04-15). Use this as the reference when implementing 5ive DSL types and functions that mirror Percolator's risk engine.

> **⚠️ v16 status (2026-05-28):** upstream has since rewritten the engine — `percolator.rs` (v12.17) was replaced by a monolithic `src/v16.rs` (12,376 lines), **171 commits** ahead of our port reference. v16 is a **multi-asset portfolio** model (per-leg settlement, source-credit IM liens, per-domain insurance budgets, bankruptcy residual ledgers) — a re-port, not a tightening. The perc5ive 9-handler port and any "bit-exact conformance" claim are calibrated to **v12.17** (preserved at clone tag `v12.17-port-ref`); the v16 rebase is in progress. The full per-handler delta analysis and VM-level blockers (no checked-arithmetic opcodes, the mono u256/i256 opcode drop, per-leg iteration primitives) are documented in `docs-internal/V16_REBASE_ANALYSIS.md`.

**Canonical source:** `https://github.com/aeyakovenko/percolator/blob/master/spec.md`
**Verify before building** — re-fetch and diff before any architecture decisions. Anatoly iterates weekly.

---

## Top-level invariants (the things that must NEVER break)

1. **Conservation:** `V >= C_tot + I` always (vault value ≥ total capital + insurance fund)
2. **Protected principal:** zero-position accounts cannot have principal reduced by others' insolvency
3. **ADL eligibility:** explicit protocol-state-driven (not queue-based)
4. **Oracle-manipulation safety:** short-lived price distortions must not yield immediately-withdrawable profit
5. **Reserve strictness:** a fresh reserve MUST NOT inherit time already elapsed on an older scheduled bucket
6. **Fee neutrality:** strict risk-reducing comparisons use actual `fee_equity_impact_i`, not nominal fees
7. **Atomic execution:** every top-level instruction rolls back fully on any failure
8. **Permissionless liveness:** no global scan, canonical order, or manual intervention required

All 39 normative properties in `§0` of Percolator spec must be preserved by any compatible implementation.

---

## Constants (mirror these exactly in 5ive DSL)

```
POS_SCALE        = 1_000_000          // position scaling
ADL_ONE          = 1_000_000_000_000_000  // auto-deleveraging scaling
FUNDING_DEN      = 1_000_000_000      // funding denominator
```

**Integer width rule:** persistent state fits natively into 128-bit boundaries. Wider integers are permitted ONLY for transient intermediates.

---

## State model (per-account)

Each user account stores:
- `C_i` — protected principal (collateral)
- `PNL_i` — realized PnL
- `R_i` — reserved positive PnL (not yet matured)
- `position_basis` — position scale
- `A_i, K_i, F_i` — three snapshot indices
- optional **scheduled** bucket (linear-maturity warmup)
- optional **pending** bucket (holds until promotion to scheduled)

### Two-bucket warmup pattern
- **Scheduled:** positive PnL linearly matures over a fixed window
- **Pending:** holds new positive PnL until promoted to scheduled (prevents inheritance of elapsed time)

This is the critical oracle-manipulation safety mechanism.

---

## Solvency & haircuts

Two distinct haircut lanes (do not conflate):

- **`h`** = matured withdrawal haircut — applies to released profit only
- **`g`** = trade-collateral haircut — applies to all positive PnL supporting risk

**Rule:** aggregate positive PnL admitted through `g` MUST NOT exceed current `Residual`.

---

## A/K/F side-index mechanics (lazy settlement)

Cumulative side indices instead of per-account updates:
- `A_side` — position quantity multiplier (dimensionless)
- `K_side` — cumulative mark-to-market liability
- `F_side_num` — cumulative funding liability

Settlement is lazy: account state is updated on first interaction in a given epoch.

**Epoch-based stale-account reconciliation** enables permissionless reset finalization (no global scan needed).

See Tarun Chitra's autodeleveraging paper [arXiv `2512.01112`](https://arxiv.org/abs/2512.01112) for the academic foundation.

---

## Live finalization lifecycle

Standard flow:
1. **Local touch** — account interacts
2. **Finalize touched accounts** — apply accumulated settlement
3. **Schedule resets** — for stale accounts on opposite side
4. **Finalize resets** — permissionless

Resolved (terminated) accounts follow a separate terminal path with shared payout snapshot capture.

---

## Margin & liquidation — three equity lanes

1. **Withdrawal equity** — matured claims only
2. **Trade-open equity** — counterfactual (after proposed trade)
3. **Maintenance equity** — minimum required to avoid liquidation

**Strict risk-reducing trades** use exact widened buffers and actual fee impact, not nominal fees.

---

## Fees

### Native trading fees
`ceil(notional * bps / 10_000)`

### Liquidation fees
`min(max(raw, min_abs), cap)`

### Optional wrapper fees
Routed through canonical `charge_fee_to_insurance()` helper.

---

## 12 public instruction types

From `§9` of spec — these are the entry points your SPL wrapper must call:

1. `deposit` — user adds collateral
2. `settle` — lazy settlement of account
3. `withdraw` — user removes matured profit
4. `convert` — move between reserve / capital buckets
5. `trade` — open/adjust position
6. `liquidate` — close under-margined account
7. `keeper_crank` — permissionless stale-account advancement
8. `resolve` — terminate an account
9. `force_close_resolved` — force-close after resolution
10. `reclaim` — reclaim dust / residual
11. `charge_fee_to_insurance` — internal helper, may be exposed
12. `open_fee_sink` — internal helper, may be exposed

**Perc5ive 5ive DSL must provide typed stubs for all 12.**

---

## Keeper mode

Off-chain candidate discovery is PERMITTED. The engine validates per-account paths locally WITHOUT global order dependency.

**Implication for Perc5ive:** we can ship a simple keeper bot in TypeScript to crank stale accounts, but it's not required for the hackathon submission to be spec-compliant.

---

## Mandatory test catalog

`§10-11` specifies **55+ mandatory test properties** covering:
- Conservation across all 12 instructions
- Warmup exactness (scheduled + pending promotion)
- State invariants per transition
- Phantom-dust bounds (same-epoch settlement truncation)
- Epoch-lag resolution (stale accounts on opposite side)
- Resolved terminal K deltas
- Max-safe flat conversion

**PercolatorBench implements all 55+ as a public test harness.** Perc5ive passes all of them before submission.

---

## Novel mechanisms (the "what makes Percolator interesting" catalog)

- **Two-bucket warmup** — scheduled (linear maturity) + pending (holds until promotion)
- **Phantom-dust bounds** — tracks same-epoch settlement truncation to prevent hidden inventory
- **Epoch-lag resolution** — stale accounts on opposite side during reset via epoch-gap invariant
- **Resolved terminal K deltas** — final settlement mark stored separately, not into live cumulative K
- **Max-safe flat conversion** — widened comparison prevents liquidation from lossy conversions under `h < 1`

---

## Formal verification with Kani

Anatoly's Rust implementation uses Kani model checker:
```bash
cargo install kani-verifier
cargo kani
```

**Perc5ive can't match Kani directly** (5ive DSL doesn't integrate with Kani). But we can:
1. Ship PercolatorBench's 55+ property tests as our equivalent
2. Formally document which invariants the DSL type system enforces by construction
3. Run Kani against Anatoly's Rust reference implementation as an artifact to show parity awareness

---

## Wrapper pattern — CRITICAL DESIGN CONSTRAINT

> *"Percolator does NOT move tokens — a wrapper program performs SPL transfers and calls into the engine"*

(from HaidarIDK/PERColator README)

**This means Perc5ive implementations are always two programs:**
1. A **wrapper** (written in 5ive DSL) that holds the SPL Token accounts and calls Percolator via CPI
2. The **Percolator engine** (Anatoly's Rust Pinocchio program, deployed by Anatoly or someone else)

Architectural implications for the sprint:
- 5ive DSL must successfully CPI into Percolator's Pinocchio-based program. **This is the Day 1 HELLO_SLAB spike.**
- The wrapper holds all user funds. This is the trust-minimization layer.
- Each Perc5ive market is a separate wrapper program calling the same shared Percolator engine.

---

## Devnet / testnet deployment references

As of 2026-04-15:
- Anatoly's upstream repo has testnet deployment scripts in `/scripts`
- HaidarIDK's web version live at `dex.percolator.site`
- We will deploy our own instances to Solana devnet for Perc5ive testing
- For the final submission, we target devnet with optional mainnet deploy if CPI round-trips clean

---

## Known unknowns (need verification before committing)

1. **Pinocchio calling convention CPI from 5ive DSL** — 5ive currently emits Anchor-compatible CPIs (e.g., `spl_token::transfer`). Pinocchio may require account layout differences. **Day 1 spike.**
2. **Return-data / log handling** — Percolator returns structured state after each instruction. Does 5ive's DSL parse program return data?
3. **Compute budget** — Percolator's 1.4M CU path with multiple CPIs; 5iveVM interpretation overhead is unknown.

---

## Suggested reading order (before implementation)

1. This file (SPEC.md)
2. `COMPETITIVE_LANDSCAPE.md` — what NOT to re-invent
3. `TOLY_CORPUS.md` — quotes + recent signals
4. Anatoly's actual spec at [`aeyakovenko/percolator/blob/master/spec.md`](https://github.com/aeyakovenko/percolator/blob/master/spec.md) — always re-read, it changes
5. Tarun Chitra autodeleveraging paper [arXiv `2512.01112`](https://arxiv.org/abs/2512.01112)
6. `MARKETS/SOV.md`, `MARKETS/PYTH_RACE.md`, `MARKETS/LP_PERP.md`
7. `PERCOLATOR_BENCH.md`

---

## Citations

All content in this file is derived from:
- `github.com/aeyakovenko/percolator` master branch, commit state as of 2026-04-15 (851 commits)
- `github.com/aeyakovenko/percolator/blob/master/spec.md` v12.17.0
- `github.com/aeyakovenko/percolator/blob/master/plan.md`
- `github.com/aeyakovenko/percolator/blob/master/KITCHEN_SINK_TEST.md`
- `github.com/HaidarIDK/PERColator` README
- Public press + Twitter corpus in `TOLY_CORPUS.md`

**When in doubt, re-fetch the upstream spec. The canonical source is Anatoly's repo, not this file.**
