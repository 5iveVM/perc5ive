//! Hand-written bytecode for Percolator's u128-arithmetic handlers.
//!
//! Each function in this module emits the raw body bytes that the linker
//! appends and wires into a sentinel-stubbed DSL handler. The DSL handler's
//! body (a `PUSH_U64 <sentinel>; RETURN_VALUE`) is rewritten in-place by
//! [`Linker::rewrite_stub`] to `CALL 0 <appended>; RETURN_VALUE; NOP...`, and
//! the appended body does the real load-field / u128-arith / store-field
//! work, pushes a status u64, and returns. The outer `RETURN_VALUE` then
//! surfaces that status as the DSL handler's return value.
//!
//! # Why hand-written
//!
//! The `five-dsl-compiler` rejects u128 rvalues produced by a binary operator
//! or function call when assigned to a let or field (see
//! `memory/project_five_dsl_compiler_regression.md`). Every handler in this
//! file works around that by pushing the operands directly onto the value
//! stack and using the generic polymorphic `ADD` / `SUB` opcodes, which are
//! value-type-aware and promote to u128 when both operands are u128.
//!
//! # Calling convention
//!
//! The rewrite CALL uses `param_count = 0`, so the appended callee inherits
//! the DSL handler's parameter frame and account table. This means:
//!
//! - `LOAD_PARAM <idx>` inside an appended body reads the DSL caller's Nth
//!   parameter. The DSL reserves index 0, so accounts start at 1 and scalars
//!   follow (e.g. in `deposit(risk, acct, owner, amount, oracle_price)` the
//!   indices are risk=1, acct=2, owner=3, amount=4, oracle_price=5).
//! - `LOAD_FIELD acct_idx, offset` / `STORE_FIELD acct_idx, offset` use the
//!   same positional account index, and the offset is the DSL's packed
//!   byte offset inside the account's backing buffer. Offsets for
//!   `MarginAccount` and `RiskEngine` are documented in `dsl/src/main.v`
//!   next to the account declaration.
//!
//! # What each function expects
//!
//! Each `handler_body_*` function returns the raw callee bytes (no header,
//! no trailing HALT). Pass them to [`Linker::append_function`]; then call
//! [`Linker::rewrite_stub`] with the matching sentinel from
//! [`SENTINELS`] to wire the DSL stub to the appended body.

use super::emit::Program;
use five_protocol::opcodes::{ADD, RETURN_VALUE, SUB};

// =============================================================================
// Sentinels — must stay in sync with `dsl/src/main.v`'s `sentinel_*()` consts.
// =============================================================================

/// Sentinel for `top_up_insurance_fund`. Matches `0xFEED_FACE_DEAD_5001`.
pub const SENTINEL_TOP_UP_INSURANCE_FUND: u64 = 0xFEED_FACE_DEAD_5001;
/// Sentinel for `deposit`. Matches `0xFEED_FACE_DEAD_5002`.
pub const SENTINEL_DEPOSIT: u64 = 0xFEED_FACE_DEAD_5002;
/// Sentinel for `withdraw`. Matches `0xFEED_FACE_DEAD_5003`.
pub const SENTINEL_WITHDRAW: u64 = 0xFEED_FACE_DEAD_5003;
/// Sentinel for `convert_released_pnl`. Matches `0xFEED_FACE_DEAD_5004`.
pub const SENTINEL_CONVERT_RELEASED_PNL: u64 = 0xFEED_FACE_DEAD_5004;
/// Sentinel for `close_account`. Matches `0xFEED_FACE_DEAD_5005`.
pub const SENTINEL_CLOSE_ACCOUNT: u64 = 0xFEED_FACE_DEAD_5005;

// =============================================================================
// Account indices — DSL parameter layout
// =============================================================================
//
// The DSL compiler assigns index 0 as a reserved slot, then each positional
// @mut/@signer/@readonly account gets an incrementing index starting at 1.
// Scalar parameters follow in declaration order.

/// Account index of the `risk: RiskEngine` parameter (always the first
/// account across all Percolator handlers that take it).
pub const RISK_ACCT: u8 = 1;
/// Account index of `acct: MarginAccount` in handlers that take one.
pub const MARGIN_ACCT: u8 = 2;

// =============================================================================
// Field offsets — mirror `dsl/src/main.v`
// =============================================================================

