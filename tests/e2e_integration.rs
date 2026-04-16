//! End-to-end integration tests — hand-written bytecode executes against real
//! pinocchio `AccountInfo` instances representing live Percolator state.
//!
//! These tests do NOT go through the `@five-vm/cli`-compiled `main.v`. They
//! validate the narrower claim that the bytecode we hand-write for the u128
//! handlers performs the correct arithmetic against real account storage when
//! executed by `MitoVM::execute_direct`. A full DSL→linker→VM test has
//! separate architectural prereqs (calling convention for params across
//! sentinel-rewrite CALL frames) tracked in `SESSION_STATE.md`.
//!
//! What we verify:
//!   * `LOAD_FIELD` / `STORE_FIELD` read and write u128 fields at the expected
//!     packed byte offsets.
//!   * The generic polymorphic `ADD` / `SUB` opcodes operate on u128 values
//!     pulled out of account storage.
//!   * After execution, the `AccountInfo`'s data buffer is mutated in place
//!     (the VM is actually touching our leaked memory — not a copy).
//!
//! Field offsets here mirror `dsl/src/main.v` verbatim; changes to that file
//! must be reflected here or these tests will silently pass for the wrong
//! data window.

mod common;

use common::{
    get_u128, get_u16, get_u64, make_vm_account, put_u128, put_u16, put_u64,
};
use five_protocol::opcodes::{
    ADD, HALT, LOAD_FIELD, LOAD_FIELD_U128, LOAD_FIELD_U16, PUSH_U128, STORE_FIELD,
    STORE_FIELD_U128, STORE_FIELD_U16, SUB,
};
use five_vm_mito::MitoVM;
use perc5ive::bytecode::handlers::{margin_offsets, risk_offsets};

/// Build a `.five` binary in VM-native form (10-byte header) that runs the
/// supplied body and halts. Account indices inside `body` are absolute into
/// the `accounts` slice passed to `execute_direct`.
fn vm_binary(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + body.len() + 1);
    out.extend_from_slice(b"5IVE");
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // features = 0
    out.push(0x00); // public_function_count
    out.push(0x00); // total_function_count
    out.extend_from_slice(body);
    out.push(HALT);
    out
}

/// Encode a `LOAD_FIELD account_index offset` (u64 field) using VLE for the
/// offset. The `offset` fits in a u16 here, so at most 3 VLE bytes.
fn encode_load_field(account_index: u8, offset: u64) -> Vec<u8> {
    let mut v = vec![LOAD_FIELD, account_index];
    write_vle(&mut v, offset);
    v
}

fn encode_store_field(account_index: u8, offset: u64) -> Vec<u8> {
    let mut v = vec![STORE_FIELD, account_index];
    write_vle(&mut v, offset);
    v
}

fn encode_load_field_u128(account_index: u8, offset: u64) -> Vec<u8> {
    let mut v = vec![LOAD_FIELD_U128, account_index];
    write_vle(&mut v, offset);
    v
}

fn encode_store_field_u128(account_index: u8, offset: u64) -> Vec<u8> {
    let mut v = vec![STORE_FIELD_U128, account_index];
    write_vle(&mut v, offset);
    v
}

fn encode_load_field_u16(account_index: u8, offset: u64) -> Vec<u8> {
    let mut v = vec![LOAD_FIELD_U16, account_index];
    write_vle(&mut v, offset);
    v
}

fn encode_store_field_u16(account_index: u8, offset: u64) -> Vec<u8> {
    let mut v = vec![STORE_FIELD_U16, account_index];
    write_vle(&mut v, offset);
    v
}

