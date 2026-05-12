# Percolator Bounty Hunt — autonomous session plan

Five-session plan to convert perc5ive's conformance infrastructure into a working bounty-hunting harness against `aeyakovenko/percolator-prog`. Four autonomous sessions + one human-gated review session before any external PR.

## Context (always-on for every session)

- **Working directory:** `/home/marche/5iveVM/perc5ive`
- **Target repo:** `aeyakovenko/percolator-prog` — Solana program wrapping the formally-verified `aeyakovenko/percolator` risk engine. Deployed immutably to mainnet 2026-05-05 with a bounty challenge.
- **Bounty win condition:** cause `engine.insurance_fund.balance` to decrease via any sequence of public calls. Pyth manipulation and Solana validator attacks out of scope. Everything else (admission bypass, K overflow, ADL math, conservation violation, fee-credits sign flip, etc.) is in scope.
- **Bounty cap:** `bounty_sol_20x_max` spec, `tvl_insurance_cap_mult` recently raised from 20 → 50.
- **Today's date:** check `date` — sessions should treat the percolator-prog HEAD as a moving target.
- **Prior art:** Jelleo audit cycle `20260511-183154` filed 20 findings (10 Critical / 4 High / 6 Medium) on 2026-05-12. PR #91 (2026-05-11) fixed three insurance-drain decoy-lock root causes. PR #48 (2026-04-24) reproduced an insurance vault drainer attack. PR #45 was a paid Claude Code audit. Multiple AI agents are active bounty participants.
- **Our edge:** we have a working 5ive DSL port of percolator v12.17 with bit-exact conformance harness against Anatoly's reference Rust (`bench/src/anatoly_conformance.rs`). Three independent implementations of the same spec — divergences between any pair are bug candidates.

## Hard rules (NEVER cross these without human review)

1. **NEVER `gh pr create --repo aeyakovenko/...`** or any external repo. PRs are human-gated only.
2. **NEVER push to mainnet.** No `solana program deploy --url mainnet-beta`. No mainnet wallet operations of any kind.
3. **NEVER spend SOL** beyond local devnet/localnet that you can airdrop. Devnet wallet has 8.9 SOL, mainnet has 0.
4. **NEVER `git push --force`** or modify shared branch history.
5. **NEVER file a finding without confirming it's not already in:**
   - The Jelleo cycle PDF (`https://jelleo.com/cycles/20260511-183154/`)
   - Any closed PR on `aeyakovenko/percolator-prog`
   - Any open issue on `aeyakovenko/percolator-prog`
6. **NEVER claim a finding without:**
   - A failing PoC test that triggers the violation
   - Either BPF-level reproduction (litesvm or localnet) OR a mechanical code walk citing exact line numbers
   - A diff against the latest `percolator-prog` HEAD that you've confirmed is current
7. **Output findings to `FINDINGS.md` in the perc5ive root**, never to the upstream repo. Human reviews `FINDINGS.md` before any external action.
8. **All session work happens on branch `bounty/<topic>`** off `port/mono`. Create the branch at session start, commit incrementally, never merge to main.

## Launch command

```bash
claude --permission-mode bypassPermissions
```

This enables the autonomous loop — Claude won't pause for tool-approval prompts. The hard rules above are the actual safety boundary; rely on them, not on the permission prompt. Verify before launching: `gh auth status` should show authenticated, `solana config get` should show devnet (not mainnet-beta).

Then paste the Session N prompt below into the new session.

---

## Session 1 — Recon and target selection

### Prompt

