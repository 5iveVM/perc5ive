//! Minimal stdio MCP server for perc5ive.
//!
//! Speaks JSON-RPC 2.0 over stdin/stdout per the Model Context Protocol
//! handshake. Implements three core methods:
//!
//!   * `initialize`     — capability negotiation
//!   * `tools/list`     — return the catalogue from `mcp_perc5ive::catalogue()`
//!   * `tools/call`     — dispatch to a handler; only `list_perc5ive_markets`
//!                         is wired today (returns the 4 live devnet program
//!                         IDs from DEVNET.md). The remaining 18 schemas
//!                         respond with a structured "not yet implemented"
//!                         envelope so MCP clients see them in the catalogue
//!                         but get an honest answer when invoked.
//!
//! No external MCP SDK — JSON-RPC over stdio is small enough to roll directly
//! and avoids pulling in a moving dependency. Add an SDK if/when the catalogue
//! grows past the size where hand-routing becomes painful.

use std::io::{self, BufRead, Write};

use mcp_perc5ive::{catalogue, McpTool};
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "mcp-perc5ive";
const SERVER_VERSION: &str = "0.1.0";

/// Hard-coded devnet program IDs (mirrors `DEVNET.md` in the repo root).
/// Single source of truth for now; once we add a `markets.json` registry
/// this should pull from disk.
const DEVNET_MARKETS: &[(&str, &str, &str)] = &[
    (
        "perc5ive-engine",
        "engine",
        "2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK",
    ),
    (
        "sov-anchor",
        "sov",
        "2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV",
    ),
    (
        "pyth-race-anchor",
        "pyth_race",
        "5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i",
    ),
    (
        "lp-perp-anchor",
        "lp_perp",
        "DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp",
    ),
];

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                respond(&mut stdout, json!(null), Err(parse_error()))?;
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let result = dispatch(method, params);
        respond(&mut stdout, id, result)?;
    }
    Ok(())
}

fn dispatch(method: &str, params: Value) -> Result<Value, JsonRpcError> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        })),
        "tools/list" => Ok(json!({
            "tools": catalogue().iter().map(tool_to_listing).collect::<Vec<_>>(),
        })),
        "tools/call" => call_tool(params),
        // No-op for notifications the client may send during handshake.
        "notifications/initialized" | "ping" => Ok(Value::Null),
        _ => Err(method_not_found(method)),
    }
}

fn call_tool(params: Value) -> Result<Value, JsonRpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("missing tool 'name'"))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    match name {
        "list_perc5ive_markets" => Ok(text_tool_result(handle_list_markets(arguments))),
        "simulate_liquidation" => Ok(text_tool_result(handle_simulate_liquidation(arguments))),
        "simulate_trade" => Ok(text_tool_result(handle_simulate_trade(arguments))),
        "simulate_crank" => Ok(text_tool_result(handle_simulate_crank(arguments))),
        "project_pnl" => Ok(text_tool_result(handle_project_pnl(arguments))),
        "explain_risk_math" => Ok(text_tool_result(handle_explain_risk_math(arguments))),
        "get_risk_engine_state" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_risk_engine_state",
            arguments,
        ))),
        "get_margin_account" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_margin_account",
            arguments,
        ))),
        "list_margin_accounts" => Ok(text_tool_result(handle_devnet_read_stub(
            "list_margin_accounts",
            arguments,
        ))),
        "get_sov_market" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_sov_market",
            arguments,
        ))),
        "get_pyth_race_market" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_pyth_race_market",
            arguments,
        ))),
        "get_lp_perp_market" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_lp_perp_market",
            arguments,
        ))),
        "get_position" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_position",
            arguments,
        ))),
        "get_market_history" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_market_history",
            arguments,
        ))),
        "get_insurance_fund" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_insurance_fund",
            arguments,
        ))),
        "get_oracle_state" => Ok(text_tool_result(handle_devnet_read_stub(
            "get_oracle_state",
            arguments,
        ))),
        other if catalogue().iter().any(|t| t.name == other) => Ok(text_tool_result(format!(
            "Tool '{other}' is a devnet-write operation and requires a signed \
             transaction path that is not wired to this stdio server. Use the \
             perc5ive scripts in `scripts/` to invoke write ops against the \
             deployed programs."
        ))),
        unknown => Err(invalid_params(&format!(
            "unknown tool '{unknown}' — call tools/list to enumerate"
        ))),
    }
}

