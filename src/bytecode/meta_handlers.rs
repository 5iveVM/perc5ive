//! Hand-written bytecode for percolator-meta's genesis-lifecycle handlers.
//!
//! Mirrors the architecture of [`super::handlers`] (the risk-engine port): each
//! genesis handler in `meta/src/main.v` is a sentinel-returning DSL stub, and
//! the linker rewrites the stub in place to `CALL`/`JUMP` into the matching
//! body emitted here.
//!
//! # Source
//!
//! `hello_slab/percolator-meta/program/src/lib.rs` (read-only oracle; pinned
//! SHA recorded in commit messages). The genesis ledger is **all-u64** — no
//! u128/i128 fields — so unlike the risk-engine port these bodies use plain
//! `ADD`/`SUB`/`MUL`/`DIV` over u64 with no wide-math workarounds.
//!
//! # Calling convention (mono)
//!
//! Verified against `five-vm-mito`'s `ExecutionContext::parse_parameters`:
//!   * **Scalars** compact into `params[1..]` in scalar-declaration order
//!     (first scalar → `LOAD_PARAM_1`). `params[0]` holds the function index.
//!   * **Accounts** are resolved through the `accounts[]` slice (NOT param
//!     slots). In ROOT_CONTEXT `LOAD_FIELD(acct_idx)` reads `accounts[acct_idx]`
//!     directly. The handler picks the indices; callers pass accounts
//!     positionally in that order.
//!
//! This is the *current* convention. The older `super::handlers` bodies still
//! use the pre-mono indexing (accounts counted into param slots) and are
//! rebased separately.

use super::emit::Program;
use five_protocol::opcodes::{ADD, DIV, EQ, GT, LT, MUL, RETURN_VALUE, SHIFT_RIGHT, SUB};

/// A genesis handler body ready for the linker. Re-uses [`super::handlers::HandlerBody`]
/// so `link_perc5ive` / the linker treat both handler families identically.
pub use super::handlers::HandlerBody;

// =============================================================================
// Sentinels — must stay in sync with `meta/src/main.v`'s `sentinel_*()` consts.
//
// Values are 0xFEED_FACE_DEAD_6000 + tag, distinct from the risk-engine
// handlers' 0x..5000 range so the two never collide in a shared binary.
// =============================================================================

pub const SENTINEL_INIT_COIN_CONFIG: u64 = 0xFEED_FACE_DEAD_6003;
pub const SENTINEL_MINT_REWARD: u64 = 0xFEED_FACE_DEAD_6008;
pub const SENTINEL_TRANSFER_MINT_AUTHORITY: u64 = 0xFEED_FACE_DEAD_600A;
pub const SENTINEL_ACTIVATE_LIVE: u64 = 0xFEED_FACE_DEAD_600B;
pub const SENTINEL_INIT_PERCOLATOR_MARKET: u64 = 0xFEED_FACE_DEAD_6013;
pub const SENTINEL_PERCOLATOR_ADMIN: u64 = 0xFEED_FACE_DEAD_6014;
pub const SENTINEL_INIT_GENESIS_BOOTSTRAP: u64 = 0xFEED_FACE_DEAD_6015;
pub const SENTINEL_GENESIS_DEPOSIT: u64 = 0xFEED_FACE_DEAD_6016;
pub const SENTINEL_GENESIS_WITHDRAW: u64 = 0xFEED_FACE_DEAD_6017;
pub const SENTINEL_GENESIS_MINT_REWARD: u64 = 0xFEED_FACE_DEAD_6018;
pub const SENTINEL_FINALIZE_GENESIS: u64 = 0xFEED_FACE_DEAD_6019;
pub const SENTINEL_DRAW_GENESIS_SURPLUS: u64 = 0xFEED_FACE_DEAD_601A;
pub const SENTINEL_KICKSTART_GENESIS_MARKET: u64 = 0xFEED_FACE_DEAD_601B;
pub const SENTINEL_RECOVER_GENESIS_MARKET: u64 = 0xFEED_FACE_DEAD_601C;
pub const SENTINEL_INIT_GENESIS_DISTRIBUTION: u64 = 0xFEED_FACE_DEAD_601D;
pub const SENTINEL_VOTE_GENESIS_DISTRIBUTION: u64 = 0xFEED_FACE_DEAD_601E;
pub const SENTINEL_APPROVE_BUILDER: u64 = 0xFEED_FACE_DEAD_601F;
pub const SENTINEL_INIT_GENESIS_SQUADS: u64 = 0xFEED_FACE_DEAD_6020;
pub const SENTINEL_HANDOVER_GENESIS_SQUADS: u64 = 0xFEED_FACE_DEAD_6021;

// =============================================================================
// Account field offsets — mirror `meta/src/main.v` account declarations.
//
// 5ive-native packed layout (no alignment padding, no 8-byte discriminator —
// account identity is enforced by the VM's owner/PDA checks, not a disc tag).
// =============================================================================

pub mod genesis_cfg_offsets {
    //! `GenesisConfig` — bootstrap vote + principal ledger. Size 144.
    //! Lifecycle flags are **u64** (not u8): in a pool-enabled binary the only
    //! pool-free constant push is the 0..3 nibble, so a u8 flag couldn't be
    //! masked/tested without a pool constant. u64 flags read cleanly and store
    //! via a nibble push. See `meta_handlers` field-access notes.
    pub const COIN_MINT: u32 = 0; // pubkey 32
    pub const BASE_MINT: u32 = 32; // pubkey 32
    pub const TOKEN_VAULT: u32 = 64; // pubkey 32
    pub const TOTAL_DEPOSITED: u32 = 96; // u64
    pub const TOTAL_WITHDRAWN: u32 = 104; // u64
    pub const REWARD_SUPPLY: u32 = 112; // u64
    pub const MINTED_SUPPLY: u32 = 120; // u64
    pub const FINALIZED: u32 = 128; // u64 (0/1)
    pub const KICKED: u32 = 136; // u64 (0/1)
    pub const SIZE: usize = 144;
}

