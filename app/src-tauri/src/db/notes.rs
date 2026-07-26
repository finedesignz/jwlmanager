//! Full located + independent-notes UNION query with resources.db label
//! synthesis. Analog: `JWLManager.py:694-767` (`get_notes`/`load_independent`),
//! `578-627` (`process_code`/`process_detail`/`process_color`).
//!
//! The `dupes` CTE branch (`JWLManager.py:707-750`) is out of scope for
//! Phase 1 (read-only) — only the base located query + independent-notes
//! UNION are needed for DATA-01.

use crate::db::labels::{process_code, process_color, process_detail, resolve_publication};
use crate::db::resources::ResourceCatalog;
use crate::error::ArchiveError;
use rusqlite::Connection;
use serde::Serialize;
use ts_rs::TS;

/// A single browse-list row over IPC to the frontend — the ONE unified row
/// type every category (Notes, Bookmarks, Annotations, Favorites, Highlights,
/// Playlists) collapses to (D6-02). Field shape mirrors the columns the Python
/// getters produce after `merge_df` (`Id`, `Language`, `Symbol`, `Color`,
/// `Tags`, `Modified`, `Year`, `Detail1`, `Detail2`, `Short`, `Full`, `Type`).
///
/// The nullable columns are the `merge_df` `fill_null` analog: a category that
/// does not produce a column leaves it `None` (Bookmarks/Annotations/Favorites
/// have no color/tags/modified; Playlists has no language). Notes populates
/// every column, so it wraps each value in `Some(...)` — byte-identical to the
/// pre-refactor `NotesRow`. `short`/`full`/`type_group` are always filled by
/// `merge_df` (even for Playlists), so they stay non-optional.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/BrowseRow.ts")]
pub struct BrowseRow {
    pub id: i64,
    /// UI language name, or `None` for a category without a language column
    /// (e.g. Playlists). Notes always sets `Some(...)`.
    pub language: Option<String>,
    /// Processed publication code/symbol (`process_code` output), or
    /// `"* OTHER *"` when empty — never the raw `KeySymbol`.
    pub symbol: String,
    /// English color name (`process_color`), or `None` for a category with no
    /// color column (e.g. Bookmarks/Favorites). i18n is out of scope for
    /// Phase 1 (UI-SPEC defers locale switching to Phase 11).
    pub color: Option<String>,
    /// Concatenated tag names, or `None` for a category that has no tags.
    pub tags: Option<String>,
    /// Last-modified date, or `None` for a category without a modified column.
    pub modified: Option<String>,
    pub year: Option<String>,
    pub detail1: Option<String>,
    pub detail2: Option<String>,
    pub short: String,
    pub full: String,
    pub type_group: String,
    /// True for independent notes (`LocationId IS NULL`) — surfaced so the
    /// frontend can render the `* INDEPENDENT *` affordance without
    /// re-deriving it from `type_group`. Only Notes sets this true.
    pub independent: bool,
    /// The Annotation's own `InputField.TextTag` (07-05-PLAN.md, EDIT-07).
    /// `Some` ONLY for Annotations rows — `id` alone is `LocationId`, which
    /// is NOT unique across Annotation rows (one Location can carry several
    /// `InputField`s, one per `TextTag`); this field disambiguates them so
    /// the record editor can key its `(LocationId, TextTag)` single-record
    /// edit/delete (D7-09/rule #10) precisely. `None` for every other
    /// category.
    pub text_tag: Option<String>,
}

/// Base located-Note query — `JWLManager.py:751-757`'s main SQL shape (no
/// `dupes` CTE, no `WHERE` filter).
const LOCATED_NOTES_SQL: &str = "SELECT NoteId Id, MepsLanguage Language, KeySymbol Symbol, \
    IssueTagNumber Issue, BookNumber Book, ChapterNumber Chapter, ColorIndex Color, \
    GROUP_CONCAT(Name, ' | ') Tags, substr(LastModified, 0, 11) Modified \
    FROM (SELECT * FROM Note n JOIN Location l USING (LocationId) \
        LEFT JOIN TagMap tm USING (NoteId) \
        LEFT JOIN Tag t USING (TagId) \
        LEFT JOIN UserMark u USING (UserMarkId) \
        ORDER BY t.Name) n \
    GROUP BY n.NoteId";

