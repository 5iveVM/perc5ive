//! Bytecode linker — append hand-written functions to a `.five` binary.
//!
//! # Why
//!
//! The `five-dsl-compiler` produces a `.five` binary whose `CALL` instructions
//! carry **absolute byte offsets** into that same binary. Hand-written
//! bytecode (e.g. the u256/i256 sequences in this crate) lives outside the DSL
//! and needs to be merged into a single binary the VM can run.
//!
//! The linker takes a base binary and appends functions to its tail, returning
//! the absolute offset of each appended function so callers can patch the
//! corresponding `CALL` site. Because we only ever **append** (never insert)
//! and never modify the existing body, every pre-existing CALL target in the
//! base binary remains valid — the only edits are to the call sites the user
//! explicitly hands us.
//!
//! # Wire-level model
//!
//! - `.five` layout: `header(10) | optional metadata | body`.
//! - VM's `CALL` handler (`five-vm-mito/src/handlers/functions.rs:79`) fetches
//!   the target as a raw little-endian `u16` and validates `target < script.len()`.
//! - With `param_count = 0`, the callee runs against the caller's stack
//!   directly — exactly what the multiprecision sequences want.
//! - `RETURN_VALUE` leaves the top stack value alone on the way back
//!   (`control_flow.rs:221`), so a callee that ends with `... <result> RETURN_VALUE`
//!   surfaces `<result>` as the caller's next stack value.
//!
//! # What this linker does NOT do (yet)
//!
//! - Does not parse or extend the `FUNCTION_NAMES` metadata section. The
//!   appended functions remain anonymous (callable by offset). Once we want
//!   tooling to surface them by name, we'll have to grow the metadata section
//!   — which requires re-patching every existing CALL target in the base
//!   binary by the size of the metadata growth.
//! - Does not bump `total_function_count` in the header. The runtime VM does
//!   not consult that field for CALL dispatch (validated against
//!   `script.len()` only); the off-line `parse_optimized_bytecode` analyzer
//!   does, but that analyzer is not on the runtime path.

use super::emit::CallPatch;
use five_protocol::opcodes::{CALL, JUMP, NOP, PUSH_U64, PUSH_U64_W, RETURN_VALUE};
use five_protocol::{
    FEATURE_CONSTANT_POOL, FEATURE_FUNCTION_NAMES, FEATURE_PUBLIC_ENTRY_TABLE,
    SCRIPT_BYTECODE_HEADER_V1_SIZE,
};

/// Mutable working buffer for an in-progress link.
pub struct Linker {
    binary: Vec<u8>,
}

/// Reference to a function the linker has appended. The `offset` field is the
/// absolute byte offset of the function's first opcode in the linked binary,
/// suitable for stuffing into a `CALL` operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendedFn {
    pub offset: u16,
}

impl Linker {
    /// Wrap a base binary that the linker will append functions to.
    /// The base must already include the 5IVE header — typically produced by
    /// [`crate::bytecode::Program::finish_with_halt`] or by `five-dsl-compiler`.
    pub fn from_base(base: &[u8]) -> Self {
        Self {
            binary: base.to_vec(),
        }
    }

    /// Append a hand-written bytecode body to the binary. The body is inserted
    /// verbatim and **must** end with `RETURN`, `RETURN_VALUE`, or another
    /// instruction that yields control back to the caller — otherwise the VM
    /// will fall off the end of the bytecode and trap.
    pub fn append_function(&mut self, body: &[u8]) -> AppendedFn {
        let offset = u16::try_from(self.binary.len())
            .expect("linker: binary too large for a u16 absolute CALL target");
        self.binary.extend_from_slice(body);
        AppendedFn { offset }
    }

