//! `.txt` export (IO-01) — the `'None'`-sentinel row-join helper shared by
//! every category, plus Favorites' export function
//! (`export_favorites`, `JWLManager.py:1454-1468`).
//!
//! Byte-exactness is the point of this phase: `export_wireformat_tests.rs`
//! compares the written file's bytes against a hand-authored golden fixture,
//! never a normalized/parsed comparison.

use super::header::{build_export_header, ExportHeaderCtx};
use crate::db::color::NonEmptyBlockRangeIds;
use crate::db::delete::{NonEmptyBookmarkIds, NonEmptyLocationIds, NonEmptyNoteIds};
use crate::db::favorites::NonEmptyTagMapIds;
use crate::db::resources::ResourceCatalog;
use crate::error::ArchiveError;
use rusqlite::types::Value;
use rusqlite::Connection;
use std::io::Write;
use std::path::Path;

/// Favorites never writes an `==={END}===` sentinel — unlike Annotations,
/// which does (`JWLManager.py:1420`). Encoded as an explicit stated fact
/// (RESEARCH Pitfall 1) rather than a silent omission, so a future category
/// module can't "fix" this into a spurious consistency.
#[allow(dead_code)] // documented fact, referenced by module docs/tests rather than code
pub(crate) const FAVORITES_WRITES_END_SENTINEL: bool = false;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::ExportFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Converts one SQLite cell to `Option<String>` matching Python's
/// `str(x) if x is not None else None` — an integer renders as its plain
/// decimal string (never e.g. `1.0`), text renders verbatim, `NULL` renders
/// as `None` (the Rust `Option`, NOT yet the wire literal — [`join_row`]
/// applies the `'None'` STRING sentinel at join time, keeping the two
/// concerns separate).
fn value_to_field(value: Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Integer(i) => Some(i.to_string()),
        Value::Real(f) => Some(f.to_string()),
        Value::Text(s) => Some(s),
        Value::Blob(_) => None,
    }
}

/// Joins one row's fields with `|`, rendering `None` as the literal
/// four-character string `None` — ports
/// `'|'.join(str(x) if x is not None else 'None' for x in row)`
/// (`JWLManager.py:1445`/`:1461`/`:1477`, identical across every `.txt`
/// category). This EXACT helper is reused by Task 3's import dup-check
/// (`db::io::import`) so the two paths format like-with-like.
pub(crate) fn join_row(values: &[Option<String>]) -> String {
    values
        .iter()
        .map(|v| v.as_deref().unwrap_or("None"))
        .collect::<Vec<_>>()
        .join("|")
}

/// Reads every current Favorite row (or, when `ids` is given, exactly the
/// selected `TagMapId`s) in `Position` order, formatted via [`join_row`].
/// Reused verbatim by Task 3's import dup-check (`get_current` port) so the
/// "already present" comparison is always like-with-like against THIS export
/// path's own formatting.
pub(crate) fn read_favorite_lines(
    conn: &Connection,
    ids: Option<&NonEmptyTagMapIds>,
) -> Result<Vec<String>, ArchiveError> {
    let base_sql = "SELECT DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type \
         FROM Location JOIN TagMap USING (LocationId) \
         WHERE TagId = (SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite')";

    let (sql, bound): (String, Vec<i64>) = match ids {
        Some(ids) => {
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            (
                format!("{base_sql} AND TagMapId IN ({placeholders}) ORDER BY Position"),
                ids.iter().copied().collect(),
            )
        }
        None => (format!("{base_sql} ORDER BY Position"), Vec::new()),
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "read_favorite_lines: prepare"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            let mut fields = Vec::with_capacity(6);
            for i in 0..6 {
                let value: Value = row.get(i)?;
                fields.push(value);
            }
            Ok(fields)
        })
        .map_err(|e| map_sqlite_err(e, "read_favorite_lines: query"))?;

    let mut lines = Vec::new();
    for row in rows {
        let fields = row.map_err(|e| map_sqlite_err(e, "read_favorite_lines: read row"))?;
        let fields: Vec<Option<String>> = fields.into_iter().map(value_to_field).collect();
        lines.push(join_row(&fields));
    }
    Ok(lines)
}