fn handle_simulate_liquidation(args: Value) -> String {
    let user = args.get("user").and_then(Value::as_str).unwrap_or("<unknown>");
    let market = args
        .get("market")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let oracle_price = args
        .get("oracle_price")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let capital = args.get("capital").and_then(Value::as_u64).unwrap_or(1_000_000_000_000);
    let liq_fee_bps = args.get("liquidation_fee_bps").and_then(Value::as_u64).unwrap_or(50);

    // Match handler_body_liquidate_at_oracle semantics: fee = capital * bps / 10_000,
    // insurance_fund gets the fee, liquidator gets (capital - fee).
    let capital = capital as u128;
    let fee = capital.saturating_mul(liq_fee_bps as u128) / 10_000;
    let liquidator_credit = capital.saturating_sub(fee);

    format!(
        "Liquidation simulation (perc5ive::handler_body_liquidate_at_oracle)\n\
         \n\
         user:              {user}\n\
         market:            {market}\n\
         oracle_price:      {oracle_price}\n\
         input capital:     {capital}\n\
         liq_fee_bps:       {liq_fee_bps}\n\
         \n\
         Predicted state transition:\n\
         - fee moved to risk.insurance_fund:  {fee}\n\
         - credit to liquidator.reserved_pnl: {liquidator_credit}\n\
         - victim.capital:                    0\n\
         - victim.position_basis_q:           0\n\
         - victim.reserved_pnl:               0\n\
         - risk.open_account_count:           -1\n\
         \n\
         Fee is clamped to min(raw_fee, risk.liquidation_fee_cap) at runtime.\n\
         Source: `perc5ive::bytecode::handlers::handler_body_liquidate_at_oracle`"
    )
}

fn handle_simulate_trade(args: Value) -> String {
    let market = args
        .get("market")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let taker = args
        .get("taker")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let maker = args
        .get("maker")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let size_q = args.get("size_q").and_then(Value::as_i64).unwrap_or(0);
    let exec_price = args.get("exec_price").and_then(Value::as_u64).unwrap_or(0);
    let oracle_price = args.get("oracle_price").and_then(Value::as_u64).unwrap_or(exec_price);
    let trading_fee = args.get("trading_fee_override").and_then(Value::as_i64).unwrap_or(0);
    let initial_taker_pos = args.get("initial_taker_pos").and_then(Value::as_i64).unwrap_or(0) as i128;
    let initial_maker_pos = args.get("initial_maker_pos").and_then(Value::as_i64).unwrap_or(0) as i128;

    // Apply handler_body_execute_trade semantics:
    let new_taker_pos = initial_taker_pos.wrapping_add(size_q as i128);
    let new_maker_pos = initial_maker_pos.wrapping_sub(size_q as i128);
    let price_delta = (oracle_price as i128).wrapping_sub(exec_price as i128);
    let pnl_delta = price_delta.wrapping_mul(size_q as i128);
    let taker_fee_delta: i128 = -(trading_fee as i128);
    let maker_fee_delta: i128 = trading_fee as i128;

    // OI-bound check (|new_pos| <= MAX_POSITION_ABS_Q = 1e14).
    const MAX_POSITION_ABS_Q: i128 = 100_000_000_000_000;
    let taker_within_bounds = new_taker_pos.unsigned_abs() <= MAX_POSITION_ABS_Q as u128;
    let maker_within_bounds = new_maker_pos.unsigned_abs() <= MAX_POSITION_ABS_Q as u128;

    format!(
        "Trade simulation (perc5ive::handler_body_execute_trade, Full scope)\n\
         \n\
         market:    {market}\n\
         taker:     {taker}\n\
         maker:     {maker}\n\
         size_q:    {size_q}\n\
         exec:      {exec_price}\n\
         oracle:    {oracle_price}\n\
         fee ovr:   {trading_fee}\n\
         \n\
         Position updates:\n\
         - taker.position_basis_q: {initial_taker_pos} -> {new_taker_pos}\n\
         - maker.position_basis_q: {initial_maker_pos} -> {new_maker_pos}\n\
         \n\
         PnL updates (i128, polymorphic chain):\n\
         - price_delta = oracle - exec = {price_delta}\n\
         - pnl_delta   = price_delta * size_q = {pnl_delta}\n\
         - taker.pnl += {pnl_delta}\n\
         - maker.pnl -= {pnl_delta}\n\
         \n\
         Fee updates:\n\
         - taker.fee_credits: {taker_fee_delta:+} (paid)\n\
         - maker.fee_credits: {maker_fee_delta:+} (rebated)\n\
         \n\
         Post-trade OI bounds (spec §4.6 / MAX_POSITION_ABS_Q = 1e14):\n\
         - taker within bounds: {taker_within_bounds}\n\
         - maker within bounds: {maker_within_bounds}\n\
         Trade aborts with status 1 if either is false.\n\
         \n\
         Source: `perc5ive::bytecode::handlers::handler_body_execute_trade`"
    )
}

