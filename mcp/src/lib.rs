//! mcp-perc5ive — MCP server surfacing Perc5ive state to AI assistants.
//!
//! This crate is the **tool schema + simulation** surface of the MCP.
//! Transport (stdio / HTTP / SSE) is provided by 5ive's `five-mcp/`
//! adapter; this crate owns the tool catalogue and the Rust-backed
//! simulation endpoints that run `hello_slab/percolator` as a trusted
//! oracle.
//!
//! See `mcp/README.md` for the judge-facing demo flow and the strategic
//! rationale for shipping MCP as the Perc5ive demo-day hero component.

pub mod tools;

pub use tools::{catalogue, McpTool, ToolCategory};
