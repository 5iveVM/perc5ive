// Perc5ive — Percolator risk engine (v12.17.0) ported to 5ive DSL.
//
// Source of truth for math and invariants:
//   hello_slab/percolator/src/percolator.rs  (4,881 lines Rust)
//   hello_slab/percolator/src/wide_math.rs   (2,067 lines, u256/i256 ops)
//   hello_slab/percolator/src/i128.rs        (927 lines, BPF-safe i128 wrapper)
//
// This file is the **skeleton**. The five-dsl-compiler rejects u128 rvalues
// from binary ops or function calls when they bind to a let or field (tracked
// in memory/project_five_dsl_compiler_regression.md). So every u128-arithmetic
// handler is a sentinel stub that the perc5ive linker rewrites to hand-written
// bytecode appended after compile. See `perc5ive/src/bytecode/link.rs` +
// `perc5ive/src/bytecode/handlers.rs` for the rewriter and handler bytecode.

// =============================================================================
// Constants (Percolator spec §1, §4, §6)
// =============================================================================

pos_scale() -> u128         { return 1000000; }
adl_one() -> u128           { return 1000000000000000; }
min_a_side() -> u128        { return 100000000000000; }
max_oracle_price() -> u64   { return 1000000000000; }
gc_close_budget() -> u32    { return 32; }
accounts_per_crank() -> u16 { return 128; }
liq_budget_per_crank() -> u16 { return 64; }
bps_scale() -> u64          { return 10000; }

market_mode_active() -> u8    { return 0; }
market_mode_halted() -> u8    { return 1; }
market_mode_resolving() -> u8 { return 2; }
market_mode_resolved() -> u8  { return 3; }

kind_user() -> u8 { return 0; }
kind_lp() -> u8   { return 1; }

// =============================================================================
// Sentinel constants for handler-body rewriting
// =============================================================================
// Each u128-arithmetic handler below returns one of these sentinels. The
// linker (perc5ive::bytecode::link::Linker) scans the compiled `.fbin` for
// PUSH_U64<vle>; RETURN_VALUE sequences matching these constants and rewrites
// them to CALL into appended hand-written bytecode that performs the real
// load-field / u128-arith / store-field sequence.
//
// Values are 0xFEED_FACE_DEAD_5000 + handler_id. Kept large enough to make
// accidental collision with DSL-emitted literals astronomically unlikely.

sentinel_top_up_insurance_fund() -> u64 { return 18369614221520228353; }
sentinel_deposit()               -> u64 { return 18369614221520228354; }
sentinel_withdraw()              -> u64 { return 18369614221520228355; }
sentinel_convert_released_pnl()  -> u64 { return 18369614221520228356; }
sentinel_close_account()         -> u64 { return 18369614221520228357; }
sentinel_settle_account()        -> u64 { return 18369614221520228358; }
sentinel_execute_trade()         -> u64 { return 18369614221520228359; }
sentinel_liquidate_at_oracle()   -> u64 { return 18369614221520228360; }
sentinel_keeper_crank()          -> u64 { return 18369614221520228361; }

// =============================================================================
// Account types
// =============================================================================

