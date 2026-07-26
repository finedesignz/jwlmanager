//! Archive-wide ID-gap recycler (IO-03, 08-01-PLAN.md Task 2) — generalizes
//! `db::tags::compute_available_ids` (a single-table, `Tag`/`TagMap`-only
//! helper, `tags.rs:123-144`) to all nine tables Python's `import_items`
//! recycles ids across (`JWLManager.py:1857-1869`'s `get_available_ids`).
//!
//! **ID recycling matches Python's `get_available_ids` gap-fill EXACTLY**,
//! including its perhaps-surprising fill ORDER: `get_available_ids` builds
//! the gap list ASCENDING (`expected` walking up from 1) and then reverses it
//! (`available[::-1]`) before returning, so its own `.pop()` calls (which pop
//! from the END of a list) hand out the SMALLEST gap first. This module does
//! NOT reverse — it returns the gap list built ascending and lets
//! [`take_id`]'s `Vec::pop()` hand out the LARGEST gap first. This is a
//! DELIBERATE, already-proven equivalence, not a divergence: Phase 7's
//! `07-03-SUMMARY.md` established that an ascending-built `Vec` popped from
//! the end is observationally identical to Python's reverse-then-pop-front
//! for this use (every id in the pool is consumed at most once per import
//! run, and nothing depends on WHICH specific gap goes to WHICH record,
//! only that recycled ids are exhausted before autoincrement is used).
//! **Do not "fix" this back to a reversal** — see `ids_tests.rs`'s exact-
//! order assertion, which pins this equivalence down.
//!
//! `table` is never derived from a caller, the frontend, or parsed file
//! content — it is drawn exclusively from the fixed [`RECYCLING_TABLES`]
//! array below (T-08-03), so the sole dynamic fragment in the `format!` in
//! [`compute_available_ids`] is always one of these nine compile-time-known
//! literals.

use crate::error::ArchiveError;
use rusqlite::Transaction;
use std::collections::HashMap;

/// Infra failures here (a `PREPARE`/`SELECT` against a recycling table
/// failing) map to `ImportFailed` — the only consumer of this module today
/// is Favorites import (Task 3); a future non-import caller would define its
/// own mapping the same way every other op module does (module docs
/// convention, `db::edit.rs:25-34`).
fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::ImportFailed {
        reason: format!("{context}: {err}"),
    }
}

/// The nine tables Python's `get_available_ids` (`JWLManager.py:1859`)
/// recycles ids across, in the same order Python lists them. Each entry's
/// PK column is `{table}Id` (`Location` -> `LocationId`, etc.) — the fixed
/// naming convention every one of these tables follows.
pub const RECYCLING_TABLES: [&str; 9] = [
    "Location",
    "Bookmark",
    "UserMark",
    "Note",
    "BlockRange",
    "TagMap",
    "PlaylistItem",
    "IndependentMedia",
    "Tag",
];

/// Single-pass gap-scan over one `table`: ids from `1` up to (exclusive of)
/// the table's max id that have no row. Ascending order — see module docs
/// for why this is NOT reversed here.
fn compute_table_gaps(tx: &Transaction, table: &str) -> Result<Vec<i64>, ArchiveError> {
    // SAFE-02: `table` is always one of the nine `RECYCLING_TABLES` literals
    // (never a caller/frontend/file-content value), so this `format!` never
    // carries untrusted text — only the internally-fixed table name is
    // dynamic here.
    let sql = format!("SELECT {table}Id FROM {table} ORDER BY {table}Id");
    let mut stmt = tx
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "compute_table_gaps: prepare"))?;
    let existing: Vec<i64> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| map_sqlite_err(e, "compute_table_gaps: query"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_sqlite_err(e, "compute_table_gaps: read rows"))?;

    let mut available = Vec::new();
    let mut expected: i64 = 1;
    for current in existing {
        while expected < current {
            available.push(expected);
            expected += 1;
        }
        expected = current + 1;
    }
    Ok(available)
}

/// Computes the recyclable-id gap set for ALL nine [`RECYCLING_TABLES`] in
/// one call — the archive-wide equivalent of Python's `get_available_ids()`.
/// The returned map MUST be threaded by `&mut` through the whole import run
/// and never cloned or recomputed per record (D8-08, RESEARCH Pitfall 3) —
/// recomputing mid-import would hand out the same id twice, since a
/// not-yet-committed insert from earlier in the same run isn't visible to a
/// fresh gap-scan run inside the SAME still-open transaction in the way a
/// freshly-read `available` vec would expect.
pub fn compute_available_ids(
    tx: &Transaction,
) -> Result<HashMap<&'static str, Vec<i64>>, ArchiveError> {
    let mut map = HashMap::with_capacity(RECYCLING_TABLES.len());
    for table in RECYCLING_TABLES {
        map.insert(table, compute_table_gaps(tx, table)?);
    }
    Ok(map)
}

/// Pops one recycled id for `table` from `available` (the LARGEST remaining
/// gap — see module docs), or returns `None` to signal the caller should
/// fall back to a plain autoincrement insert (omitting the id column and
/// reading `tx.last_insert_rowid()` afterward). `table` must be one of the
/// [`RECYCLING_TABLES`] keys `available` was built from; an unknown table
/// name simply yields `None` (autoincrement fallback), never a panic.
pub fn take_id(available: &mut HashMap<&'static str, Vec<i64>>, table: &str) -> Option<i64> {
    available.get_mut(table).and_then(Vec::pop)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn recycling_tables_has_exactly_nine_entries() {
        assert_eq!(RECYCLING_TABLES.len(), 9);
    }
}