    /// Append a body that contains body-relative jumps (see
    /// [`crate::bytecode::emit::Program::emit_jump_placeholder_body_relative`]).
    /// Each offset in `jump_patch_offsets_in_body` points at a 2-byte LE target
    /// inside `body`; the linker rewrites each to `append_offset + stored_u16`
    /// so the jump lands at the correct absolute IP.
    pub fn append_function_with_body_relative_jumps(
        &mut self,
        body: &[u8],
        jump_patch_offsets_in_body: &[usize],
    ) -> AppendedFn {
        let append_offset = u16::try_from(self.binary.len())
            .expect("linker: binary too large for a u16 absolute CALL target");
        let body_start = self.binary.len();
        self.binary.extend_from_slice(body);
        for &rel in jump_patch_offsets_in_body {
            let abs_at = body_start + rel;
            let stored = u16::from_le_bytes([self.binary[abs_at], self.binary[abs_at + 1]]);
            let final_target = append_offset
                .checked_add(stored)
                .expect("linker: body-relative jump target overflows u16");
            let bytes = final_target.to_le_bytes();
            self.binary[abs_at] = bytes[0];
            self.binary[abs_at + 1] = bytes[1];
        }
        AppendedFn { offset: append_offset }
    }

    /// Patch a CALL site in the base binary whose absolute offset (the `CALL`
    /// opcode byte itself) is known, redirecting it to an appended function.
    ///
    /// `call_site_abs` is the offset of the `CALL` byte in the **linked**
    /// binary (which equals the base binary's offset for any site that lived
    /// in the base, since we only ever append). For CALL sites emitted by
    /// [`crate::bytecode::Program`], use `Program::absolute_call_site` to
    /// recover that offset before consuming the program.
    pub fn patch_call_target(&mut self, call_site_abs: u16, target: AppendedFn) {
        // Layout at `call_site_abs`: [CALL=0x90][param_count u8][target u16 LE]
        let target_at = call_site_abs as usize + 2;
        debug_assert_eq!(
            self.binary[call_site_abs as usize],
            five_protocol::opcodes::CALL,
            "linker: patch_call_target: byte at call_site_abs is not the CALL opcode",
        );
        let bytes = target.offset.to_le_bytes();
        self.binary[target_at] = bytes[0];
        self.binary[target_at + 1] = bytes[1];
    }

    /// Convenience: take a [`CallPatch`] handle from the source `Program`'s
    /// builder and a base offset (where that program landed in the linked
    /// binary), compute the absolute call-site offset, and patch.
    /// For programs that ARE the base (placed at offset 0), just pass 0.
    pub fn patch_call_target_from_handle(
        &mut self,
        program_base_offset: u16,
        handle_relative_call_site: u16,
        target: AppendedFn,
    ) {
        let abs = program_base_offset
            .checked_add(handle_relative_call_site)
            .expect("linker: call-site offset overflows u16");
        self.patch_call_target(abs, target);
    }

    /// Consume the linker and return the final binary.
    pub fn into_bytes(self) -> Vec<u8> {
        self.binary
    }

    /// Length of the linked binary so far. Useful for predicting the offset
    /// of the **next** function before appending.
    pub fn len(&self) -> usize {
        self.binary.len()
    }

    pub fn is_empty(&self) -> bool {
        self.binary.is_empty()
    }

    // ------------------------------------------------------------------------
    // Sentinel-based stub rewriting
    //
    // The DSL has no "extern fn" or "@link" decorator yet, so the convention
    // used here is: the DSL author writes a stub function whose body returns
    // a unique `u64` sentinel:
    //
    //     pub fn _u256_add(a_lo: u64, a_hi: u64, ...) -> u64 {
    //         return 0xFEED_FACE_DEAD_C0DE; // sentinel for u256_add
    //     }
    //
    // `five-dsl-compiler` lowers this body to:
    //     PUSH_U64 <VLE-encoded sentinel>
    //     RETURN_VALUE
    //
    // The linker scans for that exact byte sequence, then **overwrites in
    // place** with `CALL 0 <appended_offset>` followed by `RETURN_VALUE`,
    // padding the rest of the slot with `NOP` so the binary's overall length
    // (and therefore every other CALL target offset) is preserved.
    //
    // The caller is responsible for pre-pushing CALL parameters onto the
    // stack INSIDE the DSL stub if they want them passed via the parameter
    // table. Because the appended bytecode operates on the value stack
    // directly (not the param table), the typical use is to declare the stub
    // with the operand types as `u64` arguments and load them into the value
    // stack before calling — but for now most of the multiprecision sequences
    // expect operands already on the stack from the caller. The shape of the
    // calling convention will firm up as the first DSL-side use case lands.
    // ------------------------------------------------------------------------

