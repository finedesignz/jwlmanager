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
use super::usermark::{merge_range_into, synthesize_usermark};
use crate::db::edit::{
    diff_snapshots, snapshot_tables, DryRunReport, ANNOTATION_SNAPSHOT_TABLES,
    BOOKMARK_SNAPSHOT_TABLES, FAVORITE_SNAPSHOT_TABLES, HIGHLIGHT_SNAPSHOT_TABLES,
};
use crate::db::ids::{compute_available_ids, take_id};
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use rusqlite::{Connection, OptionalExtension, Transaction};
use std::collections::{BTreeMap, HashMap};

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

// ---------------------------------------------------------------------------
// Bookmarks (08-02-PLAN.md Task 1) — flat pipe rows, `¦` escaping wart, three
// distinct location-dedup predicates, upsert on `(PublicationLocationId,
// Slot)`. Ports `import_bookmarks` (`JWLManager.py:1958-2043`).
// ---------------------------------------------------------------------------

/// The 5 field positions Python unwraps the literal `'None'` string to
/// `None` on (`JWLManager.py:2021`): BookNumber(0), ChapterNumber(1),
/// DocumentId(2), Snippet(9), BlockIdentifier(11). Every OTHER field
/// (IssueTagNumber(3), KeySymbol(4), MepsLanguage(5), Type(6), Slot(7),
/// Title(8), BlockType(10)) is left as the raw string verbatim — Python
/// genuinely does not unwrap those, so a literal `'None'` in one of them
/// would be stored as the four-character TEXT `'None'`, not SQL NULL. Port
/// this asymmetry exactly; "fixing" it into a uniform per-field None-check
/// would diverge from Python.
const BOOKMARK_NULL_UNWRAP_INDICES: [usize; 5] = [0, 1, 2, 9, 11];

/// One parsed Bookmarks data row, fields in wire-format column order
/// (`JWLManager.py:1444`). Kept as raw `Option<String>` for the same reason
/// [`FavoriteRecord`] is — SQLite's column-affinity coercion on INSERT does
/// the string->int work, exactly like Python's own untyped bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkRecord {
    pub book_number: Option<String>,
    pub chapter_number: Option<String>,
    pub document_id: Option<String>,
    pub issue_tag_number: Option<String>,
    pub key_symbol: Option<String>,
    pub meps_language: Option<String>,
    /// The `Type` column — named `kind` because `type` is a Rust keyword.
    pub kind: Option<String>,
    pub slot: Option<String>,
    pub title: Option<String>,
    /// A literal `¦` (U+00A6) here is NEVER un-escaped back to `|` — Pitfall
    /// 2, `JWLManager.py:2020` never touches this substitution on import.
    pub snippet: Option<String>,
    pub block_type: Option<String>,
    pub block_identifier: Option<String>,
}

/// Parses a whole Bookmarks `.txt` file's TEXT into records, entirely BEFORE
/// any transaction opens (D8-04) — same two-stage shape as
/// [`parse_favorites_file`]. Line 1 must contain `{BOOKMARKS}`; every
/// subsequent line containing a `|` must split into EXACTLY 12
/// pipe-delimited fields (`JWLManager.py:2020`) or the whole parse fails,
/// naming the exact 1-indexed line.
pub fn parse_bookmarks_file(text: &str) -> Result<Vec<BookmarkRecord>, ArchiveError> {
    let mut lines = text.split('\n');
    let first_line = lines.next().unwrap_or("");
    if !first_line.contains("{BOOKMARKS}") {
        return Err(ArchiveError::ImportMalformed {
            category: "Bookmarks".to_string(),
            line: 1,
            reason: "missing {BOOKMARKS} tag line".to_string(),
        });
    }

    let mut records = Vec::new();
    for (offset, raw_line) in lines.enumerate() {
        let line_no = offset + 2;
        let line = raw_line.trim_end_matches('\r');
        if !line.contains('|') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() != 12 {
            return Err(ArchiveError::ImportMalformed {
                category: "Bookmarks".to_string(),
                line: line_no,
                reason: format!("expected 12 pipe-delimited fields, found {}", fields.len()),
            });
        }
        let mut opts = fields.into_iter().enumerate().map(|(i, f)| {
            if BOOKMARK_NULL_UNWRAP_INDICES.contains(&i) && f == "None" {
                None
            } else {
                Some(f.to_string())
            }
        });
        // `unwrap_or(None)` is safe here: `opts` always yields exactly 12
        // items (the length check above already guaranteed it).
        records.push(BookmarkRecord {
            book_number: opts.next().unwrap_or(None),
            chapter_number: opts.next().unwrap_or(None),
            document_id: opts.next().unwrap_or(None),
            issue_tag_number: opts.next().unwrap_or(None),
            key_symbol: opts.next().unwrap_or(None),
            meps_language: opts.next().unwrap_or(None),
            kind: opts.next().unwrap_or(None),
            slot: opts.next().unwrap_or(None),
            title: opts.next().unwrap_or(None),
            snippet: opts.next().unwrap_or(None),
            block_type: opts.next().unwrap_or(None),
            block_identifier: opts.next().unwrap_or(None),
        });
    }
    Ok(records)
}