pub mod genesis_position_offsets {
    //! `GenesisPosition` — per-user base-unit deposit and voting weight. Size 64.
    pub const OWNER: u32 = 0; // pubkey 32
    pub const AMOUNT: u32 = 32; // u64
    pub const WITHDRAWN: u32 = 40; // u64
    pub const START_SLOT: u32 = 48; // u64 (last-write-time)
    pub const ACTIVE_VOTES: u32 = 56; // u64 — live ballots; must be 0 to exit while voting
    pub const SIZE: usize = 64;
}

pub mod genesis_distribution_offsets {
    //! `GenesisDistribution` — a vote-approved mint allocation item. Size 112.
    pub const GENESIS_CFG: u32 = 0; // pubkey 32
    pub const DESTINATION: u32 = 32; // pubkey 32
    pub const PROPOSAL_ID: u32 = 64; // u64
    pub const AMOUNT: u32 = 72; // u64
    pub const YES_VOTES: u32 = 80; // u64
    pub const NO_VOTES: u32 = 88; // u64
    pub const EXECUTED: u32 = 96; // u64 (0/1)
    pub const VOTED_PRINCIPAL: u32 = 104; // u64
    pub const SIZE: usize = 112;
}

pub mod genesis_vote_offsets {
    //! `GenesisDistributionVote` — one voter's weight on one item. Size 96.
    pub const PROPOSAL: u32 = 0; // pubkey 32
    pub const VOTER: u32 = 32; // pubkey 32
    pub const WEIGHT: u32 = 64; // u64
    pub const SUPPORT: u32 = 72; // u64 (0=no, 1=yes)
    pub const RETRACTED: u32 = 80; // u64 (1 once backed out of the tally)
    pub const PRINCIPAL: u32 = 88; // u64
    pub const SIZE: usize = 96;
}

pub mod coin_cfg_offsets {
    //! `CoinConfig` — shared across markets using the same COIN mint. Size 64.
    pub const AUTHORITY: u32 = 0; // pubkey 32
    pub const BOOTSTRAP_START_SLOT: u32 = 32; // u64
    pub const BOOTSTRAP_DELAY_SLOTS: u32 = 40; // u64
    pub const LIVE_SLOT: u32 = 48; // u64
    pub const PHASE: u32 = 56; // u64 (0=bootstrap, 1=live)
    pub const SIZE: usize = 64;
}

pub const PHASE_BOOTSTRAP: u64 = 0;
pub const PHASE_LIVE: u64 = 1;

// Status codes returned by the genesis handler bodies. 0 == success; nonzero
// maps to an upstream `ProgramError` reason so DSL/clients can branch on it.
pub const STATUS_OK: u64 = 0;
pub const STATUS_DEPOSITS_CLOSED: u64 = 1; // kicked/finalized: deposits closed
pub const STATUS_NOT_KICKED: u64 = 2; // mint/finalize before kickstart
pub const STATUS_ALREADY_FINALIZED: u64 = 3;
pub const STATUS_NO_MAJORITY: u64 = 4; // yes <= no
pub const STATUS_NO_QUORUM: u64 = 5; // voted_principal <= outstanding/2
pub const STATUS_SUPPLY_OVERFLOW: u64 = 6; // minted + amount > reward_supply
pub const STATUS_SUPPLY_INCOMPLETE: u64 = 7; // finalize: minted != reward_supply
pub const STATUS_INSUFFICIENT_SURPLUS: u64 = 8; // draw > vault - outstanding

// =============================================================================
// Account-table + field-offset conventions  (empirically verified — see
// tests/probe_account_index.rs, which runs against the real MitoVM)
// =============================================================================
//
// In ROOT_CONTEXT `MitoVM::execute_direct` indexes the `accounts: &[AccountInfo]`
// slice **directly** (no remap). Slot 0 is the script/VM-state account — its
// key binds every data account's header, and writes to it are forbidden. The
// declared accounts therefore start at slot 1, so account param `i` (0-based in
// declaration order) maps to account-table index `i + 1`.
//
// (Earlier this file assumed `+2`; the probe proved the runtime offset is `+1`,
// matching the risk-engine handlers' `RISK_ACCT = 1` convention.)
pub const VM_ACCOUNT_PARAM_OFFSET: u8 = 1;

/// Account-table index of the `i`-th declared account param.
pub const fn acct(decl_index: u8) -> u8 {
    decl_index + VM_ACCOUNT_PARAM_OFFSET
}

/// Length of the mandatory `5RAW` per-account header (`DSLRawAccountHeader`)
/// that every VM-owned data account carries. `LOAD_FIELD`/`STORE_FIELD` offsets
/// are absolute into the account buffer — the struct begins *after* this
/// header — so every logical field offset is biased by this constant. The
/// probe confirms a header-inclusive offset reads the payload and the header
/// stays intact across stores.
pub const DSL_RAW_HEADER: u32 = 40;

/// Buffer-absolute offset of a logical struct field (header + logical offset).
#[inline]
pub const fn fld(logical_offset: u32) -> u32 {
    logical_offset + DSL_RAW_HEADER
}

/// The 12 genesis-lifecycle sentinels whose bodies the linker rewrites (Phase
/// 2). Order is irrelevant; used by the structural link test + `link_meta`.
pub const GENESIS_SENTINELS: [u64; 12] = [
    SENTINEL_INIT_COIN_CONFIG,
    SENTINEL_ACTIVATE_LIVE,
    SENTINEL_INIT_GENESIS_BOOTSTRAP,
    SENTINEL_GENESIS_DEPOSIT,
    SENTINEL_GENESIS_WITHDRAW,
    SENTINEL_KICKSTART_GENESIS_MARKET,
    SENTINEL_INIT_GENESIS_DISTRIBUTION,
    SENTINEL_VOTE_GENESIS_DISTRIBUTION,
    SENTINEL_GENESIS_MINT_REWARD,
    SENTINEL_FINALIZE_GENESIS,
    SENTINEL_DRAW_GENESIS_SURPLUS,
    SENTINEL_RECOVER_GENESIS_MARKET,
];

