//! Bundled `res/resources.db` label lookups (Languages / BibleBooks /
//! Publications+Extras). Analog: `JWLManager.py:4023-4053` (`read_resources`).
//!
//! All SQL uses `rusqlite` bound parameters — never string interpolation.
//! The Python original interpolates `ui_lang` (an internal integer) directly
//! into the SQL text (`f"...WHERE Language = {ui_lang};"`); that is the
//! anti-pattern this port fixes (CLAUDE.md: no f-string/format-string SQL).

use crate::error::ArchiveError;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// A single publication/extra lookup row (`Publications`/`Extras` JOIN
/// `Types`), keyed by `Symbol` in `ResourceCatalog::publications`.
#[derive(Debug, Clone)]
pub struct PublicationInfo {
    pub short: String,
    pub full: String,
    pub year: Option<i64>,
    pub type_group: Option<String>,
}

/// One row of the bundled `Favorites` VIEW (columns `Language, Symbol,
/// Short, Lang` — 07-RESEARCH.md Corrections item 1: there is no
/// `favorites` table, only this VIEW over `Publications`/`Extras`). A
/// Bible-edition catalog entry the Favorite Dialog's edition picker renders
/// (07-01-PLAN.md Task 2/3, EDIT-05 mark).
///
/// `language` is the integer `MepsLanguage` id (goes into
/// `Location.MepsLanguage` when marking) and `symbol` is the edition's
/// `KeySymbol` (goes into `Location.KeySymbol` and the `TagMap` duplicate
/// check) — both are what [`crate::db::favorites::apply_favorite_add`]
/// actually needs. `short` and `lang` are DISPLAY-ONLY strings (`short` =
/// edition short title shown in the edition list; `lang` = the display
/// language name, e.g. `"English"` — what [`ResourceCatalog::load_favorite_editions`]
/// filters by, NOT the integer `language` field despite the similar name).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/FavoriteEdition.ts")]
pub struct FavoriteEdition {
    pub language: i64,
    pub symbol: String,
    pub short: String,
    pub lang: String,
}

/// Cached label-lookup maps loaded once from the bundled `resources.db`,
/// mirroring `read_resources`'s module-global cache (`lang_name`,
/// `bible_books`, `publications`) via a struct instead of Python globals.
#[derive(Debug, Clone)]
pub struct ResourceCatalog {
    lang_name: HashMap<i64, String>,
    bible_books: HashMap<i64, String>,
    publications: HashMap<String, PublicationInfo>,
    /// [`FavoriteEdition`] rows grouped by their DISPLAY language name
    /// (`Favorites.Lang`), built eagerly here rather than queried on demand
    /// per `load_favorite_editions` call — `ResourceCatalog` holds no live
    /// connection after `load` returns (same reason `lang_name`/
    /// `bible_books`/`publications` are eager maps too).
    favorite_editions: HashMap<String, Vec<FavoriteEdition>>,
}

impl ResourceCatalog {
    /// Loads Languages / BibleBooks (for `ui_lang_code`) / Publications+Extras
    /// (for `ui_lang_code`) from the bundled resources.db at `resources_db_path`.
    /// `ui_lang_code` is matched against `Languages.Code` (e.g. `"en"`) —
    /// Phase 1 has no locale switcher (UI-SPEC defers that to Phase 11), so
    /// the caller always passes the fixed UI language for now.
    pub fn load(resources_db_path: &Path, ui_lang_code: &str) -> Result<Self, ArchiveError> {
        let conn = Connection::open(resources_db_path)?;

        let mut lang_name = HashMap::new();
        let mut ui_lang_id: Option<i64> = None;
        {
            let mut stmt = conn.prepare("SELECT Language, Name, Code FROM Languages")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (id, name, code) = row?;
                if code == ui_lang_code {
                    ui_lang_id = Some(id);
                }
                lang_name.insert(id, name);
            }
        }
        let ui_lang_id = ui_lang_id.ok_or(ArchiveError::MissingResourcesLanguage)?;

