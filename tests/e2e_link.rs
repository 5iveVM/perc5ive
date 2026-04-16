//! End-to-end test for the bytecode linker.
//!
//! Demonstrates the full pipeline:
//!   1. Hand-build a "main" caller `.five` binary that pushes two u256 operands
//!      and emits a CALL with a placeholder target.
//!   2. Append a hand-written u256 ADD function as a callee using [`Linker`].
//!   3. Patch the placeholder CALL to point at the appended function.
//!   4. Run the linked binary through `MitoVM::execute_direct` and check the
//!      return value against a Rust reference.
//!
//! The point isn't u256 ADD itself — that's already tested elsewhere — but
//! that the linker correctly composes a base binary with appended bytecode
//! and that CALL/RETURN_VALUE work as the linker assumes.

use five_protocol::opcodes::{ADD_U256, CMP_U256, RETURN_VALUE};
use five_vm_mito::{MitoVM, Value};
use perc5ive::bytecode::{
    emit::Program,
    link::Linker,
    u256::FLAG_WRAPPING,
};

fn run_u64(bytecode: &[u8]) -> u64 {
    let result = MitoVM::execute_direct(bytecode, &[], &[])
        .expect("VM execution should succeed");
    match result {
        Some(Value::U64(v)) => v,
        Some(other) => panic!("expected U64, got {:?}", other),
        None => panic!("expected a return value"),
    }
}

/// Build the appended callee body: it inherits the caller's stack (the two
/// u256s sit on top), runs `ADD_U256` wrapping, then `RETURN_VALUE` to surface
/// the top limb. The other 3 result limbs remain on the caller's stack;
/// the caller is responsible for cleaning them up if it doesn't want them.
fn u256_add_top_limb_callee() -> Vec<u8> {
    vec![ADD_U256, FLAG_WRAPPING, RETURN_VALUE]
}

/// Build a main program that:
///   1. Pushes `a` (4 limbs) and `b` (4 limbs) onto the stack.
///   2. CALLs a placeholder offset (param_count = 0 so callee sees the limbs).
///   3. After return, drops 3 limbs to keep just the requested one on top.
///   4. HALTs (the top limb is the program return value).
fn build_main(a: [u64; 4], b: [u64; 4], limb_to_keep: u8) -> (Vec<u8>, u16) {
    use five_protocol::opcodes::{DROP, SWAP};
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    let call_handle = p.emit_call_placeholder(0);
    let call_site_abs = p.absolute_call_site(call_handle);
    // After the call, stack has [r0, r1, r2, r3] with r3 on top.
    // Drop limbs above the chosen one, then SWAP/DROP to bury and remove the
    // limbs below it.
    for _ in 0..(3 - limb_to_keep) {
        p.raw(DROP);
    }
    for _ in 0..limb_to_keep {
        p.raw(SWAP).raw(DROP);
    }
    let bin = p.finish_with_halt();
    (bin, call_site_abs)
}

fn add_u256_reference(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut carry: u128 = 0;
    let mut r = [0u64; 4];
    for i in 0..4 {
        let s = (a[i] as u128) + (b[i] as u128) + carry;
        r[i] = s as u64;
        carry = s >> 64;
    }
    r
}

#[test]
fn linked_call_into_u256_add_returns_each_limb() {
    let a = [0xDEAD_BEEFu64, 0xCAFE_BABE, 0x1234_5678, 0x0FED_CBA9];
    let b = [0x1111_2222u64, 0x3333_4444, 0x5555_6666, 0x7777_8888];
    let expected = add_u256_reference(a, b);

    for limb in 0..4u8 {
        let (main_bin, call_site) = build_main(a, b, limb);
        let mut linker = Linker::from_base(&main_bin);
        let callee = linker.append_function(&u256_add_top_limb_callee());
        linker.patch_call_target(call_site, callee);
        let linked = linker.into_bytes();
        let got = run_u64(&linked);
        assert_eq!(
            got, expected[limb as usize],
            "limb {} mismatch: got 0x{:016x}, expected 0x{:016x}",
            limb, got, expected[limb as usize]
        );
    }
}

