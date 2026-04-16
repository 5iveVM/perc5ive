//! Low-level bytecode emitter for building `.five` binaries by hand.
//!
//! The emitter is a thin wrapper over `Vec<u8>`: each method appends one opcode
//! plus its immediate operands, and `finish()` prepends the 5IVE file header.
//! All operand encoding (VLE for u64, little-endian for u128) lives here so the
//! op-specific modules (`u256`, future `i256`, ...) stay focused on opcode
//! sequences.

use five_protocol::opcodes::*;

/// A builder for `.five` bytecode.
///
/// Build up an instruction stream with the `push_*` / `emit_*` methods, then
/// call [`Program::finish_with_halt`] to produce a complete binary ready for
/// [`MitoVM::execute_direct`](../../../five_vm_mito/struct.MitoVM.html#method.execute_direct).
///
/// The builder does **not** model functions or the function table — it emits a
/// flat instruction stream that runs from offset 0 past the 10-byte header.
/// That is sufficient for Percolator's u256 math helpers, which are pure
/// computations over stack values with no control flow beyond arithmetic.
#[derive(Default, Clone)]
pub struct Program {
    body: Vec<u8>,
}

/// Handle for a forward jump that needs its target patched once known.
/// Returned by `emit_jump*_placeholder`; consumed by `patch_jump_to_here`.
#[derive(Debug, Clone, Copy)]
pub struct JumpPatch {
    patch_at: usize,
}

/// Handle for a CALL whose target is patched after emission — typically by
/// the linker once it knows the absolute offset of the appended callee.
#[derive(Debug, Clone, Copy)]
pub struct CallPatch {
    pub(crate) patch_at: usize,
}

impl Program {
    pub fn new() -> Self {
        Self { body: Vec::new() }
    }

    /// Raw byte append — escape hatch for ops this builder doesn't have a method
    /// for yet. Prefer the typed methods below.
    pub fn raw(&mut self, byte: u8) -> &mut Self {
        self.body.push(byte);
        self
    }

