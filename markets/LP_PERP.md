# LPPerp — AMM-backed passive market-maker perp

**Status:** third market of Perc5ive submission; built in Week 2-3

---

## What it does

Liquidity providers deposit into a 5ive AMM pool. The pool's liquidity functions as the Percolator slab's passive quote book. LPs earn:
1. Standard AMM swap fees (from spot-style trades against the pool)
2. Perp funding payments + trading fees (from positions routed through the slab)

Risk is managed by Percolator's engine — LPs are not exposed to unbounded losses because the slab enforces position caps + insurance-fund-absorbs-shortfall pattern.

**One-sentence pitch:** *"Deposit into an AMM, get passive perp MM exposure, let Percolator handle the risk math."*

## Why this fills a gap

From Copilot data:
- Maiker.fun (Breakout, sim=0.055): automated concentrated liquidity vaults for yield — closest adjacency, but for spot not perps
- Tyrbine (Cypherpunk): single-sided liquidity — different mechanic
- LP Bot / LPbot.io: copy-trading Orca LP — management tools not primitives

**LP-deposits-as-perp-slab is a novel composition.** Ships with a real UX win: solo LPs get perp-MM exposure without running HFT infrastructure.

Also notable:
- Percolator's thesis explicitly includes "prop-AMM competition for perps" — this is the most literal interpretation
- AMM primitive is over-represented in winners (+4.07% share-delta) per analyze data
- 5ive already has a battle-tested x*y=k AMM (`5ive-amm/` — 289 lines, verified on-chain)

---

## Architecture

### Three programs
1. **LPPerp Wrapper** (5ive DSL) — market state, LP accounting, trade routing
2. **5ive AMM** (existing, reuse `5ive-amm/src/main.v`) — holds LP deposits, manages LP shares
3. **Percolator Engine** — perp risk math

### Per-market account
```v
account LpPerpMarket {
    amm_pool: pubkey;              // reuse existing 5ive AMM pool
    lp_share_mint: pubkey;          // reuse existing LP share token
    percolator_market_pda: pubkey;  // Percolator slab state
    active_positions: u32;
    total_funding_accrued: i64;
    fee_bps_swap: u16;              // paid to LPs
    fee_bps_perp: u16;              // split LPs / insurance / treasury
    insurance_pool: pubkey;
}
```

### Per-LP account
Reuse 5ive AMM's existing `LPAccount` struct. No LPPerp-specific user state needed — LPs are fungible.

### Trade routing flow
1. Trader calls `open_perp_position(user, market, notional, is_long)`
2. LPPerp wrapper reads AMM pool state for quote
3. Wrapper CPIs to Percolator `trade` with the AMM-derived price
4. Wrapper charges `fee_bps_perp` from trader's collateral
5. Wrapper distributes fee to LPs (pro-rata on `lp_share_mint` balances) + insurance fund
6. Funding accrual happens on `keeper_crank` per Percolator's standard flow

### LP withdrawal flow
1. LP calls `withdraw_lp(user, share_amount)`
2. Wrapper reads current AMM share ratio
3. Pulls underlying tokens from AMM proportional to share
4. Applies any pending funding debt (LPs net on funding; may owe)
5. Returns tokens to user

---

## Build scope (Week 2-3, ~6 engineer-days)

### Must-ship
- [ ] `init_lp_perp_market(amm_pool, fee_bps_swap, fee_bps_perp)`
- [ ] `deposit_lp(user, amount_a, amount_b)` — delegates to existing 5ive AMM
- [ ] `open_perp_position` — prices via AMM, CPIs Percolator `trade`
- [ ] `close_perp_position`
- [ ] `withdraw_lp(user, share_amount)` with pending-funding settlement
- [ ] `keeper_crank` — permissionless funding advancement
- [ ] MCP `get_lp_perp_market`, `get_lp_pnl(user)`
- [ ] Devnet deploy + LP earnings demo

