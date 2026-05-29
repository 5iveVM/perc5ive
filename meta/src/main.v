// MetaGenesis — percolator-meta (MetaDAO-futarchy fair-launch / genesis +
// market-factory layer) ported to the 5ive DSL.
//
// Source of truth:
//   hello_slab/percolator-meta/program/src/lib.rs  (read-only oracle)
//   hello_slab/percolator-meta/spec.md + README.md
//   pinned SHA recorded in the commit that lands each phase.
//
// Model: depositing base units into the genesis vault is a Sybil bond — capital
// at risk in the bootstrap market, recoverable on withdrawal (pro-rata under
// loss), earning only time-weighted voting power over the fixed COIN
// distribution. No yield. The winning distribution mints the COIN and becomes
// the MetaDAO.
//
// Architecture mirrors dsl/src/main.v: every handler with non-trivial logic is
// a sentinel-returning stub that the perc5ive linker rewrites in place to
// hand-written bytecode (see src/bytecode/meta_handlers.rs). Unlike the
// risk-engine port, the genesis ledger is **all-u64** (no u128/i128 fields), so
// the bodies use plain ADD/SUB/MUL/DIV — the five-dsl-compiler u128
// LHS-assignment regression does not apply here.

// =============================================================================
// Phase constants
// =============================================================================

phase_bootstrap() -> u8 { return 0; }
phase_live() -> u8      { return 1; }

// Vote support codes (vote_genesis_distribution): 0=no, 1=yes, 2=retract.
support_no() -> u8      { return 0; }
support_yes() -> u8     { return 1; }
support_retract() -> u8 { return 2; }

// =============================================================================
// Sentinel constants for handler-body rewriting (match
// src/bytecode/meta_handlers.rs SENTINEL_* — values 0xFEEDFACEDEAD_60xx).
// =============================================================================

sentinel_init_coin_config()        -> u64 { return 18369614221520232451; }
sentinel_mint_reward()             -> u64 { return 18369614221520232456; }
sentinel_transfer_mint_authority() -> u64 { return 18369614221520232458; }
sentinel_activate_live()           -> u64 { return 18369614221520232459; }
sentinel_init_percolator_market()  -> u64 { return 18369614221520232467; }
sentinel_percolator_admin()        -> u64 { return 18369614221520232468; }
sentinel_init_genesis_bootstrap()  -> u64 { return 18369614221520232469; }
sentinel_genesis_deposit()         -> u64 { return 18369614221520232470; }
sentinel_genesis_withdraw()        -> u64 { return 18369614221520232471; }
sentinel_genesis_mint_reward()     -> u64 { return 18369614221520232472; }
sentinel_finalize_genesis()        -> u64 { return 18369614221520232473; }
sentinel_draw_genesis_surplus()    -> u64 { return 18369614221520232474; }
sentinel_kickstart_genesis_market()-> u64 { return 18369614221520232475; }
sentinel_recover_genesis_market()  -> u64 { return 18369614221520232476; }
sentinel_init_genesis_distribution()->u64 { return 18369614221520232477; }
sentinel_vote_genesis_distribution()->u64 { return 18369614221520232478; }
sentinel_approve_builder()         -> u64 { return 18369614221520232479; }

// =============================================================================
// Account types (5ive-native packed layout — no alignment padding, no 8-byte
// discriminator; account identity is enforced by the VM owner/PDA checks).
// Offsets mirror src/bytecode/meta_handlers.rs.
// =============================================================================

// CoinConfig — shared across all markets using the same COIN mint. Size 57.
account CoinConfig {
    authority: pubkey;             // @0
    bootstrap_start_slot: u64;     // @32
    bootstrap_delay_slots: u64;    // @40
    live_slot: u64;                // @48
    phase: u8;                     // @56
}

// GenesisConfig — bootstrap vote + principal ledger. Size 130.
account GenesisConfig {
    coin_mint: pubkey;             // @0
    base_mint: pubkey;             // @32
    token_vault: pubkey;           // @64
    total_deposited: u64;          // @96
    total_withdrawn: u64;          // @104
    reward_supply: u64;            // @112
    minted_supply: u64;            // @120
    finalized: u8;                 // @128
    kicked: u8;                    // @129
}