fn write_vle(out: &mut Vec<u8>, mut value: u64) {
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

fn push_u128_bytes(value: u128) -> Vec<u8> {
    let mut v = vec![PUSH_U128];
    v.extend_from_slice(&value.to_le_bytes());
    v
}

/// Size of the full `RiskEngine` account data as defined in `dsl/src/main.v`.
/// Zero-initialize to this size then overwrite the fields the test cares about.
const RISK_DATA_SIZE: usize = 645;
/// Size of the full `MarginAccount` account data.
const MARGIN_DATA_SIZE: usize = 333;

/// Apply `top_up_insurance_fund`'s arithmetic inline:
///
/// ```text
///   risk.insurance_fund += amount;
///   risk.vault          += amount;
/// ```
///
/// Executed as a single sequential bytecode program against a single account
/// at index 0 — no CALL frame, no param frame. This decouples the arithmetic
/// correctness claim from the parameter-passing-across-CALL question.
#[test]
fn top_up_insurance_fund_arithmetic_mutates_risk_engine() {
    let initial_vault: u128 = 1_000_000_000_000;
    let initial_insurance: u128 = 500_000_000_000;
    let amount: u128 = 123_456_789;

    let mut data = vec![0u8; RISK_DATA_SIZE];
    put_u128(&mut data, risk_offsets::VAULT as usize, initial_vault);
    put_u128(
        &mut data,
        risk_offsets::INSURANCE_FUND as usize,
        initial_insurance,
    );

    let risk = make_vm_account(1, false, data);
    let accounts = [risk];

    // Build body: load+add+store for insurance_fund, then vault. Account
    // index 0 because the accounts slice has risk at index 0 (we're skipping
    // the DSL's reserved-ctx-slot convention here). LOAD_FIELD_U128 /
    // STORE_FIELD_U128 read/write 16-byte packed u128 fields — the plain
    // LOAD_FIELD reads only 8 bytes and would overlap into the next field.
    let mut body = Vec::new();
    // insurance_fund += amount
    body.extend(encode_load_field_u128(0, risk_offsets::INSURANCE_FUND));
    body.extend(push_u128_bytes(amount));
    body.push(ADD);
    body.extend(encode_store_field_u128(0, risk_offsets::INSURANCE_FUND));
    // vault += amount
    body.extend(encode_load_field_u128(0, risk_offsets::VAULT));
    body.extend(push_u128_bytes(amount));
    body.push(ADD);
    body.extend(encode_store_field_u128(0, risk_offsets::VAULT));

    let bin = vm_binary(&body);
    let result = MitoVM::execute_direct(&bin, &[], &accounts);
    assert!(result.is_ok(), "execute_direct failed: {:?}", result.err());

    // Inspect in-place mutation of the account data buffer.
    let post = common::read_data(&accounts[0]);
    assert_eq!(
        get_u128(&post, risk_offsets::INSURANCE_FUND as usize),
        initial_insurance + amount,
        "insurance_fund should equal initial + amount",
    );
    assert_eq!(
        get_u128(&post, risk_offsets::VAULT as usize),
        initial_vault + amount,
        "vault should equal initial + amount",
    );
}

/// `deposit` arithmetic (no oracle check, no pending promotion): mutates both
/// the margin account's capital and the risk engine's vault + c_tot.
#[test]
fn deposit_arithmetic_mutates_both_accounts() {
    let initial_capital: u128 = 100_000_000;
    let initial_vault: u128 = 800_000_000_000;
    let initial_c_tot: u128 = 700_000_000_000;
    let amount: u128 = 42_000_000;

    let mut risk_data = vec![0u8; RISK_DATA_SIZE];
    put_u128(&mut risk_data, risk_offsets::VAULT as usize, initial_vault);
    put_u128(&mut risk_data, risk_offsets::C_TOT as usize, initial_c_tot);

    let mut margin_data = vec![0u8; MARGIN_DATA_SIZE];
    put_u128(
        &mut margin_data,
        margin_offsets::CAPITAL as usize,
        initial_capital,
    );

    let risk = make_vm_account(1, false, risk_data);
    let margin = make_vm_account(2, false, margin_data);
    let accounts = [risk, margin];

    let mut body = Vec::new();
    // acct.capital += amount  (margin is index 1)
    body.extend(encode_load_field_u128(1, margin_offsets::CAPITAL));
    body.extend(push_u128_bytes(amount));
    body.push(ADD);
    body.extend(encode_store_field_u128(1, margin_offsets::CAPITAL));
    // risk.vault += amount
    body.extend(encode_load_field_u128(0, risk_offsets::VAULT));
    body.extend(push_u128_bytes(amount));
    body.push(ADD);
    body.extend(encode_store_field_u128(0, risk_offsets::VAULT));
    // risk.c_tot += amount
    body.extend(encode_load_field_u128(0, risk_offsets::C_TOT));
    body.extend(push_u128_bytes(amount));
    body.push(ADD);
    body.extend(encode_store_field_u128(0, risk_offsets::C_TOT));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let risk_post = common::read_data(&accounts[0]);
    let margin_post = common::read_data(&accounts[1]);
    assert_eq!(
        get_u128(&margin_post, margin_offsets::CAPITAL as usize),
        initial_capital + amount,
    );
    assert_eq!(
        get_u128(&risk_post, risk_offsets::VAULT as usize),
        initial_vault + amount,
    );
    assert_eq!(
        get_u128(&risk_post, risk_offsets::C_TOT as usize),
        initial_c_tot + amount,
    );
}

/// `withdraw` arithmetic: inverse of deposit.
#[test]
fn withdraw_arithmetic_mutates_both_accounts() {
    let initial_capital: u128 = 500_000_000;
    let initial_vault: u128 = 800_000_000_000;
    let initial_c_tot: u128 = 700_000_000_000;
    let amount: u128 = 25_000_000;

    let mut risk_data = vec![0u8; RISK_DATA_SIZE];
    put_u128(&mut risk_data, risk_offsets::VAULT as usize, initial_vault);
    put_u128(&mut risk_data, risk_offsets::C_TOT as usize, initial_c_tot);

    let mut margin_data = vec![0u8; MARGIN_DATA_SIZE];
    put_u128(
        &mut margin_data,
        margin_offsets::CAPITAL as usize,
        initial_capital,
    );

    let risk = make_vm_account(1, false, risk_data);
    let margin = make_vm_account(2, false, margin_data);
    let accounts = [risk, margin];

    let mut body = Vec::new();
    body.extend(encode_load_field_u128(1, margin_offsets::CAPITAL));
    body.extend(push_u128_bytes(amount));
    body.push(SUB);
    body.extend(encode_store_field_u128(1, margin_offsets::CAPITAL));
    body.extend(encode_load_field_u128(0, risk_offsets::VAULT));
    body.extend(push_u128_bytes(amount));
    body.push(SUB);
    body.extend(encode_store_field_u128(0, risk_offsets::VAULT));
    body.extend(encode_load_field_u128(0, risk_offsets::C_TOT));
    body.extend(push_u128_bytes(amount));
    body.push(SUB);
    body.extend(encode_store_field_u128(0, risk_offsets::C_TOT));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let risk_post = common::read_data(&accounts[0]);
    let margin_post = common::read_data(&accounts[1]);
    assert_eq!(
        get_u128(&margin_post, margin_offsets::CAPITAL as usize),
        initial_capital - amount,
    );
    assert_eq!(
        get_u128(&risk_post, risk_offsets::VAULT as usize),
        initial_vault - amount,
    );
    assert_eq!(
        get_u128(&risk_post, risk_offsets::C_TOT as usize),
        initial_c_tot - amount,
    );
}

/// `convert_released_pnl` arithmetic: reserved_pnl decreases, capital grows by
/// the same amount. Runs on the margin account alone.
#[test]
fn convert_released_pnl_arithmetic_moves_u128_between_fields() {
    let initial_capital: u128 = 200_000_000;
    let initial_reserved: u128 = 80_000_000;
    let amount: u128 = 30_000_000;

    let mut margin_data = vec![0u8; MARGIN_DATA_SIZE];
    put_u128(
        &mut margin_data,
        margin_offsets::CAPITAL as usize,
        initial_capital,
    );
    put_u128(
        &mut margin_data,
        margin_offsets::RESERVED_PNL as usize,
        initial_reserved,
    );

    let margin = make_vm_account(2, false, margin_data);
    let accounts = [margin];

    let mut body = Vec::new();
    body.extend(encode_load_field_u128(0, margin_offsets::RESERVED_PNL));
    body.extend(push_u128_bytes(amount));
    body.push(SUB);
    body.extend(encode_store_field_u128(0, margin_offsets::RESERVED_PNL));

    body.extend(encode_load_field_u128(0, margin_offsets::CAPITAL));
    body.extend(push_u128_bytes(amount));
    body.push(ADD);
    body.extend(encode_store_field_u128(0, margin_offsets::CAPITAL));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let post = common::read_data(&accounts[0]);
    assert_eq!(
        get_u128(&post, margin_offsets::RESERVED_PNL as usize),
        initial_reserved - amount,
    );
    assert_eq!(
        get_u128(&post, margin_offsets::CAPITAL as usize),
        initial_capital + amount,
    );
}

/// Smoke test for the bigger primitive mix in `close_account`: u16 counter
/// decrement alongside u128 vault/c_tot subtraction, followed by zeroing the
/// account. We only run the u16 half here to verify generic SUB can drive the
/// `open_account_count` field.
#[test]
fn close_account_decrements_u16_counter() {
    let mut risk_data = vec![0u8; RISK_DATA_SIZE];
    put_u16(&mut risk_data, risk_offsets::OPEN_ACCOUNT_COUNT as usize, 7);

    let risk = make_vm_account(1, false, risk_data);
    let accounts = [risk];

    // LOAD_FIELD_U16 reads exactly 2 bytes and pushes U64. Plain SUB then
    // works against the PUSH_U16 constant (which also lands on the stack as
    // U64 after VLE decode). STORE_FIELD_U16 writes low 2 bytes.
    let mut body = Vec::new();
    body.extend(encode_load_field_u16(0, risk_offsets::OPEN_ACCOUNT_COUNT));
    body.extend(&[five_protocol::opcodes::PUSH_U16, 1]); // PUSH_U16(1) via VLE
    body.push(SUB);
    body.extend(encode_store_field_u16(0, risk_offsets::OPEN_ACCOUNT_COUNT));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let post = common::read_data(&accounts[0]);
    assert_eq!(
        get_u16(&post, risk_offsets::OPEN_ACCOUNT_COUNT as usize),
        6,
        "open_account_count should decrement from 7 to 6"
    );
}

/// `execute_trade` arithmetic: taker position grows by size_q, maker
/// position shrinks by the same. Validates signed i128 round-trip through
/// LOAD_FIELD_U128 / ADD / STORE_FIELD_U128 (i128 uses the bit-identical
/// u128 wire format).
#[test]
fn execute_trade_mirrors_position_basis_between_taker_and_maker() {
    let initial_taker_pos: i128 = 50_000_000;
    let initial_maker_pos: i128 = -30_000_000;
    let size_q: i128 = 10_000_000;

    let mut taker_data = vec![0u8; MARGIN_DATA_SIZE];
    taker_data[margin_offsets::POSITION_BASIS_Q as usize
        ..margin_offsets::POSITION_BASIS_Q as usize + 16]
        .copy_from_slice(&initial_taker_pos.to_le_bytes());

    let mut maker_data = vec![0u8; MARGIN_DATA_SIZE];
    maker_data[margin_offsets::POSITION_BASIS_Q as usize
        ..margin_offsets::POSITION_BASIS_Q as usize + 16]
        .copy_from_slice(&initial_maker_pos.to_le_bytes());

    let taker = make_vm_account(2, false, taker_data);
    let maker = make_vm_account(3, false, maker_data);
    let accounts = [taker, maker];

    let mut body = Vec::new();
    // taker.position_basis_q += size_q
    body.extend(encode_load_field_u128(0, margin_offsets::POSITION_BASIS_Q));
    body.extend(push_u128_bytes(size_q as u128));
    body.push(ADD);
    body.extend(encode_store_field_u128(0, margin_offsets::POSITION_BASIS_Q));
    // maker.position_basis_q -= size_q
    body.extend(encode_load_field_u128(1, margin_offsets::POSITION_BASIS_Q));
    body.extend(push_u128_bytes(size_q as u128));
    body.push(SUB);
    body.extend(encode_store_field_u128(1, margin_offsets::POSITION_BASIS_Q));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let taker_post = common::read_data(&accounts[0]);
    let maker_post = common::read_data(&accounts[1]);
    let mut buf = [0u8; 16];
    buf.copy_from_slice(
        &taker_post[margin_offsets::POSITION_BASIS_Q as usize
            ..margin_offsets::POSITION_BASIS_Q as usize + 16],
    );
    assert_eq!(i128::from_le_bytes(buf), initial_taker_pos + size_q);
    buf.copy_from_slice(
        &maker_post[margin_offsets::POSITION_BASIS_Q as usize
            ..margin_offsets::POSITION_BASIS_Q as usize + 16],
    );
    assert_eq!(i128::from_le_bytes(buf), initial_maker_pos - size_q);
}

/// `liquidate_at_oracle` arithmetic (simplified): victim's capital moves
/// wholesale into liquidator's reserved_pnl, victim's capital and position
/// are zeroed.
#[test]
fn liquidate_at_oracle_drains_victim_into_liquidator_reserved() {
    let initial_victim_capital: u128 = 400_000_000;
    let initial_liquidator_reserved: u128 = 10_000_000;

    let mut victim_data = vec![0u8; MARGIN_DATA_SIZE];
    put_u128(
        &mut victim_data,
        margin_offsets::CAPITAL as usize,
        initial_victim_capital,
    );
    put_u128(
        &mut victim_data,
        margin_offsets::POSITION_BASIS_Q as usize,
        50_000_000u128, // nonzero position — should be flattened
    );

    let mut liquidator_data = vec![0u8; MARGIN_DATA_SIZE];
    put_u128(
        &mut liquidator_data,
        margin_offsets::RESERVED_PNL as usize,
        initial_liquidator_reserved,
    );

    let victim = make_vm_account(2, false, victim_data);
    let liquidator = make_vm_account(3, false, liquidator_data);
    let accounts = [victim, liquidator];

    // Body: liquidator.reserved_pnl += victim.capital; then zero victim.
    let mut body = Vec::new();
    body.extend(encode_load_field_u128(1, margin_offsets::RESERVED_PNL));
    body.extend(encode_load_field_u128(0, margin_offsets::CAPITAL));
    body.push(ADD);
    body.extend(encode_store_field_u128(1, margin_offsets::RESERVED_PNL));
    body.extend(push_u128_bytes(0));
    body.extend(encode_store_field_u128(0, margin_offsets::CAPITAL));
    body.extend(push_u128_bytes(0));
    body.extend(encode_store_field_u128(0, margin_offsets::POSITION_BASIS_Q));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let victim_post = common::read_data(&accounts[0]);
    let liq_post = common::read_data(&accounts[1]);
    assert_eq!(
        get_u128(&victim_post, margin_offsets::CAPITAL as usize),
        0,
    );
    assert_eq!(
        get_u128(&victim_post, margin_offsets::POSITION_BASIS_Q as usize),
        0,
    );
    assert_eq!(
        get_u128(&liq_post, margin_offsets::RESERVED_PNL as usize),
        initial_liquidator_reserved + initial_victim_capital,
    );
}

/// `settle_account` arithmetic (simplified): updates current_oracle_price
/// on the RiskEngine. Proves the crank-style state-update path works.
#[test]
fn settle_account_updates_current_oracle_price() {
    let mut risk_data = vec![0u8; RISK_DATA_SIZE];
    put_u64(
        &mut risk_data,
        risk_offsets::CURRENT_ORACLE_PRICE as usize,
        100,
    );

    let risk = make_vm_account(1, false, risk_data);
    let accounts = [risk];

    // Body: PUSH_U64 new_price; STORE_FIELD current_oracle_price.
    let new_price: u64 = 250;
    let mut body = Vec::new();
    body.extend(&[
        five_protocol::opcodes::PUSH_U64,
        new_price as u8, // VLE: 250 fits in 2 bytes
        0x01,
    ]);
    // Wait — 250 VLE = 0xFA 0x01. Let me simplify — rewrite VLE.
    body.clear();
    body.push(five_protocol::opcodes::PUSH_U64);
    let mut v = new_price;
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            body.push(b);
            break;
        }
        body.push(b | 0x80);
    }
    body.extend(encode_store_field(0, risk_offsets::CURRENT_ORACLE_PRICE));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let post = common::read_data(&accounts[0]);
    assert_eq!(
        get_u64(&post, risk_offsets::CURRENT_ORACLE_PRICE as usize),
        new_price,
    );
}

/// Validates u64 field round-trip (current_slot on RiskEngine). Simplest of
/// all — sanity-check that u64 works in the test harness.
#[test]
fn u64_field_round_trip_on_current_slot() {
    let initial_slot: u64 = 100;
    let delta_slot: u64 = 23;

    let mut risk_data = vec![0u8; RISK_DATA_SIZE];
    put_u64(
        &mut risk_data,
        risk_offsets::CURRENT_SLOT as usize,
        initial_slot,
    );

    let risk = make_vm_account(1, false, risk_data);
    let accounts = [risk];

    let mut body = Vec::new();
    body.extend(encode_load_field(0, risk_offsets::CURRENT_SLOT));
    body.extend(&[five_protocol::opcodes::PUSH_U64, delta_slot as u8]); // VLE: 23 fits in 1 byte
    body.push(ADD);
    body.extend(encode_store_field(0, risk_offsets::CURRENT_SLOT));

    let bin = vm_binary(&body);
    MitoVM::execute_direct(&bin, &[], &accounts).expect("execute_direct");

    let post = common::read_data(&accounts[0]);
    assert_eq!(
        get_u64(&post, risk_offsets::CURRENT_SLOT as usize),
        initial_slot + delta_slot,
    );
}