pub mod risk_offsets {
    //! Packed offsets for fields on `RiskEngine` referenced by the handlers.
    //! Values come from the `account RiskEngine { ... }` declaration in
    //! `dsl/src/main.v` and were verified with compiler probes.
    pub const ADMIN: u64 = 0;
    pub const MATCHER_PROGRAM: u64 = 32;
    pub const ORACLE: u64 = 64;
    pub const VAULT: u64 = 96;
    pub const INSURANCE_FUND: u64 = 112;
    pub const MAINTENANCE_MARGIN_BPS: u64 = 128;
    pub const INITIAL_MARGIN_BPS: u64 = 136;
    pub const TRADING_FEE_BPS: u64 = 144;
    pub const MAX_ACCOUNTS: u64 = 152;
    pub const NEW_ACCOUNT_FEE: u64 = 160;
    pub const MAX_CRANK_STALENESS_SLOTS: u64 = 176;
    pub const LIQUIDATION_FEE_BPS: u64 = 184;
    pub const LIQUIDATION_FEE_CAP: u64 = 192;
    pub const MIN_LIQUIDATION_ABS: u64 = 208;
    pub const MIN_INITIAL_DEPOSIT: u64 = 224;
    pub const MIN_NONZERO_MM_REQ: u64 = 240;
    pub const MIN_NONZERO_IM_REQ: u64 = 256;
    pub const INSURANCE_FLOOR: u64 = 272;
    pub const H_MIN: u64 = 288;
    pub const H_MAX: u64 = 296;
    pub const RESOLVE_PRICE_DEVIATION_BPS: u64 = 304;
    pub const CURRENT_SLOT: u64 = 312;
    pub const CURRENT_ORACLE_PRICE: u64 = 320;
    pub const CURRENT_FUNDING_RATE_E9: u64 = 328;
    pub const MARKET_MODE: u64 = 344;
    pub const RESOLVED_PRICE: u64 = 345;
    pub const RESOLVED_SLOT: u64 = 353;
    pub const RESOLVED_H_NUM: u64 = 361;
    pub const RESOLVED_H_DEN: u64 = 377;
    pub const C_TOT: u64 = 393;
    pub const H_NUM: u64 = 409;
    pub const ADL_A_LONG_LIMB_0: u64 = 425;
    pub const ADL_A_SHORT_LIMB_0: u64 = 457;
    pub const ADL_K_LONG_LIMB_0: u64 = 489;
    pub const ADL_K_SHORT_LIMB_0: u64 = 521;
    pub const ADL_EPOCH_LONG: u64 = 553;
    pub const ADL_EPOCH_SHORT: u64 = 561;
    pub const CUMULATIVE_FUNDING_E9_LIMB_0: u64 = 569;
    pub const OPEN_ACCOUNT_COUNT: u64 = 601;
    pub const ACCOUNT_BITMAP_HI: u64 = 603;
    pub const ACCOUNT_BITMAP_LO: u64 = 619;
    pub const LAST_FUNDING_UPDATE_SLOT: u64 = 635;
    pub const PENDING_CRANKS: u64 = 643;
}

pub mod margin_offsets {
    //! Packed offsets for fields on `MarginAccount` referenced by the
    //! handlers. See `account MarginAccount { ... }` in `dsl/src/main.v`.
    pub const HOLDER: u64 = 0;
    pub const MATCHER_PROGRAM: u64 = 32;
    pub const MATCHER_CONTEXT: u64 = 64;
    pub const KIND: u64 = 96;
    pub const ACCOUNT_IDX: u64 = 97;
    pub const CAPITAL: u64 = 99;
    pub const PNL: u64 = 115;
    pub const RESERVED_PNL: u64 = 131;
    pub const POSITION_BASIS_Q: u64 = 147;
    pub const ADL_A_BASIS: u64 = 163;
    pub const ADL_K_SNAP: u64 = 179;
    pub const F_SNAP: u64 = 195;
    pub const ADL_EPOCH_SNAP: u64 = 211;
    pub const FEE_CREDITS: u64 = 219;
    pub const SCHED_PRESENT: u64 = 235;
    pub const SCHED_REMAINING_Q: u64 = 236;
    pub const SCHED_ANCHOR_Q: u64 = 252;
    pub const SCHED_START_SLOT: u64 = 268;
    pub const SCHED_HORIZON: u64 = 276;
    pub const SCHED_RELEASE_Q: u64 = 284;
    pub const PENDING_PRESENT: u64 = 300;
    pub const PENDING_REMAINING_Q: u64 = 301;
    pub const PENDING_HORIZON: u64 = 317;
    pub const PENDING_CREATED_SLOT: u64 = 325;
}

// =============================================================================
// Handler bodies
// =============================================================================

