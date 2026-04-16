//! Hand-written bytecode modules for Perc5ive.
//!
//! Each submodule exposes functions that append opcode sequences to a [`Program`]
//! builder, which wraps a `Vec<u8>` and adds the 5IVE file header. The output is
//! a `.five`-compatible binary that [`five_vm_mito::MitoVM::execute_direct`] can
//! run directly.
//!
//! The u256 / i256 modules shadow Percolator's `wide_math.rs` operation-by-
//! operation so the Rust source stays readable next to the bytecode that
//! replaces it.

pub mod dsl_header;
pub mod emit;
pub mod handlers;
pub mod i128;
pub mod i256;
pub mod link;
pub mod u256;

pub use emit::Program;
pub use link::{AppendedFn, Linker};