// Per-user / per-LP margin account (spec §2.1).
// Source: percolator.rs:240-301. Named `MarginAccount` because `Account` is
// a DSL-reserved type.
//
// Field offsets (DSL packs with no alignment — verified by compiler probe):
//   holder                  @   0  (pubkey 32) — renamed from `owner`
//                                                 (owner is DSL-reserved as
//                                                 the Solana account owner
//                                                 program, immutable).
//   matcher_program         @  32  (pubkey 32)
//   matcher_context         @  64  (pubkey 32)
//   kind                    @  96  (u8 1)
//   account_idx             @  97  (u16 2)
//   capital                 @  99  (u128 16)
//   pnl                     @ 115  (i128 16)
//   reserved_pnl            @ 131  (u128 16)
//   position_basis_q        @ 147  (i128 16)
//   adl_a_basis             @ 163  (u128 16)
//   adl_k_snap              @ 179  (i128 16)
//   f_snap                  @ 195  (i128 16)
//   adl_epoch_snap          @ 211  (u64 8)
//   fee_credits             @ 219  (i128 16)
//   sched_present           @ 235  (u8 1)
//   sched_remaining_q       @ 236  (u128 16)
//   sched_anchor_q          @ 252  (u128 16)
//   sched_start_slot        @ 268  (u64 8)
//   sched_horizon           @ 276  (u64 8)
//   sched_release_q         @ 284  (u128 16)
//   pending_present         @ 300  (u8 1)
//   pending_remaining_q     @ 301  (u128 16)
//   pending_horizon         @ 317  (u64 8)
//   pending_created_slot    @ 325  (u64 8)
//   (total size 333)
account MarginAccount {
    holder: pubkey;
    matcher_program: pubkey;
    matcher_context: pubkey;
    kind: u8;
    account_idx: u16;

    capital: u128;
    pnl: u128;
    reserved_pnl: u128;

    position_basis_q: u128;

    adl_a_basis: u128;
    adl_k_snap: u128;
    f_snap: u128;
    adl_epoch_snap: u64;

    fee_credits: u128;

    sched_present: u8;
    sched_remaining_q: u128;
    sched_anchor_q: u128;
    sched_start_slot: u64;
    sched_horizon: u64;
    sched_release_q: u128;
    pending_present: u8;
    pending_remaining_q: u128;
    pending_horizon: u64;
    pending_created_slot: u64;
}