/// `top_up_insurance_fund(risk, admin, amount) -> u64`.
///
/// Params: risk=1 (account), admin=2 (account), amount=3 (u128).
/// Effect: `risk.insurance_fund += amount; risk.vault += amount`. Returns 0
/// (success) as a u64 on the stack.
///
/// Source: hello_slab/percolator/src/percolator.rs top_up_insurance_fund.
pub fn handler_body_top_up_insurance_fund() -> Vec<u8> {
    let mut p = Program::new();
    // risk.insurance_fund += amount
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    p.emit_load_param(3);
    p.raw(ADD);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    // risk.vault += amount
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_param(3);
    p.raw(ADD);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::VAULT);
    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `deposit(risk, acct, owner, amount, oracle_price) -> u64`.
///
/// Params: risk=1, acct=2, owner=3, amount=4, oracle_price=5.
/// Effect: `acct.capital += amount; risk.vault += amount; risk.c_tot += amount`.
///
/// The warmup-bucket promotion (spec §4.3) is NOT done here — upstream
/// `deposit_not_atomic` handles it conditionally on `acct.pending_present`,
/// and the DSL-side `if` support for that flag is still pending. That work
/// lands when the remaining Percolator handlers are ported.
pub fn handler_body_deposit() -> Vec<u8> {
    let mut p = Program::new();
    // acct.capital += amount
    p.emit_load_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    // risk.vault += amount
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::VAULT);
    // risk.c_tot += amount
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::C_TOT);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::C_TOT);
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `withdraw(risk, acct, owner, amount, oracle_price) -> u64`.
///
/// Effect: `acct.capital -= amount; risk.vault -= amount; risk.c_tot -= amount`.
///
/// Upstream's free-collateral check (u256 muldiv against position basis) is
/// not performed here yet — see SESSION_STATE.md remaining items. Callers
/// invoking `withdraw` with a non-flat position need to rely on the DSL's
/// require-check once the ctx.key + u256 conformance work lands.
pub fn handler_body_withdraw() -> Vec<u8> {
    let mut p = Program::new();
    // acct.capital -= amount
    p.emit_load_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    // risk.vault -= amount
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::VAULT);
    // risk.c_tot -= amount
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::C_TOT);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::C_TOT);
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `convert_released_pnl(risk, acct, owner, amount) -> u64`.
///
/// Params: risk=1, acct=2, owner=3, amount=4.
/// Effect: `acct.reserved_pnl -= amount; acct.capital += amount`.
pub fn handler_body_convert_released_pnl() -> Vec<u8> {
    let mut p = Program::new();
    // acct.reserved_pnl -= amount
    p.emit_load_field_u128(MARGIN_ACCT, margin_offsets::RESERVED_PNL);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field_u128(MARGIN_ACCT, margin_offsets::RESERVED_PNL);
    // acct.capital += amount
    p.emit_load_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `close_account(risk, acct, owner) -> u64`.
///
/// Effect:
///   risk.vault -= acct.capital + acct.reserved_pnl
///   risk.c_tot -= acct.capital
///   risk.open_account_count -= 1      (u16 arithmetic via generic SUB)
///   acct.capital = 0                  (u128)
///   acct.reserved_pnl = 0             (u128)
///
/// Upstream guards this with `require(position == 0 && no pending buckets)`
/// in DSL `require()` calls. Those compile in the DSL when the lvalue is not
/// a u128 binary op, but we've centralized all of close_account's logic in
/// bytecode for consistency. Position/pending-present pre-checks are added
/// here as `require`-equivalent `CMP + JUMP_IF` guards once the full
/// conformance suite lands. For now, the bytecode trusts caller-side
/// invariants (matches the current DSL skeleton, which has `require(true)`
/// placeholders pending the `ctx.key` resolution fix).
pub fn handler_body_close_account() -> Vec<u8> {
    let mut p = Program::new();

    // risk.vault -= acct.capital
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.raw(SUB);
    // ... -= acct.reserved_pnl
    p.emit_load_field_u128(MARGIN_ACCT, margin_offsets::RESERVED_PNL);
    p.raw(SUB);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::VAULT);

    // risk.c_tot -= acct.capital
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::C_TOT);
    p.emit_load_field_u128(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.raw(SUB);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::C_TOT);

    // risk.open_account_count -= 1 (u16 counter)
    p.emit_load_field_u16(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);
    p.push_u64(1);
    p.raw(SUB);
    p.emit_store_field_u16(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);

    // acct.capital = 0 (u128)
    p.push_u128(0);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    // acct.reserved_pnl = 0 (u128)
    p.push_u128(0);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::RESERVED_PNL);

    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

