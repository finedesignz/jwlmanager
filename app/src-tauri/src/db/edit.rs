//! Shared safety spine for every Phase 7 edit-op backend (EDIT-02..EDIT-07,
//! D7-01, 07-01-PLAN.md Task 1) — the semantic dry-run/apply envelope
//! generalized from Phase 2's `db::delete` so `db::favorites` (this plan)
//! and the op modules Plans 02-05 add (`color`/`tags`/`reorder`/`scrub`/
//! `record_edit`) build on ONE proven implementation rather than six ad-hoc
//! ones. This module is pure Rust infrastructure and ports no Python
//! directly itself — every op module built on top of it cites its own
//! Python line range (`JWLManager.py:3237-3856` spans the six op groups;
//! see `07-CONTEXT.md`/`07-RESEARCH.md` canonical_refs for the per-op
//! ranges).
//!
//! [`DryRunReport`], the default [`TRACKED_TABLES`] set, and
//! [`snapshot_pks`]/[`snapshot_tables`]/[`snapshot_all`]/[`diff_snapshots`]
//! MOVED here verbatim from `db::delete` (D2-07's "intentionally GENERAL"
//! design finally gets its own home) — behavior is UNCHANGED, only the
//! module moved. `db::delete`, `archive::downgrade`, and `archive::merge`
//! now import these from here instead of from `db::delete`.
//!
//! Each op module also defines its OWN per-op snapshot-table constant when
//! its affected-table set differs from the default (`archive::downgrade`'s
//! `DOWNGRADE_SNAPSHOT_TABLES` is the established precedent this phase
//! follows) — [`FAVORITE_SNAPSHOT_TABLES`] is the first of these to live
//! alongside the shared primitives.
//!
//! Convention (documented, not shared code — each op module's error variant
//! differs): every op module defines its OWN module-private
//! `map_sqlite_err(err, context) -> ArchiveError` mapping SQL failures to
//! THAT module's typed variant (`db::delete`'s maps to `DeleteFailed`;
//! `db::favorites`'s maps to `FavoriteFailed`), matching `delete.rs:42-46`'s
//! shape exactly. This module keeps its OWN copy (below) for its own
//! snapshot-read failures, inherited unchanged from `db::delete` — it is a
//! copy-pasted convention, not a generic helper, because `error.rs`'s
//! `to_dto` is an exhaustive match with no `_` arm, so every variant is a
//! compile-time-forced, named decision.

use crate::error::ArchiveError;
use rusqlite::Transaction;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use ts_rs::TS;

/// Snapshot/diff infra failures (a `PREPARE`/`SELECT` against a tracked
/// table failing) map to the same variant `db::delete` always used before
/// this module existed — kept unchanged by the Task 1 move (behavior
/// preservation). In practice this path only fires if the schema itself is
/// unexpectedly broken, since every tracked table/column pair is a fixed,
/// compile-time-known-valid identifier, never derived from archive content.
fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::DeleteFailed {
        reason: format!("{context}: {err}"),
    }
}

/// General semantic preview report: per-table primary-key counts of rows
/// newly present (`added`), present in both before/after snapshots
/// (`overwritten` — e.g. the TagMap re-densify's preserved mappings, or
/// `Location.Title` normalized to `''`), and genuinely removed (`deleted`).
/// Intentionally GENERAL (D2-07) — every edit op in the app reuses this one
/// shape. MOVED here unchanged from `db::delete` (07-01-PLAN.md Task 1).
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/DryRunReport.ts")]
pub struct DryRunReport {
    pub added: BTreeMap<String, usize>,
    pub overwritten: BTreeMap<String, usize>,
    pub deleted: BTreeMap<String, usize>,
    pub total_deleted: usize,
    /// Per-table count of records an import silently no-op'd on because they
    /// already exactly match an existing row (PD-2, 08-01-PLAN.md) — e.g.
    /// Favorites' string-level dup check (`db::io::import`). Distinct from
    /// `overwritten`: a skip performs ZERO writes, so reporting it as an
    /// "update" would tell the user the archive changed when it did not.
    /// `Default`-derived alongside the rest of the struct, so every existing
    /// Phase 2/7 constructor keeps compiling and reports an empty map here.
    pub skipped: BTreeMap<String, usize>,
}