// Global risk-engine state (spec §2.2). Source: percolator.rs:367-485.
//
// Field offsets (DSL packs with no alignment — verified by compiler probe):
//   admin                       @   0  (pubkey 32)
//   matcher_program             @  32  (pubkey 32)
//   oracle                      @  64  (pubkey 32)
//   vault                       @  96  (u128 16)
//   insurance_fund              @ 112  (u128 16)
//   maintenance_margin_bps      @ 128  (u64 8)
//   initial_margin_bps          @ 136  (u64 8)
//   trading_fee_bps             @ 144  (u64 8)
//   max_accounts                @ 152  (u64 8)
//   new_account_fee             @ 160  (u128 16)
//   max_crank_staleness_slots   @ 176  (u64 8)
//   liquidation_fee_bps         @ 184  (u64 8)
//   liquidation_fee_cap         @ 192  (u128 16)
//   min_liquidation_abs         @ 208  (u128 16)
//   min_initial_deposit         @ 224  (u128 16)
//   min_nonzero_mm_req          @ 240  (u128 16)
//   min_nonzero_im_req          @ 256  (u128 16)
//   insurance_floor             @ 272  (u128 16)
//   h_min                       @ 288  (u64 8)
//   h_max                       @ 296  (u64 8)
//   resolve_price_deviation_bps @ 304  (u64 8)
//   current_slot                @ 312  (u64 8)
//   current_oracle_price        @ 320  (u64 8)
//   current_funding_rate_e9     @ 328  (i128 16)
//   market_mode                 @ 344  (u8 1)
//   resolved_price              @ 345  (u64 8)
//   resolved_slot               @ 353  (u64 8)
//   resolved_h_num              @ 361  (u128 16)
//   resolved_h_den              @ 377  (u128 16)
//   c_tot                       @ 393  (u128 16)
//   h_num                       @ 409  (u128 16)
//   adl_a_long_limb_0           @ 425  (u64 8)
//   adl_a_long_limb_1           @ 433  (u64 8)
//   adl_a_long_limb_2           @ 441  (u64 8)
//   adl_a_long_limb_3           @ 449  (u64 8)
//   adl_a_short_limb_0          @ 457  (u64 8)
//   adl_a_short_limb_1          @ 465  (u64 8)
//   adl_a_short_limb_2          @ 473  (u64 8)
//   adl_a_short_limb_3          @ 481  (u64 8)
//   adl_k_long_limb_0           @ 489  (u64 8)
//   adl_k_long_limb_1           @ 497  (u64 8)
//   adl_k_long_limb_2           @ 505  (u64 8)
//   adl_k_long_limb_3           @ 513  (u64 8)
//   adl_k_short_limb_0          @ 521  (u64 8)
//   adl_k_short_limb_1          @ 529  (u64 8)
//   adl_k_short_limb_2          @ 537  (u64 8)
//   adl_k_short_limb_3          @ 545  (u64 8)
//   adl_epoch_long              @ 553  (u64 8)
//   adl_epoch_short             @ 561  (u64 8)
//   cumulative_funding_e9_limb_0@ 569  (u64 8)
//   cumulative_funding_e9_limb_1@ 577  (u64 8)
//   cumulative_funding_e9_limb_2@ 585  (u64 8)
//   cumulative_funding_e9_limb_3@ 593  (u64 8)
//   open_account_count          @ 601  (u16 2)
//   account_bitmap_hi           @ 603  (u128 16)
//   account_bitmap_lo           @ 619  (u128 16)
//   last_funding_update_slot    @ 635  (u64 8)
//   pending_cranks              @ 643  (u16 2)
//   (total size 645)
account RiskEngine {
    admin: pubkey;
    matcher_program: pubkey;
    oracle: pubkey;

    vault: u128;
    insurance_fund: u128;

    maintenance_margin_bps: u64;
    initial_margin_bps: u64;
    trading_fee_bps: u64;
    max_accounts: u64;
    new_account_fee: u128;
    max_crank_staleness_slots: u64;
    liquidation_fee_bps: u64;
    liquidation_fee_cap: u128;
    min_liquidation_abs: u128;
    min_initial_deposit: u128;
    min_nonzero_mm_req: u128;
    min_nonzero_im_req: u128;
    insurance_floor: u128;
    h_min: u64;
    h_max: u64;
    resolve_price_deviation_bps: u64;

    current_slot: u64;
    current_oracle_price: u64;
    current_funding_rate_e9: u128;
    market_mode: u8;

    resolved_price: u64;
    resolved_slot: u64;
    resolved_h_num: u128;
    resolved_h_den: u128;

    c_tot: u128;
    h_num: u128;

    adl_a_long_limb_0: u64;
    adl_a_long_limb_1: u64;
    adl_a_long_limb_2: u64;
    adl_a_long_limb_3: u64;
    adl_a_short_limb_0: u64;
    adl_a_short_limb_1: u64;
    adl_a_short_limb_2: u64;
    adl_a_short_limb_3: u64;
    adl_k_long_limb_0: u64;
    adl_k_long_limb_1: u64;
    adl_k_long_limb_2: u64;
    adl_k_long_limb_3: u64;
    adl_k_short_limb_0: u64;
    adl_k_short_limb_1: u64;
    adl_k_short_limb_2: u64;
    adl_k_short_limb_3: u64;
    adl_epoch_long: u64;
    adl_epoch_short: u64;

    cumulative_funding_e9_limb_0: u64;
    cumulative_funding_e9_limb_1: u64;
    cumulative_funding_e9_limb_2: u64;
    cumulative_funding_e9_limb_3: u64;

    open_account_count: u16;
    account_bitmap_hi: u128;
    account_bitmap_lo: u128;

    last_funding_update_slot: u64;
    pending_cranks: u16;
}

// =============================================================================
// Instruction handlers
// =============================================================================
// Handlers that do u128 arithmetic return a sentinel u64 and are rewritten by
// the linker. Handlers that only do pubkey / direct-assignment / comparison
// work stay as pure DSL.

// top_up_insurance_fund — increases both insurance_fund and vault.
// Source: percolator.rs:~2824 (top_up_insurance_fund).
// Linker rewrites body to: LOAD risk.insurance_fund, LOAD amount, ADD, STORE;
// LOAD risk.vault, LOAD amount, ADD, STORE; PUSH 0; RETURN_VALUE.
pub top_up_insurance_fund(
    risk: RiskEngine @mut,
    admin: account @mut @signer,
    amount: u128
) -> u64 {
    return sentinel_top_up_insurance_fund();
}

// set_owner — admin-only owner swap. Pure pubkey assignment, stays DSL.
// Source: percolator.rs:2939-2955.
pub set_owner(
    risk: RiskEngine,
    acct: MarginAccount @mut,
    admin: account @mut @signer,
    new_holder: pubkey
) {
    acct.holder = new_holder;
}

