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

// Phase 2 emitters use these; suppress unused warnings until the bodies land.
#[allow(unused_imports)]
use super::emit::Program;
#[allow(unused_imports)]
use super::meta_math;
#[allow(unused_imports)]
use five_protocol::opcodes::{ADD, EQ, GT, LT, MUL, RETURN_VALUE, SUB};

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
    //! `GenesisConfig` — bootstrap vote + principal ledger. Size 130.
    pub const COIN_MINT: u32 = 0; // pubkey 32
    pub const BASE_MINT: u32 = 32; // pubkey 32
    pub const TOKEN_VAULT: u32 = 64; // pubkey 32
    pub const TOTAL_DEPOSITED: u32 = 96; // u64
    pub const TOTAL_WITHDRAWN: u32 = 104; // u64
    pub const REWARD_SUPPLY: u32 = 112; // u64
    pub const MINTED_SUPPLY: u32 = 120; // u64
    pub const FINALIZED: u32 = 128; // u8
    pub const KICKED: u32 = 129; // u8
    pub const SIZE: usize = 130;
}

pub mod genesis_position_offsets {
    //! `GenesisPosition` — per-user base-unit deposit and voting weight. Size 56.
    pub const OWNER: u32 = 0; // pubkey 32
    pub const AMOUNT: u32 = 32; // u64
    pub const WITHDRAWN: u32 = 40; // u64
    pub const START_SLOT: u32 = 48; // u64 (last-write-time)
    pub const SIZE: usize = 56;
}

pub mod genesis_distribution_offsets {
    //! `GenesisDistribution` — a vote-approved mint allocation item. Size 105.
    pub const GENESIS_CFG: u32 = 0; // pubkey 32
    pub const DESTINATION: u32 = 32; // pubkey 32
    pub const PROPOSAL_ID: u32 = 64; // u64
    pub const AMOUNT: u32 = 72; // u64
    pub const YES_VOTES: u32 = 80; // u64
    pub const NO_VOTES: u32 = 88; // u64
    pub const EXECUTED: u32 = 96; // u8
    pub const VOTED_PRINCIPAL: u32 = 97; // u64
    pub const SIZE: usize = 105;
}

pub mod genesis_vote_offsets {
    //! `GenesisDistributionVote` — one voter's weight on one item. Size 81.
    pub const PROPOSAL: u32 = 0; // pubkey 32
    pub const VOTER: u32 = 32; // pubkey 32
    pub const WEIGHT: u32 = 64; // u64
    pub const SUPPORT: u32 = 72; // u8
    pub const PRINCIPAL: u32 = 73; // u64
    pub const SIZE: usize = 81;
}

pub mod coin_cfg_offsets {
    //! `CoinConfig` — shared across markets using the same COIN mint. Size 57.
    pub const AUTHORITY: u32 = 0; // pubkey 32
    pub const BOOTSTRAP_START_SLOT: u32 = 32; // u64
    pub const BOOTSTRAP_DELAY_SLOTS: u32 = 40; // u64
    pub const LIVE_SLOT: u32 = 48; // u64
    pub const PHASE: u32 = 56; // u8 (0=bootstrap, 1=live)
    pub const SIZE: usize = 57;
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

// Phase 2 fills these bodies. The genesis handler-body emitters and
// `all_meta_handler_bodies()` land with the lifecycle work; Phase 1 only needs
// the offsets/sentinels above to compile `meta/src/main.v` and run the
// structural link test.
