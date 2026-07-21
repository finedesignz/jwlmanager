//! `trim_db` orphan sweep + tag re-densify + VACUUM (ARCH-04, D2-01..D2-04,
//! 02-01-PLAN.md). Ports `JWLManager.py:3858-3935` statement-for-statement in
//! the SAME order — the order encodes referential dependencies (children
//! before parents; re-densify tags after orphan TagMap removal).
//!
//! **Deliberate deviation from the Python source (documented safety fix,
//! finding 3):** the Python source uses `col NOT IN (SELECT col2 FROM t)`
//! over columns whose subquery can contain NULL (`Note.LocationId`,
//! `TagMap.LocationId`, `TagMap.PlaylistItemId` are all nullable). SQL `NOT
//! IN` evaluates to `UNKNOWN` (excluded from the result) for EVERY outer row
//! whenever the subquery's result set contains any NULL — so those two
//! predicates (the `Location` unused-row sweep and the `PlaylistItem`
//! unused-row sweep) would NEVER delete anything in the Python source once a
//! single un-tagged/independent Note or a Note-linked TagMap row exists,
//! which is the common case. This port rewrites exactly those two predicates
//! as `NOT EXISTS (SELECT 1 FROM t WHERE t.col = outer.col)`, which is
//! immune to NULL-poisoning and sweeps correctly. Every other predicate below
//! references a NOT-NULL primary-key or NOT-NULL foreign-key column (verified
//! against `res/blank`'s live schema) and is kept byte-for-byte verbatim.
//!
//! Lines 3930-3933 of the Python source (`except: crash_box; clean_up;
//! sys.exit()`) are the defect NOT ported (D2-02) — failure here returns a
//! typed [`ArchiveError::TrimFailed`] and the caller's transaction rolls
//! back on drop; the process never exits and the working copy is never left
//! half-trimmed.
//!
//! `trim_sweep` is DML-only (no VACUUM) so a future dry-run/preview path
//! (Plan-02) can run the sweep inside a transaction it always rolls back,
//! without VACUUM ever running non-rollback-able mutation. `trim_db` is the
//! save-path entry point: forces sweep-friendly PRAGMAs, runs `trim_sweep`
//! inside a committed transaction, restores the PRAGMAs via [`PragmaGuard`],
//! then runs `VACUUM` OUTSIDE the transaction (SQLite forbids `VACUUM`
//! inside an active transaction).

use crate::db::pragma_guard::PragmaGuard;
use crate::error::ArchiveError;
use rusqlite::{Connection, Transaction};
use std::collections::BTreeMap;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::TrimFailed {
        reason: format!("{context}: {err}"),
    }
}

/// (label, static SQL) pairs run BEFORE the TagMap re-densify step, in the
/// exact order of `JWLManager.py:3870-3880`. All SQL is static/parameterless
/// (SAFE-02 by construction — nothing here is ever built from user input).
const SWEEP_PRE_REDENSIFY: &[(&str, &str)] = &[
    (
        "empty_input_field",
        "DELETE FROM InputField WHERE COALESCE(Value, '') = ''",
    ),
    (
        "untagged_note",
        "DELETE FROM Note WHERE COALESCE(Title, '') = '' AND COALESCE(Content, '') = '' \
         AND NOT EXISTS (SELECT 1 FROM TagMap WHERE TagMap.NoteId = Note.NoteId)",
    ),
    (
        "orphan_tagmap",
        "DELETE FROM TagMap WHERE \
         (NoteId IS NOT NULL AND NoteId NOT IN (SELECT NoteId FROM Note)) \
         OR (PlaylistItemId IS NOT NULL AND PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem))",
    ),
    (
        "unused_tag",
        "DELETE FROM Tag WHERE TagId NOT IN (SELECT DISTINCT TagId FROM TagMap) AND Type > 0",
    ),
];