```
You are running Session 1 of the perc5ive bounty hunt plan.

Read /home/marche/5iveVM/perc5ive/BOUNTY_HUNT_PLAN.md fully before doing anything else. The "Context" and "Hard rules" sections apply to this session.

Your goal: produce a prioritized list of 3–5 bug-class targets that have the highest chance of yielding a real, novel finding via differential fuzzing between (a) aeyakovenko/percolator (the reference library), (b) perc5ive's 5ive DSL port, (c) aeyakovenko/percolator-prog (the deployed BPF wrapper).

Concrete steps:

1. Pull latest state:
   - `cd hello_slab/percolator && git pull origin master` (the reference library)
   - `gh repo clone aeyakovenko/percolator-prog hello_slab/percolator-prog` if not present, else `cd hello_slab/percolator-prog && git pull origin master`
   - Record both HEAD shas in your branch.

2. Download and read the Jelleo audit PDF at https://github.com/Copenhagen0x/audit-pipeline-cli/releases/download/cycle-20260511-183154/jelleo-percolator-cycle-20260511-183154.pdf (use WebFetch). Extract the 20 findings — store the title, severity, bug class, and a one-line summary per finding in a local file `hello_slab/jelleo_findings.md`. We need this so we don't re-submit anything they already filed.

3. List every closed PR on aeyakovenko/percolator-prog from 2026-04-15 onward via `gh pr list --repo aeyakovenko/percolator-prog --state closed --limit 100 --json number,title,closedAt,body`. Save to `hello_slab/prog_pr_history.json`. These are bugs that were already found and fixed — don't re-find them.

4. Read the percolator-prog README, security.md, and src/percolator.rs structure (file listing only — don't read the full src yet). Understand the instruction surface (DepositCollateral, WithdrawCollateral, TradeNoCpi, TradeCpi, CloseAccount, etc.) and the matcher CPI model.

5. Analyze which bug classes are highest-EV for differential fuzzing given our toolkit. Our 5ive DSL port is at percolator v12.17. The deployed wrapper is at v12.18.x+. We have a working u128 + i128 conformance harness but no u256 path (mono dropped those opcodes). Promising classes:
   - Arithmetic divergences (overflow, signed bounds, MIN/MAX edges) — bench/anatoly_conformance.rs already does u8²/i8² coverage for primitive ops, can extend to compute_trade_pnl, funding accrual, haircut math.
   - State conservation invariants (vault vs counters) — see Jelleo H1, V1, V7, V26 patterns.
   - Permissionless trigger surface — instructions that reach privileged code paths without admin signer.
   - DO NOT pick: anything Jelleo or prior PRs already cover. Be paranoid about novelty.

6. Write `BOUNTY_HUNT_PLAN.md`'s "Session 1 output" section: ranked list of 3-5 targets with:
   - Bug class name
   - Why it's high-EV (what divergence we expect, why others might have missed it)
   - Confirmation that it's not in Jelleo's 20 nor in closed PRs
   - Approximate effort to build a fuzzer for it (S/M/L)
   - Expected severity if found

7. Commit your work to a new branch `bounty/recon` off `port/mono`. Push to origin (5iveVM/perc5ive — our repo, not Toly's).

8. End with a one-paragraph summary of what to do in Session 2.

Hard rules apply. Do not file any external PR. Do not spend SOL. Stop and ask for clarification only if a hard rule would otherwise be crossed; otherwise work autonomously to completion.
```

### Exit criteria

- `hello_slab/jelleo_findings.md` exists and lists all 20 Jelleo findings
- `hello_slab/prog_pr_history.json` exists with closed PR data
- `BOUNTY_HUNT_PLAN.md` has a populated "Session 1 output" section
- Branch `bounty/recon` pushed to origin

### Session 1 output

**Run date:** 2026-05-12 (autonomous, branch `bounty/recon`)

**Recorded HEAD shas:**
- `aeyakovenko/percolator` (engine, reference library): `6cd742f25a9bcebeb9adc01136b129b5996397dd` — matches the engine sha that Jelleo audited (`6cd742f25a`)
- `aeyakovenko/percolator-prog` (BPF wrapper): `ba667e8c68b4dbc4ebf47740bccf59a9aa1ec6a8` — diverges from the wrapper sha Jelleo audited (`873ac13042`); revalidate diff in Session 4
- Wrapper's `Cargo.toml` engine pin: `1dc4466e1a6c3532f2781bc242fa4e4033751fb6`. `git log 1dc4466e..6cd742f` yields exactly one cosmetic commit (`Record full proof strength audit`) → **the deployed BPF binary's engine is functionally identical to Jelleo's audit target**, so every Jelleo finding that hits engine code is live on mainnet *and* already disclosed.

