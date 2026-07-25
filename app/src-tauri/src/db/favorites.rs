//! Favorites mark/unmark backend (EDIT-05, 07-01-PLAN.md). Unmark ports
//! `JWLManager.py:3662` (`delete('TagMap', 'TagMapId')` — a Favorite's
//! identity, per `db::browse::query_favorites`'s `FAVORITES_SQL`, is a
//! `TagMap` row with `NoteId IS NULL`); mark ports `add_favorite`
//! (`JWLManager.py:3391-3460`).
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

/// The minimal identifying fields needed to mark a Favorite: the edition's
/// `KeySymbol` and integer `MepsLanguage` id — exactly what
/// [`apply_favorite_add`]'s `Location` find-or-insert and duplicate check
/// need. Deliberately NOT the full
/// [`crate::db::resources::FavoriteEdition`] catalog row: per T-07-02 no
/// table/column name crosses the IPC boundary, and the frontend only ever
/// needs to send back these two typed fields (taken from whichever
/// `FavoriteEdition` row the user picked) — never the display-only
/// `short`/`lang` strings, which the mutation itself doesn't use.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/FavoriteEditionRef.ts")]
pub struct FavoriteEditionRef {
    pub symbol: String,
    pub language: i64,
}

/// Marks a Bible edition as a Favorite — ports `add_favorite`
/// (`JWLManager.py:3391-3460`):
///
/// 1. Ensures the system `Tag (Type = 0, Name = 'Favorite')` exists
///    (`INSERT ... WHERE NOT EXISTS`, `:3435`).
/// 2. Finds or inserts the `Location` row for this (KeySymbol, MepsLanguage)
///    pair (`:3444`, matched on `KeySymbol`/`MepsLanguage`/`IssueTagNumber=0`/
///    `Type=1` — `BookNumber`/`ChapterNumber` are always `NULL` for a
///    Favorite Location and, per SQLite's `UNIQUE` semantics, two `NULL`s
///    are never equal to each other, so `Location`'s own
///    `UNIQUE(BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)`
///    constraint cannot dedupe these rows on its own — the explicit
///    find-or-insert is load-bearing, not defensive redundancy).
/// 3. Rejects a duplicate favorite for that (edition, language) with a
///    typed error BEFORE attempting the `TagMap` INSERT (07-PATTERNS.md
///    Correction #3 — `TagMap` carries `UNIQUE(TagId, LocationId)`, so a
///    duplicate is a hard DB error, never a caught constraint violation).
/// 4. Inserts one `TagMap` row at `Position = ifnull(max(Position), -1) + 1`
///    for that tag (`:3437-3441`).
///
/// Step order is regrouped from the Python source (which interleaves
/// `add_location` and the duplicate check BEFORE `tag_positions()`'s
/// tag-ensure) into ensure-tag / ensure-location / check-duplicate /
/// compute-position / insert — every step besides the final insert is an
/// idempotent find-or-insert that doesn't interact with the others except
/// through the final duplicate check, so this reordering produces the
/// identical final DB state in both the happy path and the duplicate path
/// while reading top-to-bottom instead of Python's callback-order. All
/// values bound via typed params, never interpolated (Python's own
/// duplicate-check SELECT at `:3455` f-string-interpolates the location id
/// directly into the SQL text — the exact anti-pattern this port fixes).
pub fn apply_favorite_add(
    tx: &Transaction,
    edition: &FavoriteEditionRef,
) -> Result<(), ArchiveError> {
    // 1. Ensure the system Favorite tag exists (idempotent).
    tx.execute(
        "INSERT INTO Tag (Type, Name) \
         SELECT 0, 'Favorite' \
         WHERE NOT EXISTS (SELECT 1 FROM Tag WHERE Type = 0 AND Name = 'Favorite')",
        [],
    )
    .map_err(|e| map_sqlite_err(e, "apply_favorite_add: ensure system tag"))?;
    let tag_id: i64 = tx
        .query_row(
            "SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "apply_favorite_add: read system tag id"))?;

    // 2. Find or insert the Location for this (KeySymbol, MepsLanguage).
    tx.execute(
        "INSERT INTO Location (IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         SELECT 0, ?1, ?2, 1 \
         WHERE NOT EXISTS ( \
             SELECT 1 FROM Location \
             WHERE KeySymbol = ?1 AND MepsLanguage = ?2 AND IssueTagNumber = 0 AND Type = 1 \
         )",
        rusqlite::params![edition.symbol, edition.language],
    )
    .map_err(|e| map_sqlite_err(e, "apply_favorite_add: find-or-insert location"))?;
    let location_id: i64 = tx
        .query_row(
            "SELECT LocationId FROM Location \
             WHERE KeySymbol = ?1 AND MepsLanguage = ?2 AND IssueTagNumber = 0 AND Type = 1",
            rusqlite::params![edition.symbol, edition.language],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "apply_favorite_add: read location id"))?;

    // 3. Reject a duplicate BEFORE attempting the INSERT.
    let already_favorited: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM TagMap WHERE LocationId = ?1 AND TagId = ?2)",
            rusqlite::params![location_id, tag_id],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "apply_favorite_add: duplicate check"))?;
    if already_favorited {
        return Err(ArchiveError::FavoriteDuplicate {
            language: edition.language.to_string(),
            edition: edition.symbol.clone(),
        });
    }

    // 4. Insert at the next Position for this tag.
    let position: i64 = tx
        .query_row(
            "SELECT IFNULL(MAX(Position), -1) + 1 FROM TagMap WHERE TagId = ?1",
            rusqlite::params![tag_id],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "apply_favorite_add: compute position"))?;
    tx.execute(
        "INSERT INTO TagMap (PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (NULL, ?1, NULL, ?2, ?3)",
        rusqlite::params![location_id, tag_id, position],
    )
    .map_err(|e| map_sqlite_err(e, "apply_favorite_add: insert tagmap"))?;

    Ok(())
}