/// Independent-notes query — `JWLManager.py:696-704`'s `load_independent`.
/// MUST be UNIONed with the located query above: dropping this silently
/// loses the user's standalone notes (Core Value: never lose user data).
const INDEPENDENT_NOTES_SQL: &str = "SELECT NoteId Id, ColorIndex Color, \
    GROUP_CONCAT(Name, ' | ') Tags, substr(LastModified, 0, 11) Modified \
    FROM (SELECT * FROM Note n LEFT JOIN TagMap tm USING (NoteId) \
        LEFT JOIN Tag t USING (TagId) \
        LEFT JOIN UserMark u USING (UserMarkId) \
        ORDER BY t.Name) n \
    WHERE n.BlockType = 0 AND LocationId IS NULL \
    GROUP BY n.NoteId";

struct LocatedRawRow {
    id: i64,
    language: i64,
    symbol: Option<String>,
    issue: i64,
    book: Option<i64>,
    chapter: Option<i64>,
    color: i64,
    tags: Option<String>,
    modified: Option<String>,
}

/// Queries the located-Note rows and synthesizes their display labels via
/// `catalog`.
fn query_located_notes(
    conn: &Connection,
    catalog: &ResourceCatalog,
) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut stmt = conn.prepare(LOCATED_NOTES_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(LocatedRawRow {
            id: row.get(0)?,
            language: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            symbol: row.get(2)?,
            issue: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            book: row.get(4)?,
            chapter: row.get(5)?,
            color: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            tags: row.get(7)?,
            modified: row.get(8)?,
        })
    })?;

    let mut rows = Vec::new();
    for raw in mapped {
        let raw = raw?;
        let language = catalog
            .lang_name(raw.language)
            .map(str::to_string)
            .unwrap_or_else(|| format!("#{}", raw.language));
        let (code, year) = process_code(raw.symbol.as_deref(), raw.issue);
        let symbol = if code.is_empty() {
            "* OTHER *".to_string()
        } else {
            code
        };
        let (detail1, year, detail2) =
            process_detail(&symbol, raw.book, raw.chapter, raw.issue, year, catalog);
        let (short, full, type_group, year) = resolve_publication(catalog, &symbol, year);
        let year = year.or(Some("* NO YEAR *".to_string()));

        // Notes populates every column, so each optional field is `Some(...)`
        // — byte-identical to the pre-BrowseRow `NotesRow` values (D6-02).
        rows.push(BrowseRow {
            id: raw.id,
            language: Some(language),
            symbol,
            color: Some(process_color(raw.color)),
            tags: Some(raw.tags.unwrap_or_else(|| "* NO TAG *".to_string())),
            modified: Some(raw.modified.unwrap_or_default()),
            year,
            detail1,
            detail2,
            short,
            full,
            type_group,
            independent: false,
            text_tag: None,
        });
    }
    Ok(rows)
}

struct IndependentRawRow {
    id: i64,
    color: i64,
    tags: Option<String>,
    modified: Option<String>,
}

/// Queries the independent-Note rows (`LocationId IS NULL`). MUST run and be
/// concatenated with `query_located_notes`'s result — see module docs and
/// `INDEPENDENT_NOTES_SQL`.
fn query_independent_notes(conn: &Connection) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut stmt = conn.prepare(INDEPENDENT_NOTES_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(IndependentRawRow {
            id: row.get(0)?,
            color: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            tags: row.get(2)?,
            modified: row.get(3)?,
        })
    })?;

    let mut rows = Vec::new();
    for raw in mapped {
        let raw = raw?;
        let modified = raw.modified.unwrap_or_default();
        let year = if modified.len() >= 4 {
            Some(modified[0..4].to_string())
        } else {
            None
        };
        rows.push(BrowseRow {
            id: raw.id,
            language: Some("* NO LANGUAGE *".to_string()),
            symbol: "* OTHER *".to_string(),
            color: Some(process_color(raw.color)),
            tags: Some(raw.tags.unwrap_or_else(|| "* NO TAG *".to_string())),
            modified: Some(modified),
            year,
            detail1: None,
            detail2: None,
            short: "* OTHER *".to_string(),
            full: "* OTHER *".to_string(),
            type_group: "* INDEPENDENT *".to_string(),
            independent: true,
            text_tag: None,
        });
    }
    Ok(rows)
}

/// Queries ALL Notes rows (located + independent) from an already-open
/// `userData.db` connection, in the Python's `pl.concat([i_notes, notes])`
/// order (independent first, then located).
pub fn query_notes(
    conn: &Connection,
    catalog: &ResourceCatalog,
) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut rows = query_independent_notes(conn)?;
    rows.extend(query_located_notes(conn, catalog)?);
    Ok(rows)
}
