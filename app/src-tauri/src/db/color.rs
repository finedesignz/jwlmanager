//! Highlight/Note recolor backend (EDIT-02, 07-02-PLAN.md Task 3). Ports
//! `set_color` (`JWLManager.py:3237-3278`) faithfully:
//!
//! - Highlights branch (`:3241`): resolve `UserMarkId`s from the selected
//!   `BlockRangeId`s, then `UPDATE UserMark SET ColorIndex`.
//! - Notes branch (`:3243-3247`): for every selected Note that has a
//!   `LocationId` but no `UserMarkId` yet, synthesize a new `UserMark`
//!   (`StyleIndex = 0`, fresh GUID, `Version = 1`, the note's `LocationId`,
//!   the chosen `ColorIndex`) and link it via `Note.UserMarkId` — this is
//!   what turns a plain note into a highlighted one for the first time. Then
//!   resolve every selected Note's (now-non-null) `UserMarkId` and
//!   `UPDATE UserMark SET ColorIndex` for those too.
//! - Highlights + ColorIndex 0 (Grey) guard (`:3255-3256`): a silent no-op —
//!   zero rows changed, never an error. Notes + Grey is NOT a no-op (it still
//!   synthesizes a grey-colored UserMark).
//!
//! **D7-03 (recorded in `07-02-SUMMARY.md`): recolor does NOT invoke the
//! geometric range-union-merge primitive `db::highlights` exposes.** The
//! Python `set_color` never merges — the union-merge exists only in the
//! import path (`add_usermark`). Wiring it into recolor would be a behavior
//! the Python app never had, applied to a DELETE-on-geometric-predicate
//! path; strict parity is the resolved decision. Enforced by a negative
//! grep in `07-02-PLAN.md`'s prohibitions — this module's source contains
//! no reference, textual or otherwise, to that primitive's name.
//!
//! Follows the D7-01 safety pattern generalized in `db::edit` — see
//! `db::favorites` for the shape this module copies verbatim (typed
//! non-empty selection wrapper, `apply_*(tx)` inside the caller's
//! transaction, `dry_run_*(conn)` in a never-committed `unchecked_transaction`
//! under [`PragmaGuard`], semantic [`DryRunReport`] via
//! `snapshot_tables`/`diff_snapshots`).

use crate::db::delete::NonEmptyNoteIds;
use crate::db::edit::{diff_snapshots, snapshot_tables, DryRunReport};
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use crate::guid::format_guid_v4;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::ColorFailed {
        reason: format!("{context}: {err}"),
    }
}

/// A non-empty selection of `BlockRange.BlockRangeId` values — the Highlights
/// identity PK (`db::browse::query_highlights`, `browse.rs:47-52`). Shared by
/// Highlights recolor (this module) and Highlights delete
/// (`db::delete::delete_highlights`), which imports this type rather than
/// defining a second identical wrapper. Constructed only via
/// `TryFrom<Vec<i64>>`/`serde`'s `try_from` container attribute — an empty
/// selection is impossible by construction, mirroring
/// `db::favorites::NonEmptyTagMapIds` exactly.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyBlockRangeIds.ts")]
pub struct NonEmptyBlockRangeIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyBlockRangeIds {
    type Error = String;

    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyBlockRangeIds(ids))
        }
    }
}

