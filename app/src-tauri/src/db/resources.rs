//! Bundled `res/resources.db` label lookups (Languages / BibleBooks /
//! Publications+Extras). Analog: `JWLManager.py:4023-4053` (`read_resources`).
//!
//! All SQL uses `rusqlite` bound parameters — never string interpolation.
//! The Python original interpolates `ui_lang` (an internal integer) directly
//! into the SQL text (`f"...WHERE Language = {ui_lang};"`); that is the
//! anti-pattern this port fixes (CLAUDE.md: no f-string/format-string SQL).

use crate::error::ArchiveError;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single publication/extra lookup row (`Publications`/`Extras` JOIN
/// `Types`), keyed by `Symbol` in `ResourceCatalog::publications`.
#[derive(Debug, Clone)]
pub struct PublicationInfo {
    pub short: String,
    pub full: String,
    pub year: Option<i64>,
    pub type_group: Option<String>,
}

/// Cached label-lookup maps loaded once from the bundled `resources.db`,
/// mirroring `read_resources`'s module-global cache (`lang_name`,
/// `bible_books`, `publications`) via a struct instead of Python globals.
#[derive(Debug, Clone)]
pub struct ResourceCatalog {
    lang_name: HashMap<i64, String>,
    bible_books: HashMap<i64, String>,
    publications: HashMap<String, PublicationInfo>,
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

        Ok(Self {
            lang_name,
            bible_books,
            publications,
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
}
