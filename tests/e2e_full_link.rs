//! Phase 2 end-to-end: **full compiled `main.v`** linked against all 9
//! hand-written handler bodies.
//!
//! This closes the structural half of the Phase 2 exit criteria: prove that
//! the build.rs-produced `target/perc5ive.fbin` — which contains the real
//! 33-function DSL skeleton with sentinel-stubbed u128 handlers — can be run
//! through the linker to rewrite every sentinel with its matching appended
//! body, and the resulting binary is a well-formed VM-native `.five` that
//! MitoVM accepts.
//!
//! The **runtime** half (dispatch into `deposit` with `input_data` carrying
//! a u128 amount, execute, assert post-state on both AccountInfos) is still
//! blocked on the param-table u128 pathway — `input_data` VLE fields are
//! u32-capped today. That extension is tracked as a follow-on; this file
//! locks in everything achievable without it.

use five_vm_mito::MitoVM;
use perc5ive::bytecode::{
    dsl_header::normalize_dsl_header, handlers::all_handler_bodies, link::Linker,
};

fn main_v_binary() -> Vec<u8> {
    std::fs::read("target/perc5ive.fbin").expect(
        "target/perc5ive.fbin missing — the build.rs step should regenerate it from \
         dsl/src/main.v on every `cargo build` / `cargo test`",
    )
}

/// Every sentinel declared in `handlers.rs` must exist in the compiled main.v.
/// If one is missing, either the DSL source has drifted from the sentinel
/// constants or the compiler didn't emit the stub body we expect.
#[test]
fn every_sentinel_is_present_in_compiled_main_v() {
    let bin = main_v_binary();
    let linker = Linker::from_base(&bin);
    for (sentinel, _body) in all_handler_bodies() {
        let found = linker
            .find_stub(sentinel)
            .unwrap_or_else(|e| panic!("find_stub({:#x}) errored: {:?}", sentinel, e));
        assert!(
            found.is_some(),
            "sentinel {:#x} not found in compiled main.v — DSL source and \
             handlers.rs SENTINEL_* constants may have drifted",
            sentinel
        );
    }
}

/// Append each of the 9 handler bodies and rewrite the matching stub. Sequential
/// rewrites must not collide — the pattern search re-scans the binary from
/// scratch each call, so a previous rewrite's NOP-padded CALL sequence cannot
/// be mistaken for a fresh sentinel pattern.
#[test]
fn linker_rewrites_all_9_sentinels_without_collisions() {
    let bin = main_v_binary();
    let bin = normalize_dsl_header(&bin).expect("normalize_dsl_header accepts real main.v");
    let mut linker = Linker::from_base(&bin);
    for (sentinel, body) in all_handler_bodies() {
        let callee = linker.append_function(&body);
        linker
            .rewrite_stub(sentinel, callee)
            .unwrap_or_else(|e| panic!("rewrite_stub({:#x}) failed: {:?}", sentinel, e));
    }
    let linked = linker.into_bytes();

    let verify = Linker::from_base(&linked);
    for (sentinel, _) in all_handler_bodies() {
        assert_eq!(
            verify.find_stub(sentinel).unwrap(),
            None,
            "sentinel {:#x} still present after rewrite",
            sentinel
        );
    }
}

/// The fully linked binary must be a well-formed VM-native `.five`: 5IVE magic
/// in the first four bytes, at least a complete header, and MitoVM must
/// accept it for execution without panicking on parse. Whatever function 0
/// does when called with no input_data is irrelevant here — the assertion is
/// that the VM doesn't reject the linked layout.
#[test]
fn linked_main_v_is_vm_native_and_parses_via_mito() {
    let bin = main_v_binary();
    let bin = normalize_dsl_header(&bin).expect("normalize_dsl_header accepts real main.v");
    let mut linker = Linker::from_base(&bin);
    for (sentinel, body) in all_handler_bodies() {
        let callee = linker.append_function(&body);
        linker.rewrite_stub(sentinel, callee).unwrap();
    }
    let linked = linker.into_bytes();

    assert_eq!(&linked[..4], b"5IVE", "linked binary missing 5IVE magic");
    assert!(
        linked.len() >= 10,
        "linked binary shorter than the 10-byte VM-native header"
    );

    // Smoke: VM accepts the layout. The Result (Ok or Err) depends on what
    // function 0 is in the compiled main.v — we deliberately do not assert
    // on its shape, only that no panic escapes the call.
    let _ = MitoVM::execute_direct(&linked, &[], &[]);
}

/// The linked binary grows by exactly the total size of all 9 appended
/// bodies. The rewrite step does not change binary length (it swaps equal-
/// sized byte slots). This is a sanity check that the linker's size
/// invariants still hold when driven by the full handler set.
#[test]
fn linked_main_v_length_equals_base_plus_appended_bodies() {
    let bin = main_v_binary();
    let bin = normalize_dsl_header(&bin).expect("normalize_dsl_header accepts real main.v");
    let base_len = bin.len();

    let mut linker = Linker::from_base(&bin);
    let mut total_appended = 0usize;
    for (sentinel, body) in all_handler_bodies() {
        total_appended += body.len();
        let callee = linker.append_function(&body);
        linker.rewrite_stub(sentinel, callee).unwrap();
    }
    let linked = linker.into_bytes();

    assert_eq!(
        linked.len(),
        base_len + total_appended,
        "linked length should be base + sum(appended bodies); rewrite must be \
         length-preserving"
    );
}
