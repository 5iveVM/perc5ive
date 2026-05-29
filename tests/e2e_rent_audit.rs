//! Rent audit — proves the MetaGenesis lifecycle extracts zero operator rent on
//! the **real linked binary**. Tracks the base-unit ledger across the full
//! lifecycle and asserts (a) withdrawals never exceed deposits (no base-unit
//! minting / no skim), (b) a solvent full wind-down returns every base unit to
//! depositors (operator rent = 0), (c) COIN supply conserves (minted == reward),
//! (d) draw_genesis_surplus is bounded by available surplus.
//!
//! Harness helpers are copied verbatim from `tests/e2e_meta_genesis.rs`
//! (integration tests are separate crates and cannot share private helpers).

mod common;

use common::{make_account_info, read_data, FIVE_VM_PROGRAM_ID};
use five_protocol::types;
use five_protocol::{
    DSL_RAW_ACCOUNT_HEADER_LEN, DSL_RAW_ACCOUNT_HEADER_MAGIC, DSL_RAW_ACCOUNT_HEADER_VERSION,
};
use five_vm_mito::{AccountInfo, MitoVM, Pubkey, StackStorage, Value};
use perc5ive::bytecode::meta_handlers::{
    self as mh, coin_cfg_offsets, fld, genesis_cfg_offsets, genesis_distribution_offsets,
    genesis_position_offsets, genesis_vote_offsets,
};

const HDR: usize = DSL_RAW_ACCOUNT_HEADER_LEN;

// Public-function indices = DSL pub-declaration order in meta/src/main.v.
const FN_INIT_COIN_CONFIG: u32 = 0;
const FN_ACTIVATE_LIVE: u32 = 1;
const FN_INIT_GENESIS_BOOTSTRAP: u32 = 2;
const FN_GENESIS_DEPOSIT: u32 = 3;
const FN_GENESIS_WITHDRAW: u32 = 4;
const FN_KICKSTART: u32 = 5;
const FN_INIT_DISTRIBUTION: u32 = 6;
const FN_VOTE: u32 = 7;
const FN_MINT_REWARD: u32 = 8;
const FN_FINALIZE: u32 = 9;
const FN_DRAW_SURPLUS: u32 = 10;

fn script_key() -> Pubkey {
    let mut k = [0u8; 32];
    k[0] = 0x5C;
    k[31] = 0x9E;
    k
}

/// VM-owned data account: `[40-byte 5RAW header bound to script_key][logical
/// struct, generously padded so 8-byte field reads never overrun]`.
fn data_account(tag: u8, logical_size: usize) -> AccountInfo {
    let mut data = vec![0u8; HDR + logical_size + 16];
    data[0..4].copy_from_slice(&DSL_RAW_ACCOUNT_HEADER_MAGIC);
    data[4] = DSL_RAW_ACCOUNT_HEADER_VERSION;
    data[8..40].copy_from_slice(&script_key());
    let mut key = [0u8; 32];
    key[30] = tag;
    key[31] = 0xDA;
    make_account_info(key, FIVE_VM_PROGRAM_ID, false, true, 1_000_000, data)
}

/// A signer/identity account the handler bodies never field-access.
fn signer_account(tag: u8) -> AccountInfo {
    let mut key = [0u8; 32];
    key[31] = tag;
    make_account_info(key, [9u8; 32], true, false, 1, vec![0u8; 8])
}

fn script_account() -> AccountInfo {
    make_account_info(script_key(), [7u8; 32], false, false, 1, vec![0u8; 8])
}

/// Encode the typed-param envelope for `scalars` (all u64) selecting `func_idx`.
fn input_u64s(func_idx: u32, scalars: &[u64]) -> Vec<u8> {
    let mut input = Vec::new();
    input.extend_from_slice(&func_idx.to_le_bytes());
    input.extend_from_slice(&(scalars.len() as u32).to_le_bytes());
    for &s in scalars {
        input.push(types::U64);
        input.extend_from_slice(&s.to_le_bytes());
    }
    input
}

