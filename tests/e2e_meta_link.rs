//! Phase 1 structural gate for the percolator-meta port.
//!
//! Proves the build.rs-produced `target/meta.fbin` — the 19-handler genesis
//! DSL skeleton — is well-formed: every genesis + deferred sentinel is present
//! and locatable by the linker, the header normalizes to VM-native, and
//! MitoVM accepts the layout. The runtime half (executing genesis handler
//! bodies against crafted accounts) lands in Phase 2 (`e2e_meta_genesis.rs`).

use five_vm_mito::MitoVM;
use perc5ive::bytecode::{
    dsl_header::normalize_dsl_header,
    link::Linker,
    meta_handlers::{DEFERRED_SENTINELS, GENESIS_SENTINELS},
};

fn meta_binary() -> Vec<u8> {
    std::fs::read("target/meta.fbin").expect(
        "target/meta.fbin missing — build.rs regenerates it from meta/src/main.v on cargo build",
    )
}

#[test]
fn every_genesis_sentinel_is_present_in_compiled_meta() {
    let bin = meta_binary();
    let linker = Linker::from_base(&bin);
    for sentinel in GENESIS_SENTINELS {
        let found = linker
            .find_stub(sentinel)
            .unwrap_or_else(|e| panic!("find_stub({sentinel:#x}) errored: {e:?}"));
        assert!(
            found.is_some(),
            "genesis sentinel {sentinel:#x} not found in compiled meta/src/main.v — \
             DSL source and meta_handlers.rs SENTINEL_* constants may have drifted",
        );
    }
}

#[test]
fn every_deferred_sentinel_is_present_in_compiled_meta() {
    let bin = meta_binary();
    let linker = Linker::from_base(&bin);
    for sentinel in DEFERRED_SENTINELS {
        assert!(
            linker.find_stub(sentinel).unwrap().is_some(),
            "deferred sentinel {sentinel:#x} not found — these stay stubbed but must compile",
        );
    }
}

#[test]
fn compiled_meta_normalizes_and_parses_via_mito() {
    let bin = meta_binary();
    let normalized = normalize_dsl_header(&bin).expect("normalize_dsl_header accepts meta.fbin");
    assert_eq!(&normalized[..4], b"5IVE", "normalized meta missing 5IVE magic");
    assert!(normalized.len() >= 10, "normalized meta shorter than the 10-byte header");
    // Smoke: VM accepts the layout (function 0 with no input is irrelevant).
    let _ = MitoVM::execute_direct(
        &normalized,
        &[],
        &[],
        &[0u8; 32],
        &mut five_vm_mito::StackStorage::new(),
    );
}