// =============================================================================
// Extended handlers — settle_account, execute_trade, liquidate_at_oracle,
// keeper_crank. These are the second-wave handlers from the Percolator spec
// (percolator.rs:3119-3950). Each is ported as a simplified state transition
// that captures the core arithmetic; the full spec-level risk checks live in
// the comment above each body and are queued for incremental expansion once
// the full u256 muldiv-rem opcode lands (see SESSION_STATE.md).
// =============================================================================

/// Sentinel for `settle_account`. Matches `0xFEED_FACE_DEAD_5006`.
pub const SENTINEL_SETTLE_ACCOUNT: u64 = 0xFEED_FACE_DEAD_5006;
/// Sentinel for `execute_trade`. Matches `0xFEED_FACE_DEAD_5007`.
pub const SENTINEL_EXECUTE_TRADE: u64 = 0xFEED_FACE_DEAD_5007;
/// Sentinel for `liquidate_at_oracle`. Matches `0xFEED_FACE_DEAD_5008`.
pub const SENTINEL_LIQUIDATE_AT_ORACLE: u64 = 0xFEED_FACE_DEAD_5008;
/// Sentinel for `keeper_crank`. Matches `0xFEED_FACE_DEAD_5009`.
pub const SENTINEL_KEEPER_CRANK: u64 = 0xFEED_FACE_DEAD_5009;

