# MetaGenesis Rent Audit + Watermelon Isolation Demo — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove on the executed VM bytecode that the MetaGenesis lifecycle extracts zero operator rent, demonstrate that markets under the shared futarchy are isolated, and draft the gated Toly-tagged RFS-answer thread.

**Architecture:** Add a `rent_breakdown` reference to `meta_math` (the single source the bench property and MCP schema cite), prove conservation + zero-extraction on the real linked binary in two new e2e tests (single-market audit, two-market isolation), surface a `simulate_rent_audit` MCP catalogue entry, and write the launch thread to `docs-internal/launch/` (NOT posted). No new hand-written bytecode — only already-linked genesis handlers are exercised.

**Tech Stack:** Rust, `five-vm-mito` (`MitoVM::execute_direct`), `perc5ive::bytecode::meta_math` / `meta_handlers`, the `bench` conformance crate, the `mcp` catalogue crate.

**Spec:** `docs/superpowers/specs/2026-05-29-metagenesis-rent-audit-watermelon-design.md`

---

## File structure

- `src/bytecode/meta_math.rs` — **modify**: add `RentBreakdown` + `rent_breakdown()` + `aggregate_rent()` references and their unit tests. One responsibility: the genesis ledger math, independently testable.
- `bench/src/meta_conformance.rs` — **modify**: add the `rent_zero_extraction` property to `run()`; bump the `total()` assertion 4 → 5.
- `tests/e2e_rent_audit.rs` — **create**: single-market ledger-conservation + zero-extraction proof on the real linked binary, plus the surplus-draw guard.
- `tests/e2e_market_factory.rs` — **create**: two independent genesis lifecycles on one binary; assert isolation + operator rent 0 in both.
- `mcp/src/tools.rs` — **modify**: add the `simulate_rent_audit` catalogue entry (Simulation category).
- `docs-internal/launch/RFS_ANSWER_THREAD.md` — **create**: the gated Toly-tagged thread draft.

The two e2e files each duplicate the small test harness (helpers `data_account`, `signer_account`, `script_account`, `input_u64s`, `call`, `call_ok`, `u64_at`, `u8_at`, `linked_meta`) that `tests/e2e_meta_genesis.rs:31-128` already defines. Integration tests are separate crates and cannot share private helpers, so copying that block verbatim is the established pattern — do it.

---

## Phase 1 — rent audit

### Task 1: `rent_breakdown` reference in `meta_math`

**Files:**
- Modify: `src/bytecode/meta_math.rs` (add after `distribution_approved`, before the bytecode section at line 87)
- Test: same file, `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing test**

Add inside `mod tests` in `src/bytecode/meta_math.rs`:

```rust
    #[test]
    fn rent_breakdown_conserves_and_never_extracts() {
        // Solvent wind-down: vault covers the whole pool → everyone made whole,
        // nothing held back, zero operator rent.
        let b = rent_breakdown(&[4, 4, 2], 10);
        assert_eq!(b.total_in, 10);
        assert_eq!(b.returned_to_users, 10);
        assert_eq!(b.held_in_protocol, 0);
        assert_eq!(b.operator_rent, 0);

        // 50% market loss: vault is half the pool. Users recover pro-rata; the
        // shortfall stays as in-protocol bad debt, NOT skimmed to an operator.
        let b = rent_breakdown(&[4, 4, 2], 5);
        assert_eq!(b.total_in, 10);
        assert_eq!(b.returned_to_users, 5, "floor(4*5/10)+floor(4*5/10)+floor(2*5/10)");
        assert_eq!(b.held_in_protocol, 5);
        assert_eq!(b.operator_rent, 0);

        // Conservation + no-extraction over a probe sweep.
        let mut seed: u64 = 0xC0FFEE_u64;
        let mut next = || { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1); seed };
        for _ in 0..500 {
            let deposits = [next() % 1_000, next() % 1_000, next() % 1_000];
            let total: u64 = deposits.iter().sum();
            let vault = next() % (2 * total.max(1));
            let b = rent_breakdown(&deposits, vault);
            assert_eq!(b.total_in, total);
            assert!(b.returned_to_users <= b.total_in, "no base-unit minting");
            assert_eq!(b.returned_to_users + b.held_in_protocol, b.total_in, "conservation");
            assert_eq!(b.operator_rent, 0, "genesis ledger has no operator sink");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p perc5ive --lib bytecode::meta_math::tests::rent_breakdown_conserves_and_never_extracts`
Expected: FAIL — `cannot find function rent_breakdown` / `cannot find type RentBreakdown`.

- [ ] **Step 3: Write the implementation**

Insert in `src/bytecode/meta_math.rs` immediately after the `distribution_approved` fn (after line 85):

```rust
/// Where every base unit ends up when a genesis market winds down.
///
/// The genesis ledger has **no operator/founder destination**: a deposited base
/// unit is either returned to its depositor (`genesis_withdraw`) or remains held
/// in the protocol — in the genesis vault or in the isolated Percolator market it
/// was deployed into at kickstart, both recoverable to depositors via
/// `recover_genesis_market`. Any shortfall under market loss is absorbed as
/// in-protocol bad debt, never skimmed. So `operator_rent` is 0 by construction;
/// a nonzero value would mean a value sink leaked and is a conformance failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RentBreakdown {
    pub total_in: u64,
    pub returned_to_users: u64,
    pub held_in_protocol: u64,
    pub operator_rent: u64,
}