/// Exports Favorites (whole category when `ids` is `None`, D8-10
/// selection-optional) to `path` as a `.txt` file: [`build_export_header`]
/// tagged `{FAVORITES}`, then one `\n`-prefixed data row per favorite, in
/// `Position` order. Writes NO `==={END}===` sentinel
/// ([`FAVORITES_WRITES_END_SENTINEL`]). UTF-8, no BOM. Returns the row count.
/// Never mutates the archive (D8-09) — this is a pure read + file write.
pub fn export_favorites(
    conn: &Connection,
    ids: Option<&NonEmptyTagMapIds>,
    header: &ExportHeaderCtx,
    path: &Path,
) -> Result<usize, ArchiveError> {
    let lines = read_favorite_lines(conn, ids)?;

    let mut file = std::fs::File::create(path).map_err(ArchiveError::from)?;
    file.write_all(build_export_header(header).as_bytes())
        .map_err(ArchiveError::from)?;
    for line in &lines {
        file.write_all(b"\n").map_err(ArchiveError::from)?;
        file.write_all(line.as_bytes()).map_err(ArchiveError::from)?;
    }

    Ok(lines.len())
}

/// Bookmarks never writes an `==={END}===` sentinel (flat row format, same
/// asymmetry as [`FAVORITES_WRITES_END_SENTINEL`]) — Annotations, below,
/// DOES (RESEARCH Pitfall 1). Kept as a named, documented constant rather
/// than a bare `false` literal at the call site so a future refactor can't
/// silently drop the fact.
#[allow(dead_code)] // documented fact, referenced by module docs/tests rather than code
pub(crate) const BOOKMARKS_WRITES_END_SENTINEL: bool = false;

/// Annotations DOES write an `==={END}===` sentinel — the counterpart to
/// [`BOOKMARKS_WRITES_END_SENTINEL`]/[`FAVORITES_WRITES_END_SENTINEL`].
#[allow(dead_code)] // documented fact, referenced by module docs/tests rather than code
pub(crate) const ANNOTATIONS_WRITES_END_SENTINEL: bool = true;

/// Reads every Bookmark row (or, when `ids` is given, exactly the selected
/// `BookmarkId`s) formatted via [`join_row`], applying the SAME `|`->`¦`
/// (U+00A6 BROKEN BAR) substitution Python's SQL `REPLACE()` performs on
/// `Title`/`Snippet` ONLY (`JWLManager.py:1444`) — done IN SQL, not Rust
/// string code, so the substitution happens at the identical layer/collation
/// as Python (RESEARCH `## Wire Formats` Bookmarks subsection). No `ORDER
/// BY` — Python's own `export_bookmarks` has none either, so row order is
/// whatever SQLite's natural scan order yields.
fn read_bookmark_lines(
    conn: &Connection,
    ids: Option<&NonEmptyBookmarkIds>,
) -> Result<Vec<String>, ArchiveError> {
    let base_sql = "SELECT l.BookNumber, l.ChapterNumber, l.DocumentId, l.IssueTagNumber, \
         l.KeySymbol, l.MepsLanguage, l.Type, Slot, REPLACE(b.Title, \"|\", \"\u{A6}\"), \
         REPLACE(Snippet, \"|\", \"\u{A6}\"), BlockType, BlockIdentifier \
         FROM Bookmark b LEFT JOIN Location l USING (LocationId)";

    let (sql, bound): (String, Vec<i64>) = match ids {
        Some(ids) => {
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            (
                format!("{base_sql} WHERE BookmarkId IN ({placeholders})"),
                ids.iter().copied().collect(),
            )
        }
        None => (base_sql.to_string(), Vec::new()),
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "read_bookmark_lines: prepare"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            let mut fields = Vec::with_capacity(12);
            for i in 0..12 {
                let value: Value = row.get(i)?;
                fields.push(value);
            }
            Ok(fields)
        })
        .map_err(|e| map_sqlite_err(e, "read_bookmark_lines: query"))?;

    let mut lines = Vec::new();
    for row in rows {
        let fields = row.map_err(|e| map_sqlite_err(e, "read_bookmark_lines: read row"))?;
        let fields: Vec<Option<String>> = fields.into_iter().map(value_to_field).collect();
        lines.push(join_row(&fields));
    }
    Ok(lines)
}

