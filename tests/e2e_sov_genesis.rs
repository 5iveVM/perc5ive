//! Phase 5 (HERO DEMO) — **Sov fair-launched through percolator-meta genesis.**
//!
//! Sov is an inverted memecoin perp whose trust story is "admin key burned at
//! launch." percolator-meta genesis *is* that launch mechanism: instead of a
//! founder calling `init_sov_market` and (hopefully) burning the key, the SOV
//! COIN is born through a community Sybil-bond → vote → mint → finalize loop,
//! and the market is kicked under a PDA admin the DAO inherits. The burned-key
//! property comes for free — no privileged signer ever holds mint or custody
//! authority after finalize.
//!
//! This composes the Phase-2 genesis lifecycle (run against the real linked
//! `meta.fbin`) with Sov framing and asserts the properties that make the
//! launch trust-minimized, rather than re-deriving the generic ledger math
//! (covered by `e2e_meta_genesis`). On-chain market-creation / token-custody
//! CPI is the INVOKE layer; here we prove the genesis ledger enforces the
//! fair-launch invariants.

mod common;

use common::{make_account_info, read_data, FIVE_VM_PROGRAM_ID};
use five_protocol::types;
use five_protocol::{
    DSL_RAW_ACCOUNT_HEADER_LEN, DSL_RAW_ACCOUNT_HEADER_MAGIC, DSL_RAW_ACCOUNT_HEADER_VERSION,
};
use five_vm_mito::{AccountInfo, MitoVM, Pubkey, StackStorage, Value};
use perc5ive::bytecode::meta_handlers::{
    self as mh, coin_cfg_offsets as cc, fld, genesis_cfg_offsets as gc,
    genesis_distribution_offsets as gd, genesis_position_offsets as gp,
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

fn script_key() -> Pubkey {
    let mut k = [0u8; 32];
    k[0] = 0x50; // 'P' — perc5ive SOV launch program
    k[31] = 0xC0;
    k
}

fn data_account(tag: u8, logical_size: usize) -> AccountInfo {
    let mut data = vec![0u8; HDR + logical_size + 16];
    data[0..4].copy_from_slice(&DSL_RAW_ACCOUNT_HEADER_MAGIC);
    data[4] = DSL_RAW_ACCOUNT_HEADER_VERSION;
    data[8..40].copy_from_slice(&script_key());
    let mut key = [0u8; 32];
    key[30] = tag;
    key[31] = 0x50;
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

fn call(linked: &[u8], func: u32, scalars: &[u64], accounts: &[AccountInfo], what: &str) -> u64 {
    let res = MitoVM::execute_direct(
        linked,
        &input_u64s(func, scalars),
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

fn call_ok(linked: &[u8], func: u32, scalars: &[u64], accounts: &[AccountInfo], what: &str) {
    let s = call(linked, func, scalars, accounts, what);
    assert_eq!(s, 0, "{what}: expected success, got status {s}");
}

fn u64_at(acct: &AccountInfo, logical: u32) -> u64 {
    let data = read_data(acct);
    let off = fld(logical) as usize;
    u64::from_le_bytes(data[off..off + 8].try_into().unwrap())
}

fn linked_meta() -> Vec<u8> {
    let fbin = std::fs::read("target/meta.fbin").expect("target/meta.fbin (build.rs)");
    mh::link_meta(&fbin).expect("link_meta")
}

/// The SOV fair-launch: a community bonds capital, the market is kicked under a
/// PDA admin, the community votes the full COIN distribution to itself, mints
/// 100%, and finalizes — after which every depositor recovers their principal
/// and the COIN supply is fully community-minted with no admin lever left.
#[test]
fn sov_fair_launch_through_genesis() {
    let linked = linked_meta();
    let script = script_account();

    // SOV_SUPPLY = 1_000_000 COIN units; three community bonders.
    const SOV_SUPPLY: u64 = 1_000_000;
    let coin_cfg = data_account(1, cc::SIZE);
    let genesis_cfg = data_account(2, gc::SIZE);
    let dao = signer_account(0xDA); // the configured governance authority

    // 1. SOV COIN config — 100-slot bootstrap delay, start slot 1000.
    call_ok(
        &linked,
        FN_INIT_COIN_CONFIG,
        &[100, 1000],
        &[script.clone(), coin_cfg.clone(), dao.clone()],
        "init SOV coin config",
    );
    assert_eq!(u64_at(&coin_cfg, cc::PHASE), 0, "starts in bootstrap (not yet tradable)");

    // 2. Genesis bootstrap with the fixed SOV reward supply.
    call_ok(
        &linked,
        FN_INIT_GENESIS_BOOTSTRAP,
        &[SOV_SUPPLY],
        &[script.clone(), genesis_cfg.clone(), dao.clone()],
        "bootstrap SOV genesis",
    );
    assert_eq!(u64_at(&genesis_cfg, gc::REWARD_SUPPLY), SOV_SUPPLY);

    // 3. The community bonds memecoin collateral (the Sybil bond / insurance +
    //    backing seed for Sov's inverted market). Three bonders, deposit slot 1000.
    let bonders = [(signer_account(0xB1), data_account(10, gp::SIZE), 500u64),
                   (signer_account(0xB2), data_account(11, gp::SIZE), 300),
                   (signer_account(0xB3), data_account(12, gp::SIZE), 200)];
    for (who, pos, amt) in &bonders {
        call_ok(
            &linked,
            FN_GENESIS_DEPOSIT,
            &[*amt, 1000],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), who.clone()],
            "community bond",
        );
    }
    assert_eq!(u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED), 1000, "1000 units bonded");

    // 4. Kickstart: Sov's inverted memecoin perp market is born, split 50/50
    //    insurance/backing, under the genesis PDA admin (the CPI layer). The
    //    only ledger effect is the kicked flag — no founder key is involved.
    call_ok(
        &linked,
        FN_KICKSTART,
        &[0, 100_000],
        &[script.clone(), genesis_cfg.clone(), dao.clone()],
        "kickstart SOV market",
    );
    assert_eq!(u64_at(&genesis_cfg, gc::KICKED), 1, "Sov market live under PDA admin");

    // 5. Go live (voting opens) once the bootstrap delay elapses.
    call_ok(
        &linked,
        FN_ACTIVATE_LIVE,
        &[1100],
        &[script.clone(), coin_cfg.clone(), dao.clone()],
        "activate SOV",
    );
    assert_eq!(u64_at(&coin_cfg, cc::PHASE), 1, "SOV COIN live");

    // 6. Community proposes + votes the FULL supply to itself (one distribution
    //    item here for clarity). age = 1200-1000 = 200 → floor(log2(200))=7.
    let dist = data_account(20, gd::SIZE);
    call_ok(
        &linked,
        FN_INIT_DISTRIBUTION,
        &[1, SOV_SUPPLY],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), dao.clone()],
        "propose full SOV distribution",
    );
    for (i, (who, pos, _)) in bonders.iter().enumerate() {
        let vrec = data_account(30 + i as u8, perc5ive::bytecode::meta_handlers::genesis_vote_offsets::SIZE);
        call_ok(
            &linked,
            FN_VOTE,
            &[1, 1200], // yes, now_slot
            &[script.clone(), pos.clone(), dist.clone(), vrec.clone(), who.clone()],
            "community vote yes",
        );
    }
    // All three bonders (1000 principal) backed it; quorum needs > outstanding/2 = 500.
    assert_eq!(u64_at(&dist, gd::VOTED_PRINCIPAL), 1000, "whole community voted");

    // 7. Mint the full supply to the approved item.
    call_ok(
        &linked,
        FN_MINT_REWARD,
        &[SOV_SUPPLY],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), dao.clone()],
        "mint full SOV supply",
    );
    assert_eq!(u64_at(&genesis_cfg, gc::MINTED_SUPPLY), SOV_SUPPLY, "100% minted by community");

    // 8. Finalize — the moment the keys pass to the MetaDAO. Mint authority is
    //    spent (minted == reward_supply) and no further privileged mint is
    //    possible. This is Sov's "admin key burned," enforced by the protocol.
    call_ok(
        &linked,
        FN_FINALIZE,
        &[],
        &[script.clone(), genesis_cfg.clone(), dao.clone()],
        "finalize → keys to MetaDAO",
    );
    assert_eq!(u64_at(&genesis_cfg, gc::FINALIZED), 1, "SOV launch finalized — admin burned");

    // --- Trust-minimization invariants (Sov's value-add) ---

    // (a) Supply is fully community-minted and capped: minting more must fail
    //     (already finalized). No admin can inflate SOV post-launch.
    let s = call(
        &linked,
        FN_MINT_REWARD,
        &[1],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), dao.clone()],
        "post-finalize mint attempt",
    );
    assert_eq!(s, mh::STATUS_ALREADY_FINALIZED, "no inflation after launch");

    // (b) Bonders recover their principal from the refunded vault (path B). The
    //     bond was capital-at-risk, not a sale — every unit is reclaimable.
    let vault_balance = 1000; // DAO recovered the market principal back to the vault
    for (who, pos, amt) in &bonders {
        call_ok(
            &linked,
            FN_GENESIS_WITHDRAW,
            &[1, 0, vault_balance],
            &[script.clone(), genesis_cfg.clone(), pos.clone(), who.clone()],
            "bonder reclaim",
        );
        assert_eq!(u64_at(pos, gp::WITHDRAWN), *amt, "full principal reclaimable");
    }
    assert_eq!(
        u64_at(&genesis_cfg, gc::TOTAL_WITHDRAWN),
        1000,
        "all bonded principal retired — no value captured by a founder"
    );
}