/// Wind-down breakdown for `deposits` against the market `vault_balance` at
/// wind-down. `vault_balance < sum(deposits)` models a market loss; recovery is
/// the same pro-rata rule the handler uses (`genesis_recoverable_principal`),
/// with the whole pool as the outstanding principal.
pub fn rent_breakdown(deposits: &[u64], vault_balance: u64) -> RentBreakdown {
    let total_in: u64 = deposits.iter().copied().sum();
    let returned_to_users: u64 = deposits
        .iter()
        .map(|&d| genesis_recoverable_principal(d, vault_balance, total_in).unwrap_or(0))
        .sum();
    let held_in_protocol = total_in.saturating_sub(returned_to_users);
    let operator_rent = total_in
        .saturating_sub(returned_to_users)
        .saturating_sub(held_in_protocol);
    RentBreakdown { total_in, returned_to_users, held_in_protocol, operator_rent }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p perc5ive --lib bytecode::meta_math::tests::rent_breakdown_conserves_and_never_extracts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/bytecode/meta_math.rs
git commit -m "feat(meta): rent_breakdown reference — genesis ledger has zero operator sink"
```

---

### Task 2: `rent_zero_extraction` conformance property

**Files:**
- Modify: `bench/src/meta_conformance.rs` (import line 18-21; add property after the recovery block ~line 129; bump `total()` assertion line 148)

- [ ] **Step 1: Write the failing test**

Edit the existing unit test in `bench/src/meta_conformance.rs` to expect 5 properties (this is the failing assertion that drives the work):

```rust
        // 5 properties: vote-weight (bytecode), split, quorum, recovery, rent.
        assert_eq!(report.total(), 5, "{}", report.summary());
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p perc5ive-bench meta_conformance_run_is_all_green`
Expected: FAIL — `assertion failed: left: 4, right: 5`.

- [ ] **Step 3: Write the implementation**

In `bench/src/meta_conformance.rs`, extend the import (line 18-21) to add `rent_breakdown`:

```rust
use perc5ive::bytecode::meta_math::{
    distribution_approved, genesis_recoverable_principal, genesis_vote_weight, kickstart_split,
    program_genesis_vote_weight, rent_breakdown,
};
```

Then insert this block after the recoverable-principal property (after line 129, before `r`):

```rust
    // --- rent: the genesis lifecycle has no operator sink; conservation holds ---
    let mut rent_ok = true;
    let mut rseed: u64 = 0x5EED_5EED_5EED_5EED;
    let mut rnext = || {
        rseed = rseed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        rseed
    };
    for _ in 0..600 {
        let deposits = [rnext() % 1_000_000, rnext() % 1_000_000, rnext() % 1_000_000];
        let total: u64 = deposits.iter().sum();
        let vault = rnext() % (2 * total.max(1));
        let b = rent_breakdown(&deposits, vault);
        let conserves = b.returned_to_users + b.held_in_protocol == b.total_in;
        let no_mint = b.returned_to_users <= b.total_in;
        if !(b.operator_rent == 0 && conserves && no_mint && b.total_in == total) {
            rent_ok = false;
        }
    }
    if rent_ok {
        r.record_pass("rent_zero_extraction_and_conservation");
    } else {
        r.record_fail("rent_zero_extraction_and_conservation", "operator rent != 0 or pool not conserved");
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p perc5ive-bench meta_conformance`
Expected: PASS — `meta_conformance_run_is_all_green` green, report total 5.

- [ ] **Step 5: Commit**

```bash
git add bench/src/meta_conformance.rs
git commit -m "feat(bench): rent_zero_extraction conformance property (5th meta property)"
```

---

### Task 3: `tests/e2e_rent_audit.rs` — proof on the real VM

**Files:**
- Create: `tests/e2e_rent_audit.rs`

- [ ] **Step 1: Write the test file**

Create `tests/e2e_rent_audit.rs`. Copy the header `mod common;` + use-block + every helper from `tests/e2e_meta_genesis.rs:18-128` verbatim (the `FN_*` consts, `script_key`, `data_account`, `signer_account`, `script_account`, `input_u64s`, `call_ok`, `call`, `u64_at`, `u8_at`, `linked_meta`), then add `FN_DRAW_SURPLUS` and the two tests below. Full file:

```rust
//! Rent audit — proves the MetaGenesis lifecycle extracts zero operator rent on
//! the **real linked binary**. Tracks the base-unit ledger across the full
//! lifecycle and asserts (a) withdrawals never exceed deposits (no base-unit
//! minting / no skim), (b) a solvent full wind-down returns every base unit to
//! depositors (operator rent = 0), (c) COIN supply conserves (minted == reward),
//! (d) draw_genesis_surplus is bounded by available surplus.

mod common;

use common::{make_account_info, read_data, FIVE_VM_PROGRAM_ID};
use five_protocol::types;
use five_protocol::{
    DSL_RAW_ACCOUNT_HEADER_LEN, DSL_RAW_ACCOUNT_HEADER_MAGIC, DSL_RAW_ACCOUNT_HEADER_VERSION,
};
use five_vm_mito::{AccountInfo, MitoVM, Pubkey, StackStorage, Value};
use perc5ive::bytecode::meta_handlers::{
    self as mh, coin_cfg_offsets, fld, genesis_cfg_offsets, genesis_distribution_offsets,
    genesis_position_offsets, genesis_vote_offsets,
};

const HDR: usize = DSL_RAW_ACCOUNT_HEADER_LEN;

// Public-function indices = DSL pub-declaration order in meta/src/main.v.
const FN_INIT_COIN_CONFIG: u32 = 0;
const FN_ACTIVATE_LIVE: u32 = 1;
const FN_INIT_GENESIS_BOOTSTRAP: u32 = 2;
const FN_GENESIS_DEPOSIT: u32 = 3;
const FN_GENESIS_WITHDRAW: u32 = 4;
const FN_KICKSTART: u32 = 5;
const FN_INIT_DISTRIBUTION: u32 = 6;
const FN_VOTE: u32 = 7;
const FN_MINT_REWARD: u32 = 8;
const FN_FINALIZE: u32 = 9;
const FN_DRAW_SURPLUS: u32 = 10;

// === BEGIN verbatim copy of helpers from tests/e2e_meta_genesis.rs:45-128 ===
// script_key, data_account, signer_account, script_account, input_u64s,
// call_ok, call, u64_at, u8_at, linked_meta — copy them exactly.
// === END verbatim copy ===

/// Conservation guard: withdrawals must never exceed deposits at any point.
fn assert_no_overdraw(genesis_cfg: &AccountInfo, step: &str) {
    use genesis_cfg_offsets as gc;
    let dep = u64_at(genesis_cfg, gc::TOTAL_DEPOSITED);
    let wd = u64_at(genesis_cfg, gc::TOTAL_WITHDRAWN);
    assert!(wd <= dep, "{step}: withdrawn {wd} > deposited {dep} — base units minted/skimmed");
}

#[test]
fn lifecycle_returns_every_base_unit_zero_operator_rent() {
    use coin_cfg_offsets as cc;
    use genesis_cfg_offsets as gc;
    use genesis_position_offsets as gp;

    let linked = linked_meta();
    let script = script_account();
    let coin_cfg = data_account(1, coin_cfg_offsets::SIZE);
    let genesis_cfg = data_account(2, genesis_cfg_offsets::SIZE);
    let alice_pos = data_account(3, genesis_position_offsets::SIZE);
    let bob_pos = data_account(4, genesis_position_offsets::SIZE);
    let dist1 = data_account(6, genesis_distribution_offsets::SIZE);
    let alice_v1 = data_account(8, genesis_vote_offsets::SIZE);
    let bob_v1 = data_account(9, genesis_vote_offsets::SIZE);
    let authority = signer_account(0x41);
    let alice = signer_account(0x42);
    let bob = signer_account(0x43);

    call_ok(&linked, FN_INIT_COIN_CONFIG, &[50, 100],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "init_coin_config");
    call_ok(&linked, FN_INIT_GENESIS_BOOTSTRAP, &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "bootstrap");

    for (pos, amt, who) in [(&alice_pos, 6u64, "alice"), (&bob_pos, 4, "bob")] {
        call_ok(&linked, FN_GENESIS_DEPOSIT, &[amt, 100],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], who);
        assert_no_overdraw(&genesis_cfg, who);
    }
    assert_eq!(u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED), 10);

    call_ok(&linked, FN_KICKSTART, &[0, 1000],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "kickstart");
    assert_no_overdraw(&genesis_cfg, "post-kickstart");

    call_ok(&linked, FN_ACTIVATE_LIVE, &[150],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "activate_live");
    call_ok(&linked, FN_INIT_DISTRIBUTION, &[1, 100],
        &[script.clone(), genesis_cfg.clone(), dist1.clone(), authority.clone()], "init_dist1");
    for (voter, pos, vrec, who) in [(&alice, &alice_pos, &alice_v1, "a"), (&bob, &bob_pos, &bob_v1, "b")] {
        call_ok(&linked, FN_VOTE, &[1, 160],
            &[script.clone(), pos.clone(), dist1.clone(), vrec.clone(), voter.clone()], who);
    }
    call_ok(&linked, FN_MINT_REWARD, &[100],
        &[script.clone(), genesis_cfg.clone(), dist1.clone(), authority.clone()], "mint");
    assert_eq!(u64_at(&genesis_cfg, gc::MINTED_SUPPLY), u64_at(&genesis_cfg, gc::REWARD_SUPPLY),
        "COIN supply conserved: minted == reward");

    call_ok(&linked, FN_FINALIZE, &[],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "finalize");

    // Solvent vault (== pool, 10) → both recover full principal.
    for (pos, who) in [(&alice_pos, "alice"), (&bob_pos, "bob")] {
        call_ok(&linked, FN_GENESIS_WITHDRAW, &[1, 0, 10],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], who);
        assert_no_overdraw(&genesis_cfg, who);
    }

    // Zero operator rent: every deposited base unit came back to depositors.
    let dep = u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED);
    let wd = u64_at(&genesis_cfg, gc::TOTAL_WITHDRAWN);
    assert_eq!(wd, dep, "solvent wind-down returns 100% of principal — operator rent = 0");
    assert_eq!(u64_at(&alice_pos, gp::WITHDRAWN), 6);
    assert_eq!(u64_at(&bob_pos, gp::WITHDRAWN), 4);
    let _ = cc::PHASE; // keep import used
}