/// Finds-or-inserts a SCRIPTURE `Location` for a Bookmark record — ports
/// `add_scripture_location` (`JWLManager.py:1970-1980`). Dedup key:
/// `KeySymbol + MepsLanguage + BookNumber + ChapterNumber` — DISTINCT from
/// [`find_or_insert_bookmark_publication_location`] and
/// [`find_or_insert_bookmark_container_location`] (D8-04: three separate
/// predicates, never collapsed into one generic helper).
fn find_or_insert_bookmark_scripture_location(
    tx: &Transaction,
    record: &BookmarkRecord,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT LocationId FROM Location \
             WHERE KeySymbol = ? AND MepsLanguage = ? AND BookNumber = ? AND ChapterNumber = ?",
            rusqlite::params![
                record.key_symbol,
                record.meps_language,
                record.book_number,
                record.chapter_number
            ],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_scripture_location: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Location") {
        tx.execute(
            "INSERT INTO Location (LocationId, KeySymbol, MepsLanguage, BookNumber, ChapterNumber, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id,
                record.key_symbol,
                record.meps_language,
                record.book_number,
                record.chapter_number,
                record.kind
            ],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_scripture_location: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Location (KeySymbol, MepsLanguage, BookNumber, ChapterNumber, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                record.key_symbol,
                record.meps_language,
                record.book_number,
                record.chapter_number,
                record.kind
            ],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_scripture_location: insert autoincrement"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Finds-or-inserts a PUBLICATION `Location` for a Bookmark record — ports
/// `add_publication_location` (`JWLManager.py:1982-1992`). Dedup key:
/// `KeySymbol + MepsLanguage + IssueTagNumber + DocumentId + Type` —
/// DISTINCT from the scripture predicate above.
fn find_or_insert_bookmark_publication_location(
    tx: &Transaction,
    record: &BookmarkRecord,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT LocationId FROM Location \
             WHERE KeySymbol = ? AND MepsLanguage = ? AND IssueTagNumber = ? AND DocumentId = ? AND Type = ?",
            rusqlite::params![
                record.key_symbol,
                record.meps_language,
                record.issue_tag_number,
                record.document_id,
                record.kind
            ],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_publication_location: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Location") {
        tx.execute(
            "INSERT INTO Location (LocationId, IssueTagNumber, KeySymbol, MepsLanguage, DocumentId, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id,
                record.issue_tag_number,
                record.key_symbol,
                record.meps_language,
                record.document_id,
                record.kind
            ],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_publication_location: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Location (IssueTagNumber, KeySymbol, MepsLanguage, DocumentId, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                record.issue_tag_number,
                record.key_symbol,
                record.meps_language,
                record.document_id,
                record.kind
            ],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_publication_location: insert autoincrement"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Finds-or-inserts the Bookmark's OWN CONTAINER `Location` (`Type = 1`) —
/// ports the inline resolution inside `add_bookmark`
/// (`JWLManager.py:1995-2003`). This is the THIRD, distinct dedup predicate:
/// `KeySymbol + MepsLanguage + Type = 1` with Book/Chapter/DocumentId all
/// NULL-or-zero.
fn find_or_insert_bookmark_container_location(
    tx: &Transaction,
    record: &BookmarkRecord,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT LocationId FROM Location \
             WHERE KeySymbol = ? AND MepsLanguage = ? AND Type = 1 \
             AND (BookNumber IS NULL OR BookNumber = 0) \
             AND (ChapterNumber IS NULL OR ChapterNumber = 0) \
             AND (DocumentId IS NULL OR DocumentId = 0)",
            rusqlite::params![record.key_symbol, record.meps_language],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_container_location: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Location") {
        tx.execute(
            "INSERT INTO Location (LocationId, KeySymbol, MepsLanguage, Type) VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![id, record.key_symbol, record.meps_language],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_container_location: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Location (KeySymbol, MepsLanguage, Type) VALUES (?1, ?2, 1)",
            rusqlite::params![record.key_symbol, record.meps_language],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_bookmark_container_location: insert autoincrement"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Upserts one Bookmark on `(PublicationLocationId, Slot)` — ports
/// `add_bookmark` (`JWLManager.py:1994-2013`). An existing row at that
/// `(publication_id, slot)` UPDATEs in place; the Bookmark row count stays
/// unchanged and `diff_snapshots` reports it under `overwritten` (the PK
/// survives the UPDATE, landing in both before/after snapshots).
fn upsert_bookmark(
    tx: &Transaction,
    record: &BookmarkRecord,
    location_id: i64,
    publication_id: i64,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<(), ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT BookmarkId FROM Bookmark WHERE PublicationLocationId = ? AND Slot = ?",
            rusqlite::params![publication_id, record.slot],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "upsert_bookmark: select"))?;

    if let Some(bookmark_id) = existing {
        tx.execute(
            "UPDATE Bookmark SET LocationId = ?, Title = ?, Snippet = ?, BlockType = ?, BlockIdentifier = ? \
             WHERE BookmarkId = ?",
            rusqlite::params![
                location_id,
                record.title,
                record.snippet,
                record.block_type,
                record.block_identifier,
                bookmark_id
            ],
        )
        .map_err(|e| map_sqlite_err(e, "upsert_bookmark: update"))?;
        return Ok(());
    }

    if let Some(id) = take_id(available, "Bookmark") {
        tx.execute(
            "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, Snippet, BlockType, BlockIdentifier) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                location_id,
                publication_id,
                record.slot,
                record.title,
                record.snippet,
                record.block_type,
                record.block_identifier
            ],
        )
        .map_err(|e| map_sqlite_err(e, "upsert_bookmark: insert recycled id"))?;
    } else {
        tx.execute(
            "INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title, Snippet, BlockType, BlockIdentifier) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                location_id,
                publication_id,
                record.slot,
                record.title,
                record.snippet,
                record.block_type,
                record.block_identifier
            ],
        )
        .map_err(|e| map_sqlite_err(e, "upsert_bookmark: insert autoincrement"))?;
    }
    Ok(())
}

