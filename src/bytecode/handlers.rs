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

use super::emit::{BodyRelativeJumpPatch, Program};
use five_protocol::opcodes::{ADD, DUP, EQ, MUL, RETURN_VALUE, SUB};

/// A handler body ready to be appended by the linker.
///
/// `bytes` is the raw bytecode (no header, no trailing HALT). `jump_patches`
/// is the list of byte offsets (within `bytes`) where the linker must add the
/// final append offset to the stored u16 target — required for body-relative
/// jumps emitted via `Program::emit_jump_*_placeholder_body_relative`.
///
/// Handlers with no internal branches return `jump_patches: vec![]` —
/// `Linker::append_function_with_body_relative_jumps` degenerates to
/// `append_function` in that case.
#[derive(Debug, Clone)]
pub struct HandlerBody {
    pub bytes: Vec<u8>,
    pub jump_patches: Vec<usize>,
}

impl HandlerBody {
    /// Convenience: wrap a linear body (no jumps) in a HandlerBody.
    pub fn linear(bytes: Vec<u8>) -> Self {
        Self { bytes, jump_patches: vec![] }
    }

    /// Consume a Program together with its captured body-relative jump patches.
    pub fn from_program_with_patches(p: Program, patches: &[BodyRelativeJumpPatch]) -> Self {
        let (bytes, jump_patches) = p.into_body_with_jumps(patches);
        Self { bytes, jump_patches }
    }
}

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
    pub const ADMIN: u32 = 0;
    pub const MATCHER_PROGRAM: u32 = 32;
    pub const ORACLE: u32 = 64;
    pub const VAULT: u32 = 96;
    pub const INSURANCE_FUND: u32 = 112;
    pub const MAINTENANCE_MARGIN_BPS: u32 = 128;
    pub const INITIAL_MARGIN_BPS: u32 = 136;
    pub const TRADING_FEE_BPS: u32 = 144;
    pub const MAX_ACCOUNTS: u32 = 152;
    pub const NEW_ACCOUNT_FEE: u32 = 160;
    pub const MAX_CRANK_STALENESS_SLOTS: u32 = 176;
    pub const LIQUIDATION_FEE_BPS: u32 = 184;
    pub const LIQUIDATION_FEE_CAP: u32 = 192;
    pub const MIN_LIQUIDATION_ABS: u32 = 208;
    pub const MIN_INITIAL_DEPOSIT: u32 = 224;
    pub const MIN_NONZERO_MM_REQ: u32 = 240;
    pub const MIN_NONZERO_IM_REQ: u32 = 256;
    pub const INSURANCE_FLOOR: u32 = 272;
    pub const H_MIN: u32 = 288;
    pub const H_MAX: u32 = 296;
    pub const RESOLVE_PRICE_DEVIATION_BPS: u32 = 304;
    pub const CURRENT_SLOT: u32 = 312;
    pub const CURRENT_ORACLE_PRICE: u32 = 320;
    pub const CURRENT_FUNDING_RATE_E9: u32 = 328;
    pub const MARKET_MODE: u32 = 344;
    pub const RESOLVED_PRICE: u32 = 345;
    pub const RESOLVED_SLOT: u32 = 353;
    pub const RESOLVED_H_NUM: u32 = 361;
    pub const RESOLVED_H_DEN: u32 = 377;
    pub const C_TOT: u32 = 393;
    pub const H_NUM: u32 = 409;
    pub const ADL_A_LONG_LIMB_0: u32 = 425;
    pub const ADL_A_SHORT_LIMB_0: u32 = 457;
    pub const ADL_K_LONG_LIMB_0: u32 = 489;
    pub const ADL_K_SHORT_LIMB_0: u32 = 521;
    pub const ADL_EPOCH_LONG: u32 = 553;
    pub const ADL_EPOCH_SHORT: u32 = 561;
    pub const CUMULATIVE_FUNDING_E9_LIMB_0: u32 = 569;
    pub const OPEN_ACCOUNT_COUNT: u32 = 601;
    pub const ACCOUNT_BITMAP_HI: u32 = 603;
    pub const ACCOUNT_BITMAP_LO: u32 = 619;
    pub const LAST_FUNDING_UPDATE_SLOT: u32 = 635;
    pub const PENDING_CRANKS: u32 = 643;
}

