# MCP-Percolator — live-audit demo hook

**Status:** demo-day hero component. Distinguishes Perc5ive from every other hackathon submission.

---

## What it is

A Model Context Protocol (MCP) server that exposes Perc5ive's on-chain state as queryable tools to any AI assistant (Claude, Cursor, ChatGPT, GPT-Claude, Anthropic Console, custom agents).

Built on top of 5ive's existing `five-mcp/` infrastructure.

Allows a judge, investor, or community member to point an AI assistant at the live Perc5ive deployment and ask any question — from "what happens if this position is liquidated?" to "show me all active Sov markets" to "simulate a Pyth lag on Market X."

## Why it's the demo-day hero

- **Nobody else will have this.** 5ive is the only Colosseum submission with an MCP server for smart contract state. Copilot search confirmed.
- **Interactive demo** — judges type questions live on stage, AI responds with queryable data. Unprepared moment = maximum authenticity.
- **Mirrors Anatoly's workflow** — Toly explicitly mentioned using Claude to iterate on Percolator. Our demo says: *"you can now do the same thing on any Perc5ive market."*
- **Defensible** — competitors can't ship this in the remaining hackathon window; requires 5ive's existing MCP infrastructure.
- **Post-hackathon utility** — the same MCP endpoint supports investor demos, community analytics, and future Percolator-fork audits.

---

## Tool catalog (exposed via MCP)

### Read-only query tools

#### `list_perc5ive_markets()`
Returns all deployed Perc5ive markets on mainnet + devnet, with type (Sov / PythRace / LPPerp) and status.

Example response:
```json
[
  {"name": "Sov-BONK", "type": "sov", "memecoin": "DezXAZ...BONK", "admin_burned": true, "insurance_fund": 1200000, "position_count": 47, "network": "devnet"},
  {"name": "PythRace-SOL-60s-up", "type": "pyth_race", "feed": "SOL-USD", "window": 60, "active_markets": 3, "network": "devnet"},
  {"name": "LPPerp-USDC-SOL", "type": "lp_perp", "amm_pool": "...", "lp_tvl": 450000, "open_positions": 12, "network": "devnet"}
]
```

#### `get_sov_market(market_address)`
Returns full state of a specific Sov market: collateral vault balance, admin-burn state, insurance fund, active positions count, Pyth feed, current mark price.

#### `get_pyth_race_market(feed, market_id)`
Current state of a Pyth-race market: open slot, resolution slot, threshold, total long/short collateral, current Pyth price, time-to-resolution.

#### `get_lp_perp_market(market_address)`
LP TVL, open perp positions, funding rate, fee accrual per LP (current epoch), insurance fund balance.

#### `get_position(user, market_address)`
A specific user's position in a specific market. Collateral, size, entry price, current PnL, liquidation price.

#### `get_market_history(market_address, from_slot, to_slot)`
Event log replay for a market over a slot range.

### Simulation tools

#### `simulate_liquidation(user, market_address, price_change_bps)`
Dry-run: if the underlying price moved by `price_change_bps` from current, would this user be liquidated? If so, show the cascade.

Returns:
```json
{
  "user": "...",
  "market": "Sov-BONK",
  "price_change_bps": -3000,
  "new_mark_price": "...",
  "would_liquidate": true,
  "liquidation_sequence": [...],
  "insurance_fund_used": 12500,
  "residual_shortfall": 0
}
```

#### `simulate_pyth_race_resolution(market_id, final_price)`
Given a hypothetical resolution price, show who wins and what the payout allocation is.

#### `simulate_lp_withdraw(user, market_address, share_amount)`
If this LP withdraws now, what do they receive (net of pending funding debt)?

### Auditing tools

#### `explain_transaction(signature)`
Given a Perc5ive transaction signature, decode the instruction, show which Percolator engine call was made, and annotate the state transitions.

#### `run_percolator_bench_property(property_id, market_address)`
Run a single PercolatorBench property test against a live market. Returns pass/fail with diagnostic trace.

#### `compare_to_upstream(market_address)`
For a given Perc5ive market, show the mapping to Anatoly's Percolator spec. Confirm instruction-level parity.

---

## Demo-day script (3 minutes, for the Frontier pitch)

*The judges each have a browser open with Claude. Our URL is projected on screen.*

