//! Full located + independent-notes UNION query with resources.db label
//! synthesis. Analog: `JWLManager.py:694-767` (`get_notes`/`load_independent`),
//! `578-627` (`process_code`/`process_detail`/`process_color`).
//!
//! The `dupes` CTE branch (`JWLManager.py:707-750`) is out of scope for
//! Phase 1 (read-only) — only the base located query + independent-notes
//! UNION are needed for DATA-01.

use crate::db::resources::ResourceCatalog;
use crate::error::ArchiveError;
use regex::Regex;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::LazyLock;
use ts_rs::TS;

/// A single Notes-list row, over IPC to the frontend. Field shape mirrors
/// the columns `get_notes`/`load_independent` produce after `merge_df`
/// (`Id`, `Language`, `Symbol`, `Color`, `Tags`, `Modified`, `Year`,
/// `Detail1`, `Detail2`, `Short`, `Full`, `Type`).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/NotesRow.ts")]
pub struct NotesRow {
    pub id: i64,
    pub language: String,
    /// Processed publication code/symbol (`process_code` output), or
    /// `"* OTHER *"` when empty — never the raw `KeySymbol`.
    pub symbol: String,
    /// English color name (`process_color`); i18n is out of scope for
    /// Phase 1 (UI-SPEC defers locale switching to Phase 11).
    pub color: String,
    pub tags: String,
    pub modified: String,
    pub year: Option<String>,
    pub detail1: Option<String>,
    pub detail2: Option<String>,
    pub short: String,
    pub full: String,
    pub type_group: String,
    /// True for independent notes (`LocationId IS NULL`) — surfaced so the
    /// frontend can render the `* INDEPENDENT *` affordance without
    /// re-deriving it from `type_group`.
    pub independent: bool,
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

/// `code_yr = regex.compile(r'(.*?[^\d-])(\d{2}$)')` (`JWLManager.py:930`).
/// Both patterns below are fixed, compile-time-known-valid literals — a
/// compile failure here would be a programmer error caught by the
/// `regex_patterns_compile` test, not a runtime archive-data-path panic
/// (D-15 targets untrusted archive input, not this constant).
static CODE_YR: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"^(.*?[^\d-])(\d{2})$").expect("CODE_YR regex must compile")
});
/// `code_jwb = regex.compile(r'jwb-\d+$')` (`JWLManager.py:931`), matched
/// with `regex.match` (anchored at the start) in the original.
static CODE_JWB: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"^jwb-\d+$").expect("CODE_JWB regex must compile")
});

const DATED_PREFIX_EXCLUDED: [&str; 7] = ["bi", "br", "brg", "kn", "ks", "pt", "tp"];
const BIBLE_APPENDIX_SYMBOLS: [&str; 12] = [
    "Rbi8", "bi10", "bi12", "bi22", "bi7", "by", "int", "nwt", "nwtsty", "rh", "sbi1", "sbi2",
];
const COLOR_NAMES: [&str; 7] = ["Grey", "Yellow", "Green", "Blue", "Red", "Orange", "Purple"];

/// Ports `process_code` (`JWLManager.py:578-596`). Returns the processed
/// code (never the raw `KeySymbol`) and an optional embedded year.
fn process_code(symbol: Option<&str>, issue: i64) -> (String, Option<String>) {
    let mut code = match symbol {
        Some("ws") if issue == 0 => "ws-".to_string(),
        Some("") | None => String::new(),
        Some(s) if CODE_JWB.is_match(s) => "jwb-".to_string(),
        Some(s) => s.to_string(),
    };

    let mut year = None;
    if let Some(caps) = CODE_YR.captures(&code) {
        let prefix = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let suffix = caps
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if !DATED_PREFIX_EXCLUDED.contains(&prefix.as_str()) {
            if let Ok(n) = suffix.parse::<i64>() {
                year = Some(if n >= 50 {
                    format!("19{suffix}")
                } else {
                    format!("20{suffix}")
                });
            }
            code = prefix;
        }
    }
    (code, year)
}

/// Ports `process_color` (`JWLManager.py:598-599`).
fn process_color(color_index: i64) -> String {
    COLOR_NAMES
        .get(usize::try_from(color_index.max(0)).unwrap_or(0))
        .unwrap_or(&"Grey")
        .to_string()
}

/// Ports `process_detail` (`JWLManager.py:601-627`).
fn process_detail(
    symbol: &str,
    book: Option<i64>,
    chapter: Option<i64>,
    issue: i64,
    year: Option<String>,
    catalog: &ResourceCatalog,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut detail1 = if BIBLE_APPENDIX_SYMBOLS.contains(&symbol) {
        Some("* OTHER *".to_string())
    } else {
        None
    };
    let mut year = year;

    if issue > 19_000_000 {
        let iss = issue.to_string();
        if iss.len() >= 8 {
            let y = &iss[0..4];
            let m = &iss[4..6];
            let d = &iss[6..8];
            detail1 = Some(if d == "00" {
                format!("{y}-{m}")
            } else {
                format!("{y}-{m}-{d}")
            });
            if year.is_none() {
                year = Some(y.to_string());
            }
        }
    }

    let mut detail2 = None;
    if let (Some(book), Some(chapter)) = (book, chapter) {
        let book_name = catalog.bible_book(book).unwrap_or("?");
        detail1 = Some(format!("{book:0>2}: {book_name}"));
        detail2 = Some(format!("Chap.{chapter:>4}"));
    }

    if detail1.is_none() {
        if let Some(y) = &year {
            detail1 = Some(y.clone());
        }
    }
    (detail1, year, detail2)
}

