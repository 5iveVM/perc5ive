//! Header normalization for `.five` binaries emitted by five-dsl-compiler.
//!
//! On pre-mono builds this module converted a compact 6-byte DSL header to
//! the VM-native 10-byte header and re-pointed every CALL/JUMP target by +4.
//! Under five-mono the DSL compiler emits the VM-native 10-byte header AND
//! fixed-width encoding directly, so normalization is now a passthrough.
//!
//! The API is kept (rather than deleted) so callers don't need to know
//! whether the binary they're working with came from an older compiler.

/// The version-marker byte the legacy DSL compiler placed at position 4.
pub const DSL_VERSION_MARKER: u8 = b':';

/// Error cases when inspecting a DSL binary header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DslNormalizeError {
    /// Binary is shorter than the minimum header (10 bytes).
    TooShort,
    /// Magic bytes don't match `5IVE`.
    BadMagic,
}

/// Returns `true` when the binary carries the legacy 6-byte DSL header
/// (`byte[4] == b':'`). Mono-era compilers emit VM-native 10-byte headers so
/// this returns `false` for all new binaries.
pub fn is_dsl_format(bin: &[u8]) -> bool {
    bin.len() >= 5 && &bin[0..4] == b"5IVE" && bin[4] == DSL_VERSION_MARKER
}

/// Normalize a `.five` binary to the VM-native 10-byte header layout.
///
/// Under mono this is a passthrough — the compiler already emits the right
/// shape. Legacy DSL binaries (with the 6-byte header) aren't supported on
/// this branch; returns `TooShort`/`BadMagic` only for malformed input.
pub fn normalize_dsl_header(bin: &[u8]) -> Result<Vec<u8>, DslNormalizeError> {
    if bin.len() < 10 {
        return Err(DslNormalizeError::TooShort);
    }
    if &bin[0..4] != b"5IVE" {
        return Err(DslNormalizeError::BadMagic);
    }
    Ok(bin.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_passthrough_for_vm_native_binary() {
        let bin = b"5IVE\x00\x00\x00\x00\x01\x01\x11\x2a\x07".to_vec();
        let out = normalize_dsl_header(&bin).unwrap();
        assert_eq!(out, bin);
    }

    #[test]
    fn normalize_rejects_bad_magic() {
        let bin = b"NOT5:0junk_";
        assert_eq!(
            normalize_dsl_header(bin).unwrap_err(),
            DslNormalizeError::BadMagic,
        );
    }

    #[test]
    fn normalize_rejects_too_short() {
        let bin = b"5IVE\x00";
        assert_eq!(
            normalize_dsl_header(bin).unwrap_err(),
            DslNormalizeError::TooShort,
        );
    }

    #[test]
    fn is_dsl_format_detects_legacy_marker() {
        let bin = b"5IVE:\x03\x90\x00\x01\x00\x07";
        assert!(is_dsl_format(bin));
        let bin_native = b"5IVE\x00\x00\x00\x00\x01\x01";
        assert!(!is_dsl_format(bin_native));
    }
}
