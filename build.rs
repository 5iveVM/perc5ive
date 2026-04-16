use std::fs;
use std::path::PathBuf;

use five_dsl_compiler::DslCompiler;

fn main() {
    let dsl_source = PathBuf::from("dsl/src/main.v");
    println!("cargo:rerun-if-changed={}", dsl_source.display());
    println!("cargo:rerun-if-changed=build.rs");

    let source = fs::read_to_string(&dsl_source)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", dsl_source.display(), e));

    let bytecode = DslCompiler::compile_dsl(&source)
        .unwrap_or_else(|e| panic!("DSL compile of {} failed: {:?}", dsl_source.display(), e));

    let out_path = PathBuf::from("target/perc5ive.fbin");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&out_path, &bytecode)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", out_path.display(), e));

    println!(
        "cargo:warning=perc5ive: compiled {} ({} bytes) → {}",
        dsl_source.display(),
        bytecode.len(),
        out_path.display()
    );
}