impl NonEmptyBlockRangeIds {
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

/// The recolor target: which category's selection, carrying that category's
/// own typed non-empty identity-PK wrapper. A tagged enum rather than a bare
/// `Vec<i64>` + separate `Category` field so the empty-selection-
/// unrepresentable guarantee (SAFE-03) holds per category at IPC
/// deserialization — a `Highlights` selection of `BlockRangeId`s and a
/// `Notes` selection of `NoteId`s are never interchangeable, and neither can
/// ever be empty.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "category")]
#[ts(export, export_to = "../../src/bindings/ColorSelection.ts")]
pub enum ColorSelection {
    Highlights { ids: NonEmptyBlockRangeIds },
    Notes { ids: NonEmptyNoteIds },
}

/// Affected-table set for recolor's `DryRunReport`: `UserMark` (recolored =
/// `overwritten`; synthesized = `added`) and `Note` (its `UserMarkId` gets
/// set on synthesis = `overwritten`). Follows the per-op-table-set precedent
/// (`db::favorites::FAVORITE_SNAPSHOT_TABLES`) rather than the broader
/// default.
pub(crate) const COLOR_SNAPSHOT_TABLES: &[(&str, &str)] =
    &[("UserMark", "UserMarkId"), ("Note", "NoteId")];

/// `UPDATE UserMark SET ColorIndex = ? WHERE UserMarkId IN (...)` — the tail
/// shared by both branches of [`apply_color`]. A no-op (not an error) when
/// `user_mark_ids` is empty, since a selection can legally resolve to zero
/// UserMarks (e.g. every selected Note was independent/`LocationId IS NULL`).
fn update_usermark_colors(
    tx: &Transaction,
    user_mark_ids: &[i64],
    color_index: i64,
) -> Result<(), ArchiveError> {
    if user_mark_ids.is_empty() {
        return Ok(());
    }
    let placeholders: String = std::iter::repeat_n("?", user_mark_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("UPDATE UserMark SET ColorIndex = ? WHERE UserMarkId IN ({placeholders})");
    tx.execute(
        &sql,
        rusqlite::params_from_iter(std::iter::once(&color_index).chain(user_mark_ids.iter())),
    )
    .map_err(|e| map_sqlite_err(e, "apply_color: update usermark colors"))?;
    Ok(())
}

/// Applies a recolor — ports `set_color` (module docs). Runs inside the
/// caller's transaction; every SQL statement binds only typed params (SAFE-02
/// — only placeholder COUNTs are ever dynamic).
pub fn apply_color(
    tx: &Transaction,
    selection: &ColorSelection,
    color_index: i64,
    guid_seed: u64,
) -> Result<(), ArchiveError> {
    match selection {
        ColorSelection::Highlights { ids } => {
            // Highlights + Grey (ColorIndex 0) is a silent no-op
            // (JWLManager.py:3255-3256) — zero rows changed, never an error.
            if color_index == 0 {
                return Ok(());
            }
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let select_sql =
                format!("SELECT UserMarkId FROM BlockRange WHERE BlockRangeId IN ({placeholders})");
            let user_mark_ids: Vec<i64> = {
                let mut stmt = tx
                    .prepare(&select_sql)
                    .map_err(|e| map_sqlite_err(e, "apply_color: resolve highlight usermarks"))?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(ids.iter()), |row| row.get(0))
                    .map_err(|e| map_sqlite_err(e, "apply_color: query highlight usermarks"))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| map_sqlite_err(e, "apply_color: read highlight usermarks"))?;
                rows
            };
            update_usermark_colors(tx, &user_mark_ids, color_index)?;
        }
        ColorSelection::Notes { ids } => {
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");

            // Synthesize a UserMark for every selected Note that has a
            // LocationId but no UserMarkId yet (JWLManager.py:3243-3246) —
            // the Note→UserMark synthesis that turns a plain note into a
            // highlighted one. A Note with LocationId IS NULL (independent —
            // BrowseRow.independent) never matches this predicate and is
            // simply excluded, never an error (07-PATTERNS.md).
            let synth_sql = format!(
                "SELECT NoteId, LocationId FROM Note \
                 WHERE LocationId IS NOT NULL AND UserMarkId IS NULL AND NoteId IN ({placeholders}) \
                 ORDER BY NoteId"
            );
            let to_synthesize: Vec<(i64, i64)> = {
                let mut stmt = tx
                    .prepare(&synth_sql)
                    .map_err(|e| map_sqlite_err(e, "apply_color: find notes needing synthesis"))?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })
                    .map_err(|e| map_sqlite_err(e, "apply_color: query notes needing synthesis"))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| map_sqlite_err(e, "apply_color: read notes needing synthesis"))?;
                rows
            };

            for (note_id, location_id) in to_synthesize {
                // Seed varies per note (XOR with the note id) so a single
                // apply_color call recoloring several notes never mints two
                // identical GUIDs, while the SAME seed across two separate
                // calls (e.g. a dry-run followed by the real apply, or two
                // test invocations) still reproduces the same GUID for the
                // same note — the deterministic-per-seed contract this
                // module's tests rely on.
                let guid = format_guid_v4(guid_seed ^ (note_id as u64));
                tx.execute(
                    "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
                     VALUES (?1, ?2, 0, ?3, 1)",
                    rusqlite::params![color_index, location_id, guid],
                )
                .map_err(|e| map_sqlite_err(e, "apply_color: synthesize usermark"))?;
                let user_mark_id = tx.last_insert_rowid();
                tx.execute(
                    "UPDATE Note SET UserMarkId = ?1 WHERE NoteId = ?2",
                    rusqlite::params![user_mark_id, note_id],
                )
                .map_err(|e| map_sqlite_err(e, "apply_color: link synthesized usermark"))?;
            }

            let resolve_sql = format!(
                "SELECT UserMarkId FROM Note \
                 WHERE UserMarkId IS NOT NULL AND NoteId IN ({placeholders})"
            );
            let user_mark_ids: Vec<i64> = {
                let mut stmt = tx
                    .prepare(&resolve_sql)
                    .map_err(|e| map_sqlite_err(e, "apply_color: resolve note usermarks"))?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(ids.iter()), |row| row.get(0))
                    .map_err(|e| map_sqlite_err(e, "apply_color: query note usermarks"))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| map_sqlite_err(e, "apply_color: read note usermarks"))?;
                rows
            };
            update_usermark_colors(tx, &user_mark_ids, color_index)?;
        }
    }
    Ok(())
}