/// Runs the ALREADY-PARSED Bookmarks `records` inside the caller's
/// transaction (`JWLManager.py:2015-2033`): for each record, resolves the
/// LOCATED Location (scripture if `BookNumber` is present/non-empty —
/// Python's own `if attribs[0]:` truthiness check, else publication),
/// resolves the bookmark's own container Location, then upserts the
/// Bookmark. Every new id is allocated via [`take_id`] before falling back to
/// autoincrement (D8-08).
pub fn apply_import_bookmarks(
    tx: &Transaction,
    records: &[BookmarkRecord],
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<(), ArchiveError> {
    for record in records {
        let is_scripture = record
            .book_number
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let location_id = if is_scripture {
            find_or_insert_bookmark_scripture_location(tx, record, available)?
        } else {
            find_or_insert_bookmark_publication_location(tx, record, available)?
        };
        let publication_id = find_or_insert_bookmark_container_location(tx, record, available)?;
        upsert_bookmark(tx, record, location_id, publication_id, available)?;
    }
    Ok(())
}

/// Runs the REAL [`apply_import_bookmarks`] + `trim_sweep` inside a
/// transaction that is NEVER committed, returning a SEMANTIC [`DryRunReport`]
/// over [`BOOKMARK_SNAPSHOT_TABLES`] — same shape as
/// [`dry_run_import_favorites`].
pub fn dry_run_import_bookmarks(
    conn: &mut Connection,
    records: &[BookmarkRecord],
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
    let before = snapshot_tables(&tx, BOOKMARK_SNAPSHOT_TABLES)?;
    apply_import_bookmarks(&tx, records, &mut available)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, BOOKMARK_SNAPSHOT_TABLES)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
    drop(guard);

    Ok(report)
}

// ---------------------------------------------------------------------------
// Annotations (08-02-PLAN.md Task 2) — bracket-tag records, `{END}`
// sentinel, conditional `{ISSUE}` bracket. Ports `import_annotations`
// (`JWLManager.py:1871-1956`).
// ---------------------------------------------------------------------------

/// One parsed Annotations record. `issue`/`doc` are `None` exactly when the
/// header omitted the bracket / wrote the literal `None` string
/// (`JWLManager.py:1922`'s `fill_null(0)` happens later, at insert time, in
/// [`find_or_insert_annotation_location`] — kept out of the record itself so
/// the record is a faithful, unmodified capture of the file's content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationRecord {
    pub pub_sym: String,
    pub issue: Option<i64>,
    pub doc: Option<i64>,
    pub label: String,
    /// The captured record body — NOT trimmed at parse time (Python:
    /// `attribs['VALUE'] = item.group(2)`, `:1897`); trimming happens at
    /// insert time (`.strip()`, `:1930`), in [`apply_import_annotations`].
    pub value: String,
}