    /// Compute the exact byte sequence that `five-dsl-compiler` emits for the
    /// inline `return <sentinel>;` form: `PUSH_U64 <8 LE> RETURN_VALUE` =
    /// 10 bytes. Used when the binary's `FEATURE_CONSTANT_POOL` bit is *off*
    /// or when the sentinel didn't make it into the pool (small values
    /// usually inline; the compiler emits the wide pool-backed `PUSH_U64_W`
    /// form for u64 literals whose encoded size makes inlining worse).
    fn inline_stub_pattern(sentinel: u64) -> Vec<u8> {
        let mut bytes = vec![PUSH_U64];
        bytes.extend_from_slice(&sentinel.to_le_bytes());
        bytes.push(RETURN_VALUE);
        bytes
    }

    /// Compute the exact byte sequence for the pool-backed `return <sentinel>;`
    /// form. Mono's `PUSH_U64` (0x1B) opcode is polymorphic on the pool
    /// feature flag: with the pool enabled the operand is a u8 pool index,
    /// without it the operand is 8 LE bytes of the literal value. We
    /// produce the *compact-u8* pool form here: `PUSH_U64 <u8 idx>
    /// RETURN_VALUE` = 3 bytes. Used when the constant pool exists and the
    /// sentinel slot fits in a u8 (i.e. there are ≤ 256 pool slots).
    fn pool_stub_pattern_u8(pool_idx: u8) -> [u8; 3] {
        [PUSH_U64, pool_idx, RETURN_VALUE]
    }

    /// Wider pool-backed form: `PUSH_U64_W <u16 idx LE> RETURN_VALUE` =
    /// 4 bytes. Emitted by the compiler when the pool index doesn't fit in
    /// a u8 (≥ 256 slots), and accepted by the VM regardless of feature
    /// flag because the opcode itself encodes pool semantics.
    fn pool_stub_pattern_u16(pool_idx: u16) -> [u8; 4] {
        let idx = pool_idx.to_le_bytes();
        [PUSH_U64_W, idx[0], idx[1], RETURN_VALUE]
    }

    /// Backwards-compatible accessor returning the *inline* stub pattern.
    /// Kept for the existing unit tests that hand-roll an inline binary
    /// without a constant pool. New code should use the typed forms above.
    #[doc(hidden)]
    pub fn stub_pattern(sentinel: u64) -> Vec<u8> {
        Self::inline_stub_pattern(sentinel)
    }

