//! The five not-yet-built category browse queries (Bookmarks, Favorites,
//! Highlights, Annotations, Playlists), each returning the unified [`BrowseRow`]
//! from 06-01. Analogs: the Python `get_*` getters at `JWLManager.py:641`
//! (annotations), `:654` (bookmarks), `:667` (favorites), `:680` (highlights),
//! `:768` (playlists) — the Notes getter (`:694`) already lives in `notes.rs`.
//!
//! Each SQL below is a static `const &str` ported STRUCTURALLY-VERBATIM from the
//! cited Python line. None of these five take a runtime parameter, so there is
//! nothing to interpolate — the CLAUDE.md "no f-string/format-string SQL" rule
//! is satisfied trivially (the only bound value anywhere in the browse path is
//! resources.db's `ui_lang_id`, already parameterized in `resources.rs`).
//!
//! Label synthesis reuses the shared `labels.rs` helpers (D6-01): the four
//! located categories (Annotations/Bookmarks/Favorites/Highlights) resolve
//! publication names/refs via `ResourceCatalog`; Playlists (D6-04) needs NO
//! resources.db lookup (its label comes from `Tag.Name`/`PlaylistItem.Label`).
//! Only names/refs metadata are read — NEVER publication body text (T-06-05,
//! project constraint). No `.unwrap()`/`.expect()` on the archive-data path
//! (crate deny gate): every column read defaults via `unwrap_or`, every step
//! propagates with `?`.

use crate::db::labels::{process_code, process_color, process_detail, resolve_publication};
use crate::db::notes::BrowseRow;
use crate::db::resources::ResourceCatalog;
use crate::error::ArchiveError;
use rusqlite::Connection;

/// `get_annotations` — `JWLManager.py:643`. Identity = `LocationId`.
const ANNOTATIONS_SQL: &str = "SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, \
    TextTag, l.BookNumber, l.ChapterNumber, l.Title \
    FROM InputField JOIN Location l USING (LocationId)";

/// `get_bookmarks` — `JWLManager.py:656`. Identity = `BookmarkId` (col 5),
/// NOT the first-SELECTed `LocationId` — the load-bearing pitfall for Phase 7.
const BOOKMARKS_SQL: &str = "SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, \
    BookmarkId, l.BookNumber, l.ChapterNumber, l.Title \
    FROM Bookmark b JOIN Location l USING (LocationId)";

/// `get_favorites` — `JWLManager.py:669`. Identity = `TagMapId`. The
/// `WHERE tm.NoteId IS NULL ORDER BY tm.Position` is load-bearing: a Favorite
/// is a TagMap row with a NULL NoteId; dropping the predicate lists note-tag
/// mappings as favorites.
const FAVORITES_SQL: &str = "SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, \
    TagMapId \
    FROM TagMap tm JOIN Location l USING (LocationId) WHERE tm.NoteId IS NULL ORDER BY tm.Position";

/// `get_highlights` — `JWLManager.py:682`. Identity = `BlockRangeId`. ONE row
/// per BlockRange (no GROUP BY) — a multi-block highlight is intentionally
/// multiple selectable rows. The only new category carrying a color.
const HIGHLIGHTS_SQL: &str = "SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, \
    b.BlockRangeId, u.UserMarkId, u.ColorIndex, l.BookNumber, l.ChapterNumber \
    FROM UserMark u JOIN Location l USING (LocationId), BlockRange b USING (UserMarkId)";

/// `get_playlists` — `JWLManager.py:770`. Identity = `PlaylistItemId`. Needs NO
/// resources.db lookup (D6-04): label is `Tag.Name`/`PlaylistItem.Label`.
const PLAYLISTS_SQL: &str = "SELECT PlaylistItemId, Name, Position, Label \
    FROM PlaylistItem JOIN TagMap USING (PlaylistItemId) JOIN Tag t USING (TagId) \
    WHERE t.Type = 2 ORDER BY Name, Position";

/// The publication-label bundle shared by the four located categories
/// (Annotations/Bookmarks/Favorites/Highlights) — the `process_code` +
/// `process_detail` + `resolve_publication` pipeline `notes.rs` runs, factored
/// out so no category re-implements it.
struct PubLabel {
    symbol: String,
    year: Option<String>,
    detail1: Option<String>,
    detail2: Option<String>,
    short: String,
    full: String,
    type_group: String,
}

