//! Archive-wide tag reorder ("Sort Tags…", EDIT-04, 07-03-PLAN.md Task 2).
//! Ports the OBSERVABLE CONTRACT of `sort_notes`/`reorder`
//! (`JWLManager.py:3825-3855`): for every `Tag WHERE Type = 1`, its
//! `TagMap.Position` values end up 0-based dense, ordered by `NoteId`
//! ascending.
//!
//! **D7-05 resolved (07-03-PLAN.md objective) — reuses the shipped
//! `redensify_tag_positions` TEMP-table staging technique
//! (`db::trim::redensify_tag_positions`, `trim.rs:171-205`) rather than
//! reimplementing Python's own negative-position two-pass rewrite.** Python
//! avoids the `UNIQUE(TagId, Position)` mid-loop collision by writing every
//! row to a NEGATIVE position first (pass 1, `:3829-3832`) and then flipping
//! sign (pass 2, `:3833-3834`) — a valid collision-free rewrite, but not the
//! only one. `redensify_tag_positions` solves the IDENTICAL collision on the
//! IDENTICAL table with a different collision-free technique: stage the
//! target `(TagMapId, Position)` pairs into a TEMP table, `DELETE FROM
//! TagMap`, then re-`INSERT` from staging — no row is ever written twice, and
//! no intermediate state can violate the constraint because the table is
//! empty between the delete and the re-insert. The OBSERVABLE CONTRACT (every
//! `Tag WHERE Type = 1`'s positions end up 0-based dense, ordered by
//! `NoteId`) is IDENTICAL between the two techniques — the round-trip tests
//! in `tests/reorder_tests.rs` assert exactly that contract, never Python's
//! intermediate negative values, so this is not an observable behavioral
//! difference. Reusing the already-shipped, already-tested primitive is the
//! smaller diff (Karpathy-aligned) and keeps reorder and save's trim-path
//! re-densify on ONE technique, so composing them (reorder, then a
//! subsequent save) is trivially idempotent — the second run is a no-op over
//! data the first run already made dense.
//!
//! Ordering key: `redensify_tag_positions`'s `ROW_NUMBER() OVER (PARTITION
//! BY TagId ORDER BY Position, TagMapId)` orders ties by the EXISTING
//! `Position` (preserving prior relative order for equal-`NoteId` rows, which
//! cannot occur here since `UNIQUE(TagId, NoteId)` forbids two rows for the
//! same tag+note) then `TagMapId`. Sort Tags needs `ORDER BY NoteId`
//! (`JWLManager.py:3830`) instead — [`apply_reorder`] stages its own
//! `ROW_NUMBER() OVER (PARTITION BY TagId ORDER BY NoteId) - 1` ordering
//! directly (Python's exact `ORDER BY`), reusing `redensify_tag_positions`'s
//! delete-then-reinsert-from-staging SHAPE rather than calling it verbatim
//! (which orders by `Position`, not `NoteId` — the wrong key for this op).
//! Never disables SQLite's constraint checking and never drops the
//! `UNIQUE` constraint — the staging technique makes that unnecessary.