fn handle_simulate_crank(args: Value) -> String {
    let market = args.get("market").and_then(Value::as_str).unwrap_or("<unknown>");
    let proposed_slot = args.get("proposed_slot").and_then(Value::as_u64).unwrap_or(0);
    let oracle = args.get("proposed_oracle_price").and_then(Value::as_u64).unwrap_or(0);
    let funding = args.get("proposed_funding_rate_e9").and_then(Value::as_i64).unwrap_or(0) as i128;
    let current_slot = args.get("current_slot").and_then(Value::as_u64).unwrap_or(0);

    let monotonic_ok = proposed_slot >= current_slot;
    let advances_last = proposed_slot > current_slot;

    format!(
        "Keeper-crank simulation (perc5ive::handler_body_keeper_crank, Full scope)\n\
         \n\
         market:         {market}\n\
         proposed slot:  {proposed_slot}\n\
         current slot:   {current_slot}\n\
         oracle price:   {oracle}\n\
         funding_rate_e9: {funding}\n\
         \n\
         Precondition checks:\n\
         - time monotonicity (now_slot >= current_slot): {monotonic_ok}\n\
         \n\
         If accepted, this crank will:\n\
         - risk.current_slot = {proposed_slot}\n\
         - risk.current_oracle_price = {oracle}\n\
         - risk.current_funding_rate_e9 = {funding}\n\
         - risk.cumulative_funding_e9 += sign-extend(funding_rate_e9) (i256 accrual)\n\
         - risk.last_funding_update_slot = max(current, {proposed_slot}) (advanced: {advances_last})\n\
         \n\
         Batch-liquidation loop over `ordered_candidates` is DSL-side future\n\
         work; single-account liquidation path is Full today.\n\
         \n\
         Source: `perc5ive::bytecode::handlers::handler_body_keeper_crank`"
    )
}

fn handle_project_pnl(args: Value) -> String {
    let user = args.get("user").and_then(Value::as_str).unwrap_or("<unknown>");
    let market = args.get("market").and_then(Value::as_str).unwrap_or("<unknown>");
    let position = args.get("position_basis_q").and_then(Value::as_i64).unwrap_or(0) as i128;
    let entry = args.get("entry_price").and_then(Value::as_u64).unwrap_or(0);
    let path = args.get("price_path").and_then(Value::as_array).cloned().unwrap_or_default();

    let mut out = format!(
        "PnL trajectory (perc5ive::bytecode::handlers::execute_trade PnL formula)\n\
         \n\
         user:              {user}\n\
         market:            {market}\n\
         position_basis_q:  {position}\n\
         entry_price:       {entry}\n\
         \n\
         Slot       OraclePrice    PnL (i128)\n\
         ---------- -------------- ---------------\n"
    );
    for point in &path {
        let slot = point.get("slot").and_then(Value::as_u64).unwrap_or(0);
        let price = point.get("oracle_price").and_then(Value::as_u64).unwrap_or(0);
        let pnl = (price as i128 - entry as i128).wrapping_mul(position);
        out.push_str(&format!("{slot:<10} {price:<14} {pnl:<15}\n"));
    }
    out.push_str(
        "\n\
         Note: this is a marked-to-market projection assuming no fees,\n\
         funding, or liquidations. Real post-settle state folds in\n\
         cumulative_funding_e9 and fee_credits — see simulate_trade.\n",
    );
    out
}

