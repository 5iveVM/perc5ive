use std::fs;
use std::path::PathBuf;

use five_dsl_compiler::DslCompiler;

/// Each entry: (source `.v` path, output `.fbin` path, required?).
///
/// `required = true` means a compile failure halts the build. `false` means
/// we emit a cargo warning and continue so the rest of the crate still
/// builds (useful for known-blocked files — today: `markets/lp_perp` pending
/// DSL i128 literal support, see
/// https://github.com/5iveVM/five-dsl-compiler/issues for the tracking
/// ticket).
const DSL_SOURCES: &[(&str, &str, bool)] = &[
    ("dsl/src/main.v", "target/perc5ive.fbin", true),
    ("markets/sov/src/main.v", "target/sov.fbin", true),
    ("markets/pyth_race/src/main.v", "target/pyth_race.fbin", true),
    ("markets/lp_perp/src/main.v", "target/lp_perp.fbin", false),
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    for (source_path, out_path, required) in DSL_SOURCES {
        let source_path = PathBuf::from(source_path);
        let out_path = PathBuf::from(out_path);
        println!("cargo:rerun-if-changed={}", source_path.display());

        let source = fs::read_to_string(&source_path).unwrap_or_else(|e| {
            panic!("failed to read {}: {}", source_path.display(), e)
        });

        match DslCompiler::compile_dsl(&source) {
            Ok(bytecode) => {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                fs::write(&out_path, &bytecode).unwrap_or_else(|e| {
                    panic!("failed to write {}: {}", out_path.display(), e)
                });
                println!(
                    "cargo:warning=perc5ive: compiled {} ({} bytes) → {}",
                    source_path.display(),
                    bytecode.len(),
                    out_path.display()
                );
            }
            Err(e) if !required => {
                println!(
                    "cargo:warning=perc5ive: {} failed to compile ({:?}) — continuing \
                     because this source is marked non-required pending compiler support",
                    source_path.display(),
                    e.code
                );
            }
            Err(e) => {
                panic!(
                    "DSL compile of {} failed: {:?}",
                    source_path.display(),
                    e
                );
            }
        }
    }
}