// GenesisPosition — per-user base-unit deposit + voting weight. Size 64.
account GenesisPosition {
    owner: pubkey;                 // @0
    amount: u64;                   // @32
    withdrawn: u64;                // @40
    start_slot: u64;               // @48 (last-write-time)
    active_votes: u64;             // @56 (live ballots; must be 0 to exit while voting)
}

// GenesisDistribution — a vote-approved mint allocation item. Size 105.
account GenesisDistribution {
    genesis_cfg: pubkey;           // @0
    destination: pubkey;           // @32
    proposal_id: u64;              // @64
    amount: u64;                   // @72
    yes_votes: u64;                // @80
    no_votes: u64;                 // @88
    executed: u8;                  // @96
    voted_principal: u64;          // @97
}

// GenesisDistributionVote — one voter's weight on one item. Size 82.
account GenesisDistributionVote {
    proposal: pubkey;              // @0
    voter: pubkey;                 // @32
    weight: u64;                   // @64
    support: u8;                   // @72
    retracted: u8;                 // @73
    principal: u64;                // @74
}

// BuilderApproval — governed builder-code registry entry. Size 137.
account BuilderApproval {
    coin_mint: pubkey;             // @0
    builder_program: pubkey;       // @32
    code_hash: pubkey;             // @64
    terms_hash: pubkey;            // @96
    approved_slot: u64;            // @128
    enabled: u8;                   // @136
}

// =============================================================================
// Genesis-lifecycle handlers — Phase 2 rewrites these bodies (sentinel stubs).
//
// Account params are declared first (account-table indices base at 2 under the
// mono DSL convention: account-decl-index + VM_ACCOUNT_PARAM_OFFSET). Scalar
// params follow and compact into params[1..] in declaration order.
// =============================================================================

// init_coin_config(bootstrap_delay_slots) — one-time COIN setup. Zero delay
// starts live immediately; nonzero requires a later activate_live.
pub init_coin_config(
    coin_cfg: CoinConfig @mut,
    authority: account @signer,
    bootstrap_delay_slots: u64,
    start_slot: u64
) -> u64 {
    return sentinel_init_coin_config();
}

// activate_live — move bootstrap → live once the delay has elapsed.
pub activate_live(
    coin_cfg: CoinConfig @mut,
    authority: account @signer,
    now_slot: u64
) -> u64 {
    return sentinel_activate_live();
}

// init_genesis_bootstrap(reward_supply) — create the genesis ledger with a
// fixed COIN reward supply; counters start at zero.
pub init_genesis_bootstrap(
    genesis_cfg: GenesisConfig @mut,
    authority: account @signer,
    reward_supply: u64
) -> u64 {
    return sentinel_init_genesis_bootstrap();
}

// genesis_deposit(amount) — Sybil-bond deposit; one vote unit per base unit;
// (re)sets the position's start_slot (last-write-time). Closes at kickstart.
pub genesis_deposit(
    genesis_cfg: GenesisConfig @mut,
    position: GenesisPosition @mut,
    user: account @signer,
    amount: u64,
    now_slot: u64
) -> u64 {
    return sentinel_genesis_deposit();
}

// genesis_withdraw(insurance_pull, backing_pull, vault_balance) — exit any
// time; forfeits the vote (start_slot -> 0). Locked only while voting if the
// position still has live ballots (active_votes != 0 -> must retract first).
pub genesis_withdraw(
    genesis_cfg: GenesisConfig @mut,
    position: GenesisPosition @mut,
    user: account @signer,
    coin_phase: u64,
    recovered: u64,
    vault_balance: u64
) -> u64 {
    return sentinel_genesis_withdraw();
}

// kickstart_genesis_market(backing_domain, expiry) — deploy the pooled base
// units 50/50 (insurance = floor(total/2), backing = remainder) into the
// PDA-admin Percolator market and mark the genesis kicked.
pub kickstart_genesis_market(
    genesis_cfg: GenesisConfig @mut,
    authority: account @signer,
    backing_domain: u64,
    backing_expiry_slot: u64
) -> u64 {
    return sentinel_kickstart_genesis_market();
}

