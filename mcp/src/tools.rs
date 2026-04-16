//! MCP tool schemas — one function per MCP tool, matching the catalogue in
//! `mcp/README.md`. Each `*_schema()` returns the JSON-Schema envelope that
//! the 5ive MCP adapter uses to advertise the tool to connecting clients.
//!
//! The `*_handler()` functions are left as `todo!()` stubs — they land when
//! Perc5ive ships to devnet. This file exists so the MCP surface is
//! code-complete and the JSON schemas are reviewable before the contract
//! layer lands.

#![allow(dead_code)]

use core::fmt;

/// Describes a single MCP tool.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: &'static str, // JSON Schema literal
    pub category: ToolCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    ReadOnly,
    PercolatorPrimitive,
    Simulation,
    WriteDevnet,
}

impl fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ToolCategory::ReadOnly => "read-only",
            ToolCategory::PercolatorPrimitive => "percolator-primitive",
            ToolCategory::Simulation => "simulation",
            ToolCategory::WriteDevnet => "write-devnet",
        })
    }
}

/// Full catalogue of MCP tools exported by the Perc5ive server.
pub fn catalogue() -> Vec<McpTool> {
    vec![
        // Read-only queries
        McpTool {
            name: "list_perc5ive_markets",
            description: "Enumerate every deployed Sov, PythRaceMarket, and LPPerp instance across the configured networks.",
            input_schema: r#"{ "type": "object", "properties": { "network": { "type": "string", "enum": ["mainnet", "devnet"] } } }"#,
            category: ToolCategory::ReadOnly,
        },
        McpTool {
            name: "get_sov_market",
            description: "Full state of a Sov market — collateral vault, admin-burn state, insurance fund, active positions.",
            input_schema: r#"{ "type": "object", "required": ["address"], "properties": { "address": { "type": "string" } } }"#,
            category: ToolCategory::ReadOnly,
        },
        McpTool {
            name: "get_pyth_race_market",
            description: "Race-specific state: start/end slots, start/end Pyth prices, stakes per side, resolution status.",
            input_schema: r#"{ "type": "object", "required": ["address"], "properties": { "address": { "type": "string" } } }"#,
            category: ToolCategory::ReadOnly,
        },
        McpTool {
            name: "get_lp_perp_market",
            description: "LP TVL, open perp positions, funding rate, fee accrual across the current epoch.",
            input_schema: r#"{ "type": "object", "required": ["address"], "properties": { "address": { "type": "string" } } }"#,
            category: ToolCategory::ReadOnly,
        },
        McpTool {
            name: "get_position",
            description: "A user's specific position — collateral, size, entry price, current PnL, liquidation price.",
            input_schema: r#"{ "type": "object", "required": ["user", "market"], "properties": { "user": { "type": "string" }, "market": { "type": "string" } } }"#,
            category: ToolCategory::ReadOnly,
        },
        McpTool {
            name: "get_market_history",
            description: "Event-log replay for a market over a slot range.",
            input_schema: r#"{ "type": "object", "required": ["market", "from_slot", "to_slot"], "properties": { "market": { "type": "string" }, "from_slot": { "type": "integer" }, "to_slot": { "type": "integer" } } }"#,
            category: ToolCategory::ReadOnly,
        },
        McpTool {
            name: "get_insurance_fund",
            description: "Current insurance-fund size and top-up / drain history.",
            input_schema: r#"{ "type": "object", "required": ["market"], "properties": { "market": { "type": "string" } } }"#,
            category: ToolCategory::ReadOnly,
        },
        McpTool {
            name: "get_oracle_state",
            description: "Latest oracle update, staleness, and price distribution over the last N slots.",
            input_schema: r#"{ "type": "object", "required": ["market"], "properties": { "market": { "type": "string" }, "lookback_slots": { "type": "integer", "default": 1000 } } }"#,
            category: ToolCategory::ReadOnly,
        },
        // Percolator primitives
        McpTool {
            name: "get_risk_engine_state",
            description: "Full RiskEngine snapshot — vault, insurance_fund, c_tot, open_account_count, current_funding_rate_e9, market_mode.",
            input_schema: r#"{ "type": "object", "required": ["address"], "properties": { "address": { "type": "string" } } }"#,
            category: ToolCategory::PercolatorPrimitive,
        },
        McpTool {
            name: "get_margin_account",
            description: "Full MarginAccount snapshot — capital, pnl, reserved_pnl, position_basis_q, fee_credits, sched/pending buckets.",
            input_schema: r#"{ "type": "object", "required": ["address"], "properties": { "address": { "type": "string" } } }"#,
            category: ToolCategory::PercolatorPrimitive,
        },
        McpTool {
            name: "list_margin_accounts",
            description: "Every MarginAccount tied to a specific RiskEngine.",
            input_schema: r#"{ "type": "object", "required": ["risk_engine"], "properties": { "risk_engine": { "type": "string" } } }"#,
            category: ToolCategory::PercolatorPrimitive,
        },
        // Simulation
        McpTool {
            name: "simulate_liquidation",
            description: "Dry-run liquidation at the given oracle price. Returns cascade: flat account? insurance fund deduction? residual?",
            input_schema: r#"{ "type": "object", "required": ["user", "market", "oracle_price"], "properties": { "user": { "type": "string" }, "market": { "type": "string" }, "oracle_price": { "type": "integer" } } }"#,
            category: ToolCategory::Simulation,
        },
        McpTool {
            name: "simulate_trade",
            description: "Dry-run execute_trade. Returns the post-state diff on taker, maker, and the RiskEngine.",
            input_schema: r#"{ "type": "object", "required": ["market", "taker", "maker", "size_q", "exec_price"], "properties": { "market": { "type": "string" }, "taker": { "type": "string" }, "maker": { "type": "string" }, "size_q": { "type": "integer" }, "exec_price": { "type": "integer" } } }"#,
            category: ToolCategory::Simulation,
        },
        McpTool {
            name: "simulate_crank",
            description: "Dry-run keeper_crank. Returns ADL basis update, funding accrual, and liquidations triggered.",
            input_schema: r#"{ "type": "object", "required": ["market", "proposed_slot", "proposed_oracle_price", "proposed_funding_rate_e9"], "properties": { "market": { "type": "string" }, "proposed_slot": { "type": "integer" }, "proposed_oracle_price": { "type": "integer" }, "proposed_funding_rate_e9": { "type": "integer" } } }"#,
            category: ToolCategory::Simulation,
        },
        McpTool {
            name: "project_pnl",
            description: "Project PnL trajectory across a (slot, oracle_price) path.",
            input_schema: r#"{ "type": "object", "required": ["user", "market", "price_path"], "properties": { "user": { "type": "string" }, "market": { "type": "string" }, "price_path": { "type": "array", "items": { "type": "object", "properties": { "slot": { "type": "integer" }, "oracle_price": { "type": "integer" } } } } } }"#,
            category: ToolCategory::Simulation,
        },
        McpTool {
            name: "explain_risk_math",
            description: "Annotate every step of §8 equity computation for that account at that price — exposes u256 intermediate values and the bytecode-vs-reference cross-check.",
            input_schema: r#"{ "type": "object", "required": ["margin_account", "oracle_price"], "properties": { "margin_account": { "type": "string" }, "oracle_price": { "type": "integer" } } }"#,
            category: ToolCategory::Simulation,
        },
        // Write (devnet-only)
        McpTool {
            name: "deploy_sov_market",
            description: "Spin up a fresh Sov market on devnet with the specified memecoin collateral and insurance seed.",
            input_schema: r#"{ "type": "object", "required": ["memecoin_mint", "insurance_seed"], "properties": { "memecoin_mint": { "type": "string" }, "insurance_seed": { "type": "integer" } } }"#,
            category: ToolCategory::WriteDevnet,
        },
        McpTool {
            name: "burn_sov_admin",
            description: "Renounce admin on an existing Sov instance. Irreversible.",
            input_schema: r#"{ "type": "object", "required": ["market"], "properties": { "market": { "type": "string" } } }"#,
            category: ToolCategory::WriteDevnet,
        },
        McpTool {
            name: "execute_trade_devnet",
            description: "Real on-chain execute_trade on devnet.",
            input_schema: r#"{ "type": "object", "required": ["market", "taker", "maker", "size_q", "exec_price"], "properties": { "market": { "type": "string" }, "taker": { "type": "string" }, "maker": { "type": "string" }, "size_q": { "type": "integer" }, "exec_price": { "type": "integer" } } }"#,
            category: ToolCategory::WriteDevnet,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_nonempty_and_unique() {
        let cat = catalogue();
        assert!(!cat.is_empty());
        let mut names: Vec<&str> = cat.iter().map(|t| t.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), cat.len(), "duplicate tool names in catalogue");
    }

    #[test]
    fn catalogue_has_at_least_one_tool_per_category() {
        let cat = catalogue();
        for category in [
            ToolCategory::ReadOnly,
            ToolCategory::PercolatorPrimitive,
            ToolCategory::Simulation,
            ToolCategory::WriteDevnet,
        ] {
            assert!(
                cat.iter().any(|t| t.category == category),
                "no tools in category {}",
                category
            );
        }
    }

    #[test]
    fn input_schemas_are_nonempty() {
        for t in catalogue() {
            assert!(!t.input_schema.is_empty(), "empty schema for {}", t.name);
        }
    }

    #[test]
    fn descriptions_are_nonempty() {
        for t in catalogue() {
            assert!(!t.description.is_empty(), "empty description for {}", t.name);
        }
    }
}
