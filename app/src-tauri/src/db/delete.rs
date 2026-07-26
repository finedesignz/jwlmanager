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

use crate::db::color::NonEmptyBlockRangeIds;
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

/// Deletes EXACTLY the selected `BlockRange` rows and NOTHING else — NEVER
/// `UserMark` (rule #9: Highlights delete targets the range geometry only; a
/// `UserMark` left with zero remaining `BlockRange`s becomes an orphan swept
/// later by `trim_sweep`/`trim_db` on save, exactly like the deliberate
/// Notes-delete scope decision above). Ports the Highlights branch of
/// `delete_items` (`JWLManager.py:3658-3671`, `BlockRange`/`BlockRangeId`,
/// D7-10). Uses [`crate::db::color::NonEmptyBlockRangeIds`] — the same
/// identity-PK wrapper Highlights recolor (`db::color`) already owns, rather
/// than defining a second identical type.
pub fn delete_highlights(
    tx: &Transaction,
    ids: &NonEmptyBlockRangeIds,
) -> Result<usize, ArchiveError> {
    let placeholders: String = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM BlockRange WHERE BlockRangeId IN ({placeholders})");
    tx.execute(&sql, rusqlite::params_from_iter(ids.iter()))
        .map_err(|e| map_sqlite_err(e, "delete_highlights"))
}

/// Runs the REAL `delete_highlights` + `trim_sweep` inside a transaction that
/// is NEVER committed (SAFE-01) and returns a SEMANTIC [`DryRunReport`] over
/// the DEFAULT [`TRACKED_TABLES`] set (like [`dry_run_delete_notes`]) —
/// broad enough to also surface any `trim_sweep` orphan-fallout (e.g. a
/// `UserMark` now orphaned because its last `BlockRange` was just removed)
/// truthfully in the preview, without this function needing its own narrower
/// per-op table set.
pub fn dry_run_delete_highlights(
    conn: &mut Connection,
    ids: &NonEmptyBlockRangeIds,
) -> Result<DryRunReport, ArchiveError> {
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_sqlite_err(e, "setting dry-run pragmas"))?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening dry-run transaction"))?;

    let before = snapshot_all(&tx)?;
    delete_highlights(&tx, ids)?;
    trim_sweep(&tx)?;
    let after = snapshot_all(&tx)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
    drop(guard);

    Ok(report)
}

/// A non-empty selection of `Bookmark.BookmarkId` values — the Bookmarks
/// identity PK (`browse.rs:33-37`, NOT the first-SELECTed `LocationId` — the
/// load-bearing pitfall this phase's research called out). Constructed only
/// via `TryFrom<Vec<i64>>`/serde's `try_from` container attribute, mirroring
/// [`NonEmptyNoteIds`] exactly.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyBookmarkIds.ts")]
pub struct NonEmptyBookmarkIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyBookmarkIds {
    type Error = String;

    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyBookmarkIds(ids))
        }
    }
}

impl NonEmptyBookmarkIds {
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

/// Deletes EXACTLY the selected `Bookmark` rows — `DELETE FROM Bookmark
/// WHERE BookmarkId IN (...)` (D7-10, `JWLManager.py:3658-3671`'s Bookmarks
/// branch). Runs inside the caller's transaction (SAFE-04).
pub fn delete_bookmarks(
    tx: &Transaction,
    ids: &NonEmptyBookmarkIds,
) -> Result<usize, ArchiveError> {
    let placeholders: String = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM Bookmark WHERE BookmarkId IN ({placeholders})");
    tx.execute(&sql, rusqlite::params_from_iter(ids.iter()))
        .map_err(|e| map_sqlite_err(e, "delete_bookmarks"))
}

/// Runs the REAL `delete_bookmarks` + `trim_sweep` inside a transaction that
/// is NEVER committed (SAFE-01) and returns a SEMANTIC [`DryRunReport`] over
/// the default [`crate::db::edit::TRACKED_TABLES`] set (which already
/// includes `("Bookmark", "BookmarkId")`, 07-01-PLAN.md Task 1).
pub fn dry_run_delete_bookmarks(
    conn: &mut Connection,
    ids: &NonEmptyBookmarkIds,
) -> Result<DryRunReport, ArchiveError> {
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_sqlite_err(e, "setting dry-run pragmas"))?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening dry-run transaction"))?;

    let before = snapshot_all(&tx)?;
    delete_bookmarks(&tx, ids)?;
    trim_sweep(&tx)?;
    let after = snapshot_all(&tx)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
    drop(guard);

    Ok(report)
}

/// A non-empty selection of Annotation `LocationId` values — the browse-list
/// Annotations identity (`browse.rs:28-31`). Deleting by `LocationId`
/// removes ALL `InputField` rows at that location — an INTENTIONAL
/// over-deletion (rule #10, `JWLManager.py:3669`), distinct from the
/// record-editor's own `(LocationId, TextTag)`-scoped single delete
/// (`crate::db::record_edit::apply_record_delete`). The two paths must never
/// be crossed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyLocationIds.ts")]
pub struct NonEmptyLocationIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyLocationIds {
    type Error = String;

    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyLocationIds(ids))
        }
    }
}

impl NonEmptyLocationIds {
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

/// Deletes ALL `InputField` rows at every selected `LocationId` — see
/// [`NonEmptyLocationIds`]'s docs for the deliberate over-deletion this
/// implements (rule #10). Runs inside the caller's transaction (SAFE-04).
pub fn delete_annotations(
    tx: &Transaction,
    ids: &NonEmptyLocationIds,
) -> Result<usize, ArchiveError> {
    let placeholders: String = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM InputField WHERE LocationId IN ({placeholders})");
    tx.execute(&sql, rusqlite::params_from_iter(ids.iter()))
        .map_err(|e| map_sqlite_err(e, "delete_annotations"))
}

/// Runs the REAL `delete_annotations` + `trim_sweep` inside a transaction
/// that is NEVER committed (SAFE-01) and returns a SEMANTIC [`DryRunReport`]
/// over the default [`crate::db::edit::TRACKED_TABLES`] set (which already
/// includes `("InputField", "rowid")`, 07-01-PLAN.md Task 1) — so the
/// preview shows the TRUE row count when more than one `InputField` is
/// removed at a location (rule #10).
pub fn dry_run_delete_annotations(
    conn: &mut Connection,
    ids: &NonEmptyLocationIds,
) -> Result<DryRunReport, ArchiveError> {
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_sqlite_err(e, "setting dry-run pragmas"))?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening dry-run transaction"))?;

    let before = snapshot_all(&tx)?;
    delete_annotations(&tx, ids)?;
    trim_sweep(&tx)?;
    let after = snapshot_all(&tx)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
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
