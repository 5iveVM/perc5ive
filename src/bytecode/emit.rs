//! Low-level bytecode emitter for building `.five` binaries by hand against the
//! five-mono VM surface.
//!
//! The emitter wraps a `Vec<u8>`: each method appends one opcode plus its
//! immediate operands, and `finish_with_halt` prepends the 10-byte 5IVE header.
//! All operand encoding is **fixed little-endian** — the mono VM dropped VLE
//! (variable-length encoding), so every immediate now has a fixed byte width
//! keyed off the opcode.
//!
//! Opcode width reference (matches `five-protocol/src/opcodes.rs`):
//!   PUSH_U8       → 1 byte operand (u8)
//!   PUSH_U16      → 2 bytes LE (u16)
//!   PUSH_U32      → 4 bytes LE (u32)
//!   PUSH_U64      → 8 bytes LE (u64)
//!   PUSH_I64      → 8 bytes LE (i64)
//!   PUSH_U128     → 16 bytes LE (u128)
//!   PUSH_BOOL     → 1 byte (0x00 | 0x01)
//!   PUSH_PUBKEY   → 32 bytes
//!   LOAD_PARAM    → 1 byte (param index)
//!   LOAD_FIELD    → 1 byte acct + 4 bytes LE offset
//!   STORE_FIELD   → 1 byte acct + 4 bytes LE offset
//!   CALL          → 1 byte param_count + 2 bytes LE target
//!   JUMP / JUMP_IF / JUMP_IF_NOT → 2 bytes LE target

use five_protocol::opcodes::*;

/// A builder for `.five` bytecode.
#[derive(Default, Clone)]
pub struct Program {
    body: Vec<u8>,
}

/// Handle for a forward jump that needs its target patched once known.
#[derive(Debug, Clone, Copy)]
pub struct JumpPatch {
    patch_at: usize,
}

/// Handle for a CALL whose target is patched after emission.
#[derive(Debug, Clone, Copy)]
pub struct CallPatch {
    pub(crate) patch_at: usize,
}

impl Program {
    pub fn new() -> Self {
        Self { body: Vec::new() }
    }

    /// Raw byte append — escape hatch for opcodes the builder has no typed
    /// method for.
    pub fn raw(&mut self, byte: u8) -> &mut Self {
        self.body.push(byte);
        self
    }