    /// Parse the script header to find `(pool_offset, pool_slots)` if a
    /// constant pool is present. Returns `None` if the binary doesn't have
    /// `FEATURE_CONSTANT_POOL` set (or the header doesn't parse) — the
    /// caller falls back to the inline pattern in that case.
    ///
    /// Mirrors the offset walk in `five_protocol::parser::parse_code_bounds`
    /// so the linker can find the `ConstantPoolDescriptor` directly. The
    /// descriptor type itself is `pub(crate)` upstream; we read its 14
    /// public bytes by hand (u32 pool_offset, u32 string_offset,
    /// u32 string_len, u16 pool_slots) and discard the reserved tail.
    fn parse_pool_window(binary: &[u8]) -> Option<(usize, u16)> {
        if binary.len() < SCRIPT_BYTECODE_HEADER_V1_SIZE {
            return None;
        }
        let features = u32::from_le_bytes([binary[4], binary[5], binary[6], binary[7]]);
        if features & FEATURE_CONSTANT_POOL == 0 {
            return None;
        }

        let mut offset = SCRIPT_BYTECODE_HEADER_V1_SIZE;

        // FEATURE_FUNCTION_NAMES adds a 2-byte size prefix + names blob.
        if features & FEATURE_FUNCTION_NAMES != 0 {
            if offset + 2 > binary.len() {
                return None;
            }
            let section_size =
                u16::from_le_bytes([binary[offset], binary[offset + 1]]) as usize;
            offset = offset.checked_add(2)?.checked_add(section_size)?;
        }

        // FEATURE_PUBLIC_ENTRY_TABLE adds another 2-byte size prefix + table.
        if features & FEATURE_PUBLIC_ENTRY_TABLE != 0 {
            if offset + 2 > binary.len() {
                return None;
            }
            let section_size =
                u16::from_le_bytes([binary[offset], binary[offset + 1]]) as usize;
            offset = offset.checked_add(2)?.checked_add(section_size)?;
        }

        // ConstantPoolDescriptor: u32 pool_offset (0..4), u32 string_offset
        // (4..8), u32 string_len (8..12), u16 pool_slots (12..14), u16
        // reserved (14..16). 16 bytes total.
        if offset + 14 > binary.len() {
            return None;
        }
        let pool_offset = u32::from_le_bytes([
            binary[offset],
            binary[offset + 1],
            binary[offset + 2],
            binary[offset + 3],
        ]) as usize;
        let pool_slots = u16::from_le_bytes([binary[offset + 12], binary[offset + 13]]);
        Some((pool_offset, pool_slots))
    }

    /// Linear scan of the constant pool for a u64 value, returning its slot
    /// index. Slots are 8 bytes each, little-endian. Returns `None` if the
    /// sentinel is absent (the compiler chose the inline form for it).
    fn find_pool_slot(binary: &[u8], sentinel: u64) -> Option<u16> {
        let (pool_offset, pool_slots) = Self::parse_pool_window(binary)?;
        let needle = sentinel.to_le_bytes();
        for i in 0..(pool_slots as usize) {
            let start = pool_offset + i * 8;
            if start + 8 > binary.len() {
                return None;
            }
            if binary[start..start + 8] == needle {
                return Some(i as u16);
            }
        }
        None
    }

    /// Locate the unique offset of the stub body for `sentinel`. Tries the
    /// two pool-backed forms first (mono-default for u64 literals when
    /// `FEATURE_CONSTANT_POOL` is set), then falls back to the inline form.
    /// Returns `None` if none match.
    ///
    /// `Err(StubFindError::Ambiguous)` is reserved for *more than one form
    /// matching* or *any form matching more than once* — pick a less
    /// common sentinel.
    pub fn find_stub(&self, sentinel: u64) -> Result<Option<usize>, StubFindError> {
        let mut found: Option<usize> = None;

        if let Some(pool_idx) = Self::find_pool_slot(&self.binary, sentinel) {
            // Pool-backed compact form: `PUSH_U64 <u8 idx> RETURN_VALUE`
            // = 3 bytes. Compiler picks this when the pool index fits a
            // u8 (which is the common case — pools rarely exceed 256
            // slots in practice).
            if pool_idx <= u8::MAX as u16 {
                let needle = Self::pool_stub_pattern_u8(pool_idx as u8);
                for window_start in self.scan_for_pattern(&needle) {
                    if found.is_some() {
                        return Err(StubFindError::Ambiguous);
                    }
                    found = Some(window_start);
                }
            }

            // Pool-backed wide form: `PUSH_U64_W <u16 idx LE> RETURN_VALUE`
            // = 4 bytes. Used when the pool index doesn't fit a u8.
            let needle_w = Self::pool_stub_pattern_u16(pool_idx);
            for window_start in self.scan_for_pattern(&needle_w) {
                if found.is_some() {
                    return Err(StubFindError::Ambiguous);
                }
                found = Some(window_start);
            }
        }

        // Inline: 10-byte `PUSH_U64 <8 LE> RETURN_VALUE`. Used when the
        // pool isn't enabled at all (legacy / hand-rolled test binaries).
        let inline = Self::inline_stub_pattern(sentinel);
        for window_start in self.scan_for_pattern(&inline) {
            if found.is_some() {
                return Err(StubFindError::Ambiguous);
            }
            found = Some(window_start);
        }

        Ok(found)
    }