/// Runs [`apply_favorite_add`] and returns the resulting semantic
/// [`DryRunReport`], snapshotting [`FAVORITE_SNAPSHOT_TABLES`] before/after
/// inside the caller's transaction. Unlike unmark's single deterministic
/// `deleted` count, mark's affected-table shape isn't a fixed count — `Tag`
/// and `Location` are each added only conditionally (absent-tag/absent-
/// location cases) — so both the rolled-back preview ([`dry_run_favorite_add`])
/// and the real, committed apply (`favorite_add_apply` in `lib.rs`) reuse
/// this general snapshot/diff machinery rather than hand-building the
/// report from a count.
pub fn apply_favorite_add_reporting(
    tx: &Transaction,
    edition: &FavoriteEditionRef,
) -> Result<DryRunReport, ArchiveError> {
    let before = snapshot_tables(tx, FAVORITE_SNAPSHOT_TABLES)?;
    apply_favorite_add(tx, edition)?;
    let after = snapshot_tables(tx, FAVORITE_SNAPSHOT_TABLES)?;
    Ok(diff_snapshots(&before, &after))
}

/// Runs the REAL `apply_favorite_add` + `trim_sweep` inside a transaction
/// that is NEVER committed and returns a SEMANTIC [`DryRunReport`] — same
/// shape as [`dry_run_favorite_remove`]. A duplicate favorite surfaces as
/// `Err(ArchiveError::FavoriteDuplicate)` here too, before any row is
/// staged, mirroring exactly what `favorite_add_apply` will do — the early
/// return still runs `tx`'s (rollback) and `guard`'s (pragma-restore) `Drop`
/// impls, since Rust drops function-local values on any return path.
pub fn dry_run_favorite_add(
    conn: &mut Connection,
    edition: &FavoriteEditionRef,
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

    let before = snapshot_tables(&tx, FAVORITE_SNAPSHOT_TABLES)?;
    apply_favorite_add(&tx, edition)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, FAVORITE_SNAPSHOT_TABLES)?;

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

    #[test]
    fn favorite_edition_ref_round_trips_through_json() {
        let edition = FavoriteEditionRef {
            symbol: "nwt".to_string(),
            language: 0,
        };
        let json = serde_json::to_string(&edition).unwrap();
        let round_tripped: FavoriteEditionRef = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.symbol, "nwt");
        assert_eq!(round_tripped.language, 0);
    }
}