/// The 5 Phase-3-deferred (governance / market-factory) sentinels. Present in
/// the compiled binary but NOT rewritten until post-validation work lands.
pub const DEFERRED_SENTINELS: [u64; 5] = [
    SENTINEL_MINT_REWARD,
    SENTINEL_TRANSFER_MINT_AUTHORITY,
    SENTINEL_INIT_PERCOLATOR_MARKET,
    SENTINEL_PERCOLATOR_ADMIN,
    SENTINEL_APPROVE_BUILDER,
];

// Additional status codes used by the genesis lifecycle bodies.
pub const STATUS_NO_VOTE_WEIGHT: u64 = 9; // vote: position has no weight (age<2 / exited / unstaked)
pub const STATUS_NOT_FINALIZED: u64 = 10; // draw_surplus before finalize
pub const STATUS_ALREADY_KICKED: u64 = 11; // kickstart twice / after finalize
pub const STATUS_EMPTY_DEPOSIT: u64 = 12; // kickstart with total_deposited == 0
pub const STATUS_TOO_EARLY: u64 = 13; // activate_live before the bootstrap delay elapsed
pub const STATUS_BAD_ACTION: u64 = 14; // vote action not in {0,1} (retract/re-vote deferred — see body)

// =============================================================================
// Field-access helpers — all genesis fields are u64/u8 within a `5RAW`-headered
// account, so every offset is biased by `DSL_RAW_HEADER` (see `fld`).
// =============================================================================

/// `LOAD_FIELD acct, fld(logical)` — header-inclusive field read.
#[inline]
fn lf(p: &mut Program, acct_idx: u8, logical: u32) {
    p.emit_load_field(acct_idx, fld(logical));
}

/// `STORE_FIELD acct, fld(logical)` — header-inclusive field write. The popped
/// value's width decides byte count (push a u8 for 1-byte flag fields, a u64
/// for the 8-byte counters), so callers must push the correctly-typed value.
#[inline]
fn sf(p: &mut Program, acct_idx: u8, logical: u32) {
    p.emit_store_field(acct_idx, fld(logical));
}

/// Load a u64 flag/counter field and force the lazy `AccountRef` to a concrete
/// `U64` value via `+ 0` (pool-free nibble add), so `JUMP_IF` / `JUMP_IF_NOT`
/// branch on the field's value rather than the ref. (A bare `LOAD_FIELD` leaves
/// an `AccountRef`, whose truthiness is the ref, not the contents.)
#[inline]
fn flag_val(p: &mut Program, acct_idx: u8, logical: u32) {
    lf(p, acct_idx, logical);
    p.emit_push_nibble(0);
    p.raw(ADD);
}

// =============================================================================
// Genesis-lifecycle handler bodies
//
// Each body runs in its DSL stub's frame (the linker rewrites the pool-backed
// stub to `JUMP <body>`, so params + the account table carry through). Account
// param `i` → table index `acct(i)`; first scalar → LOAD_PARAM 1. Bodies end
// with a u64 status on the stack + RETURN_VALUE (0 == success). SPL-token and
// Percolator CPI are the on-chain INVOKE layer and are NOT modelled here — the
// bodies implement the u64 *ledger* state transitions, which are what the
// genesis lifecycle e2e exercises. Side-effectful reads (token balances, pulled
// amounts) arrive as explicit scalar params, the standard perc5ive porting
// convention.
//
// Source of truth: hello_slab/percolator-meta/program/src/lib.rs @ b6d5f2a.
// =============================================================================

/// `init_coin_config(coin_cfg, authority, bootstrap_delay_slots, start_slot)`.
/// Source: process_init_coin_config (lib.rs:1097). Zero delay → live now;
/// nonzero → bootstrap, awaiting a later activate_live. The authority-pubkey
/// binding + mint authority/freeze checks are the on-chain identity/CPI layer.
pub fn handler_body_init_coin_config() -> HandlerBody {
    use coin_cfg_offsets as o;
    const CFG: u8 = acct(0);
    let mut p = Program::new();

    // bootstrap_start_slot = start_slot (param 2); bootstrap_delay_slots = delay (param 1).
    p.emit_load_param(2);
    sf(&mut p, CFG, o::BOOTSTRAP_START_SLOT);
    p.emit_load_param(1);
    sf(&mut p, CFG, o::BOOTSTRAP_DELAY_SLOTS);

    // if delay == 0 → phase=LIVE, live_slot=start_slot; else phase=BOOTSTRAP, live_slot=0.
    p.emit_load_param(1);
    p.emit_push_nibble(0);
    p.raw(EQ);
    let delay_nonzero = p.emit_jump_if_not_placeholder_body_relative();
    // delay == 0 branch
    p.emit_push_nibble(PHASE_LIVE);
    sf(&mut p, CFG, o::PHASE);
    p.emit_load_param(2);
    sf(&mut p, CFG, o::LIVE_SLOT);
    let to_end = p.emit_jump_placeholder_body_relative();
    // delay != 0 branch
    p.patch_jump_to_here_body_relative(delay_nonzero);
    p.emit_push_nibble(PHASE_BOOTSTRAP);
    sf(&mut p, CFG, o::PHASE);
    p.emit_push_nibble(0);
    sf(&mut p, CFG, o::LIVE_SLOT);
    p.patch_jump_to_here_body_relative(to_end);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &[delay_nonzero, to_end])
}

