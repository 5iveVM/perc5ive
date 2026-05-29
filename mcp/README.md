# mcp-perc5ive — MCP server exposing Perc5ive state to AI assistants

Built on 5ive's existing `five-mcp/` infrastructure. Surfaces every Perc5ive market's on-chain state and Percolator's risk-engine primitives as queryable MCP tools, plus a set of simulation endpoints that let an AI assistant iterate on a market without touching mainnet.

## Why this ships

The MCP layer is the **demo-day hero** of the Perc5ive submission — it's the thing judges interact with live on stage. No other Colosseum submission has an MCP for smart-contract state, and 5ive's existing MCP infrastructure means we can ship this in days (not weeks) without reinventing the transport.

## Tool catalogue

### Read-only queries

| Tool | Purpose |
|---|---|
| `list_perc5ive_markets()` | Enumerate every deployed Sov / PythRaceMarket / LPPerp instance |
| `get_sov_market(address)` | Full state of a Sov market (collateral vault, insurance fund, admin-burn state, active positions) |
| `get_pyth_race_market(address)` | Race-specific state (start/end slots, start/end Pyth prices, stakes on each side, resolution status) |
| `get_lp_perp_market(address)` | LP TVL, open perp positions, funding rate, fee accrual |
| `get_position(user, market)` | A user's specific position — collateral, size, entry price, current PnL, liquidation price |
| `get_market_history(market, from_slot, to_slot)` | Event-log replay for the slot range |
| `get_insurance_fund(market)` | Current size + contribution history |
| `get_oracle_state(market)` | Latest Pyth / pool-state oracle update, staleness, price ranges |

### Percolator-level primitives

| Tool | Purpose |
|---|---|
| `get_risk_engine_state(address)` | Full RiskEngine snapshot (vault, insurance_fund, c_tot, open_account_count, current_funding_rate_e9, market_mode) |
| `get_margin_account(address)` | Full MarginAccount snapshot (capital, pnl, reserved_pnl, position_basis_q, fee_credits, sched/pending buckets) |
| `list_margin_accounts(risk_engine)` | Every MarginAccount tied to a specific RiskEngine |

### Simulation endpoints

| Tool | Purpose |
|---|---|
| `simulate_liquidation(user, market, oracle_price_override)` | Dry-run liquidation at the given oracle price; returns the cascade (flat account? insurance fund deduction?) |
| `simulate_trade(market, taker, maker, size_q, exec_price)` | Dry-run execute_trade; returns the post-state diff on both accounts and the RiskEngine |
| `simulate_crank(market, proposed_slot, proposed_oracle_price, proposed_funding_rate_e9)` | Dry-run keeper_crank; returns the ADL basis update, funding accrual, and any liquidations triggered |
| `project_pnl(user, market, price_path)` | Given an array of `(slot, oracle_price)` tuples, project the user's PnL trajectory |
| `explain_risk_math(margin_account, oracle_price)` | Annotate every step of the spec's §8 equity computation for that account at that price — exposes the u256 intermediate values |

### Write (devnet-only)

| Tool | Purpose |
|---|---|
| `deploy_sov_market(memecoin_mint, insurance_seed)` | Spins up a fresh Sov market on devnet |
| `burn_sov_admin(market)` | Renounces admin on an existing Sov instance |
| `execute_trade_devnet(market, taker, maker, size_q, exec_price)` | Real on-chain execute_trade on devnet |

## Transport

Runs as a standard MCP server over stdio and HTTP/SSE via 5ive's `five-mcp/` adapter. Clients:
- Claude Desktop / Claude Code (via MCP config)
- Cursor (via `mcp.json`)
- Anthropic Console custom tools
- Any OpenAI-compatible agent via the MCP HTTP bridge

## Live demo flow

1. Judge asks: *"Show me all active Perc5ive markets on devnet."*
   → Agent calls `list_perc5ive_markets()`, streams the list.

2. Judge drills in: *"Explain the risk math for user `XYZ` on the Sov-BONK market at an oracle price of $0.000015."*
   → Agent calls `explain_risk_math`, walks through the u256 intermediate values — showing both the bytecode-side magnitudes and the Rust-reference cross-check.

3. Judge tests an edge: *"Simulate a liquidation if the BONK/USD price halves."*
   → Agent calls `simulate_liquidation`, returns the cascade including which accounts would be forced out and what the insurance fund would pay.

4. Judge pivots: *"Deploy a fresh Sov market backed by `$PENGU`, and burn the admin key."*
   → Agent calls `deploy_sov_market` then `burn_sov_admin` — live on devnet, result visible on explorer.

The point is that the judge never has to leave their conversation with the assistant. The entire interaction looks like talking to a senior perps quant with complete knowledge of the deployed system.

## Why Anatoly in particular

@aeyakovenko has publicly discussed using Claude to iterate on Percolator. The MCP demo says *"Toly, we built the tool that makes this loop 10× faster for anyone forking Percolator"* — concrete, specific, and leverages the one developer relationship most likely to move the needle on the submission.

## Implementation status

* **MCP infrastructure**: hand-rolled stdio JSON-RPC 2.0 server in `src/bin/server.rs` (~200 LOC, no SDK dep). `initialize`, `tools/list`, `tools/call` all live.
* **Tool registrations**: full 24-tool catalogue surfaces via `tools/list` (8 read-only, 3 Percolator-primitive, 10 simulation, 3 devnet-write).
* **Wired handlers**: `list_perc5ive_markets` returns the four live devnet program IDs (perc5ive engine + sov + pyth_race + lp_perp; see `../DEVNET.md`). Ten simulation tools compute real answers (below); devnet read/write tools respond with a structured envelope pending the embedded RPC + signer path.
* **Simulation backend**: the risk-engine `simulate_*` / `project_pnl` / `explain_risk_math` tools run the perc5ive bytecode references; the percolator-meta genesis tools — `simulate_genesis_vote`, `simulate_kickstart_split`, `project_coin_distribution`, `explain_futarchy_lifecycle`, `simulate_rent_audit` — compute against `perc5ive::bytecode::meta_math`, the same references the genesis handler bytecode conforms against. Zero on-chain cost, fully deterministic.
* **Deploy tools**: `deploy_*` tools will wrap `scripts/deploy.sh` + `five execute`; credentials are devnet-only and gated by a session-level auth token. Pending wire-up.

## Running it

```
cargo build -p mcp-perc5ive --release
./target/release/mcp-perc5ive
```

Smoke test (stdin → stdout):

```
(echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'; \
 echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_perc5ive_markets","arguments":{"network":"devnet"}}}') \
| ./target/release/mcp-perc5ive
```

To wire it into Claude Code, add to `~/.claude/mcp_servers.json`:

```json
{
  "perc5ive": {
    "command": "/absolute/path/to/perc5ive/mcp/target/release/mcp-perc5ive",
    "args": []
  }
}
```