/// Exports Bookmarks (whole category when `ids` is `None`, D8-10
/// selection-optional) to `path` as a `.txt` file: [`build_export_header`]
/// tagged `{BOOKMARKS}`, then one `\n`-prefixed 12-field data row per
/// bookmark. Writes NO `==={END}===` sentinel
/// ([`BOOKMARKS_WRITES_END_SENTINEL`]). Never mutates the archive (D8-09).
pub fn export_bookmarks(
    conn: &Connection,
    ids: Option<&NonEmptyBookmarkIds>,
    header: &ExportHeaderCtx,
    path: &Path,
) -> Result<usize, ArchiveError> {
    let lines = read_bookmark_lines(conn, ids)?;

    let mut file = std::fs::File::create(path).map_err(ArchiveError::from)?;
    file.write_all(build_export_header(header).as_bytes())
        .map_err(ArchiveError::from)?;
    for line in &lines {
        file.write_all(b"\n").map_err(ArchiveError::from)?;
        file.write_all(line.as_bytes()).map_err(ArchiveError::from)?;
    }

    Ok(lines.len())
}

/// One exported Annotation record's already-formatted attributes, ordered
/// per `JWLManager.py:1394-1404`.
struct AnnotationExportRow {
    label: String,
    value: String,
    /// `str(DocumentId)` — literally the four-character string `None` when
    /// NULL (`JWLManager.py:1418`'s `str(item['DOC'])`), unlike `pub_sym`
    /// below which is never wrapped in `str()` by Python.
    doc: String,
    /// `IssueTagNumber` when `> 10000000`, else omitted entirely (never
    /// rendered as `{ISSUE=None}` or `{ISSUE=0}`) — `JWLManager.py:1400-1403`.
    issue: Option<i64>,
    /// The raw `KeySymbol` string. Python concatenates this directly
    /// (`'{PUB='+item['PUB']+'}'`, NOT `str()`-wrapped) — a NULL `KeySymbol`
    /// would raise a `TypeError` and crash Python's own export; this is a
    /// pathological/corrupt-archive case no valid data hits, so Rust renders
    /// an empty string rather than reproducing a crash (a documented,
    /// harmless strengthening, not a behavior Rust needs "parity" with).
    pub_sym: String,
}

/// Reads every Annotation (`InputField`) row (or, when `ids` is given,
/// exactly the selected `LocationId`s — the Annotations browse-list identity,
/// `db::delete::NonEmptyLocationIds`'s doc comment) into
/// [`AnnotationExportRow`]s, `ORDER BY doc, i` exactly as Python's SQL
/// (`JWLManager.py:1378-1392`) — `i` is the numeric suffix parsed out of
/// `TextTag` via `CAST(TRIM(TextTag, 'abcdefghijklmnopqrstuvwxyz') AS INT)`.
fn read_annotation_rows(
    conn: &Connection,
    ids: Option<&NonEmptyLocationIds>,
) -> Result<Vec<AnnotationExportRow>, ArchiveError> {
    let base_sql = "SELECT TextTag, Value, l.DocumentId doc, l.IssueTagNumber, l.KeySymbol, \
         CAST(TRIM(TextTag, 'abcdefghijklmnopqrstuvwxyz') AS INT) i \
         FROM InputField LEFT JOIN Location l USING (LocationId) \
         WHERE Value <> '' AND Value IS NOT NULL";

    let (sql, bound): (String, Vec<i64>) = match ids {
        Some(ids) => {
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            (
                format!("{base_sql} AND LocationId IN ({placeholders}) ORDER BY doc, i"),
                ids.iter().copied().collect(),
            )
        }
        None => (format!("{base_sql} ORDER BY doc, i"), Vec::new()),
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "read_annotation_rows: prepare"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            let label: String = row.get(0)?;
            let value: String = row.get(1)?;
            let doc: Option<i64> = row.get(2)?;
            let issue_tag_number: Option<i64> = row.get(3)?;
            let pub_sym: Option<String> = row.get(4)?;
            Ok((label, value, doc, issue_tag_number, pub_sym))
        })
        .map_err(|e| map_sqlite_err(e, "read_annotation_rows: query"))?;

    let mut out = Vec::new();
    for row in rows {
        let (label, value, doc, issue_tag_number, pub_sym) =
            row.map_err(|e| map_sqlite_err(e, "read_annotation_rows: read row"))?;
        out.push(AnnotationExportRow {
            label,
            value: value.trim().to_string(),
            doc: doc.map(|d| d.to_string()).unwrap_or_else(|| "None".to_string()),
            issue: issue_tag_number.filter(|n| *n > 10_000_000),
            pub_sym: pub_sym.unwrap_or_default(),
        });
    }
    Ok(out)
}

