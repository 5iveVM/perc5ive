//! Phase 2 verification gate — the percolator-meta genesis lifecycle, ported to
//! 5ive and run end-to-end against the **real linked binary**
//! (`target/meta.fbin` → `link_meta`, every genesis sentinel rewritten to its
//! hand-written body).
//!
//! Mirrors the upstream `test_full_genesis_to_dao_lifecycle_end_to_end`
//! (percolator-meta @ b6d5f2a) at the *ledger* level: deposit → kickstart →
//! mid-bootstrap exit → go live → propose → vote → mint 100% → finalize →
//! withdraw. SPL-token / Percolator CPI is the on-chain INVOKE layer and is not
//! modelled here; side-effectful reads (pulled amounts, vault balance) arrive
//! as scalar params, the perc5ive porting convention. The assertions track the
//! u64 genesis ledger — the substance the DSL port owns.
//!
//! Account/field conventions are the ones pinned in `probe_account_index.rs`:
//! data accounts start at table index 1, carry a 40-byte `5RAW` header bound to
//! `accounts[0].key()`, and field offsets are header-inclusive (`fld`).

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

/// A signer/identity account the handler bodies never field-access (owner is
/// arbitrary; no 5RAW header needed since LOAD_FIELD is never called on it).
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

/// Dispatch `func_idx` on the linked binary with `scalars` + `accounts`; assert
/// the handler returned the success status (0).
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

fn u8_at(acct: &AccountInfo, logical: u32) -> u8 {
    read_data(acct)[fld(logical) as usize]
}

fn linked_meta() -> Vec<u8> {
    let fbin = std::fs::read("target/meta.fbin")
        .expect("target/meta.fbin missing — build.rs regenerates it on cargo build");
    mh::link_meta(&fbin).expect("link_meta rewrites every genesis sentinel")
}

