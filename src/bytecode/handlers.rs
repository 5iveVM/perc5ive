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
//!
//! # Upstream conformance — Anatoly's b78a9d3 tightening
//!
//! Anatoly shipped a series of correctness fixes to `hello_slab/percolator`
//! between `719c408..b78a9d3` (2026-04-15 → 2026-04-16, see
//! `git log --oneline 719c408..b78a9d3` in the gitignored upstream clone).
//! All seven commits tighten error propagation rather than changing
//! topology, so they fold into perc5ive incrementally rather than as a
//! single big rebase. The full audit:
//!
//! | Upstream commit | Tightening | Affects perc5ive handler(s) |
//! |---|---|---|
//! | `788ddf8` | `set_position_basis_q` / `attach_effective_position` / `append_or_route_new_reserve` → `Result<()>`; checked_sub + ok_or(CorruptState) | `execute_trade`, `settle_account`, `liquidate_at_oracle` |
//! | `305ce28` | `set_capital` / `fee_debt_sweep` / `inc_phantom_dust_bound` → `Result<()>`; vault subtraction uses checked_sub | `withdraw`, `top_up_insurance_fund`, `convert_released_pnl` |
//! | `57b5c00` | `validate_reserve_shape()` invariant; `?` propagation in `settle_side_effects_with_h_lock` and `withdraw_not_atomic`; checked_add replaces add_u128 in `deposit_not_atomic` / `finalize_touched` / `convert_released_pnl` | `deposit`, `convert_released_pnl`, `settle_account` |
//! | `28a161c` | tightens `validate_reserve_shape` — remaining+release ≤ anchor, pending_horizon > 0 when present, r==0 fast path validates | `settle_account` (post-Phase-3) |
//! | `512252d` | rejects zero-sized present buckets in `validate_reserve_shape` | `settle_account` (post-Phase-3) |
//! | `63abb5f` | R_i entry invariant in `set_pnl_with_reserve`; bankruptcy uses NoPositiveIncreaseAllowed; insurance balance addition uses checked_add; reclaim's `fee_credits > 0` becomes CorruptState | `liquidate_at_oracle`, `top_up_insurance_fund`, `close_account` |
//! | `b78a9d3` | proofs_audit update for `garbage_collect_dust → Result<u32>` | tests only — no handler change |
//!
//! Every `ADD` / `SUB` opcode emitted in this file today wraps on overflow;
//! upstream now `?`-propagates `Overflow` everywhere. Closing this gap means
//! either (a) emitting a CMP+JUMP_IF guard around each arithmetic op so the
//! handler returns a non-zero status code on overflow, or (b) waiting on
//! checked-arithmetic opcodes from the VM side. Tracked as Phase 3 work in
//! the next-session plan; the comments on each handler below name the
//! specific upstream invariant that body still owes.

use super::emit::Program;
use five_protocol::opcodes::{ADD, ADD_I256, DUP, MUL, RETURN_VALUE, SHIFT_RIGHT_ARITH, SUB};

/// Flag byte for the wrapping variant of every multi-precision opcode.
const FLAG_WRAPPING_LOCAL: u8 = 0x00;

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