### Nice-to-have
- [ ] Multi-pool support (one LPPerp market per AMM pool)
- [ ] LP dashboard showing funding+swap-fee accrual
- [ ] Impermanent-loss alerting
- [ ] Partner pool with Raydium / Orca for real liquidity

### Out of scope
- Dynamic fee adjustment (v2 feature)
- LP-vs-LP arbitrage prevention (use Percolator's standard guards)
- Cross-market LP (stick to one AMM pool per market for simplicity)

---

## Design decisions

### Why reuse 5ive's existing AMM
- 5ive AMM is already on mainnet (verified bytecode)
- LP share accounting is battle-tested
- Frees engineering time for the Percolator CPI layer
- Composability signal: "5ive primitives compose"

### Why Percolator (and not a custom perp)
- Percolator handles the hard risk math (A/K indices, haircuts, insurance fund)
- BYOM thesis: Percolator is the risk library; we bring the market
- Formally verified upstream — we inherit the safety guarantees

### Why passive MM for LPs
Professional MMs on Solana have $5-30K/mo infra costs (per Solana DEX/MM pain-scout research). Passive LP-backed perps open the MM role to anyone with an AMM position. 10× the potential LP base.

### Why fee split (LPs + insurance + treasury)
- LPs need yield to attract capital
- Insurance fund absorbs tail risk (Percolator requirement)
- Treasury funds ongoing development

Recommended split (adjustable post-launch): 70% LPs, 20% insurance, 10% treasury.

---

## Competitive positioning

| Project | Difference |
|---------|-----------|
| Maiker.fun | Spot concentrated liquidity vaults; not perps |
| Tyrbine | Single-sided impermanent-loss protected; not perp MM |
| LP Bot | Copy-trading Orca LPs; no perp composition |
| Drift's internal AMM | Drift-only; not a standalone primitive anyone can deploy |
| Jupiter JLP | JLP is a specific vault structure, not a generalizable AMM-as-slab |
| Hyperliquid HLP | Hyperliquid-only; single-chain lock-in |

**Niche:** AMM-backed passive perp MM on Solana as a reusable primitive — empty.

---

## Risk matrix

| Risk | Severity | Mitigation |
|------|---------|------------|
| LP-AMM + perp-risk math conflicts | HIGH | Separate concerns cleanly: AMM holds tokens, Percolator tracks risk, wrapper bridges |
| LPs get picked off by sophisticated perp traders | MEDIUM | Use Percolator's fee floor (min_abs) to absorb toxic flow |
| Funding rate math gets inverted by the AMM side | MEDIUM | Write PercolatorBench invariants SPECIFIC to this market; run before launch |
| Low liquidity = wide spreads = no volume | MEDIUM | Seed with Raydium/Orca partnership OR 5ive team provides initial LP |
| 5ive AMM can't handle the CPI load from Percolator calls | LOW | AMM primitive is stateless on the LP side; CPIs hit individual accounts |

---

## Demo moment

*"Show me what happens if all LPs withdraw at once during a perp funding spike"*

MCP-Claude queries the live market:
1. Current LP TVL: $X
2. Current outstanding perp positions: $Y
3. If all LPs withdraw: Percolator's insurance fund absorbs shortfall; LPs get haircut proportional to pending funding
4. Shows the actual math on screen

Reinforces "Percolator risk library + 5ive composition = safe BYOM" narrative.

---

## Open questions

1. What's the correct quote-function when the AMM has much lower liquidity than the perp position size? (Percolator has `g` haircut for this — confirm it composes correctly)
2. How do we handle the "LP withdraws while they owe funding" case — deny the withdrawal or allow with settlement?
3. Can two different LPPerp markets share the same AMM pool? (Probably no — each AMM pool is single-purpose for accounting simplicity)
4. Should we partner with an existing AMM (Raydium, Orca) or stay 5ive-native for the hackathon? (Stay 5ive-native — own the primitive end-to-end)

Resolve by start of Week 3.
