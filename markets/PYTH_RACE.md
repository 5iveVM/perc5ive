# PythRaceMarket — short-duration binary oracle-threshold market

**Replaces:** AttestedPerp (dropped because Umbra's Breakout Infrastructure PRIZE + Accelerator C4 make the privacy-perp cluster too head-on)

**Status:** second market of Perc5ive submission; built in Week 2 after Sov lands

---

## What it does

Short-duration binary prediction market on Pyth oracle feeds:
> *"Will Pyth SOL-USD feed exceed $275 within the next 60 seconds?"*

- **Market opens** at current Pyth price
- **Threshold set** at open-time-price ± delta (e.g., +2% for uptick markets, -2% for downtick)
- **Resolution window** fixed at 30s–10min depending on market config
- **Settlement** triggered by any user post-window; reads the Pyth feed at resolution time; pays out winners via Percolator's `trade` + `liquidate` flow
- **Collateral:** USDC (standard)
- **Risk managed by:** Percolator engine's H haircut + insurance fund

## Why this fills a gap

From Copilot data (see `COMPETITIVE_LANDSCAPE.md`):
- Kérdos Markets: LATAM prediction markets — different geography, not oracle-race
- SAGA Prediction Market: AMM-based conditional tokens — different mechanics, not Percolator-based
- PrismaFi: real-world event forecasting — different topic class
- FinancePolymarket: financial investment predictions — different timeframes

**No direct short-duration oracle-threshold-on-Percolator market in the corpus.** This is a genuine PARTIAL-to-FULL gap.

Additional thesis:
- Solana Foundation's "Internet Capital Markets" essay explicitly mentions oracle-based DeFi as an undervalued primitive
- Pyth-race markets compose naturally with existing HFT infrastructure (MEV bots, Jito bundles) — sophisticated-user appeal
- Short-duration markets are capital-efficient — users don't tie up collateral overnight

---

## Architecture

### Two programs (same pattern as Sov)
1. **PythRace Wrapper** (5ive DSL) — market state, oracle reads, CPIs to Percolator
2. **Percolator Engine** — risk math

### Per-market account (5ive DSL)
```v
account PythRaceMarket {
    pyth_feed: pubkey;
    initial_price: i64;           // Pyth price at market-open
    threshold_price: i64;          // initial_price ± delta
    direction: bool;               // true = up-side market, false = down-side
    open_slot: u64;
    resolution_slot: u64;          // open_slot + window_slots
    resolved: bool;
    final_price: i64;
    total_long_collateral: u64;
    total_short_collateral: u64;
    percolator_market_pda: pubkey;
}
```

### Per-user position
Reuse Percolator's standard position account. The PythRace wrapper doesn't need a custom user account; it just encodes the binary outcome at settlement.

### Settlement flow
1. Any user calls `resolve_pyth_race(market)`
2. Wrapper reads current Pyth feed
3. Determines winning side (long if `current_price > threshold` for up-markets, etc.)
4. CPIs to Percolator `liquidate` for all losing positions
5. CPIs to Percolator `withdraw` for all winning positions (with pro-rata allocation)
6. Marks market `resolved = true`; locks further interaction

---

## Build scope (Week 2, ~4 engineer-days)

### Must-ship
- [ ] `init_pyth_race_market(pyth_feed, threshold_delta_bps, window_slots)`
- [ ] `open_position(user, is_long, collateral_amount)` — CPIs Percolator `deposit` + `trade`
- [ ] `resolve_pyth_race(market)` — permissionless settlement
- [ ] Pyth pull oracle integration (must work before build starts; defer if not)
- [ ] MCP `get_pyth_race_market(feed, market_id)` + `list_active_markets()`
- [ ] Devnet deploy + 5 test markets run to completion

### Nice-to-have
- [ ] Multi-window offering (30s, 1min, 5min, 10min variants)
- [ ] Fee rebate for early settlement (incentivize fast resolution)
- [ ] Event stream for market open/close (enables leaderboards, volume dashboards)

---

## Design decisions

### Why Pyth specifically
Pyth has pull-based price feeds with per-slot updates and cryptographic signatures. This makes oracle-race markets provably fair — the feed at resolution slot is deterministic.

Alternatives (Switchboard, Chainlink) have different update cadences or are not Solana-native.

### Why fixed windows
Arbitrary-duration markets require continuous liquidity. Fixed windows (30s, 1min, 5min) let us pre-bin liquidity and simplify the UX.

### Why inside Percolator (not standalone)
Percolator's risk engine handles the insurance-fund-on-loss case that plain prediction markets can't. If a market resolves with massive one-sided flow (everyone was long and lost), Percolator's insurance fund absorbs the shortfall in a principled way.

This is the 5ive+Percolator composition thesis in action: we get battle-tested risk math for free.

---

## Competitive positioning

| Project | How it differs from PythRaceMarket |
|---------|-----------------------------------|
| Kérdos Markets | LATAM prediction markets; multi-day horizon; not oracle-race |
| SAGA Prediction Market | AMM-conditional tokens; any event type; not specifically short-duration |
| PrismaFi | Real-world forecasting; not oracle-based |
| FinancePolymarket | Investment predictions; longer horizons |
| Polymarket (non-Solana) | Different chain; general events; not Percolator-backed |

**Niche:** short-duration + oracle-threshold + Percolator-risk-backed is empty.

---

## Risk matrix

| Risk | Severity | Mitigation |
|------|---------|------------|
| Pyth pull CPI blocked (same as Sov) | HIGH | HELLO_SLAB spike covers both markets; if blocked globally, mock oracle + disclose |
| MEV / frontrunning on settlement | MEDIUM | Use Jito bundle for atomic read+settle OR permissionless-settle-anyone |
| Binary markets look like gambling → regulatory risk | MEDIUM | Frame as "oracle attestation derivative"; reference existing Kalshi/Polymarket regulatory treatment |
| Low liquidity kills mechanic | MEDIUM | Incentivize LPs with fee rebate + launch partnership with a market maker |
| Price manipulation at resolution slot | MEDIUM | Percolator's oracle-manipulation safety (`§0` spec) handles this via two-bucket warmup |

---

## Demo moment (Frontier 2026 pitch)

Judge asks: *"show me what happens if Pyth lags and the race resolves ambiguously"*

MCP-Claude queries a live resolved market → explains:
1. Market opened at price X with threshold X+2%
2. Pyth feed at resolution slot was X+2.3% (above threshold)
3. Longs won; payout allocated pro-rata
4. Insurance fund unused (no shortfall)

30-second demo clip that shows Percolator's BYOM architecture working in a non-Sov context.

---

## Open questions

1. Can Percolator support per-market resolution deadlines as first-class state, or do we have to encode that in the wrapper?
2. Do we need a keeper bot to trigger resolution, or can it be user-triggered with a finder's fee?
3. What's the minimum window duration that's still arbitrage-safe given Solana block time + Pyth feed latency?
4. Should the threshold be set at init-time or via a Dutch-auction-like open phase?

Resolve during Week 2 architecture session.