#[test]
fn linker_can_chain_two_appended_functions() {
    // Caller pushes 4 u64s [10, 20, 30, 40] (treated as a u256 = limbs lo->hi).
    // Then pushes 4 u64s [1, 2, 3, 4] for the second u256.
    // Calls "add" callee, then drops to keep limb 0.
    let a = [10u64, 20, 30, 40];
    let b = [1u64, 2, 3, 4];
    let (main_bin, call_site) = build_main(a, b, 0);
    let mut linker = Linker::from_base(&main_bin);

    // Append two distinct callees; verify each gets a unique offset and only
    // the patched one is reachable from main. The second is dead code (good —
    // proves we can carry library bodies in the binary without affecting
    // the executed path).
    let dead = linker.append_function(&[
        // Push something obviously wrong to make a regression visible.
        five_protocol::opcodes::PUSH_U64, 0x99,
        RETURN_VALUE,
    ]);
    let live = linker.append_function(&u256_add_top_limb_callee());
    assert_ne!(dead.offset, live.offset);
    assert!(live.offset > dead.offset, "second appended fn must come later");

    linker.patch_call_target(call_site, live);
    let linked = linker.into_bytes();
    let got = run_u64(&linked);
    assert_eq!(got, 11, "10 + 1 = 11 (low limb)");
}

#[test]
fn linker_preserves_base_bytes_byte_for_byte() {
    let mut p = Program::new();
    p.push_u64(7);
    let base = p.finish_with_halt();
    let base_bytes = base.clone();

    let mut linker = Linker::from_base(&base);
    let _ = linker.append_function(&[RETURN_VALUE]);
    let linked = linker.into_bytes();

    assert_eq!(&linked[..base_bytes.len()], &base_bytes[..]);
}

// =============================================================================
// Sentinel-based stub rewriting — simulates DSL → linker → VM pipeline
// =============================================================================

const SENTINEL_CMP: u64 = 0xFEED_FACE_DEAD_C001;

/// Build a "DSL-like" base binary:
///   1. Push two u256 operands onto the stack.
///   2. The "stub function" body: PUSH_U64 sentinel, RETURN_VALUE — this is
///      what `five-dsl-compiler` would emit for `return 0xFEED_FACE_DEAD_C001;`.
///   3. After the stub is rewritten by the linker, step 2 becomes:
///      CALL 0 <appended>, RETURN_VALUE, NOP... — the appended callee
///      runs CMP_U256 on the two u256s already on the stack and returns
///      the comparison result.
///   4. After RETURN_VALUE the caller falls through to HALT, surfacing the
///      comparison result (0=lt, 1=eq, 2=gt).
fn build_dsl_like_base(a: [u64; 4], b: [u64; 4]) -> Vec<u8> {
    let mut p = Program::new();
    p.push_u256(a).push_u256(b);
    // Emit the stub body inline (what `five build` would produce for a stub fn
    // whose body is `return SENTINEL_CMP;`).
    p.push_u64(SENTINEL_CMP);
    p.raw(RETURN_VALUE);
    p.finish_with_halt()
}

/// The callee: pops nothing (param_count=0 preserves caller's stack), runs
/// CMP_U256 wrapping on the two u256s, then RETURN_VALUE — leaving the
/// comparison result (a single u64) as the program's return value.
fn cmp_u256_callee() -> Vec<u8> {
    vec![CMP_U256, FLAG_WRAPPING, RETURN_VALUE]
}

#[test]
fn sentinel_stub_rewrite_then_vm_executes_cmp() {
    // a = 10, b = 20 → cmp < → should return 0
    let a = [10u64, 0, 0, 0];
    let b = [20u64, 0, 0, 0];
    let base = build_dsl_like_base(a, b);

    let mut linker = Linker::from_base(&base);
    let callee = linker.append_function(&cmp_u256_callee());
    linker.rewrite_stub(SENTINEL_CMP, callee).expect("rewrite_stub OK");
    let linked = linker.into_bytes();

    assert_eq!(run_u64(&linked), 0, "10 < 20 → cmp result 0 (lt)");
}

#[test]
fn sentinel_stub_rewrite_equal() {
    let v = [42u64, 0, 0, 0];
    let base = build_dsl_like_base(v, v);
    let mut linker = Linker::from_base(&base);
    let callee = linker.append_function(&cmp_u256_callee());
    linker.rewrite_stub(SENTINEL_CMP, callee).unwrap();
    let linked = linker.into_bytes();
    assert_eq!(run_u64(&linked), 1, "42 == 42 → cmp result 1 (eq)");
}

#[test]
fn sentinel_stub_rewrite_greater() {
    let a = [100u64, 0, 0, 0];
    let b = [1u64, 0, 0, 0];
    let base = build_dsl_like_base(a, b);
    let mut linker = Linker::from_base(&base);
    let callee = linker.append_function(&cmp_u256_callee());
    linker.rewrite_stub(SENTINEL_CMP, callee).unwrap();
    let linked = linker.into_bytes();
    assert_eq!(run_u64(&linked), 2, "100 > 1 → cmp result 2 (gt)");
}
