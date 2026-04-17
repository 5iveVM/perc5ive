//! Round-trip a u128 value through `MitoVM::execute_direct`'s typed-param
//! `input_data` pathway. Regression test for the U128 typed-param branch
//! added in five-vm-mito PR #86.
//!
//! Without that PR's change, the typed-param parser only recognised
//! U8 / U32 / U64 / BOOL / STRING — a `type_id == 14 (U128)` byte fell
//! through to the catch-all and returned `TypeMismatch`. That was the
//! single biggest blocker for the Phase 2 runtime e2e: every Percolator
//! handler signature has at least one `u128` parameter (amount, oracle
//! price scaled, basis), and there was no way to drive them via the
//! public `execute_direct` entry point without per-test shim binaries.
//!
//! This test confirms a u128 value walked into the param table is the
//! same value that comes out via `LOAD_PARAM` + `RETURN_VALUE`.

use five_protocol::opcodes::{LOAD_PARAM_1, RETURN_VALUE};
use five_protocol::types;
use five_vm_mito::{MitoVM, Value};

const TYPED_PARAM_SENTINEL: u32 = 128;

fn write_vle(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let b = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Build the smallest valid VM-native script with a single function whose
/// body is `LOAD_PARAM_1 ; RETURN_VALUE`. The returned bytecode is what the
/// VM consumes — the dispatcher routes function index 0 to this body.
fn one_function_script_loading_param_1() -> Vec<u8> {
    let mut s = Vec::new();
    s.extend_from_slice(b"5IVE");
    s.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // features
    s.push(0x01); // public_function_count
    s.push(0x01); // total_function_count
    // Function 0 body
    s.push(LOAD_PARAM_1);
    s.push(RETURN_VALUE);
    s
}

/// Encode the typed-param `input_data` envelope for a single u128 argument:
///   [func_idx VLE] [TYPED_PARAM_SENTINEL=128 VLE] [typed_count=1 VLE]
///   [type_id=U128] [16 bytes u128 LE]
fn typed_input_data_u128(func_idx: u32, value: u128) -> Vec<u8> {
    let mut input = Vec::new();
    write_vle(&mut input, func_idx);
    write_vle(&mut input, TYPED_PARAM_SENTINEL);
    write_vle(&mut input, 1);
    input.push(types::U128);
    input.extend_from_slice(&value.to_le_bytes());
    input
}

#[test]
fn typed_u128_input_param_round_trips_through_load_param() {
    let amount: u128 = 0xDEAD_BEEF_CAFE_BABE_F00D_BAAD_FEEB_DAEDu128;
    let bytecode = one_function_script_loading_param_1();
    let input = typed_input_data_u128(0, amount);

    let result = MitoVM::execute_direct(&bytecode, &input, &[], &[0u8; 32], &mut five_vm_mito::StackStorage::new())
        .expect("VM should accept the u128 typed-param input")
        .expect("function should return a value");

    match result {
        Value::U128(v) => assert_eq!(v, amount, "u128 round-trip mismatch"),
        other => panic!("expected Value::U128, got {:?}", other),
    }
}

#[test]
fn typed_u128_zero_round_trips_cleanly() {
    let bytecode = one_function_script_loading_param_1();
    let input = typed_input_data_u128(0, 0u128);
    let result = MitoVM::execute_direct(&bytecode, &input, &[], &[0u8; 32], &mut five_vm_mito::StackStorage::new()).unwrap().unwrap();
    assert_eq!(result, Value::U128(0));
}

#[test]
fn typed_u128_max_round_trips_cleanly() {
    let bytecode = one_function_script_loading_param_1();
    let input = typed_input_data_u128(0, u128::MAX);
    let result = MitoVM::execute_direct(&bytecode, &input, &[], &[0u8; 32], &mut five_vm_mito::StackStorage::new()).unwrap().unwrap();
    assert_eq!(result, Value::U128(u128::MAX));
}

#[test]
fn typed_u128_truncated_input_data_returns_error() {
    // Input data shorter than 16 bytes after the type_id should error
    // gracefully (InvalidInstructionPointer), never panic / overrun.
    let bytecode = one_function_script_loading_param_1();
    let mut input = Vec::new();
    write_vle(&mut input, 0);
    write_vle(&mut input, TYPED_PARAM_SENTINEL);
    write_vle(&mut input, 1);
    input.push(types::U128);
    // Only 8 bytes of "value" — should be rejected by the bounds check.
    input.extend_from_slice(&[0xAA; 8]);

    let result = MitoVM::execute_direct(&bytecode, &input, &[], &[0u8; 32], &mut five_vm_mito::StackStorage::new());
    assert!(
        result.is_err(),
        "truncated u128 input_data should error, got {:?}",
        result
    );
}