/// `activate_live(coin_cfg, authority, now_slot)`.
/// Source: process_activate_live (lib.rs:1184). Requires the bootstrap delay to
/// have elapsed: `now_slot >= bootstrap_start_slot + bootstrap_delay_slots`.
pub fn handler_body_activate_live() -> HandlerBody {
    use coin_cfg_offsets as o;
    const CFG: u8 = acct(0);
    let mut p = Program::new();

    // too_early: now_slot < bootstrap_start_slot + bootstrap_delay_slots.
    p.emit_load_param(1);
    lf(&mut p, CFG, o::BOOTSTRAP_START_SLOT);
    lf(&mut p, CFG, o::BOOTSTRAP_DELAY_SLOTS);
    p.raw(ADD);
    p.raw(LT); // (now_slot < start+delay)
    let elapsed = p.emit_jump_if_not_placeholder_body_relative();
    p.emit_push_uint(STATUS_TOO_EARLY);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(elapsed);

    p.emit_push_nibble(PHASE_LIVE);
    sf(&mut p, CFG, o::PHASE);
    p.emit_load_param(1);
    sf(&mut p, CFG, o::LIVE_SLOT);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &[elapsed])
}

/// `init_genesis_bootstrap(genesis_cfg, authority, reward_supply)`.
/// Source: process_init_genesis_bootstrap (lib.rs:1531). Records the fixed COIN
/// reward supply; zeroes the principal/mint counters and lifecycle flags. PDA +
/// vault creation is the on-chain INVOKE layer.
pub fn handler_body_init_genesis_bootstrap() -> HandlerBody {
    use genesis_cfg_offsets as o;
    const CFG: u8 = acct(0);
    let mut p = Program::new();

    p.emit_load_param(1);
    sf(&mut p, CFG, o::REWARD_SUPPLY);
    p.emit_push_nibble(0);
    sf(&mut p, CFG, o::TOTAL_DEPOSITED);
    p.emit_push_nibble(0);
    sf(&mut p, CFG, o::TOTAL_WITHDRAWN);
    p.emit_push_nibble(0);
    sf(&mut p, CFG, o::MINTED_SUPPLY);
    p.emit_push_nibble(0);
    sf(&mut p, CFG, o::FINALIZED);
    p.emit_push_nibble(0);
    sf(&mut p, CFG, o::KICKED);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::linear(p.into_body())
}

/// `genesis_deposit(genesis_cfg, position, user, amount, now_slot)`.
/// Source: process_genesis_deposit (lib.rs:1668). One vote unit per base unit
/// (the staked principal itself); (re)sets the position's last-write-time so
/// vote weight measures hold duration. Closes once kicked or finalized. The SPL
/// transfer of `amount` base units into the vault is the on-chain INVOKE layer.
pub fn handler_body_genesis_deposit() -> HandlerBody {
    use genesis_cfg_offsets as c;
    use genesis_position_offsets as q;
    const CFG: u8 = acct(0);
    const POS: u8 = acct(1);
    let mut p = Program::new();

    // Deposits close once pooled capital is deployed (kicked) or genesis ends
    // (finalized). u64 flags read cleanly: coerce each to a value and branch.
    let mut closed_patches = vec![];
    flag_val(&mut p, CFG, c::FINALIZED);
    let f_closed = p.emit_jump_if_placeholder_body_relative();
    closed_patches.push(f_closed);
    flag_val(&mut p, CFG, c::KICKED);
    let k_closed = p.emit_jump_if_placeholder_body_relative();
    closed_patches.push(k_closed);

    // total_deposited += amount; position.amount += amount; position.start_slot = now_slot.
    lf(&mut p, CFG, c::TOTAL_DEPOSITED);
    p.emit_load_param(1);
    p.raw(ADD);
    sf(&mut p, CFG, c::TOTAL_DEPOSITED);
    lf(&mut p, POS, q::AMOUNT);
    p.emit_load_param(1);
    p.raw(ADD);
    sf(&mut p, POS, q::AMOUNT);
    p.emit_load_param(2);
    sf(&mut p, POS, q::START_SLOT);
    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);

    // closed: both flag branches land here.
    p.patch_jump_to_here_body_relative(f_closed);
    p.patch_jump_to_here_body_relative(k_closed);
    p.emit_push_uint(STATUS_DEPOSITS_CLOSED);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &closed_patches)
}

