//! Stable fixture-id → UUID mapping.
//!
//! Fixture ids are human-readable strings (`mem-pay-0042`); the runtime and
//! store speak UUIDs. This derivation is deterministic and collision-checked
//! at load time, so replaying fixtures always produces identical database
//! rows — a prerequisite for diffable eval results.

use uuid::Uuid;

fn fnv1a64(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325 ^ seed;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// Deterministic UUID for a stable fixture id string.
pub fn stable_uuid(fixture_id: &str) -> Uuid {
    let hi = fnv1a64(fixture_id.as_bytes(), 0);
    let lo = fnv1a64(fixture_id.as_bytes(), 0x9E37_79B9_7F4A_7C15);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hi.to_be_bytes());
    bytes[8..].copy_from_slice(&lo.to_be_bytes());
    // Stamp RFC-4122 version 8 (custom) + variant bits so the value is a
    // well-formed UUID wherever it lands.
    bytes[6] = (bytes[6] & 0x0F) | 0x80;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(stable_uuid("mem-pay-0042"), stable_uuid("mem-pay-0042"));
    }

    #[test]
    fn distinct_for_distinct_ids() {
        assert_ne!(stable_uuid("mem-pay-0042"), stable_uuid("mem-pay-0043"));
        assert_ne!(stable_uuid("a"), stable_uuid("b"));
    }
}