/// Runs the shared located-category label pipeline, mirroring
/// `query_located_notes` in `notes.rs`. `other_on_empty` applies the
/// `"* OTHER *"`-when-empty symbol rule (Annotations/Bookmarks/Favorites, per
/// plan + Python `code or _('* OTHER *')`); Highlights ports the bare `code`
/// (`JWLManager.py:688`) and passes `false`.
fn synthesize_pub_label(
    catalog: &ResourceCatalog,
    key_symbol: Option<&str>,
    issue: i64,
    book: Option<i64>,
    chapter: Option<i64>,
    other_on_empty: bool,
) -> PubLabel {
    let (code, year) = process_code(key_symbol, issue);
    let symbol = if code.is_empty() {
        if other_on_empty {
            "* OTHER *".to_string()
        } else {
            String::new()
        }
    } else {
        code
    };
    let (detail1, year, detail2) = process_detail(&symbol, book, chapter, issue, year, catalog);
    let (short, full, type_group, year) = resolve_publication(catalog, &symbol, year);
    // merge_df's `Year` fill: publication year already folded in by
    // `resolve_publication`; final fallback to the `* NO YEAR *` sentinel.
    let year = year.or_else(|| Some("* NO YEAR *".to_string()));
    PubLabel {
        symbol,
        year,
        detail1,
        detail2,
        short,
        full,
        type_group,
    }
}

/// Resolves a MepsLanguage id to its UI name. `no_language_fallback` selects
/// the per-category miss sentinel: Annotations uses `"* NO LANGUAGE *"`
/// (`lang_name.get(row[2], _('* NO LANGUAGE *'))`); Bookmarks/Favorites/
/// Highlights use `f'#{row[2]}'` (`lang_name.get(row[2], f'#{row[2]}')`).
fn resolve_language(
    catalog: &ResourceCatalog,
    meps_language: i64,
    no_language_fallback: bool,
) -> String {
    catalog
        .lang_name(meps_language)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if no_language_fallback {
                "* NO LANGUAGE *".to_string()
            } else {
                format!("#{meps_language}")
            }
        })
}

struct AnnotationRaw {
    location_id: i64,
    key_symbol: Option<String>,
    meps_language: i64,
    issue: i64,
    book: Option<i64>,
    chapter: Option<i64>,
}

/// Ports `get_annotations` (`JWLManager.py:641-652`). Identity = `LocationId`.
pub fn query_annotations(
    conn: &Connection,
    catalog: &ResourceCatalog,
) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut stmt = conn.prepare(ANNOTATIONS_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(AnnotationRaw {
            location_id: row.get(0)?,
            key_symbol: row.get(1)?,
            meps_language: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            issue: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            // col 4 = TextTag: selected verbatim but unused in label synthesis.
            book: row.get(5)?,
            chapter: row.get(6)?,
            // col 7 = Title: selected verbatim but unused.
        })
    })?;

    let mut rows = Vec::new();
    for raw in mapped {
        let raw = raw?;
        let language = resolve_language(catalog, raw.meps_language, true);
        let label = synthesize_pub_label(
            catalog,
            raw.key_symbol.as_deref(),
            raw.issue,
            raw.book,
            raw.chapter,
            true,
        );
        rows.push(BrowseRow {
            id: raw.location_id,
            language: Some(language),
            symbol: label.symbol,
            color: None,
            tags: None,
            modified: None,
            year: label.year,
            detail1: label.detail1,
            detail2: label.detail2,
            short: label.short,
            full: label.full,
            type_group: label.type_group,
            independent: false,
        });
    }
    Ok(rows)
}

struct BookmarkRaw {
    key_symbol: Option<String>,
    meps_language: i64,
    issue: i64,
    bookmark_id: i64,
    book: Option<i64>,
    chapter: Option<i64>,
}

/// Ports `get_bookmarks` (`JWLManager.py:654-665`). Identity = `BookmarkId`
/// (NOT the first-SELECTed `LocationId`).
pub fn query_bookmarks(
    conn: &Connection,
    catalog: &ResourceCatalog,
) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut stmt = conn.prepare(BOOKMARKS_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(BookmarkRaw {
            // col 0 = LocationId (the join key) is NOT the identity.
            key_symbol: row.get(1)?,
            meps_language: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            issue: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            bookmark_id: row.get(4)?,
            book: row.get(5)?,
            chapter: row.get(6)?,
            // col 7 = Title: selected verbatim but unused.
        })
    })?;

    let mut rows = Vec::new();
    for raw in mapped {
        let raw = raw?;
        let language = resolve_language(catalog, raw.meps_language, false);
        let label = synthesize_pub_label(
            catalog,
            raw.key_symbol.as_deref(),
            raw.issue,
            raw.book,
            raw.chapter,
            true,
        );
        rows.push(BrowseRow {
            id: raw.bookmark_id,
            language: Some(language),
            symbol: label.symbol,
            color: None,
            tags: None,
            modified: None,
            year: label.year,
            detail1: label.detail1,
            detail2: label.detail2,
            short: label.short,
            full: label.full,
            type_group: label.type_group,
            independent: false,
        });
    }
    Ok(rows)
}

struct FavoriteRaw {
    key_symbol: Option<String>,
    meps_language: i64,
    issue: i64,
    tag_map_id: i64,
}

