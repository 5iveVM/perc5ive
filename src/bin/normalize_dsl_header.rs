//! Convert a DSL-compiler-native `.fbin` (6-byte header) into a VM-native
//! binary (10-byte header) ready for `five deploy`. Used by the deploy
//! script for market artifacts — they don't need the full link pipeline
//! because they have no sentinel-stubbed handlers, but they do still need
//! the header format the on-chain VM parser expects.
//!
//! Usage:
//!   cargo run --bin normalize-dsl-header -- <in.fbin> <out.bin>

use std::process::ExitCode;

use perc5ive::bytecode::dsl_header::normalize_dsl_header;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output) = match args.len() {
        2 => (args[0].clone(), args[1].clone()),
        _ => {
            eprintln!("usage: normalize-dsl-header <input.fbin> <output.bin>");
            return ExitCode::from(2);
        }
    };

    let fbin = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("normalize-dsl-header: failed to read {input}: {e}");
            return ExitCode::from(1);
        }
    };

    let normalized = match normalize_dsl_header(&fbin) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("normalize-dsl-header: rejected {input}: {e:?}");
            return ExitCode::from(1);
        }
    };

    if let Err(e) = std::fs::write(&output, &normalized) {
        eprintln!("normalize-dsl-header: failed to write {output}: {e}");
        return ExitCode::from(1);
    }

    println!(
        "normalize-dsl-header: {} ({} B) → {} ({} B, 5IVE header)",
        input,
        fbin.len(),
        output,
        normalized.len()
    );
    ExitCode::SUCCESS
}