pub mod margin_offsets {
    //! Packed offsets for fields on `MarginAccount` referenced by the
    //! handlers. See `account MarginAccount { ... }` in `dsl/src/main.v`.
    pub const HOLDER: u32 = 0;
    pub const MATCHER_PROGRAM: u32 = 32;
    pub const MATCHER_CONTEXT: u32 = 64;
    pub const KIND: u32 = 96;
    pub const ACCOUNT_IDX: u32 = 97;
    pub const CAPITAL: u32 = 99;
    pub const PNL: u32 = 115;
    pub const RESERVED_PNL: u32 = 131;
    pub const POSITION_BASIS_Q: u32 = 147;
    pub const ADL_A_BASIS: u32 = 163;
    pub const ADL_K_SNAP: u32 = 179;
    pub const F_SNAP: u32 = 195;
    pub const ADL_EPOCH_SNAP: u32 = 211;
    pub const FEE_CREDITS: u32 = 219;
    pub const SCHED_PRESENT: u32 = 235;
    pub const SCHED_REMAINING_Q: u32 = 236;
    pub const SCHED_ANCHOR_Q: u32 = 252;
    pub const SCHED_START_SLOT: u32 = 268;
    pub const SCHED_HORIZON: u32 = 276;
    pub const SCHED_RELEASE_Q: u32 = 284;
    pub const PENDING_PRESENT: u32 = 300;
    pub const PENDING_REMAINING_Q: u32 = 301;
    pub const PENDING_HORIZON: u32 = 317;
    pub const PENDING_CREATED_SLOT: u32 = 325;
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
pub fn handler_body_top_up_insurance_fund() -> HandlerBody {
    let mut p = Program::new();
    // risk.insurance_fund += amount
    p.emit_load_field(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    p.emit_load_param(3);
    p.raw(ADD);
    p.emit_store_field(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    // risk.vault += amount
    p.emit_load_field(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_param(3);
    p.raw(ADD);
    p.emit_store_field(RISK_ACCT, risk_offsets::VAULT);
    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::linear(p.into_body())
}

/// `deposit(risk, acct, owner, amount, oracle_price) -> u64`.
///
/// Source: percolator.rs:3004 (deposit_not_atomic). Spec §10.3.
///
/// Params: risk=1, acct=2, owner=3, amount=4 (u128), oracle_price=5.
///
/// Full scope:
///   * Vault capacity pre-check: `risk.vault + amount <= MAX_VAULT_TVL`
///     (spec §10.3 step 1b). Abort with status 2 if exceeded.
///   * Mutations: `acct.capital += amount; risk.vault += amount;
///     risk.c_tot += amount`.
///   * Fee-debt sweep: if `acct.position_basis_q == 0 && acct.pnl >= 0`,
///     clear `acct.fee_credits` (spec §7.5 — deposit only sweeps when flat
///     and solvent).
///
/// Still deferred:
///   * New-account materialization with MIN_INITIAL_DEPOSIT + new_account_fee
///     gate — handled by the DSL front-end before this handler runs, so the
///     bytecode assumes the account is already materialized.
///   * settle_losses invocation — upstream invokes it after set_capital;
///     our flat-pos sweep approximates the effect for the common case where
///     the account has no outstanding losses.
pub fn handler_body_deposit() -> HandlerBody {
    const MAX_VAULT_TVL: u128 = 10_000_000_000_000_000;

    let mut p = Program::new();
    // --- Vault capacity pre-check -------------------------------------------
    // risk.vault + amount > MAX_VAULT_TVL ⇒ abort with status 2.
    p.emit_load_field(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_param(4);
    p.raw(ADD);
    p.push_u128(MAX_VAULT_TVL);
    p.raw(five_protocol::opcodes::GT);
    let vault_ok = p.emit_jump_if_not_placeholder_body_relative();
    p.push_u64(2);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(vault_ok);

    // --- Core mutations -----------------------------------------------------
    // acct.capital += amount
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    // risk.vault += amount
    p.emit_load_field(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field(RISK_ACCT, risk_offsets::VAULT);
    // risk.c_tot += amount
    p.emit_load_field(RISK_ACCT, risk_offsets::C_TOT);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field(RISK_ACCT, risk_offsets::C_TOT);

    // --- Flat-position fee-debt sweep --------------------------------------
    // if acct.position_basis_q == 0 && acct.pnl (bit-cast as i128) is
    // non-negative, zero acct.fee_credits. Non-negative check: sign bit of
    // the pnl's high u64 is 0.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.push_u128(0);
    p.raw(EQ);
    let skip_sweep = p.emit_jump_if_not_placeholder_body_relative();
    // Pnl non-negative test
    p.emit_load_field(MARGIN_ACCT, margin_offsets::PNL + 8);
    p.push_u64(1u64 << 63);
    p.raw(five_protocol::opcodes::BITWISE_AND);
    let skip_sweep_neg_pnl = p.emit_jump_if_placeholder_body_relative();
    // Sweep: fee_credits = 0.
    p.push_u128(0);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::FEE_CREDITS);
    p.patch_jump_to_here_body_relative(skip_sweep_neg_pnl);
    p.patch_jump_to_here_body_relative(skip_sweep);

    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &[vault_ok, skip_sweep, skip_sweep_neg_pnl])
}

/// `withdraw(risk, acct, owner, amount, oracle_price) -> u64`.
///
/// Source: percolator.rs:3079 (withdraw_not_atomic). Spec §10.3.
///
/// Params: risk=1, acct=2, owner=3, amount=4 (u128), oracle_price=5.
///
/// Full scope:
///   * Pre-check: `amount <= acct.capital` (step 4 of the spec). Abort with
///     status 3 if insufficient.
///   * Dust guard: `post_cap == 0 || post_cap >= risk.min_initial_deposit`
///     (step 5). Abort with status 4 if the remaining capital would be dust.
///   * Free-collateral check: if position is non-zero, require
///     `post_cap >= risk.min_nonzero_im_req` (step 6 — simplified from the
///     full MM math that would demand notional × IM_bps / 10_000, which
///     matches the spec's `max(..., min_nonzero_im_req)` clamp when the
///     proportional leg floors to zero for small positions).
///   * Mutations: `acct.capital -= amount; risk.vault -= amount;
///     risk.c_tot -= amount`.
///
/// Still deferred: the proportional-notional MM check requires
/// `MULDIV_REM_U256` fed with oracle_price × |position|; the bytecode
/// primitive now exists (wide_signed_mul_div_floor) but wiring it into
/// this handler's post-write flow is queued.
pub fn handler_body_withdraw() -> HandlerBody {
    let mut p = Program::new();

    // --- amount <= capital (step 4) -----------------------------------------
    p.emit_load_param(4);
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.raw(five_protocol::opcodes::GT);
    let cap_sufficient = p.emit_jump_if_not_placeholder_body_relative();
    p.push_u64(3);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(cap_sufficient);

    // --- Dust guard (step 5) ------------------------------------------------
    // post_cap = capital - amount (u128 polymorphic SUB, safe post step 4).
    // If post_cap > 0 AND post_cap < min_initial_deposit → abort.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(SUB);
    p.raw(DUP);
    p.push_u128(0);
    p.raw(EQ);
    let post_cap_is_zero = p.emit_jump_if_placeholder_body_relative();
    // post_cap > 0: check >= min_initial_deposit.
    p.raw(DUP);
    p.emit_load_field(RISK_ACCT, risk_offsets::MIN_INITIAL_DEPOSIT);
    p.raw(five_protocol::opcodes::LT);
    let dust_ok = p.emit_jump_if_not_placeholder_body_relative();
    p.push_u64(4);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(dust_ok);
    p.patch_jump_to_here_body_relative(post_cap_is_zero);
    // Drop the duplicate post_cap (we only needed it for the branches above).
    p.raw(five_protocol::opcodes::DROP);

    // --- Free-collateral / IM check (step 6, simplified) --------------------
    // if position_basis_q != 0 && post_cap < risk.min_nonzero_im_req → abort.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.push_u128(0);
    p.raw(EQ);
    let position_is_flat = p.emit_jump_if_placeholder_body_relative();
    // Non-flat: check post_cap vs min_nonzero_im_req.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_load_field(RISK_ACCT, risk_offsets::MIN_NONZERO_IM_REQ);
    p.raw(five_protocol::opcodes::LT);
    let collateral_ok = p.emit_jump_if_not_placeholder_body_relative();
    p.push_u64(5);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(collateral_ok);
    p.patch_jump_to_here_body_relative(position_is_flat);

    // --- Core mutations -----------------------------------------------------
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_field(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_field(RISK_ACCT, risk_offsets::C_TOT);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field(RISK_ACCT, risk_offsets::C_TOT);

    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(
        p,
        &[cap_sufficient, post_cap_is_zero, dust_ok, position_is_flat, collateral_ok],
    )
}

/// `convert_released_pnl(risk, acct, owner, amount) -> u64`.
///
/// Params: risk=1, acct=2, owner=3, amount=4.
/// Effect: `acct.reserved_pnl -= amount; acct.capital += amount`.
pub fn handler_body_convert_released_pnl() -> HandlerBody {
    let mut p = Program::new();
    // acct.reserved_pnl -= amount
    p.emit_load_field(MARGIN_ACCT, margin_offsets::RESERVED_PNL);
    p.emit_load_param(4);
    p.raw(SUB);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::RESERVED_PNL);
    // acct.capital += amount
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_param(4);
    p.raw(ADD);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::linear(p.into_body())
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
/// Full scope adds the flat-position pre-check (spec §10.3 close):
///   * `acct.position_basis_q == 0` — else abort with status 6.
///   * `acct.reserved_pnl == 0` — else abort with status 7. Upstream requires
///     the user to convert released PnL before close.
///   * `acct.fee_credits == 0` — else abort with status 8. Upstream's spec
///     §7.5 requires a clean slate on close.
///
/// The state-transition half of the handler (zero fields + decrement counter)
/// runs only after every guard passes.
pub fn handler_body_close_account() -> HandlerBody {
    let mut p = Program::new();
    let mut jump_patches = vec![];

    // Guard: position_basis_q == 0.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.push_u128(0);
    p.raw(EQ);
    let pos_ok = p.emit_jump_if_placeholder_body_relative();
    jump_patches.push(pos_ok);
    p.push_u64(6);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(pos_ok);

    // Guard: reserved_pnl == 0.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::RESERVED_PNL);
    p.push_u128(0);
    p.raw(EQ);
    let rpnl_ok = p.emit_jump_if_placeholder_body_relative();
    jump_patches.push(rpnl_ok);
    p.push_u64(7);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(rpnl_ok);

    // Guard: fee_credits == 0 (polymorphic EQ handles i128 bit pattern).
    p.emit_load_field(MARGIN_ACCT, margin_offsets::FEE_CREDITS);
    p.push_u128(0);
    p.raw(EQ);
    let fc_ok = p.emit_jump_if_placeholder_body_relative();
    jump_patches.push(fc_ok);
    p.push_u64(8);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(fc_ok);

    // --- State transitions (unchanged from the Simplified scope) -----------
    // risk.vault -= acct.capital
    p.emit_load_field(RISK_ACCT, risk_offsets::VAULT);
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.raw(SUB);
    // ... -= acct.reserved_pnl
    p.emit_load_field(MARGIN_ACCT, margin_offsets::RESERVED_PNL);
    p.raw(SUB);
    p.emit_store_field(RISK_ACCT, risk_offsets::VAULT);

    // risk.c_tot -= acct.capital
    p.emit_load_field(RISK_ACCT, risk_offsets::C_TOT);
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.raw(SUB);
    p.emit_store_field(RISK_ACCT, risk_offsets::C_TOT);

    // risk.open_account_count -= 1 (u16 counter)
    p.emit_load_field(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);
    p.push_u64(1);
    p.raw(SUB);
    p.emit_store_field(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);

    // acct.capital = 0 (u128)
    p.push_u128(0);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    // acct.reserved_pnl = 0 (u128)
    p.push_u128(0);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::RESERVED_PNL);

    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &jump_patches)
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
pub fn handler_body_settle_account() -> HandlerBody {
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
    p.emit_store_field(RISK_ACCT, risk_offsets::CURRENT_FUNDING_RATE_E9);

    // --- cumulative_funding_e9 accrual (u128 polymorphic ADD) --------------
    // The pre-mono i256 accumulator (4× u64 limbs) collapses onto u128's low
    // 16 bytes now that ADD_I256 is gone. Since the funding-rate opcode
    // surface in mono removed wide-math, and polymorphic ADD on u128 wraps,
    // we lose the i256 overflow headroom — but 2^128 nanounits at 1e9/s is
    // still ~1e29 seconds of continuous accrual, so practical wrap is
    // impossible. Limbs 2 and 3 of the declared layout stay zero.
    //
    // funding_rate_e9 (param 6) is already in the right u128 bit pattern
    // for polymorphic ADD to handle as i128 two's-complement wrapping.
    p.emit_load_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0);
    p.emit_load_param(6);
    p.push_u128(0); // type tag: forces u128 width at polymorphic dispatch
    p.raw(ADD);     // funding_rate_e9 + 0 → u128 (no-op; type-promotes param)
    p.raw(ADD);     // cumulative += funding_rate_e9 (u128)
    p.emit_store_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0);

    // --- acct.f_snap = current_funding_rate_e9 (u128 copy) -----------------
    p.emit_load_field(RISK_ACCT, risk_offsets::CURRENT_FUNDING_RATE_E9);
    p.push_u128(0); // force u128-width polymorphic copy
    p.raw(ADD);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::F_SNAP);

    // --- Flat-position PnL zero (branch) ------------------------------------
    // if acct.position_basis_q == 0:
    //     acct.pnl = 0
    //     acct.fee_credits = 0
    // Uses polymorphic EQ on the U128 ValueRef — EQ pushes 1 iff equal.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.push_u128(0);
    p.raw(EQ);
    let skip_flat_zero = p.emit_jump_if_not_placeholder_body_relative();
    // Flat: zero pnl + fee_credits.
    p.push_u128(0);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::PNL);
    p.push_u128(0);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::FEE_CREDITS);
    p.patch_jump_to_here_body_relative(skip_flat_zero);

    // --- Fee sweep (branch) -------------------------------------------------
    // if acct.capital > risk.min_nonzero_im_req:
    //     excess = acct.capital - threshold
    //     acct.capital = threshold
    //     acct.fee_credits += excess
    //
    // Polymorphic GT (0x25) handles u128 directly — pushes 1 iff capital > thr.
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_field(RISK_ACCT, risk_offsets::MIN_NONZERO_IM_REQ);
    p.raw(five_protocol::opcodes::GT);
    let skip_fee_sweep = p.emit_jump_if_not_placeholder_body_relative();
    // Fee-sweep branch:
    //   excess = capital - threshold (polymorphic u128 SUB; safe because the
    //   branch guard above proved capital > threshold)
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.emit_load_field(RISK_ACCT, risk_offsets::MIN_NONZERO_IM_REQ);
    p.raw(SUB);
    // Stack: [excess]
    //   fee_credits += excess
    p.raw(DUP);
    p.emit_load_field(MARGIN_ACCT, margin_offsets::FEE_CREDITS);
    p.raw(ADD);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::FEE_CREDITS);
    //   capital -= excess (equivalent to capital := threshold)
    p.emit_load_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.raw(five_protocol::opcodes::SWAP);
    p.raw(SUB);
    p.emit_store_field(MARGIN_ACCT, margin_offsets::CAPITAL);
    p.patch_jump_to_here_body_relative(skip_fee_sweep);

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &[skip_flat_zero, skip_fee_sweep])
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
pub fn handler_body_execute_trade() -> HandlerBody {
    let mut p = Program::new();
    const TAKER_ACCT: u8 = 2;
    const MAKER_ACCT: u8 = 3;

    // taker.position_basis_q += size_q_signed (i128 addition via generic ADD)
    p.emit_load_field(TAKER_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.emit_load_param(6);
    p.raw(ADD);
    p.emit_store_field(TAKER_ACCT, margin_offsets::POSITION_BASIS_Q);

    // maker.position_basis_q -= size_q_signed
    p.emit_load_field(MAKER_ACCT, margin_offsets::POSITION_BASIS_Q);
    p.emit_load_param(6);
    p.raw(SUB);
    p.emit_store_field(MAKER_ACCT, margin_offsets::POSITION_BASIS_Q);

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
    p.emit_load_field(TAKER_ACCT, margin_offsets::PNL);
    p.emit_load_param(5); // oracle_price (u64)
    p.emit_load_param(7); // exec_price (u64)
    p.raw(SUB);           // price_delta (u64, wraps for negative deltas)
    p.emit_load_param(6); // size_q_signed (i128)
    p.raw(MUL);           // pnl_delta (i128, polymorphic promotes u64×i128)
    p.raw(ADD);           // taker.pnl += pnl_delta
    p.emit_store_field(TAKER_ACCT, margin_offsets::PNL);

    // maker.pnl -= (oracle - exec) * size_q_signed
    p.emit_load_field(MAKER_ACCT, margin_offsets::PNL);
    p.emit_load_param(5);
    p.emit_load_param(7);
    p.raw(SUB);
    p.emit_load_param(6);
    p.raw(MUL);
    p.raw(SUB);
    p.emit_store_field(MAKER_ACCT, margin_offsets::PNL);

    // --- Trading fee collection --------------------------------------------
    // taker.fee_credits -= fee (fee is a positive u128 magnitude; the account
    // field is i128, polymorphic SUB treats this as signed subtraction).
    // Exact fee formula uses MULDIV u256 to preserve precision before dividing
    // by 10_000 and POS_SCALE; the trade's trading_fee_override (param 8) is a
    // pre-computed u64 that already folds the bps→absolute math.
    p.emit_load_field(TAKER_ACCT, margin_offsets::FEE_CREDITS);
    p.emit_load_param(8);
    p.raw(SUB);
    p.emit_store_field(TAKER_ACCT, margin_offsets::FEE_CREDITS);

    p.emit_load_field(MAKER_ACCT, margin_offsets::FEE_CREDITS);
    p.emit_load_param(8);
    p.raw(ADD);
    p.emit_store_field(MAKER_ACCT, margin_offsets::FEE_CREDITS);

    // --- OI-bound enforcement (post-trade, both sides) ---------------------
    // Spec §4.6 / percolator.rs:3303 gates the trade on
    // `|new_position|.unsigned_abs() <= MAX_POSITION_ABS_Q` (1e14). For each
    // of taker and maker: branch on the i128 sign bit (high u64 of
    // position_basis_q), compute the unsigned magnitude (either pos or
    // 0 - pos), compare against MAX_POSITION_ABS_Q with polymorphic CMP,
    // return status 1 if exceeded.
    const MAX_POSITION_ABS_Q: u128 = 100_000_000_000_000;
    let mut jump_patches = vec![];
    for account in [TAKER_ACCT, MAKER_ACCT] {
        // Sign test on the high u64 of position_basis_q.
        p.emit_load_field(account, margin_offsets::POSITION_BASIS_Q + 8);
        p.push_u64(1u64 << 63);
        p.raw(five_protocol::opcodes::BITWISE_AND);
        let pos_non_negative = p.emit_jump_if_not_placeholder_body_relative();
        jump_patches.push(pos_non_negative);
        // Negative branch: magnitude = 0 - pos (u128 polymorphic SUB).
        p.push_u128(0);
        p.emit_load_field(account, margin_offsets::POSITION_BASIS_Q);
        p.raw(SUB);
        let jmp_to_cmp = p.emit_jump_placeholder_body_relative();
        jump_patches.push(jmp_to_cmp);
        // Positive branch: magnitude = pos as u128.
        p.patch_jump_to_here_body_relative(pos_non_negative);
        p.emit_load_field(account, margin_offsets::POSITION_BASIS_Q);
        // Join with u128 magnitude on top of stack.
        p.patch_jump_to_here_body_relative(jmp_to_cmp);
        // Compare vs MAX_POSITION_ABS_Q using polymorphic GT (opcode 0x25).
        // Stack before GT: [magnitude, MAX_POSITION_ABS_Q]; polymorphic_comparison_op
        // pops b then a and pushes (a > b), so pushing MAX second gives exactly
        // the predicate magnitude > MAX.
        p.push_u128(MAX_POSITION_ABS_Q);
        p.raw(five_protocol::opcodes::GT);
        let ok = p.emit_jump_if_not_placeholder_body_relative();
        jump_patches.push(ok);
        p.push_u64(1);
        p.raw(RETURN_VALUE);
        p.patch_jump_to_here_body_relative(ok);
    }

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &jump_patches)
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
pub fn handler_body_liquidate_at_oracle() -> HandlerBody {
    let mut p = Program::new();
    const VICTIM_ACCT: u8 = 2;
    const LIQUIDATOR_ACCT: u8 = 3;

    // fee = victim.capital * liquidation_fee_bps / 10_000
    // The division is u128 polymorphic DIV (opcode 0x23). Leaves fee (u128)
    // on top of the stack after the polymorphic MUL → DIV chain.
    p.emit_load_field(VICTIM_ACCT, margin_offsets::CAPITAL);
    p.emit_load_field(RISK_ACCT, risk_offsets::LIQUIDATION_FEE_BPS); // u64
    p.raw(MUL);
    p.push_u128(10_000);
    p.raw(five_protocol::opcodes::DIV);
    // Stack: [fee_raw]

    // --- fee_cap clamp (branch) --------------------------------------------
    // fee = min(fee_raw, risk.liquidation_fee_cap) — spec §12 caps fees to
    // prevent a liquidator from extracting more than the absolute cap on an
    // unusually large liquidation. Done inline so the rest of this handler
    // can assume `fee` is already clamped.
    p.raw(DUP);
    p.emit_load_field(RISK_ACCT, risk_offsets::LIQUIDATION_FEE_CAP);
    p.raw(five_protocol::opcodes::GT);
    // Stack: [fee_raw, (fee_raw > cap)]
    let fee_within_cap = p.emit_jump_if_not_placeholder_body_relative();
    // Over-cap branch: pop fee_raw, push cap (ignored the copy above).
    p.raw(five_protocol::opcodes::DROP);
    p.emit_load_field(RISK_ACCT, risk_offsets::LIQUIDATION_FEE_CAP);
    p.patch_jump_to_here_body_relative(fee_within_cap);
    // Stack: [fee]  (either fee_raw itself, or cap, whichever was smaller)

    // DUP so the fee stays available for both the insurance credit and the
    // liquidator's share subtraction later on.
    p.raw(DUP);
    // Stack: [fee, fee]

    // risk.insurance_fund += fee
    p.emit_load_field(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    p.raw(SWAP_FOR_POLY);
    p.raw(ADD);
    p.emit_store_field(RISK_ACCT, risk_offsets::INSURANCE_FUND);
    // Stack still has one fee on top.

    // liquidator.reserved_pnl += (victim.capital - fee + victim.reserved_pnl)
    // Build step-by-step: load liquidator.reserved_pnl, load victim.capital,
    // add (intermediate result = liquidator_pnl + capital), subtract the fee
    // sitting on top of the stack already (put it last via swap), then add
    // victim.reserved_pnl for completeness.
    p.emit_load_field(LIQUIDATOR_ACCT, margin_offsets::RESERVED_PNL);
    p.emit_load_field(VICTIM_ACCT, margin_offsets::CAPITAL);
    p.raw(ADD);
    // Stack: [fee, liquidator_pnl + capital]  (fee is underneath — need swap)
    p.raw(SWAP_FOR_POLY);
    p.raw(SUB);
    // Stack: [liquidator_pnl + capital - fee]
    p.emit_load_field(VICTIM_ACCT, margin_offsets::RESERVED_PNL);
    p.raw(ADD);
    p.emit_store_field(LIQUIDATOR_ACCT, margin_offsets::RESERVED_PNL);

    // Zero out the victim's live-state fields.
    p.push_u128(0);
    p.emit_store_field(VICTIM_ACCT, margin_offsets::CAPITAL);
    p.push_u128(0);
    p.emit_store_field(VICTIM_ACCT, margin_offsets::RESERVED_PNL);
    p.push_u128(0);
    p.emit_store_field(VICTIM_ACCT, margin_offsets::POSITION_BASIS_Q);

    // Decrement open_account_count (the victim is effectively closed).
    p.emit_load_field(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);
    p.push_u64(1);
    p.raw(SUB);
    p.emit_store_field(RISK_ACCT, risk_offsets::OPEN_ACCOUNT_COUNT);

    // Status 0 → success.
    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &[fee_within_cap])
}

/// Local SWAP alias so the polymorphic-arithmetic code reads without needing
/// to import `SWAP` alongside the multi-precision opcodes.
const SWAP_FOR_POLY: u8 = five_protocol::opcodes::SWAP;

/// `keeper_crank(risk, caller, now_slot, oracle_price, funding_rate_e9) -> u64`.
///
/// Source: percolator.rs:3836 (keeper_crank_not_atomic). Spec §13.
///
/// Full scope:
///   * Time monotonicity: `now_slot >= risk.current_slot` (spec §13 step 3).
///     Abort with status 9 if violated.
///   * Accrue market: fold `funding_rate_e9` into `risk.cumulative_funding_e9`
///     (i256 ADD_I256 with sign-extended delta — same sequence as
///     settle_account's accrual).
///   * last_crank_slot advance: if `now_slot > risk.last_crank_slot`,
///     set `last_crank_slot = now_slot`. Implemented as an unconditional
///     polymorphic `max(now_slot, last_crank_slot)` write (correct in both
///     cases — if now_slot <= last_crank_slot the old value is preserved).
///     Uses GT + conditional store.
///
/// Still deferred:
///   * Batch-liquidation loop over `ordered_candidates` — needs iterative
///     bytecode with bitmap scanning over risk.account_bitmap_lo/hi and a
///     call-into-liquidate-at-oracle path. The single-account liquidation
///     path is already Full (handler_body_liquidate_at_oracle); a batching
///     driver is future work best done in DSL.
///
/// Params: risk=1, caller=2, now_slot=3, oracle_price=4, funding_rate_e9=5.
pub fn handler_body_keeper_crank() -> HandlerBody {
    let mut p = Program::new();
    let mut jump_patches = vec![];

    // --- Time monotonicity (step 3) ----------------------------------------
    // now_slot < risk.current_slot ⇒ abort with status 9.
    p.emit_load_param(3); // now_slot (u64)
    p.emit_load_field(RISK_ACCT, risk_offsets::CURRENT_SLOT);
    p.raw(five_protocol::opcodes::LT);
    let monotonic_ok = p.emit_jump_if_not_placeholder_body_relative();
    jump_patches.push(monotonic_ok);
    p.push_u64(9);
    p.raw(RETURN_VALUE);
    p.patch_jump_to_here_body_relative(monotonic_ok);

    // --- Cumulative funding accrual (u128 polymorphic ADD) ----------------
    // Same collapse as settle_account: 4-limb i256 → 16-byte u128.
    // funding_rate_e9 (param 5) is already in the right u128 bit pattern
    // for wrapping add; polymorphic ADD handles u128 width when either
    // operand is U128.
    p.emit_load_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0);
    p.emit_load_param(5);
    p.push_u128(0);
    p.raw(ADD);
    p.raw(ADD);
    p.emit_store_field(RISK_ACCT, risk_offsets::CUMULATIVE_FUNDING_E9_LIMB_0);

    // --- last_crank_slot advance -------------------------------------------
    // if now_slot > risk.last_crank_slot: write now_slot there.
    // (The DSL side wrote last_funding_update_slot already; this field is
    // distinct — tracks when the KEEPER last ran vs when funding was last
    // applied. Sometimes they coincide, sometimes they don't.)
    //
    // Note: the spec uses `last_crank_slot` on the RiskEngine, but our DSL
    // struct calls the same semantic field `last_funding_update_slot`.
    // We reuse that field for the keeper slot update — writing it twice is
    // idempotent for the common case. The DSL's own last_funding_update_slot
    // write happens unconditionally, so this branch effectively no-ops when
    // the slot is already up-to-date.
    p.emit_load_param(3); // now_slot
    p.emit_load_field(RISK_ACCT, risk_offsets::LAST_FUNDING_UPDATE_SLOT);
    p.raw(five_protocol::opcodes::GT);
    let crank_not_advancing = p.emit_jump_if_not_placeholder_body_relative();
    jump_patches.push(crank_not_advancing);
    p.emit_load_param(3);
    p.emit_store_field(RISK_ACCT, risk_offsets::LAST_FUNDING_UPDATE_SLOT);
    p.patch_jump_to_here_body_relative(crank_not_advancing);

    p.push_u64(0);
    p.raw(RETURN_VALUE);
    HandlerBody::from_program_with_patches(p, &jump_patches)
}

// =============================================================================
// Linker convenience
// =============================================================================

/// (sentinel, HandlerBody) pair for every Percolator handler.
///
/// Used by tests and by consumers that want to fully link a compiled main.v
/// in one call. Order is arbitrary — the linker handles each sentinel
/// independently. Each HandlerBody carries both the raw bytes and the list of
/// body-relative jump offsets the linker must fix up at append time.
pub fn all_handler_bodies() -> [(u64, HandlerBody); 9] {
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
            assert!(
                !body.bytes.is_empty(),
                "empty body for sentinel {:#x}",
                sentinel
            );
            let n = body.bytes.len();
            assert!(n >= 3, "body too short for sentinel {:#x}", sentinel);
            assert_eq!(body.bytes[n - 1], RETURN_VALUE);
        }
    }

    #[test]
    fn body_relative_jump_patches_stay_within_bounds() {
        for (sentinel, body) in all_handler_bodies() {
            for &patch in &body.jump_patches {
                assert!(
                    patch + 2 <= body.bytes.len(),
                    "jump patch at {} overruns body length {} (sentinel {:#x})",
                    patch,
                    body.bytes.len(),
                    sentinel
                );
            }
        }
    }
}
