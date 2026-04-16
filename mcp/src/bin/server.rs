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
        other if catalogue().iter().any(|t| t.name == other) => Ok(text_tool_result(format!(
            "Tool '{other}' is in the catalogue but not yet wired to a live data \
             source. Tracked as follow-up work in the next-session plan; the \
             schema is exposed so clients can discover the surface area now."
        ))),
        unknown => Err(invalid_params(&format!(
            "unknown tool '{unknown}' — call tools/list to enumerate"
        ))),
    }
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