/// `genesis_withdraw(genesis_cfg, position, user, coin_phase, recovered, vault_balance)`.
/// Source: process_genesis_withdraw (lib.rs:1863). Three ledger paths keyed off
/// the cfg flags; the vote is forfeited (`start_slot = 0`) on every exit:
///   * **not kicked** — capital still in the genesis vault: credit
///     `actual = min(remaining, vault_balance)`, debit `total_deposited` and
///     `position.amount`.
///   * **finalized** — futarchy refunded the vault: retire the *full* remaining
///     principal against `total_withdrawn` / `position.withdrawn` (pro-rata
///     payout is the token-layer concern; the ledger claim is fully settled).
///   * **kicked, not finalized** — pull `recovered` (caller resolves the
///     market CPI) capped at remaining; credit `total_withdrawn` /
///     `position.withdrawn`.
///
/// `recovered` / `vault_balance` arrive as scalars (the token-balance reads and
/// market-pull CPI are the on-chain layer). The active-votes lock and SPL
/// transfers are enforced on-chain; here the depositor is assumed cleared.
pub fn handler_body_genesis_withdraw() -> HandlerBody {
    use genesis_cfg_offsets as c;
    use genesis_position_offsets as q;
    const CFG: u8 = acct(0);
    const POS: u8 = acct(1);
    let mut p = Program::new();
    let mut patches = vec![];
    p.emit_alloc_locals(2); // slot0 = remaining, slot1 = actual

    // remaining = position.amount - position.withdrawn
    lf(&mut p, POS, q::AMOUNT);
    lf(&mut p, POS, q::WITHDRAWN);
    p.raw(SUB);
    p.emit_set_local(0);

    // finalized? → path B
    lf(&mut p, CFG, c::FINALIZED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let to_path_b = p.emit_jump_if_placeholder_body_relative();
    patches.push(to_path_b);
    // kicked? → path C
    lf(&mut p, CFG, c::KICKED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let to_path_c = p.emit_jump_if_placeholder_body_relative();
    patches.push(to_path_c);

    // --- PATH A (not kicked): actual = min(remaining, vault_balance) ---
    p.emit_get_local(0);
    p.emit_set_local(1); // actual = remaining
    p.emit_load_param(3); // vault_balance
    p.emit_get_local(0);
    p.raw(LT); // vault_balance < remaining ?
    let a_keep = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(a_keep);
    p.emit_load_param(3);
    p.emit_set_local(1); // actual = vault_balance
    p.patch_jump_to_here_body_relative(a_keep);
    lf(&mut p, CFG, c::TOTAL_DEPOSITED);
    p.emit_get_local(1);
    p.raw(SUB);
    sf(&mut p, CFG, c::TOTAL_DEPOSITED);
    lf(&mut p, POS, q::AMOUNT);
    p.emit_get_local(1);
    p.raw(SUB);
    sf(&mut p, POS, q::AMOUNT);
    let a_to_forfeit = p.emit_jump_placeholder_body_relative();
    patches.push(a_to_forfeit);

    // --- PATH C (kicked, not finalized): actual = min(recovered, remaining) ---
    p.patch_jump_to_here_body_relative(to_path_c);
    p.emit_get_local(0);
    p.emit_set_local(1); // actual = remaining
    p.emit_load_param(2); // recovered
    p.emit_get_local(0);
    p.raw(LT); // recovered < remaining ?
    let c_keep = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(c_keep);
    p.emit_load_param(2);
    p.emit_set_local(1); // actual = recovered
    p.patch_jump_to_here_body_relative(c_keep);
    lf(&mut p, CFG, c::TOTAL_WITHDRAWN);
    p.emit_get_local(1);
    p.raw(ADD);
    sf(&mut p, CFG, c::TOTAL_WITHDRAWN);
    lf(&mut p, POS, q::WITHDRAWN);
    p.emit_get_local(1);
    p.raw(ADD);
    sf(&mut p, POS, q::WITHDRAWN);
    let c_to_forfeit = p.emit_jump_placeholder_body_relative();
    patches.push(c_to_forfeit);

    // --- PATH B (finalized): retire full remaining ---
    p.patch_jump_to_here_body_relative(to_path_b);
    lf(&mut p, CFG, c::TOTAL_WITHDRAWN);
    p.emit_get_local(0);
    p.raw(ADD);
    sf(&mut p, CFG, c::TOTAL_WITHDRAWN);
    lf(&mut p, POS, q::WITHDRAWN);
    p.emit_get_local(0);
    p.raw(ADD);
    sf(&mut p, POS, q::WITHDRAWN);

    // --- forfeit + return (all paths) ---
    p.patch_jump_to_here_body_relative(a_to_forfeit);
    p.patch_jump_to_here_body_relative(c_to_forfeit);
    p.emit_push_nibble(0);
    sf(&mut p, POS, q::START_SLOT);
    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &patches)
}

/// `kickstart_genesis_market(genesis_cfg, authority, backing_domain, backing_expiry_slot)`.
/// Source: process_kickstart_genesis_market (lib.rs:2664). The 50/50 split
/// (`insurance = total/2`, `backing = total - insurance`) and the
/// top-up-insurance / top-up-backing CPIs are the on-chain layer (modelled by
/// `meta_math::kickstart_split` for the bench/MCP); the ledger effect here is
/// the `kicked` flag, gated on a non-empty, not-yet-deployed pool.
pub fn handler_body_kickstart_genesis_market() -> HandlerBody {
    use genesis_cfg_offsets as c;
    const CFG: u8 = acct(0);
    let mut p = Program::new();
    let mut patches = vec![];

    // already kicked or finalized → reject (test the two u64 flags separately).
    flag_val(&mut p, CFG, c::FINALIZED);
    let f_already = p.emit_jump_if_placeholder_body_relative();
    patches.push(f_already);
    flag_val(&mut p, CFG, c::KICKED);
    let k_already = p.emit_jump_if_placeholder_body_relative();
    patches.push(k_already);

    // total_deposited == 0 → nothing to deploy.
    lf(&mut p, CFG, c::TOTAL_DEPOSITED);
    p.emit_push_nibble(0);
    p.raw(EQ);
    let nonempty = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(nonempty);
    p.emit_push_uint(STATUS_EMPTY_DEPOSIT);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(nonempty);

    p.emit_push_nibble(1);
    sf(&mut p, CFG, c::KICKED);
    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);

    // already-deployed: both flag branches land here.
    p.patch_jump_to_here_body_relative(f_already);
    p.patch_jump_to_here_body_relative(k_already);
    p.emit_push_uint(STATUS_ALREADY_KICKED);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &patches)
}

/// `init_genesis_distribution(genesis_cfg, distribution, proposer, proposal_id, amount)`.
/// Source: process_init_genesis_distribution (lib.rs:2152). Creates an
/// allocation item with zeroed tallies. The destination-mint check + PDA
/// creation are on-chain; the `amount <= reward_supply` guard is ported here.
pub fn handler_body_init_genesis_distribution() -> HandlerBody {
    use genesis_cfg_offsets as c;
    use genesis_distribution_offsets as d;
    const CFG: u8 = acct(0);
    const DIST: u8 = acct(1);
    let mut p = Program::new();

    // amount > reward_supply → reject.
    p.emit_load_param(2);
    lf(&mut p, CFG, c::REWARD_SUPPLY);
    p.raw(GT);
    let amount_ok = p.emit_jump_if_not_placeholder_body_relative();
    p.emit_push_uint(STATUS_SUPPLY_OVERFLOW);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(amount_ok);

    p.emit_load_param(1);
    sf(&mut p, DIST, d::PROPOSAL_ID);
    p.emit_load_param(2);
    sf(&mut p, DIST, d::AMOUNT);
    p.emit_push_nibble(0);
    sf(&mut p, DIST, d::YES_VOTES);
    p.emit_push_nibble(0);
    sf(&mut p, DIST, d::NO_VOTES);
    p.emit_push_nibble(0);
    sf(&mut p, DIST, d::EXECUTED);
    p.emit_push_nibble(0);
    sf(&mut p, DIST, d::VOTED_PRINCIPAL);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &[amount_ok])
}

