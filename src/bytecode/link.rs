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
use five_protocol::opcodes::{CALL, NOP, PUSH_U64, RETURN_VALUE};

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
    /// stub body `return <sentinel>;`. Mono uses fixed-width encoding: the
    /// sentinel is 8 LE bytes after the opcode.
    fn stub_pattern(sentinel: u64) -> Vec<u8> {
        let mut bytes = vec![PUSH_U64];
        bytes.extend_from_slice(&sentinel.to_le_bytes());
        bytes.push(RETURN_VALUE);
        bytes
    }

    /// Locate the unique offset of the stub body for the given sentinel.
    ///
    /// Returns `None` if the pattern is not present, and `Err(StubFindError::Ambiguous)`
    /// if it appears more than once (a sentinel collision — pick a less common
    /// constant).
    pub fn find_stub(&self, sentinel: u64) -> Result<Option<usize>, StubFindError> {
        let needle = Self::stub_pattern(sentinel);
        let mut found: Option<usize> = None;
        if needle.len() > self.binary.len() {
            return Ok(None);
        }
        for window_start in 0..=self.binary.len() - needle.len() {
            if &self.binary[window_start..window_start + needle.len()] == &needle[..] {
                if found.is_some() {
                    return Err(StubFindError::Ambiguous);
                }
                found = Some(window_start);
            }
        }
        Ok(found)
    }

    /// Rewrite a stub body in-place: replace its `PUSH_U64 sentinel; RETURN_VALUE`
    /// sequence with `CALL 0 <appended target>; RETURN_VALUE; NOP*` so the
    /// containing function calls into the appended hand-written bytecode and
    /// then returns. The slot's byte length is preserved so all other CALL
    /// targets in the base binary remain valid.
    ///
    /// `param_count = 0` is intentional: the appended bytecode runs on the
    /// caller's value stack directly. If you need the DSL stub to first
    /// arrange operands on the value stack, do so via a richer stub body
    /// (and a correspondingly larger sentinel slot to overwrite) — the
    /// shape this should take will be decided when wired up to the DSL.
    pub fn rewrite_stub(
        &mut self,
        sentinel: u64,
        target: AppendedFn,
    ) -> Result<(), StubFindError> {
        let stub_offset = self
            .find_stub(sentinel)?
            .ok_or(StubFindError::NotFound)?;
        let pattern_len = Self::stub_pattern(sentinel).len();
        // New body: CALL 0 target_lo target_hi RETURN_VALUE [NOP...]
        let mut new_body = Vec::with_capacity(pattern_len);
        new_body.push(CALL);
        new_body.push(0); // param_count
        let bytes = target.offset.to_le_bytes();
        new_body.push(bytes[0]);
        new_body.push(bytes[1]);
        new_body.push(RETURN_VALUE);
        if new_body.len() > pattern_len {
            // Should not happen with a typical sentinel — only triggers if
            // the sentinel VLE-encodes to fewer than ~3 bytes and the stub
            // body shrinks below 5 bytes total. Pick a larger sentinel.
            return Err(StubFindError::PatternTooSmall);
        }
        while new_body.len() < pattern_len {
            new_body.push(NOP);
        }
        self.binary[stub_offset..stub_offset + pattern_len].copy_from_slice(&new_body);
        Ok(())
    }
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