        let mut bible_books = HashMap::new();
        {
            let mut stmt =
                conn.prepare("SELECT Number, Name FROM BibleBooks WHERE Language = ?1")?;
            let rows = stmt.query_map([ui_lang_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (number, name) = row?;
                bible_books.insert(number, name);
            }
        }

        let mut publications = HashMap::new();
        for table in ["Publications", "Extras"] {
            // `table` is one of two fixed internal literals above, never
            // interpolated user/archive-derived data; the WHERE value is
            // still bound via ?1, not interpolated.
            let sql = format!(
                "SELECT p.Symbol, p.ShortTitle, p.Title, p.Year, t.[Group] \
                 FROM {table} p JOIN Types t USING (Type, Language) WHERE p.Language = ?1"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([ui_lang_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?;
            for row in rows {
                let (symbol, short, full, year, type_group) = row?;
                publications.entry(symbol).or_insert(PublicationInfo {
                    short: short.unwrap_or_default(),
                    full: full.unwrap_or_default(),
                    year,
                    type_group,
                });
            }
        }

        let mut favorite_editions: HashMap<String, Vec<FavoriteEdition>> = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT Language, Symbol, Short, Lang FROM Favorites")?;
            let rows = stmt.query_map([], |row| {
                Ok(FavoriteEdition {
                    language: row.get(0)?,
                    symbol: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    short: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    lang: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                })
            })?;
            for row in rows {
                let row = row?;
                favorite_editions
                    .entry(row.lang.clone())
                    .or_default()
                    .push(row);
            }
        }

        Ok(Self {
            lang_name,
            bible_books,
            publications,
            favorite_editions,
        })
    }

    pub fn lang_name(&self, meps_language: i64) -> Option<&str> {
        self.lang_name.get(&meps_language).map(String::as_str)
    }

    pub fn bible_book(&self, number: i64) -> Option<&str> {
        self.bible_books.get(&number).map(String::as_str)
    }

    pub fn publication(&self, symbol: &str) -> Option<&PublicationInfo> {
        self.publications.get(symbol)
    }

    /// Bible editions available to mark as a Favorite for a DISPLAY
    /// `language` name (`Favorites.Lang`, e.g. `"English"` — NOT the
    /// integer `MepsLanguage` id). Empty (never an error) when the language
    /// has no favorite-eligible editions — most of [`Self::all_language_names`]'s
    /// entries fall in this bucket (07-01-PLAN.md Task 2 behavior; the
    /// bundled catalog has ~1400 known languages but favorite-eligible
    /// editions in only a handful of them).
    pub fn load_favorite_editions(&self, language: &str) -> Vec<FavoriteEdition> {
        self.favorite_editions
            .get(language)
            .cloned()
            .unwrap_or_default()
    }

    /// Every DISPLAY language name the Favorite Dialog's Language `<select>`
    /// can offer, sorted — the full bundled `Languages` catalog (~1400
    /// entries), NOT narrowed to languages that currently have a
    /// favorite-eligible edition (only 9 of them do). Deliberate deviation
    /// from `JWLManager.py:3406-3410`, whose combo box is populated from
    /// `favorites['Lang'].unique()` and so is ALWAYS narrowed to
    /// non-empty choices: 07-UI-SPEC.md's "No editions found for
    /// {Language}. Try a different language." empty state is listed as
    /// reachable (`covered`, not `backstop`), which is only possible if
    /// the language list includes languages with zero favorite-eligible
    /// editions — a narrowed-to-Python's-set list could never produce that
    /// state at all.
    pub fn all_language_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.lang_name.values().map(String::as_str).collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

/// Repo-root `res/` dir when running from source (dev / `cargo test`).
/// `CARGO_MANIFEST_DIR` is `<repo>/app/src-tauri` at compile time, so `res/`
/// lives two levels up — mirrors `jwlcore::loader::dev_libs_dir`.
fn dev_resources_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../res"))
}

/// Repo-root-relative path to `res/resources.db`, used directly by tests and
/// as the first candidate at runtime (dev / `cargo tauri dev`).
pub fn dev_resources_db_path() -> PathBuf {
    dev_resources_dir().join("resources.db")
}

/// Resolves the on-disk path to the bundled `resources.db`. Prefers the
/// dev-tree `res/` (source checkout); falls back to the Tauri bundled
/// resource directory (`res/resources.db`, declared in `tauri.conf.json`)
/// when running from a packaged build and the dev path doesn't exist.
pub fn resolve_resources_db_path(app: &tauri::AppHandle) -> Result<PathBuf, ArchiveError> {
    let dev_path = dev_resources_db_path();
    if dev_path.exists() {
        return Ok(dev_path);
    }

    use tauri::Manager;
    app.path()
        .resolve("res/resources.db", tauri::path::BaseDirectory::Resource)
        .map_err(|_| ArchiveError::MissingResourcesDb)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn resources_lookups() {
        let catalog = ResourceCatalog::load(&dev_resources_db_path(), "en")
            .expect("resources.db must load for the English UI language");

        // Language name resolves for English's own Languages.Language id.
        let english_id = catalog
            .lang_name
            .iter()
            .find(|(_, name)| name.as_str() == "English")
            .map(|(id, _)| *id)
            .expect("English must be present in Languages");
        assert_eq!(catalog.lang_name(english_id), Some("English"));

        // A Bible book name resolves (Genesis is BibleBooks.Number = 1).
        assert_eq!(catalog.bible_book(1), Some("Genesis"));

        // At least one publication symbol resolves with a non-empty title.
        let (_symbol, info) = catalog
            .publications
            .iter()
            .next()
            .expect("resources.db must have at least one publication");
        assert!(
            !info.full.is_empty() || !info.short.is_empty(),
            "publication lookup must resolve a non-empty title"
        );
    }

    #[test]
    fn favorite_editions_lookup_and_language_breadth() {
        let catalog = ResourceCatalog::load(&dev_resources_db_path(), "en")
            .expect("resources.db must load for the English UI language");

        // English has favorite-eligible editions, including the New World
        // Translation under the "nwt" KeySymbol (confirmed against the real
        // bundled resources.db, not assumed).
        let english_editions = catalog.load_favorite_editions("English");
        assert!(
            !english_editions.is_empty(),
            "English must have at least one favorite-eligible edition"
        );
        assert!(
            english_editions
                .iter()
                .any(|e| e.symbol == "nwt" && e.short.contains("New World")),
            "expected an 'nwt' New World Translation row for English"
        );

        // An unknown language name returns empty, not an error.
        assert!(catalog.load_favorite_editions("Not A Real Language").is_empty());

        // The language list is the FULL bundled catalog (~1400 entries),
        // deliberately broader than the ~9 languages with favorite-eligible
        // editions — see `all_language_names`'s doc comment for why.
        let all_languages = catalog.all_language_names();
        assert!(
            all_languages.len() > 100,
            "all_language_names should return the full Languages catalog, not just \
             favorite-eligible languages"
        );
        assert!(all_languages.contains(&"English"));
        // Sorted, per the method's contract.
        let mut sorted_copy = all_languages.clone();
        sorted_copy.sort_unstable();
        assert_eq!(all_languages, sorted_copy);
    }
}
