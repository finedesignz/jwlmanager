//! Dependency-free `YYYY-MM-DDTHH:MM:SSZ` UTC timestamp formatting (matches
//! `JWLManager.py`'s `datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')`
//! shape used for `creationDate`/`lastModifiedDate`). No `chrono`/`time`
//! dependency is added for this single formatting need — civil-date
//! conversion from a Unix day count is a well-known, leap-second-free
//! algorithm (Howard Hinnant's `civil_from_days`).

use std::time::{SystemTime, UNIX_EPOCH};

/// Converts a day count since the Unix epoch (1970-01-01) into
/// `(year, month, day)`. Proleptic Gregorian calendar, no leap seconds.
/// Reference: <http://howardhinnant.github.io/date_algorithms.html>.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Returns the current UTC time formatted as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now_iso8601_utc() -> String {
    let secs_since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let days = secs_since_epoch.div_euclid(86_400);
    let secs_of_day = secs_since_epoch.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_matches_known_dates() {
        // 1970-01-01 is day 0.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2000-01-01 (leap year start).
        assert_eq!(civil_from_days(10_957), (2000, 1, 1));
    }

    #[test]
    fn now_iso8601_utc_has_the_right_shape() {
        let ts = now_iso8601_utc();
        assert_eq!(ts.len(), 20, "expected YYYY-MM-DDTHH:MM:SSZ, got: {ts}");
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[4], b'-');
        assert_eq!(ts.as_bytes()[7], b'-');
        assert_eq!(ts.as_bytes()[10], b'T');
    }
}
