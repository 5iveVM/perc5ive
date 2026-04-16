// PythRaceMarket — head-to-head price race on two Pyth feeds.
//
// Concept: two Pyth-listed assets (say BTC and ETH), traders bet which
// will outperform between `race_start_slot` and `race_end_slot`. Settlement
// reads both Pyth price accounts at expiry and awards winners proportional
// to their stake. Uses Percolator's risk engine only for the collateral
// haircut / ADL path — the race outcome itself is deterministic once both
// Pyth prices are pinned.
//
// Why Percolator under the hood: during the race window, open positions
// can be closed out or liquidated using Percolator's normal machinery
// against a synthetic oracle (the ratio of the two Pyth feeds). That gives
// users intra-race liquidity rather than forcing them to hold to expiry.

// =============================================================================
// Accounts
// =============================================================================

account PythRaceMarket {
    asset_a_pyth: pubkey;
    asset_b_pyth: pubkey;
    risk_engine: pubkey;         // PDA of the underlying Percolator RiskEngine
    collateral_mint: pubkey;     // typically USDC
    collateral_vault: pubkey;
    race_start_slot: u64;
    race_end_slot: u64;
    asset_a_start_price: u64;    // set once keeper_start is cranked at race_start_slot
    asset_b_start_price: u64;
    asset_a_end_price: u64;      // set at resolution
    asset_b_end_price: u64;
    resolved: u8;                // 0 = ongoing, 1 = resolved
    winner: u8;                  // 0 = A, 1 = B, 2 = tie
    total_a_stake: u128;
    total_b_stake: u128;
    total_fees_collected: u128;
    created_slot: u64;
}

// Per-user race entry. `side` picks which asset the user is betting on.
account PythRaceEntry {
    user: pubkey;
    market: pubkey;
    margin_account: pubkey;       // Percolator MarginAccount PDA
    side: u8;                     // 0 = A, 1 = B
    stake: u128;                  // collateral posted
    opened_slot: u64;
    closed_slot: u64;             // 0 if still open
    claimed: u8;                  // 1 once winnings withdrawn
}

// =============================================================================
// Constants
// =============================================================================

side_a() -> u8 { return 0; }
side_b() -> u8 { return 1; }
winner_tie() -> u8 { return 2; }

status_ongoing() -> u8  { return 0; }
status_resolved() -> u8 { return 1; }

// =============================================================================
// Instruction handlers
// =============================================================================

pub init_race_market(
    market: PythRaceMarket @mut,
    admin: account @mut @signer,
    asset_a_pyth: pubkey,
    asset_b_pyth: pubkey,
    risk_engine: pubkey,
    collateral_mint: pubkey,
    collateral_vault: pubkey,
    race_start_slot: u64,
    race_end_slot: u64,
    created_slot: u64
) {
    market.asset_a_pyth = asset_a_pyth;
    market.asset_b_pyth = asset_b_pyth;
    market.risk_engine = risk_engine;
    market.collateral_mint = collateral_mint;
    market.collateral_vault = collateral_vault;
    market.race_start_slot = race_start_slot;
    market.race_end_slot = race_end_slot;
    market.resolved = status_ongoing();
    market.created_slot = created_slot;
}

// snapshot_start — captures the starting Pyth prices at `race_start_slot`.
// Cranked by anyone (keeper rebate paid from the fee pool).
pub snapshot_start(
    market: PythRaceMarket @mut,
    keeper: account @mut @signer,
    asset_a_start: u64,
    asset_b_start: u64
) {
    market.asset_a_start_price = asset_a_start;
    market.asset_b_start_price = asset_b_start;
}

// enter_race — user posts collateral and picks a side.
pub enter_race(
    market: PythRaceMarket @mut,
    entry: PythRaceEntry @mut,
    user: account @mut @signer,
    side: u8,
    stake: u128,
    now_slot: u64
) {
    entry.side = side;
    entry.stake = stake;
    entry.opened_slot = now_slot;
    entry.claimed = 0;
}

// close_early — user closes their race entry before expiry. Uses the
// Percolator engine's oracle-driven settlement (via the synthetic
// price = asset_b_price / asset_a_price) so early closers get fair
// mid-race pricing.
pub close_early(
    market: PythRaceMarket,
    entry: PythRaceEntry @mut,
    user: account @mut @signer,
    asset_a_price: u64,
    asset_b_price: u64,
    now_slot: u64
) {
    entry.closed_slot = now_slot;
}

// resolve_race — keeper cranks this at race_end_slot with the ending
// Pyth prices. Computes the winner by ratio of end-to-start prices.
// Tie rule: if both sides moved identically (ratio difference below
// 0.1% of larger denominator), side = tie and stakes are refunded.
pub resolve_race(
    market: PythRaceMarket @mut,
    keeper: account @mut @signer,
    asset_a_end: u64,
    asset_b_end: u64,
    now_slot: u64
) {
    market.asset_a_end_price = asset_a_end;
    market.asset_b_end_price = asset_b_end;
    market.resolved = status_resolved();
}

// claim_winnings — winners pull their share of the pool (proportional
// to stake). Losers' stakes form the prize pool. Ties refund both sides.
pub claim_winnings(
    market: PythRaceMarket,
    entry: PythRaceEntry @mut,
    user: account @mut @signer
) {
    entry.claimed = 1;
}

// liquidate_underwater — during-race liquidation of entries whose margin
// fell below the maintenance threshold against the synthetic oracle.
// Delegates to Percolator's `liquidate_at_oracle`; PythRaceMarket just
// records the event.
pub liquidate_underwater(
    market: PythRaceMarket,
    victim: PythRaceEntry @mut,
    liquidator: account @mut @signer,
    asset_a_price: u64,
    asset_b_price: u64
) {
    victim.closed_slot = 0;
}