/// Scans one record's header text for `{KEY=value}` pairs — ports
/// `process_header`'s `regex.findall('{(.*?)=(.*?)}', line)`
/// (`JWLManager.py:1883-1887`) as an explicit forward scan (no lookahead
/// needed here, unlike the record-boundary scan below).
fn parse_header_attrs(header: &str) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    let mut rest = header;
    while let Some(open) = rest.find('{') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            break;
        };
        let inner = &after_open[..close];
        if let Some(eq) = inner.find('=') {
            attrs.insert(inner[..eq].to_string(), inner[eq + 1..].to_string());
        }
        rest = &after_open[close + 1..];
    }
    attrs
}

/// Parses a whole Annotations `.txt` file's TEXT into records, entirely
/// BEFORE any transaction opens (D8-04). Line 1 must contain `{ANNOTATIONS}`.
///
/// Record boundaries are found via an explicit forward scan for the literal
/// 5-byte sequence `\n==={` — Rust's `regex` crate has no lookahead, so this
/// replaces Python's `regex.finditer('^===({.*?})===\n(.*?)(?=\n==={)', ...)`
/// (`JWLManager.py:1892`) with the equivalent boundary-list approach: each
/// occurrence of `\n==={` starts a new record (or, for the LAST occurrence,
/// terminates the preceding record as the `{END}` sentinel — the sentinel is
/// NEVER itself parsed as a data record, matching Python's lookahead
/// behavior exactly, RESEARCH `## Wire Formats` Annotations subsection).
/// A header lacking `PUB`/`DOC`/`LABEL`, or a record with no `===\n` header
/// terminator, is `ImportMalformed` naming the 1-indexed record number (the
/// `line` field is reused to carry a record index here, since bracket-tag
/// records have no single meaningful source line).
pub fn parse_annotations_file(text: &str) -> Result<Vec<AnnotationRecord>, ArchiveError> {
    let first_line_end = text.find('\n').unwrap_or(text.len());
    let first_line = &text[..first_line_end];
    if !first_line.contains("{ANNOTATIONS}") {
        return Err(ArchiveError::ImportMalformed {
            category: "Annotations".to_string(),
            line: 1,
            reason: "missing {ANNOTATIONS} tag line".to_string(),
        });
    }
    let rest = if first_line_end < text.len() {
        &text[first_line_end + 1..]
    } else {
        ""
    };

    let boundaries: Vec<usize> = rest
        .match_indices("\n===")
        .filter(|(idx, _)| rest[*idx + 4..].starts_with('{'))
        .map(|(idx, _)| idx)
        .collect();

    let mut records = Vec::new();
    if boundaries.len() < 2 {
        // Zero complete records (no data, or a dangling record with no
        // {END} terminator) — Python's regex simply yields no matches here
        // too; not an error.
        return Ok(records);
    }

    for (i, window) in boundaries.windows(2).enumerate() {
        let (start, end) = (window[0], window[1]);
        let chunk = &rest[start + 4..end]; // skip the leading "\n==="
        let record_no = i + 1;

        let Some(header_end) = chunk.find("===\n") else {
            return Err(ArchiveError::ImportMalformed {
                category: "Annotations".to_string(),
                line: record_no,
                reason: "unterminated record header".to_string(),
            });
        };
        let header = &chunk[..header_end];
        let body = &chunk[header_end + 4..];

        let attrs = parse_header_attrs(header);
        let malformed = |reason: &str| ArchiveError::ImportMalformed {
            category: "Annotations".to_string(),
            line: record_no,
            reason: reason.to_string(),
        };

        let pub_sym = attrs
            .get("PUB")
            .cloned()
            .ok_or_else(|| malformed("missing {PUB=...} attribute"))?;
        let doc_raw = attrs
            .get("DOC")
            .ok_or_else(|| malformed("missing {DOC=...} attribute"))?;
        let doc = if doc_raw == "None" {
            None
        } else {
            Some(
                doc_raw
                    .parse::<i64>()
                    .map_err(|_| malformed(&format!("unparseable DOC value: {doc_raw}")))?,
            )
        };
        let label = attrs
            .get("LABEL")
            .cloned()
            .ok_or_else(|| malformed("missing {LABEL=...} attribute"))?;
        let issue = match attrs.get("ISSUE") {
            Some(raw) => Some(
                raw.parse::<i64>()
                    .map_err(|_| malformed(&format!("unparseable ISSUE value: {raw}")))?,
            ),
            None => None,
        };

        records.push(AnnotationRecord {
            pub_sym,
            issue,
            doc,
            label,
            value: body.to_string(),
        });
    }
    Ok(records)
}