    pub fn raw_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.body.extend_from_slice(bytes);
        self
    }

    // ------------------------------------------------------------------------
    // Push constants — fixed-width LE encoding
    // ------------------------------------------------------------------------

    /// Emit `PUSH_U8(value)`.
    pub fn push_u8(&mut self, value: u8) -> &mut Self {
        self.body.push(PUSH_U8);
        self.body.push(value);
        self
    }

    /// Emit `PUSH_U16(value)` as 2 LE bytes.
    pub fn push_u16(&mut self, value: u16) -> &mut Self {
        self.body.push(PUSH_U16);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Emit `PUSH_U32(value)` as 4 LE bytes.
    pub fn push_u32(&mut self, value: u32) -> &mut Self {
        self.body.push(PUSH_U32);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Emit `PUSH_U64(value)` as 8 LE bytes. Mono uses fixed-width, not VLE.
    pub fn push_u64(&mut self, value: u64) -> &mut Self {
        self.body.push(PUSH_U64);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Emit `PUSH_I64(value)` as 8 LE bytes (two's-complement).
    pub fn push_i64(&mut self, value: i64) -> &mut Self {
        self.body.push(PUSH_I64);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Emit `PUSH_U128(value)` as 16 LE bytes. Also used for i128 hotspots —
    /// signed arithmetic is bit-level two's-complement under mono's polymorphic
    /// `ADD`/`SUB`/`MUL` so an i128 round-trips as its unsigned bit pattern.
    pub fn push_u128(&mut self, value: u128) -> &mut Self {
        self.body.push(PUSH_U128);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Push an i128 as its two's-complement u128 bit pattern.
    pub fn push_i128(&mut self, value: i128) -> &mut Self {
        self.push_u128(value as u128)
    }

    /// Emit a **pool-free** push of a small constant via the nibble opcodes
    /// `PUSH_0..PUSH_3` (0xD8-0xDB), which push `U64(0..=3)` directly.
    ///
    /// In a binary with `FEATURE_CONSTANT_POOL` set (everything the
    /// five-dsl-compiler emits), the regular `PUSH_U64`/`PUSH_U128` opcodes read
    /// their operand as a *pool index*, not an inline literal — so hand-written
    /// bodies appended into such a binary must source constants either from the
    /// pool or from these nibble pushes. Panics if `value > 3`.
    pub fn emit_push_nibble(&mut self, value: u64) -> &mut Self {
        let op = match value {
            0 => PUSH_0,
            1 => PUSH_1,
            2 => PUSH_2,
            3 => PUSH_3,
            _ => panic!("emit_push_nibble: {value} > 3 — use emit_push_uint"),
        };
        self.body.push(op);
        self
    }

    /// Emit a **pool-free** push of an arbitrary `u64` by composing nibble
    /// pushes with `ADD` (`3 + 3 + … + r`). Single opcode for `value <= 3`;
    /// otherwise `PUSH_3` then `(PUSH_d; ADD)` chunks. Intended for the small
    /// constants hand-written bodies need (status codes, divisors) when the
    /// binary's constant pool makes `PUSH_U64` a pool-index opcode. The pushed
    /// value is a `U64` on top of the stack.
    pub fn emit_push_uint(&mut self, value: u64) -> &mut Self {
        if value <= 3 {
            return self.emit_push_nibble(value);
        }
        self.emit_push_nibble(3);
        let mut rem = value - 3;
        while rem > 0 {
            let d = rem.min(3);
            self.emit_push_nibble(d);
            self.body.push(ADD);
            rem -= d;
        }
        self
    }

    /// Emit `PUSH_BOOL(value)`.
    pub fn push_bool(&mut self, value: bool) -> &mut Self {
        self.body.push(PUSH_BOOL);
        self.body.push(if value { 1 } else { 0 });
        self
    }

    /// Emit `PUSH_PUBKEY(32 bytes)`.
    pub fn push_pubkey(&mut self, key: &[u8; 32]) -> &mut Self {
        self.body.push(PUSH_PUBKEY);
        self.body.extend_from_slice(key);
        self
    }

    // ------------------------------------------------------------------------
    // DSL-handler primitives — LOAD_PARAM / LOAD_FIELD / STORE_FIELD
    //
    // A sentinel-rewrite callee running with `param_count = 0` inherits the
    // outer DSL handler's parameter frame and account table. DSL parameter
    // indexing: index 0 reserved; positional accounts occupy 1..N; scalar
    // params follow in declaration order.
    //
    // DSL field layout is packed: pubkey=32, u128/i128=16, u64=8, u32/i32=4,
    // u16/i16=2, u8/i8/bool=1.
    // ------------------------------------------------------------------------

    /// Emit `LOAD_PARAM index`. Uses the single-byte fast paths
    /// (`LOAD_PARAM_0..3`) when possible; otherwise encodes as `LOAD_PARAM`
    /// + 1-byte index.
    pub fn emit_load_param(&mut self, index: u8) -> &mut Self {
        match index {
            0 => self.body.push(LOAD_PARAM_0),
            1 => self.body.push(LOAD_PARAM_1),
            2 => self.body.push(LOAD_PARAM_2),
            3 => self.body.push(LOAD_PARAM_3),
            _ => {
                self.body.push(LOAD_PARAM);
                self.body.push(index);
            }
        }
        self
    }

    /// Emit `LOAD_FIELD account_index_u8 offset_u32_le`.
    ///
    /// Mono's LOAD_FIELD is polymorphic — it pushes an `AccountRef(acct, off)`
    /// that resolves lazily when a consumer (arithmetic / STORE_FIELD / …)
    /// needs the value. Width is inferred from the consumer's ValueRef type.
    pub fn emit_load_field(&mut self, account_index: u8, offset: u32) -> &mut Self {
        self.body.push(LOAD_FIELD);
        self.body.push(account_index);
        self.body.extend_from_slice(&offset.to_le_bytes());
        self
    }

    /// Emit `STORE_FIELD account_index_u8 offset_u32_le`.
    ///
    /// Mono's STORE_FIELD is polymorphic — it writes `N` bytes where `N` is
    /// determined by the ValueRef type popped off the stack (U64→8, U128→16,
    /// U16→2, U8→1, Bool→1, Pubkey→32). Callers must push the correctly-typed
    /// value before emitting `STORE_FIELD`.
    pub fn emit_store_field(&mut self, account_index: u8, offset: u32) -> &mut Self {
        self.body.push(STORE_FIELD);
        self.body.push(account_index);
        self.body.extend_from_slice(&offset.to_le_bytes());
        self
    }

    /// Emit `LOAD_FIELD_PUBKEY account_index_u8 offset_u32_le`. Pushes a
    /// `PubkeyRef`; used for the 32-byte owner/mint fields in account structs.
    pub fn emit_load_field_pubkey(&mut self, account_index: u8, offset: u32) -> &mut Self {
        self.body.push(LOAD_FIELD_PUBKEY);
        self.body.push(account_index);
        self.body.extend_from_slice(&offset.to_le_bytes());
        self
    }

    // ------------------------------------------------------------------------
    // Locals — ALLOC_LOCALS / SET_LOCAL / GET_LOCAL
    //
    // Locals live in a store separate from the value stack. `ALLOC_LOCALS n`
    // reserves `n` slots for the current frame; SET/GET move values between a
    // slot and the stack top. Used by branchy handlers (e.g. the genesis
    // vote-weight log2 loop) that need to stash intermediates across jumps.
    // ------------------------------------------------------------------------

    /// Emit `ALLOC_LOCALS count`.
    pub fn emit_alloc_locals(&mut self, count: u8) -> &mut Self {
        self.body.push(ALLOC_LOCALS);
        self.body.push(count);
        self
    }

    /// Emit `SET_LOCAL idx` (pops stack top into local slot `idx`). Uses the
    /// single-byte fast paths `SET_LOCAL_0..3` when possible.
    pub fn emit_set_local(&mut self, idx: u8) -> &mut Self {
        match idx {
            0 => self.body.push(SET_LOCAL_0),
            1 => self.body.push(SET_LOCAL_1),
            2 => self.body.push(SET_LOCAL_2),
            3 => self.body.push(SET_LOCAL_3),
            _ => {
                self.body.push(SET_LOCAL);
                self.body.push(idx);
            }
        }
        self
    }

    /// Emit `GET_LOCAL idx` (pushes local slot `idx`). Uses the single-byte
    /// fast paths `GET_LOCAL_0..3` when possible.
    pub fn emit_get_local(&mut self, idx: u8) -> &mut Self {
        match idx {
            0 => self.body.push(GET_LOCAL_0),
            1 => self.body.push(GET_LOCAL_1),
            2 => self.body.push(GET_LOCAL_2),
            3 => self.body.push(GET_LOCAL_3),
            _ => {
                self.body.push(GET_LOCAL);
                self.body.push(idx);
            }
        }
        self
    }

    /// Emit an unconditional `JUMP target_u16` to an already-known absolute IP.
    /// Used for backward edges (loops) where the target precedes the jump.
    pub fn emit_jump_to(&mut self, absolute_target: u16) -> &mut Self {
        self.body.push(JUMP);
        self.body.extend_from_slice(&absolute_target.to_le_bytes());
        self
    }

    // ------------------------------------------------------------------------
    // Control flow — JUMP, JUMP_IF, JUMP_IF_NOT (2-byte LE target)
    // ------------------------------------------------------------------------

    /// Emit `JUMP_IF target_u16`. Pop top stack value; jump iff truthy.
    pub fn emit_jump_if_placeholder(&mut self) -> JumpPatch {
        self.body.push(JUMP_IF);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        JumpPatch { patch_at }
    }

    /// Emit `JUMP_IF_NOT target_u16`.
    pub fn emit_jump_if_not_placeholder(&mut self) -> JumpPatch {
        self.body.push(JUMP_IF_NOT);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        JumpPatch { patch_at }
    }

    /// Emit `JUMP target_u16`.
    pub fn emit_jump_placeholder(&mut self) -> JumpPatch {
        self.body.push(JUMP);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        JumpPatch { patch_at }
    }

    /// Patch a previously-reserved jump's target to the current emit position
    /// (expressed as an absolute IP, including the 10-byte header).
    pub fn patch_jump_to_here(&mut self, patch: JumpPatch) {
        let absolute = self.body.len() + 10;
        let target = u16::try_from(absolute).expect("jump target overflows u16");
        let bytes = target.to_le_bytes();
        self.body[patch.patch_at] = bytes[0];
        self.body[patch.patch_at + 1] = bytes[1];
    }

    // ------------------------------------------------------------------------
    // CALL — encoded as `CALL[1] param_count[1] func_addr[2 LE]` (4 bytes).
    // ------------------------------------------------------------------------

    /// Emit `CALL param_count <placeholder>`.
    pub fn emit_call_placeholder(&mut self, param_count: u8) -> CallPatch {
        self.body.push(CALL);
        self.body.push(param_count);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        CallPatch { patch_at }
    }

    /// Patch a CALL site to target an absolute address (local to this Program).
    pub fn patch_call_target(&mut self, patch: CallPatch, absolute_target: u16) {
        let bytes = absolute_target.to_le_bytes();
        self.body[patch.patch_at] = bytes[0];
        self.body[patch.patch_at + 1] = bytes[1];
    }

    /// Current emit position as an absolute IP (body length + 10-byte header).
    pub fn current_absolute_offset(&self) -> u16 {
        u16::try_from(self.body.len() + 10).expect("absolute offset overflows u16")
    }

    /// Absolute offset of a captured `CallPatch`'s CALL opcode byte (used by
    /// the linker to rewrite call sites once functions are appended).
    pub fn absolute_call_site(&self, patch: CallPatch) -> u16 {
        u16::try_from(patch.patch_at - 2 + 10).expect("call-site offset overflows u16")
    }

    // ------------------------------------------------------------------------
    // Termination
    // ------------------------------------------------------------------------

    /// Append `HALT` and return the full `.five` binary with header.
    pub fn finish_with_halt(mut self) -> Vec<u8> {
        self.body.push(HALT);
        prepend_five_header(self.body)
    }

    /// Consume the program and return the raw body bytes — no header, no HALT.
    pub fn into_body(self) -> Vec<u8> {
        self.body
    }

    /// Current body length (without header).
    pub fn body_len(&self) -> usize {
        self.body.len()
    }

    // ------------------------------------------------------------------------
    // Appended-body flavour — jumps that stay correct after the linker places
    // the body at an arbitrary append offset.
    // ------------------------------------------------------------------------

    /// Emit `JUMP_IF target_u16` with a body-relative target.
    pub fn emit_jump_if_placeholder_body_relative(&mut self) -> BodyRelativeJumpPatch {
        self.body.push(JUMP_IF);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        BodyRelativeJumpPatch { patch_at }
    }

    /// Emit `JUMP_IF_NOT target_u16` with a body-relative target.
    pub fn emit_jump_if_not_placeholder_body_relative(&mut self) -> BodyRelativeJumpPatch {
        self.body.push(JUMP_IF_NOT);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        BodyRelativeJumpPatch { patch_at }
    }

    /// Emit unconditional `JUMP target_u16` with a body-relative target.
    pub fn emit_jump_placeholder_body_relative(&mut self) -> BodyRelativeJumpPatch {
        self.body.push(JUMP);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        BodyRelativeJumpPatch { patch_at }
    }

    /// Emit an unconditional backward `JUMP` to an already-emitted point in the
    /// body, expressed as a **body-relative** offset (use [`Self::body_len`] to
    /// capture the loop-start offset before the loop). The returned patch must
    /// be passed to the linker alongside the other body-relative patches so the
    /// stored target is biased by the body's final append offset. Used for the
    /// genesis vote-weight `log2` loop inside an appended handler body, where
    /// absolute targets aren't known until link time.
    pub fn emit_jump_backward_body_relative(&mut self, body_target: u16) -> BodyRelativeJumpPatch {
        self.body.push(JUMP);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&body_target.to_le_bytes());
        BodyRelativeJumpPatch { patch_at }
    }

    /// Patch a body-relative jump to land at the current emit position. The
    /// stored value is body-relative; the linker adds the final body-start
    /// offset at append time.
    pub fn patch_jump_to_here_body_relative(&mut self, patch: BodyRelativeJumpPatch) {
        let relative = u16::try_from(self.body.len()).expect("body too large for u16 jump");
        let bytes = relative.to_le_bytes();
        self.body[patch.patch_at] = bytes[0];
        self.body[patch.patch_at + 1] = bytes[1];
    }

    /// Consume the program and return `(body, jump_patch_offsets)`.
    pub fn into_body_with_jumps(self, patches: &[BodyRelativeJumpPatch]) -> (Vec<u8>, Vec<usize>) {
        let offsets = patches.iter().map(|p| p.patch_at).collect();
        (self.body, offsets)
    }
}

/// Handle for a body-relative jump — paired with
/// [`Program::patch_jump_to_here_body_relative`] and fixed up at link time.
#[derive(Debug, Clone, Copy)]
pub struct BodyRelativeJumpPatch {
    pub(crate) patch_at: usize,
}

/// Prepend the 10-byte 5IVE header:
///   magic(4) = b"5IVE"
///   features(4 LE) = 0
///   public_function_count(1) = 0
///   total_function_count(1) = 0
fn prepend_five_header(body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 10);
    out.extend_from_slice(b"5IVE");
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    out.push(0x00);
    out.push(0x00);
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_u64_is_fixed_width_8_bytes() {
        let mut p = Program::new();
        p.push_u64(42);
        assert_eq!(p.body.len(), 1 + 8); // opcode + 8 LE bytes
        assert_eq!(p.body[0], PUSH_U64);
        assert_eq!(&p.body[1..9], &42u64.to_le_bytes());
    }

    #[test]
    fn push_u128_is_fixed_width_16_bytes() {
        let mut p = Program::new();
        p.push_u128(0xDEAD_BEEF_CAFE_BABE_1234_5678_9ABC_DEF0);
        assert_eq!(p.body.len(), 1 + 16);
        assert_eq!(p.body[0], PUSH_U128);
    }

    #[test]
    fn load_field_has_5_byte_operand() {
        let mut p = Program::new();
        p.emit_load_field(3, 0x1234_5678);
        assert_eq!(p.body.len(), 1 + 1 + 4);
        assert_eq!(p.body[0], LOAD_FIELD);
        assert_eq!(p.body[1], 3);
        assert_eq!(&p.body[2..6], &0x1234_5678u32.to_le_bytes());
    }

    #[test]
    fn jump_patch_writes_absolute_target() {
        let mut p = Program::new();
        p.push_u64(1);
        let jp = p.emit_jump_if_placeholder();
        p.push_u64(7);
        p.patch_jump_to_here(jp);
        let bin = p.finish_with_halt();
        // Target is stored little-endian right after JUMP_IF (at body offset
        // 1+8+1 = 10, absolute 20). The patch lands at `body.len() + 10` when
        // patch_jump_to_here was called.
        let body_start = 10; // header size
        // After push_u64(1) (9 body bytes) + emit_jump_if_placeholder (3 bytes)
        // we are at body offset 12. Patch lives at body offset 10 (operand
        // bytes). After push_u64(7) we're at body offset 12+9=21, then
        // patch_jump_to_here records absolute=body_start+21=31.
        let target = u16::from_le_bytes([bin[body_start + 10], bin[body_start + 11]]);
        assert_eq!(target, 31);
    }

    #[test]
    fn header_is_prepended() {
        let mut p = Program::new();
        p.push_u64(42);
        let bin = p.finish_with_halt();
        assert_eq!(&bin[..4], b"5IVE");
        assert_eq!(&bin[4..10], &[0, 0, 0, 0, 0, 0]);
        assert_eq!(bin[10], PUSH_U64);
    }
}
