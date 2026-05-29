//! Build a fully-linked, on-chain-ready `meta.bin` from the DSL-compiled
//! `meta.fbin` plus every hand-written genesis-lifecycle handler body.
//!
//! Mirrors `link_perc5ive` for the percolator-meta port: normalize the header,
//! append each `handler_body_*` body (fixing body-relative jumps), and rewrite
//! every genesis sentinel in place to a length-preserving jump into its body.
//! The 5 Phase-3 governance / market-factory sentinels are intentionally left
//! stubbed (they trap on invocation until the post-validation work lands).
//!
//! Usage:
//!   cargo run --bin link-meta -- [<in.fbin> <out.bin>]
//! Defaults to `target/meta.fbin` → `target/meta.linked.bin`.

use std::process::ExitCode;

use perc5ive::bytecode::meta_handlers::{
    all_meta_handler_bodies, link_meta, DEFERRED_SENTINELS, GENESIS_SENTINELS,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output) = match args.len() {
        0 => ("target/meta.fbin".to_string(), "target/meta.linked.bin".to_string()),
        2 => (args[0].clone(), args[1].clone()),
        _ => {
            eprintln!("usage: link-meta [<input.fbin> <output.bin>]");
            eprintln!("defaults: target/meta.fbin → target/meta.linked.bin");
            return ExitCode::from(2);
        }
    };

    let fbin = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("link-meta: failed to read {input}: {e}");
            eprintln!("  hint: run `cargo build` to regenerate it from meta/src/main.v");
            return ExitCode::from(1);
        }
    };

    let base_len = fbin.len();
    let linked = match link_meta(&fbin) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "link-meta: link_meta failed: {e:?} — meta/src/main.v may be missing a \
                 sentinel or the SENTINEL_* constants drifted"
            );
            return ExitCode::from(1);
        }
    };

    if let Err(e) = std::fs::write(&output, &linked) {
        eprintln!("link-meta: failed to write {output}: {e}");
        return ExitCode::from(1);
    }

    println!("link-meta: {input} → {output}");
    println!("  compiled meta.fbin:   {base_len} bytes");
    println!("  linked total:         {} bytes (+{} from handler bodies)",
             linked.len(), linked.len() - base_len);
    println!("  genesis sentinels rewritten: {}", GENESIS_SENTINELS.len());
    println!("  deferred sentinels (left stubbed): {}", DEFERRED_SENTINELS.len());
    println!();
    println!("  Genesis handler-body append map:");
    println!("  {:<24}  {:>8}", "sentinel", "bytes");
    for (sentinel, body) in all_meta_handler_bodies() {
        println!("  {sentinel:#024x}  {:>8}", body.bytes.len());
    }

    ExitCode::SUCCESS
}
