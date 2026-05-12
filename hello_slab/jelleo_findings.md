# Jelleo audit cycle `20260511-183154` — 20 findings

Audit metadata (Page 1):
- Auditor: Kirill Sakharuk / Jelleo (`kirill@jelleo.com`)
- Customer: `percolator-live`
- Engine SHA audited: `6cd742f25a` (= current `aeyakovenko/percolator` master HEAD as of 2026-05-12)
- Wrapper SHA audited: `873ac13042` (a percolator-prog commit; we have `ba667e8c` locally — diff to check during validation)
- Generated: 2026-05-12T08:53:23+00:00
- Distribution: 10 Critical / 4 High / 6 Medium / 0 Low / 0 Info
- Hypothesis IDs run from `741` (H1) to `758` (U30) in Jelleo's internal numbering.

The audit attaches Layer-2 PoC (engine-direct Rust test), Layer-3 (Kani symbolic verification), Layer-4 (on-chain BPF reproduction via litesvm), and a 5-gate verifier for the proposed patch. **All 20 findings are bounty-class evidence and must be considered out-of-bounds for novelty in Sessions 2–4.**

Pattern summary up front (bug-class clustering):

| Bug class | Findings |
| --- | --- |
| `insurance-counter-vault-divergence` (use_insurance_buffer credits insurance without debiting vault) | 1, 3, 4, 8, 9 |
| `vault-balance-divergence` (V = vault - c_tot - insurance not conserved) | 5, 6 |
| `insurance-buffer-permissionless-trigger` (insurance reachable without admin) | 2 |
| `haircut-direction-violation` (haircut grows residual instead of shrinking) | 7, 11 |
| `cross-instruction-state-race` (per-tx accrual cap bypassable by bundling) | 10 |
| `resolved-state-pnl-leak` (settle / close after resolve under-distributes) | 12, 16 |
| `oracle-substitution` (per-leg `feed_id` not enforced on supplied oracle) | 13 |
| `signed-overflow-trade-pnl` (compute_trade_pnl returns `i128::MIN` or asymmetric floor) | 14 |
| `arithmetic-overflow-pnl-mark` (saturating arithmetic uses `u128::MAX` not protocol cap) | 15 |
| `resolved-fee-accumulation-skip` (KeeperCrank in Resolved mode skips fee sync) | 17 |
| `init-state-resolve-bypass` (resolve permissionless on empty/under-init market) | 18 |
| `zero-push-after-stale-readmission` (cheap mark push resets staleness window) | 19 |
| `zero-debt-zero-effect-side-channel` (zero-debt no-op still advances `current_slot`) | 20 |

The single dominant class (12/20 findings) is the **insurance/vault residual conservation family**, all rooted in `use_insurance_buffer` (engine `src/percolator.rs:4827`) decrementing `insurance_fund.balance` without an equal-magnitude debit of `vault`. Jelleo's recommended fix is a 2-line vault.saturating_sub(pay) inside that function.

---

## Detailed findings (numbered per Jelleo's Page-2 TOC)

### F01 — CRITICAL — `H1-residual-conservation` *(insurance-counter-vault-divergence)*
> The post-haircut residual cash on a market (`vault - cash_locked_in_orderbook - claimable_pnl - insurance_counter`) is conserved by every internal accounting helper.
PoC: `h1_residual_conservation_fires`. BPF reproduction ✓. Patch: vault.saturating_sub in `use_insurance_buffer`.

### F02 — CRITICAL — `H5-permissionless-trigger-surface` *(insurance-buffer-permissionless-trigger)*
> Every public/permissionless instruction that reaches `use_insurance_buffer` requires either an admin signer OR cannot drain the insurance pool below its initial f[unded amount].
PoC: `h5_permissionless_trigger_surface_fires` calling `settle_flat_negative_pnl_not_atomic(0,0)`. BPF reproduction ✓.

