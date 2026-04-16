//! Convert the five-dsl-compiler's 6-byte header format to MitoVM's 10-byte
//! format so the linked binary can run through `MitoVM::execute_direct`.
//!
//! # Why this exists
//!
//! The `@five-vm/cli` compiler emits binaries with a compact 6-byte header:
//!
//! ```text
//!   [0..=3]  magic: b"5IVE"
//!   [4]      version marker: b':' (0x3A)
//!   [5]      function_count: u8
//!   [6..]    body (dispatch + handler bytecode)
//! ```
//!
//! `MitoVM::parse_optimized_header` expects the protocol's 10-byte layout:
//!
//! ```text
//!   [0..=3]  magic: b"5IVE"
//!   [4..=7]  features: u32 LE
//!   [8]      public_function_count: u8
//!   [9]      total_function_count: u8
//!   [10..]   body
//! ```
//!
//! Every CALL instruction in the DSL body carries a raw little-endian u16
//! target that is an **absolute byte offset into the running script**. When
//! we shift the body from offset 6 to offset 10, those targets are off by 4
//! unless we re-point them.
//!
//! The normalizer:
//!   1. Detects the DSL header (`byte[4] == b':'`).
//!   2. Walks the body linearly using a compact opcode-size table covering
//!      every opcode the compiler emits (verified against the empirical set
//!      observed in `dsl/src/main.v` and the `link_test.v` probe).
//!   3. Bumps each CALL/JUMP target by +4 to compensate for the larger
//!      header.
//!   4. Emits a 10-byte-header binary with the same body length + 4.
//!
//! Unknown opcodes cause the normalizer to return an error rather than
//! silently mis-parse — this trades false-flexible for safety, since an
//! unknown opcode might be a data byte the normalizer would otherwise
//! misinterpret as a CALL.

use five_protocol::opcodes::*;

/// The version-marker byte the DSL compiler places at position 4 of every
/// `.five` it emits. Equals ASCII `:`.
pub const DSL_VERSION_MARKER: u8 = b':';

/// Error cases when normalizing a DSL binary. Each variant names the thing
/// that broke so callers can fix the underlying input or extend the opcode
/// table as the compiler grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DslNormalizeError {
    /// Binary is shorter than the minimum valid DSL header (6 bytes).
    TooShort,
    /// Magic bytes don't match `5IVE`.
    BadMagic,
    /// Byte 4 isn't the DSL version marker (`0x3A` / `b':'`). Caller should
    /// treat the binary as already VM-native and skip normalization.
    NotDslFormat,
    /// Walked off the end of the binary while decoding an instruction.
    TruncatedInstruction,
    /// A VLE operand ran past the binary end or overran the u64 space.
    BadVle,
    /// Encountered an opcode the table doesn't know. Add it to
    /// [`opcode_size`] with the right length once it's first observed.
    UnknownOpcode(u8),
}

/// Return `true` iff this buffer looks like a DSL-format `.five`:
/// 5IVE magic + `:` version marker at position 4.
pub fn is_dsl_format(bin: &[u8]) -> bool {
    bin.len() >= 6 && &bin[..4] == b"5IVE" && bin[4] == DSL_VERSION_MARKER
}

/// Convert a DSL-format binary to the VM's 10-byte header layout.
///
/// Returns the new binary (body shifted by +4, CALL / JUMP targets bumped
/// by +4, header re-encoded). If the input is already in VM-native format,
/// returns a verbatim copy (the whole point is that callers can hand any
/// compiled output to this function without pre-checking).
pub fn normalize_dsl_header(bin: &[u8]) -> Result<Vec<u8>, DslNormalizeError> {
    if bin.len() < 4 || &bin[..4] != b"5IVE" {
        return Err(DslNormalizeError::BadMagic);
    }
    // If this isn't DSL format, return as-is — it's already VM-native.
    if bin.len() < 6 {
        return Err(DslNormalizeError::TooShort);
    }
    if bin[4] != DSL_VERSION_MARKER {
        return Ok(bin.to_vec());
    }

    let function_count = bin[5];
    let body = &bin[6..];

    // Walk every instruction in the body; collect each CALL/JUMP operand
    // position (relative to the body start) so we can bump the stored u16.
    let mut call_sites: Vec<usize> = Vec::new();
    let mut jump_sites: Vec<usize> = Vec::new();
    let mut cursor = 0;
    while cursor < body.len() {
        let opcode = body[cursor];
        let size = opcode_size(opcode, &body[cursor..])?;
        // CALL layout: `op u8_param_count u16_target`. The u16 target starts
        // at cursor + 2.
        if opcode == CALL {
            call_sites.push(cursor + 2);
        } else if opcode == JUMP || opcode == JUMP_IF || opcode == JUMP_IF_NOT {
            // JUMP / JUMP_IF / JUMP_IF_NOT: `op u16_target` — operand at cursor+1.
            jump_sites.push(cursor + 1);
        }
        cursor = cursor
            .checked_add(size)
            .ok_or(DslNormalizeError::TruncatedInstruction)?;
    }
    if cursor != body.len() {
        // Walked past the end — final instruction was truncated.
        return Err(DslNormalizeError::TruncatedInstruction);
    }

    // Build the VM-format binary: new 10-byte header + bumped body.
    let mut out = Vec::with_capacity(10 + body.len());
    out.extend_from_slice(b"5IVE");
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // features = 0
    out.push(function_count); // public_function_count
    out.push(function_count); // total_function_count — DSL doesn't split them
    out.extend_from_slice(body);

    // Body was copied starting at offset 10; the sites we recorded were
    // relative to the body start, so add 10 to get absolute positions in
    // the output.
    for site in call_sites {
        let abs = 10 + site;
        let old = u16::from_le_bytes([out[abs], out[abs + 1]]);
        let new = old.checked_add(4).ok_or(DslNormalizeError::BadVle)?;
        let bytes = new.to_le_bytes();
        out[abs] = bytes[0];
        out[abs + 1] = bytes[1];
    }
    for site in jump_sites {
        let abs = 10 + site;
        let old = u16::from_le_bytes([out[abs], out[abs + 1]]);
        let new = old.checked_add(4).ok_or(DslNormalizeError::BadVle)?;
        let bytes = new.to_le_bytes();
        out[abs] = bytes[0];
        out[abs + 1] = bytes[1];
    }

    Ok(out)
}