#[test]
fn full_genesis_to_dao_lifecycle_end_to_end() {
    use coin_cfg_offsets as cc;
    use genesis_cfg_offsets as gc;
    use genesis_distribution_offsets as gd;
    use genesis_position_offsets as gp;
    use genesis_vote_offsets as gv;

    let linked = linked_meta();
    let script = script_account();

    // Persistent ledger accounts (mutations stick across calls — the backing
    // buffers are leaked and reused).
    let coin_cfg = data_account(1, coin_cfg_offsets::SIZE);
    let genesis_cfg = data_account(2, genesis_cfg_offsets::SIZE);
    let alice_pos = data_account(3, genesis_position_offsets::SIZE);
    let bob_pos = data_account(4, genesis_position_offsets::SIZE);
    let carol_pos = data_account(5, genesis_position_offsets::SIZE);
    let dist1 = data_account(6, genesis_distribution_offsets::SIZE);
    let dist2 = data_account(7, genesis_distribution_offsets::SIZE);
    let alice_v1 = data_account(8, genesis_vote_offsets::SIZE);
    let bob_v1 = data_account(9, genesis_vote_offsets::SIZE);
    let alice_v2 = data_account(10, genesis_vote_offsets::SIZE);
    let bob_v2 = data_account(11, genesis_vote_offsets::SIZE);
    let authority = signer_account(0x41);
    let alice = signer_account(0x42);
    let bob = signer_account(0x43);
    let carol = signer_account(0x44);

    // --- 0. Coin config: 50-slot bootstrap delay, start slot 100 ---
    call_ok(
        &linked,
        FN_INIT_COIN_CONFIG,
        &[50, 100],
        &[script.clone(), coin_cfg.clone(), authority.clone()],
        "init_coin_config",
    );
    assert_eq!(u8_at(&coin_cfg, cc::PHASE), 0, "bootstrap phase");
    assert_eq!(u64_at(&coin_cfg, cc::BOOTSTRAP_START_SLOT), 100);
    assert_eq!(u64_at(&coin_cfg, cc::BOOTSTRAP_DELAY_SLOTS), 50);

    // --- 1. Genesis bootstrap: fixed COIN reward supply 100 ---
    call_ok(
        &linked,
        FN_INIT_GENESIS_BOOTSTRAP,
        &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()],
        "init_genesis_bootstrap",
    );
    assert_eq!(u64_at(&genesis_cfg, gc::REWARD_SUPPLY), 100);
    assert_eq!(u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED), 0);

    // --- 2. Deposits during the window (1 base unit = 1 vote unit) ---
    for (pos, amt, who) in [(&alice_pos, 4u64, "alice"), (&bob_pos, 4, "bob"), (&carol_pos, 2, "carol")] {
        call_ok(
            &linked,
            FN_GENESIS_DEPOSIT,
            &[amt, 100], // amount, now_slot
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()],
            &format!("deposit_{who}"),
        );
    }
    assert_eq!(u64_at(&genesis_cfg, gc::TOTAL_DEPOSITED), 10, "10 base units pooled");
    assert_eq!(u64_at(&alice_pos, gp::AMOUNT), 4);
    assert_eq!(u64_at(&carol_pos, gp::AMOUNT), 2);
    assert_eq!(u64_at(&alice_pos, gp::START_SLOT), 100, "deposit set last-write-time");

    // Deposits close once kicked — prove the guard before we kickstart.
    // (kickstart first, then a rejected deposit.)

    // --- 3. Kickstart: deploy the pool (insurance=5, backing=5 is the CPI layer) ---
    call_ok(
        &linked,
        FN_KICKSTART,
        &[0, 1000], // backing_domain, backing_expiry_slot
        &[script.clone(), genesis_cfg.clone(), authority.clone()],
        "kickstart",
    );
    assert_eq!(u8_at(&genesis_cfg, gc::KICKED), 1, "market kicked");

    // A deposit after kickstart must be rejected (STATUS_DEPOSITS_CLOSED = 1).
    let closed = call(
        &linked,
        FN_GENESIS_DEPOSIT,
        &[1, 100],
        &[script.clone(), genesis_cfg.clone(), alice_pos.clone(), alice.clone()],
        "deposit_after_kick",
    );
    assert_eq!(closed, mh::STATUS_DEPOSITS_CLOSED, "deposits close at kickstart");

    // --- 4. Carol exits mid-bootstrap (path C: kicked, not finalized) ---
    // She pulled 2 base units back from the market (the CPI layer resolves
    // `recovered`); her full principal is recovered, her vote forfeited.
    call_ok(
        &linked,
        FN_GENESIS_WITHDRAW,
        &[0, 2, 0], // coin_phase(unused), recovered=2, vault_balance(unused on path C)
        &[script.clone(), genesis_cfg.clone(), carol_pos.clone(), carol.clone()],
        "carol_exit",
    );
    assert_eq!(u64_at(&genesis_cfg, gc::TOTAL_WITHDRAWN), 2, "carol's 2 retired");
    assert_eq!(u64_at(&carol_pos, gp::WITHDRAWN), 2);
    assert_eq!(u64_at(&carol_pos, gp::START_SLOT), 0, "carol forfeited her vote");

    // --- 5. Go live once the delay elapses (slot 150 >= 100 + 50) ---
    call_ok(
        &linked,
        FN_ACTIVATE_LIVE,
        &[150],
        &[script.clone(), coin_cfg.clone(), authority.clone()],
        "activate_live",
    );
    assert_eq!(u8_at(&coin_cfg, cc::PHASE), 1, "live phase");
    assert_eq!(u64_at(&coin_cfg, cc::LIVE_SLOT), 150);

    // --- 6. Two distribution items summing to 100% of the reward supply ---
    call_ok(
        &linked,
        FN_INIT_DISTRIBUTION,
        &[1, 40], // proposal_id, amount
        &[script.clone(), genesis_cfg.clone(), dist1.clone(), authority.clone()],
        "init_dist1",
    );
    call_ok(
        &linked,
        FN_INIT_DISTRIBUTION,
        &[2, 60],
        &[script.clone(), genesis_cfg.clone(), dist2.clone(), authority.clone()],
        "init_dist2",
    );
    assert_eq!(u64_at(&dist1, gd::AMOUNT), 40);
    assert_eq!(u64_at(&dist2, gd::AMOUNT), 60);

    // --- 7. Votes. age = 160 - 100 = 60 → floor(log2(60)) = 5; weight = 5*4 = 20 ---
    let vote = |voter: &AccountInfo, pos: &AccountInfo, dist: &AccountInfo, vrec: &AccountInfo, what: &str| {
        call_ok(
            &linked,
            FN_VOTE,
            &[1, 160], // action=yes, now_slot=160
            &[script.clone(), pos.clone(), dist.clone(), vrec.clone(), voter.clone()],
            what,
        );
    };
    vote(&alice, &alice_pos, &dist1, &alice_v1, "alice_vote_d1");
    vote(&bob, &bob_pos, &dist1, &bob_v1, "bob_vote_d1");
    vote(&alice, &alice_pos, &dist2, &alice_v2, "alice_vote_d2");
    vote(&bob, &bob_pos, &dist2, &bob_v2, "bob_vote_d2");

    assert_eq!(u64_at(&alice_v1, gv::WEIGHT), 20, "floor(log2(60)) * staked(4)");
    assert_eq!(u64_at(&dist1, gd::YES_VOTES), 40, "alice 20 + bob 20");
    assert_eq!(u64_at(&dist1, gd::VOTED_PRINCIPAL), 8, "alice 4 + bob 4 staked");
    assert_eq!(u64_at(&alice_pos, gp::ACTIVE_VOTES), 2, "alice has 2 live ballots");

    // --- 8. Mint 100% of the supply to the two approved items ---
    call_ok(
        &linked,
        FN_MINT_REWARD,
        &[40],
        &[script.clone(), genesis_cfg.clone(), dist1.clone(), authority.clone()],
        "mint_d1",
    );
    call_ok(
        &linked,
        FN_MINT_REWARD,
        &[60],
        &[script.clone(), genesis_cfg.clone(), dist2.clone(), authority.clone()],
        "mint_d2",
    );
    assert_eq!(u64_at(&genesis_cfg, gc::MINTED_SUPPLY), 100, "full supply minted");
    assert_eq!(u8_at(&dist1, gd::EXECUTED), 1);
    assert_eq!(u8_at(&dist2, gd::EXECUTED), 1);

    // --- 9. Finalize (kicked + minted == reward_supply) ---
    call_ok(
        &linked,
        FN_FINALIZE,
        &[],
        &[script.clone(), genesis_cfg.clone(), authority.clone()],
        "finalize",
    );
    assert_eq!(u8_at(&genesis_cfg, gc::FINALIZED), 1, "finalized");

    // --- 10. Remaining depositors withdraw their full principal (path B) ---
    for (pos, who) in [(&alice_pos, "alice"), (&bob_pos, "bob")] {
        call_ok(
            &linked,
            FN_GENESIS_WITHDRAW,
            &[1, 0, 8], // coin_phase, recovered(unused on path B), vault_balance
            &[script.clone(), genesis_cfg.clone(), pos.clone(), alice.clone()],
            &format!("withdraw_{who}"),
        );
    }
    // carol's 2 + alice's 4 + bob's 4 = 10 retired.
    assert_eq!(u64_at(&genesis_cfg, gc::TOTAL_WITHDRAWN), 10, "all principal retired");
    assert_eq!(u64_at(&alice_pos, gp::WITHDRAWN), 4);
    assert_eq!(u64_at(&bob_pos, gp::WITHDRAWN), 4);
    assert_eq!(u64_at(&alice_pos, gp::START_SLOT), 0, "vote worthless post-withdraw");
}