/// Default tables tracked for semantic before/after diffing when an op has
/// no narrower per-op set of its own, each with its single-column identity
/// (a real PK, or — for `InputField` — its implicit `rowid`, see below).
/// Covers every table `delete_notes` + `trim_sweep` can affect for a
/// Notes-delete flow. The remaining composite-key `PlaylistItem*` junction
/// tables (`PlaylistItemLocationMap`, `PlaylistItemIndependentMediaMap`,
/// `PlaylistItemMarkerBibleVerseMap`, `PlaylistItemMarkerParagraphMap`) have
/// no single-column identity PK and stay intentionally out of scope for
/// row-identity diffing here.
///
/// 07-01-PLAN.md extends this with two entries (07-PATTERNS.md Correction
/// #2): `("InputField", "rowid")` — `InputField`'s primary key
/// (`LocationId`, `TextTag`) is non-INTEGER, so the table stays an ordinary
/// SQLite rowid table and `snapshot_pks(tx, "InputField", "rowid")` works
/// with ZERO new code; a captured `rowid` is valid ONLY for the lifetime of
/// the one transaction that read it (SQLite may renumber rowids across a
/// `VACUUM`), so it must never be persisted or compared across the save
/// pipeline. And `("Bookmark", "BookmarkId")`, needed by the per-category
/// deletes D7-10 adds later this phase.
pub(crate) const TRACKED_TABLES: &[(&str, &str)] = &[
    ("Note", "NoteId"),
    ("UserMark", "UserMarkId"),
    ("BlockRange", "BlockRangeId"),
    ("TagMap", "TagMapId"),
    ("Tag", "TagId"),
    ("Location", "LocationId"),
    ("PlaylistItem", "PlaylistItemId"),
    ("PlaylistItemMarker", "PlaylistItemMarkerId"),
    ("InputField", "rowid"),
    ("Bookmark", "BookmarkId"),
];

/// Favorites (mark/unmark, EDIT-05) affected-table set: `Tag` (the system
/// `Type=0 Name='Favorite'` row is created at most once, by the first-ever
/// mark), `Location` (created by mark; can be genuinely orphaned by unmark's
/// later `trim_sweep`), `TagMap` (the mark/unmark row itself). Follows
/// `archive::downgrade::DOWNGRADE_SNAPSHOT_TABLES`'s established per-op
/// precedent rather than reusing the broader [`TRACKED_TABLES`] default.
pub(crate) const FAVORITE_SNAPSHOT_TABLES: &[(&str, &str)] = &[
    ("Tag", "TagId"),
    ("Location", "LocationId"),
    ("TagMap", "TagMapId"),
];

/// Bookmarks import (08-02-PLAN.md Task 1) affected-table set: `Location`
/// (the scripture/publication/bookmark-container Locations `apply_import_bookmarks`
/// finds-or-inserts) and `Bookmark` (the upserted row itself). Same
/// narrow-set-over-`TRACKED_TABLES` precedent as [`FAVORITE_SNAPSHOT_TABLES`]
/// — narrowing keeps `diff_snapshots`'s `overwritten` count meaningful (an
/// UPDATEd Bookmark keeps its PK, so it lands in the before/after
/// intersection) without unrelated pre-existing rows in OTHER tables
/// polluting the count.
pub(crate) const BOOKMARK_SNAPSHOT_TABLES: &[(&str, &str)] =
    &[("Location", "LocationId"), ("Bookmark", "BookmarkId")];

/// Annotations import (08-02-PLAN.md Task 2) affected-table set: `Location`
/// (the dedup'd publication Location) and `InputField` (via its `rowid` —
/// `InputField`'s real PK, `(LocationId, TextTag)`, is non-integer, same
/// precedent as [`TRACKED_TABLES`]'s own `("InputField", "rowid")` entry). An
/// `ON CONFLICT DO UPDATE`'d `InputField` row keeps the same `rowid` (an
/// UPDATE, not a delete+insert), so it lands in the before/after
/// intersection and reports as `overwritten`.
pub(crate) const ANNOTATION_SNAPSHOT_TABLES: &[(&str, &str)] =
    &[("Location", "LocationId"), ("InputField", "rowid")];

/// Highlights import (08-03-PLAN.md Task 2) affected-table set: `Location`
/// (the scripture/publication Locations `apply_import_highlights`
/// finds-or-inserts), `UserMark` (ALWAYS a fresh row per record — the
/// accepted non-idempotency, RESEARCH Pitfall 5), and `BlockRange` (the
/// merged/inserted range; absorbed rows are DELETEd by
/// `merge_block_ranges`, so a `BlockRangeId` present before and absent after
/// reports as `deleted`, never silently vanishing from the count). Same
/// narrow-set-over-`TRACKED_TABLES` precedent as the other categories above.
pub(crate) const HIGHLIGHT_SNAPSHOT_TABLES: &[(&str, &str)] = &[
    ("Location", "LocationId"),
    ("UserMark", "UserMarkId"),
    ("BlockRange", "BlockRangeId"),
];

/// Notes import (08-04-PLAN.md Task 2) affected-table set: `Location` (the
/// scripture/publication Locations `apply_import_notes` finds-or-inserts),
/// `UserMark`/`BlockRange` (the second `merge_range_into` call site, same
/// non-idempotent-UserMark / convergent-BlockRange shape as
/// [`HIGHLIGHT_SNAPSHOT_TABLES`]), `Note` (the upserted note row itself —
/// an UPDATE keeps its PK, landing in `overwritten`; a bucket-delete removes
/// PKs, landing in `deleted`), and `Tag`/`TagMap` (the note's tag list,
/// re-processed on every import per `process_tags`'s DELETE-then-INSERT
/// shape).
pub(crate) const NOTE_IMPORT_SNAPSHOT_TABLES: &[(&str, &str)] = &[
    ("Location", "LocationId"),
    ("UserMark", "UserMarkId"),
    ("BlockRange", "BlockRangeId"),
    ("Note", "NoteId"),
    ("Tag", "TagId"),
    ("TagMap", "TagMapId"),
];