/// Finds-or-inserts the Annotation's `Location` — ports `add_location`
/// (`JWLManager.py:1909-1919`). Dedup key: `DocumentId + IssueTagNumber +
/// KeySymbol + MepsLanguage IS NULL + Type = 0`, with a missing `ISSUE`
/// bracket filled to `0` (never NULL) BEFORE the query, matching
/// `fill_null(0)` (`:1922`).
fn find_or_insert_annotation_location(
    tx: &Transaction,
    record: &AnnotationRecord,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let issue = record.issue.unwrap_or(0);
    let existing: Option<i64> = tx
        .query_row(
            "SELECT LocationId FROM Location \
             WHERE DocumentId = ? AND IssueTagNumber = ? AND KeySymbol = ? AND MepsLanguage IS NULL AND Type = 0",
            rusqlite::params![record.doc, issue, record.pub_sym],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_insert_annotation_location: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Location") {
        tx.execute(
            "INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (?1, ?2, ?3, ?4, NULL, 0)",
            rusqlite::params![id, record.doc, issue, record.pub_sym],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_annotation_location: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Location (DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (?1, ?2, ?3, NULL, 0)",
            rusqlite::params![record.doc, issue, record.pub_sym],
        )
        .map_err(|e| map_sqlite_err(e, "find_or_insert_annotation_location: insert autoincrement"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Runs the ALREADY-PARSED Annotations `records` inside the caller's
/// transaction (`JWLManager.py:1926-1935`): resolves/creates each record's
/// `Location`, then upserts `InputField` on `(LocationId, TextTag)` — a
/// re-imported annotation UPDATEs `Value` in place rather than duplicating.
/// The `Value` is trimmed HERE, at insert time (`.strip()`, `:1930`), not at
/// parse time.
pub fn apply_import_annotations(
    tx: &Transaction,
    records: &[AnnotationRecord],
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<(), ArchiveError> {
    for record in records {
        let location_id = find_or_insert_annotation_location(tx, record, available)?;
        tx.execute(
            "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?1, ?2, ?3) \
             ON CONFLICT(LocationId, TextTag) DO UPDATE SET Value = excluded.Value",
            rusqlite::params![location_id, record.label, record.value.trim()],
        )
        .map_err(|e| map_sqlite_err(e, "apply_import_annotations: upsert InputField"))?;
    }
    Ok(())
}

/// Runs the REAL [`apply_import_annotations`] + `trim_sweep` inside a
/// transaction that is NEVER committed, returning a SEMANTIC [`DryRunReport`]
/// over [`ANNOTATION_SNAPSHOT_TABLES`] — same shape as
/// [`dry_run_import_favorites`]/[`dry_run_import_bookmarks`].
pub fn dry_run_import_annotations(
    conn: &mut Connection,
    records: &[AnnotationRecord],
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
    let before = snapshot_tables(&tx, ANNOTATION_SNAPSHOT_TABLES)?;
    apply_import_annotations(&tx, records, &mut available)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, ANNOTATION_SNAPSHOT_TABLES)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
    drop(guard);

    Ok(report)
}

// ---------------------------------------------------------------------------
// Highlights (08-03-PLAN.md Task 2) — the RANGE-MERGE import call site.
// Ports `import_highlights` (`JWLManager.py:2124-2211`).
// ---------------------------------------------------------------------------

/// One parsed Highlights data row, fields in the SQL's column order
/// (`JWLManager.py:1476`/`:2191`). `block_type`/`identifier`/`start_token`/
/// `end_token`/`color_index`/`version` are parsed to `i64` because
/// [`crate::db::io::usermark::synthesize_usermark`] and
/// [`crate::db::io::usermark::merge_range_into`] both require typed
/// integers — a strengthening over Python's untyped SQL bind for exactly
/// these six fields (which the schema declares NOT NULL on the
/// `UserMark`/`BlockRange` side, so a genuine file would never carry
/// anything else there). The remaining seven Location-predicate fields stay
/// raw `String` (never `Option<String>`) because [`parse_highlights_file`]
/// ports Python's BLANKET `'None'`->`''` replacement BEFORE splitting
/// (RESEARCH assumption A5) rather than a per-field None-check — an actual
/// NULL renders as an EMPTY STRING here, not a Rust `None`, exactly
/// reproducing Python's fragile-but-intentional behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightRecord {
    pub block_type: i64,
    pub identifier: i64,
    pub start_token: i64,
    pub end_token: i64,
    pub color_index: i64,
    pub version: i64,
    pub book_number: String,
    pub chapter_number: String,
    pub document_id: String,
    pub issue_tag_number: String,
    pub key_symbol: String,
    pub meps_language: String,
    /// The `Type` column — named `kind` because `type` is a Rust keyword.
    pub kind: String,
}

/// Parses a whole Highlights `.txt` file's TEXT into records, entirely
/// BEFORE any transaction opens (D8-04). Line 1 must contain `{HIGHLIGHTS}`.
///
/// Every SUBSEQUENT line is tested against the line-shape guard
/// `^(\d+\|){6}` (`JWLManager.py:2188`) — at least 6 digit-groups each
/// followed by `|` — which skips header/blank/divider lines WITHOUT needing
/// a line-count offset (RESEARCH `## Wire Formats` Highlights subsection). A
/// line that passes the guard has the literal substring `None` BLANKET-
/// replaced with the empty string, ported verbatim (NOT "fixed" into a
/// per-field check — RESEARCH assumption A5: a field whose real value
/// happens to contain the substring "None" would be corrupted, exactly as in
/// Python, but `KeySymbol`s are always short alphanumeric codes that never
/// contain it in practice), THEN split on `|` and required to yield EXACTLY
/// 13 fields or the whole parse fails naming the 1-indexed line. The six
/// integer fields ([`HighlightRecord::block_type`] etc.) are parsed here too
/// — an unparseable one is also `ImportMalformed` (Python's own bare `except`
/// around `int(attribs[2])`/`int(attribs[3])` at `:2168-2169` aborts the
/// whole import with a ROLLBACK the same way, `:2197-2200`).
pub fn parse_highlights_file(text: &str) -> Result<Vec<HighlightRecord>, ArchiveError> {
    let mut lines = text.split('\n');
    let first_line = lines.next().unwrap_or("");
    if !first_line.contains("{HIGHLIGHTS}") {
        return Err(ArchiveError::ImportMalformed {
            category: "Highlights".to_string(),
            line: 1,
            reason: "missing {HIGHLIGHTS} tag line".to_string(),
        });
    }

    // Matches `^(\d+\|){6}`: at least 6 groups of one-or-more ASCII digits
    // each immediately followed by `|`.
    fn has_highlights_line_shape(line: &str) -> bool {
        let mut rest = line;
        for _ in 0..6 {
            let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if digit_count == 0 {
                return false;
            }
            rest = &rest[digit_count..];
            let Some(stripped) = rest.strip_prefix('|') else {
                return false;
            };
            rest = stripped;
        }
        true
    }

    let mut records = Vec::new();
    for (offset, raw_line) in lines.enumerate() {
        let line_no = offset + 2; // 1-indexed; line 1 already consumed above
        if !has_highlights_line_shape(raw_line) {
            continue;
        }
        // Python: `line.rstrip().replace('None', '')` — rstrip (all
        // trailing whitespace) BEFORE the blanket replace, then split.
        let cleaned = raw_line.trim_end().replace("None", "");
        let fields: Vec<&str> = cleaned.split('|').collect();
        if fields.len() != 13 {
            return Err(ArchiveError::ImportMalformed {
                category: "Highlights".to_string(),
                line: line_no,
                reason: format!("expected 13 pipe-delimited fields, found {}", fields.len()),
            });
        }

        let malformed_int = |field_name: &str, raw: &str| ArchiveError::ImportMalformed {
            category: "Highlights".to_string(),
            line: line_no,
            reason: format!("unparseable {field_name} value: {raw:?}"),
        };
        let parse_i64 = |idx: usize, field_name: &str| -> Result<i64, ArchiveError> {
            fields[idx]
                .parse::<i64>()
                .map_err(|_| malformed_int(field_name, fields[idx]))
        };

        records.push(HighlightRecord {
            block_type: parse_i64(0, "BlockType")?,
            identifier: parse_i64(1, "Identifier")?,
            start_token: parse_i64(2, "StartToken")?,
            end_token: parse_i64(3, "EndToken")?,
            color_index: parse_i64(4, "ColorIndex")?,
            version: parse_i64(5, "Version")?,
            book_number: fields[6].to_string(),
            chapter_number: fields[7].to_string(),
            document_id: fields[8].to_string(),
            issue_tag_number: fields[9].to_string(),
            key_symbol: fields[10].to_string(),
            meps_language: fields[11].to_string(),
            kind: fields[12].to_string(),
        });
    }
    Ok(records)
}

/// Finds-or-inserts a SCRIPTURE `Location` for a Highlight record — ports
/// `add_scripture_location` (`JWLManager.py:2136-2146`). Dedup key:
/// `KeySymbol + MepsLanguage + BookNumber + ChapterNumber` — DISTINCT from
/// [`find_or_insert_highlight_publication_location`] (D8-04: two separate
/// predicates per category, never collapsed, and never shared with
/// Bookmarks' own same-shaped predicate — each category's location
/// resolution is its own port of its own Python function).
fn find_or_insert_highlight_scripture_location(
    tx: &Transaction,
    record: &HighlightRecord,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT LocationId FROM Location \
             WHERE KeySymbol = ? AND MepsLanguage = ? AND BookNumber = ? AND ChapterNumber = ?",
            rusqlite::params![
                record.key_symbol,
                record.meps_language,
                record.book_number,
                record.chapter_number
            ],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_insert_highlight_scripture_location: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Location") {
        tx.execute(
            "INSERT INTO Location (LocationId, KeySymbol, MepsLanguage, BookNumber, ChapterNumber, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id,
                record.key_symbol,
                record.meps_language,
                record.book_number,
                record.chapter_number,
                record.kind
            ],
        )
        .map_err(|e| {
            map_sqlite_err(e, "find_or_insert_highlight_scripture_location: insert recycled id")
        })?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Location (KeySymbol, MepsLanguage, BookNumber, ChapterNumber, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                record.key_symbol,
                record.meps_language,
                record.book_number,
                record.chapter_number,
                record.kind
            ],
        )
        .map_err(|e| {
            map_sqlite_err(e, "find_or_insert_highlight_scripture_location: insert autoincrement")
        })?;
        Ok(tx.last_insert_rowid())
    }
}

/// Finds-or-inserts a PUBLICATION `Location` for a Highlight record — ports
/// `add_publication_location` (`JWLManager.py:2148-2158`). Dedup key:
/// `KeySymbol + MepsLanguage + IssueTagNumber + DocumentId + Type` — DISTINCT
/// from the scripture predicate above.
fn find_or_insert_highlight_publication_location(
    tx: &Transaction,
    record: &HighlightRecord,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT LocationId FROM Location \
             WHERE KeySymbol = ? AND MepsLanguage = ? AND IssueTagNumber = ? AND DocumentId = ? AND Type = ?",
            rusqlite::params![
                record.key_symbol,
                record.meps_language,
                record.issue_tag_number,
                record.document_id,
                record.kind
            ],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "find_or_insert_highlight_publication_location: select"))?;
    if let Some(id) = existing {
        return Ok(id);
    }

    if let Some(id) = take_id(available, "Location") {
        tx.execute(
            "INSERT INTO Location (LocationId, IssueTagNumber, KeySymbol, MepsLanguage, DocumentId, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id,
                record.issue_tag_number,
                record.key_symbol,
                record.meps_language,
                record.document_id,
                record.kind
            ],
        )
        .map_err(|e| {
            map_sqlite_err(e, "find_or_insert_highlight_publication_location: insert recycled id")
        })?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Location (IssueTagNumber, KeySymbol, MepsLanguage, DocumentId, Type) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                record.issue_tag_number,
                record.key_symbol,
                record.meps_language,
                record.document_id,
                record.kind
            ],
        )
        .map_err(|e| {
            map_sqlite_err(
                e,
                "find_or_insert_highlight_publication_location: insert autoincrement",
            )
        })?;
        Ok(tx.last_insert_rowid())
    }
}

