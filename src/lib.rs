//! Perc5ive — Percolator risk engine ported to 5iveVM.
//!
//! Following the Path 3 decision in `MULTIPRECISION_DSL_DECISION.md`, the port is
//! split across two source forms:
//!
//! * **DSL source** (future work) — handles state layout, account lifecycle,
//!   instruction dispatch, and all u128-range arithmetic.
//! * **Hand-written bytecode** (this crate's `bytecode` module) — handles the
//!   u256 / i256 math that Percolator's `wide_math.rs` provides in Rust, using
//!   the multiprecision opcodes (`0xC0-0xCD`) added in five-protocol#37 /
//!   five-vm-mito#84.
//!
//! For the hackathon timeline we prove the foundation here (emitter + e2e tests);
//! the full port is staged in `SESSION_STATE.md` Step 3.

#![deny(unsafe_code)]

pub mod bytecode;
