//! Tag add/remove/rename backend (EDIT-03, 07-03-PLAN.md Task 1). Ports
//! `tag_notes` (`JWLManager.py:3281-3386`): the tri-state row counts
//! (`get_notes`, `:3283-3301`), the free-id gap-fill (`get_available_ids`,
//! `:3303-3315`/`:1857-1869`), the per-selected-notes unmark pass
//! (`delete_tags`, `:3317-3331`), and the per-selected-notes mark/create
//! pass (`add_tags`, `:3333-3361`).
//!
//! Follows the D7-01 safety pattern generalized in `db::edit` — same shape
//! as `db::favorites`/`db::color`: a typed non-empty selection wrapper
//! ([`crate::db::delete::NonEmptyNoteIds`], reused rather than redefined),
//! `apply_tag_edit(tx, ...)` inside the caller's transaction, `dry_run_tag_edit(conn, ...)`
//! in a never-committed `unchecked_transaction` under [`PragmaGuard`],
//! returning a semantic [`DryRunReport`].
//!
//! **`TagMap` carries THREE `UNIQUE` constraints** — `(TagId, Position)`,
//! `(TagId, NoteId)`, `(TagId, LocationId)` — plus a CHECK that exactly one
//! of `PlaylistItemId`/`NoteId`/`LocationId` is non-NULL. Note-tag mappings
//! written here always set `NoteId` and leave `PlaylistItemId`/`LocationId`
//! `NULL`, and every insert is `INSERT OR IGNORE` (guarding `(TagId,
//! NoteId)` — a note already carrying a tag is silently skipped, never a
//! constraint error) at a freshly-computed `Position` (guarding `(TagId,
//! Position)` — recomputed via `MAX(Position)+1` before each insert, never
//! stale across the loop).
//!
//! **ID recycling matches Python's `get_available_ids` gap-fill EXACTLY**,
//! including its perhaps-surprising fill ORDER: `get_available_ids` builds
//! the free-id list in ascending order, then reverses it
//! (`available[::-1]`) before `.pop(0)`-ing from the front — which means the
//! Python source fills the LARGEST free gap first, not the smallest. This
//! module reproduces that behavior with a plain ascending `Vec<i64>` and
//! `Vec::pop()` (removing the LAST/largest element), which is exactly
//! equivalent to Python's reverse-then-pop-front — no semantic difference,
//! just a simpler Rust idiom for the identical id-selection order.

use crate::db::delete::NonEmptyNoteIds;
use crate::db::edit::{diff_snapshots, snapshot_tables, DryRunReport};
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::TagFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Affected-table set for tag edit's `DryRunReport`: `Tag` (a brand-new tag
/// name is `added`) and `TagMap` (mark/unmark rows). Follows the per-op-
/// table-set precedent (`db::favorites::FAVORITE_SNAPSHOT_TABLES`) rather
/// than the broader default.
pub(crate) const TAG_SNAPSHOT_TABLES: &[(&str, &str)] =
    &[("Tag", "TagId"), ("TagMap", "TagMapId")];

/// One `Tag WHERE Type = 1` row's tri-state count for a given note
/// selection — `count == 0` (unchecked), `count == selection.len()`
/// (checked), otherwise indeterminate. Ports `get_notes`'s per-tag SUM
/// (`JWLManager.py:3287-3300`). The frontend derives the checkbox
/// tri-state purely from `count` vs. the selection size it already knows —
/// no separate boolean flags cross the IPC boundary.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/TagState.ts")]
pub struct TagState {
    pub tag_id: i64,
    pub name: String,
    pub count: i64,
}

fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(",")
}

/// Every `Tag WHERE Type = 1` row with the count of `ids` that currently
/// carry it. Ports `get_notes` (`JWLManager.py:3283-3301`) — a single
/// `LEFT JOIN` + conditional `SUM`, ordered by `Name` (matching Python's
/// `ORDER BY t.Name`). `ids` is bound as typed params (SAFE-02); only the
/// placeholder COUNT is dynamic, never an interpolated id value — the
/// anti-pattern the Python source itself commits at `:3285`
/// (`str(selected).replace(...)`) is deliberately not ported.
pub fn tag_states(conn: &Connection, ids: &NonEmptyNoteIds) -> Result<Vec<TagState>, ArchiveError> {
    let note_ph = placeholders(ids.len());
    let sql = format!(
        "SELECT t.TagId, t.Name, \
             SUM(CASE WHEN tm.NoteId IN ({note_ph}) THEN 1 ELSE 0 END) AS c \
         FROM Tag t \
             LEFT JOIN TagMap tm ON tm.TagId = t.TagId \
         WHERE t.Type = 1 \
         GROUP BY t.TagId \
         ORDER BY t.Name"
    );
    let mut stmt = tag_states_prepare(conn, &sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            Ok(TagState {
                tag_id: row.get(0)?,
                name: row.get(1)?,
                count: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            })
        })
        .map_err(|e| map_sqlite_err(e, "tag_states: query"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_sqlite_err(e, "tag_states: read rows"))?;
    Ok(rows)
}