    pub fn raw_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.body.extend_from_slice(bytes);
        self
    }

    // ------------------------------------------------------------------------
    // Push constants
    // ------------------------------------------------------------------------

    /// Emit `PUSH_U64(value)` using VLE encoding.
    pub fn push_u64(&mut self, value: u64) -> &mut Self {
        self.body.push(PUSH_U64);
        write_vle_u64(&mut self.body, value);
        self
    }

    /// Emit `PUSH_U128(value)` using 16 little-endian bytes (no VLE).
    pub fn push_u128(&mut self, value: u128) -> &mut Self {
        self.body.push(PUSH_U128);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Push four u64 limbs in order (a0, a1, a2, a3) to put a u256 on the stack.
    /// Convenient for operands to the multiprecision opcodes.
    pub fn push_u256(&mut self, limbs: [u64; 4]) -> &mut Self {
        for limb in limbs {
            self.push_u64(limb);
        }
        self
    }

    // ------------------------------------------------------------------------
    // DSL-handler primitives — LOAD_PARAM / LOAD_FIELD / STORE_FIELD
    //
    // These are the shape the five-dsl-compiler emits inside a handler body.
    // An appended linker-rewrite callee running with `param_count = 0` inherits
    // the outer DSL handler's parameter frame and account table, so LOAD_PARAM
    // and LOAD_FIELD / STORE_FIELD see the same params/accounts as the DSL
    // caller — exactly what sentinel-rewrite needs for u128 handler bodies.
    //
    // DSL parameter indexing (empirically verified via compiler probe):
    //   * index 0 is reserved (not addressable by user params)
    //   * positional accounts occupy indices 1..N (first account = 1)
    //   * scalar params follow, starting at index N+1 in declaration order
    //
    // Field layout is packed (no alignment padding) — the DSL lays out fields
    // in declaration order with byte sizes: pubkey=32, u128/i128=16, u64=8,
    // u32/i32=4, u16/i16=2, u8/i8/bool=1.
    // ------------------------------------------------------------------------

    /// Emit a `LOAD_PARAM index` instruction. Uses the single-byte fast path
    /// (`LOAD_PARAM_0..3`) when possible — which is what the compiler itself
    /// emits for the common small-index cases — and falls back to the generic
    /// `LOAD_PARAM + VLE index` form otherwise.
    pub fn emit_load_param(&mut self, index: u8) -> &mut Self {
        match index {
            0 => self.body.push(LOAD_PARAM_0),
            1 => self.body.push(LOAD_PARAM_1),
            2 => self.body.push(LOAD_PARAM_2),
            3 => self.body.push(LOAD_PARAM_3),
            _ => {
                self.body.push(LOAD_PARAM);
                write_vle_u64(&mut self.body, index as u64);
            }
        }
        self
    }

    /// Emit `LOAD_FIELD account_index_u8 offset_vle`. Reads 8 bytes (u64) from
    /// the account's data at the given offset and pushes a `ValueRef::U64`.
    /// For u128 or u16 fields use [`Program::emit_load_field_u128`] or
    /// [`Program::emit_load_field_u16`] — plain LOAD_FIELD would overlap into
    /// adjacent fields in a packed struct.
    pub fn emit_load_field(&mut self, account_index: u8, offset: u64) -> &mut Self {
        self.body.push(LOAD_FIELD);
        self.body.push(account_index);
        write_vle_u64(&mut self.body, offset);
        self
    }

    /// Emit `STORE_FIELD account_index_u8 offset_vle` — pops a u64 from the
    /// stack and writes 8 LE bytes at `offset`. u128/u16 fields need the
    /// sibling emitters below.
    pub fn emit_store_field(&mut self, account_index: u8, offset: u64) -> &mut Self {
        self.body.push(STORE_FIELD);
        self.body.push(account_index);
        write_vle_u64(&mut self.body, offset);
        self
    }

    /// Emit `LOAD_FIELD_U128 account_index_u8 offset_vle` — reads 16 LE bytes
    /// from the account and pushes a `ValueRef::U128`. Use for u128 fields
    /// (vault, insurance_fund, capital, …) and i128 fields (the bit pattern is
    /// identical; callers interpret).
    pub fn emit_load_field_u128(&mut self, account_index: u8, offset: u64) -> &mut Self {
        self.body.push(LOAD_FIELD_U128);
        self.body.push(account_index);
        write_vle_u64(&mut self.body, offset);
        self
    }

    /// Emit `STORE_FIELD_U128 account_index_u8 offset_vle` — pops a U128 and
    /// writes 16 LE bytes. Errors at runtime if the popped value isn't U128.
    pub fn emit_store_field_u128(&mut self, account_index: u8, offset: u64) -> &mut Self {
        self.body.push(STORE_FIELD_U128);
        self.body.push(account_index);
        write_vle_u64(&mut self.body, offset);
        self
    }

    /// Emit `LOAD_FIELD_U16 account_index_u8 offset_vle` — reads 2 LE bytes
    /// and pushes the zero-extended value as `ValueRef::U64` (no native U16
    /// variant).
    pub fn emit_load_field_u16(&mut self, account_index: u8, offset: u64) -> &mut Self {
        self.body.push(LOAD_FIELD_U16);
        self.body.push(account_index);
        write_vle_u64(&mut self.body, offset);
        self
    }

    /// Emit `STORE_FIELD_U16 account_index_u8 offset_vle` — pops a u64 and
    /// writes its low 2 bytes. High 48 bits are discarded.
    pub fn emit_store_field_u16(&mut self, account_index: u8, offset: u64) -> &mut Self {
        self.body.push(STORE_FIELD_U16);
        self.body.push(account_index);
        write_vle_u64(&mut self.body, offset);
        self
    }

    // ------------------------------------------------------------------------
    // Multi-precision opcodes (see five-vm-mito/src/handlers/multiprecision.rs)
    //
    // Each takes a 1-byte flags immediate. Bit 0 = checked mode. All binary ops
    // pop 8 u64 limbs (two u256s) and push 4; CMP pushes 1; MULDIV pops 12.
    // ------------------------------------------------------------------------

    pub fn emit_add_u256(&mut self, flags: u8) -> &mut Self {
        self.body.extend_from_slice(&[ADD_U256, flags]);
        self
    }

    pub fn emit_sub_u256(&mut self, flags: u8) -> &mut Self {
        self.body.extend_from_slice(&[SUB_U256, flags]);
        self
    }

    pub fn emit_mul_u256(&mut self, flags: u8) -> &mut Self {
        self.body.extend_from_slice(&[MUL_U256, flags]);
        self
    }

    pub fn emit_div_u256(&mut self, flags: u8) -> &mut Self {
        self.body.extend_from_slice(&[DIV_U256, flags]);
        self
    }

    pub fn emit_cmp_u256(&mut self, flags: u8) -> &mut Self {
        self.body.extend_from_slice(&[CMP_U256, flags]);
        self
    }

    pub fn emit_muldiv_u256(&mut self, flags: u8) -> &mut Self {
        self.body.extend_from_slice(&[MULDIV_U256, flags]);
        self
    }

    // ------------------------------------------------------------------------
    // Control flow — JUMP, JUMP_IF, JUMP_IF_NOT
    //
    // Targets are absolute offsets into the FULL bytecode (10-byte header +
    // body). Forward jumps are emitted with a placeholder via
    // `emit_jump_*_placeholder`, which returns the byte offset of the 2-byte
    // operand inside `body`. Once the target position is known, call
    // [`Program::patch_jump_to_here`] to fill it in.
    // ------------------------------------------------------------------------

    /// Emit `JUMP_IF target_u16`, returning the offset (in `body`) of the
    /// 2-byte operand. Pop the top stack value; jump iff truthy.
    pub fn emit_jump_if_placeholder(&mut self) -> JumpPatch {
        self.body.push(JUMP_IF);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        JumpPatch { patch_at }
    }

    /// Emit `JUMP_IF_NOT target_u16`, returning the patch handle.
    pub fn emit_jump_if_not_placeholder(&mut self) -> JumpPatch {
        self.body.push(JUMP_IF_NOT);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        JumpPatch { patch_at }
    }

    /// Emit `JUMP target_u16`, returning the patch handle.
    pub fn emit_jump_placeholder(&mut self) -> JumpPatch {
        self.body.push(JUMP);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        JumpPatch { patch_at }
    }

    /// Patch a previously-reserved jump's target to point at the current emit
    /// position (i.e. where the next opcode will be placed). Target is the
    /// absolute IP, so it includes the 10-byte header.
    pub fn patch_jump_to_here(&mut self, patch: JumpPatch) {
        let absolute = self.body.len() + 10; // header is 10 bytes
        let target = u16::try_from(absolute).expect("jump target overflows u16");
        let bytes = target.to_le_bytes();
        self.body[patch.patch_at] = bytes[0];
        self.body[patch.patch_at + 1] = bytes[1];
    }

    // ------------------------------------------------------------------------
    // CALL — encoded as `CALL[1] param_count[1] func_addr[2 LE]`.
    //
    // `func_addr` is an absolute byte offset into the running script. With
    // `param_count = 0`, no values are popped from the caller's stack and the
    // callee runs against the caller's stack directly. RETURN_VALUE leaves the
    // top-of-stack alone on the way back, so the callee's last computation
    // becomes the caller's next stack value.
    // ------------------------------------------------------------------------

    /// Emit `CALL param_count <placeholder>`. Returns a [`CallPatch`] handle
    /// that can later be patched with [`Program::patch_call_target`] or via
    /// the linker's `patch_call_target` helper (which works on the absolute
    /// position in the final linked binary).
    pub fn emit_call_placeholder(&mut self, param_count: u8) -> CallPatch {
        self.body.push(CALL);
        self.body.push(param_count);
        let patch_at = self.body.len();
        self.body.extend_from_slice(&[0, 0]);
        CallPatch { patch_at }
    }

    /// Patch a CALL site whose handle came from this same `Program` to target
    /// the given absolute address. For cross-program (linker-time) patching
    /// see `link::Linker::patch_call_target`.
    pub fn patch_call_target(&mut self, patch: CallPatch, absolute_target: u16) {
        let bytes = absolute_target.to_le_bytes();
        self.body[patch.patch_at] = bytes[0];
        self.body[patch.patch_at + 1] = bytes[1];
    }

    /// The current position where the next opcode would be emitted, expressed
    /// as an **absolute IP** (i.e. body length + the 10-byte header). Useful
    /// when capturing function entry points for later CALL patching.
    pub fn current_absolute_offset(&self) -> u16 {
        u16::try_from(self.body.len() + 10).expect("absolute offset overflows u16")
    }

    /// Absolute byte offset of a captured `CallPatch` site (the position of
    /// its `CALL` opcode byte, NOT the operand). Needed by the linker to
    /// rewrite call sites once functions are appended after build time.
    pub fn absolute_call_site(&self, patch: CallPatch) -> u16 {
        // patch.patch_at points at the start of the operand. The CALL byte
        // sits two bytes earlier (CALL + param_count).
        u16::try_from(patch.patch_at - 2 + 10).expect("call-site offset overflows u16")
    }

    // ------------------------------------------------------------------------
    // Termination
    // ------------------------------------------------------------------------

    /// Append `HALT` (0x00) and return the full `.five` binary with header.
    /// `HALT` leaves the top-of-stack as the program's return value, which
    /// `MitoVM::execute_direct` surfaces as `Option<Value>`.
    pub fn finish_with_halt(mut self) -> Vec<u8> {
        self.body.push(HALT);
        prepend_five_header(self.body)
    }

    /// Consume the program and return the raw body bytes — no header, no
    /// trailing HALT. Use this for bytes that will be appended inside another
    /// binary by the linker (where the outer `.five` file already has its
    /// header, and the appended callee ends with `RETURN_VALUE`, not `HALT`).
    pub fn into_body(self) -> Vec<u8> {
        self.body
    }

    /// Current body length (without header). Useful when computing relative
    /// jump targets inside an in-progress appended function.
    pub fn body_len(&self) -> usize {
        self.body.len()
    }
}