/// `vote_genesis_distribution(position, distribution, vote, voter, action, now_slot)`.
/// Source: process_vote_genesis_distribution (lib.rs:2230). Casts a fresh vote:
/// weight = `floor(log2(now_slot - start_slot)) * staked` (the time-weighted
/// `genesis_vote_weight`, inlined as a log2 loop), folded into yes/no by
/// `action`; the raw staked principal joins the quorum tally and `active_votes`
/// is incremented; the vote record stores weight/support/principal.
///
/// **Port scope (documented like the risk-engine deferred items):** the
/// re-vote backout and `action == 2` retract paths are not yet emitted — a
/// repeat or retract returns `STATUS_BAD_ACTION` rather than mis-tallying.
/// The genesis lifecycle (one fresh ballot per voter/proposal) is fully
/// covered; backout/retract is queued.
pub fn handler_body_vote_genesis_distribution() -> HandlerBody {
    use genesis_distribution_offsets as d;
    use genesis_position_offsets as q;
    use genesis_vote_offsets as v;
    const POS: u8 = acct(0);
    const DIST: u8 = acct(1);
    const VOTE: u8 = acct(2);
    let mut p = Program::new();
    let mut patches = vec![];
    // locals: 0 = age_work, 1 = k (log2 count), 2 = staked, 3 = weight
    p.emit_alloc_locals(4);

    // reject action not in {0,1} (retract / re-vote deferred).
    p.emit_load_param(1);
    p.emit_push_nibble(1);
    p.raw(GT); // action > 1 ?
    let action_ok = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(action_ok);
    p.emit_push_uint(STATUS_BAD_ACTION);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(action_ok);

    // staked = position.amount - position.withdrawn
    lf(&mut p, POS, q::AMOUNT);
    lf(&mut p, POS, q::WITHDRAWN);
    p.raw(SUB);
    p.emit_set_local(2);
    // age_work = now_slot - position.start_slot
    p.emit_load_param(2);
    lf(&mut p, POS, q::START_SLOT);
    p.raw(SUB);
    p.emit_set_local(0);
    // k = 0; weight = 0 (set in slot 3 at the end / zero paths)
    p.emit_push_nibble(0);
    p.emit_set_local(1);

    // zero-weight guards: staked == 0 OR age < 2 → weight stays 0.
    p.emit_get_local(2);
    p.emit_push_nibble(0);
    p.raw(EQ);
    let zero_a = p.emit_jump_if_placeholder_body_relative();
    patches.push(zero_a);
    p.emit_get_local(0);
    p.emit_push_nibble(2);
    p.raw(LT);
    let zero_b = p.emit_jump_if_placeholder_body_relative();
    patches.push(zero_b);

    // loop: while age_work >= 2 { age_work >>= 1; k += 1 }
    let loop_start = p.body_len() as u16;
    p.emit_get_local(0);
    p.emit_push_nibble(2);
    p.raw(LT); // age_work < 2 → break
    let to_break = p.emit_jump_if_placeholder_body_relative();
    patches.push(to_break);
    p.emit_get_local(0);
    p.emit_push_nibble(1);
    p.raw(SHIFT_RIGHT);
    p.emit_set_local(0);
    p.emit_get_local(1);
    p.emit_push_nibble(1);
    p.raw(ADD);
    p.emit_set_local(1);
    let back = p.emit_jump_backward_body_relative(loop_start);
    patches.push(back);

    // break: weight = k * staked
    p.patch_jump_to_here_body_relative(to_break);
    p.emit_get_local(1);
    p.emit_get_local(2);
    p.raw(MUL);
    p.emit_set_local(3);
    let to_weight_check = p.emit_jump_placeholder_body_relative();
    patches.push(to_weight_check);

    // zero-weight paths converge here: weight = 0.
    p.patch_jump_to_here_body_relative(zero_a);
    p.patch_jump_to_here_body_relative(zero_b);
    p.emit_push_nibble(0);
    p.emit_set_local(3);

    // weight == 0 → no vote weight.
    p.patch_jump_to_here_body_relative(to_weight_check);
    p.emit_get_local(3);
    p.emit_push_nibble(0);
    p.raw(EQ);
    let has_weight = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(has_weight);
    p.emit_push_uint(STATUS_NO_VOTE_WEIGHT);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(has_weight);

    // tally: action == 1 → yes_votes += weight (and support=1); else no_votes (support=0).
    p.emit_load_param(1);
    p.emit_push_nibble(1);
    p.raw(EQ);
    let is_no = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(is_no);
    // yes branch
    lf(&mut p, DIST, d::YES_VOTES);
    p.emit_get_local(3);
    p.raw(ADD);
    sf(&mut p, DIST, d::YES_VOTES);
    p.emit_push_nibble(1);
    sf(&mut p, VOTE, v::SUPPORT);
    let after_tally = p.emit_jump_placeholder_body_relative();
    patches.push(after_tally);
    // no branch
    p.patch_jump_to_here_body_relative(is_no);
    lf(&mut p, DIST, d::NO_VOTES);
    p.emit_get_local(3);
    p.raw(ADD);
    sf(&mut p, DIST, d::NO_VOTES);
    p.emit_push_nibble(0);
    sf(&mut p, VOTE, v::SUPPORT);
    p.patch_jump_to_here_body_relative(after_tally);

    // voted_principal += staked
    lf(&mut p, DIST, d::VOTED_PRINCIPAL);
    p.emit_get_local(2);
    p.raw(ADD);
    sf(&mut p, DIST, d::VOTED_PRINCIPAL);
    // active_votes += 1 (fresh ballot)
    lf(&mut p, POS, q::ACTIVE_VOTES);
    p.emit_push_nibble(1);
    p.raw(ADD);
    sf(&mut p, POS, q::ACTIVE_VOTES);

    // vote record: weight, retracted=0, principal=staked.
    p.emit_get_local(3);
    sf(&mut p, VOTE, v::WEIGHT);
    p.emit_push_nibble(0);
    sf(&mut p, VOTE, v::RETRACTED);
    p.emit_get_local(2);
    sf(&mut p, VOTE, v::PRINCIPAL);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &patches)
}