// init_genesis_distribution(proposal_id, amount) — create an allocation item.
pub init_genesis_distribution(
    genesis_cfg: GenesisConfig,
    distribution: GenesisDistribution @mut,
    proposer: account @signer,
    proposal_id: u64,
    amount: u64
) -> u64 {
    return sentinel_init_genesis_distribution();
}

// vote_genesis_distribution(action) — action 0=no, 1=yes, 2=retract. Weighted
// power floor(log2(age)) * staked; quorum counts each voter's staked principal
// once. Maintains active_votes so a cast ballot never outlives its capital.
pub vote_genesis_distribution(
    position: GenesisPosition @mut,
    distribution: GenesisDistribution @mut,
    vote: GenesisDistributionVote @mut,
    voter: account @signer,
    action: u64,
    now_slot: u64
) -> u64 {
    return sentinel_vote_genesis_distribution();
}

// genesis_mint_reward(amount) — mint a majority-approved + quorum-cleared item;
// cannot exceed reward_supply; requires a kicked market.
pub genesis_mint_reward(
    genesis_cfg: GenesisConfig @mut,
    distribution: GenesisDistribution @mut,
    authority: account @signer,
    amount: u64
) -> u64 {
    return sentinel_genesis_mint_reward();
}

// finalize_genesis — complete genesis: requires a kicked market and full
// reward-supply distribution (minted_supply == reward_supply).
pub finalize_genesis(
    genesis_cfg: GenesisConfig @mut,
    authority: account @signer
) -> u64 {
    return sentinel_finalize_genesis();
}

// draw_genesis_surplus(amount, vault_balance) — DAO draws only vault balance
// above outstanding genesis principal; requires finalized.
pub draw_genesis_surplus(
    genesis_cfg: GenesisConfig,
    authority: account @signer,
    amount: u64,
    vault_balance: u64
) -> u64 {
    return sentinel_draw_genesis_surplus();
}

// recover_genesis_market(kind, domain, amount) — recover bootstrap market funds
// to the genesis vault (CPI). Disabled after finalization. Pure custody move;
// the genesis ledger counters are unchanged.
pub recover_genesis_market(
    genesis_cfg: GenesisConfig,
    authority: account @signer,
    kind: u64,
    domain: u64,
    amount: u64
) -> u64 {
    return sentinel_recover_genesis_market();
}

// =============================================================================
// Governed COIN ops + market factory — Phase 3 (DEFER UNTIL VALIDATED).
// Declared with ABI-stable signatures; bodies remain sentinel stubs (not
// rewritten by the linker) until the post-validation governance work lands.
// =============================================================================

pub mint_reward(
    coin_cfg: CoinConfig,
    authority: account @signer,
    amount: u64
) -> u64 {
    return sentinel_mint_reward();
}

pub transfer_mint_authority(
    coin_cfg: CoinConfig,
    authority: account @signer,
    new_authority: pubkey
) -> u64 {
    return sentinel_transfer_mint_authority();
}

pub init_percolator_market(
    coin_cfg: CoinConfig,
    market_admin: account @mut,
    user: account @signer
) -> u64 {
    return sentinel_init_percolator_market();
}

pub percolator_admin(
    coin_cfg: CoinConfig,
    authority: account @signer,
    tag: u64
) -> u64 {
    return sentinel_percolator_admin();
}

pub approve_builder(
    approval: BuilderApproval @mut,
    authority: account @signer,
    enabled: u8
) -> u64 {
    return sentinel_approve_builder();
}

// =============================================================================
// Squads v4 key handover — Phase 7 sub-step (DEFER UNTIL VALIDATED).
// Real Squads v4 CPI is product surface, not needed for the genesis demo.
// Bodies are require(false) placeholders so any accidental invocation traps.
// TODO(phase7-deferred): implement the multisig create + config_authority
// rotation CPI into the Squads v4 mainnet binary.
// =============================================================================

pub init_genesis_squads(
    coin_cfg: CoinConfig,
    authority: account @signer
) {
    require(false);
}

pub handover_genesis_squads(
    coin_cfg: CoinConfig,
    authority: account @signer
) {
    require(false);
}