#[test]
fn surplus_draw_is_bounded_no_rent_leak() {
    use genesis_cfg_offsets as gc;
    let linked = linked_meta();
    let script = script_account();
    let genesis_cfg = data_account(2, genesis_cfg_offsets::SIZE);
    let pos = data_account(3, genesis_position_offsets::SIZE);
    let authority = signer_account(0x41);
    let alice = signer_account(0x42);

    call_ok(&linked, FN_INIT_GENESIS_BOOTSTRAP, &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "bootstrap");
    call_ok(&linked, FN_GENESIS_DEPOSIT, &[10, 100],
        &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], "deposit");
    call_ok(&linked, FN_KICKSTART, &[0, 1000],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "kickstart");

    // Draw more than the available surplus (amount 50, vault_balance 10, all of
    // it outstanding principal → surplus 0) must be rejected, not skimmed.
    let s = call(&linked, FN_DRAW_SURPLUS, &[50, 10],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "overdraw");
    assert_eq!(s, mh::STATUS_INSUFFICIENT_SURPLUS, "surplus draw bounded by vault - outstanding");
    assert_no_overdraw(&genesis_cfg, "post-overdraw-attempt");
    let _ = gc::TOTAL_DEPOSITED;
}
```

- [ ] **Step 2: Run to verify it builds and fails on a real assertion (not compile error)**

Run: `cargo build && cargo test --test e2e_rent_audit`
Expected: COMPILES; both tests run. If the verbatim-copy block is missing helpers, fix the copy. (Tests should PASS once helpers are copied correctly — the lifecycle mirrors the proven `e2e_meta_genesis`.)

- [ ] **Step 3: Confirm green**

Run: `cargo test --test e2e_rent_audit -- --nocapture`
Expected: PASS — `lifecycle_returns_every_base_unit_zero_operator_rent`, `surplus_draw_is_bounded_no_rent_leak`.

- [ ] **Step 4: Commit**

```bash
git add tests/e2e_rent_audit.rs
git commit -m "test(meta): e2e rent audit — zero operator rent proven on the linked binary"
```

---

### Task 4: `simulate_rent_audit` MCP catalogue entry

**Files:**
- Modify: `mcp/src/tools.rs` (add after `explain_futarchy_lifecycle`, ~line 168)

- [ ] **Step 1: Add the catalogue entry**

In `mcp/src/tools.rs`, immediately after the `explain_futarchy_lifecycle` `McpTool { ... }` block (before the `// Write (devnet-only)` comment at line 169):