/// Runs [`apply_color`] and returns the resulting semantic [`DryRunReport`],
/// snapshotting [`COLOR_SNAPSHOT_TABLES`] before/after inside the caller's
/// (already-open, real, committing) transaction — the shape
/// `db::favorites::apply_favorite_add_reporting` established for `*_apply`
/// commands whose affected-row count isn't a single deterministic number.
pub fn apply_color_reporting(
    tx: &Transaction,
    selection: &ColorSelection,
    color_index: i64,
    guid_seed: u64,
) -> Result<DryRunReport, ArchiveError> {
    let before = snapshot_tables(tx, COLOR_SNAPSHOT_TABLES)?;
    apply_color(tx, selection, color_index, guid_seed)?;
    let after = snapshot_tables(tx, COLOR_SNAPSHOT_TABLES)?;
    Ok(diff_snapshots(&before, &after))
}

/// Runs the REAL `apply_color` + `trim_sweep` inside a transaction that is
/// NEVER committed and returns a SEMANTIC [`DryRunReport`] — copied verbatim
/// in shape from `db::favorites::dry_run_favorite_add`
/// (`favorites.rs:278-307`), swapping only the mutation call and the
/// affected-table set.
pub fn dry_run_color(
    conn: &mut Connection,
    selection: &ColorSelection,
    color_index: i64,
    guid_seed: u64,
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

    let before = snapshot_tables(&tx, COLOR_SNAPSHOT_TABLES)?;
    apply_color(&tx, selection, color_index, guid_seed)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, COLOR_SNAPSHOT_TABLES)?;

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
    fn non_empty_block_range_ids_rejects_empty_array() {
        let result: Result<NonEmptyBlockRangeIds, _> = serde_json::from_str("[]");
        assert!(result.is_err(), "empty array must fail to deserialize");
    }

    #[test]
    fn non_empty_block_range_ids_accepts_non_empty_array() {
        let result: Result<NonEmptyBlockRangeIds, _> = serde_json::from_str("[1,2,3]");
        let ids = result.expect("non-empty array must deserialize");
        assert_eq!(ids.len(), 3);
        assert!(!ids.is_empty());
    }

    #[test]
    fn color_selection_highlights_round_trips_through_json() {
        let sel = ColorSelection::Highlights {
            ids: NonEmptyBlockRangeIds::try_from(vec![10_i64, 20]).unwrap(),
        };
        let json = serde_json::to_string(&sel).unwrap();
        let round_tripped: ColorSelection = serde_json::from_str(&json).unwrap();
        match round_tripped {
            ColorSelection::Highlights { ids } => assert_eq!(ids.len(), 2),
            ColorSelection::Notes { .. } => panic!("expected Highlights variant"),
        }
    }

    #[test]
    fn color_selection_notes_round_trips_through_json() {
        let sel = ColorSelection::Notes {
            ids: NonEmptyNoteIds::try_from(vec![1_i64]).unwrap(),
        };
        let json = serde_json::to_string(&sel).unwrap();
        let round_tripped: ColorSelection = serde_json::from_str(&json).unwrap();
        match round_tripped {
            ColorSelection::Notes { ids } => assert_eq!(ids.len(), 1),
            ColorSelection::Highlights { .. } => panic!("expected Notes variant"),
        }
    }
}