    /// Internal helper: scan `self.binary` for every occurrence of `needle`.
    /// Returns an iterator-like Vec because find_stub needs to count matches
    /// across two patterns; a real iterator would borrow self.
    fn scan_for_pattern(&self, needle: &[u8]) -> Vec<usize> {
        let mut hits = Vec::new();
        if needle.is_empty() || needle.len() > self.binary.len() {
            return hits;
        }
        for window_start in 0..=self.binary.len() - needle.len() {
            if &self.binary[window_start..window_start + needle.len()] == needle {
                hits.push(window_start);
            }
        }
        hits
    }

    /// Identify which stub form lives at `offset` (must point at a `PUSH_U64`
    /// or `PUSH_U64_W` opcode). Returned length is the slot size we must
    /// preserve when rewriting. The pool-compact and inline forms share an
    /// opcode (`PUSH_U64`); the binary's `FEATURE_CONSTANT_POOL` bit
    /// disambiguates them.
    fn stub_form_at(&self, offset: usize) -> Option<StubForm> {
        let opcode = *self.binary.get(offset)?;
        let pool_active = Self::parse_pool_window(&self.binary).is_some();
        match opcode {
            PUSH_U64 if pool_active
                && offset + 3 <= self.binary.len()
                && self.binary[offset + 2] == RETURN_VALUE =>
            {
                // Compact pool form: 0x1B <u8 idx> 0x07 = 3 bytes.
                Some(StubForm::PoolBackedU8 { len: 3 })
            }
            PUSH_U64 if !pool_active
                && offset + 10 <= self.binary.len()
                && self.binary[offset + 9] == RETURN_VALUE =>
            {
                Some(StubForm::Inline { len: 10 })
            }
            PUSH_U64_W if offset + 4 <= self.binary.len()
                && self.binary[offset + 3] == RETURN_VALUE =>
            {
                Some(StubForm::PoolBackedU16 { len: 4 })
            }
            _ => None,
        }
    }

    /// Rewrite a stub body in-place. The 10-byte inline form is replaced by
    /// `CALL 0 <target_le> RETURN_VALUE` + NOP padding so the appended body
    /// is called as a sub-function and control returns to the stub for its
    /// own RETURN_VALUE.
    ///
    /// The 4-byte pool-backed form is replaced by `JUMP <target_le>` + one
    /// NOP — control transfers directly to the appended body, whose terminal
    /// `RETURN_VALUE` returns from the STUB's frame (still on the call stack
    /// from the outer caller's CALL). Frame counting stays balanced because
    /// JUMP does not push a new frame; the single RETURN_VALUE pops the
    /// stub's frame as expected.
    ///
    /// `param_count = 0` is intentional for the CALL path: the appended
    /// bytecode runs on the caller's value stack directly.
    pub fn rewrite_stub(
        &mut self,
        sentinel: u64,
        target: AppendedFn,
    ) -> Result<(), StubFindError> {
        let stub_offset = self
            .find_stub(sentinel)?
            .ok_or(StubFindError::NotFound)?;
        let form = self
            .stub_form_at(stub_offset)
            .ok_or(StubFindError::NotFound)?;

        let target_le = target.offset.to_le_bytes();

        match form {
            StubForm::Inline { len } => {
                // CALL 0 target_lo target_hi RETURN_VALUE [NOP...] = 5 + padding
                let mut new_body = vec![CALL, 0, target_le[0], target_le[1], RETURN_VALUE];
                if new_body.len() > len {
                    return Err(StubFindError::PatternTooSmall);
                }
                while new_body.len() < len {
                    new_body.push(NOP);
                }
                self.binary[stub_offset..stub_offset + len].copy_from_slice(&new_body);
            }
            StubForm::PoolBackedU8 { len } => {
                // JUMP target_lo target_hi = 3 bytes, exact fit. The
                // appended body's RETURN_VALUE returns from the stub frame
                // (still on the call stack from the outer caller's CALL),
                // so frame counting stays balanced.
                let new_body = [JUMP, target_le[0], target_le[1]];
                debug_assert_eq!(new_body.len(), len);
                self.binary[stub_offset..stub_offset + len].copy_from_slice(&new_body);
            }
            StubForm::PoolBackedU16 { len } => {
                // JUMP target_lo target_hi NOP = 4 bytes, exact fit.
                let new_body = [JUMP, target_le[0], target_le[1], NOP];
                debug_assert_eq!(new_body.len(), len);
                self.binary[stub_offset..stub_offset + len].copy_from_slice(&new_body);
            }
        }
        Ok(())
    }
}