/// Publication/extra lookup for a processed `code`, mirroring `merge_df`'s
/// `Symbol`-keyed join (`JWLManager.py:629-639`): fills `Short`/`Full` with
/// the symbol itself when unmatched, `Type` with `"Other"`, and `Year` with
/// the publication's own year (falling back to `"* NO YEAR *"`).
fn resolve_publication(
    catalog: &ResourceCatalog,
    symbol: &str,
    year: Option<String>,
) -> (String, String, String, Option<String>) {
    match catalog.publication(symbol) {
        Some(info) => {
            let short = if info.short.is_empty() {
                symbol.to_string()
            } else {
                info.short.clone()
            };
            let full = if info.full.is_empty() {
                symbol.to_string()
            } else {
                info.full.clone()
            };
            let type_group = info
                .type_group
                .clone()
                .unwrap_or_else(|| "Other".to_string());
            let year = year.or_else(|| info.year.map(|y| y.to_string()));
            (short, full, type_group, year)
        }
        None => (
            symbol.to_string(),
            symbol.to_string(),
            "Other".to_string(),
            year,
        ),
    }
}

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
) -> Result<Vec<NotesRow>, ArchiveError> {
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

        rows.push(NotesRow {
            id: raw.id,
            language,
            symbol,
            color: process_color(raw.color),
            tags: raw.tags.unwrap_or_else(|| "* NO TAG *".to_string()),
            modified: raw.modified.unwrap_or_default(),
            year,
            detail1,
            detail2,
            short,
            full,
            type_group,
            independent: false,
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
fn query_independent_notes(conn: &Connection) -> Result<Vec<NotesRow>, ArchiveError> {
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
        rows.push(NotesRow {
            id: raw.id,
            language: "* NO LANGUAGE *".to_string(),
            symbol: "* OTHER *".to_string(),
            color: process_color(raw.color),
            tags: raw.tags.unwrap_or_else(|| "* NO TAG *".to_string()),
            modified,
            year,
            detail1: None,
            detail2: None,
            short: "* OTHER *".to_string(),
            full: "* OTHER *".to_string(),
            type_group: "* INDEPENDENT *".to_string(),
            independent: true,
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
) -> Result<Vec<NotesRow>, ArchiveError> {
    let mut rows = query_independent_notes(conn)?;
    rows.extend(query_located_notes(conn, catalog)?);
    Ok(rows)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::db::resources::dev_resources_db_path;

    fn catalog() -> ResourceCatalog {
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load")
    }

    #[test]
    fn process_code_ws_zero_issue() {
        let (code, year) = process_code(Some("ws"), 0);
        assert_eq!(code, "ws-");
        assert_eq!(year, None);
    }

    #[test]
    fn process_code_jwb_prefixed() {
        let (code, _year) = process_code(Some("jwb-123"), 5);
        assert_eq!(code, "jwb-");
    }

    #[test]
    fn process_code_dated_symbol_19xx() {
        // "w" + "95" -> prefix "w", suffix "95" -> year 1995 (suffix >= 50).
        let (code, year) = process_code(Some("w95"), 5);
        assert_eq!(code, "w");
        assert_eq!(year.as_deref(), Some("1995"));
    }

    #[test]
    fn process_code_dated_symbol_20xx() {
        // "w" + "15" -> suffix < 50 -> year 2015.
        let (code, year) = process_code(Some("w15"), 5);
        assert_eq!(code, "w");
        assert_eq!(year.as_deref(), Some("2015"));
    }

    #[test]
    fn process_code_excluded_prefix_not_dated() {
        // "bi" is in the excluded-prefix set: no year strip even though the
        // trailing-2-digit shape matches.
        let (code, year) = process_code(Some("bi12"), 5);
        assert_eq!(code, "bi12");
        assert_eq!(year, None);
    }

    #[test]
    fn process_detail_bible_reference() {
        let cat = catalog();
        let (detail1, year, detail2) = process_detail("nwt", Some(1), Some(3), 0, None, &cat);
        assert_eq!(detail1.as_deref(), Some("01: Genesis"));
        assert_eq!(detail2.as_deref(), Some("Chap.   3"));
        assert_eq!(year, None);
    }

    #[test]
    fn process_detail_publication_issue() {
        let cat = catalog();
        let (detail1, year, detail2) = process_detail("w", None, None, 20230115, None, &cat);
        assert_eq!(detail1.as_deref(), Some("2023-01-15"));
        assert_eq!(year.as_deref(), Some("2023"));
        assert_eq!(detail2, None);
    }

    #[test]
    fn process_detail_bible_appendix_symbol_falls_back_to_other() {
        let cat = catalog();
        let (detail1, _year, _detail2) = process_detail("nwt", None, None, 0, None, &cat);
        assert_eq!(detail1.as_deref(), Some("* OTHER *"));
    }
}
