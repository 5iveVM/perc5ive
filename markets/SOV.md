# Sov — inverted memecoin perp (anchor market)

**Source concept:** @aeyakovenko retweet, tweet `2020173049094619222`, April 2026
> *"sov = percolator inverted market for the memecoin, so it's backed by the memecoin + burn the admin key. The insurance fund will..."*

**Status:** to be built in Sov 48-hour sprint (see `/SOV_SPRINT.md`)
**Priority:** anchor market of Perc5ive submission — the single highest-leverage artifact

---

## What Sov does

A perpetual-futures market where:
- **Collateral is the memecoin itself** (not USDC / SOL)
- **Contract is inverted** — longs profit when memecoin falls relative to USD peg; shorts profit when memecoin rises (relative to the memecoin's own denomination)
  - Equivalent to traditional "short the dollar in memecoin terms"
- **Admin key is burned at initialization** — no upgradable authority, no insurance-fund discretion, no market parameter changes post-launch
- **Insurance fund** seeded at init + fee-funded, backs solvency shortfalls in Percolator's standard H (haircut) lane

## Why this is interesting

- **Credibly trust-minimized** — burn-the-admin-key pattern signals to memecoin holders that there's no rug rail
- **Native-token collateral** — no need to hold USDC to trade; memecoin holders can leverage their existing bags
- **Fees paid in memecoin** — flywheel for the underlying token (every trade creates token demand)
- **Specifically Percolator-native** — Percolator's haircut + two-bucket warmup handles the oracle manipulation risk that plain inverted-perps can't

## Architecture

### Two programs
1. **Sov Wrapper Program** (5ive DSL) — holds user memecoin collateral; CPIs into Percolator engine
2. **Percolator Engine** (Anatoly's Rust Pinocchio) — does the risk math

### Sov Wrapper responsibilities
- Hold memecoin ATAs for deposits
- Track `admin_key_burned: bool` (true from day 1)
- Track insurance fund accrual
- Route each of the 12 Percolator instructions through a Sov-specific wrapper
- Export MCP tools (`get_sov_state`, `simulate_sov_liquidation`)

### Per-user account layout (5ive DSL)
```v
account SovPosition {
    user: pubkey;
    market: pubkey;
    percolator_account: pubkey;  // the PDA that Percolator engine reads
    collateral_amount: u64;
    position_size_scaled: i64;   // POS_SCALE = 1_000_000
    is_long: bool;
    entry_index_a: u64;          // A_side at entry
    entry_index_k: u64;          // K_side at entry
    entry_index_f: i64;          // F_side_num at entry
    liquidation_count: u8;
}
```

### Per-market account (5ive DSL)
```v
account SovMarket {
    memecoin_mint: pubkey;
    collateral_vault: pubkey;
    admin_key_burned: bool;      // always true after init
    insurance_fund: u64;
    insurance_fund_authority: pubkey;  // set to zero address post-burn
    fee_bps: u16;
    init_timestamp: u64;
    position_count: u32;
    oracle: pubkey;              // Pyth pull or fallback
}
```

---

## Build scope (48-hour sprint)

### Must-ship
- [ ] `init_sov_market(memecoin_mint, initial_insurance_seed)` with admin-key-burn in same tx
- [ ] `deposit_collateral(user, amount)` — CPIs to Percolator `deposit`
- [ ] `open_position(user, is_long, notional, leverage)` — CPIs to Percolator `trade`
- [ ] `close_position(user)` — CPIs to Percolator `trade` (reducing)
- [ ] `liquidate(account)` — CPIs to Percolator `liquidate`
- [ ] MCP `get_sov_position` + `get_sov_market_state` tools
- [ ] Devnet deployment of full flow
- [ ] 1 test user opens + closes a position end-to-end

### Nice-to-have (defer if time short)
- [ ] Pyth pull oracle integration (falls back to mock oracle if blocked)
- [ ] `keeper_crank` exposure
- [ ] Per-user position history event stream
- [ ] Funding rate visualization

### Explicitly OUT of 48h scope
- UI / frontend
- Multi-market support (one hardcoded test memecoin)
- Mainnet deploy
- Formal verification beyond Percolator's upstream Kani
- Comprehensive testing (basic happy-path only)

---

## Design decisions (rationale)

### Why "inverted" specifically
Anatoly's tweet said "inverted market for the memecoin, backed by the memecoin." In traditional perps:
- Standard perp: collateral USDC, position denominated in memecoin terms, P&L in USDC
- Inverted perp (BitMEX-style): collateral in memecoin, position denominated in memecoin, P&L in memecoin

Sov is the inverted variant because it lets memecoin holders trade without needing USDC.

### Why burn the admin key
Memecoin communities are rightly paranoid about rugs and parameter-change attacks. Burning the admin key is a costly signal that the founder cannot:
- Pause the market
- Change fees
- Seize insurance fund
- Upgrade the program

Combined with Percolator's formally verified risk logic, the market becomes as trust-minimized as on-chain code allows.

### Why the insurance fund matters
Inverted perps have tail risk when the memecoin goes to zero — collateral and position P&L both collapse simultaneously. Percolator's insurance fund + two-bucket warmup absorbs these shortfalls before they hit solvent accounts.

### Why Pyth pull (and why fallback matters)
Pyth pull is the cleanest way to get real-time memecoin price feeds on Solana. BUT many memecoins don't have Pyth feeds, only the majors (BONK, WIF, POPCAT, etc.). For the 48h sprint, use a Pyth-feed-supported memecoin OR mock oracle.

---

## Risk matrix

| Risk | Severity | Mitigation |
|------|---------|------------|
| Pinocchio CPI from 5ive DSL fails | HIGH | HELLO_SLAB spike in hour 1-8. If blocked: document honestly in tweet, demo with mock CPI |
| No Pyth feed for test memecoin | MEDIUM | Use BONK or WIF devnet feed. Mock if no feed available. |
| Admin-key-burn mechanism isn't verifiable | MEDIUM | Make the burn explicit in init_sov_market — post-init the market_authority field = System Program (11111...1) |
| Insurance fund can be drained by malicious LP | LOW | Percolator's formal verification handles this; Sov just exposes Percolator's standard insurance flow |
| Inverted math has off-by-one vs upstream Percolator | MEDIUM | Write PercolatorBench invariants FIRST; every Sov op must maintain conservation |
| Post-launch Anatoly ships Sov himself | HIGH | Ship in 48 hours. Monitor @aeyakovenko + @percolator_fun hourly. |

---

## Demo script (for the 60-second video)

See `/SOV_SPRINT.md` for the full video script. Sov-specific demo moments:

1. **Show the admin-burn tx on devnet explorer** — zoom in on the post-init state where `authority` = `11111111...`
2. **Open a position** — watch the Percolator engine log the deposit + trade
3. **Show the MCP query** — judge types "what happens if memecoin drops 30%" → Claude simulates liquidation
4. **End-card** — Toly's retweet quote + "Sov. Shipped in 5ive in 48 hours. Percolator's BYOM works."

---

## Post-sprint expansion (if GREEN)

Sov becomes market 1 of 3 in the Perc5ive Frontier submission. Extensions:
- Multi-memecoin support (one wrapper per memecoin)
- Funding rate dashboard (reuse 5ive's existing event primitive)
- Cross-market portfolio view via MCP
- Leaderboard of Sov positions (public state reads, no trading surveillance)
- "Sov index" — weighted basket of top 10 memecoins' Sov positions

---

## Open questions

1. Does Percolator's engine support collateral denominated in anything other than the base asset (memecoin vs USDC)? Check `spec.md §1` for asset-class constraints.
2. Is there a Percolator instruction for "this market has no authority" or do we enforce it in the wrapper?
3. How does Percolator's `F_side_num` funding rate interact with inverted markets where funding is paid in the memecoin?
4. Can we run two Sov markets (different memecoins) against the same Percolator engine instance, or does each market need its own deployment?

Answer all four before hour 24 of the sprint, ideally during hello-slab.