/// Internal: which encoding form a sentinel stub uses. Each form has its
/// own slot length so the rewriter picks a same-length replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StubForm {
    /// `PUSH_U64 <8 LE> RETURN_VALUE` = 10 bytes. Used when the constant
    /// pool feature is disabled (legacy / hand-rolled test binaries).
    Inline { len: usize },
    /// `PUSH_U64 <u8 idx> RETURN_VALUE` = 3 bytes. Polymorphic-PUSH_U64 in
    /// pool-enabled mode reads a u8 pool index. This is the form mono's
    /// compiler emits for pooled u64 literals.
    PoolBackedU8 { len: usize },
    /// `PUSH_U64_W <u16 idx LE> RETURN_VALUE` = 4 bytes. Emitted when the
    /// pool index exceeds u8 range.
    PoolBackedU16 { len: usize },
}

/// Errors returned when locating or rewriting a sentinel stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubFindError {
    /// Sentinel was not found in the binary. Either the DSL stub is missing
    /// or the sentinel constant doesn't match.
    NotFound,
    /// The sentinel byte sequence appears more than once. Pick a constant
    /// less likely to collide (e.g. one with random-looking bits).
    Ambiguous,
    /// The stub body byte slot is too small to fit `CALL ... RETURN_VALUE`.
    /// Use a larger (more bytes after VLE encoding) sentinel.
    PatternTooSmall,
}