fn handle_explain_risk_math(args: Value) -> String {
    let margin = args
        .get("margin_account")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let oracle = args.get("oracle_price").and_then(Value::as_u64).unwrap_or(0);
    let capital = args.get("capital").and_then(Value::as_u64).unwrap_or(0);
    let pnl = args.get("pnl").and_then(Value::as_i64).unwrap_or(0);
    let position = args.get("position_basis_q").and_then(Value::as_i64).unwrap_or(0) as i128;
    let initial_margin_bps = args
        .get("initial_margin_bps")
        .and_then(Value::as_u64)
        .unwrap_or(500);

    // Spec §8: equity = capital + pnl; notional = |position| * oracle / POS_SCALE;
    // IM requirement = max(notional * im_bps / 10_000, min_nonzero_im_req).
    const POS_SCALE: u128 = 1_000_000;
    let notional = position.unsigned_abs().saturating_mul(oracle as u128) / POS_SCALE;
    let equity = capital as i128 + pnl as i128;
    let im_req = notional.saturating_mul(initial_margin_bps as u128) / 10_000;
    let free_collateral = equity - im_req as i128;
    let health = if im_req == 0 {
        "flat / unrestricted".to_string()
    } else {
        format!("{}%", equity.saturating_mul(100) / im_req as i128)
    };

    format!(
        "Risk-math walkthrough (spec §8, equity + IM chain)\n\
         \n\
         margin_account:       {margin}\n\
         oracle_price:         {oracle}\n\
         capital:              {capital}\n\
         pnl:                  {pnl}\n\
         position_basis_q:     {position}\n\
         initial_margin_bps:   {initial_margin_bps}\n\
         \n\
         Computation:\n\
         1. notional = |position| * oracle / POS_SCALE\n\
            = |{position}| * {oracle} / {POS_SCALE}\n\
            = {notional}\n\
         2. equity = capital + pnl\n\
            = {capital} + {pnl}\n\
            = {equity}\n\
         3. im_req = notional * im_bps / 10_000\n\
            = {notional} * {initial_margin_bps} / 10_000\n\
            = {im_req}\n\
         4. free_collateral = equity - im_req = {free_collateral}\n\
         5. health ratio:     {health}\n\
         \n\
         All arithmetic matches the wide_signed_mul_div_floor bytecode\n\
         (perc5ive::bytecode::i256::program_wide_signed_mul_div_floor_return_limb)\n\
         which is the conformance oracle for every equity-chain computation."
    )
}

fn handle_devnet_read_stub(tool: &str, arguments: Value) -> String {
    format!(
        "Tool '{tool}' is a devnet-read operation that queries live account\n\
         state from a connected Solana RPC. This stdio MCP server doesn't\n\
         carry an embedded RPC client yet; the simulation tools above\n\
         (simulate_trade / simulate_liquidation / simulate_crank /\n\
         project_pnl / explain_risk_math) are the pure-compute equivalents\n\
         that work without any RPC.\n\
         \n\
         Arguments received:\n\
         {arguments}\n\
         \n\
         To read live state: use the `scripts/deploy.sh` deploy output +\n\
         `solana account <address>` against the DEVNET.md program IDs."
    )
}

fn handle_list_markets(arguments: Value) -> String {
    let network = arguments
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("devnet");
    if network != "devnet" {
        return format!(
            "perc5ive is only deployed on devnet today. Mainnet deploy is a \
             post-submission stretch goal. Requested network: '{network}'."
        );
    }
    let mut out = String::from("Live perc5ive deployments on Solana devnet:\n\n");
    for (label, kind, program_id) in DEVNET_MARKETS {
        out.push_str(&format!(
            "  {label:<22} ({kind:<10}) {program_id}\n"
        ));
    }
    out.push_str(
        "\nExplorer prefix: https://explorer.solana.com/address/<id>?cluster=devnet\n",
    );
    out
}

fn tool_to_listing(t: &McpTool) -> Value {
    let schema: Value = serde_json::from_str(t.input_schema).unwrap_or_else(|_| json!({}));
    json!({
        "name": t.name,
        "description": t.description,
        "inputSchema": schema,
    })
}

fn text_tool_result(text: String) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
    })
}

#[derive(Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
}

fn parse_error() -> JsonRpcError {
    JsonRpcError {
        code: -32700,
        message: "Parse error".into(),
    }
}

fn method_not_found(method: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32601,
        message: format!("Method not found: {method}"),
    }
}

fn invalid_params(detail: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Invalid params: {detail}"),
    }
}