/// Runs the ALREADY-PARSED Highlights `records` inside the caller's
/// transaction (`JWLManager.py:2186-2201`): for each record, in FILE order,
/// resolves the LOCATED Location (scripture if `BookNumber` is
/// present/non-empty — Python's own `if attribs[6]:` truthiness check, else
/// publication), synthesizes a FRESH `UserMark`
/// ([`synthesize_usermark`] — never looked up/reused, D8-05/RESEARCH
/// Pitfall 5), then merges the record's range into the existing BlockRange
/// set at `(Identifier, LocationId)` via [`merge_range_into`], which
/// delegates the geometry entirely to `db::highlights::merge_block_ranges`.
/// `guid_seed` is XORed with the record's index so a multi-record file never
/// mints two identical GUIDs in one run, while the SAME seed across two
/// separate calls (e.g. dry-run then apply) reproduces the same GUIDs —
/// mirrors `db::color::apply_color`'s `guid_seed ^ note_id` pattern exactly.
pub fn apply_import_highlights(
    tx: &Transaction,
    records: &[HighlightRecord],
    available: &mut HashMap<&'static str, Vec<i64>>,
    guid_seed: u64,
) -> Result<(), ArchiveError> {
    for (index, record) in records.iter().enumerate() {
        let is_scripture = !record.book_number.is_empty();
        let location_id = if is_scripture {
            find_or_insert_highlight_scripture_location(tx, record, available)?
        } else {
            find_or_insert_highlight_publication_location(tx, record, available)?
        };

        let user_mark_id = synthesize_usermark(
            tx,
            location_id,
            record.color_index,
            record.version,
            guid_seed ^ (index as u64),
            available,
        )?;

        merge_range_into(
            tx,
            record.identifier,
            location_id,
            record.start_token,
            record.end_token,
            record.block_type,
            user_mark_id,
            available,
        )?;
    }
    Ok(())
}