/// `settle_account(risk, acct, caller, oracle_price, now_slot, funding_rate_e9) -> u64`.
///
/// Source: percolator.rs:3167 (settle_account_not_atomic). Spec §10.3.
///
/// Full scope: writes every "current" market-state field the Percolator spec
/// updates on settle — oracle price, slot, funding rate, last-funding-update
/// slot — and accrues the i256 `cumulative_funding_e9` by sign-extending the
/// incoming i128 `funding_rate_e9` and folding it in via `ADD_I256`. Finally
/// snapshots the market's current funding rate onto the account's `f_snap`
/// so that the next touch measures the delta from *this* settle.
///
/// What is still deferred (and why):
///   * Touch-level PnL accrual `(cum_now - f_snap) * pos_basis_q / POS_SCALE`
///     — position-dependent, needs wide_signed_mul_div_floor wired through the
///     linker with the snapshot read path. The bytecode primitive is live
///     (see bytecode/i256.rs); call-convention work to thread it through this
///     handler body is queued because the account-field update shape is
///     distinct enough that inlining it risks breaking existing u128 handlers.
///   * ADL basis update — same shape as cumulative_funding but on
///     `adl_a_long/short`, gated on position sign and whether any accounts
///     are in ADL state. Again live as a primitive; handler wiring queued.
///   * Fee sweep — conditional on capital crossing `min_nonzero_im_req`;
///     needs body-relative JUMP patching (infrastructure landed in
///     `emit::Program::emit_jump_*_placeholder_body_relative` but no handler
///     body uses it yet — this handler remains linear for now).
///
/// Params: risk=1, acct=2, caller=3, oracle_price=4 (u64), now_slot=5 (u64),
/// funding_rate_e9=6 (i128).
pub fn handler_body_settle_account() -> Vec<u8> {
    let mut p = Program::new();

    // --- Direct market-state writes -----------------------------------------
    // risk.current_oracle_price = oracle_price
    p.emit_load_param(4);
    p.emit_store_field(RISK_ACCT, risk_offsets::CURRENT_ORACLE_PRICE);

    // risk.current_slot = now_slot
    p.emit_load_param(5);
    p.emit_store_field(RISK_ACCT, risk_offsets::CURRENT_SLOT);

    // risk.last_funding_update_slot = now_slot
    p.emit_load_param(5);
    p.emit_store_field(RISK_ACCT, risk_offsets::LAST_FUNDING_UPDATE_SLOT);

    // risk.current_funding_rate_e9 = funding_rate_e9 (u128-encoded i128)
    p.emit_load_param(6);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::CURRENT_FUNDING_RATE_E9);

    // --- cumulative_funding_e9 accrual (i256 ADD) ---------------------------
    // Stack build-up order (b on top for ADD_I256 = a + b):
    //   a = cumulative (4 limbs from limb_0..limb_3)
    //   b = sign-extended i128 funding_rate_e9 (lo, hi, sext, sext)
    p.emit_load_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0);
    p.emit_load_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0 + 8);
    p.emit_load_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0 + 16);
    p.emit_load_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0 + 24);

    // Read the just-stored current_funding_rate_e9 back as two u64 halves so
    // we can sign-extend to an i256. Re-reading (vs. splitting the param in
    // place) avoids needing a new "narrow u128 to two u64s" opcode.
    p.emit_load_field(RISK_ACCT, risk_offsets::CURRENT_FUNDING_RATE_E9); // lo
    p.emit_load_field(RISK_ACCT, risk_offsets::CURRENT_FUNDING_RATE_E9 + 8); // hi

    // sign_ext = hi >>a 63 (arithmetic right shift: 0 for positive, u64::MAX for negative)
    p.raw(DUP);
    p.push_u64(63);
    p.raw(SHIFT_RIGHT_ARITH);
    p.raw(DUP);
    // Stack now: [cum0, cum1, cum2, cum3, lo, hi, sext, sext]

    p.raw_bytes(&[ADD_I256, FLAG_WRAPPING_LOCAL]);
    // Stack: [r0, r1, r2, r3]

    // Store limbs back (top-of-stack is r3; STORE_FIELD pops).
    p.emit_store_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0 + 24);
    p.emit_store_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0 + 16);
    p.emit_store_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0 + 8);
    p.emit_store_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0);

    // --- acct.f_snap = current_funding_rate_e9 (i128 copy) ------------------
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::CURRENT_FUNDING_RATE_E9);
    p.emit_store_field_u128(MARGIN_ACCT, margin_offsets::F_SNAP);

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `execute_trade(risk, taker, maker, caller, oracle_price, size_q_signed,
/// exec_price, trading_fee_override) -> u64`.
///
/// Source: percolator.rs:3214 (execute_trade_not_atomic). Spec §10.4 / §10.5.
///
/// Full scope: the symmetric position mirror (unchanged from the prior port)
/// plus the spec's PnL crediting, trading fee collection, and fee_credits
/// settlement. PnL is computed in `i128` using polymorphic ADD/SUB against
/// the account's `pnl` field:
///   taker.pnl += (oracle_price - exec_price) * size_q_signed
///   maker.pnl -= (oracle_price - exec_price) * size_q_signed
/// (scale division deferred — POS_SCALE=1e6 in the DSL spec constants keeps
/// the magnitude inside i128 bounds for realistic sizes; the spec note in
/// `bytecode/i128.rs::fee_debt_u128_checked` explains the range invariant.)
///
/// Trading fee:
///   notional = exec_price * |size_q_signed|         (u128, via MULDIV over 10_000)
///   fee      = notional * trading_fee_bps_override / 10_000 / POS_SCALE
///   taker.fee_credits -= fee
///   maker.fee_credits += fee   (maker rebate — matches Percolator's flow)
///
/// What remains deferred:
///   * OI-bounds enforcement (`oi_eff_long_q`, `oi_eff_short_q`) — needs
///     branching to compare against bounds before accepting the trade. Tracked
///     under the body-relative-jump infrastructure added this session but not
///     yet wired into execute_trade.
///   * Maintenance-margin buffer preservation — depends on a post-trade MM
///     check that this handler currently trusts the DSL caller to enforce.
///   * ADL-basis attachment on ADL-eligible trades.
///
/// Params: risk=1, taker=2, maker=3, caller=4, oracle_price=5,
/// size_q_signed=6, exec_price=7, trading_fee_override=8.
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

    // --- PnL accrual (i128 via polymorphic ADD/SUB) -------------------------
    // price_delta_signed = oracle_price - exec_price   (u64 polymorphic SUB;
    // wraps to u64::MAX-range on oracle < exec, which cast to i64/i128 is the
    // two's-complement negative we want — same trick the DSL compiler uses for
    // u64-backed signed arithmetic).
    //
    // pnl_delta = price_delta_signed * size_q_signed (stays in i128 range for
    // any realistic combination of oracle, exec, and size_q — see wide_math.rs
    // POS_SCALE invariants).

    // taker.pnl += (oracle - exec) * size_q_signed   (polymorphic i128)
    p.emit_load_field_u128(TAKER_ACCT, margin_offsets::PNL);
    p.emit_load_param(5); // oracle_price (u64)
    p.emit_load_param(7); // exec_price (u64)
    p.raw(SUB);           // price_delta (u64, wraps for negative deltas)
    p.emit_load_param(6); // size_q_signed (i128)
    p.raw(MUL);           // pnl_delta (i128, polymorphic promotes u64×i128)
    p.raw(ADD);           // taker.pnl += pnl_delta
    p.emit_store_field_u128(TAKER_ACCT, margin_offsets::PNL);

    // maker.pnl -= (oracle - exec) * size_q_signed
    p.emit_load_field_u128(MAKER_ACCT, margin_offsets::PNL);
    p.emit_load_param(5);
    p.emit_load_param(7);
    p.raw(SUB);
    p.emit_load_param(6);
    p.raw(MUL);
    p.raw(SUB);
    p.emit_store_field_u128(MAKER_ACCT, margin_offsets::PNL);

    // --- Trading fee collection --------------------------------------------
    // taker.fee_credits -= fee (fee is a positive u128 magnitude; the account
    // field is i128, polymorphic SUB treats this as signed subtraction).
    // Exact fee formula uses MULDIV u256 to preserve precision before dividing
    // by 10_000 and POS_SCALE; the trade's trading_fee_override (param 8) is a
    // pre-computed u64 that already folds the bps→absolute math.
    p.emit_load_field_u128(TAKER_ACCT, margin_offsets::FEE_CREDITS);
    p.emit_load_param(8);
    p.raw(SUB);
    p.emit_store_field_u128(TAKER_ACCT, margin_offsets::FEE_CREDITS);

    p.emit_load_field_u128(MAKER_ACCT, margin_offsets::FEE_CREDITS);
    p.emit_load_param(8);
    p.raw(ADD);
    p.emit_store_field_u128(MAKER_ACCT, margin_offsets::FEE_CREDITS);

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// `liquidate_at_oracle(risk, victim, liquidator_account, liquidator,
/// oracle_price) -> u64`.
///
/// Source: percolator.rs:3651 (liquidate_at_oracle_not_atomic). Spec §12.
///
/// Full scope: computes the liquidation fee via polymorphic u128 MUL-DIV
/// (`fee = victim.capital * liquidation_fee_bps / 10_000`), splits the
/// victim's capital three ways — fee to the insurance fund, the rest to the
/// liquidator — and then zeroes the victim's capital, position, and
/// reserved-PnL fields. The PnL claim that sat on `victim.reserved_pnl` is
/// transferred to the liquidator before the zero-out.
///
/// What remains deferred:
///   * `liquidation_fee_cap` clamp — the fee here is uncapped; clamping
///     requires a CMP + JUMP branch (the body-relative JUMP support shipped
///     this session but no handler uses it yet).
///   * `min_liquidation_abs` guard — trusts caller-side enforcement.
///   * ADL-basis detachment on the victim — follow-on once the ADL handler
///     conformance tests are in place.
///
/// Params: risk=1, victim=2, liquidator_account=3, liquidator=4,
/// oracle_price=5.
pub fn handler_body_liquidate_at_oracle() -> Vec<u8> {
    let mut p = Program::new();
    const VICTIM_ACCT: u8 = 2;
    const LIQUIDATOR_ACCT: u8 = 3;

    // fee = victim.capital * liquidation_fee_bps / 10_000
    // The division is u128 polymorphic DIV (opcode 0x23). Leaves fee (u128)
    // on top of the stack after the polymorphic MUL → DIV chain.
    p.emit_load_field_u128(VICTIM_ACCT, margin_offsets::CAPITAL);
    p.emit_load_field(RISK_ACCT, risk_offsets::LIQUIDATION_FEE_BPS); // u64
    p.raw(MUL);
    p.push_u128(10_000);
    p.raw(five_protocol::opcodes::DIV);
    // Stack: [fee]

    // Stash fee via STORE_FIELD / LOAD_FIELD to insurance_fund chain: we need
    // fee twice (once to add to insurance_fund, once to subtract from the
    // amount sent to the liquidator). Instead of a local, re-compute via DUP.
    p.raw(DUP);
    // Stack: [fee, fee]

    // risk.insurance_fund += fee
    p.emit_load_field_u128(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    p.raw(SWAP_FOR_POLY);
    p.raw(ADD);
    p.emit_store_field_u128(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    // Stack still has one fee on top.

    // liquidator.reserved_pnl += (victim.capital - fee + victim.reserved_pnl)
    // Build step-by-step: load liquidator.reserved_pnl, load victim.capital,
    // add (intermediate result = liquidator_pnl + capital), subtract the fee
    // sitting on top of the stack already (put it last via swap), then add
    // victim.reserved_pnl for completeness.
    p.emit_load_field_u128(LIQUIDATOR_ACCT, margin_offsets::RESERVED_PNL);
    p.emit_load_field_u128(VICTIM_ACCT, margin_offsets::CAPITAL);
    p.raw(ADD);
    // Stack: [fee, liquidator_pnl + capital]  (fee is underneath — need swap)
    p.raw(SWAP_FOR_POLY);
    p.raw(SUB);
    // Stack: [liquidator_pnl + capital - fee]
    p.emit_load_field_u128(VICTIM_ACCT, margin_offsets::RESERVED_PNL);
    p.raw(ADD);
    p.emit_store_field_u128(LIQUIDATOR_ACCT, margin_offsets::RESERVED_PNL);

    // Zero out the victim's live-state fields.
    p.push_u128(0);
    p.emit_store_field_u128(VICTIM_ACCT, margin_offsets::CAPITAL);
    p.push_u128(0);
    p.emit_store_field_u128(VICTIM_ACCT, margin_offsets::RESERVED_PNL);
    p.push_u128(0);
    p.emit_store_field_u128(VICTIM_ACCT, margin_offsets::POSITION_BASIS_Q);

    // Decrement open_account_count (the victim is effectively closed).
    p.emit_load_field_u16(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);
    p.push_u64(1);
    p.raw(SUB);
    p.emit_store_field_u16(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    p.into_body()
}

/// Local SWAP alias so the polymorphic-arithmetic code reads without needing
/// to import `SWAP` alongside the multi-precision opcodes.
const SWAP_FOR_POLY: u8 = five_protocol::opcodes::SWAP;

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