/// Reserve the same kind of placeholder for a CALL emitted into a sub-program
/// that is destined to be appended (not the base). The returned absolute
/// offset is **relative to the appended function's start** at the time of
/// emission, so callers must add the function's `AppendedFn::offset` before
/// patching.
///
/// In practice this is just `CallPatch::patch_at - 2 + 10` translated into the
/// appended function's coordinate space — provided here as documentation.
pub fn patch_relative_call_site_in_appended(_handle: CallPatch) -> u16 {
    unimplemented!(
        "Cross-program CALL patching from inside an appended function is not yet \
         needed by Perc5ive. Add when we want appended functions to call each \
         other by name. For now compose them as one larger appended body."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::emit::Program;
    use five_protocol::opcodes::HALT;

    #[test]
    fn append_only_grows_the_binary() {
        let mut p = Program::new();
        p.push_u64(42);
        let base = p.finish_with_halt();
        let base_len = base.len();
        let mut linker = Linker::from_base(&base);
        let f = linker.append_function(&[PUSH_U64, 0x05, RETURN_VALUE]);
        let out = linker.into_bytes();
        assert_eq!(f.offset as usize, base_len);
        assert_eq!(&out[..base_len], &base[..]);
        assert_eq!(&out[base_len..], &[PUSH_U64, 0x05, RETURN_VALUE]);
    }

    #[test]
    fn patch_call_target_writes_le_u16_at_correct_offset() {
        let mut p = Program::new();
        let _patch = p.emit_call_placeholder(0);
        let base = {
            let mut body = p;
            body.raw(HALT);
            body.finish_with_halt()
        };
        let mut linker = Linker::from_base(&base);
        let call_site = 10u16;
        let f = AppendedFn { offset: 0x1234 };
        linker.patch_call_target(call_site, f);
        let out = linker.into_bytes();
        assert_eq!(out[10], five_protocol::opcodes::CALL);
        assert_eq!(out[11], 0x00); // param_count
        assert_eq!(out[12], 0x34); // target lo
        assert_eq!(out[13], 0x12); // target hi
    }

    const SENTINEL: u64 = 0xFEED_FACE_DEAD_C0DE;

    #[test]
    fn find_stub_locates_exact_sentinel() {
        let mut p = Program::new();
        p.push_u64(SENTINEL);
        p.raw(RETURN_VALUE);
        let base = p.finish_with_halt();
        let linker = Linker::from_base(&base);
        let found = linker.find_stub(SENTINEL).expect("find_stub OK");
        assert!(found.is_some(), "should find the sentinel");
        // Offset must be inside the binary, past the 10-byte header.
        assert!(found.unwrap() >= 10);
    }

    #[test]
    fn find_stub_returns_none_if_absent() {
        let mut p = Program::new();
        p.push_u64(42);
        let base = p.finish_with_halt();
        let linker = Linker::from_base(&base);
        assert_eq!(linker.find_stub(SENTINEL).unwrap(), None);
    }

    #[test]
    fn find_stub_returns_ambiguous_on_duplicates() {
        let mut p = Program::new();
        p.push_u64(SENTINEL);
        p.raw(RETURN_VALUE);
        p.push_u64(SENTINEL);
        p.raw(RETURN_VALUE);
        let base = p.finish_with_halt();
        let linker = Linker::from_base(&base);
        assert_eq!(linker.find_stub(SENTINEL).unwrap_err(), StubFindError::Ambiguous);
    }

    #[test]
    fn rewrite_stub_preserves_binary_length() {
        let mut p = Program::new();
        p.push_u64(SENTINEL);
        p.raw(RETURN_VALUE);
        let base = p.finish_with_halt();
        let base_len = base.len();
        let mut linker = Linker::from_base(&base);
        let callee = linker.append_function(&[PUSH_U64, 0x07, RETURN_VALUE]);
        // Binary grew by the appended function.
        let pre_rewrite_len = linker.len();
        linker.rewrite_stub(SENTINEL, callee).expect("rewrite_stub OK");
        // The rewrite does not change length.
        assert_eq!(linker.len(), pre_rewrite_len);
        // The base portion is still the same length.
        let out = linker.into_bytes();
        assert_eq!(out.len(), base_len + 3); // 3 = appended callee bytes
    }

    #[test]
    fn rewrite_stub_places_call_and_nop_padding() {
        let mut p = Program::new();
        p.push_u64(SENTINEL);
        p.raw(RETURN_VALUE);
        let base = p.finish_with_halt();
        let mut linker = Linker::from_base(&base);
        let callee = linker.append_function(&[PUSH_U64, 0x07, RETURN_VALUE]);
        let stub_offset = linker.find_stub(SENTINEL).unwrap().unwrap();
        linker.rewrite_stub(SENTINEL, callee).unwrap();
        let out = linker.into_bytes();
        assert_eq!(out[stub_offset], CALL);
        assert_eq!(out[stub_offset + 1], 0x00); // param_count
        let target = u16::from_le_bytes([out[stub_offset + 2], out[stub_offset + 3]]);
        assert_eq!(target, callee.offset);
        assert_eq!(out[stub_offset + 4], RETURN_VALUE);
        // Everything from stub_offset+5 to end of the stub is NOP.
        let pattern_len = Linker::stub_pattern(SENTINEL).len();
        for i in 5..pattern_len {
            assert_eq!(out[stub_offset + i], NOP, "byte at offset {} should be NOP", stub_offset + i);
        }
    }
}