**Captured artefacts:**
- `hello_slab/jelleo_findings.md` — 20 findings with bug-class clustering and coverage-gap notes.
- `hello_slab/prog_pr_history.json` — 48 closed PRs on `aeyakovenko/percolator-prog`.

**Deploy / patch timeline that matters for novelty:**
- Mainnet immutable deploy: **2026-05-05**.
- Post-deploy PRs that are NOT on mainnet: `#80` (Anchor v2 port), `#87` GetAccountHealth, `#88` UpdateAccountOwner, `#91` risk-buffer 3-root-cause fix.
- PR `#39` (F7 self-trade insurance teleport) and PR `#40` (F8 permissionless_stale_matured gate missing on 6 ix) are public-disclosure PRs documenting live mainnet bugs — already public, not novel.
- PR `#91` documents three independent risk-buffer decoy-lock root causes; the deployed binary still has them. They are disclosed but unfixed on mainnet → any *fourth* root cause in the same class would be in-scope novel.

**Toolkit honesty check:**
- Our 5ive DSL port is locked at percolator v12.17. The wrapper-pinned engine `1dc4466e` is also recent (parent of master); both are effectively v12.18.x. A v12.17→v12.18 spec gap will look like a divergence in our harness — disqualifier check needs the percolator changelog before each candidate.
- `bench/src/anatoly_conformance.rs` already gives u8²/i8² primitive-op coverage. Extending to specific helper functions (compute_trade_pnl, accrue_market_to, K-pair fixed-point) is incremental work, not green-field.
- No u256 path in mono. Any helper that ever widens to u256 internally can't be DSL-side; we must compare BPF↔Rust-reference only for those. Drop u256 paths from the DSL leg of the differential.

#### Prioritized targets

Ranked by expected value = (novelty × insurance-fund reachability × probability our harness produces signal). Bug class, why-EV, novelty argument, fuzzer build effort, expected severity if confirmed.

##### T1 — `accrue_market_to` cumulative-funding divergence  *(Effort: M, Sev: M–H)*

- **Bug class:** Mixed-scale fixed-point divergence in `accrue_market_to(accrual_slot, oracle_price, funding_rate_e9)`, specifically the `cum_funding_long_e18` / `cum_funding_short_e18` splits under signed `funding_rate_e9` and asymmetric OI (`oi_eff_long_q != oi_eff_short_q`). Adversarial inputs: max `dt`, near-MIN/MAX rate, OI ratio near 0/∞.
- **Why high-EV:** Jelleo's `T1` only tested same-tx bundle cap, and `V26` covered `compute_trade_pnl` floor asymmetry, but no PoC adversarially fuzzes `accrue_market_to` itself. The helper runs on every market-touching instruction (TradeCpi, TradeNoCpi, KeeperCrank, UpdateMarkPrice, LiquidateAtOracle, settles, withdraws), so a 1-LSB rounding bias compounds over thousands of calls. If the split between `cum_funding_long_e18` and `cum_funding_short_e18` doesn't sum back to the funding payment within rounding tolerance, the OI counters drift, and the V invariant (`vault - c_tot - insurance`) eventually breaks — same family as Jelleo's H1/V1, *different code path*.
- **Novelty confirmation:** Not in Jelleo's 20 (compute_trade_pnl ≠ accrue_market_to). Not in closed PRs (`PERC-150`, `PERC-241`, `PERC-365`, `PERC-622`, `PERC-637`, `PERC-8206` cover other surfaces; PR `#14` is ADL; PR `#15` is hyperp staleness; PR `#74` is matcher abi). No PR title mentions funding-rate fuzzing.
- **Why ours catches it:** DSL port's i128/u128 ops are bit-exact vs Rust reference for `mul_div_floor` at u8²/i8². Extending the existing `anatoly_conformance.rs` pattern to chain N `accrue_market_to` calls and compare the full `(cum_funding_long_e18, cum_funding_short_e18, last_market_slot, oi_eff_*)` tuple is mechanical.
- **Severity if confirmed:** Medium standalone (accumulated drift, not single-call drain), High if the drift can be steered to debit insurance via a follow-up `settle_flat_negative_pnl_not_atomic` — i.e. composed with the F02/F09 permissionless surface that's already disclosed but unfixed on mainnet.

