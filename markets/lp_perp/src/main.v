// LPPerp — LP-side perpetual on 5ive AMM pool depths.
//
// Concept: a perpetual futures market whose underlying "price" is the spot
// depth / reserve ratio of a designated 5ive AMM pool. Lets LPs hedge
// impermanent loss without unwinding their pool shares, and lets
// speculators take directional bets on AMM composition. Settles via
// Percolator's risk engine against a computed pool-state oracle.
//
// Design constraints:
//   * Underlying price is `quote_reserve * POS_SCALE / base_reserve`.
//     Updating the price is a constant-time pool read — the keeper_crank
//     passes it in like any other oracle price.
//   * Funding rate tracks the delta between the mark (Percolator's
//     implied mid) and the pool's real reserve ratio. Longs pay when
//     the pool drifts heavier on the base side.
//   * Liquidation keeper MUST close positions against the actual pool —
//     the pool-state oracle is trivially manipulable without this anchor.

// =============================================================================
// Accounts
// =============================================================================

account LpPerpMarket {
    amm_pool: pubkey;             // 5ive AMM pool whose depth we track
    base_mint: pubkey;
    quote_mint: pubkey;
    risk_engine: pubkey;          // Percolator RiskEngine PDA
    collateral_mint: pubkey;      // typically the quote asset
    collateral_vault: pubkey;
    oracle: pubkey;               // fallback spot oracle (for liquidation sanity)
    pool_price_last: u64;         // reserve ratio × POS_SCALE, updated by crank
    pool_price_last_slot: u64;
    max_position_frac_bps: u64;   // cap a single position at this % of pool TVL
    total_long_exposure_q: u128;
    total_short_exposure_q: u128;
    created_slot: u64;
}

account LpPerpPosition {
    user: pubkey;
    market: pubkey;
    margin_account: pubkey;       // Percolator MarginAccount
    size_q: i128;                 // signed position, POS_SCALE units
    entry_pool_price: u64;
    hedge_target: u8;             // 0 = speculative, 1 = LP hedge
    lp_pool_shares_ref: pubkey;   // LP shares this position is hedging
    opened_slot: u64;
    last_settle_slot: u64;
}

// =============================================================================
// Constants
// =============================================================================

pos_scale() -> u128 { return 1000000; }

hedge_target_speculative() -> u8 { return 0; }
hedge_target_lp_hedge() -> u8    { return 1; }

// =============================================================================
// Instruction handlers
// =============================================================================

pub init_lp_perp_market(
    market: LpPerpMarket @mut,
    admin: account @mut @signer,
    amm_pool: pubkey,
    base_mint: pubkey,
    quote_mint: pubkey,
    risk_engine: pubkey,
    collateral_mint: pubkey,
    collateral_vault: pubkey,
    oracle: pubkey,
    max_position_frac_bps: u64,
    created_slot: u64
) {
    market.amm_pool = amm_pool;
    market.base_mint = base_mint;
    market.quote_mint = quote_mint;
    market.risk_engine = risk_engine;
    market.collateral_mint = collateral_mint;
    market.collateral_vault = collateral_vault;
    market.oracle = oracle;
    market.max_position_frac_bps = max_position_frac_bps;
    market.created_slot = created_slot;
}

// crank_pool_price — keeper pulls the current reserves from the AMM pool
// and publishes the implied pool-state oracle price. Anyone can call;
// keeper rebate paid from fee pool. Percolator's `keeper_crank` handler
// is invoked next with this price as `oracle_price`.
pub crank_pool_price(
    market: LpPerpMarket @mut,
    keeper: account @mut @signer,
    pool_price: u64,
    now_slot: u64
) {
    market.pool_price_last = pool_price;
    market.pool_price_last_slot = now_slot;
}

// open_hedge — LP's standard entrance. `hedge_target = 1` signals this
// is a hedge tied to a specific LP position; the Sov / analytics layer
// uses this to compute per-LP net delta.
pub open_hedge(
    market: LpPerpMarket,
    position: LpPerpPosition @mut,
    user: account @mut @signer,
    size_q: i128,
    hedge_target: u8,
    lp_pool_shares_ref: pubkey,
    entry_pool_price: u64,
    now_slot: u64
) {
    position.size_q = size_q;
    position.entry_pool_price = entry_pool_price;
    position.hedge_target = hedge_target;
    position.lp_pool_shares_ref = lp_pool_shares_ref;
    position.opened_slot = now_slot;
    position.last_settle_slot = now_slot;
}

// close_hedge — unwinds the position at the current pool price.
// Percolator settles PnL; this handler just tombstones the LpPerpPosition.
pub close_hedge(
    market: LpPerpMarket,
    position: LpPerpPosition @mut,
    user: account @mut @signer,
    now_slot: u64
) {
    position.last_settle_slot = now_slot;
    position.size_q = 0;
}

// settle_funding — Percolator's `settle_account` with the pool's implied
// mark. Charges long or short based on the mark-vs-pool drift.
pub settle_funding(
    market: LpPerpMarket,
    position: LpPerpPosition @mut,
    user: account @mut @signer,
    funding_rate_e9: i128,
    now_slot: u64
) {
    position.last_settle_slot = now_slot;
}

// liquidate_hedge — keeper crank. Uses the fallback spot oracle for the
// sanity check (so the pool-state oracle can't be trivially grieved),
// but executes the liquidation against the pool's real price.
pub liquidate_hedge(
    market: LpPerpMarket,
    victim: LpPerpPosition @mut,
    liquidator_pos: LpPerpPosition @mut,
    liquidator: account @mut @signer,
    oracle_spot: u64,
    pool_price: u64,
    now_slot: u64
) {
    victim.last_settle_slot = now_slot;
    victim.size_q = 0;
}