fn tag_states_prepare<'a>(
    conn: &'a Connection,
    sql: &str,
) -> Result<rusqlite::Statement<'a>, ArchiveError> {
    conn.prepare(sql)
        .map_err(|e| map_sqlite_err(e, "tag_states: prepare"))
}

/// `get_available_ids`-equivalent gap-fill (`JWLManager.py:1857-1869`,
/// `:3303-3315`) over a single table: the ascending list of ids skipped
/// between `1` and the table's current max id. Callers take from the END
/// via `Vec::pop()` — the largest gap first, matching Python's
/// reverse-then-pop-front exactly (module docs). `table` is always one of
/// the two fixed literals `"Tag"`/`"TagMap"` passed by this module's own
/// callers, never derived from user input.
fn compute_available_ids(tx: &Transaction, table: &str) -> Result<Vec<i64>, ArchiveError> {
    let sql = format!("SELECT {table}Id FROM {table} ORDER BY {table}Id");
    let mut stmt = tx
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "compute_available_ids: prepare"))?;
    let existing: Vec<i64> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| map_sqlite_err(e, "compute_available_ids: query"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_sqlite_err(e, "compute_available_ids: read rows"))?;

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

/// Inserts one `TagMap` row linking `note_id` to `tag_id` at
/// `Position = ifnull(max(Position), -1) + 1` for that tag
/// (`JWLManager.py:3351`, recomputed fresh before every insert so a run of
/// several inserts for the same tag never collides), recycling a free
/// `TagMapId` from `available_tagmap_ids` when one exists (else a plain
/// autoincrement insert). `INSERT OR IGNORE` guards `UNIQUE(TagId, NoteId)`
/// — a note that already carries this tag is silently skipped (zero rows
/// changed), exactly like Python's own `rowcount`-checked but
/// otherwise-uncaught `INSERT OR IGNORE` (`:3354`/`:3356`). The recycled id
/// is popped regardless of whether the insert actually lands (matching
/// Python: the id is consumed from the pool either way, never "given back"
/// on an ignored insert).
fn insert_tagmap(
    tx: &Transaction,
    note_id: i64,
    tag_id: i64,
    available_tagmap_ids: &mut Vec<i64>,
) -> Result<(), ArchiveError> {
    let position: i64 = tx
        .query_row(
            "SELECT IFNULL(MAX(Position), -1) + 1 FROM TagMap WHERE TagId = ?1",
            rusqlite::params![tag_id],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "insert_tagmap: compute position"))?;

    if let Some(tagmap_id) = available_tagmap_ids.pop() {
        tx.execute(
            "INSERT OR IGNORE INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (?1, NULL, NULL, ?2, ?3, ?4)",
            rusqlite::params![tagmap_id, note_id, tag_id, position],
        )
    } else {
        tx.execute(
            "INSERT OR IGNORE INTO TagMap (PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (NULL, NULL, ?1, ?2, ?3)",
            rusqlite::params![note_id, tag_id, position],
        )
    }
    .map_err(|e| map_sqlite_err(e, "insert_tagmap: insert"))?;
    Ok(())
}