/// (label, static SQL) pairs run AFTER the TagMap re-densify step, in the
/// exact order of `JWLManager.py:3888-3917`. `unused_location` and
/// `orphan_playlist_item` carry the NOT-EXISTS fix (finding 3, module docs);
/// every other predicate here is verbatim (its subquery column is a
/// NOT-NULL primary or foreign key, verified against `res/blank`'s schema).
const SWEEP_POST_REDENSIFY: &[(&str, &str)] = &[
    (
        "orphan_usermark",
        "DELETE FROM UserMark WHERE \
         (UserMarkId NOT IN (SELECT UserMarkId FROM BlockRange WHERE UserMarkId IS NOT NULL) \
         AND UserMarkId NOT IN (SELECT UserMarkId FROM Note WHERE UserMarkId IS NOT NULL)) \
         OR LocationId NOT IN (SELECT LocationId FROM Location WHERE LocationId IS NOT NULL)",
    ),
    (
        "orphan_blockrange",
        "DELETE FROM BlockRange WHERE \
         UserMarkId NOT IN (SELECT UserMarkId FROM UserMark WHERE UserMarkId IS NOT NULL)",
    ),
    (
        "orphan_playlist_item",
        // Finding 3: TagMap.PlaylistItemId is nullable (a Note/Location tag
        // mapping always has it NULL), so the Python `NOT IN (SELECT
        // PlaylistItemId FROM TagMap)` is NULL-poisoned in the common case
        // and never sweeps. Rewritten as NOT EXISTS.
        "DELETE FROM PlaylistItem WHERE \
         NOT EXISTS (SELECT 1 FROM TagMap WHERE TagMap.PlaylistItemId = PlaylistItem.PlaylistItemId)",
    ),
    (
        "orphan_playlist_item_marker",
        "DELETE FROM PlaylistItemMarker WHERE \
         PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem)",
    ),
    (
        "orphan_playlist_item_location_map",
        "DELETE FROM PlaylistItemLocationMap WHERE \
         PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem)",
    ),
    (
        "orphan_playlist_item_independent_media_map_by_item",
        "DELETE FROM PlaylistItemIndependentMediaMap WHERE \
         PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem)",
    ),
    (
        "orphan_playlist_item_independent_media_map_by_media",
        "DELETE FROM PlaylistItemIndependentMediaMap WHERE \
         IndependentMediaId NOT IN (SELECT IndependentMediaId FROM IndependentMedia)",
    ),
    (
        "orphan_playlist_item_marker_bible_verse_map",
        "DELETE FROM PlaylistItemMarkerBibleVerseMap WHERE \
         PlaylistItemMarkerId NOT IN (SELECT PlaylistItemMarkerId FROM PlaylistItemMarker)",
    ),
    (
        "orphan_playlist_item_marker_paragraph_map",
        "DELETE FROM PlaylistItemMarkerParagraphMap WHERE \
         PlaylistItemMarkerId NOT IN (SELECT PlaylistItemMarkerId FROM PlaylistItemMarker)",
    ),
    (
        "unused_location",
        // Finding 3: Note.LocationId and TagMap.LocationId are nullable, so
        // the Python NOT IN over those two subqueries is NULL-poisoned in
        // the common case (an independent Note, or any Note/Location tag
        // mapping) and never sweeps. Rewritten as NOT EXISTS. The remaining
        // five branches (UserMark, Bookmark x2, InputField,
        // PlaylistItemLocationMap) all key off NOT-NULL columns and are kept
        // verbatim as NOT IN.
        "DELETE FROM Location WHERE \
         LocationId NOT IN (SELECT LocationId FROM UserMark) \
         AND NOT EXISTS (SELECT 1 FROM Note WHERE Note.LocationId = Location.LocationId) \
         AND NOT EXISTS (SELECT 1 FROM TagMap WHERE TagMap.LocationId = Location.LocationId) \
         AND LocationId NOT IN (SELECT LocationId FROM Bookmark) \
         AND LocationId NOT IN (SELECT PublicationLocationId FROM Bookmark) \
         AND LocationId NOT IN (SELECT LocationId FROM InputField) \
         AND LocationId NOT IN (SELECT LocationId FROM PlaylistItemLocationMap)",
    ),
    (
        "fix_missing_note_links",
        "UPDATE Location SET Title = '' WHERE Title IS NULL",
    ),
];

/// Runs one ordered slice of (label, SQL) pairs via individual `tx.execute`
/// calls (never `execute_batch`) so each statement's `changes()` count can be
/// captured per label — a future dry-run/report consumer sums these.
fn run_labeled_sweep(
    tx: &Transaction,
    steps: &[(&str, &str)],
    counts: &mut BTreeMap<String, usize>,
) -> Result<(), ArchiveError> {
    for (label, sql) in steps {
        let changed = tx.execute(sql, []).map_err(|e| map_sqlite_err(e, label))?;
        *counts.entry((*label).to_string()).or_insert(0) += changed;
    }
    Ok(())
}

