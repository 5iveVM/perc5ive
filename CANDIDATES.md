# Session 3 candidate divergences

Hunt date: 2026-05-12. Branch: `bounty/hunt`. Harness: `bench/src/bounty_fuzz/` from Session 2.

## Run summary

| Target | Probes | Seed | Divergences | Dirty probes | Output |
| --- | --- | --- | --- | --- | --- |
| `sanity` | 1000 | 0xC0FFEE | 0 | 0 | sanity check passed |
| `t1_funding` | 1000 + 1000 | 42 + 7919 | 0 | 0 | `bench/fuzz_results/*_t1_funding.jsonl` (overwritten by 2nd run, same name template) |
| `t2_conservation` | 50000 | 1 | **143_435** | 46_502 | `bench/fuzz_results/1778622662_t2_conservation.jsonl` |
| `t3_margin` | (stub) | — | 0 | 0 | stub probe, no impl post-mono |
| `t4_riskbuffer` | (stub) | — | 0 | 0 | stub probe, BPF builders deferred |

## T2 divergence cluster analysis

Single cluster covers 100% of T2 output:

- **Op that triggered the divergence:** `op=absorb_protocol_loss` for **all 143_435 / 143_435** divergences (100%).
  - `op=settle_flat_negative_pnl_not_atomic`: 0 — error path before the buggy helper is reached (random `idx` + slot args don't satisfy the entry guards).
  - All other ops (`deposit`, `top_up_insurance`, `accrue`, `deposit_fee_credits`, `sync_account_fee`, `withdraw` (skipped due to complex signature)): 0 divergences.
- **Field:** `residual_R` (= `vault - c_tot - insurance_fund.balance`, saturated at 0).
- **Direction:** `delta_R > 0` in every observed case — residual grew after `absorb_protocol_loss`.
- **State trace:** `vault` unchanged across the offending step, `insurance_fund.balance` dropped by `delta_R`, `c_tot` unchanged. Numeric match for Jelleo F03 PD4 trace ("absorb_protocol_loss reduced insurance_fund.balance without debiting engine.vault by the same amount").

### Hypothesis (one paragraph)

The engine helper `use_insurance_buffer` at `hello_slab/percolator/src/percolator.rs:4827-4836` decrements `self.insurance_fund.balance` by `pay = min(loss, ins_bal)` and returns `loss - pay`, but does NOT subtract `pay` from `self.vault`. Every public-or-`test_visible` path that calls `absorb_protocol_loss` — and therefore `use_insurance_buffer` — inflates the senior residual `R` by exactly `pay`. The bug class is `insurance-counter-vault-divergence`. All 12 of Jelleo's findings F01, F03, F04, F05, F06, F08 (and to a lesser extent F02, F07, F09, F11) are different invariant-lens views of the same single bug. Jelleo's proposed fix is a 2-line `self.vault = U128::new(vault.get().saturating_sub(pay))` immediately after the `insurance_fund.balance` write.

## T1 divergence cluster

None. 2000 probes of chained `accrue_market_to(slot, oracle, rate)` with adversarial dt + sign + magnitude inputs produced zero `residual_R` deltas and zero insurance decreases. `accrue_market_to` is residual-conservative in the engine — the funding payment math touches OI counters (`oi_eff_long_q`, `oi_eff_short_q`), the cumulative funding accumulators (`cum_funding_long_e18`, `cum_funding_short_e18`), and the `last_market_slot` anchor, but does not directly touch `vault`, `c_tot`, or `insurance_fund.balance`. This matches the spec — funding settles at touch time, not at accrue time, so `R` is invariant under `accrue_market_to`.

The negative result is honest but expected: Jelleo's T1 finding (F10, `T1-hyperp-mark-cpi-bundled-trade`) was a **cross-instruction state race** — same-tx bundle of PushHyperpMark + TradeCpi exceeds the per-slot mark cap. My probe chains `accrue_market_to` calls sequentially with fresh dt each time, so the per-call cap correctly limits each step. To reach Jelleo F10 we'd need to simulate two `accrue_market_to(slot=S, ...)` calls with the SAME `S` after the bundled-ix path advances `last_market_slot` — that's Session 4 work if we choose to revisit T1.

## Disqualification pass (per Hard rule §5)

Apply the 5 Session 3 disqualification filters to the single surviving cluster:

| Filter | Result |
| --- | --- |
| Traces to a v12.17 → v12.19 spec change? | No — bug is in engine code, not a spec-version gap. |
| Within documented rounding tolerance? | No — `delta_R` ranges from hundreds to millions of base units, well above any rounding bound. |
| Touches only DSL-port internal state? | No — divergence is engine-direct, no DSL leg in play. |
| Matches a Jelleo finding from `hello_slab/jelleo_findings.md`? | **Yes**: F01 (`H1-residual-conservation`), F03 (`PD4-residual-conservation`), F04 (`V1-residual-conservation-strict`), F05 (`V1-vault-residual-conservation`), F06 (`V2-vault-balance-equation`), F08 (`V7-insurance-counter-vault-coupling`) all describe the same `use_insurance_buffer` debit-without-vault-credit pattern. Disqualified on Jelleo overlap. |
| Matches a closed PR in `hello_slab/prog_pr_history.json`? | Partial overlap — PR #1 (2026-02-12) fixed the *same pattern* in `WithdrawInsurance`; PR #39 (F7) and PR #48 (insurance vault drainer) document live mainnet variants. The specific `use_insurance_buffer` path is Jelleo's coverage. |

**Verdict: disqualified.** Zero novel candidates surface from this hunt.

## Surviving candidates

**None.** Every divergence the harness produced traces to a Jelleo-documented root cause.

## What this tells us

The conservation-family bug Jelleo enumerated is the dominant pathology in the engine. Single-call PoCs (Jelleo's pattern) already exhaust the engine-direct surface for this bug class. Multi-call sequence fuzzing rediscovers the same root cause through different op orderings but doesn't reveal a *new* invariant violation, because every helper that touches insurance routes through the same `use_insurance_buffer`.

The plan's Session 1 hypothesis that "sequences would find what per-call PoCs missed" is empirically wrong for T2 against an engine where the bug is in a single deeply-shared helper.

## Proposed pivots (for the Session 3 redo OR Session 4 prep)

Two viable directions, ranked by EV given what we now know:

### Pivot A: Wrapper-only state hunting (T4 RiskBuffer, BPF leg)

The deployed mainnet BPF binary as of 2026-05-05 still has the three PR #91 risk-buffer root causes unfixed (PR #91 landed 2026-05-11). Wrapper-only state (`RiskBuffer::upsert`, `gen_table`, `lp_account_id` reuse) has no engine analog and isn't covered by Jelleo's engine-direct PoCs. To hunt this:

1. Build BPF instruction-builders in `bench/src/bounty_fuzz/bpf_runner.rs` for the 4-5 instructions PR #91 touched (KeeperCrank, DepositCollateral, the candidate-bearing crank paths).
2. Construct sequences of RiskBuffer admission/eviction probes via litesvm.
3. Check the invariant PR #91's body articulates: "no admission sequence locks a victim with notional above the current `min_notional`."

Effort: L (Session 4-sized, not Session 3 redo). EV: H — this is the only target where novelty is structurally plausible.

### Pivot B: Cross-instruction state races (T1-extension)

Jelleo F10 (`T1-hyperp-mark-cpi-bundled-trade`) shows the cross-instruction-state-race class is reachable in this engine. Their PoC tests PushHyperpMark + TradeCpi in one tx. The class probably has more instances: any two ops whose pre-state validation reads cached state could race if the cached state is mutated between them in the same tx. Enumerate the (op_a, op_b) pairs where op_a mutates a cache op_b reads, and check whether two-step sequences exceed any documented per-tx bound.

Effort: M. EV: M — the bug class exists, but Jelleo specifically targeted the mark-cap variant; other variants might already be wrapper-defended by the same `validate_*` helpers PR #56 added.

### Pivot C: Acknowledge the harness is complete and stop hunting

The deployed binary's engine is functionally Jelleo's audit target, and Jelleo enumerated 20 findings spanning every engine-direct bug class. Our toolkit gave us no new view into the engine that Jelleo lacked. The honest outcome is to bank Session 1+2 as infrastructure (`bench/src/bounty_fuzz/` is now a reusable harness for future percolator releases) and not file a bounty submission against the current deployment.

Effort: 0. EV: 0 immediate, retains future option value.

## Recommended next step (one line)

**Pivot A** if there is appetite for Session 4 BPF infrastructure work; **Pivot C** if the cycle has run its course. **Pivot B** is the middle option — moderate work, uncertain payoff. The Session 4 prompt as written assumes "validate the surviving candidates" — given there are none, Session 4 in its current form is a no-op and should be replaced by the chosen pivot before the next session is run.