/// Finds an existing `Tag WHERE Type = 1 AND Name = ?` row, or creates one
/// (recycling a free `TagId` from `available_tag_ids` when one exists, else
/// a plain autoincrement insert) — ports the `tag_id is None` branch of
/// `add_tags` (`JWLManager.py:3341-3350`). A tag name that already exists
/// (e.g. the user typed a name matching a tag they didn't check in the
/// list) is reused rather than duplicated, matching Python's own
/// find-before-create check.
fn find_or_create_tag(
    tx: &Transaction,
    name: &str,
    available_tag_ids: &mut Vec<i64>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT TagId FROM Tag WHERE Type = 1 AND Name = ?1",
            rusqlite::params![name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_create_tag: lookup"))?;
    if let Some(tag_id) = existing {
        return Ok(tag_id);
    }

    if let Some(tag_id) = available_tag_ids.pop() {
        tx.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (?1, 1, ?2)",
            rusqlite::params![tag_id, name],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_create_tag: insert recycled id"))?;
        Ok(tag_id)
    } else {
        tx.execute(
            "INSERT INTO Tag (Type, Name) VALUES (1, ?1)",
            rusqlite::params![name],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_create_tag: insert new id"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Applies a tag edit for `ids` (a Notes selection) — ports `tag_notes`'s
/// commit body (`delete_tags` then `add_tags`, `JWLManager.py:3372-3376`):
///
/// 1. `removed_tag_ids`: deletes `TagMap` rows for `(NoteId IN ids, TagId IN
///    removed_tag_ids)` — ONLY for the selected notes, leaving other notes'
///    mappings for the same tag untouched (`delete_tags`, `:3317-3331`).
/// 2. `added_tag_ids`: for every selected note missing the tag, inserts a
///    `TagMap` row (`add_tags`'s `tag_id is not None` branch, `:3338-3358`).
/// 3. `new_tag_names`: finds-or-creates each named `Tag` row, then maps it
///    to every selected note (`add_tags`'s `tag_id is None` branch,
///    `:3341-3358`).
///
/// Runs inside the caller's transaction; every SQL statement binds only
/// typed params (SAFE-02).
pub fn apply_tag_edit(
    tx: &Transaction,
    ids: &NonEmptyNoteIds,
    removed_tag_ids: &[i64],
    added_tag_ids: &[i64],
    new_tag_names: &[String],
) -> Result<(), ArchiveError> {
    if !removed_tag_ids.is_empty() {
        let note_ph = placeholders(ids.len());
        let tag_ph = placeholders(removed_tag_ids.len());
        let sql =
            format!("DELETE FROM TagMap WHERE NoteId IN ({note_ph}) AND TagId IN ({tag_ph})");
        let params: Vec<i64> = ids
            .iter()
            .copied()
            .chain(removed_tag_ids.iter().copied())
            .collect();
        tx.execute(&sql, rusqlite::params_from_iter(params.iter()))
            .map_err(|e| map_sqlite_err(e, "apply_tag_edit: remove tags"))?;
    }

    // Computed ONCE up front (matching `add_tags`'s single `get_available_ids()`
    // call, `:3335`) and threaded through every insert below — never
    // recomputed mid-loop, so ids already handed out this call are never
    // reused within it.
    let mut available_tag_ids = compute_available_ids(tx, "Tag")?;
    let mut available_tagmap_ids = compute_available_ids(tx, "TagMap")?;

    for &tag_id in added_tag_ids {
        for &note_id in ids.iter() {
            insert_tagmap(tx, note_id, tag_id, &mut available_tagmap_ids)?;
        }
    }

    for name in new_tag_names {
        let tag_id = find_or_create_tag(tx, name, &mut available_tag_ids)?;
        for &note_id in ids.iter() {
            insert_tagmap(tx, note_id, tag_id, &mut available_tagmap_ids)?;
        }
    }

    Ok(())
}

/// Runs [`apply_tag_edit`] and returns the resulting semantic
/// [`DryRunReport`], snapshotting [`TAG_SNAPSHOT_TABLES`] before/after
/// inside the caller's (already-open, real, committing) transaction — the
/// shape `db::favorites::apply_favorite_add_reporting` established.
pub fn apply_tag_edit_reporting(
    tx: &Transaction,
    ids: &NonEmptyNoteIds,
    removed_tag_ids: &[i64],
    added_tag_ids: &[i64],
    new_tag_names: &[String],
) -> Result<DryRunReport, ArchiveError> {
    let before = snapshot_tables(tx, TAG_SNAPSHOT_TABLES)?;
    apply_tag_edit(tx, ids, removed_tag_ids, added_tag_ids, new_tag_names)?;
    let after = snapshot_tables(tx, TAG_SNAPSHOT_TABLES)?;
    Ok(diff_snapshots(&before, &after))
}

/// Runs the REAL `apply_tag_edit` + `trim_sweep` inside a transaction that
/// is NEVER committed and returns a SEMANTIC [`DryRunReport`] — copied
/// verbatim in shape from `db::favorites::dry_run_favorite_add`, swapping
/// only the mutation call and the affected-table set. Leaves the DB
/// unchanged (SAFE-01).
pub fn dry_run_tag_edit(
    conn: &mut Connection,
    ids: &NonEmptyNoteIds,
    removed_tag_ids: &[i64],
    added_tag_ids: &[i64],
    new_tag_names: &[String],
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

    let before = snapshot_tables(&tx, TAG_SNAPSHOT_TABLES)?;
    apply_tag_edit(&tx, ids, removed_tag_ids, added_tag_ids, new_tag_names)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, TAG_SNAPSHOT_TABLES)?;

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
    fn placeholders_join_with_commas() {
        assert_eq!(placeholders(3), "?,?,?");
        assert_eq!(placeholders(1), "?");
    }
}