/// Exports Annotations (whole category when `ids` is `None`, D8-10
/// selection-optional) to `path` as a `.txt` file: [`build_export_header`]
/// tagged `{ANNOTATIONS}`, then per record
/// `\n==={PUB=…}[{ISSUE=…}]{DOC=…}{LABEL=…}===\n<Value>`, then the literal
/// `\n==={END}===` with NO trailing newline
/// ([`ANNOTATIONS_WRITES_END_SENTINEL`], the counterpart to Bookmarks'
/// `false` — RESEARCH Pitfall 1). Never mutates the archive (D8-09).
pub fn export_annotations(
    conn: &Connection,
    ids: Option<&NonEmptyLocationIds>,
    header: &ExportHeaderCtx,
    path: &Path,
) -> Result<usize, ArchiveError> {
    let rows = read_annotation_rows(conn, ids)?;

    let mut file = std::fs::File::create(path).map_err(ArchiveError::from)?;
    file.write_all(build_export_header(header).as_bytes())
        .map_err(ArchiveError::from)?;
    for row in &rows {
        file.write_all(b"\n===").map_err(ArchiveError::from)?;
        file.write_all(format!("{{PUB={}}}", row.pub_sym).as_bytes())
            .map_err(ArchiveError::from)?;
        if let Some(issue) = row.issue {
            file.write_all(format!("{{ISSUE={issue}}}").as_bytes())
                .map_err(ArchiveError::from)?;
        }
        file.write_all(format!("{{DOC={}}}{{LABEL={}}}===\n", row.doc, row.label).as_bytes())
            .map_err(ArchiveError::from)?;
        file.write_all(row.value.as_bytes())
            .map_err(ArchiveError::from)?;
    }
    file.write_all(b"\n==={END}===").map_err(ArchiveError::from)?;

    Ok(rows.len())
}

// ---------------------------------------------------------------------------
// Highlights (08-03-PLAN.md Task 1) — 13 flat pipe fields, no `¦` escaping,
// no `{END}` sentinel. Ports `export_highlights` (`JWLManager.py:1470-1484`).
// ---------------------------------------------------------------------------

/// Highlights never writes an `==={END}===` sentinel — same asymmetry as
/// [`BOOKMARKS_WRITES_END_SENTINEL`]/[`FAVORITES_WRITES_END_SENTINEL`].
#[allow(dead_code)] // documented fact, referenced by module docs/tests rather than code
pub(crate) const HIGHLIGHTS_WRITES_END_SENTINEL: bool = false;