fn respond(
    out: &mut impl Write,
    id: Value,
    result: Result<Value, JsonRpcError>,
) -> io::Result<()> {
    let envelope = match result {
        Ok(v) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": e.code, "message": e.message },
        }),
    };
    writeln!(out, "{}", envelope)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulate_trade_produces_symmetric_position_updates() {
        let args = json!({
            "market": "test",
            "taker": "T",
            "maker": "M",
            "size_q": 100,
            "exec_price": 500,
            "oracle_price": 550,
            "trading_fee_override": 3,
            "initial_taker_pos": 1000,
            "initial_maker_pos": -500,
        });
        let out = handle_simulate_trade(args);
        assert!(out.contains("taker.position_basis_q: 1000 -> 1100"));
        assert!(out.contains("maker.position_basis_q: -500 -> -600"));
        assert!(out.contains("pnl_delta   = price_delta * size_q = 5000"));
        assert!(out.contains("OI bounds"));
    }

    #[test]
    fn simulate_trade_flags_oi_bound_violation() {
        // size_q push taker well past MAX_POSITION_ABS_Q (1e14).
        let args = json!({
            "market": "test",
            "size_q": 200_000_000_000_000i64,
            "initial_taker_pos": 0,
            "initial_maker_pos": 0,
            "exec_price": 1,
            "oracle_price": 1,
        });
        let out = handle_simulate_trade(args);
        assert!(out.contains("taker within bounds: false"));
    }

    #[test]
    fn simulate_liquidation_computes_expected_fee_split() {
        let args = json!({
            "user": "V",
            "market": "test",
            "oracle_price": 100,
            "capital": 1_000_000u64,
            "liquidation_fee_bps": 50u64,
        });
        let out = handle_simulate_liquidation(args);
        // fee = 1_000_000 * 50 / 10_000 = 5_000
        assert!(out.contains("fee moved to risk.insurance_fund:  5000"));
        // credit = 1_000_000 - 5_000 = 995_000
        assert!(out.contains("credit to liquidator.reserved_pnl: 995000"));
    }

    #[test]
    fn simulate_crank_flags_time_monotonicity_violation() {
        let args = json!({
            "market": "test",
            "proposed_slot": 100u64,
            "current_slot": 150u64,
            "proposed_oracle_price": 42,
            "proposed_funding_rate_e9": 0,
        });
        let out = handle_simulate_crank(args);
        assert!(out.contains("time monotonicity (now_slot >= current_slot): false"));
    }

    #[test]
    fn project_pnl_enumerates_path() {
        let args = json!({
            "user": "U",
            "market": "M",
            "position_basis_q": 10,
            "entry_price": 100,
            "price_path": [
                { "slot": 1, "oracle_price": 105 },
                { "slot": 2, "oracle_price": 95 },
            ],
        });
        let out = handle_project_pnl(args);
        // (105 - 100) * 10 = 50; (95 - 100) * 10 = -50
        assert!(out.contains("50"));
        assert!(out.contains("-50"));
    }

    #[test]
    fn explain_risk_math_walks_the_equity_chain() {
        let args = json!({
            "margin_account": "A",
            "oracle_price": 1_000_000u64, // == POS_SCALE, so notional == |position|
            "capital": 10_000u64,
            "pnl": 500i64,
            "position_basis_q": 5_000i64,
            "initial_margin_bps": 500u64,
        });
        let out = handle_explain_risk_math(args);
        assert!(out.contains("equity = capital + pnl"));
        assert!(out.contains("notional = |position|"));
        // notional = 5000; im_req = 5000 * 500 / 10_000 = 250
        assert!(out.contains("= 250"));
    }

    #[test]
    fn list_markets_returns_devnet_deployments() {
        let args = json!({});
        let out = handle_list_markets(args);
        for (_, _, program_id) in DEVNET_MARKETS {
            assert!(out.contains(program_id));
        }
    }

    #[test]
    fn call_tool_routes_unknown_tool_to_error() {
        let params = json!({ "name": "not_a_real_tool" });
        let result = call_tool(params);
        assert!(result.is_err());
    }

    #[test]
    fn call_tool_routes_known_simulation_tool_to_handler() {
        let params = json!({
            "name": "simulate_trade",
            "arguments": {
                "market": "m",
                "taker": "t",
                "maker": "k",
                "size_q": 1,
                "exec_price": 100,
            }
        });
        let result = call_tool(params).expect("should route");
        let text = result.get("content").unwrap()[0].get("text").unwrap().as_str().unwrap();
        assert!(text.contains("Trade simulation"));
    }
}