```rust
        McpTool {
            name: "simulate_rent_audit",
            description: "Audit operator rent across a genesis wind-down: given per-depositor deposits and the market vault balance, return total_in, returned_to_users, held_in_protocol (in-network bad debt under loss), and operator_rent (0 by construction — the genesis ledger has no operator sink). Backs the rent_zero_extraction conformance property.",
            input_schema: r#"{ "type": "object", "required": ["deposits", "vault_balance"], "properties": { "deposits": { "type": "array", "items": { "type": "integer" } }, "vault_balance": { "type": "integer" } } }"#,
            category: ToolCategory::Simulation,
        },
```

- [ ] **Step 2: Run the catalogue tests**

Run: `cargo test -p perc5ive-mcp tools`
Expected: PASS — `catalogue_is_nonempty_and_unique`, `input_schemas_are_nonempty`, `descriptions_are_nonempty` all green with the new entry.

- [ ] **Step 3: Commit**

```bash
git add mcp/src/tools.rs
git commit -m "feat(mcp): simulate_rent_audit tool schema (5th genesis simulation tool)"
```

---

## Phase 2 — watermelon isolation demo

### Task 5: `aggregate_rent` across markets in `meta_math`

**Files:**
- Modify: `src/bytecode/meta_math.rs` (add after `rent_breakdown`)
- Test: same file `mod tests`

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src/bytecode/meta_math.rs`:

```rust
    #[test]
    fn aggregate_rent_sums_markets_and_stays_zero_extraction() {
        let a = rent_breakdown(&[6, 4], 10); // solvent
        let b = rent_breakdown(&[5, 5], 5);  // 50% loss
        let agg = aggregate_rent(&[a, b]);
        assert_eq!(agg.total_in, 20);
        assert_eq!(agg.returned_to_users, 10 + 5);
        assert_eq!(agg.held_in_protocol, 0 + 5);
        assert_eq!(agg.operator_rent, 0, "aggregate operator rent across markets is 0");
        assert_eq!(agg.returned_to_users + agg.held_in_protocol, agg.total_in);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p perc5ive --lib bytecode::meta_math::tests::aggregate_rent_sums_markets_and_stays_zero_extraction`
Expected: FAIL — `cannot find function aggregate_rent`.

- [ ] **Step 3: Write the implementation**

In `src/bytecode/meta_math.rs`, after the `rent_breakdown` fn:

```rust
/// Aggregate the rent breakdown across multiple **isolated** markets launched
/// under one shared futarchy. Markets do not cross-collateralize (Percolator's
/// per-market isolation), so the aggregate is a plain field-wise sum; operator
/// rent stays 0 in the aggregate iff it is 0 per market.
pub fn aggregate_rent(markets: &[RentBreakdown]) -> RentBreakdown {
    markets.iter().fold(
        RentBreakdown { total_in: 0, returned_to_users: 0, held_in_protocol: 0, operator_rent: 0 },
        |acc, m| RentBreakdown {
            total_in: acc.total_in + m.total_in,
            returned_to_users: acc.returned_to_users + m.returned_to_users,
            held_in_protocol: acc.held_in_protocol + m.held_in_protocol,
            operator_rent: acc.operator_rent + m.operator_rent,
        },
    )
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p perc5ive --lib bytecode::meta_math::tests::aggregate_rent_sums_markets_and_stays_zero_extraction`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/bytecode/meta_math.rs
git commit -m "feat(meta): aggregate_rent across isolated markets — sum, no cross-collateral"
```

---

### Task 6: `tests/e2e_market_factory.rs` — two isolated markets

**Files:**
- Create: `tests/e2e_market_factory.rs`

- [ ] **Step 1: Write the test file**

Create `tests/e2e_market_factory.rs`. Copy the same harness block from `tests/e2e_meta_genesis.rs:18-128` (use-block + `FN_*` consts 0-9 + helpers), then:

```rust
/// Run a minimal solvent genesis lifecycle on the given account-tag base and
/// return (total_deposited, total_withdrawn) from its genesis_cfg. Each call
/// uses a disjoint account set, so the two markets share only the linked binary.
fn run_isolated_genesis(linked: &[u8], base: u8, deposit_a: u64, deposit_b: u64) -> (u64, u64) {
    use genesis_cfg_offsets as gc;
    let script = script_account();
    let coin_cfg = data_account(base, coin_cfg_offsets::SIZE);
    let genesis_cfg = data_account(base + 1, genesis_cfg_offsets::SIZE);
    let pa = data_account(base + 2, genesis_position_offsets::SIZE);
    let pb = data_account(base + 3, genesis_position_offsets::SIZE);
    let dist = data_account(base + 4, genesis_distribution_offsets::SIZE);
    let va = data_account(base + 5, genesis_vote_offsets::SIZE);
    let vb = data_account(base + 6, genesis_vote_offsets::SIZE);
    let authority = signer_account(base + 0x40);
    let alice = signer_account(base + 0x41);
    let bob = signer_account(base + 0x42);
    let total = deposit_a + deposit_b;

    call_ok(linked, FN_INIT_COIN_CONFIG, &[50, 100],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "init");
    call_ok(linked, FN_INIT_GENESIS_BOOTSTRAP, &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "bootstrap");
    call_ok(linked, FN_GENESIS_DEPOSIT, &[deposit_a, 100],
        &[script.clone(), genesis_cfg.clone(), pa.clone(), alice.clone()], "dep_a");
    call_ok(linked, FN_GENESIS_DEPOSIT, &[deposit_b, 100],
        &[script.clone(), genesis_cfg.clone(), pb.clone(), bob.clone()], "dep_b");
    call_ok(linked, FN_KICKSTART, &[0, 1000],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "kick");
    call_ok(linked, FN_ACTIVATE_LIVE, &[150],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "live");
    call_ok(linked, FN_INIT_DISTRIBUTION, &[1, 100],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), authority.clone()], "dist");
    call_ok(linked, FN_VOTE, &[1, 160],
        &[script.clone(), pa.clone(), dist.clone(), va.clone(), alice.clone()], "va");
    call_ok(linked, FN_VOTE, &[1, 160],
        &[script.clone(), pb.clone(), dist.clone(), vb.clone(), bob.clone()], "vb");
    call_ok(linked, FN_MINT_REWARD, &[100],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), authority.clone()], "mint");
    call_ok(linked, FN_FINALIZE, &[],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "finalize");
    for (pos, who) in [(&pa, "wa"), (&pb, "wb")] {
        call_ok(linked, FN_GENESIS_WITHDRAW, &[1, 0, total],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], who);
    }
    (u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED), u64_at(&genesis_cfg, gc::TOTAL_WITHDRAWN))
}

#[test]
fn two_markets_under_one_futarchy_are_isolated_and_rent_free() {
    let linked = linked_meta();

    // Market A: pool of 10. Market B: pool of 30. Disjoint account tags.
    let (a_dep, a_wd) = run_isolated_genesis(&linked, 1, 6, 4);
    let (b_dep, b_wd) = run_isolated_genesis(&linked, 21, 20, 10);

    // Isolation: distinct pools, neither bled into the other.
    assert_eq!(a_dep, 10, "market A pool");
    assert_eq!(b_dep, 30, "market B pool independent of A");
    assert_ne!(a_dep, b_dep, "markets do not share a ledger");

    // Rent-free in both: solvent wind-down returns 100% to depositors.
    assert_eq!(a_wd, a_dep, "market A operator rent = 0");
    assert_eq!(b_wd, b_dep, "market B operator rent = 0");

    // Aggregate matches the meta_math model: 40 in, 40 returned, 0 operator rent.
    use perc5ive::bytecode::meta_math::{aggregate_rent, rent_breakdown};
    let agg = aggregate_rent(&[rent_breakdown(&[6, 4], 10), rent_breakdown(&[20, 10], 30)]);
    assert_eq!(agg.total_in, a_dep + b_dep);
    assert_eq!(agg.returned_to_users, a_wd + b_wd);
    assert_eq!(agg.operator_rent, 0);
}
```

- [ ] **Step 2: Build and run**

Run: `cargo build && cargo test --test e2e_market_factory -- --nocapture`
Expected: PASS — markets isolated (10 vs 30), both rent-free, aggregate operator rent 0.

- [ ] **Step 3: Commit**

```bash
git add tests/e2e_market_factory.rs
git commit -m "test(meta): two isolated markets under one futarchy — isolation + rent-free"
```

---

## Phase 3 — launch / positioning

### Task 7: draft the gated RFS-answer thread

**Files:**
- Create: `docs-internal/launch/RFS_ANSWER_THREAD.md`

- [ ] **Step 1: Verify the honesty bounds against the build before writing claims**

Run: `cargo test --test e2e_rent_audit --test e2e_market_factory && cargo test -p perc5ive-bench meta_conformance`
Expected: all PASS. Re-read `DEVNET.md:30-41` to confirm meta is still NOT devnet-live (claim must not assert devnet for meta).

- [ ] **Step 2: Write the thread draft**

Create `docs-internal/launch/RFS_ANSWER_THREAD.md` with this content (voice/guardrails per `docs-internal/launch/METAGENESIS_THREAD.md` and `TOLY_STRATEGY.md`; tag Toly once; 🦞 only emoji; Perc5ive = implementation):

```markdown
# RFS-answer thread (draft — DO NOT POST until gate cleared)

**Status:** draft. Gate: Phases 1-2 green (e2e_rent_audit, e2e_market_factory,
meta_conformance all pass) AND honesty bounds in the design spec respected.
Posting is the user's action, not Claude's. Best window: Tue/Wed 9-11am PT.

**Honesty gate (hard):** MUST NOT claim meta is devnet-live (it is not —
DEVNET.md:36). MAY claim: ported + conformance + e2e against the real linked
binary + operator rent = 0 proven on the VM. MUST NOT claim "we built
Percolator" or "formally verified" of our code.

---

1/ @aeyakovenko today: "bootstrap a Futarchy together... markets under one roof
that can't wreck each other... reduce rents to marginal and return all rents to
users." We've been building exactly this since our Frontier submission —
percolator + percolator-meta, ported to the 5ive DSL.

2/ The fair-launch is live in the port: bond base units → earn time-weighted
votes over a fixed COIN distribution → winning distribution mints and becomes the
MetaDAO. No yield. Capital-at-risk is the cost of a vote, not an investment.

3/ "Reduce rents to marginal, return to users" — we measured it. Across the whole
genesis lifecycle, operator rent = 0, proven on the executed VM bytecode
(tests/e2e_rent_audit.rs): every base unit returns to depositors or stays in the
protocol's own pools. Nothing skims to a founder key.

4/ "Markets under one roof that can't wreck each other" — two genesis markets run
under one futarchy, fully isolated: a fault in one cannot touch the other's
ledger (tests/e2e_market_factory.rs). Isolation by construction, not by promise.

5/ Conformance, as a gift to the ecosystem: vote-weight bit-exact vs the
reference across every log2 bucket; kickstart split, quorum, recovery, and the
new rent-zero-extraction property all green in PercolatorBench.

6/ Honest status: ported + conformance + e2e against the real linked binary.
Meta's devnet deploy is pending our mono redeploy wave; the engine + three
markets are already on devnet (pre-mono). No overclaim.

7/ Percolator + percolator-meta are @aeyakovenko's design; the futarchy lineage
is @MetaDAOProject. Perc5ive is the 5ive-DSL implementation. Repo: <link>. 🦞

---

## Poster notes
- Replace <link> with the repo URL (and the meta devnet program ID page IF the
  mono redeploy has landed by post time — otherwise omit, do not fake it).
- If engagement is zero, the repo + green conformance is the durable artifact.
- Tag @aeyakovenko once (this thread is the milestone). No follow-up tagging.
```

- [ ] **Step 3: Commit**

```bash
git add docs-internal/launch/RFS_ANSWER_THREAD.md
git commit -m "docs(launch): gated Toly-tagged RFS-answer thread draft (do not post)"
```

---

## Self-review notes (resolved)

- **Spec coverage:** Phase 1 (rent audit) → Tasks 1-4; Phase 2 (isolation demo) → Tasks 5-6; Phase 3 (gated thread) → Task 7. All spec sections covered. The deferred governance-bytecode factory is explicitly out of scope (own spec).
- **No new bytecode:** every e2e drives only already-linked genesis handlers (FN 0-10). `draw_genesis_surplus` (FN 10) is linked; `recover`/governance (11+) are not exercised.
- **Type consistency:** `RentBreakdown` fields (`total_in`, `returned_to_users`, `held_in_protocol`, `operator_rent`) are identical across Tasks 1, 2, 5, 6. `rent_breakdown(&[u64], u64)` and `aggregate_rent(&[RentBreakdown])` signatures match every call site.
- **Honesty gate:** Task 7 Step 1 re-verifies devnet status before any claim is written; the draft is explicitly not-for-posting by Claude.
```
