//! Favorites `.txt` import (IO-02/IO-03, D8-04/D8-08) — ports
//! `import_favorites` (`JWLManager.py:2044-2123`).
//!
//! Two-stage shape (D8-04 fail-fast-whole-transaction): [`parse_favorites_file`]
//! runs ENTIRELY before any transaction opens and returns either a fully
//! parsed `Vec<FavoriteRecord>` or a typed `ImportMalformed` naming the exact
//! offending line — a short/long line can never reach SQL, and a malformed
//! file leaves zero rows changed because no transaction was ever opened for
//! it. [`apply_import_favorites`] then runs the parsed records inside the
//! caller's transaction, and [`dry_run_import_favorites`] wraps that in the
//! same never-committed `unchecked_transaction` + [`PragmaGuard`] shape every
//! other `dry_run_*` in this app uses.
//!
//! Strict UTF-8 read (the caller uses `std::fs::read_to_string`, which
//! rejects invalid UTF-8 as a typed `Io` error): Python's Annotations-only
//! `errors='namereplace'` leniency (`JWLManager.py:1939`) is deliberately NOT
//! reproduced anywhere in this phase (RESEARCH assumption A6) — `namereplace`
//! silently corrupts data by substituting undecodable bytes with escape
//! sequences, which this app's Core Value (never corrupt a user's archive)
//! forbids reproducing even for import compatibility.

use super::export::{join_row, read_favorite_lines};
use crate::db::edit::{diff_snapshots, snapshot_tables, DryRunReport, FAVORITE_SNAPSHOT_TABLES};
use crate::db::ids::{compute_available_ids, take_id};
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use rusqlite::{Connection, OptionalExtension, Transaction};
use std::collections::HashMap;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::ImportFailed {
        reason: format!("{context}: {err}"),
    }
}

/// One parsed Favorites data row, fields in the SAME order the wire format
/// (and `export_favorites`'s SQL) uses: `DocumentId, Track, IssueTagNumber,
/// KeySymbol, MepsLanguage, Type`. Kept as raw `Option<String>` (never
/// converted to a typed `i64`) because SQLite's column-affinity coercion on
/// INSERT is exactly what Python itself relies on (`import_favorites` never
/// converts these strings to `int` either, per `JWLManager.py:2100-2107`) —
/// converting them here would be a divergence, not a strengthening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteRecord {
    pub document_id: Option<String>,
    pub track: Option<String>,
    pub issue_tag_number: Option<String>,
    pub key_symbol: Option<String>,
    pub meps_language: Option<String>,
    /// The `Type` column — named `kind` because `type` is a Rust keyword.
    pub kind: Option<String>,
}

impl FavoriteRecord {
    /// The record's fields in wire-format column order, for [`join_row`] —
    /// the SAME helper `export_favorites` uses, so the dup-check comparison
    /// in [`apply_import_favorites`] is always like-with-like.
    fn as_fields(&self) -> [Option<&str>; 6] {
        [
            self.document_id.as_deref(),
            self.track.as_deref(),
            self.issue_tag_number.as_deref(),
            self.key_symbol.as_deref(),
            self.meps_language.as_deref(),
            self.kind.as_deref(),
        ]
    }

    fn formatted_line(&self) -> String {
        let fields = self.as_fields();
        join_row(&fields.map(|f| f.map(str::to_string)))
    }
}

/// Parses a whole Favorites `.txt` file's TEXT (already read as strict UTF-8
/// by the caller) into records, entirely BEFORE any transaction opens
/// (D8-04). Line 1 must contain the substring `{FAVORITES}` (unanchored —
/// `regex.search`, not `regex.match`, `JWLManager.py:2048`); every
/// subsequent line CONTAINING a `|` is treated as a data row and must split
/// into EXACTLY 6 pipe-delimited fields (`JWLManager.py:2100`) or the whole
/// parse fails, naming the exact 1-indexed line. Lines without a `|`
/// (blank lines, the header's own non-tag lines) are silently skipped,
/// exactly like Python's `if '|' in line` filter (`:2097`).
pub fn parse_favorites_file(text: &str) -> Result<Vec<FavoriteRecord>, ArchiveError> {
    let mut lines = text.split('\n');
    let first_line = lines.next().unwrap_or("");
    if !first_line.contains("{FAVORITES}") {
        return Err(ArchiveError::ImportMalformed {
            category: "Favorites".to_string(),
            line: 1,
            reason: "missing {FAVORITES} tag line".to_string(),
        });
    }

    let mut records = Vec::new();
    for (offset, raw_line) in lines.enumerate() {
        let line_no = offset + 2; // 1-indexed; line 1 already consumed above
        let line = raw_line.trim_end_matches('\r');
        if !line.contains('|') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() != 6 {
            return Err(ArchiveError::ImportMalformed {
                category: "Favorites".to_string(),
                line: line_no,
                reason: format!(
                    "expected 6 pipe-delimited fields, found {}",
                    fields.len()
                ),
            });
        }
        let mut opts = fields
            .into_iter()
            .map(|f| if f == "None" { None } else { Some(f.to_string()) });
        // `unwrap()` is safe here: `opts` always yields exactly 6 items
        // (the length check above already guaranteed it), never fewer.
        records.push(FavoriteRecord {
            document_id: opts.next().unwrap_or(None),
            track: opts.next().unwrap_or(None),
            issue_tag_number: opts.next().unwrap_or(None),
            key_symbol: opts.next().unwrap_or(None),
            meps_language: opts.next().unwrap_or(None),
            kind: opts.next().unwrap_or(None),
        });
    }
    Ok(records)
}

