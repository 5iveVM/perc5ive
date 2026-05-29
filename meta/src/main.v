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
// Sentinel-rewrite convention.
//
// Each handler body is a single `return <sentinel-literal>;` — the
// five-dsl-compiler lowers that to `PUSH_U64 <pooled const>; RETURN_VALUE`,
// the canonical stub the perc5ive linker scans for and rewrites in place to a
// `JUMP` into the matching hand-written body (src/bytecode/meta_handlers.rs).
//
// The literal is inlined directly (NOT routed through a `sentinel_x()` helper
// function): a helper would compile to a separate param-less CALL frame, and
// the rewritten body would then run without the handler's parameters/accounts.
// Inlining keeps the stub *inside* the handler's own entry frame, so
// `LOAD_PARAM` / `LOAD_FIELD` in the appended body see the real arguments.
//
// Literals match src/bytecode/meta_handlers.rs SENTINEL_* (0xFEEDFACEDEAD_60xx):
//   init_coin_config        18369614221520232451   activate_live           18369614221520232459
//   init_genesis_bootstrap  18369614221520232469   genesis_deposit         18369614221520232470
//   genesis_withdraw        18369614221520232471   kickstart_genesis_market 18369614221520232475
//   init_genesis_distribution 18369614221520232477 vote_genesis_distribution 18369614221520232478
//   genesis_mint_reward     18369614221520232472   finalize_genesis        18369614221520232473
//   draw_genesis_surplus    18369614221520232474   recover_genesis_market  18369614221520232476
//   mint_reward             18369614221520232456   transfer_mint_authority 18369614221520232458
//   init_percolator_market  18369614221520232467   percolator_admin        18369614221520232468
//   approve_builder         18369614221520232479
// =============================================================================

// =============================================================================
// Account types (5ive-native packed layout — no alignment padding, no 8-byte
// discriminator; account identity is enforced by the VM owner/PDA checks).
// Offsets mirror src/bytecode/meta_handlers.rs.
// =============================================================================

// CoinConfig — shared across all markets using the same COIN mint. Size 64.
// Lifecycle flags are u64 (not u8): the hand-written bodies run in a
// pool-enabled binary where the only pool-free constant push is a 0..3 nibble,
// so flags must be full words to read/store without a pool constant.
account CoinConfig {
    authority: pubkey;             // @0
    bootstrap_start_slot: u64;     // @32
    bootstrap_delay_slots: u64;    // @40
    live_slot: u64;                // @48
    phase: u64;                    // @56 (0=bootstrap, 1=live)
}

// GenesisConfig — bootstrap vote + principal ledger. Size 144.
account GenesisConfig {
    coin_mint: pubkey;             // @0
    base_mint: pubkey;             // @32
    token_vault: pubkey;           // @64
    total_deposited: u64;          // @96
    total_withdrawn: u64;          // @104
    reward_supply: u64;            // @112
    minted_supply: u64;            // @120
    finalized: u64;                // @128 (0/1)
    kicked: u64;                   // @136 (0/1)
}

// GenesisPosition — per-user base-unit deposit + voting weight. Size 64.
account GenesisPosition {
    owner: pubkey;                 // @0
    amount: u64;                   // @32
    withdrawn: u64;                // @40
    start_slot: u64;               // @48 (last-write-time)
    active_votes: u64;             // @56 (live ballots; must be 0 to exit while voting)
}

// GenesisDistribution — a vote-approved mint allocation item. Size 112.
account GenesisDistribution {
    genesis_cfg: pubkey;           // @0
    destination: pubkey;           // @32
    proposal_id: u64;              // @64
    amount: u64;                   // @72
    yes_votes: u64;                // @80
    no_votes: u64;                 // @88
    executed: u64;                 // @96 (0/1)
    voted_principal: u64;          // @104
}

// GenesisDistributionVote — one voter's weight on one item. Size 96.
account GenesisDistributionVote {
    proposal: pubkey;              // @0
    voter: pubkey;                 // @32
    weight: u64;                   // @64
    support: u64;                  // @72 (0=no, 1=yes)
    retracted: u64;                // @80 (1 once backed out of the tally)
    principal: u64;                // @88
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
    return 18369614221520232451;
}

// activate_live — move bootstrap → live once the delay has elapsed.
pub activate_live(
    coin_cfg: CoinConfig @mut,
    authority: account @signer,
    now_slot: u64
) -> u64 {
    return 18369614221520232459;
}

// init_genesis_bootstrap(reward_supply) — create the genesis ledger with a
// fixed COIN reward supply; counters start at zero.
pub init_genesis_bootstrap(
    genesis_cfg: GenesisConfig @mut,
    authority: account @signer,
    reward_supply: u64
) -> u64 {
    return 18369614221520232469;
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
    return 18369614221520232470;
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
    return 18369614221520232471;
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
    return 18369614221520232475;
}

// init_genesis_distribution(proposal_id, amount) — create an allocation item.
pub init_genesis_distribution(
    genesis_cfg: GenesisConfig,
    distribution: GenesisDistribution @mut,
    proposer: account @signer,
    proposal_id: u64,
    amount: u64
) -> u64 {
    return 18369614221520232477;
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
    return 18369614221520232478;
}

// genesis_mint_reward(amount) — mint a majority-approved + quorum-cleared item;
// cannot exceed reward_supply; requires a kicked market.
pub genesis_mint_reward(
    genesis_cfg: GenesisConfig @mut,
    distribution: GenesisDistribution @mut,
    authority: account @signer,
    amount: u64
) -> u64 {
    return 18369614221520232472;
}

// finalize_genesis — complete genesis: requires a kicked market and full
// reward-supply distribution (minted_supply == reward_supply).
pub finalize_genesis(
    genesis_cfg: GenesisConfig @mut,
    authority: account @signer
) -> u64 {
    return 18369614221520232473;
}

// draw_genesis_surplus(amount, vault_balance) — DAO draws only vault balance
// above outstanding genesis principal; requires finalized.
pub draw_genesis_surplus(
    genesis_cfg: GenesisConfig,
    authority: account @signer,
    amount: u64,
    vault_balance: u64
) -> u64 {
    return 18369614221520232474;
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
    return 18369614221520232476;
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
    return 18369614221520232456;
}

pub transfer_mint_authority(
    coin_cfg: CoinConfig,
    authority: account @signer,
    new_authority: pubkey
) -> u64 {
    return 18369614221520232458;
}

pub init_percolator_market(
    coin_cfg: CoinConfig,
    market_admin: account @mut,
    user: account @signer
) -> u64 {
    return 18369614221520232467;
}

pub percolator_admin(
    coin_cfg: CoinConfig,
    authority: account @signer,
    tag: u64
) -> u64 {
    return 18369614221520232468;
}

pub approve_builder(
    approval: BuilderApproval @mut,
    authority: account @signer,
    enabled: u8
) -> u64 {
    return 18369614221520232479;
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