/// Runs the REAL [`apply_import_highlights`] + `trim_sweep` inside a
/// transaction that is NEVER committed, returning a SEMANTIC [`DryRunReport`]
/// over [`HIGHLIGHT_SNAPSHOT_TABLES`] — same shape as
/// [`dry_run_import_bookmarks`]/[`dry_run_import_annotations`].
pub fn dry_run_import_highlights(
    conn: &mut Connection,
    records: &[HighlightRecord],
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

    let mut available = compute_available_ids(&tx)?;
    let before = snapshot_tables(&tx, HIGHLIGHT_SNAPSHOT_TABLES)?;
    apply_import_highlights(&tx, records, &mut available, guid_seed)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, HIGHLIGHT_SNAPSHOT_TABLES)?;

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

    // -----------------------------------------------------------------
    // Highlights (08-03-PLAN.md Task 2)
    // -----------------------------------------------------------------

    #[test]
    fn highlights_parse_rejects_missing_tag_line() {
        let err = parse_highlights_file("not a tag line\n1|1|0|5|1|1|1|1|0|0|nwt|0|0").unwrap_err();
        match err {
            ArchiveError::ImportMalformed { category, line, .. } => {
                assert_eq!(category, "Highlights");
                assert_eq!(line, 1);
            }
            other => panic!("expected ImportMalformed, got {other:?}"),
        }
    }

    #[test]
    fn highlights_parse_skips_header_and_divider_lines_without_offset() {
        let text = "{HIGHLIGHTS}\n \nExported from x\nby y (1) on z\n****\n1|1|0|5|1|1|1|1|0|0|nwt|0|0";
        let records = parse_highlights_file(text).unwrap();
        assert_eq!(records.len(), 1, "only the one real data line should parse");
        assert_eq!(records[0].identifier, 1);
    }

    #[test]
    fn highlights_parse_rejects_wrong_field_count() {
        // Passes the `^(\d+\|){6}` shape guard but has only 12 fields total.
        let text = "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|0|0|nwt|0";
        let err = parse_highlights_file(text).unwrap_err();
        match err {
            ArchiveError::ImportMalformed { line, reason, .. } => {
                assert_eq!(line, 2);
                assert!(reason.contains("12"), "reason should name the actual field count: {reason}");
            }
            other => panic!("expected ImportMalformed, got {other:?}"),
        }
    }

    #[test]
    fn highlights_parse_blanket_replaces_none_before_splitting() {
        // Field 6 (BookNumber) is the literal string "None" -> becomes "" after
        // the blanket replace, NOT a Rust `Option::None` (RESEARCH A5).
        let text = "{HIGHLIGHTS}\n1|1|0|5|1|1|None|None|1001|0|pub-x|0|0";
        let records = parse_highlights_file(text).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].book_number, "");
        assert_eq!(records[0].chapter_number, "");
        assert_eq!(records[0].document_id, "1001");
    }

    #[test]
    fn highlights_parse_rejects_unparseable_integer_field() {
        // All-digits (so the `^(\d+\|){6}` line-shape guard still treats it
        // as a data line) but too large to fit `i64`.
        let text = "{HIGHLIGHTS}\n1|1|99999999999999999999|5|1|1|1|1|0|0|nwt|0|0";
        let err = parse_highlights_file(text).unwrap_err();
        assert!(matches!(err, ArchiveError::ImportMalformed { .. }));
    }

    #[test]
    fn highlights_parse_scripture_truthiness_matches_bookmarks_pattern() {
        // A publication record: BookNumber (field 6) is empty.
        let text = "{HIGHLIGHTS}\n1|1|0|5|1|1||0|1001|0|pub-x|0|0";
        let records = parse_highlights_file(text).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].book_number.is_empty());
    }
}