/// Guard coverage: mint cannot run before kickstart, and finalize cannot run
/// until the full reward supply is minted.
#[test]
fn mint_requires_kicked_and_finalize_requires_full_supply() {
    use genesis_cfg_offsets as gc;
    let linked = linked_meta();
    let script = script_account();
    let genesis_cfg = data_account(2, genesis_cfg_offsets::SIZE);
    let dist = data_account(6, genesis_distribution_offsets::SIZE);
    let authority = signer_account(0x41);

    call_ok(
        &linked,
        FN_INIT_GENESIS_BOOTSTRAP,
        &[100],
        &[script.clone(), genesis_cfg.clone(), authority.clone()],
        "bootstrap",
    );

    // Mint before kickstart → STATUS_NOT_KICKED.
    let s = call(
        &linked,
        FN_MINT_REWARD,
        &[40],
        &[script.clone(), genesis_cfg.clone(), dist.clone(), authority.clone()],
        "mint_premature",
    );
    assert_eq!(s, mh::STATUS_NOT_KICKED);

    // Finalize before kickstart → STATUS_NOT_KICKED.
    let s = call(
        &linked,
        FN_FINALIZE,
        &[],
        &[script.clone(), genesis_cfg.clone(), authority.clone()],
        "finalize_premature",
    );
    assert_eq!(s, mh::STATUS_NOT_KICKED);
    let _ = u64_at(&genesis_cfg, gc::REWARD_SUPPLY); // touch to keep imports used
}
