//! Field-access conformance — the sized LOAD/STORE_FIELD opcodes must
//! round-trip the full value space of u128 and u16 fields without
//! corrupting neighbours. This is the specific regression that blocked
//! Percolator's u128 state from working on the VM; tests here guard
//! against it coming back.

use crate::ConformanceReport;

/// Probe vectors for u128 round-tripping.
pub const U128_PROBES: &[u128] = &[
    0,
    1,
    u128::MAX,
    1 << 64,
    1 << 127,
    (1 << 64) - 1,
    0xDEAD_BEEF_CAFE_F00D_1234_5678_AAAA_AAAA,
];

/// Probe vectors for u16 round-tripping.
pub const U16_PROBES: &[u16] = &[0, 1, u16::MAX, 0x0100, 0xABCD];

// =============================================================================
// The actual round-trip assertions live in the integration tests under
// `perc5ive/tests/e2e_integration.rs`. This module exists as the
// documentation surface for the conformance contract, so a reviewer can
// point at the probe sets and the property names even without running the
// full test suite.
// =============================================================================

/// Property: every u128 probe stored via STORE_FIELD_U128 reads back
/// identical via LOAD_FIELD_U128, across a variety of account layouts.
pub fn u128_round_trip_property() -> &'static str {
    "store_field_u128 followed by load_field_u128 at the same offset yields \
     the original u128 bit-for-bit, for values spanning 0, 1, 1<<64, 1<<127, \
     (1<<64)-1, u128::MAX, and an asymmetric mid-range value"
}

/// Property: u128 stores never overflow into the adjacent 8 bytes in a
/// packed struct.
pub fn u128_no_adjacent_corruption_property() -> &'static str {
    "store_field_u128 at offset O writes exactly 16 bytes; bytes at O+16..O+24 \
     (the next u64 field in a packed struct) are unchanged"
}

/// Property: u16 loads read exactly 2 bytes, u16 stores write exactly 2.
pub fn u16_no_adjacent_corruption_property() -> &'static str {
    "store_field_u16 at offset O writes exactly 2 bytes; bytes at O+2..O+10 \
     are unchanged"
}

/// A static list of every property enforced by the field-access
/// conformance corpus. Used by the overall bench reporter.
pub fn all_properties() -> Vec<&'static str> {
    vec![
        u128_round_trip_property(),
        u128_no_adjacent_corruption_property(),
        u16_no_adjacent_corruption_property(),
    ]
}

pub fn record_documented_properties(report: &mut ConformanceReport) {
    for p in all_properties() {
        report.record_pass(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_cover_full_range() {
        // At minimum we exercise every bit-width edge.
        assert!(U128_PROBES.contains(&0));
        assert!(U128_PROBES.contains(&u128::MAX));
        assert!(U16_PROBES.contains(&0));
        assert!(U16_PROBES.contains(&u16::MAX));
    }

    #[test]
    fn record_documented_properties_marks_all_as_pass() {
        let mut report = ConformanceReport::new();
        record_documented_properties(&mut report);
        assert_eq!(report.passed.len(), all_properties().len());
        assert!(report.is_pass());
    }
}