### F03 — CRITICAL — `PD4-residual-conservation` *(insurance-counter-vault-divergence)*
> The post-haircut residual cash on a market (vault - cash_locked_in_orderbook - claimable_pnl - insurance_counter) is conserved by every internal accounting helper.
PoC: `pd4_residual_conservation_fires`. BPF reproduction ✓ — concrete trace (`vault_before=1_000_000 → vault_after=70_010_000_200`, `ins_before=1_000_000 → ins_after=1_000_002`, `c_tot_before=0 → c_tot_after=6_931_741_118`) shows residual grew from 0 to `6_825_908_080` after a single `absorb_protocol_loss` call.

### F04 — CRITICAL — `V1-residual-conservation-strict` *(insurance-counter-vault-divergence)*
> Define `R = vault - c_tot - insurance_fund.balance` (the senior residual, saturated at 0 on deficit). For every public `_not_atomic` entrypoint E and every reachable [state], R is non-decreasing.
PoC: `v1_residual_conservation_strict_fires` — `settle_flat_negative_pnl_not_atomic` path with `pnl=-200`, capital=0, insurance=500. R_before=500, R_after=700.

### F05 — CRITICAL — `V1-vault-residual-conservation` *(vault-balance-divergence)*
> The post-haircut residual cash (vault - cash_locked_in_orderbook - claimable_pnl - insurance_counter) is conserved across every internal accounting helper.
PoC: `v1_vault_residual_conservation_fires`. BPF reproduction ✓.

### F06 — CRITICAL — `V2-vault-balance-equation` *(vault-balance-divergence)*
> For every market state transition, the change in vault balance equals the sum of (cash deposited into orderbook + claimable_pnl_credited + insurance_counter_credited).
PoC: `v2_vault_balance_equation_fires` — Jelleo notes Layer-4 wrapper-side defenses caught it. Jelleo's recommended patch still failed post-patch (PoC still reproduced) — likely needs orthogonal fix.

### F07 — CRITICAL — `V5-haircut-direction` *(haircut-direction-violation)*
> The haircut (positive-PnL claim cap) only ever shrinks claimable PnL, never increases the residual cash that other claimants can pull.
PoC: `v5_haircut_direction_fires`. Patch line — drop the `vault.saturating_sub(pay)` recommendation that other findings ADD. (Internally inconsistent across findings: F05/F06 want vault debit on `use_insurance_buffer`; F07 wants it removed. Audit acknowledges this conflict.)

### F08 — CRITICAL — `V7-insurance-counter-vault-coupling` *(insurance-counter-vault-divergence)*
> Every code path that mutates `insurance_fund.balance` is paired with an equal-magnitude mutation of `vault` in the same direction (credit insurance ⇒ credit vault, debit insurance ⇒ debit vault).
PoC: `v7_insurance_counter_vault_coupling_fires` — same `absorb_protocol_loss` path, different invariant lens.

### F09 — CRITICAL — `SH6-resolve-flat-negative-gate` *(insurance-counter-vault-divergence)*
> `resolve_flat_negative` (engine `src/percolator.rs:4770-4785`) is reached only via `touch_account_live_local` step 7 (`src/percolator.rs:4822-4848`) when `eff_p[nl] < 0`.
PoC: `sh6_resolve_flat_negative_gate_fires` — `settle_flat_negative_pnl_not_atomic(idx, 0)` reaches `resolve_flat_negative` without the canonical live-touch gate. BPF: **not reproduced** (wrapper-side defenses caught it).

