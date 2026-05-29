//! Watermelon isolation demo — two independent genesis lifecycles run against
//! the one linked binary under a shared futarchy pattern. Proves the markets are
//! **isolated** (distinct ledgers, neither bleeds into the other) and **rent
//! free** (each solvent wind-down returns 100% of principal, operator rent = 0),
//! matching the `meta_math` aggregate model. No governance bytecode is exercised
//! — markets are isolated by construction (separate Percolator markets under
//! separate PDAs); the governed market-factory layer is deferred to its own spec.
//!
//! Harness helpers are copied verbatim from `tests/e2e_meta_genesis.rs`.

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

fn script_key() -> Pubkey {
    let mut k = [0u8; 32];
    k[0] = 0x5C;
    k[31] = 0x9E;
    k
}

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

fn signer_account(tag: u8) -> AccountInfo {
    let mut key = [0u8; 32];
    key[31] = tag;
    make_account_info(key, [9u8; 32], true, false, 1, vec![0u8; 8])
}

fn script_account() -> AccountInfo {
    make_account_info(script_key(), [7u8; 32], false, false, 1, vec![0u8; 8])
}

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
    let status = match res {
        Value::U64(v) => v,
        Value::U128(v) => v as u64,
        other => panic!("{what}: unexpected return {other:?}"),
    };
    assert_eq!(status, 0, "{what}: expected status 0, got {status}");
}

fn u64_at(acct: &AccountInfo, logical: u32) -> u64 {
    let data = read_data(acct);
    let off = fld(logical) as usize;
    u64::from_le_bytes(data[off..off + 8].try_into().unwrap())
}

fn linked_meta() -> Vec<u8> {
    let fbin = std::fs::read("target/meta.fbin")
        .expect("target/meta.fbin missing — build.rs regenerates it on cargo build");
    mh::link_meta(&fbin).expect("link_meta rewrites every genesis sentinel")
}

/// Run a minimal solvent genesis lifecycle on a disjoint account set keyed off
/// `base`, and return `(total_deposited, total_withdrawn)` from its genesis_cfg.
/// Two calls with different `base` share only the linked binary — distinct data
/// accounts mean distinct ledgers (isolation by construction).
fn run_isolated_genesis(linked: &[u8], base: u8, deposit_a: u64, deposit_b: u64) -> (u64, u64) {
    use genesis_cfg_offsets as gc;
    let script = script_account();
    let coin_cfg = data_account(base, coin_cfg_offsets::SIZE);
    let genesis_cfg = data_account(base + 1, genesis_cfg_offsets::SIZE);
    let pa = data_account(base + 2, genesis_position_offsets::SIZE);
    let pb = data_account(base + 3, genesis_position_offsets::SIZE);
    let dist = data_account(base + 4, genesis_distribution_offsets::SIZE);
    let va = data_account(base + 5, genesis_vote_offsets::SIZE);
    let vb = data_account(base + 6, genesis_vote_offsets::SIZE);
    let authority = signer_account(base.wrapping_add(0x40));
    let alice = signer_account(base.wrapping_add(0x41));
    let bob = signer_account(base.wrapping_add(0x42));
    let total = deposit_a + deposit_b;

    call_ok(linked, FN_INIT_COIN_CONFIG, &[50, 100],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "init");
    call_ok(linked, FN_INIT_GENESIS_BOOTSTRAP, &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "bootstrap");
    call_ok(linked, FN_GENESIS_DEPOSIT, &[deposit_a, 100],
        &[script.clone(), genesis_cfg.clone(), pa.clone(), alice.clone()], "dep_a");
    call_ok(linked, FN_GENESIS_DEPOSIT, &[deposit_b, 100],
        &[script.clone(), genesis_cfg.clone(), pb.clone(), bob.clone()], "dep_b");
    call_ok(linked, FN_KICKSTART, &[0, 1000],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "kick");
    call_ok(linked, FN_ACTIVATE_LIVE, &[150],
        &[script.clone(), coin_cfg.clone(), authority.clone()], "live");
    call_ok(linked, FN_INIT_DISTRIBUTION, &[1, 100],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), authority.clone()], "dist");
    call_ok(linked, FN_VOTE, &[1, 160],
        &[script.clone(), pa.clone(), dist.clone(), va.clone(), alice.clone()], "va");
    call_ok(linked, FN_VOTE, &[1, 160],
        &[script.clone(), pb.clone(), dist.clone(), vb.clone(), bob.clone()], "vb");
    call_ok(linked, FN_MINT_REWARD, &[100],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), authority.clone()], "mint");
    call_ok(linked, FN_FINALIZE, &[],
        &[script.clone(), genesis_cfg.clone(), authority.clone()], "finalize");
    for (pos, who) in [(&pa, "wa"), (&pb, "wb")] {
        call_ok(linked, FN_GENESIS_WITHDRAW, &[1, 0, total],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()], who);
    }
    (u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED), u64_at(&genesis_cfg, gc::TOTAL_WITHDRAWN))
}

#[test]
fn two_markets_under_one_futarchy_are_isolated_and_rent_free() {
    let linked = linked_meta();

    // Market A: pool of 10. Market B: pool of 30. Disjoint account tags.
    let (a_dep, a_wd) = run_isolated_genesis(&linked, 1, 6, 4);
    let (b_dep, b_wd) = run_isolated_genesis(&linked, 21, 20, 10);

    // Isolation: distinct pools, neither bled into the other.
    assert_eq!(a_dep, 10, "market A pool");
    assert_eq!(b_dep, 30, "market B pool independent of A");
    assert_ne!(a_dep, b_dep, "markets do not share a ledger");

    // Rent-free in both: solvent wind-down returns 100% to depositors.
    assert_eq!(a_wd, a_dep, "market A operator rent = 0");
    assert_eq!(b_wd, b_dep, "market B operator rent = 0");

    // Aggregate matches the meta_math model: 40 in, 40 returned, 0 operator rent.
    use perc5ive::bytecode::meta_math::{aggregate_rent, rent_breakdown};
    let agg = aggregate_rent(&[rent_breakdown(&[6, 4], 10), rent_breakdown(&[20, 10], 30)]);
    assert_eq!(agg.total_in, a_dep + b_dep);
    assert_eq!(agg.returned_to_users, a_wd + b_wd);
    assert_eq!(agg.operator_rent, 0);
}