##### T2 — Multi-instruction conservation sweep (V invariant under realistic instruction sequences)  *(Effort: M, Sev: C)*

- **Bug class:** Cross-instruction state divergence — `V = vault - c_tot - insurance_fund.balance` (and the strict residual `R`) tracked across sequences of 20-50 instructions, not single helper calls.
- **Why high-EV:** Every Jelleo CRITICAL Layer-2 PoC invokes one engine helper directly (`engine.absorb_protocol_loss(...)`, `engine.settle_flat_negative_pnl_not_atomic(...)`). They prove the helper is buggy in isolation. They do NOT search for *legitimate-looking* multi-instruction sequences that compose three patched-looking helpers into a still-buggy net effect. Bounty win condition is `engine.insurance_fund.balance` *decrease via any sequence of public calls* — sequences are explicitly the win surface.
- **Novelty confirmation:** No closed PR titled around "sequence", "composition", or "scenario fuzz". Kani harnesses (PR `#31` `PERC-8206`, `#11` `PERC-241`) are single-function. Adversarial regression PR `#79` adds three hand-written scenarios; nothing systematic.
- **Why ours catches it:** Our DSL port can replay an arbitrary instruction sequence and the Rust reference engine can replay the same, so V is computable on both. A sequence generator that randomly chooses public ix tags (DepositCollateral, TradeNoCpi, TradeCpi-with-mock-matcher, LiquidateAtOracle, KeeperCrank, settle_*, WithdrawCollateral, ResolvePermissionless, AdminForceClose, etc.) with random valid args, runs N steps, asserts V_monotone after every step on both impls. A sequence that produces V_after < V_before on either impl is a candidate.
- **Severity if confirmed:** Critical — this is the bounty win condition with a clean PoC.

##### T3 — IM/MM math via `wide_signed_mul_div_floor_from_k_pair` boundary fuzzing  *(Effort: M, Sev: H)*

- **Bug class:** Initial-margin / maintenance-margin fixed-point divergence. `account_health_snapshot` returns `(eq_raw, mm_req, im_req, above_mm)` via the K-pair helper that fuses K and F into a single floor-rounding mul-div. PR `#87` (post-deploy, not on mainnet) made this CPI-callable, but the helper has been in the engine longer.
- **Why high-EV:** Margin checks gate every trade. A 1-lamport underestimate of `mm_req` at a boundary lets a trade through that should have been rejected; the position then sits below MM, and on liquidation the deficit goes to `absorb_protocol_loss` → insurance debit (with V-invariant unmet because of the *separate* H1/V7 family, but also detectable as a stand-alone differential between DSL ↔ Rust ↔ BPF).
- **Novelty confirmation:** V26 targets `compute_trade_pnl`, a different helper. Jelleo's Page-2 TOC has no `mm_req` / `im_req` / `K-pair` entries. Kani PRs `#11`, `#31` are listed by property, none of them is margin-math. `account_health_snapshot` was added 2026-05-10 (PR `#87`) — its mainnet status is "not deployed" but the underlying helper IS deployed.
- **Why ours catches it:** Symmetric to T1 — extend `anatoly_conformance.rs` with adversarial (K, F, scale) inputs spanning the K-pair domain. DSL port has the same fixed-point op set as Rust at u128/i128 (verified for u8²/i8²); compute and compare.
- **Severity if confirmed:** High — under-margin position is a clean attack chain (open at boundary, wait for adverse move, liquidate → engine eats deficit from insurance).

##### T4 — RiskBuffer / generation-table state corruption beyond PR #91's three root causes  *(Effort: L, Sev: C)*

