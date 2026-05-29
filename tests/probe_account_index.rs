//! Empirical probe: pin mono's `execute_direct` account/field conventions so
//! the hand-written genesis handler bodies index the right bytes.
//!
//! No existing test exercised the runtime-account path (every prior e2e passed
//! `&[]`), so these facts were only ever asserted in comments. This probe
//! establishes them against the real VM and stays as a regression doc.
//!
//! Findings it locks in (see five-vm-mito/src/systems/accounts.rs +
//! handlers/memory.rs):
//!   * **Account index is a direct index** into the slice passed to
//!     execute_direct (ROOT_CONTEXT, no remap). Index 0 is the script account
//!     (its key binds data accounts); writes to index 0 are forbidden.
//!   * **VM-owned data accounts (index >= 1) must carry a 40-byte `5RAW`
//!     header** (`DSLRawAccountHeader`) whose `script_pubkey` equals
//!     `accounts[0].key()`, or LOAD_FIELD/STORE_FIELD return InvalidAccountData
//!     / ScriptNotAuthorized.
//!   * **Field offsets are absolute into the account buffer** (no implicit
//!     +header), so struct field N lives at buffer offset `40 + N`.

mod common;

use common::{get_u64, make_account_info, put_u64, read_data, FIVE_VM_PROGRAM_ID};
use five_protocol::opcodes::{ADD, LOAD_FIELD, PUSH_U64, RETURN_VALUE, STORE_FIELD};
use five_protocol::{
    DSL_RAW_ACCOUNT_HEADER_LEN, DSL_RAW_ACCOUNT_HEADER_MAGIC, DSL_RAW_ACCOUNT_HEADER_VERSION,
};
use five_vm_mito::{AccountInfo, MitoVM, Pubkey, StackStorage, Value};

const HDR: usize = DSL_RAW_ACCOUNT_HEADER_LEN; // 40

/// The key the script account (index 0) carries; data-account headers must
/// bind to it.
fn script_key() -> Pubkey {
    let mut k = [0u8; 32];
    k[0] = 0x5C;
    k[31] = 0xAA;
    k
}

/// Build a VM-owned data account whose buffer is `[40-byte 5RAW header][body]`,
/// header bound to `script_key()`.
fn headered_account(tag: u8, is_signer: bool, body: Vec<u8>) -> AccountInfo {
    let mut data = vec![0u8; HDR + body.len()];
    data[0..4].copy_from_slice(&DSL_RAW_ACCOUNT_HEADER_MAGIC);
    data[4] = DSL_RAW_ACCOUNT_HEADER_VERSION;
    // flags [5..8] = 0
    data[8..40].copy_from_slice(&script_key());
    data[HDR..].copy_from_slice(&body);
    let mut key = [0u8; 32];
    key[31] = tag;
    make_account_info(key, FIVE_VM_PROGRAM_ID, is_signer, true, 1_000_000, data)
}

/// The script account at index 0 — its key is the header binding.
fn script_account() -> AccountInfo {
    make_account_info(script_key(), FIVE_VM_PROGRAM_ID, false, false, 1, vec![0u8; 8])
}

fn one_function_script(body: &[u8]) -> Vec<u8> {
    let mut s = Vec::new();
    s.extend_from_slice(b"5IVE");
    s.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    s.push(0x01); // public_function_count
    s.push(0x01); // total_function_count
    s.extend_from_slice(body);
    s
}

fn input_func0_no_params() -> Vec<u8> {
    let mut input = Vec::new();
    input.extend_from_slice(&0u32.to_le_bytes());
    input.extend_from_slice(&0u32.to_le_bytes());
    input
}

fn run(body: &[u8], accounts: &[AccountInfo]) -> five_vm_mito::error::Result<Option<Value>> {
    MitoVM::execute_direct(
        &one_function_script(body),
        &input_func0_no_params(),
        accounts,
        &FIVE_VM_PROGRAM_ID,
        &mut StackStorage::new(),
    )
}

fn load_field_body(account_index: u8, offset: u32) -> Vec<u8> {
    // LOAD_FIELD pushes a lazy AccountRef; force it to a concrete u64 with
    // `+ 0` (polymorphic ADD reads 8 bytes for a u64 rhs) before returning.
    let mut b = vec![LOAD_FIELD, account_index];
    b.extend_from_slice(&offset.to_le_bytes());
    b.push(PUSH_U64);
    b.extend_from_slice(&0u64.to_le_bytes());
    b.push(ADD);
    b.push(RETURN_VALUE);
    b
}

fn as_u64(v: Value) -> u64 {
    match v {
        Value::U64(x) => x,
        Value::U128(x) => x as u64,
        other => panic!("expected u64-ish, got {other:?}"),
    }
}

#[test]
fn load_field_index_is_direct_and_offset_is_header_inclusive() {
    // Two data accounts at indices 1 and 2 with distinct payloads at body
    // offset 0 (= buffer offset 40).
    let mut b1 = vec![0u8; 8];
    put_u64(&mut b1, 0, 0xB1);
    let mut b2 = vec![0u8; 8];
    put_u64(&mut b2, 0, 0xC2);

    let accounts = [
        script_account(),
        headered_account(1, false, b1),
        headered_account(2, false, b2),
    ];

    // Header-inclusive offset HDR reads the payload.
    let v1 = as_u64(run(&load_field_body(1, HDR as u32), &accounts).unwrap().unwrap());
    let v2 = as_u64(run(&load_field_body(2, HDR as u32), &accounts).unwrap().unwrap());
    assert_eq!(v1, 0xB1, "LOAD_FIELD 1 @ {HDR} should read accounts[1] payload");
    assert_eq!(v2, 0xC2, "LOAD_FIELD 2 @ {HDR} should read accounts[2] payload");
}

#[test]
fn vm_owned_account_without_5raw_header_is_rejected() {
    // Same value but NO 5RAW header → InvalidAccountData on field access.
    let mut raw = vec![0u8; 8];
    put_u64(&mut raw, 0, 0xB1);
    let bad = make_account_info([1u8; 32], FIVE_VM_PROGRAM_ID, false, true, 1, raw);
    let accounts = [script_account(), bad];
    let res = run(&load_field_body(1, 0), &accounts);
    assert!(res.is_err(), "headerless VM-owned account must be rejected, got {res:?}");
}

#[test]
fn store_field_writes_payload_past_the_header() {
    // Body: PUSH_U64 999; STORE_FIELD 1 (HDR); PUSH_U64 0; RETURN_VALUE
    let mut body = vec![PUSH_U64];
    body.extend_from_slice(&999u64.to_le_bytes());
    body.push(STORE_FIELD);
    body.push(1);
    body.extend_from_slice(&(HDR as u32).to_le_bytes());
    body.push(PUSH_U64);
    body.extend_from_slice(&0u64.to_le_bytes());
    body.push(RETURN_VALUE);

    let accounts = [script_account(), headered_account(1, true, vec![0u8; 8])];
    let res = run(&body, &accounts);
    assert!(res.is_ok(), "STORE_FIELD to index 1 should be authorized: {res:?}");

    let after = read_data(&accounts[1]);
    assert_eq!(get_u64(&after, HDR), 999, "payload at buffer offset {HDR} must be 999");
    // Header must be intact after the store.
    assert_eq!(&after[0..4], &DSL_RAW_ACCOUNT_HEADER_MAGIC, "5RAW header clobbered by store");
}
