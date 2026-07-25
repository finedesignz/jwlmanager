//! Favorites mark/unmark backend (EDIT-05, 07-01-PLAN.md). Unmark ports
//! `JWLManager.py:3662` (`delete('TagMap', 'TagMapId')` — a Favorite's
//! identity, per `db::browse::query_favorites`'s `FAVORITES_SQL`, is a
//! `TagMap` row with `NoteId IS NULL`); mark ports `add_favorite`
//! (`JWLManager.py:3391-3460`) — see this module's Task 2 additions for
//! that half.
//!
//! Follows the D7-01 safety pattern generalized in `db::edit`: a typed
//! non-empty selection wrapper ([`NonEmptyTagMapIds`]), an `apply_*(tx, ...)`
//! that runs inside the caller's transaction with only-placeholder-count-
//! dynamic parameterized SQL, and a `dry_run_*(conn, ...)` that runs the REAL
//! `apply_*` (+ `trim_sweep`) inside a never-committed `unchecked_transaction`
//! under [`PragmaGuard`], returning a semantic [`DryRunReport`] — copied
//! verbatim in shape from `db::delete::dry_run_delete_notes`
//! (`delete.rs:223-259`).

use crate::db::edit::{diff_snapshots, snapshot_tables, DryRunReport, FAVORITE_SNAPSHOT_TABLES};
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::FavoriteFailed {
        reason: format!("{context}: {err}"),
    }
}

/// A non-empty selection of `TagMap.TagMapId` values identifying Favorite
/// rows to unmark (`db::browse::query_favorites`: a Favorite's identity PK
/// is `TagMapId`, the row's `NoteId IS NULL`). Constructed only via
/// `TryFrom<Vec<i64>>`/`serde`'s `try_from` container attribute, which
/// rejects an empty `Vec` — an empty selection is impossible by
/// construction, not merely a runtime-checked precondition, mirroring
/// `db::delete::NonEmptyNoteIds` (`delete.rs:48-85`) exactly.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyTagMapIds.ts")]
pub struct NonEmptyTagMapIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyTagMapIds {
    type Error = String;

    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyTagMapIds(ids))
        }
    }
}

impl NonEmptyTagMapIds {
    pub fn iter(&self) -> impl Iterator<Item = &i64> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false` by construction — kept only to satisfy
    /// `clippy::len_without_is_empty`.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Unmarks EXACTLY the selected Favorite rows and NOTHING else — a single
/// `DELETE FROM TagMap WHERE TagMapId IN (...)` bound via
/// `rusqlite::params_from_iter` (only the placeholder COUNT is dynamic; ids
/// are always bound as typed `i64` params, never string-interpolated).
/// Ports `JWLManager.py:3662`. Runs inside the caller's transaction, so a
/// failure here rolls back with everything else in that transaction.
pub fn apply_favorite_remove(
    tx: &Transaction,
    ids: &NonEmptyTagMapIds,
) -> Result<usize, ArchiveError> {
    let placeholders: String = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM TagMap WHERE TagMapId IN ({placeholders})");
    tx.execute(&sql, rusqlite::params_from_iter(ids.iter()))
        .map_err(|e| map_sqlite_err(e, "apply_favorite_remove"))
}

/// Runs the REAL `apply_favorite_remove` + `trim_sweep` inside a transaction
/// that is NEVER committed and returns a SEMANTIC [`DryRunReport`] — copied
/// verbatim in shape from `db::delete::dry_run_delete_notes`
/// (`delete.rs:223-259`), swapping only the mutation call and the
/// affected-table set ([`FAVORITE_SNAPSHOT_TABLES`]). Restores the
/// connection's prior PRAGMA values via [`PragmaGuard`] on return.
pub fn dry_run_favorite_remove(
    conn: &mut Connection,
    ids: &NonEmptyTagMapIds,
) -> Result<DryRunReport, ArchiveError> {
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_sqlite_err(e, "setting dry-run pragmas"))?;

    // `unchecked_transaction` (shared `&self`) because `guard` already holds
    // a shared borrow of `conn` for the duration of this function — see
    // `PragmaGuard`'s docs (same pattern as every other `dry_run_*` in db/).
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening dry-run transaction"))?;

    let before = snapshot_tables(&tx, FAVORITE_SNAPSHOT_TABLES)?;
    apply_favorite_remove(&tx, ids)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, FAVORITE_SNAPSHOT_TABLES)?;

    let report = diff_snapshots(&before, &after);

    // Deliberately DROPPED without `.commit()` — `Transaction::drop`'s
    // default `DropBehavior::Rollback` issues an automatic `ROLLBACK`, so
    // nothing above is ever persisted.
    drop(tx);
    // Restores the snapshotted PRIOR pragma values.
    drop(guard);

    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_tag_map_ids_rejects_empty_array() {
        let result: Result<NonEmptyTagMapIds, _> = serde_json::from_str("[]");
        assert!(
            result.is_err(),
            "an empty array must fail to deserialize, not deserialize to an empty NonEmptyTagMapIds"
        );
    }

    #[test]
    fn non_empty_tag_map_ids_accepts_non_empty_array() {
        let result: Result<NonEmptyTagMapIds, _> = serde_json::from_str("[1,2,3]");
        let ids = result.expect("non-empty array must deserialize");
        assert_eq!(ids.len(), 3);
        assert!(!ids.is_empty());
    }

    #[test]
    fn apply_favorite_remove_sql_is_a_single_static_delete_from_tagmap() {
        // SAFE-02: the SQL string built here is always
        // "DELETE FROM TagMap WHERE TagMapId IN (?,?,...)" — only the
        // placeholder COUNT varies, never interpolated id values.
        let ids = NonEmptyTagMapIds::try_from(vec![10_i64, 20, 30]).unwrap();
        let placeholders: String = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM TagMap WHERE TagMapId IN ({placeholders})");
        assert_eq!(sql, "DELETE FROM TagMap WHERE TagMapId IN (?,?,?)");
    }
}