- **Bug class:** Wrapper-only state machine bugs in `RiskBuffer::upsert / evict / append_phase2_fullclose_candidates` and `gen_table` (account materialization generation). PR `#91`'s body explicitly enumerates **three** root causes (equal-notional displacement guard `<=` vs `<`, zero-position FullClose admission, structural RR window gated on non-empty candidates). The PR title says "close three root causes" but does not claim *all* root causes are closed.
- **Why high-EV:** The deployed mainnet binary has all three PR `#91` bugs unfixed and disclosed. The bounty rewards *novel* bugs only. The natural place to look is the same code paths PR `#91` touched but with adversarial inputs that don't trigger any of the three fixed conditions — e.g., `RiskBuffer` LRU eviction when an attacker rotates many near-min-notional accounts to force the victim out via timing; `lp_account_id` collision after `gen_table` reuse across `ReclaimEmptyAccount` + `InitUser`; or the `combined` candidate-list dedupe being bypassable by tag-aliasing.
- **Novelty confirmation:** Not in Jelleo's 20 (all engine-direct, RiskBuffer is wrapper-only and Jelleo's Layer-4 BPF probes were specific to disclosed engine paths). PR `#91` claims to close THE root causes named in its body — not the class.
- **Why ours catches it:** This one needs a litesvm leg in the differential (RiskBuffer state has no DSL analog). The Session 2 harness will need a `BpfRunner` that surfaces RiskBuffer slots; Session 3 fuzzes admission/eviction sequences.
- **Severity if confirmed:** Critical — RiskBuffer decoy-lock is the documented insurance-drain pattern from PR `#48` and the disclosure thread; a novel root cause in the same class is a direct bounty win.

#### Disqualified targets (recorded so we don't relitigate)

- TradeCpi `FLAG_PARTIAL_OK` regressions — PR `#74`'s 4-state matrix is the regression; matcher abi flag fuzzing remains *interesting* but EV-dominated by T4.
- Permissionless surface enumeration beyond F02/F08 — PR `#40` (F8) systematically catalogues the gap; no obvious surface left untouched.
- Oracle binding (Jelleo F13) — covered.
- Anchor-v2 port bugs (PR `#80`, not on mainnet) — out of scope (not deployed).
- u256-only paths (multiprecision opcodes) — toolkit gap: our DSL port can't speak u256, so any divergence we'd see is "DSL doesn't support u256" not a real bug. Drop u256 from the DSL leg of the differential.

### Session 2 handoff — one-paragraph summary

Build `bench/src/bpf_runner.rs` on `litesvm` 0.1 (already a dev-dep on percolator-prog; confirm version drift before pinning), then a `Probe` struct that runs an input through all three legs (Rust-reference engine `aeyakovenko/percolator`, our 5ive DSL port via the existing `anatoly_conformance` harness, BPF via `BpfRunner`). Build probe generators in target priority order: **(1)** chained `accrue_market_to` sequences with `(dt, oracle_price, funding_rate_e9, oi_eff_long_q, oi_eff_short_q)` adversarial inputs comparing `(cum_funding_long_e18, cum_funding_short_e18, oi_eff_*)` — DSL+Rust legs only since accrual is engine-internal; **(2)** a multi-instruction sequence fuzzer over public ix tags asserting V/R monotonicity per step, all three legs; **(3)** K-pair IM/MM domain fuzzing comparing `(eq_raw, mm_req, im_req)`, DSL+Rust legs; **(4)** RiskBuffer admission/eviction sequences via litesvm only (no DSL analog), checking that no admission sequence locks a victim with notional above the current `min_notional`. Sanity gate: 1000 probes against the already-passing u8²/i8² conformance must emit zero divergences before any target probe is trusted. Document the harness invocation in `bench/README.md` and emit JSONL divergence records into `bench/fuzz_results/<timestamp>.jsonl`. Do NOT hunt in Session 2 — purely build the infrastructure so Session 3 can run for compute hours unattended.

---

## Session 2 — Build the differential harness

### Prompt