fn call_ok(linked: &[u8], func_idx: u32, scalars: &[u64], accounts: &[AccountInfo], what: &str) {
    let status = call(linked, func_idx, scalars, accounts, what);
    assert_eq!(status, 0, "{what}: expected status 0, got {status}");
}

fn call(linked: &[u8], func_idx: u32, scalars: &[u64], accounts: &[AccountInfo], what: &str) -> u64 {
    let input = input_u64s(func_idx, scalars);
    let res = MitoVM::execute_direct(
        linked,
        &input,
        accounts,
        &FIVE_VM_PROGRAM_ID,
        &mut StackStorage::new(),
    )
    .unwrap_or_else(|e| panic!("{what}: VM errored: {e:?}"))
    .unwrap_or_else(|| panic!("{what}: no return value"));
    match res {
        Value::U64(v) => v,
        Value::U128(v) => v as u64,
        other => panic!("{what}: unexpected return {other:?}"),
    }
}

fn u64_at(acct: &AccountInfo, logical: u32) -> u64 {
    let data = read_data(acct);
    let off = fld(logical) as usize;
    u64::from_le_bytes(data[off..off + 8].try_into().unwrap())
}

#[allow(dead_code)]
fn u8_at(acct: &AccountInfo, logical: u32) -> u8 {
    read_data(acct)[fld(logical) as usize]
}

fn linked_meta() -> Vec<u8> {
    let fbin = std::fs::read("target/meta.fbin")
        .expect("target/meta.fbin missing — build.rs regenerates it on cargo build");
    mh::link_meta(&fbin).expect("link_meta rewrites every genesis sentinel")
}

/// Conservation guard: withdrawals must never exceed deposits at any point.
fn assert_no_overdraw(genesis_cfg: &AccountInfo, step: &str) {
    use genesis_cfg_offsets as gc;
    let dep = u64_at(genesis_cfg, gc::TOTAL_DEPOSITED);
    let wd = u64_at(genesis_cfg, gc::TOTAL_WITHDRAWN);
    assert!(wd <= dep, "{step}: withdrawn {wd} > deposited {dep} — base units minted/skimmed");
}

#[test]
fn lifecycle_returns_every_base_unit_zero_operator_rent() {
    use genesis_cfg_offsets as gc;
    use genesis_position_offsets as gp;

    let linked = linked_meta();
    let script = script_account();
    let coin_cfg = data_account(1, coin_cfg_offsets::SIZE);
    let genesis_cfg = data_account(2, genesis_cfg_offsets::SIZE);
    let alice_pos = data_account(3, genesis_position_offsets::SIZE);
    let bob_pos = data_account(4, genesis_position_offsets::SIZE);
    let dist1 = data_account(6, genesis_distribution_offsets::SIZE);
    let alice_v1 = data_account(8, genesis_vote_offsets::SIZE);
    let bob_v1 = data_account(9, genesis_vote_offsets::SIZE);
    let authority = signer_account(0x41);
    let alice = signer_account(0x42);
    let bob = signer_account(0x43);

    call_ok(&linked, FN_INIT_COIN_CONFIG, &[50, 100],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "init_coin_config");
    call_ok(&linked, FN_INIT_GENESIS_BOOTSTRAP, &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "bootstrap");

    for (pos, amt, who) in [(&alice_pos, 6u64, "alice"), (&bob_pos, 4, "bob")] {
        call_ok(&linked, FN_GENESIS_DEPOSIT, &[amt, 100],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], who);
        assert_no_overdraw(&genesis_cfg, who);
    }
    assert_eq!(u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED), 10);

    call_ok(&linked, FN_KICKSTART, &[0, 1000],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "kickstart");
    assert_no_overdraw(&genesis_cfg, "post-kickstart");

    call_ok(&linked, FN_ACTIVATE_LIVE, &[150],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "activate_live");
    call_ok(&linked, FN_INIT_DISTRIBUTION, &[1, 100],
        &[script.clone(), genesis_cfg.clone(), dist1.clone(), authority.clone()], "init_dist1");
    for (voter, pos, vrec, who) in
        [(&alice, &alice_pos, &alice_v1, "a"), (&bob, &bob_pos, &bob_v1, "b")]
    {
        call_ok(&linked, FN_VOTE, &[1, 160],
            &[script.clone(), pos.clone(), dist1.clone(), vrec.clone(), voter.clone()], who);
    }
    call_ok(&linked, FN_MINT_REWARD, &[100],
        &[script.clone(), genesis_cfg.clone(), dist1.clone(), authority.clone()], "mint");
    assert_eq!(
        u64_at(&genesis_cfg, gc::MINTED_SUPPLY),
        u64_at(&genesis_cfg, gc::REWARD_SUPPLY),
        "COIN supply conserved: minted == reward"
    );

    call_ok(&linked, FN_FINALIZE, &[],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "finalize");

    // Solvent vault (== pool, 10) → both recover full principal.
    for (pos, who) in [(&alice_pos, "alice"), (&bob_pos, "bob")] {
        call_ok(&linked, FN_GENESIS_WITHDRAW, &[1, 0, 10],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], who);
        assert_no_overdraw(&genesis_cfg, who);
    }

    // Zero operator rent: every deposited base unit came back to depositors.
    let dep = u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED);
    let wd = u64_at(&genesis_cfg, gc::TOTAL_WITHDRAWN);
    assert_eq!(wd, dep, "solvent wind-down returns 100% of principal — operator rent = 0");
    assert_eq!(u64_at(&alice_pos, gp::WITHDRAWN), 6);
    assert_eq!(u64_at(&bob_pos, gp::WITHDRAWN), 4);
}