/// Size in bytes of the instruction starting at `buf[0]`. Returns
/// `UnknownOpcode(op)` if the opcode isn't in the table — extend this when
/// the compiler starts emitting new opcodes.
///
/// The opcode set here mirrors what `@five-vm/cli` has been observed to
/// emit for Percolator-shaped skeletons. Opcodes that only appear in
/// hand-written bytecode are included when the linker needs them to make
/// the combined binary round-trip cleanly.
fn opcode_size(opcode: u8, buf: &[u8]) -> Result<usize, DslNormalizeError> {
    // Zero-operand ops — just the opcode byte.
    match opcode {
        HALT
        | RETURN
        | RETURN_VALUE
        | NOP
        | DUP
        | DUP2
        | SWAP
        | ROT
        | DROP
        | OVER
        | ADD
        | SUB
        | MUL
        | DIV
        | MOD
        | GT
        | LT
        | EQ
        | GTE
        | LTE
        | NEQ
        | NEG
        | ADD_CHECKED
        | SUB_CHECKED
        | MUL_CHECKED
        | BITWISE_AND
        | BITWISE_OR
        | BITWISE_XOR
        | BITWISE_NOT
        | SHIFT_LEFT
        | SHIFT_RIGHT
        | SHIFT_RIGHT_ARITH
        | DEALLOC_LOCALS
        | LOAD_PARAM_0
        | LOAD_PARAM_1
        | LOAD_PARAM_2
        | LOAD_PARAM_3 => return Ok(1),
        _ => {}
    }

    // Fixed-size-with-operand ops.
    match opcode {
        PUSH_U8 | PUSH_BOOL => return Ok(2),
        PUSH_PUBKEY => return Ok(33),
        PUSH_U128 => return Ok(17),
        // CALL: opcode + param_count u8 + target u16 LE = 4 bytes.
        CALL => return Ok(4),
        JUMP | JUMP_IF | JUMP_IF_NOT => return Ok(3),
        // Multiprecision ops: opcode + 1 flags byte.
        ADD_U256 | MUL_U256 | SUB_U256 | DIV_U256 | CMP_U256 | MULDIV_U256
        | ADD_I128 | SUB_I128 | MUL_I128 | DIV_I128
        | ADD_I256 | SUB_I256 | MUL_I256 | DIV_I256 => return Ok(2),
        // ALLOC_LOCALS count_u8; SET_LOCAL / GET_LOCAL / CLEAR_LOCAL index_u8.
        ALLOC_LOCALS | SET_LOCAL | GET_LOCAL | CLEAR_LOCAL => return Ok(2),
        _ => {}
    }

    // VLE-operand ops — decode the VLE length.
    match opcode {
        PUSH_U16 | PUSH_U32 | PUSH_U64 | PUSH_I64 | LOAD_PARAM => {
            let vle_bytes = vle_u64_size(&buf[1..])?;
            return Ok(1 + vle_bytes);
        }
        STORE_FIELD | LOAD_FIELD => {
            // opcode + account_index_u8 + offset_vle.
            if buf.len() < 2 {
                return Err(DslNormalizeError::TruncatedInstruction);
            }
            let vle_bytes = vle_u64_size(&buf[2..])?;
            return Ok(2 + vle_bytes);
        }
        _ => {}
    }

    Err(DslNormalizeError::UnknownOpcode(opcode))
}