```
You are running Session 2 of the perc5ive bounty hunt plan.

Read /home/marche/5iveVM/perc5ive/BOUNTY_HUNT_PLAN.md fully — especially the "Session 1 output" section which has the prioritized bug-class targets.

Your goal: extend bench/src/anatoly_conformance.rs into a three-way differential harness that runs the same inputs through (a) aeyakovenko/percolator reference library, (b) perc5ive's 5ive DSL port, (c) aeyakovenko/percolator-prog deployed BPF wrapper via litesvm. Output divergences in a machine-parseable format.

Concrete steps:

1. Create branch `bounty/harness` off `port/mono`. Read and understand the existing `bench/src/anatoly_conformance.rs` — it already does (a) ↔ (b) for primitive arithmetic.

2. Add a litesvm-based runner. Add `litesvm = "0.6"` (check latest on crates.io via WebFetch if needed) to bench/Cargo.toml dev-dependencies. Build the percolator-prog .so from hello_slab/percolator-prog with `cargo build-sbf` and cache the artifact path. Write a thin loader: `BpfRunner::new(so_path)` → has methods to construct percolator-prog instructions, invoke them in litesvm, and parse the slab account state out.

3. Write the comparison core: `Probe { inputs: ProbeInputs, ref_output: RefOutput, dsl_output: DslOutput, bpf_output: BpfOutput, divergences: Vec<Divergence> }`. The divergence is the unit of bug-candidate output. Each Divergence carries: which-pair-diverged, field that differs, ref-value, observed-value, probe inputs.

4. Build probes for the Session 1 targets. Start with the highest-EV class. For each probe:
   - Construct inputs that exercise the target code path on all three impls
   - Run all three, capture state deltas (vault balance, insurance_fund.balance, account fields)
   - Compare. Any divergence > rounding-tolerance is a candidate.

5. Add a `cargo run --bin bounty_fuzz -- --target <class> --probes <n>` entrypoint that emits candidate divergences as JSON to `bench/fuzz_results/<timestamp>.jsonl`.

6. Sanity check: run 1000 probes against arithmetic conformance (which already passes in anatoly_conformance.rs). Zero divergences expected. If any show up, debug your harness — it's a false-positive bug in the comparison, not a real finding.

7. Document the harness in bench/README.md so a future session (or human) can run it cold.

8. Commit on `bounty/harness`, push to origin. Update BOUNTY_HUNT_PLAN.md's "Session 2 output" section: file paths, how to invoke, sanity-check results.

Do NOT actually hunt bugs in this session. Session 2 is purely infrastructure. Session 3 is the hunt.

Hard rules apply. Do not file any external PR. Do not spend SOL. Stop and ask for clarification only if a hard rule would otherwise be crossed; otherwise work autonomously to completion.
```

### Exit criteria

- `bench/src/bpf_runner.rs` exists and loads percolator-prog.so via litesvm
- `cargo run --bin bounty_fuzz -- --target <class>` runs without panicking
- Sanity-check probe (1000 runs against passing conformance) emits zero divergences
- Branch `bounty/harness` pushed

---

## Session 3 — Hunt

### Prompt