use crate::db::edit::DryRunReport;
use crate::db::pragma_guard::PragmaGuard;
use crate::error::ArchiveError;
use rusqlite::{Connection, Transaction};
use std::collections::BTreeMap;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::ReorderFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Renumbers `TagMap.Position` to be 0-based dense per `Tag WHERE Type = 1`,
/// ordered by `NoteId` ascending (module docs — the observable contract of
/// `sort_notes`/`reorder`, `JWLManager.py:3825-3855`). Only rows belonging
/// to a `Type = 1` tag are touched — `TagMap` rows for `Type = 0`
/// (Favorites) or `Type = 2` (Playlists) tags are left completely alone,
/// matching Python's own `SELECT TagId FROM Tag WHERE Type = 1` scope
/// (`:3828`).
///
/// Uses the SAME delete-then-reinsert-from-staging shape as
/// [`crate::db::trim::redensify_tag_positions`] (an explicit column list on
/// the final `INSERT`, never `SELECT *`), but stages its own `NoteId`-keyed
/// `ROW_NUMBER()` window (module docs) and scopes the delete/reinsert to
/// `Type = 1` tags only via a join back to `Tag`, so `Type = 0`/`Type = 2`
/// rows are never staged, deleted, or reinserted.
///
/// Returns the count of rows whose `Position` GENUINELY CHANGED (staged
/// `Position != OldPosition`) — NOT the total row count. This is what makes
/// "a fixture already in sorted, dense order reports zero changes" true:
/// the generic PK-set diff (`db::edit::diff_snapshots`) can't express this,
/// since every `TagMapId` survives reorder (present in both before/after
/// snapshots regardless of whether its `Position` moved), so this module
/// builds its own [`DryRunReport`] from the staged before/after `Position`
/// comparison instead of reusing the shared snapshot/diff primitives. Runs
/// inside the caller's transaction; a failure here rolls back with
/// everything else in that transaction.
pub fn apply_reorder(tx: &Transaction) -> Result<usize, ArchiveError> {
    tx.execute(
        "CREATE TEMP TABLE TagMapReorder AS \
         SELECT tm.TagMapId, tm.PlaylistItemId, tm.LocationId, tm.NoteId, tm.TagId, \
             tm.Position AS OldPosition, \
             ROW_NUMBER() OVER (PARTITION BY tm.TagId ORDER BY tm.NoteId) - 1 AS Position \
         FROM TagMap tm \
             JOIN Tag t ON t.TagId = tm.TagId \
         WHERE t.Type = 1",
        [],
    )
    .map_err(|e| map_sqlite_err(e, "apply_reorder: stage target ordering"))?;

    let changed: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMapReorder WHERE Position != OldPosition",
            [],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "apply_reorder: count changed rows"))?;
    let changed = changed as usize;

    tx.execute(
        "DELETE FROM TagMap WHERE TagId IN (SELECT TagId FROM Tag WHERE Type = 1)",
        [],
    )
    .map_err(|e| map_sqlite_err(e, "apply_reorder: delete existing rows"))?;

    tx.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         SELECT TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position FROM TagMapReorder",
        [],
    )
    .map_err(|e| map_sqlite_err(e, "apply_reorder: reinsert from staging"))?;

    tx.execute("DROP TABLE TagMapReorder", [])
        .map_err(|e| map_sqlite_err(e, "apply_reorder: cleanup staging table"))?;

    Ok(changed)
}

/// Wraps a raw changed-row count from [`apply_reorder`] into a
/// [`DryRunReport`] shape — `overwritten["TagMap"]` carries the count
/// (every reordered row keeps its `TagMapId`, so this is genuinely an
/// UPDATE-in-place, never an add/delete), `deleted`/`total_deleted` stay
/// empty/zero. Reused by both the rolled-back preview
/// ([`dry_run_reorder`]) and the real, committed apply (`reorder_apply` in
/// `lib.rs`), matching the explicit-map shape `highlight_delete_apply`
/// already uses in `lib.rs` for a single deterministic count.
pub fn reorder_report(changed: usize) -> DryRunReport {
    let mut overwritten = BTreeMap::new();
    if changed > 0 {
        overwritten.insert("TagMap".to_string(), changed);
    }
    DryRunReport {
        added: BTreeMap::new(),
        overwritten,
        deleted: BTreeMap::new(),
        total_deleted: 0,
    }
}

/// Runs the REAL `apply_reorder` inside a transaction that is NEVER
/// committed and returns a SEMANTIC [`DryRunReport`] via [`reorder_report`]
/// — leaves the DB unchanged (SAFE-01). Deliberately does NOT also run
/// `trim_sweep` — reorder is archive-wide and Type=1-scoped already; the
/// re-densify `trim_sweep` performs is over the SAME rows this op just
/// densified, so running it here would only ever show zero additional
/// change and adds nothing to the preview.
pub fn dry_run_reorder(conn: &mut Connection) -> Result<DryRunReport, ArchiveError> {
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

    let changed = apply_reorder(&tx)?;
    let report = reorder_report(changed);

    drop(tx);
    drop(guard);

    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn reorder_report_zero_changes_has_no_overwritten_entry() {
        let report = reorder_report(0);
        assert!(report.overwritten.is_empty());
        assert_eq!(report.total_deleted, 0);
    }

    #[test]
    fn reorder_report_nonzero_changes_populates_tagmap_overwritten() {
        let report = reorder_report(5);
        assert_eq!(report.overwritten.get("TagMap"), Some(&5));
    }
}