/// `genesis_mint_reward(genesis_cfg, distribution, authority, amount)`.
/// Source: process_genesis_mint_reward (lib.rs:2440). Mints a majority-approved,
/// quorum-cleared item: requires a kicked, unfinalized genesis, weighted
/// majority (`yes > no`), principal quorum (`voted_principal > outstanding/2`),
/// and that `minted_supply + amount <= reward_supply`. The COIN mint CPI is the
/// on-chain layer; the ledger effect is `minted_supply += amount` + `executed`.
pub fn handler_body_genesis_mint_reward() -> HandlerBody {
    use genesis_cfg_offsets as c;
    use genesis_distribution_offsets as d;
    const CFG: u8 = acct(0);
    const DIST: u8 = acct(1);
    let mut p = Program::new();
    let mut patches = vec![];

    // must be kicked.
    lf(&mut p, CFG, c::KICKED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let kicked_ok = p.emit_jump_if_placeholder_body_relative();
    patches.push(kicked_ok);
    p.emit_push_uint(STATUS_NOT_KICKED);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(kicked_ok);

    // must not be finalized.
    lf(&mut p, CFG, c::FINALIZED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let not_final = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(not_final);
    p.emit_push_uint(STATUS_ALREADY_FINALIZED);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(not_final);

    // weighted majority: yes_votes > no_votes.
    lf(&mut p, DIST, d::YES_VOTES);
    lf(&mut p, DIST, d::NO_VOTES);
    p.raw(GT);
    let majority = p.emit_jump_if_placeholder_body_relative();
    patches.push(majority);
    p.emit_push_uint(STATUS_NO_MAJORITY);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(majority);

    // principal quorum: voted_principal > (total_deposited - total_withdrawn)/2.
    lf(&mut p, DIST, d::VOTED_PRINCIPAL);
    lf(&mut p, CFG, c::TOTAL_DEPOSITED);
    lf(&mut p, CFG, c::TOTAL_WITHDRAWN);
    p.raw(SUB);
    p.emit_push_nibble(2);
    p.raw(DIV);
    p.raw(GT); // voted_principal > outstanding/2
    let quorum = p.emit_jump_if_placeholder_body_relative();
    patches.push(quorum);
    p.emit_push_uint(STATUS_NO_QUORUM);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(quorum);

    // supply cap: minted_supply + amount <= reward_supply.
    lf(&mut p, CFG, c::MINTED_SUPPLY);
    p.emit_load_param(1);
    p.raw(ADD);
    lf(&mut p, CFG, c::REWARD_SUPPLY);
    p.raw(GT); // (minted+amount) > reward_supply
    let within_supply = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(within_supply);
    p.emit_push_uint(STATUS_SUPPLY_OVERFLOW);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(within_supply);

    // commit: minted_supply += amount; executed = 1.
    lf(&mut p, CFG, c::MINTED_SUPPLY);
    p.emit_load_param(1);
    p.raw(ADD);
    sf(&mut p, CFG, c::MINTED_SUPPLY);
    p.emit_push_nibble(1);
    sf(&mut p, DIST, d::EXECUTED);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &patches)
}

/// `finalize_genesis(genesis_cfg, authority)`.
/// Source: process_finalize_genesis (lib.rs:2539). Requires a kicked market and
/// full reward-supply distribution (`minted_supply == reward_supply`).
pub fn handler_body_finalize_genesis() -> HandlerBody {
    use genesis_cfg_offsets as c;
    const CFG: u8 = acct(0);
    let mut p = Program::new();
    let mut patches = vec![];

    // must be kicked.
    lf(&mut p, CFG, c::KICKED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let kicked_ok = p.emit_jump_if_placeholder_body_relative();
    patches.push(kicked_ok);
    p.emit_push_uint(STATUS_NOT_KICKED);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(kicked_ok);

    // minted_supply == reward_supply.
    lf(&mut p, CFG, c::MINTED_SUPPLY);
    lf(&mut p, CFG, c::REWARD_SUPPLY);
    p.raw(EQ);
    let complete = p.emit_jump_if_placeholder_body_relative();
    patches.push(complete);
    p.emit_push_uint(STATUS_SUPPLY_INCOMPLETE);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(complete);

    p.emit_push_nibble(1);
    sf(&mut p, CFG, c::FINALIZED);
    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &patches)
}

/// `draw_genesis_surplus(genesis_cfg, authority, amount, vault_balance)`.
/// Source: process_draw_genesis_surplus (lib.rs:2583). Requires finalization
/// and `amount <= vault_balance - outstanding_principal`. The token transfer is
/// the on-chain layer; this body only enforces the surplus bound.
pub fn handler_body_draw_genesis_surplus() -> HandlerBody {
    use genesis_cfg_offsets as c;
    const CFG: u8 = acct(0);
    let mut p = Program::new();
    let mut patches = vec![];

    // must be finalized.
    lf(&mut p, CFG, c::FINALIZED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let finalized_ok = p.emit_jump_if_placeholder_body_relative();
    patches.push(finalized_ok);
    p.emit_push_uint(STATUS_NOT_FINALIZED);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(finalized_ok);

    // amount > vault_balance - (total_deposited - total_withdrawn) → insufficient.
    p.emit_load_param(1); // amount
    p.emit_load_param(2); // vault_balance
    lf(&mut p, CFG, c::TOTAL_DEPOSITED);
    lf(&mut p, CFG, c::TOTAL_WITHDRAWN);
    p.raw(SUB); // outstanding
    p.raw(SUB); // available = vault_balance - outstanding
    p.raw(GT); // amount > available
    let surplus_ok = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(surplus_ok);
    p.emit_push_uint(STATUS_INSUFFICIENT_SURPLUS);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(surplus_ok);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &patches)
}

/// `recover_genesis_market(genesis_cfg, authority, kind, domain, amount)`.
/// Source: process_recover_genesis_market (lib.rs:3049). Recovers bootstrap
/// market funds to the genesis vault (a Percolator CPI) — allowed only on a
/// kicked, unfinalized genesis. Pure custody move: no genesis-ledger mutation.
pub fn handler_body_recover_genesis_market() -> HandlerBody {
    use genesis_cfg_offsets as c;
    const CFG: u8 = acct(0);
    let mut p = Program::new();
    let mut patches = vec![];

    // must be kicked.
    lf(&mut p, CFG, c::KICKED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let kicked_ok = p.emit_jump_if_placeholder_body_relative();
    patches.push(kicked_ok);
    p.emit_push_uint(STATUS_NOT_KICKED);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(kicked_ok);

    // must not be finalized.
    lf(&mut p, CFG, c::FINALIZED);
    p.emit_push_nibble(0);
    p.raw(ADD);
    let not_final = p.emit_jump_if_not_placeholder_body_relative();
    patches.push(not_final);
    p.emit_push_uint(STATUS_ALREADY_FINALIZED);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(not_final);

    p.emit_push_uint(STATUS_OK);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &patches)
}

// =============================================================================
// Linker convenience
// =============================================================================

/// (sentinel, HandlerBody) for every Phase-2 genesis-lifecycle handler. The
/// linker rewrites each matching sentinel in the compiled `meta.fbin` to a jump
/// into the appended body. Order is irrelevant; the 5 Phase-3 governance /
/// market-factory sentinels stay stubbed (see `DEFERRED_SENTINELS`).
/// Link a compiled `meta.fbin` into a VM-native binary with every genesis
/// sentinel rewritten to its hand-written body. Normalizes the DSL header,
/// appends each body (fixing body-relative jumps), and rewrites the matching
/// stub in place. The 5 deferred governance sentinels are left untouched.
/// Mirrors `link_perc5ive` for the risk-engine port; used by the genesis e2e
/// and the devnet deploy step.
pub fn link_meta(fbin: &[u8]) -> Result<Vec<u8>, super::link::StubFindError> {
    use super::link::Linker;
    let base = super::dsl_header::normalize_dsl_header(fbin)
        .expect("normalize_dsl_header accepts meta.fbin");
    let mut linker = Linker::from_base(&base);
    for (sentinel, body) in all_meta_handler_bodies() {
        let callee =
            linker.append_function_with_body_relative_jumps(&body.bytes, &body.jump_patches);
        linker.rewrite_stub(sentinel, callee)?;
    }
    Ok(linker.into_bytes())
}

pub fn all_meta_handler_bodies() -> [(u64, HandlerBody); 12] {
    [
        (SENTINEL_INIT_COIN_CONFIG, handler_body_init_coin_config()),
        (SENTINEL_ACTIVATE_LIVE, handler_body_activate_live()),
        (SENTINEL_INIT_GENESIS_BOOTSTRAP, handler_body_init_genesis_bootstrap()),
        (SENTINEL_GENESIS_DEPOSIT, handler_body_genesis_deposit()),
        (SENTINEL_GENESIS_WITHDRAW, handler_body_genesis_withdraw()),
        (SENTINEL_KICKSTART_GENESIS_MARKET, handler_body_kickstart_genesis_market()),
        (SENTINEL_INIT_GENESIS_DISTRIBUTION, handler_body_init_genesis_distribution()),
        (SENTINEL_VOTE_GENESIS_DISTRIBUTION, handler_body_vote_genesis_distribution()),
        (SENTINEL_GENESIS_MINT_REWARD, handler_body_genesis_mint_reward()),
        (SENTINEL_FINALIZE_GENESIS, handler_body_finalize_genesis()),
        (SENTINEL_DRAW_GENESIS_SURPLUS, handler_body_draw_genesis_surplus()),
        (SENTINEL_RECOVER_GENESIS_MARKET, handler_body_recover_genesis_market()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_genesis_body_ends_with_return_value() {
        for (sentinel, body) in all_meta_handler_bodies() {
            assert!(!body.bytes.is_empty(), "empty body for {sentinel:#x}");
            assert_eq!(
                *body.bytes.last().unwrap(),
                RETURN_VALUE,
                "body for {sentinel:#x} must end with RETURN_VALUE",
            );
        }
    }

    #[test]
    fn body_relative_jump_patches_stay_within_bounds() {
        for (sentinel, body) in all_meta_handler_bodies() {
            for &patch in &body.jump_patches {
                assert!(
                    patch + 2 <= body.bytes.len(),
                    "jump patch {patch} overruns body len {} ({sentinel:#x})",
                    body.bytes.len(),
                );
            }
        }
    }

    #[test]
    fn all_twelve_genesis_sentinels_are_covered() {
        let covered: alloc::collections::BTreeSet<u64> =
            all_meta_handler_bodies().iter().map(|(s, _)| *s).collect();
        for s in GENESIS_SENTINELS {
            assert!(covered.contains(&s), "sentinel {s:#x} has no body");
        }
    }
}

#[cfg(test)]
extern crate alloc;
