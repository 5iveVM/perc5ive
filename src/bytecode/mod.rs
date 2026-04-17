//! Hand-written bytecode modules for Perc5ive.
//!
//! Each submodule exposes functions that append opcode sequences to a [`Program`]
//! builder, which wraps a `Vec<u8>` and adds the 5IVE file header. The output is
//! a `.five`-compatible binary that [`five_vm_mito::MitoVM::execute_direct`] can
//! run directly.
//!
//! Percolator's wide math runs on mono's polymorphic `ADD`/`SUB`/`MUL`/`DIV`
//! opcodes, which auto-promote between `U64` and `U128` and wrap at each
//! width — adequate for signed i128/u128 hotspots via bit-level two's-complement.

pub mod dsl_header;
pub mod emit;
pub mod handlers;
pub mod link;

pub use emit::Program;
pub use link::{AppendedFn, Linker};