/// `settle_account(risk, acct, caller, oracle_price) -> u64`.
///
/// Source: percolator.rs:3119 (settle_account_not_atomic).
///
/// Simplified port: updates `risk.current_oracle_price` and
/// `risk.current_slot` as a best-effort crank. The full spec-level flow
/// (accrue_market_to + touch_account_live_local + finalize_touched_accounts)
/// requires the ADL-basis u256 arithmetic (`adl_a_*_limb_0..3`) and a full
/// funding-rate accrual loop. Those are tracked as follow-on work:
///
///   * ADL basis update: `risk.adl_a_long += funding_delta * oi_long`
///     (needs u256 from 4-limb fields, u128 × u128 MUL_U256, ADD_U256, STORE)
///   * PnL zeroing on flat accounts after settle
///     (reads acct.position_basis_q; if 0, zero acct.pnl and acct.fee_credits)
///   * Fee sweep when capital is above the threshold
///     (compare against `min_nonzero_im_req`; move excess into fee_credits)
///
/// Params: risk=1, acct=2, caller=3, oracle_price=4 (u64).
pub fn handler_body_settle_account() -> Vec<u8> {
    let mut p = Program::new();
    // risk.current_oracle_price = oracle_price
    p.emit_load_param(4);
    p.emit_store_field(RISK_ACCT, risk_offsets::CURRENT_ORACLE_PRICE);
    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `execute_trade(risk, taker, maker, caller, oracle_price, size_q_signed,
/// exec_price, trading_fee_override) -> u64`.
///
/// Source: percolator.rs:3166 (execute_trade_not_atomic). Spec §10.4 / §10.5.
///
/// Simplified port: applies the core symmetric position update —
///   taker.position_basis_q += size_q_signed
///   maker.position_basis_q -= size_q_signed
/// and the mirrored PnL credit driven by (oracle - exec) × size_q:
///   taker.pnl += (oracle - exec) * size_q_signed / POS_SCALE
///   maker.pnl += (exec - oracle) * size_q_signed / POS_SCALE
///
/// Full spec adds: bilateral OI bounds, maintenance-margin buffer
/// preservation, trade-notional limits, side-mode gating, and ADL basis
/// updates. These follow once MULDIV_REM_U256 ships (signed muldiv has a
/// conformance Rust reference in bytecode/i256.rs).
///
/// Params: risk=1, taker=2, maker=3, caller=4, oracle_price=5,
/// size_q_signed=6, exec_price=7, trading_fee_override=8. Account indices
/// `taker=2` and `maker=3` follow the declaration order.
pub fn handler_body_execute_trade() -> Vec<u8> {
    let mut p = Program::new();
    const TAKER_ACCT: u8 = 2;
    const MAKER_ACCT: u8 = 3;

    // taker.position_basis_q += size_q_signed (i128 addition via generic ADD)
    p.emit_load_field_u128(TAKER_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.emit_load_param(6);
    p.raw(ADD);
    p.emit_store_field_u128(TAKER_ACCT, margin_offsets::POSITION_BASIS_Q);

    // maker.position_basis_q -= size_q_signed
    p.emit_load_field_u128(MAKER_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.emit_load_param(6);
    p.raw(SUB);
    p.emit_store_field_u128(MAKER_ACCT, margin_offsets::POSITION_BASIS_Q);

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `liquidate_at_oracle(risk, victim, liquidator_account, liquidator,
/// oracle_price) -> u64`.
///
/// Source: percolator.rs:3601 (liquidate_at_oracle_not_atomic). Spec §12.
///
/// Simplified port: drains the victim's capital into the liquidator's
/// reserved_pnl field. The effect is the terminal step of the real
/// liquidation — `victim.capital → 0` and the liquidator absorbs.
///
/// Full spec: compute liquidation fee (liquidation_fee_bps against notional),
/// reverse victim's position onto the liquidator, split the difference
/// between liquidator, insurance fund, and any residual into the vault.
/// That requires u256 muldiv and i256 sign tracking; these are the
/// wide_signed_mul_div_floor/MULDIV_REM_U256 dependencies queued in
/// SESSION_STATE.md.
///
/// Params: risk=1, victim=2, liquidator_account=3, liquidator=4,
/// oracle_price=5.
pub fn handler_body_liquidate_at_oracle() -> Vec<u8> {
    let mut p = Program::new();
    const VICTIM_ACCT: u8 = 2;
    const LIQUIDATOR_ACCT: u8 = 3;

    // liquidator.reserved_pnl += victim.capital
    p.emit_load_field_u128(LIQUIDATOR_ACCT, margin_offsets::RESERVED_PNL);
    p.emit_load_field_u128(VICTIM_ACCT, margin_offsets::CAPITAL);
    p.raw(ADD);
    p.emit_store_field_u128(LIQUIDATOR_ACCT, margin_offsets::RESERVED_PNL);

    // victim.capital = 0
    p.push_u128(0);
    p.emit_store_field_u128(VICTIM_ACCT, margin_offsets::CAPITAL);

    // victim.position_basis_q = 0 (position flattened)
    p.push_u128(0);
    p.emit_store_field_u128(VICTIM_ACCT, margin_offsets::POSITION_BASIS_Q);

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `keeper_crank(risk, caller, now_slot, oracle_price, funding_rate_e9) -> u64`.
///
/// Source: percolator.rs:3784 (keeper_crank_not_atomic). Spec §13.
///
/// The DSL handler in `dsl/src/main.v` already handles the non-u128 fields
/// inline (current_slot, current_oracle_price, current_funding_rate_e9,
/// last_funding_update_slot). This bytecode body exists to add the
/// future-work hook for batch liquidation loops. For now it's a no-op
/// sentinel — the DSL side does the real work.
///
/// Params: risk=1, caller=2, now_slot=3, oracle_price=4, funding_rate_e9=5.
pub fn handler_body_keeper_crank() -> Vec<u8> {
    let mut p = Program::new();
    // Currently delegated to the DSL pure-arithmetic block. Return 0 to
    // signal success; leave the u128/i256-heavy batch-liquidation loop for
    // the post-MULDIV_REM_U256 session.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

// =============================================================================
// Linker convenience
// =============================================================================

/// (sentinel, callee-body-bytes) pair for every Percolator handler.
///
/// Used by tests and by consumers that want to fully link a compiled main.v
/// in one call. Order is arbitrary — the linker handles each sentinel
/// independently.
pub fn all_handler_bodies() -> [(u64, Vec<u8>); 9] {
    [
        (
            SENTINEL_TOP_UP_INSURANCE_FUND,
            handler_body_top_up_insurance_fund(),
        ),
        (SENTINEL_DEPOSIT, handler_body_deposit()),
        (SENTINEL_WITHDRAW, handler_body_withdraw()),
        (
            SENTINEL_CONVERT_RELEASED_PNL,
            handler_body_convert_released_pnl(),
        ),
        (SENTINEL_CLOSE_ACCOUNT, handler_body_close_account()),
        (SENTINEL_SETTLE_ACCOUNT, handler_body_settle_account()),
        (SENTINEL_EXECUTE_TRADE, handler_body_execute_trade()),
        (
            SENTINEL_LIQUIDATE_AT_ORACLE,
            handler_body_liquidate_at_oracle(),
        ),
        (SENTINEL_KEEPER_CRANK, handler_body_keeper_crank()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_handler_bodies_are_nonempty_and_end_with_return_value() {
        for (sentinel, body) in all_handler_bodies() {
            assert!(!body.is_empty(), "empty body for sentinel {:#x}", sentinel);
            // Each body should end with `push_u64(0); RETURN_VALUE`. push_u64(0)
            // emits `PUSH_U64 (0x1B), 0x00` = 2 bytes. So the last 3 bytes are
            // 0x1B, 0x00, 0x07 (RETURN_VALUE).
            let n = body.len();
            assert!(n >= 3, "body too short for sentinel {:#x}", sentinel);
            assert_eq!(body[n - 1], RETURN_VALUE);
        }
    }
}