/// `.jwlplaylist` import (08-05-PLAN.md Task 2) affected-table set: `Tag`
/// (the playlist tag, created at most once per playlist name), `TagMap` (the
/// item's tag association), `PlaylistItem` (the re-keyed item row itself —
/// a reused/skipped item keeps its PK, landing in `overwritten`), `Location`
/// (the scripture/publication Location `apply_import_playlist`
/// finds-or-inserts), and `IndependentMedia` (the deduped-by-hash media
/// rows). `PlaylistItemMarker`, `PlaylistItemAccuracy` and the composite-key
/// junction tables (`PlaylistItem*Map`) have no meaningful single-column
/// identity for this diff and stay out of scope, same precedent as
/// [`TRACKED_TABLES`]'s own composite-key exclusions.
pub(crate) const PLAYLIST_IMPORT_SNAPSHOT_TABLES: &[(&str, &str)] = &[
    ("Tag", "TagId"),
    ("TagMap", "TagMapId"),
    ("PlaylistItem", "PlaylistItemId"),
    ("Location", "LocationId"),
    ("IndependentMedia", "IndependentMediaId"),
];

/// Playlist media delete (08-06-PLAN.md Task 3, D8-07) affected-table set:
/// `PlaylistItem` (the deleted item rows themselves), `PlaylistItemMarker`
/// (cascaded per-item), `TagMap` (the item's tag association, cascaded), and
/// `IndependentMedia` (the orphaned media rows `delete_playlist_items_db`
/// removes). The composite-key junction tables
/// (`PlaylistItem*Map`/`PlaylistItemMarker*Map`) have no single-column
/// identity and stay out of scope, same precedent as [`TRACKED_TABLES`]'s
/// own composite-key exclusions.
pub(crate) const MEDIA_DELETE_SNAPSHOT_TABLES: &[(&str, &str)] = &[
    ("PlaylistItem", "PlaylistItemId"),
    ("PlaylistItemMarker", "PlaylistItemMarkerId"),
    ("TagMap", "TagMapId"),
    ("IndependentMedia", "IndependentMediaId"),
];

pub(crate) fn snapshot_pks(
    tx: &Transaction,
    table: &str,
    pk_col: &str,
) -> Result<HashSet<i64>, ArchiveError> {
    let sql = format!("SELECT {pk_col} FROM {table}");
    let mut stmt = tx
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "snapshotting pks"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(|e| map_sqlite_err(e, "snapshotting pks"))?;
    let mut set = HashSet::new();
    for row in rows {
        set.insert(row.map_err(|e| map_sqlite_err(e, "snapshotting pks"))?);
    }
    Ok(set)
}

/// Snapshots the single-column integer PKs (or, for `InputField`, `rowid`)
/// of an arbitrary set of `tables` (each `(table, pk_col)`). Reused by every
/// dry-run in the app with its own table set — the diff logic must never be
/// copy-pasted.
pub(crate) fn snapshot_tables(
    tx: &Transaction,
    tables: &[(&str, &str)],
) -> Result<BTreeMap<String, HashSet<i64>>, ArchiveError> {
    let mut snapshot = BTreeMap::new();
    for (table, pk_col) in tables {
        snapshot.insert((*table).to_string(), snapshot_pks(tx, table, pk_col)?);
    }
    Ok(snapshot)
}

/// [`snapshot_tables`] over the default [`TRACKED_TABLES`] set — the shape
/// `db::delete`'s Notes-delete flow uses.
pub(crate) fn snapshot_all(
    tx: &Transaction,
) -> Result<BTreeMap<String, HashSet<i64>>, ArchiveError> {
    snapshot_tables(tx, TRACKED_TABLES)
}

/// Diffs a before/after pair of per-table PK snapshots into a
/// [`DryRunReport`]. A PK present in both sets is `overwritten` (this
/// includes the TagMap re-densify's preserved mappings and any
/// `Location.Title` normalization — SEMANTIC accounting per D2-07, never a
/// false `deleted`); a PK present only before is genuinely `deleted`; a PK
/// present only after is `added`.
pub(crate) fn diff_snapshots(
    before: &BTreeMap<String, HashSet<i64>>,
    after: &BTreeMap<String, HashSet<i64>>,
) -> DryRunReport {
    let mut report = DryRunReport::default();
    for (table, before_set) in before {
        let empty = HashSet::new();
        let after_set = after.get(table).unwrap_or(&empty);

        let deleted = before_set.difference(after_set).count();
        let added = after_set.difference(before_set).count();
        let overwritten = before_set.intersection(after_set).count();

        if deleted > 0 {
            report.deleted.insert(table.clone(), deleted);
        }
        if added > 0 {
            report.added.insert(table.clone(), added);
        }
        if overwritten > 0 {
            report.overwritten.insert(table.clone(), overwritten);
        }
    }
    report.total_deleted = report.deleted.values().sum();
    report
}