```
You are running Session 3 of the perc5ive bounty hunt plan.

Read /home/marche/5iveVM/perc5ive/BOUNTY_HUNT_PLAN.md fully — especially Session 1 output (targets) and Session 2 output (harness usage).

Your goal: run the differential harness against the Session 1 targets and produce a list of candidate divergences in `bench/fuzz_results/`. No validation yet — just collect candidates.

Concrete steps:

1. Create branch `bounty/hunt` off `bounty/harness`.

2. For each Session 1 target, in priority order:
   - Run `cargo run --release --bin bounty_fuzz -- --target <class> --probes 50000` (adjust probe count for compute time; aim for ~1-2h per target).
   - Collect divergences. Save JSON output.
   - Triage: cluster divergences by (which-pair-diverged, field, root-cause-hypothesis). Many divergences usually trace to a single root cause; collapse them.

3. For each unique cluster, write a one-paragraph hypothesis in `CANDIDATES.md`:
   - What inputs trigger the divergence
   - Which two implementations diverge
   - What field/state differs
   - First guess at root cause (be honest if you don't know)

4. Quick disqualification pass per candidate:
   - Does the divergence trace to a known v12.17 → v12.19 spec change? (Our 5ive port is v12.17, percolator-prog is v12.18.x+.) If yes, this is "outdated port" not a bug. Disqualify.
   - Is the divergence within documented rounding tolerance? Disqualify.
   - Does it touch only DSL-port internal state that doesn't map to wrapper behavior? Disqualify.
   - Does it match any Jelleo finding from hello_slab/jelleo_findings.md? Disqualify.
   - Does it match any closed PR from hello_slab/prog_pr_history.json? Disqualify.

5. After triage, you should have a smaller list of surviving candidates in `CANDIDATES.md`. These go to Session 4 for validation.

6. If zero candidates survive, that's a valid outcome. Document why each was disqualified. Update BOUNTY_HUNT_PLAN.md "Session 3 output" — propose either (a) different bug class targets for a Session 3 redo, or (b) acknowledge the harness has found everything Jelleo already found and we should pivot.

7. Commit `bounty/hunt` branch, push to origin.

Hard rules apply. Do not file any external PR. Do not spend SOL. Especially: do NOT submit any "candidate" finding externally — these are unvalidated. Stop and ask for clarification only if a hard rule would otherwise be crossed; otherwise work autonomously to completion.
```

### Exit criteria

- `bench/fuzz_results/` populated with JSONL per target
- `CANDIDATES.md` with surviving candidates after triage
- Each surviving candidate has been checked against Jelleo + closed PRs
- Branch `bounty/hunt` pushed

---

## Session 4 — Validate (paranoid mode)

### Prompt

```
You are running Session 4 of the perc5ive bounty hunt plan.

Read /home/marche/5iveVM/perc5ive/BOUNTY_HUNT_PLAN.md fully. Especially read CANDIDATES.md from Session 3.

This is the highest-stakes session. We are about to invest time validating findings that — if real — go to Toly's repo. If we're wrong, we burn reputation. Be paranoid. False positives are far worse than missed positives.

Your goal: for each surviving candidate from Session 3, either (a) confirm it as a real finding with 4-layer evidence matching Jelleo's standard, or (b) disqualify it with a documented reason.

Concrete steps:

1. Create branch `bounty/validate` off `bounty/hunt`.

2. For each candidate, do ALL of the following — in order, gating on success:

   **Layer 1 — code walk.** Find the exact lines in percolator-prog/src/percolator.rs (or the percolator engine crate) that the divergence implicates. Cite line numbers at the current HEAD sha. If the lines don't exist or the code path the divergence implies doesn't match what you see in code, DISQUALIFY — your harness had a false positive.

   **Layer 2 — PoC test.** Write a failing test in `tests/bounty_pocs/<candidate_name>.rs` (in our perc5ive repo, NOT in percolator-prog). The test must compile, must run, must demonstrate the violation. If you can't make the violation reproducible in a clean test, DISQUALIFY.

   **Layer 3 — BPF reproduction.** Same scenario as Layer 2 but invoked via litesvm against the actual percolator-prog .so. The on-chain state must show the violation post-execution. If the wrapper defends at the BPF layer even though the engine is buggy, mark as "wrapper-defended" (still potentially worth filing but lower-severity). If the BPF layer doesn't reproduce at all, DISQUALIFY — engine-only bugs that don't reach the wrapper aren't bounty-eligible.

   **Layer 4 — proof or mechanical argument.** Either (a) write a Kani harness if you can, OR (b) write a mechanical argument that walks the code line-by-line proving the invariant violation. Save to `FINDINGS/<candidate>/proof.md`.

3. After all 4 layers pass for a candidate, write `FINDINGS/<candidate>/writeup.md` following Jelleo's format:
   - Title (sev + bug class + 6-word summary)
   - Description (what the bug is)
   - Impact (concrete attack sequence: who, with what permissions, gains what)
   - Root cause (the specific line/invariant violated)
   - PoC (link to the test file)
   - BPF reproduction (link to the litesvm test)
   - Proposed patch (diff against percolator-prog HEAD, minimum surface area)
   - Verification gates (pre-patch PoC fails, post-patch PoC passes, no new public surface)

4. For each candidate, run this disqualifier checklist a SECOND time (you already ran it in Session 3 — do it again with fresh eyes because validated findings are higher-stakes):
   - Already in Jelleo's 20 findings?
   - Already fixed in any closed PR (re-pull HEAD and re-check)?
   - Trace to a v12.17→v12.19 spec change rather than a real bug?
   - Within documented rounding tolerance?
   If ANY answer is yes, disqualify and document.

5. Update FINDINGS.md (top-level, in perc5ive root) with a summary table:
   | # | Sev | Bug class | Status | PoC | BPF repro | Disqualifier check |
   
   Confirmed findings → "READY FOR HUMAN REVIEW". Disqualified → reason. Unable to validate → "NEEDS MORE WORK" with what's blocking.

6. STOP HERE. Do NOT proceed to file any external PR. Do NOT contact Toly. Do NOT post to any public channel. The next session (Session 5) is human-driven.

7. Commit `bounty/validate`, push to origin.

Hard rules apply. Do not file any external PR. Do not spend SOL. Stop and ask for clarification before any irreversible action.

Final guidance: if you're unsure whether a finding is real, it's not real enough to file. The threshold is "I would bet my reputation on this." If you wouldn't, mark NEEDS MORE WORK and let the human decide.
```

