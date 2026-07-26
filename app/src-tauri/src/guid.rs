//! Dependency-free RFC-4122 v4-SHAPED GUID formatting (07-RESEARCH.md Shared
//! Pattern 6 / 07-PATTERNS.md Correction #4 — neither `uuid` nor `rand` is a
//! declared dependency; both appear only transitively via `tauri`). Follows
//! `app/src-tauri/src/time.rs`'s established no-new-dependency precedent: a
//! cited public-domain algorithm plus shape tests, rather than exact-output
//! assertions. `UserMark.UserMarkGuid` (`TEXT NOT NULL UNIQUE`) only needs
//! per-archive uniqueness, never cryptographic unpredictability, and save is
//! not byte-preserving — so a v4-shaped string is semantically sufficient;
//! byte parity with Python's `uuid.uuid1()` is explicitly not required
//! (07-CONTEXT.md D7-02).
//!
//! [`format_guid_v4`] takes an explicit `seed: u64`, threaded through exactly
//! the way `now: &str` is threaded at `app/src-tauri/src/lib.rs:132` — the
//! command layer supplies a real seed (derived from wall-clock time), while
//! tests supply a fixed literal so output is fully deterministic.

/// One step of the SplitMix64 generator (public-domain algorithm, Vigna &
/// Steele, <https://prng.di.unimi.it/splitmix64.c>) — a fast, well-distributed
/// 64-bit PRNG used here purely as a seed-to-bytes expander, not for any
/// security-sensitive purpose. `state` is mutated in place; the returned
/// value is the next stream output.
fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Formats a deterministic RFC-4122 v4-shaped GUID string
/// (`xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`, `y` in `{8,9,a,b}`) derived from
/// `seed`. The same `seed` always yields the same string (determinism for
/// tests and for `apply_color`'s twice-with-same-seed acceptance criterion);
/// distinct seeds are, with overwhelming probability, distinct strings (128
/// bits of SplitMix64 output) — sufficient for `UserMark.UserMarkGuid`'s
/// `UNIQUE` constraint within one archive.
pub fn format_guid_v4(seed: u64) -> String {
    let mut state = seed;
    let hi = splitmix64_next(&mut state);
    let lo = splitmix64_next(&mut state);
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&hi.to_be_bytes());
    bytes[8..16].copy_from_slice(&lo.to_be_bytes());
    // Set the version nibble to 4 and the variant bits to RFC-4122 (10xx).
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-\
         {:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_guid_v4_is_deterministic_per_seed() {
        assert_eq!(format_guid_v4(42), format_guid_v4(42));
    }

    #[test]
    fn format_guid_v4_has_the_rfc4122_v4_shape() {
        let g = format_guid_v4(1);
        assert_eq!(g.len(), 36, "expected 8-4-4-4-12, got: {g}");
        assert_eq!(g.as_bytes()[8], b'-');
        assert_eq!(g.as_bytes()[13], b'-');
        assert_eq!(g.as_bytes()[14], b'4', "version nibble must be 4");
        assert_eq!(g.as_bytes()[18], b'-');
        let variant = g.as_bytes()[19];
        assert!(
            matches!(variant, b'8' | b'9' | b'a' | b'b'),
            "variant nibble must be 8/9/a/b, got {}",
            variant as char
        );
        assert_eq!(g.as_bytes()[23], b'-');
    }

    #[test]
    fn format_guid_v4_differs_across_seeds() {
        assert_ne!(format_guid_v4(1), format_guid_v4(2));
    }
}