/// Write a u64 in VLE (LEB128) encoding: 7 bits per byte, continuation bit in MSB.
/// Mirrors `five_vm_mito::context::ExecutionManager::fetch_vle_u64`.
pub(crate) fn write_vle_u64(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Prepend the 10-byte 5IVE header that every `.five` file needs:
///   magic(4) = b"5IVE"
///   features(4 LE) = 0
///   public_function_count(1) = 0
///   total_function_count(1) = 0
fn prepend_five_header(body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 10);
    out.extend_from_slice(b"5IVE");
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // features
    out.push(0x00); // public_function_count
    out.push(0x00); // total_function_count
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vle_roundtrip_small() {
        let mut buf = Vec::new();
        write_vle_u64(&mut buf, 42);
        assert_eq!(buf, vec![42]);
    }

    #[test]
    fn vle_roundtrip_continuation() {
        let mut buf = Vec::new();
        write_vle_u64(&mut buf, 128);
        assert_eq!(buf, vec![0x80, 0x01]); // 128 = 0b10000000 → 0x80 cont, 0x01
    }

    #[test]
    fn vle_roundtrip_u64_max() {
        let mut buf = Vec::new();
        write_vle_u64(&mut buf, u64::MAX);
        // u64::MAX is 64 bits → 10 bytes in VLE (9 full + 1 partial)
        assert_eq!(buf.len(), 10);
        assert_eq!(buf.last(), Some(&0x01));
    }

    #[test]
    fn jump_patch_writes_absolute_target() {
        // Layout (body offsets in parens, +10 for absolute IP):
        //   (0) PUSH_U64 1               → byte at 0,1
        //   (2) JUMP_IF [patch:3,4]      → patch_at=3
        //   (5) PUSH_U64 7               → 5,6
        //   (7) HALT (target lands here) → absolute 17
        let mut p = Program::new();
        p.push_u64(1);
        let jp = p.emit_jump_if_placeholder();
        p.push_u64(7);
        p.patch_jump_to_here(jp);
        let bin = p.finish_with_halt();
        // Locate the patched bytes inside the body (offset 3, 4 → +10 in bin).
        let target = u16::from_le_bytes([bin[10 + 3], bin[10 + 4]]);
        // Expected: end of body at the moment patch_jump_to_here was called
        // = 7 (PUSH_U64 1 + JUMP_IF + 2-byte operand + PUSH_U64 7) + 10 header.
        assert_eq!(target, 17);
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