#[test]
fn surplus_draw_is_bounded_no_rent_leak() {
    use genesis_cfg_offsets as gc;
    let linked = linked_meta();
    let script = script_account();
    let coin_cfg = data_account(1, coin_cfg_offsets::SIZE);
    let genesis_cfg = data_account(2, genesis_cfg_offsets::SIZE);
    let pos = data_account(3, genesis_position_offsets::SIZE);
    let dist = data_account(6, genesis_distribution_offsets::SIZE);
    let vrec = data_account(8, genesis_vote_offsets::SIZE);
    let authority = signer_account(0x41);
    let alice = signer_account(0x42);

    // Drive the lifecycle to finalize (draw_genesis_surplus guards on finalized).
    call_ok(&linked, FN_INIT_COIN_CONFIG, &[50, 100],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "init_coin_config");
    call_ok(&linked, FN_INIT_GENESIS_BOOTSTRAP, &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "bootstrap");
    call_ok(&linked, FN_GENESIS_DEPOSIT, &[10, 100],
        &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], "deposit");
    call_ok(&linked, FN_KICKSTART, &[0, 1000],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "kickstart");
    call_ok(&linked, FN_ACTIVATE_LIVE, &[150],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "activate_live");
    call_ok(&linked, FN_INIT_DISTRIBUTION, &[1, 100],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), authority.clone()], "init_dist");
    call_ok(&linked, FN_VOTE, &[1, 160],
        &[script.clone(), pos.clone(), dist.clone(), vrec.clone(), alice.clone()], "vote");
    call_ok(&linked, FN_MINT_REWARD, &[100],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), authority.clone()], "mint");
    call_ok(&linked, FN_FINALIZE, &[],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "finalize");

    // Finalized, nobody withdrew → outstanding = 10. Drawing 50 against a vault
    // of 10 (available = 10 - 10 = 0) must be rejected, not skimmed.
    let s = call(&linked, FN_DRAW_SURPLUS, &[50, 10],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "overdraw");
    assert_eq!(s, mh::STATUS_INSUFFICIENT_SURPLUS, "surplus draw bounded by vault - outstanding");
    assert_no_overdraw(&genesis_cfg, "post-overdraw-attempt");
    let _ = gc::TOTAL_DEPOSITED;
}
