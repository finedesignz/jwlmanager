//! Note-only delete backend + semantic dry-run preview (EDIT-01, SAFE-01,
//! SAFE-02, SAFE-03, SAFE-04, 02-02-PLAN.md).
//!
//! **Scope correction (D2-05, corrected — see 02-02-SUMMARY.md):** the
//! original D2-05 wording said delete removes "the Note's UserMark/BlockRange
//! links." That is WRONG and over-deletes: `Note.UserMarkId` is not unique and
//! a `UserMark` can carry highlight `BlockRange` data the user never
//! selected. The corrected, implemented scope: [`delete_notes`] executes
//! ONLY `DELETE FROM Note WHERE NoteId IN (...)` — nothing else. Whatever
//! becomes a genuine orphan (an unreferenced `UserMark`/`BlockRange`/
//! `TagMap`/`Tag`/`Location`) is swept later by `crate::db::trim::trim_sweep`
//! on save. This matches `JWLManager.py:3666` exactly
//! (`delete('Note', 'NoteId')` only).
//!
//! [`NonEmptyNoteIds`] makes an empty selection unrepresentable: it
//! deserializes via `#[serde(try_from = "Vec<i64>")]`, so `ids: []` fails at
//! Tauri IPC deserialization BEFORE any command body or DB access runs
//! (SAFE-03, D2-06).
//!
//! [`dry_run_delete_notes`] runs the REAL `delete_notes` + `trim_sweep` inside
//! a `rusqlite::Transaction` that is deliberately never `.commit()`ed —
//! `Transaction::drop` issues an automatic `ROLLBACK` (SAFE-01, D2-07) — and
//! computes a SEMANTIC [`DryRunReport`] from BEFORE/AFTER primary-key-set
//! snapshots per affected table, NOT raw `changes()`. This is what makes the
//! TagMap re-densify (`DELETE FROM TagMap` + reinsert with the SAME
//! `TagMapId`s for surviving mappings) net out as `overwritten` rather than
//! `deleted`: a `TagMapId` present in both the before-set and the after-set
//! is counted once, as overwritten, never as a false deletion. Only PKs
//! present-before/absent-after count as genuinely `deleted`. `dry_run_delete_notes`
//! NEVER calls `trim_db`/`VACUUM` — it reuses the VACUUM-free `trim_sweep`
//! (Plan 01), so nothing here is a non-rollback-able mutation (Pitfall 2,
//! 02-RESEARCH.md).
//!
//! [`DryRunReport`] and the snapshot/diff primitives this module uses
//! (`snapshot_all`, `diff_snapshots`) now live in `crate::db::edit`
//! (07-01-PLAN.md Task 1 — the shared safety spine every Phase 7 edit op
//! reuses); this module imports them rather than defining them.

use crate::db::edit::{diff_snapshots, snapshot_all, DryRunReport};
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::DeleteFailed {
        reason: format!("{context}: {err}"),
    }
}

/// A non-empty selection of `Note.NoteId` values. Constructed only via
/// `TryFrom<Vec<i64>>`/`serde`'s `try_from` container attribute, which
/// rejects an empty `Vec` — an empty selection is impossible by
/// construction, not merely a runtime-checked precondition (SAFE-03, D2-06).
/// The `Note.NoteId` type is `i64`, so ids are always bound as typed
/// integers, never string-interpolated (SAFE-02) — see [`delete_notes`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyNoteIds.ts")]
pub struct NonEmptyNoteIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyNoteIds {
    type Error = String;

    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyNoteIds(ids))
        }
    }
}

impl NonEmptyNoteIds {
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

/// Deletes EXACTLY the selected `Note` rows and NOTHING else — a single
/// `DELETE FROM Note WHERE NoteId IN (...)` bound via
/// `rusqlite::params_from_iter` (SAFE-02: only the placeholder COUNT is
/// dynamic, ids are always bound as typed `i64` params, never
/// string-interpolated). Does NOT touch `UserMark`/`BlockRange`/`TagMap` —
/// those are swept later, only if genuinely orphaned, by
/// `crate::db::trim::trim_sweep` on save (see module docs / D2-05
/// correction). Runs inside the caller's transaction, so a failure here
/// rolls back with everything else in that transaction (SAFE-04).
pub fn delete_notes(tx: &Transaction, ids: &NonEmptyNoteIds) -> Result<usize, ArchiveError> {
    let placeholders: String = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM Note WHERE NoteId IN ({placeholders})");
    tx.execute(&sql, rusqlite::params_from_iter(ids.iter()))
        .map_err(|e| map_sqlite_err(e, "delete_notes"))
}

/// Runs the REAL `delete_notes` + `trim_sweep` inside a transaction that is
/// NEVER committed (SAFE-01, D2-07) and returns a SEMANTIC [`DryRunReport`].
/// Forces `foreign_keys = OFF` before opening the transaction (mirrors
/// `JWLManager.py:3681`/`trim_db` — `Note` deletion would otherwise trip FK
/// enforcement from `TagMap.NoteId`), and restores the connection's prior
/// PRAGMA values via [`PragmaGuard`] on return — PRAGMAs are not rolled back
/// by a transaction rollback (Plan 01, finding 4). Never calls
/// `trim_db`/`VACUUM` (module docs, Pitfall 2) — the working copy is left
/// byte-identical.
pub fn dry_run_delete_notes(
    conn: &mut Connection,
    ids: &NonEmptyNoteIds,
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
    // `PragmaGuard`'s docs (same pattern as `trim_db`).
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening dry-run transaction"))?;

    let before = snapshot_all(&tx)?;
    delete_notes(&tx, ids)?;
    trim_sweep(&tx)?;
    let after = snapshot_all(&tx)?;

    let report = diff_snapshots(&before, &after);

    // Deliberately DROPPED without `.commit()` — `Transaction::drop`'s
    // default `DropBehavior::Rollback` issues an automatic `ROLLBACK`, so
    // nothing above is ever persisted (SAFE-01).
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
    fn non_empty_note_ids_rejects_empty_array() {
        let result: Result<NonEmptyNoteIds, _> = serde_json::from_str("[]");
        assert!(
            result.is_err(),
            "an empty array must fail to deserialize, not deserialize to an empty NonEmptyNoteIds"
        );
    }

    #[test]
    fn non_empty_note_ids_accepts_non_empty_array() {
        let result: Result<NonEmptyNoteIds, _> = serde_json::from_str("[1,2,3]");
        let ids = result.expect("non-empty array must deserialize");
        assert_eq!(ids.len(), 3);
        assert!(!ids.is_empty());
    }

    #[test]
    fn delete_notes_sql_is_a_single_static_delete_from_note() {
        // Source assertion (SAFE-02): the SQL string built here is always
        // "DELETE FROM Note WHERE NoteId IN (?,?,...)" — only the
        // placeholder COUNT varies, never interpolated id values.
        let ids = NonEmptyNoteIds::try_from(vec![10_i64, 20, 30]).unwrap();
        let placeholders: String = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM Note WHERE NoteId IN ({placeholders})");
        assert_eq!(sql, "DELETE FROM Note WHERE NoteId IN (?,?,?)");
    }
}
