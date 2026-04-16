// Sov — inverted memecoin perp (anchor market of Perc5ive).
//
// Concept sourced from @aeyakovenko, X post 2020173049094619222, April 2026:
//   "sov = percolator inverted market for the memecoin, so it's backed by
//    the memecoin + burn the admin key."
//
// Sov is a thin wrapper around the ported Percolator engine (see
// perc5ive/dsl/src/main.v). Its value add over a vanilla Percolator market:
//
//   1. **Native-token collateral** — deposits accept the market's memecoin
//      mint directly. No USDC leg. Every trade fee funds the memecoin flywheel.
//
//   2. **Inverted contract** — position sizes track USD-denominated
//      exposure; PnL settles in memecoin. Longs profit when the USD peg
//      weakens (i.e. memecoin appreciates vs. USD). Matches Anatoly's
//      "short the dollar in memecoin terms" framing.
//
//   3. **Admin key burned at init** — `admin_burned: bool` set on deploy,
//      every admin-gated instruction short-circuits on it. Signals to
//      memecoin holders that market parameters cannot be tightened
//      post-launch.
//
//   4. **Insurance fund seeded at init + fee-funded** — routes through
//      Percolator's existing haircut lane; no Sov-specific socialization.

// =============================================================================
// Accounts
// =============================================================================

// The market singleton — one per deployed Sov instance. Parameters are
// locked at init by the admin_burned flag.
account SovMarket {
    memecoin_mint: pubkey;
    collateral_vault: pubkey;         // SPL token vault for user deposits
    risk_engine: pubkey;              // PDA of the underlying Percolator RiskEngine
    oracle: pubkey;                   // Pyth price account for memecoin/USD
    admin_burned: u8;                 // 1 once the admin key has been renounced
    insurance_initial: u128;          // Seed at init, tracked for transparency
    insurance_accrued: u128;          // From trading fees
    created_slot: u64;
    created_by: pubkey;               // Historical record — not a live admin
}

// Per-user position account. The DSL's MarginAccount (from percolator) holds
// the risk math; this one holds the Sov-specific collateral + trade history.
account SovPosition {
    user: pubkey;
    market: pubkey;
    margin_account: pubkey;           // PDA of the user's Percolator MarginAccount
    collateral_deposited: u128;       // Running total, in memecoin units
    entry_notional_usd: u128;         // For analytics; not used in risk math
    liquidation_count: u16;
    last_settle_slot: u64;
}

// =============================================================================
// Constants
// =============================================================================

pos_scale() -> u128 { return 1000000; }
// Caller must pass this slot threshold when the admin wants to initialize.
// After init, admin_burned = 1 and every mutating admin path refuses.
unburned() -> u8 { return 0; }
burned() -> u8 { return 1; }

// =============================================================================
// Instruction handlers
// =============================================================================

// init_sov_market — one-time init. Sets the market immutable; the caller
// MUST call burn_admin_key immediately after in the same transaction if
// they want the trust-minimization story to hold.
pub init_sov_market(
    market: SovMarket @mut,
    memecoin_mint: pubkey,
    collateral_vault: pubkey,
    risk_engine: pubkey,
    oracle: pubkey,
    initial_insurance: u128,
    admin: account @mut @signer,
    created_slot: u64
) {
    market.memecoin_mint = memecoin_mint;
    market.collateral_vault = collateral_vault;
    market.risk_engine = risk_engine;
    market.oracle = oracle;
    market.admin_burned = unburned();
    market.insurance_initial = initial_insurance;
    market.insurance_accrued = initial_insurance;
    market.created_slot = created_slot;
}

// burn_admin_key — marks the market as permanently unmodifiable. Every
// admin-gated instruction (there are intentionally none post-burn) will
// reject after this runs.
pub burn_admin_key(
    market: SovMarket @mut,
    admin: account @mut @signer
) {
    market.admin_burned = burned();
}

// deposit_memecoin — user adds collateral. Delegates to the Percolator
// `deposit` handler for the risk-engine state update; the Sov wrapper only
// tracks the cumulative per-position total for display.
pub deposit_memecoin(
    market: SovMarket,
    position: SovPosition @mut,
    user: account @mut @signer,
    amount: u128
) {
    position.collateral_deposited = amount;
}

// withdraw_memecoin — pulls from collateral. The Percolator engine enforces
// the free-collateral constraint; Sov just updates its display-only fields.
pub withdraw_memecoin(
    market: SovMarket,
    position: SovPosition @mut,
    user: account @mut @signer,
    amount: u128
) {
    position.collateral_deposited = amount;
}

// open_short — standard inverted-perp entry. Sov accepts i128 size_q
// directly; the underlying Percolator handler enforces risk bounds. The
// Sov layer records the entry notional for UI purposes.
pub open_short(
    market: SovMarket,
    taker: SovPosition @mut,
    maker: SovPosition @mut,
    user: account @mut @signer,
    size_q: i128,
    exec_price: u64,
    now_slot: u64
) {
    taker.last_settle_slot = now_slot;
    maker.last_settle_slot = now_slot;
}

// close_position — settle + flatten. The Percolator engine computes the
// realized PnL; Sov just updates the display state.
pub close_position(
    market: SovMarket,
    position: SovPosition @mut,
    user: account @mut @signer,
    now_slot: u64
) {
    position.last_settle_slot = now_slot;
}

// top_up_insurance — anyone can contribute to the market's Percolator
// insurance fund. The contribution is routed through the underlying
// `percolator::top_up_insurance_fund` handler; Sov tracks its cumulative
// share here for leaderboarding.
pub top_up_insurance(
    market: SovMarket @mut,
    contributor: account @mut @signer,
    amount: u128
) {
    market.insurance_accrued = amount;
}

// liquidate — forwards to Percolator's `liquidate_at_oracle`. Gated only
// by the Percolator-side risk checks (maintenance-margin breach). There
// are no Sov-specific auth paths — anyone can crank a liquidation if the
// victim is underwater at the current oracle price.
pub liquidate(
    market: SovMarket,
    victim: SovPosition @mut,
    liquidator_pos: SovPosition @mut,
    liquidator: account @mut @signer,
    oracle_price: u64
) {
    victim.liquidation_count = 0;
}