/// Ports `get_favorites` (`JWLManager.py:667-678`). Identity = `TagMapId`.
/// `process_detail` runs with `book=None, chapter=None` (the SQL selects no
/// Book/Chapter). The `WHERE tm.NoteId IS NULL` predicate lives in the SQL.
pub fn query_favorites(
    conn: &Connection,
    catalog: &ResourceCatalog,
) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut stmt = conn.prepare(FAVORITES_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(FavoriteRaw {
            // col 0 = LocationId (join key) is NOT the identity.
            key_symbol: row.get(1)?,
            meps_language: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            issue: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            tag_map_id: row.get(4)?,
        })
    })?;

    let mut rows = Vec::new();
    for raw in mapped {
        let raw = raw?;
        let language = resolve_language(catalog, raw.meps_language, false);
        let label = synthesize_pub_label(
            catalog,
            raw.key_symbol.as_deref(),
            raw.issue,
            None,
            None,
            true,
        );
        rows.push(BrowseRow {
            id: raw.tag_map_id,
            language: Some(language),
            symbol: label.symbol,
            color: None,
            tags: None,
            modified: None,
            year: label.year,
            detail1: label.detail1,
            detail2: label.detail2,
            short: label.short,
            full: label.full,
            type_group: label.type_group,
            independent: false,
        });
    }
    Ok(rows)
}

struct HighlightRaw {
    key_symbol: Option<String>,
    meps_language: i64,
    issue: i64,
    block_range_id: i64,
    color_index: i64,
    book: Option<i64>,
    chapter: Option<i64>,
}

/// Ports `get_highlights` (`JWLManager.py:680-692`). Identity = `BlockRangeId`,
/// ONE row per BlockRange (no GROUP BY). Carries a color (`process_color`).
pub fn query_highlights(
    conn: &Connection,
    catalog: &ResourceCatalog,
) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut stmt = conn.prepare(HIGHLIGHTS_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(HighlightRaw {
            // col 0 = LocationId (join key) is NOT the identity.
            key_symbol: row.get(1)?,
            meps_language: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            issue: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            block_range_id: row.get(4)?,
            // col 5 = UserMarkId: selected verbatim but not an identity here.
            color_index: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            book: row.get(7)?,
            chapter: row.get(8)?,
        })
    })?;

    let mut rows = Vec::new();
    for raw in mapped {
        let raw = raw?;
        let language = resolve_language(catalog, raw.meps_language, false);
        // Highlights ports the bare `code` (`JWLManager.py:688`) — no
        // `* OTHER *`-on-empty fallback (other_on_empty = false).
        let label = synthesize_pub_label(
            catalog,
            raw.key_symbol.as_deref(),
            raw.issue,
            raw.book,
            raw.chapter,
            false,
        );
        rows.push(BrowseRow {
            id: raw.block_range_id,
            language: Some(language),
            symbol: label.symbol,
            color: Some(process_color(raw.color_index)),
            tags: None,
            modified: None,
            year: label.year,
            detail1: label.detail1,
            detail2: label.detail2,
            short: label.short,
            full: label.full,
            type_group: label.type_group,
            independent: false,
        });
    }
    Ok(rows)
}

struct PlaylistRaw {
    playlist_item_id: i64,
    name: Option<String>,
    label: Option<String>,
}

/// Ports `get_playlists` (`JWLManager.py:768-775`). Identity = `PlaylistItemId`.
/// Needs NO resources.db lookup (D6-04): `_catalog` is accepted for a uniform
/// getter signature but is intentionally unused. `Tags` = `Tag.Name`,
/// `Detail1` = `PlaylistItem.Label`; symbol/short/full = `"* OTHER *"`,
/// type_group = `"Other"`, language = `None`, year = `""` (per Python).
pub fn query_playlists(
    conn: &Connection,
    _catalog: &ResourceCatalog,
) -> Result<Vec<BrowseRow>, ArchiveError> {
    let mut stmt = conn.prepare(PLAYLISTS_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(PlaylistRaw {
            playlist_item_id: row.get(0)?,
            name: row.get(1)?,
            // col 2 = Position: selected verbatim for ORDER BY, unused here.
            label: row.get(3)?,
        })
    })?;

    let mut rows = Vec::new();
    for raw in mapped {
        let raw = raw?;
        rows.push(BrowseRow {
            id: raw.playlist_item_id,
            language: None,
            symbol: "* OTHER *".to_string(),
            color: None,
            tags: raw.name,
            modified: None,
            year: Some(String::new()),
            detail1: raw.label,
            detail2: None,
            short: "* OTHER *".to_string(),
            full: "* OTHER *".to_string(),
            type_group: "Other".to_string(),
            independent: false,
        });
    }
    Ok(rows)
}