### Exit criteria

- For each candidate from Session 3: a verdict (READY / DISQUALIFIED / NEEDS MORE WORK)
- Confirmed findings have all 4 evidence layers in `FINDINGS/<candidate>/`
- `FINDINGS.md` summary table is current
- Branch `bounty/validate` pushed
- NO external action taken

---

## Session 5 — Human review and submission (NOT autonomous)

This session is run by you, the human, not by Claude. Claude's job is done; this is your call.

For each finding marked READY in `FINDINGS.md`:

1. **Read the writeup yourself end-to-end.** Does the impact claim match what the PoC shows? Is the root cause specific enough that a reasonable reader could implement the patch from the description?

2. **Run the PoC test yourself.** `cargo test --test bounty_pocs <candidate>`. Confirm it fails on current HEAD, passes on patched HEAD.

3. **Re-check the disqualifier list.** Look at `aeyakovenko/percolator-prog` issues + closed PRs *one more time* — anything filed in the last 24h since Session 4? If yes, you've been scooped.

4. **Decide submission style:**
   - **Private disclosure first** if the bug is exploitable on the live mainnet program with non-trivial impact. Email or DM, give Toly time to patch before public disclosure.
   - **Public PR** if the bug is in a class Toly's already publicly discussing (the audit PRs are all public) and the impact is bounded.
   - **Issue first** if you want to confirm scope before opening a PR.

5. **Draft the submission.** Use the writeup as the body. Attribute appropriately. Include the PoC test in the PR or as an attachment.

6. **Then and only then:** `gh pr create --repo aeyakovenko/percolator-prog ...` or `gh issue create --repo aeyakovenko/percolator-prog ...`.

7. **Track the response.** Update `FINDINGS.md` with submission link + outcome.

If no findings reach READY: the harness still exists and can be re-run as percolator-prog evolves. Each new release is another chance. Don't force a submission just because you ran the cycle.

---

## Notes on safety

- The autonomous mode flag `--permission-mode bypassPermissions` removes per-tool prompts but does NOT change the hard rules in this document. Claude is instructed to refuse mainnet/external-PR actions regardless of permission mode.
- If a session hits an unclear hard-rule situation, it should stop and emit a `BLOCKED.md` file describing what it was about to do and what permission it needs. Do not assume.
- The differential harness is read-only against external code. We never modify percolator-prog locally except in our own fork (if we make one) for testing patches. We don't push to upstream.
- All branches stay on `5iveVM/perc5ive` (our repo). Nothing on `aeyakovenko/*` until Session 5 human-gated submission.
