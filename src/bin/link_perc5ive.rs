//! Build a fully-linked, on-chain-ready `perc5ive.bin` from the DSL-compiled
//! `perc5ive.fbin` plus every hand-written handler body.
//!
//! The DSL compiler produces a 6-byte header and sentinel-stubbed u128
//! handler bodies. On-chain we need:
//!
//!   1. The 10-byte VM-native header (via `normalize_dsl_header`).
//!   2. Every `handler_body_*` bytecode appended after the base.
//!   3. Every sentinel rewritten in-place to `CALL` into its appended body
//!      (length-preserving).
//!   4. Body-relative jumps inside each appended body fixed up to use the
//!      correct absolute offsets in the linked output.
//!
//! Without this step, the deployed programs run against raw sentinels —
//! any call to deposit / withdraw / settle / liquidate / etc. would jump
//! to an address that is a raw u64 push followed by a trap.
//!
//! Usage:
//!   cargo run --bin link-perc5ive -- <in.fbin> <out.bin>
//!
//! Defaults to `target/perc5ive.fbin` → `target/perc5ive.linked.bin` when
//! invoked without arguments, matching the `scripts/deploy.sh perc5ive`
//! convention.

use std::process::ExitCode;

use perc5ive::bytecode::{
    dsl_header::normalize_dsl_header, handlers::all_handler_bodies, link::Linker,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output) = match args.len() {
        0 => ("target/perc5ive.fbin".to_string(), "target/perc5ive.linked.bin".to_string()),
        2 => (args[0].clone(), args[1].clone()),
        _ => {
            eprintln!("usage: link-perc5ive [<input.fbin> <output.bin>]");
            eprintln!("defaults: target/perc5ive.fbin → target/perc5ive.linked.bin");
            return ExitCode::from(2);
        }
    };

    let fbin = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("link-perc5ive: failed to read {input}: {e}");
            eprintln!("  hint: run `cargo build` to regenerate it from dsl/src/main.v");
            return ExitCode::from(1);
        }
    };

    let base = match normalize_dsl_header(&fbin) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("link-perc5ive: normalize_dsl_header rejected {input}: {e:?}");
            eprintln!("  hint: the DSL compiler may have emitted a header format this linker");
            eprintln!("        doesn't recognize yet. Check dsl_header.rs's opcode table.");
            return ExitCode::from(1);
        }
    };

    let mut linker = Linker::from_base(&base);
    let mut counts = Vec::new();
    for (sentinel, body) in all_handler_bodies() {
        let byte_count = body.bytes.len();
        let callee = linker.append_function_with_body_relative_jumps(&body.bytes, &body.jump_patches);
        match linker.rewrite_stub(sentinel, callee) {
            Ok(()) => counts.push((sentinel, byte_count, callee.offset)),
            Err(e) => {
                eprintln!(
                    "link-perc5ive: rewrite_stub({:#x}) failed: {:?} — DSL source \
                     may be missing the sentinel or the SENTINELS constants drifted",
                    sentinel, e
                );
                return ExitCode::from(1);
            }
        }
    }

    let linked = linker.into_bytes();
    if let Err(e) = std::fs::write(&output, &linked) {
        eprintln!("link-perc5ive: failed to write {output}: {e}");
        return ExitCode::from(1);
    }

    println!("link-perc5ive: {} → {}", input, output);
    println!("  base (normalized): {} bytes", base.len());
    println!("  linked total:      {} bytes (+{} from handlers)",
             linked.len(), linked.len() - base.len());
    println!();
    println!("  Handler-body append map:");
    println!("  {:<20}  {:>8}  {:>10}", "sentinel", "bytes", "callee@");
    for (sentinel, byte_count, offset) in counts {
        println!("  {:#020x}  {:>8}  {:>10}", sentinel, byte_count, offset);
    }

    ExitCode::SUCCESS
}