**[0-30s]** Founder: *"Anatoly said 'devs bring your own markets.' We built the toolkit + three markets. But more importantly — Anatoly said he uses Claude to iterate on Percolator."*

**[30-45s]** Founder: *"Open Claude. Point it at perc5ive.5ive.dev. Type literally anything."*

**[45s-2:00s]** Judge types: *"What happens if BONK drops 25% in the next hour?"*

Claude (via MCP):
- Lists all Sov-BONK positions
- Identifies which are liquidatable at BONK-25%
- Shows insurance fund usage
- Explains the cascade step-by-step

Judge types follow-up: *"And if all LPs withdraw at the same time on LPPerp-USDC-SOL?"*

Claude (via MCP):
- Calculates net LP exposure
- Shows funding settlement
- Demonstrates conservation holds

**[2:00-2:30s]** Founder: *"Every Perc5ive market is queryable. Every PercolatorBench property runs live. This is 'bring your own market' as Anatoly designed it — in 5ive, with MCP."*

**[2:30-3:00s]** Close: *"Three markets. Full Percolator conformance. MCP-live-audit. Percolator is the risk library. 5ive is how you use it. Sov is the first proof. Link in description."*

---

## Implementation plan

### Leverage existing 5ive infrastructure
- `five-mcp/` has the MCP server framework
- `cloudflare-workers/` in `five-mcp/` handles deployment
- `cloudflare-endpoint-example.json` as template
- Existing SDK tools can be extended

### New tools to build
Each tool is a TypeScript file following the existing `five-mcp/src/` pattern:
1. Read tool: queries on-chain state via `@solana/web3.js` + our program IDL
2. Simulation tool: uses 5ive DSL's dry-run / simulation capability OR replays in a sandboxed VM
3. Audit tool: cross-references spec properties against state

### Timeline
- Week 3 Day 1-2: infrastructure setup, existing MCP tool review
- Week 3 Day 3-4: read tools (list + get) for all three markets
- Week 3 Day 5: simulation tools (liquidation + pyth-race + LP-withdraw)
- Week 3 Day 6-7: audit tools + PercolatorBench integration
- Week 4 Day 1-2: demo rehearsal — test with real judges via Solana Tech Discord

---

## Deployment

### Target URLs
- Public demo: `perc5ive.5ive.dev` (Cloudflare Workers)
- Developer docs: `perc5ive.5ive.dev/docs` (mirror of MCP tool catalog)
- Health check: `perc5ive.5ive.dev/health`

### Authentication
- Public tools: no auth (anyone can query public on-chain state)
- Simulation tools: rate-limited but no auth
- Audit tools: API key required (prevents spam against PercolatorBench)

### Monitoring
- Uptime via existing 5ive monitoring
- Query logs (anonymized) for post-hackathon insights on what judges/investors actually ask

---

## Risks + mitigations

| Risk | Severity | Mitigation |
|------|---------|------------|
| Demo fails on stage (network issue, worker cold-start) | HIGH | Pre-warm workers 30 min before demo; have offline fallback video of the demo |
| Claude Web / whatever AI the judge uses rejects the MCP call | MEDIUM | Provide a curl fallback showing the same response; also keep a Cursor/local-agent backup |
| Judge asks a question outside our tool catalog | MEDIUM | Train the MCP description on "I can query Sov/PythRace/LPPerp markets and simulate risk scenarios"; graceful fallback messages |
| MCP protocol changes before demo | LOW | Lock to a specific MCP version; test against Claude Desktop + Cursor + web-Claude |
| Data inconsistency between on-chain state and our cached response | LOW | Zero caching for read tools (always live-query); cache only PercolatorBench reports |

---

## Post-hackathon roadmap

- Public read API (beyond MCP) for community dashboards
- PercolatorBench-as-a-service — any Percolator fork submits for conformance report
- Integration with Squads multisig for treasury-side risk queries
- White-label version for other Solana projects to expose their own state via MCP

---

## Open questions

1. Should the MCP server expose ALL three markets or just Sov for the demo (clearer narrative)?
2. Can we have the MCP server pre-load common questions as few-shot examples to make judges' first query more likely to succeed?
3. Is there a Claude-native way to embed the tool catalog in the MCP response so judges can discover what questions to ask?

Resolve during Week 3.