// close_account — terminal payout for a flat account.
// Source: percolator.rs:4081+ (close_account_not_atomic).
// Linker rewrites body to:
//   LOAD risk.vault, LOAD acct.capital, SUB, LOAD acct.reserved_pnl, SUB, STORE vault;
//   LOAD risk.c_tot, LOAD acct.capital, SUB, STORE c_tot;
//   LOAD risk.open_account_count, PUSH 1, SUB, STORE open_account_count;
//   PUSH 0 (u128), STORE acct.capital; STORE acct.reserved_pnl;
//   PUSH 0 (u64); RETURN_VALUE.
pub close_account(
    risk: RiskEngine @mut,
    acct: MarginAccount @mut,
    owner: account @mut @signer
) -> u64 {
    return sentinel_close_account();
}

// convert_released_pnl — moves matured positive PnL from reserved_pnl into capital.
// Source: percolator.rs:3986-4080 (convert_released_pnl_not_atomic).
pub convert_released_pnl(
    risk: RiskEngine,
    acct: MarginAccount @mut,
    owner: account @mut @signer,
    amount: u128
) -> u64 {
    return sentinel_convert_released_pnl();
}

// deposit — increases capital + vault + c_tot.
// Source: percolator.rs:2957-3030 (deposit_not_atomic).
pub deposit(
    risk: RiskEngine @mut,
    acct: MarginAccount @mut,
    owner: account @mut @signer,
    amount: u128,
    oracle_price: u64
) -> u64 {
    return sentinel_deposit();
}

// withdraw — decreases capital + vault + c_tot.
// Source: percolator.rs:3031-3118 (withdraw_not_atomic).
pub withdraw(
    risk: RiskEngine @mut,
    acct: MarginAccount @mut,
    owner: account @mut @signer,
    amount: u128,
    oracle_price: u64
) -> u64 {
    return sentinel_withdraw();
}

// =============================================================================
// Extended handlers — sentinel stubs, linker-rewritten from
// perc5ive::bytecode::handlers.
//
// Each has a DSL signature matching Percolator's instruction shape and a
// bytecode body that the linker appends; full spec-level risk checks live
// in the bytecode commentary and expand as the VM opcode set grows
// (MULDIV_REM_U256 + signed-muldiv-floor — see SESSION_STATE.md).
// =============================================================================

pub settle_account(
    risk: RiskEngine @mut,
    acct: MarginAccount @mut,
    caller: account @signer,
    oracle_price: u64,
    now_slot: u64,
    funding_rate_e9: u128
) -> u64 {
    return sentinel_settle_account();
}

pub execute_trade(
    risk: RiskEngine @mut,
    taker: MarginAccount @mut,
    maker: MarginAccount @mut,
    caller: account @signer,
    oracle_price: u64,
    size_q_signed: u128,
    exec_price: u64,
    trading_fee_override: u64
) -> u64 {
    return sentinel_execute_trade();
}

pub liquidate_at_oracle(
    risk: RiskEngine @mut,
    victim: MarginAccount @mut,
    liquidator_account: MarginAccount @mut,
    liquidator: account @signer,
    oracle_price: u64
) -> u64 {
    return sentinel_liquidate_at_oracle();
}

// `keeper_crank` runs both DSL-native field updates AND a sentinel hook for
// the future u128/i256-heavy batch-liquidation loop. Its DSL body already
// covers the non-u128 fields inline; the sentinel trip at the end yields
// bytecode-side control for anything we grow into the body later.
pub keeper_crank(
    risk: RiskEngine @mut,
    caller: account @signer,
    now_slot: u64,
    oracle_price: u64,
    funding_rate_e9_lo: u128
) -> u64 {
    risk.current_slot = now_slot;
    risk.current_oracle_price = oracle_price;
    risk.current_funding_rate_e9 = funding_rate_e9_lo;
    risk.last_funding_update_slot = now_slot;
    return sentinel_keeper_crank();
}