/// Resolves the system `Tag (Type = 0, Name = 'Favorite')`, creating it if
/// absent — reuses the exact find-or-create shape `db::favorites::apply_favorite_add`
/// step 1 established, extended here with [`take_id`] recycling (D8-08):
/// `Tag` is one of the nine [`crate::db::ids::RECYCLING_TABLES`], so an
/// import run must consume a recycled `TagId` before falling back to
/// autoincrement, exactly like every other insert in this module.
fn ensure_favorites_tag(
    tx: &Transaction,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "ensure_favorites_tag: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Tag") {
        tx.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (?1, 0, 'Favorite')",
            rusqlite::params![id],
        )
        .map_err(|e| map_sqlite_err(e, "ensure_favorites_tag: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Tag (Type, Name) VALUES (0, 'Favorite')",
            [],
        )
        .map_err(|e| map_sqlite_err(e, "ensure_favorites_tag: insert autoincrement"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// The six `Location` columns Favorites' publication-location shape uses, in
/// FIXED, compile-time-known order — never derived from a caller, the
/// frontend, or parsed file content (T-08-01/T-08-02). Only the VALUES bound
/// per column are dynamic (bound as typed params via `params_from_iter`,
/// SAFE-02); the column names and the `IS NULL`/`= ?` predicate SHAPE are
/// always drawn from this fixed array.
const LOCATION_COLUMNS: [&str; 6] = [
    "DocumentId",
    "Track",
    "IssueTagNumber",
    "KeySymbol",
    "MepsLanguage",
    "Type",
];

/// Finds or inserts the publication `Location` for one Favorite record —
/// ports `add_publication_location` (`JWLManager.py:2079-2091`) as a
/// SELECT-first-then-INSERT (rather than Python's `INSERT OR IGNORE` +
/// re-SELECT) so an explicit recycled `LocationId` can be supplied on
/// insert; `take_id`'s autoincrement fallback (`tx.last_insert_rowid()`)
/// otherwise matches Python's own `.lastrowid` path exactly.
fn find_or_insert_publication_location(
    tx: &Transaction,
    record: &FavoriteRecord,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let fields = record.as_fields();

    let mut conditions: Vec<String> = Vec::with_capacity(LOCATION_COLUMNS.len());
    let mut params: Vec<&str> = Vec::new();
    for (col, value) in LOCATION_COLUMNS.iter().zip(fields.iter()) {
        match value {
            None => conditions.push(format!("{col} IS NULL")),
            Some(v) => {
                conditions.push(format!("{col} = ?"));
                params.push(v);
            }
        }
    }
    let select_sql = format!(
        "SELECT LocationId FROM Location WHERE {}",
        conditions.join(" AND ")
    );
    let existing: Option<i64> = tx
        .query_row(&select_sql, rusqlite::params_from_iter(params.iter()), |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_insert_publication_location: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Location") {
        tx.execute(
            "INSERT INTO Location \
             (LocationId, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, fields[0], fields[1], fields[2], fields[3], fields[4], fields[5]],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_publication_location: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Location (DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![fields[0], fields[1], fields[2], fields[3], fields[4], fields[5]],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_publication_location: insert autoincrement"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Runs the ALREADY-PARSED `records` inside the caller's transaction
/// (`JWLManager.py:2054-2113`): resolves/creates the system Favorite tag,
/// builds the current-favorites line set via [`read_favorite_lines`] (the
/// SAME formatting `export_favorites` uses, so the dup check compares
/// like-with-like), then for each record in FILE order either skips it
/// (its formatted line already matches an existing favorite) or finds/
/// inserts its publication `Location` and inserts one new `TagMap` row at
/// the tag's next `Position`, incrementing per record. A genuine
/// `UNIQUE(TagId, LocationId)` violation on the `TagMap` insert surfaces as
/// a typed error and is never swallowed (08-CONTEXT specifics). Returns the
/// number of records SKIPPED as exact duplicates (the number ADDED is read
/// back from the caller's before/after snapshot diff, same as every other
/// `dry_run_*` in this app).
pub fn apply_import_favorites(
    tx: &Transaction,
    records: &[FavoriteRecord],
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<usize, ArchiveError> {
    let tag_id = ensure_favorites_tag(tx, available)?;

    let mut position: i64 = tx
        .query_row(
            "SELECT IFNULL(MAX(Position), -1) + 1 FROM TagMap WHERE TagId = ?1",
            rusqlite::params![tag_id],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "apply_import_favorites: compute starting position"))?;

    let current_lines: std::collections::HashSet<String> =
        read_favorite_lines(tx, None)?.into_iter().collect();

    let mut skipped = 0usize;
    for record in records {
        if current_lines.contains(&record.formatted_line()) {
            skipped += 1;
            continue;
        }

        let location_id = find_or_insert_publication_location(tx, record, available)?;

        if let Some(tagmap_id) = take_id(available, "TagMap") {
            tx.execute(
                "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
                 VALUES (?1, NULL, ?2, NULL, ?3, ?4)",
                rusqlite::params![tagmap_id, location_id, tag_id, position],
            )
            .map_err(|e| map_sqlite_err(e, "apply_import_favorites: insert tagmap (recycled id)"))?;
        } else {
            tx.execute(
                "INSERT INTO TagMap (PlaylistItemId, LocationId, NoteId, TagId, Position) \
                 VALUES (NULL, ?1, NULL, ?2, ?3)",
                rusqlite::params![location_id, tag_id, position],
            )
            .map_err(|e| map_sqlite_err(e, "apply_import_favorites: insert tagmap (autoincrement)"))?;
        }
        position += 1;
    }

    Ok(skipped)
}

/// Runs the REAL [`apply_import_favorites`] (already-parsed `records`) +
/// `trim_sweep` inside a transaction that is NEVER committed, returning a
/// SEMANTIC [`DryRunReport`] with `skipped` populated — same
/// snapshot/diff/`PragmaGuard` shape as `db::favorites::dry_run_favorite_add`.
/// The dry-run never touches the filesystem and never re-reads the source
/// file (that already happened before this was called, per D8-10 — the
/// caller re-parses once for dry-run and once for apply, accepting the
/// double-parse rather than caching parse state across the two IPC calls).
pub fn dry_run_import_favorites(
    conn: &mut Connection,
    records: &[FavoriteRecord],
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

    let mut available = compute_available_ids(&tx)?;
    let before = snapshot_tables(&tx, FAVORITE_SNAPSHOT_TABLES)?;
    let skipped = apply_import_favorites(&tx, records, &mut available)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, FAVORITE_SNAPSHOT_TABLES)?;

    let mut report = diff_snapshots(&before, &after);
    if skipped > 0 {
        report.skipped.insert("TagMap".to_string(), skipped);
    }

    drop(tx);
    drop(guard);

    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_missing_tag_line() {
        let err = parse_favorites_file("not a tag line\n1|2|0|nwt|0|1").unwrap_err();
        match err {
            ArchiveError::ImportMalformed { line, .. } => assert_eq!(line, 1),
            other => panic!("expected ImportMalformed, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_wrong_field_count() {
        let text = "{FAVORITES}\n \nExported from x\nby y (1) on z\n****\n1|2|0|nwt|0";
        let err = parse_favorites_file(text).unwrap_err();
        match err {
            ArchiveError::ImportMalformed { line, .. } => assert_eq!(line, 6),
            other => panic!("expected ImportMalformed, got {other:?}"),
        }
    }

    #[test]
    fn parse_maps_none_literal_to_option_none() {
        let text = "{FAVORITES}\nNone|Track|0|nwt|0|1";
        let records = parse_favorites_file(text).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].document_id, None);
        assert_eq!(records[0].track.as_deref(), Some("Track"));
    }

    #[test]
    fn parse_skips_lines_without_pipe() {
        let text = "{FAVORITES}\n \nheader line with no pipe\n1|2|0|nwt|0|1";
        let records = parse_favorites_file(text).unwrap();
        assert_eq!(records.len(), 1);
    }
}