/// Re-densifies `TagMap.Position` to be contiguous 0-based per `TagId`,
/// ordered by original `Position` then `TagMapId` — `JWLManager.py:3883-3886`.
/// Uses an EXPLICIT column list on the final `INSERT` (never `SELECT *`), so
/// the re-densify is immune to `TagMap`'s column order ever changing.
fn redensify_tag_positions(
    tx: &Transaction,
    counts: &mut BTreeMap<String, usize>,
) -> Result<(), ArchiveError> {
    tx.execute(
        "CREATE TEMP TABLE TagMapNew AS SELECT TagMapId, PlaylistItemId, LocationId, NoteId, \
         TagId, ROW_NUMBER() OVER (PARTITION BY TagId ORDER BY Position, TagMapId) - 1 AS Position \
         FROM TagMap",
        [],
    )
    .map_err(|e| map_sqlite_err(e, "tagmap_redensify_stage"))?;

    let deleted = tx
        .execute("DELETE FROM TagMap", [])
        .map_err(|e| map_sqlite_err(e, "tagmap_redensify_delete"))?;
    *counts
        .entry("tagmap_redensify_delete".to_string())
        .or_insert(0) += deleted;

    let inserted = tx
        .execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             SELECT TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position FROM TagMapNew",
            [],
        )
        .map_err(|e| map_sqlite_err(e, "tagmap_redensify_insert"))?;
    *counts
        .entry("tagmap_redensify_insert".to_string())
        .or_insert(0) += inserted;

    tx.execute("DROP TABLE TagMapNew", [])
        .map_err(|e| map_sqlite_err(e, "tagmap_redensify_cleanup"))?;

    Ok(())
}

/// DML-only orphan sweep + tag re-densify, run inside `tx`. Never VACUUMs
/// (module docs) — the caller commits (or rolls back on `Err`) and, for the
/// real save path, VACUUMs afterward via [`trim_db`]. Returns a per-label
/// row-change count for future dry-run/report consumers.
pub fn trim_sweep(tx: &Transaction) -> Result<BTreeMap<String, usize>, ArchiveError> {
    let mut counts = BTreeMap::new();
    run_labeled_sweep(tx, SWEEP_PRE_REDENSIFY, &mut counts)?;
    redensify_tag_positions(tx, &mut counts)?;
    run_labeled_sweep(tx, SWEEP_POST_REDENSIFY, &mut counts)?;
    Ok(counts)
}

/// Save-path entry point (ARCH-04, D2-03/D2-04): forces sweep-friendly
/// session PRAGMAs (matching `JWLManager.py:3862-3865`), runs [`trim_sweep`]
/// inside a single committed transaction, restores the connection's PRIOR
/// PRAGMA values via [`PragmaGuard`] (never hardcoded back to the Python
/// literals — PRAGMAs are not rolled back by a failed transaction, so the
/// guard restores on both the commit AND the early-return-on-`Err` paths),
/// then runs `VACUUM` OUTSIDE the transaction (SQLite forbids `VACUUM`
/// inside an active transaction; `VACUUM` never runs on any error path).
///
/// Any failure surfaces as `ArchiveError::TrimFailed` — never a partial
/// trim, never a process exit (D2-02).
pub fn trim_db(conn: &mut Connection) -> Result<(), ArchiveError> {
    // `PRAGMA foreign_keys` is a documented no-op inside an active
    // transaction, so it (and the other session pragmas) must be set BEFORE
    // the transaction opens — mirroring upgrade.rs's identical constraint.
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_sqlite_err(e, "setting sweep pragmas"))?;

    // `unchecked_transaction` (shared `&self`) is used instead of
    // `transaction` (`&mut self`) because `guard` already holds a shared
    // borrow of `conn` for the duration of this function — see
    // `PragmaGuard`'s docs.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening trim transaction"))?;

    trim_sweep(&tx)?;

    tx.commit()
        .map_err(|e| map_sqlite_err(e, "committing trim transaction"))?;

    // Restores the snapshotted PRIOR pragma values (commit path).
    drop(guard);

    // VACUUM must run OUTSIDE any transaction (SQLite requirement) and is
    // ONLY ever called here — trim_sweep never VACUUMs, so a future
    // dry-run/preview that reuses trim_sweep inside a rolled-back
    // transaction never triggers a non-rollback-able VACUUM.
    conn.execute_batch("VACUUM;")
        .map_err(|e| map_sqlite_err(e, "vacuuming after trim"))?;

    Ok(())
}