/// Reads every Highlight (`BlockRange`) row (or, when `ids` is given, exactly
/// the selected `BlockRangeId`s — the Highlights browse-list identity,
/// `db::color::NonEmptyBlockRangeIds`'s doc comment) formatted via
/// [`join_row`], in the exact 13-column order Python's SQL selects:
/// `BlockType, Identifier, StartToken, EndToken, ColorIndex, Version,
/// BookNumber, ChapterNumber, DocumentId, IssueTagNumber, KeySymbol,
/// MepsLanguage, Type` (`JWLManager.py:1476`). No `ORDER BY` — Python's own
/// `export_highlights` has none either.
fn read_highlight_lines(
    conn: &Connection,
    ids: Option<&NonEmptyBlockRangeIds>,
) -> Result<Vec<String>, ArchiveError> {
    let base_sql = "SELECT b.BlockType, b.Identifier, b.StartToken, b.EndToken, u.ColorIndex, \
         u.Version, l.BookNumber, l.ChapterNumber, l.DocumentId, l.IssueTagNumber, \
         l.KeySymbol, l.MepsLanguage, l.Type \
         FROM UserMark u JOIN Location l USING (LocationId) JOIN BlockRange b USING (UserMarkId)";

    let (sql, bound): (String, Vec<i64>) = match ids {
        Some(ids) => {
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            (
                format!("{base_sql} WHERE b.BlockRangeId IN ({placeholders})"),
                ids.iter().copied().collect(),
            )
        }
        None => (base_sql.to_string(), Vec::new()),
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "read_highlight_lines: prepare"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            let mut fields = Vec::with_capacity(13);
            for i in 0..13 {
                let value: Value = row.get(i)?;
                fields.push(value);
            }
            Ok(fields)
        })
        .map_err(|e| map_sqlite_err(e, "read_highlight_lines: query"))?;

    let mut lines = Vec::new();
    for row in rows {
        let fields = row.map_err(|e| map_sqlite_err(e, "read_highlight_lines: read row"))?;
        let fields: Vec<Option<String>> = fields.into_iter().map(value_to_field).collect();
        lines.push(join_row(&fields));
    }
    Ok(lines)
}

/// Exports Highlights (whole category when `ids` is `None`, D8-10
/// selection-optional) to `path` as a `.txt` file: [`build_export_header`]
/// tagged `{HIGHLIGHTS}`, then one `\n`-prefixed 13-field data row per
/// BlockRange. Writes NO `==={END}===` sentinel
/// ([`HIGHLIGHTS_WRITES_END_SENTINEL`]). Never mutates the archive (D8-09).
pub fn export_highlights(
    conn: &Connection,
    ids: Option<&NonEmptyBlockRangeIds>,
    header: &ExportHeaderCtx,
    path: &Path,
) -> Result<usize, ArchiveError> {
    let lines = read_highlight_lines(conn, ids)?;

    let mut file = std::fs::File::create(path).map_err(ArchiveError::from)?;
    file.write_all(build_export_header(header).as_bytes())
        .map_err(ArchiveError::from)?;
    for line in &lines {
        file.write_all(b"\n").map_err(ArchiveError::from)?;
        file.write_all(line.as_bytes()).map_err(ArchiveError::from)?;
    }

    Ok(lines.len())
}

// ---------------------------------------------------------------------------
// Notes (08-04-PLAN.md Task 1) — bracket-tag records with the widest
// optional-tag vocabulary of any category. Ports `export_notes`'s txt branch
// (`JWLManager.py:1636-1668`) plus the shared `get_notes` item-derivation
// (`:1552-1622`) that runs unconditionally before the format branch.
// ---------------------------------------------------------------------------

/// Notes DOES write an `==={END}===` sentinel — same shape as
/// [`ANNOTATIONS_WRITES_END_SENTINEL`].
#[allow(dead_code)] // documented fact, referenced by module docs/tests rather than code
pub(crate) const NOTES_WRITES_END_SENTINEL: bool = true;

/// One raw `Note` export row, straight off the SQL (`JWLManager.py:1519-1542`)
/// before any of `get_notes`'s per-shape derivation runs.
pub(crate) struct RawNoteRow {
    note_id: i64,
    book_number: Option<i64>,
    document_id: Option<i64>,
    title: Option<String>,
    content: Option<String>,
    tags: Option<String>,
    meps_language: Option<i64>,
    chapter_number: Option<i64>,
    /// `n.BlockIdentifier` — read once here and reused as BOTH Python's
    /// `VS` and `BLOCK` dict entries (`JWLManager.py:1560-1561`, both set
    /// from the same `row[7]`); the per-shape derivation below reproduces
    /// which of the two identifiers survives to the wire.
    block_identifier: Option<i64>,
    issue_tag_number: Option<i64>,
    key_symbol: Option<String>,
    location_title: Option<String>,
    last_modified: String,
    created: String,
    color_index: Option<i64>,
    user_mark_id: Option<i64>,
}