### F10 — CRITICAL — `T1-hyperp-mark-cpi-bundled-trade` *(cross-instruction-state-race)*
> A1b test runs PushHyperpMark in isolation. Does NOT test transaction containing PushHyperpMark immediately FOLLOWED by TradeCpi in SAME tx. Mark-smoothing cap applies per-call, so two `accrue_market_to` calls in one bundle exceed the per-slot cap.
PoC: `t1_hyperp_mark_cpi_bundled_trade_fires` — chains two accrue_market_to dt=1 ops, price moves 2× cap. BPF: not reproduced (test setup didn't reach buggy state). Patch failed gate (PoC still failed post-patch).

### F11 — HIGH — `PD9-haircut-direction-monotonic` *(haircut-direction-violation)*
> The haircut (positive-PnL claim cap) only ever shrinks claimable PnL, never increases. No code path lets the hai[rcut numerator grow].
PoC: `pd9_haircut_direction_monotonic_fires` — `pnl_residual` grew from 0 to 178317 after `absorb_protocol_loss` with insurance dropping by 0. BPF reproduction ✓.

### F12 — HIGH — `S3-settle-after-close` *(resolved-state-pnl-leak)*
> `settle_after_close` correctly distributes final residual to each account proportional to its claim, respecting the haircut.
PoC: `s3_settle_after_close_fires`. Note: Layer-2 PoC PASSED on unpatched repo — Jelleo flags either PoC malformed or repo already has the fix. Worth re-verifying against deployed wrapper.

### F13 — HIGH — `P2-oracle-account-binding` *(oracle-substitution)*
> Each oracle account passed to a Percolator instruction is validated against the per-leg `feed_id` recorded in `MarketConfig` (`oracle_leg_feed_id`). A spoofed P[yth oracle account passes].
PoC: `p2_oracle_account_binding_fires` — KeeperCrank with spoofed oracle account (`feed_id=[0xCC;32]`) succeeded on-chain without validating against `MarketConfig.oracle_leg_feed_id=[0xAB;32]`. BPF ✓.

### F14 — HIGH — `V26-compute-trade-pnl-no-i128-min` *(signed-overflow-trade-pnl)*
> `compute_trade_pnl(size_q, price_diff)` for `size_q in (0, MAX_TRADE_SIZE_Q]` and `price_diff in [-(MAX_ORACLE_PRICE as i128), MAX_ORACLE_PRICE as i128]` always returns a finite, non-`i128::MIN` value.
PoC: `v26_compute_trade_pnl_no_i128_min_fires` — `compute_trade_pnl(s,d) + compute_trade_pnl(-s,d) != 0` (floor-rounding asymmetry); or `i128::MIN` produces residual drift from 0 to 9097, insurance drops 1_000_002 → 1_004_560 (by 0). BPF ✓.

### F15 — MEDIUM — `AR7-saturating-arithmetic-correctness` *(arithmetic-overflow-pnl-mark)*
> Where the codebase uses saturating arithmetic, the saturation point is the documented protocol cap, not a primitive type's max.
PoC: `ar7_saturating_arithmetic_correctness_fires` — `record_uninsured_protocol_loss` saturates at `u128::MAX` (~3.4e38) instead of `MAX_VAULT_TVL = 10^16`. BPF ✓.

### F16 — MEDIUM — `CI10-resolution-final` *(resolved-state-pnl-leak)*
> Once a market is resolved and all matured claims are paid, the market account can be safely closed with no residual debt.
PoC: `ci10_resolution_final_fires`. BPF ✓.

### F17 — MEDIUM — `U20-resolvedcrank-early-return-skips-recurring-fees` *(resolved-fee-accumulation-skip)*
> KeeperCrank in resolved mode at `src/percolator.rs:6902-6921` returns `Ok(())` immediately after the resolved-context check WITHOUT calling `sync_account_fee_to_slot`. Therefore `last_fee_slot` does not advance.
PoC: `u20_resolvedcrank_early_return_skips_recurring_fees_fires`. BPF: no row (likely not run / not reproducible BPF-side).

### F18 — MEDIUM — `U21-permissionless-resolve-bypass-engine-init-check` *(init-state-resolve-bypass)*
> `ResolvePermissionless` at `src/percolator.rs:10039-10125` calls slab_guard + require_initialized + market_mode != Resolved + then checks `engine.last_oracle_price != 0` (init signal). Path reachable on empty market with `num_used_accounts=0`, `oi_eff_long_q=0`, `oi_eff_short_q=0`.
PoC: `u21_permissionless_resolve_bypass_engine_init_check_fires`. BPF: no row.

### F19 — MEDIUM — `U29-hyperp-mark-push-zero-after-stale-allowed` *(zero-push-after-stale-readmission)*
> `PushHyperpMark` at `src/percolator.rs:9146-9147` rejects `price_e6 == 0` (after `Clock::get` and stale check). But the stale check at 9113 only rejects if permissionle[ss]; admin can repeat cheap same-price mark pushes to indefinitely reset `last_market_slot`, blocking `ResolvePermissionless` recovery.
PoC: `u29_hyperp_mark_push_zero_after_stale_allowed_fires`. BPF ✓ — `last_market_slot` reset from 100 to 299 (delta=199 slots). Patch failed gate (PoC still failed post-patch — patch incomplete).

### F20 — MEDIUM — `U30-deposit-fee-credits-zero-debt-after-sync-still-succeeds` *(zero-debt-zero-effect-side-channel)*
> `DepositFeeCredits` at `src/percolator.rs:9843-9933` reads debt at 9906 (`fc < 0 ? fc.unsigned_abs() : 0`). If `sync_account_fee_bounded_to_market` at 9901 fully retire[s the debt] then `pay=0` and the function still mutates `self.current_slot = now_slot` and returns Ok — a zero-effect side-channel that advances the slot anchor.
PoC: `u30_deposit_fee_credits_zero_debt_after_sync_still_succeeds_fires`. Kani: inconclusive (timeout/OOM). BPF: no row.

---

## Coverage gaps Jelleo's pipeline did NOT touch (raw notes for Session 1 target selection)

These are dimensions Jelleo's per-call PoCs and per-function Kani harnesses do **not** cover, derived by reading every Layer-2 test and noting common limitations:

1. **Sequenced (multi-instruction, multi-slot) state evolution.** Every PoC tests a single helper invocation (`absorb_protocol_loss`, `settle_flat_negative_pnl_not_atomic`, `compute_trade_pnl`, `accrue_market_to`). No PoC chains realistic instruction sequences (deposit → multiple trades → partial fill → funding tick → liquidate → ADL → settle → close) and asserts the residual/conservation invariants at every step.
2. **`accrue_market_to` adversarial input fuzzing** beyond the T1 same-tx bundle. The funding-rate accrual function takes `(accrual_slot, oracle_price, funding_rate_e9)` and mutates `cum_funding_long_e18 / cum_funding_short_e18 / oi_eff_*` via mixed e9/e18/q14 fixed-point. Only the cross-instruction cap bypass is tested (T1), not boundary inputs (max dt, signed-rate edges, OI imbalance).
3. **`account_health_snapshot` / `wide_signed_mul_div_floor_from_k_pair`**. The K/F-pair fixed-point used for IM/MM is exercised by V26 indirectly via `compute_trade_pnl` but never targeted directly. `account_health_snapshot` was added in **post-deploy** PR #87 — but the underlying helper has been there longer.
4. **Wrapper-only state.** `RiskBuffer` upsert/eviction (PR #91 closed 3 root causes — but PR #91 is post-deploy, so the deployed binary still has those bugs; any **fourth** root cause not in PR #91 would still be novel). Generation table / `mat_counter` reuse across slot reinit. Wrapper-cached `last_effective_price_e6` divergence from engine `last_oracle_price`.
5. **TradeCpi matcher abi semantic edge cases beyond the 4-state regression** in PR #74 (partial/full/zero × with/without FLAG_PARTIAL_OK). Combinations such as `exec_size > req_size`, sign-flipped exec_price, or matcher self-reentrancy via a malicious matcher program aren't fuzzed in the existing regression.
6. **ADL distribution math** (haircut spread proportionally across all positive-PnL accounts). V5/PD9/V7 test single-account residual; no PoC sums haircut shares across many accounts to verify the total matches the absorbed loss within rounding tolerance.

These gaps form the basis of the Session 1 target list in `BOUNTY_HUNT_PLAN.md`.