/// Number of bytes a VLE u64 consumes starting at `buf[0]`. Returns
/// `BadVle` if the encoding runs off the end of the buffer or exceeds 10
/// bytes (the max for a u64 in LEB128 encoding).
fn vle_u64_size(buf: &[u8]) -> Result<usize, DslNormalizeError> {
    for (i, &byte) in buf.iter().enumerate().take(10) {
        if byte & 0x80 == 0 {
            return Ok(i + 1);
        }
    }
    Err(DslNormalizeError::BadVle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_dsl_format_accepts_real_header() {
        // First 6 bytes of any DSL-compiled binary: "5IVE" + ':' + count.
        let bin = b"5IVE:\x03\x90\x00\x01\x00\x07";
        assert!(is_dsl_format(bin));
    }

    #[test]
    fn is_dsl_format_rejects_vm_native() {
        // VM-native binaries have features at [4..=7], so byte 4 is 0 (or
        // some bit pattern that isn't ':').
        let bin = b"5IVE\x00\x00\x00\x00\x01\x01";
        assert!(!is_dsl_format(bin));
    }

    #[test]
    fn normalize_bumps_call_target() {
        // Minimal DSL binary: 6-byte header + `CALL 0 0x0008 RETURN HALT`.
        // target 0x08 should become 0x0C (+4).
        let bin = &[
            b'5', b'I', b'V', b'E', b':', 1,
            CALL, 0, 0x08, 0x00, RETURN_VALUE, HALT,
        ];
        let out = normalize_dsl_header(bin).unwrap();
        // Header stays 10 bytes, body becomes 6 bytes, total 16.
        assert_eq!(out.len(), 16);
        assert_eq!(&out[..4], b"5IVE");
        assert_eq!(&out[4..=7], &[0, 0, 0, 0]); // features
        assert_eq!(out[8], 1); // public_count
        assert_eq!(out[9], 1); // total_count
        assert_eq!(out[10], CALL);
        assert_eq!(out[11], 0); // param_count
        // Target was 0x0008, bumped to 0x000C.
        assert_eq!(u16::from_le_bytes([out[12], out[13]]), 0x000C);
        assert_eq!(out[14], RETURN_VALUE);
        assert_eq!(out[15], HALT);
    }

    #[test]
    fn normalize_vm_native_passes_through_unchanged() {
        let bin = b"5IVE\x00\x00\x00\x00\x01\x01\x11\x2a\x07".to_vec();
        let out = normalize_dsl_header(&bin).unwrap();
        assert_eq!(out, bin);
    }

    #[test]
    fn normalize_rejects_bad_magic() {
        let bin = b"NOT5:0junk";
        assert_eq!(
            normalize_dsl_header(bin).unwrap_err(),
            DslNormalizeError::BadMagic
        );
    }

    #[test]
    fn normalize_empty_body_is_ok() {
        let bin = b"5IVE:\x00"; // Header only, no functions.
        let out = normalize_dsl_header(bin).unwrap();
        assert_eq!(out.len(), 10);
        assert_eq!(out[8], 0);
        assert_eq!(out[9], 0);
    }

    #[test]
    fn normalize_preserves_push_u64_vle() {
        // PUSH_U64 42 → 0x1B 0x2A (2 bytes). The 0x2A would be misread as
        // CMP-ish if the opcode table is wrong.
        let bin = &[
            b'5', b'I', b'V', b'E', b':', 0,
            PUSH_U64, 0x2A, RETURN_VALUE, HALT,
        ];
        let out = normalize_dsl_header(bin).unwrap();
        assert_eq!(out.len(), 14);
        assert_eq!(out[10], PUSH_U64);
        assert_eq!(out[11], 0x2A);
        assert_eq!(out[12], RETURN_VALUE);
    }

    #[test]
    fn normalize_real_main_v_produces_valid_length() {
        // End-to-end: load the real compiled main.v (if present) and
        // round-trip it. Skipped when the file isn't built yet so the test
        // doesn't block a clean clone.
        let path = "target/perc5ive.fbin";
        let bin = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return, // skip if not compiled
        };
        let out = normalize_dsl_header(&bin).expect("real main.v normalizes");
        assert_eq!(&out[..4], b"5IVE");
        if is_dsl_format(&bin) {
            // Legacy 6-byte header: body shifts by +4.
            assert_eq!(out.len(), bin.len() + 4);
            assert_eq!(out[8], bin[5]);
            assert_eq!(out[9], bin[5]);
        } else {
            // New compiler (source HEAD) emits VM-native 10-byte headers
            // already; normalize should be a verbatim pass-through.
            assert_eq!(out, bin);
        }
    }
}