/// Reads the merged `RANGE` string for one `UserMarkId` — ports the inline
/// `BlockRange` sub-select (`JWLManager.py:1610-1615`): `identifier:start-end`
/// sub-ranges joined by `;`, ordered by `(Identifier, StartToken)`. `None`
/// when the UserMark carries no ranges at all (an un-highlighted-but-marked
/// edge case, or simply a UserMark with zero BlockRange rows).
fn read_note_range(conn: &Connection, user_mark_id: i64) -> Result<Option<String>, ArchiveError> {
    let mut stmt = conn
        .prepare(
            "SELECT Identifier, StartToken, EndToken FROM BlockRange \
             WHERE UserMarkId = ?1 ORDER BY Identifier, StartToken",
        )
        .map_err(|e| map_sqlite_err(e, "read_note_range: prepare"))?;
    let rows = stmt
        .query_map(rusqlite::params![user_mark_id], |row| {
            let identifier: i64 = row.get(0)?;
            let start: i64 = row.get(1)?;
            let end: i64 = row.get(2)?;
            Ok(format!("{identifier}:{start}-{end}"))
        })
        .map_err(|e| map_sqlite_err(e, "read_note_range: query"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_sqlite_err(e, "read_note_range: read rows"))?;
    if rows.is_empty() {
        Ok(None)
    } else {
        Ok(Some(rows.join(";")))
    }
}

fn read_raw_note_rows(
    conn: &Connection,
    ids: Option<&NonEmptyNoteIds>,
) -> Result<Vec<RawNoteRow>, ArchiveError> {
    let base_sql = "SELECT n.NoteId, l.BookNumber, l.DocumentId, n.Title, n.Content, \
         (SELECT GROUP_CONCAT(t.Name, ' | ') FROM Note nt \
              LEFT JOIN TagMap USING (NoteId) JOIN Tag t USING (TagId) \
              WHERE nt.NoteId = n.NoteId), \
         l.MepsLanguage, l.ChapterNumber, n.BlockIdentifier, l.IssueTagNumber, l.KeySymbol, \
         l.Title, n.LastModified, n.Created, u.ColorIndex, n.UserMarkId \
         FROM Note n \
             LEFT JOIN Location l USING (LocationId) \
             LEFT JOIN UserMark u USING (UserMarkId)";

    let (sql, bound): (String, Vec<i64>) = match ids {
        Some(ids) => {
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            (
                format!(
                    "{base_sql} WHERE n.NoteId IN ({placeholders}) \
                     GROUP BY n.NoteId ORDER BY n.BlockType, n.LastModified DESC"
                ),
                ids.iter().copied().collect(),
            )
        }
        None => (
            format!("{base_sql} GROUP BY n.NoteId ORDER BY n.BlockType, n.LastModified DESC"),
            Vec::new(),
        ),
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "read_raw_note_rows: prepare"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            Ok(RawNoteRow {
                note_id: row.get(0)?,
                book_number: row.get(1)?,
                document_id: row.get(2)?,
                title: row.get(3)?,
                content: row.get(4)?,
                tags: row.get(5)?,
                meps_language: row.get(6)?,
                chapter_number: row.get(7)?,
                block_identifier: row.get(8)?,
                issue_tag_number: row.get(9)?,
                key_symbol: row.get(10)?,
                location_title: row.get(11)?,
                last_modified: row.get(12)?,
                created: row.get(13)?,
                color_index: row.get(14)?,
                user_mark_id: row.get(15)?,
            })
        })
        .map_err(|e| map_sqlite_err(e, "read_raw_note_rows: query"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_sqlite_err(e, "read_raw_note_rows: read rows"))?;
    Ok(rows)
}

/// First 19 characters (never bytes — these are ASCII ISO date strings, so
/// char/byte truncation is identical, but this is explicit about which
/// invariant is relied on).
fn take19(s: &str) -> String {
    s.chars().take(19).collect()
}

/// Exports Notes (whole category when `ids` is `None`, D8-10
/// selection-optional) to `path` as a `.txt` file: [`build_export_header`]
/// tagged `{NOTES=}`, then per record the bracket header with the exact
/// per-shape optional-tag ORDER (`JWLManager.py:1636-1667`), then
/// `===\n<TITLE>\n<NOTE>`, then the literal `\n==={END}===`
/// ([`NOTES_WRITES_END_SENTINEL`]). `catalog` supplies the Bible book name
/// for the HEADING auto-fill fallback (`bible_books[BK]`); `now` supplies the
/// data-hygiene fallback for a corrupted `CREATED`/`MODIFIED` timestamp
/// (`JWLManager.py:1602-1608`) — both injected, never read from the wall
/// clock inside this function. Never mutates the archive (D8-09).
#[allow(clippy::too_many_arguments)]
pub fn export_notes(
    conn: &Connection,
    ids: Option<&NonEmptyNoteIds>,
    catalog: &ResourceCatalog,
    header: &ExportHeaderCtx,
    now: &str,
    path: &Path,
) -> Result<usize, ArchiveError> {
    let raw_rows = read_raw_note_rows(conn, ids)?;

    let mut file = std::fs::File::create(path).map_err(ArchiveError::from)?;
    file.write_all(build_export_header(header).as_bytes())
        .map_err(ArchiveError::from)?;

    let mut count = 0usize;
    for raw in &raw_rows {
        count += 1;
        let range = match raw.user_mark_id {
            Some(id) => read_note_range(conn, id)?,
            None => None,
        };
        let record = format_note_record(raw, range.as_deref(), catalog, now);
        file.write_all(record.as_bytes()).map_err(ArchiveError::from)?;
    }
    file.write_all(b"\n==={END}===").map_err(ArchiveError::from)?;

    Ok(count)
}

/// Renders the exact bytes [`export_notes`]'s write loop writes for ONE
/// `Note` record — the leading newline, the `\n==={CREATED=...}` opener,
/// every per-shape bracket, the `===\n<TITLE>\n<NOTE>` body — everything
/// except the shared header (written once, before the loop) and the
/// trailing `==={END}===` sentinel (written once, after it). Pure
/// extraction (09-01-PLAN.md Task 1): `export_notes` now calls this and
/// writes the returned string, so the exported bytes and the incremental
/// diff's live-side hash input can never drift apart — [`db::io::diff`]
/// reuses this SAME function via [`read_note_id_records`] below.
pub(crate) fn format_note_record(
    raw: &RawNoteRow,
    range: Option<&str>,
    catalog: &ResourceCatalog,
    now: &str,
) -> String {
    let title = raw.title.clone().unwrap_or_default();
    let note = raw
        .content
        .as_deref()
        .map(|c| c.trim().to_string())
        .unwrap_or_default();
    let tags = raw.tags.clone().unwrap_or_default().replace(" | ", "|");
    let color = raw.color_index.unwrap_or(0);

    // `JWLManager.py:1602-1608`'s data-hygiene fallback for a corrupted
    // stored timestamp — a rare, non-load-bearing edge case ported for
    // completeness, not exercised by the golden fixture.
    let mut created = take19(&raw.created);
    let mut modified = take19(&raw.last_modified);
    if !created.contains('-') || created.chars().count() < 10 {
        created = now.to_string();
    } else if !modified.contains('T') {
        modified = format!("{}T00:00:00", modified.chars().take(10).collect::<String>());
    }
    if !modified.contains('-') || modified.chars().count() < 10 {
        modified = created.clone();
    } else if !created.contains('T') {
        created = format!("{}T00:00:00", created.chars().take(10).collect::<String>());
    }

    let mut out =
        format!("\n==={{CREATED={created}}}{{MODIFIED={modified}}}{{TAGS={tags}}}");

    // `item.get('DOC')` truthiness (`JWLManager.py:1622`/`:1655`/`:1661`):
    // a `DocumentId` of `0` is treated as absent, same as `NULL`.
    let doc_present = raw.document_id.filter(|d| *d != 0);

    if let Some(bk) = raw.book_number {
        // Bible-shaped (`:1646-1657`). `VS` is the raw BlockIdentifier;
        // `BLOCK` becomes unconditionally `None` once BK is present
        // (Python's `item['BLOCK'] = None` when VS is Some, and BLOCK
        // was ALREADY None whenever VS is None — both dict entries are
        // read from the identical `row[7]`), so `{BLOCK=...}` never
        // actually renders for a Bible-shaped note.
        let vs = raw.block_identifier;
        let ch = raw.chapter_number.unwrap_or(0);
        let vs_str = vs.map(|v| format!("{v:03}")).unwrap_or_else(|| "000".to_string());
        let reference = format!("{bk:02}{ch:03}{vs_str}");

        let mut heading = raw.location_title.clone().unwrap_or_default();
        if heading.is_empty() {
            let book_name = catalog.bible_book(bk).unwrap_or("");
            heading = format!("{book_name} {ch}");
        } else if vs.is_some() && !heading.contains(':') {
            heading = format!("{heading}:{}", vs.unwrap_or_default());
        }

        let lang = raw.meps_language.unwrap_or(0);
        let pub_sym = raw.key_symbol.clone().unwrap_or_default();
        out.push_str(&format!("{{LANG={lang}}}{{PUB={pub_sym}}}{{BK={bk}}}{{CH={ch}}}"));
        if let Some(v) = vs {
            out.push_str(&format!("{{VS={v}}}"));
        }
        out.push_str(&format!("{{Reference={reference}}}"));
        if !heading.is_empty() {
            out.push_str(&format!("{{HEADING={heading}}}"));
        }
        out.push_str(&format!("{{COLOR={color}}}"));
        if let Some(r) = range {
            out.push_str(&format!("{{RANGE={r}}}"));
        }
        if doc_present.is_some() {
            out.push_str("{DOC=0}");
        }
    } else if let Some(doc) = doc_present {
        // Publication-shaped (`:1658-1666`). `BLOCK`/`HEADING` are NEVER
        // overridden here — they stay exactly as read off the row.
        let heading = raw.location_title.clone().unwrap_or_default();
        let issue = raw.issue_tag_number.filter(|n| *n > 10_000_000);
        let lang = raw.meps_language.unwrap_or(0);
        let pub_sym = raw.key_symbol.clone().unwrap_or_default();
        out.push_str(&format!("{{LANG={lang}}}{{PUB={pub_sym}}}"));
        if let Some(iss) = issue {
            out.push_str(&format!("{{ISSUE={iss}}}"));
        }
        out.push_str(&format!("{{DOC={doc}}}"));
        if let Some(blk) = raw.block_identifier {
            out.push_str(&format!("{{BLOCK={blk}}}"));
        }
        if !heading.is_empty() {
            out.push_str(&format!("{{HEADING={heading}}}"));
        }
        out.push_str(&format!("{{COLOR={color}}}"));
        if let Some(r) = range {
            out.push_str(&format!("{{RANGE={r}}}"));
        }
    }
    // else: independent-shaped — no further brackets (`:1636-1637`).

    out.push_str(&format!("===\n{title}\n{note}"));
    out
}

/// Reads every live Note (or the subset named by `ids`), paired with its
/// `NoteId` and its [`format_note_record`] wire text — the incremental
/// export diff's live side (09-01-PLAN.md Task 1). Reuses
/// [`read_raw_note_rows`]'s SAME SQL (extended once, above, to also select
/// `NoteId` — never a second column list) and [`read_note_range`], so this
/// and [`export_notes`] can never see a different row set.
pub(crate) fn read_note_id_records(
    conn: &Connection,
    ids: Option<&NonEmptyNoteIds>,
    catalog: &ResourceCatalog,
    now: &str,
) -> Result<Vec<(i64, String)>, ArchiveError> {
    let raw_rows = read_raw_note_rows(conn, ids)?;
    raw_rows
        .iter()
        .map(|raw| {
            let range = match raw.user_mark_id {
                Some(id) => read_note_range(conn, id)?,
                None => None,
            };
            Ok((raw.note_id, format_note_record(raw, range.as_deref(), catalog, now)))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn join_row_renders_null_as_literal_none() {
        let values = vec![Some("1".to_string()), None, Some("nwt".to_string())];
        assert_eq!(join_row(&values), "1|None|nwt");
    }

    #[test]
    fn join_row_empty_slice_is_empty_string() {
        let values: Vec<Option<String>> = Vec::new();
        assert_eq!(join_row(&values), "");
    }
}
