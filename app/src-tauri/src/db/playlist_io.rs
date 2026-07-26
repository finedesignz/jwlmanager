//! `.jwlplaylist` container (IO-01/IO-02/IO-03, 08-05-PLAN.md) — a
//! self-contained SQLite-in-zip mini-archive exported from a Playlist
//! selection and imported back into any archive with full re-keying.
//!
//! **Export** ports `export_playlist` (`JWLManager.py:1725-1818`): seeds a
//! fresh mini-database from `res/blank_playlist` via
//! [`crate::archive::extract::extract_zip_slip_safe`] (the only zip-open
//! path — D8-02), copies the selected `PlaylistItem` subtree in Python's
//! exact table order, and writes a compact-manifest zip via the SAME
//! `archive::manifest` serializer the main archive uses.
//!
//! **Import** ports `import_playlist`'s `update_db`
//! (`JWLManager.py:2444-2587`) with the RESEARCH-addendum-resolved re-keying
//! discipline: row identity on import is the semantic triple `(Label,
//! ThumbnailFilePath, playlist Tag Name)`, never the incoming
//! `PlaylistItemId`. A match reuses the target's existing row; a miss
//! allocates a FRESH id from the shared [`crate::db::ids`] gap pool. Every
//! dependent row (media map, location map, marker sub-maps, TagMap) is
//! written with the NEW id — no incoming primary key is ever trusted.

use crate::archive::extract::extract_zip_slip_safe;
use crate::archive::manifest::{compute_hash, Manifest, UserDataBackup};
use crate::db::edit::DryRunReport;
use crate::db::ids::take_id;
use crate::error::ArchiveError;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Typed non-empty `PlaylistItemId` selection for export — same D7-01 shape
/// as [`crate::db::favorites::NonEmptyTagMapIds`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyPlaylistItemIds.ts")]
pub struct NonEmptyPlaylistItemIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyPlaylistItemIds {
    type Error = String;

    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyPlaylistItemIds(ids))
        }
    }
}

impl NonEmptyPlaylistItemIds {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn iter(&self) -> std::slice::Iter<'_, i64> {
        self.0.iter()
    }
}

/// Result of [`export_playlist`]: the number of `PlaylistItem` rows actually
/// copied, plus any best-effort media-copy warnings (a missing source media
/// file never aborts the export, matching Python's `try/except` around
/// `shutil.copy2`).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/bindings/PlaylistExportReport.ts")]
pub struct PlaylistExportReport {
    pub item_count: usize,
    pub warnings: Vec<String>,
}

/// A `.jwlplaylist` extracted into a fresh temp directory (D8-04 — this
/// extraction, and the presence check below, run entirely BEFORE any
/// transaction opens on the target archive, so a bad container fails fast).
#[derive(Debug)]
pub struct PlaylistContainer {
    pub temp_dir: tempfile::TempDir,
    pub db_path: PathBuf,
}

/// Repo-bundled `.jwlplaylist` seed, dev-mode resolution — mirrors
/// `archive::new::dev_res_blank_path`.
fn dev_res_blank_playlist_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../res/blank_playlist")
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Exports the `PlaylistItem`s in `ids` (from `conn`, whose loose media files
/// live under `media_source_dir` — the live session's `temp_dir`) to `dest`
/// as a `.jwlplaylist`. Seeds from the repo-bundled `res/blank_playlist`.
#[allow(clippy::too_many_arguments)]
pub fn export_playlist(
    conn: &Connection,
    media_source_dir: &Path,
    ids: &NonEmptyPlaylistItemIds,
    dest: &Path,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<PlaylistExportReport, ArchiveError> {
    export_playlist_from_seed(
        &dev_res_blank_playlist_path(),
        conn,
        media_source_dir,
        ids,
        dest,
        app_name,
        device_name,
        now_iso8601,
    )
}

/// Testable core of [`export_playlist`] with an explicit seed path.
#[allow(clippy::too_many_arguments)]
pub fn export_playlist_from_seed(
    seed_path: &Path,
    conn: &Connection,
    media_source_dir: &Path,
    ids: &NonEmptyPlaylistItemIds,
    dest: &Path,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<PlaylistExportReport, ArchiveError> {
    let temp_dir = tempfile::TempDir::new()?;
    extract_zip_slip_safe(seed_path, temp_dir.path())?;
    let db_path = temp_dir.path().join("userData.db");

    let stem = dest
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Playlist".to_string());

    let mut warnings = Vec::new();
    let item_count;

    {
        let mut expconn = Connection::open(&db_path)?;
        expconn.execute_batch(
            "PRAGMA temp_store = 'MEMORY'; PRAGMA journal_mode = 'OFF'; \
             PRAGMA foreign_keys = 'OFF';",
        )?;
        let tx = expconn.transaction()?;

        // The hardcoded playlist Tag (`JWLManager.py:1728`).
        tx.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (1, 2, ?1)",
            params![stem],
        )?;

        // Conditional android_metadata locale copy (`:1730-1733`) — only when
        // the SOURCE archive actually has that table.
        let has_android: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'android_metadata'",
            [],
            |r| r.get::<_, i64>(0),
        )? > 0;
        if has_android {
            let locale: Option<String> = conn
                .query_row("SELECT locale FROM android_metadata", [], |r| r.get(0))
                .optional()?;
            if let Some(locale) = locale {
                tx.execute("UPDATE android_metadata SET locale = ?1", params![locale])?;
            }
        }

        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");

        // PlaylistItem (`:1735-1736`).
        {
            let sql = format!(
                "SELECT PlaylistItemId, Label, StartTrimOffsetTicks, EndTrimOffsetTicks, \
                 Accuracy, EndAction, ThumbnailFilePath FROM PlaylistItem \
                 WHERE PlaylistItemId IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(ids.iter()))?;
            let mut count = 0usize;
            while let Some(row) = rows.next()? {
                let pi_id: i64 = row.get(0)?;
                let label: String = row.get(1)?;
                let start: Option<i64> = row.get(2)?;
                let end: Option<i64> = row.get(3)?;
                let accuracy: i64 = row.get(4)?;
                let end_action: i64 = row.get(5)?;
                let thumb: Option<String> = row.get(6)?;
                tx.execute(
                    "INSERT INTO PlaylistItem (PlaylistItemId, Label, StartTrimOffsetTicks, \
                     EndTrimOffsetTicks, Accuracy, EndAction, ThumbnailFilePath) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![pi_id, label, start, end, accuracy, end_action, thumb],
                )?;
                count += 1;
            }
            item_count = count;
        }

        // PlaylistItemLocationMap (`:1741-1742`).
        {
            let sql = format!(
                "SELECT PlaylistItemId, LocationId, MajorMultimediaType, BaseDurationTicks \
                 FROM PlaylistItemLocationMap WHERE PlaylistItemId IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(ids.iter()))?;
            while let Some(row) = rows.next()? {
                let pi_id: i64 = row.get(0)?;
                let loc_id: i64 = row.get(1)?;
                let mmt: i64 = row.get(2)?;
                let bdt: Option<i64> = row.get(3)?;
                tx.execute(
                    "INSERT INTO PlaylistItemLocationMap \
                     (PlaylistItemId, LocationId, MajorMultimediaType, BaseDurationTicks) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![pi_id, loc_id, mmt, bdt],
                )?;
            }
        }

        // PlaylistItemMarker (`:1744-1745`).
        {
            let sql = format!(
                "SELECT PlaylistItemMarkerId, PlaylistItemId, Label, StartTimeTicks, \
                 DurationTicks, EndTransitionDurationTicks FROM PlaylistItemMarker \
                 WHERE PlaylistItemId IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(ids.iter()))?;
            while let Some(row) = rows.next()? {
                let marker_id: i64 = row.get(0)?;
                let pi_id: i64 = row.get(1)?;
                let label: String = row.get(2)?;
                let stt: i64 = row.get(3)?;
                let dt: i64 = row.get(4)?;
                let etdt: i64 = row.get(5)?;
                tx.execute(
                    "INSERT INTO PlaylistItemMarker \
                     (PlaylistItemMarkerId, PlaylistItemId, Label, StartTimeTicks, \
                      DurationTicks, EndTransitionDurationTicks) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![marker_id, pi_id, label, stt, dt, etdt],
                )?;
            }
        }

        // `pm` — the marker ids just inserted into the DEST db (`:1747-1748`).
        let marker_ids: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT PlaylistItemMarkerId FROM PlaylistItemMarker")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        if !marker_ids.is_empty() {
            let marker_placeholders = std::iter::repeat_n("?", marker_ids.len())
                .collect::<Vec<_>>()
                .join(",");

            // PlaylistItemMarkerBibleVerseMap (`:1750-1751`).
            {
                let sql = format!(
                    "SELECT PlaylistItemMarkerId, VerseId FROM PlaylistItemMarkerBibleVerseMap \
                     WHERE PlaylistItemMarkerId IN ({marker_placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params_from_iter(marker_ids.iter()))?;
                while let Some(row) = rows.next()? {
                    let marker_id: i64 = row.get(0)?;
                    let verse_id: i64 = row.get(1)?;
                    tx.execute(
                        "INSERT INTO PlaylistItemMarkerBibleVerseMap \
                         (PlaylistItemMarkerId, VerseId) VALUES (?1, ?2)",
                        params![marker_id, verse_id],
                    )?;
                }
            }

            // PlaylistItemMarkerParagraphMap (`:1753-1754`).
            {
                let sql = format!(
                    "SELECT PlaylistItemMarkerId, MepsDocumentId, ParagraphIndex, \
                     MarkerIndexWithinParagraph FROM PlaylistItemMarkerParagraphMap \
                     WHERE PlaylistItemMarkerId IN ({marker_placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params_from_iter(marker_ids.iter()))?;
                while let Some(row) = rows.next()? {
                    let marker_id: i64 = row.get(0)?;
                    let meps_doc: i64 = row.get(1)?;
                    let para_idx: i64 = row.get(2)?;
                    let marker_idx: i64 = row.get(3)?;
                    tx.execute(
                        "INSERT INTO PlaylistItemMarkerParagraphMap \
                         (PlaylistItemMarkerId, MepsDocumentId, ParagraphIndex, \
                          MarkerIndexWithinParagraph) VALUES (?1, ?2, ?3, ?4)",
                        params![marker_id, meps_doc, para_idx, marker_idx],
                    )?;
                }
            }
        }

        // TagMap — re-keyed to the hardcoded TagId=1, dense 0-based Position
        // in source `(TagId, Position)` order (`:1756-1760`).
        {
            let sql = format!(
                "SELECT PlaylistItemId FROM TagMap WHERE PlaylistItemId IN ({placeholders}) \
                 ORDER BY TagId, Position"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(ids.iter()))?;
            let mut pos: i64 = 0;
            while let Some(row) = rows.next()? {
                let pi_id: i64 = row.get(0)?;
                tx.execute(
                    "INSERT OR IGNORE INTO TagMap (PlaylistItemId, TagId, Position) \
                     VALUES (?1, 1, ?2)",
                    params![pi_id, pos],
                )?;
                pos += 1;
            }
        }

        // PlaylistItemIndependentMediaMap (`:1762-1763`).
        {
            let sql = format!(
                "SELECT PlaylistItemId, IndependentMediaId, DurationTicks \
                 FROM PlaylistItemIndependentMediaMap WHERE PlaylistItemId IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(ids.iter()))?;
            while let Some(row) = rows.next()? {
                let pi_id: i64 = row.get(0)?;
                let media_id: i64 = row.get(1)?;
                let duration: i64 = row.get(2)?;
                tx.execute(
                    "INSERT INTO PlaylistItemIndependentMediaMap \
                     (PlaylistItemId, IndependentMediaId, DurationTicks) VALUES (?1, ?2, ?3)",
                    params![pi_id, media_id, duration],
                )?;
            }
        }

        // PlaylistItemAccuracy — unfiltered, whole table (`:1765-1766`).
        {
            let mut stmt =
                conn.prepare("SELECT PlaylistItemAccuracyId, Description FROM PlaylistItemAccuracy")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                let desc: String = row.get(1)?;
                tx.execute(
                    "INSERT INTO PlaylistItemAccuracy (PlaylistItemAccuracyId, Description) \
                     VALUES (?1, ?2)",
                    params![id, desc],
                )?;
            }
        }

        // IndependentMedia — union of the thumbnail-FilePath and
        // media-map-IndependentMediaId predicates (`:1768-1774`), best-effort
        // file copy (`:1775-1779`).
        {
            let thumb_paths: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT ThumbnailFilePath FROM PlaylistItem WHERE ThumbnailFilePath IS NOT NULL",
                )?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            let media_ids: Vec<i64> = {
                let mut stmt =
                    tx.prepare("SELECT IndependentMediaId FROM PlaylistItemIndependentMediaMap")?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };

            if !thumb_paths.is_empty() || !media_ids.is_empty() {
                let path_placeholders = std::iter::repeat_n("?", thumb_paths.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let id_placeholders = std::iter::repeat_n("?", media_ids.len())
                    .collect::<Vec<_>>()
                    .join(",");
                // Both predicate lists may legitimately be empty (an
                // IN () clause matches nothing, never everything) — build
                // the WHERE with only the non-empty side(s) present.
                let clause = match (thumb_paths.is_empty(), media_ids.is_empty()) {
                    (false, false) => format!(
                        "FilePath IN ({path_placeholders}) OR IndependentMediaId IN ({id_placeholders})"
                    ),
                    (false, true) => format!("FilePath IN ({path_placeholders})"),
                    (true, false) => format!("IndependentMediaId IN ({id_placeholders})"),
                    (true, true) => "0".to_string(),
                };
                let sql = format!(
                    "SELECT IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash \
                     FROM IndependentMedia WHERE {clause}"
                );
                let mut bound: Vec<rusqlite::types::Value> = Vec::new();
                for p in &thumb_paths {
                    bound.push(rusqlite::types::Value::Text(p.clone()));
                }
                for id in &media_ids {
                    bound.push(rusqlite::types::Value::Integer(*id));
                }
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params_from_iter(bound.iter()))?;
                while let Some(row) = rows.next()? {
                    let id: i64 = row.get(0)?;
                    let original_filename: String = row.get(1)?;
                    let file_path: String = row.get(2)?;
                    let mime_type: String = row.get(3)?;
                    let hash: String = row.get(4)?;
                    tx.execute(
                        "INSERT INTO IndependentMedia \
                         (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![id, original_filename, file_path, mime_type, hash],
                    )?;
                    let src = media_source_dir.join(&file_path);
                    let dst = temp_dir.path().join(&file_path);
                    if let Err(err) = fs::copy(&src, &dst) {
                        warnings.push(format!(
                            "Problem with \"{file_path}\": {err} — export will be incomplete."
                        ));
                    }
                }
            }
        }

        // Location — filtered to exactly the LocationIds referenced by the
        // copied PlaylistItemLocationMap (`:1781-1787`).
        {
            let loc_ids: Vec<i64> = {
                let mut stmt = tx.prepare("SELECT LocationId FROM PlaylistItemLocationMap")?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            if !loc_ids.is_empty() {
                let loc_placeholders = std::iter::repeat_n("?", loc_ids.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
                     IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition \
                     FROM Location WHERE LocationId IN ({loc_placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params_from_iter(loc_ids.iter()))?;
                while let Some(row) = rows.next()? {
                    let id: i64 = row.get(0)?;
                    let book: Option<i64> = row.get(1)?;
                    let chapter: Option<i64> = row.get(2)?;
                    let doc: Option<i64> = row.get(3)?;
                    let track: Option<i64> = row.get(4)?;
                    let itn: i64 = row.get(5)?;
                    let key: Option<String> = row.get(6)?;
                    let lang: Option<i64> = row.get(7)?;
                    let kind: i64 = row.get(8)?;
                    let title: Option<String> = row.get(9)?;
                    let specialty: Option<String> = row.get(10)?;
                    let edition: Option<String> = row.get(11)?;
                    tx.execute(
                        "INSERT INTO Location \
                         (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
                          IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                        params![
                            id, book, chapter, doc, track, itn, key, lang, kind, title, specialty,
                            edition
                        ],
                    )?;
                }
            }
        }

        tx.execute(
            "UPDATE LastModified SET LastModified = ?1",
            params![now_iso8601],
        )?;
        tx.commit()?;

        expconn.execute_batch("PRAGMA foreign_keys = 'ON'; VACUUM;")?;
    } // `expconn` closes here — hash-last discipline (D-04 shape, reused).

    let hash = compute_hash(&db_path)?;
    let manifest = Manifest {
        name: app_name.to_string(),
        creation_date: now_iso8601.to_string(),
        version: 1,
        archive_type: 1,
        user_data_backup: UserDataBackup {
            last_modified_date: now_iso8601.to_string(),
            device_name: device_name.to_string(),
            database_name: "userData.db".to_string(),
            hash,
            schema_version: 16,
            extra: serde_json::Map::new(),
        },
        extra: serde_json::Map::new(),
    };
    let manifest_bytes = manifest.to_compact_string()?.into_bytes();
    fs::write(temp_dir.path().join("manifest.json"), &manifest_bytes)?;

    zip_directory(temp_dir.path(), dest)?;

    Ok(PlaylistExportReport {
        item_count,
        warnings,
    })
}

/// Zips every file directly inside `dir` (non-recursive — the mini-archive
/// working copy is always flat: `userData.db`, `manifest.json`,
/// `default_thumbnail.png`, plus any copied media files) into `dest`,
/// mirroring `ZipFile(fname, 'w', compression=ZIP_DEFLATED)` (`:1813-1816`).
fn zip_directory(dir: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let file = fs::File::create(dest)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();

    for name in names {
        let bytes = fs::read(dir.join(&name))?;
        writer.start_file(&name, options)?;
        writer.write_all(&bytes)?;
    }

    let file = writer.finish()?;
    file.sync_all()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Extracts a user-supplied `.jwlplaylist` into a fresh temp directory via
/// [`extract_zip_slip_safe`] (the ONLY zip-open path for untrusted input —
/// D8-02) and verifies the two required members are present. Runs entirely
/// BEFORE any transaction opens on the target archive (D8-04).
pub fn read_playlist_container(path: &Path) -> Result<PlaylistContainer, ArchiveError> {
    let temp_dir = tempfile::TempDir::new()?;
    extract_zip_slip_safe(path, temp_dir.path())?;

    let db_path = temp_dir.path().join("userData.db");
    let manifest_path = temp_dir.path().join("manifest.json");
    if !db_path.is_file() || !manifest_path.is_file() {
        return Err(ArchiveError::PlaylistImportFailed {
            reason: "container is missing userData.db or manifest.json".to_string(),
        });
    }

    // Fail fast on a corrupt/non-SQLite userData.db too, before any
    // transaction opens on the TARGET archive.
    Connection::open(&db_path)?
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;

    Ok(PlaylistContainer { temp_dir, db_path })
}

/// Counts the container's `IndependentMedia` rows, for the import preview's
/// "its {N} media files" clause (UI-SPEC).
pub fn count_container_media(container: &PlaylistContainer) -> Result<usize, ArchiveError> {
    let conn = Connection::open(&container.db_path)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM IndependentMedia", [], |r| r.get(0))?;
    Ok(count as usize)
}

struct SourcePlaylistItem {
    label: String,
    start_trim: Option<i64>,
    end_trim: Option<i64>,
    accuracy_description: String,
    end_action: i64,
    thumbnail_file_path: Option<String>,
}

struct SourceMedia {
    original_filename: String,
    file_path: String,
    mime_type: String,
    hash: String,
}

struct SourceLocation {
    book_number: Option<i64>,
    chapter_number: Option<i64>,
    document_id: Option<i64>,
    track: Option<i64>,
    issue_tag_number: Option<i64>,
    key_symbol: Option<String>,
    meps_language: Option<i64>,
    kind: i64,
    title: Option<String>,
    specialty: Option<String>,
    edition: Option<String>,
}

struct SourceMarker {
    label: String,
    start_time_ticks: i64,
    duration_ticks: i64,
    end_transition_duration_ticks: i64,
    verse_id: Option<i64>,
    paragraph: Option<(i64, i64, i64)>,
}

fn load_source_playlist_item(
    container_conn: &Connection,
    pi_id: i64,
) -> Result<SourcePlaylistItem, ArchiveError> {
    container_conn
        .query_row(
            "SELECT p.Label, p.StartTrimOffsetTicks, p.EndTrimOffsetTicks, a.Description, \
             p.EndAction, p.ThumbnailFilePath \
             FROM PlaylistItem p JOIN PlaylistItemAccuracy a ON p.Accuracy = a.PlaylistItemAccuracyId \
             WHERE p.PlaylistItemId = ?1",
            params![pi_id],
            |row| {
                Ok(SourcePlaylistItem {
                    label: row.get(0)?,
                    start_trim: row.get(1)?,
                    end_trim: row.get(2)?,
                    accuracy_description: row.get(3)?,
                    end_action: row.get(4)?,
                    thumbnail_file_path: row.get(5)?,
                })
            },
        )
        .map_err(ArchiveError::from)
}

fn load_source_media_maps(
    container_conn: &Connection,
    pi_id: i64,
) -> Result<Vec<(SourceMedia, i64)>, ArchiveError> {
    let mut stmt = container_conn.prepare(
        "SELECT i.OriginalFilename, i.FilePath, i.MimeType, i.Hash, m.DurationTicks \
         FROM PlaylistItemIndependentMediaMap m JOIN IndependentMedia i USING (IndependentMediaId) \
         WHERE m.PlaylistItemId = ?1",
    )?;
    let rows = stmt.query_map(params![pi_id], |row| {
        Ok((
            SourceMedia {
                original_filename: row.get(0)?,
                file_path: row.get(1)?,
                mime_type: row.get(2)?,
                hash: row.get(3)?,
            },
            row.get::<_, i64>(4)?,
        ))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(ArchiveError::from)
}

/// Resolves the `IndependentMedia` row backing a `PlaylistItem`'s own
/// `ThumbnailFilePath` — a SEPARATE lookup from [`load_source_media_maps`]
/// (which only returns rows actually present in
/// `PlaylistItemIndependentMediaMap`, i.e. "full media"; a thumbnail is
/// referenced solely via the `PlaylistItem.ThumbnailFilePath` FK, matching
/// Python's own two-source shape: `add_thumbnails` walks the map table,
/// while the thumbnail itself is resolved through the main row's
/// `JOIN IndependentMedia i ON i.FilePath = p.ThumbnailFilePath`,
/// `JWLManager.py:2558`).
fn load_source_thumbnail_media(
    container_conn: &Connection,
    thumbnail_file_path: &str,
) -> Result<Option<SourceMedia>, ArchiveError> {
    container_conn
        .query_row(
            "SELECT OriginalFilename, FilePath, MimeType, Hash FROM IndependentMedia WHERE FilePath = ?1",
            params![thumbnail_file_path],
            |row| {
                Ok(SourceMedia {
                    original_filename: row.get(0)?,
                    file_path: row.get(1)?,
                    mime_type: row.get(2)?,
                    hash: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(ArchiveError::from)
}

fn load_source_location_map(
    container_conn: &Connection,
    pi_id: i64,
) -> Result<Option<(i64, Option<i64>, SourceLocation)>, ArchiveError> {
    container_conn
        .query_row(
            "SELECT m.MajorMultimediaType, m.BaseDurationTicks, l.BookNumber, l.ChapterNumber, \
             l.DocumentId, l.Track, l.IssueTagNumber, l.KeySymbol, l.MepsLanguage, l.Type, \
             l.Title, l.Specialty, l.Edition \
             FROM PlaylistItemLocationMap m JOIN Location l USING (LocationId) \
             WHERE m.PlaylistItemId = ?1 ORDER BY l.LocationId LIMIT 1",
            params![pi_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    SourceLocation {
                        book_number: row.get(2)?,
                        chapter_number: row.get(3)?,
                        document_id: row.get(4)?,
                        track: row.get(5)?,
                        issue_tag_number: row.get(6)?,
                        key_symbol: row.get(7)?,
                        meps_language: row.get(8)?,
                        kind: row.get(9)?,
                        title: row.get(10)?,
                        specialty: row.get(11)?,
                        edition: row.get(12)?,
                    },
                ))
            },
        )
        .optional()
        .map_err(ArchiveError::from)
}

fn load_source_marker(
    container_conn: &Connection,
    pi_id: i64,
) -> Result<Option<SourceMarker>, ArchiveError> {
    let marker: Option<(i64, String, i64, i64, i64)> = container_conn
        .query_row(
            "SELECT PlaylistItemMarkerId, Label, StartTimeTicks, DurationTicks, \
             EndTransitionDurationTicks FROM PlaylistItemMarker WHERE PlaylistItemId = ?1",
            params![pi_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;

    let Some((marker_id, label, stt, dt, etdt)) = marker else {
        return Ok(None);
    };

    let verse_id: Option<i64> = container_conn
        .query_row(
            "SELECT VerseId FROM PlaylistItemMarkerBibleVerseMap WHERE PlaylistItemMarkerId = ?1",
            params![marker_id],
            |r| r.get(0),
        )
        .optional()?;
    let paragraph: Option<(i64, i64, i64)> = container_conn
        .query_row(
            "SELECT MepsDocumentId, ParagraphIndex, MarkerIndexWithinParagraph \
             FROM PlaylistItemMarkerParagraphMap WHERE PlaylistItemMarkerId = ?1",
            params![marker_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    Ok(Some(SourceMarker {
        label,
        start_time_ticks: stt,
        duration_ticks: dt,
        end_transition_duration_ticks: etdt,
        verse_id,
        paragraph,
    }))
}

/// Resolves an incoming `IndependentMedia` row to a target id + FilePath —
/// dedups by `Hash` (an exact-content match reuses the target's existing
/// row and copies nothing); a miss allocates a fresh id and, when
/// `target_media_dir` is `Some` (a real apply, never a dry run), copies the
/// file in, disambiguating the destination filename exactly like
/// `add_media`'s `tmp`/`ext` loop (`JWLManager.py:2457-2464`).
fn resolve_target_media(
    tx: &Transaction,
    media: &SourceMedia,
    container_media_dir: &Path,
    target_media_dir: Option<&Path>,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<(i64, String), ArchiveError> {
    let existing: Option<(i64, String)> = tx
        .query_row(
            "SELECT IndependentMediaId, FilePath FROM IndependentMedia WHERE Hash = ?1",
            params![media.hash],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some(found) = existing {
        return Ok(found);
    }

    let mut candidate = media.file_path.clone();
    if let Some(dir) = target_media_dir {
        let mut ext = 0u32;
        while dir.join(&candidate).exists() {
            ext += 1;
            candidate = format!("{}_{ext}", media.file_path);
        }
    }

    let new_id = if let Some(id) = take_id(available, "IndependentMedia") {
        tx.execute(
            "INSERT INTO IndependentMedia (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, media.original_filename, candidate, media.mime_type, media.hash],
        )?;
        id
    } else {
        tx.execute(
            "INSERT INTO IndependentMedia (OriginalFilename, FilePath, MimeType, Hash) \
             VALUES (?1, ?2, ?3, ?4)",
            params![media.original_filename, candidate, media.mime_type, media.hash],
        )?;
        tx.last_insert_rowid()
    };

    if let Some(dir) = target_media_dir {
        let src = container_media_dir.join(&media.file_path);
        let dst = dir.join(&candidate);
        fs::copy(&src, &dst).map_err(|err| ArchiveError::PlaylistImportFailed {
            reason: format!("copying media file \"{}\": {err}", media.file_path),
        })?;
    }

    Ok((new_id, candidate))
}

/// Finds-or-inserts a playlist-item `Location` — ports the two-branch
/// bible-shaped-vs-track-shaped predicate of `add_locations`
/// (`JWLManager.py:2530-2549`).
fn find_or_insert_playlist_location(
    tx: &Transaction,
    loc: &SourceLocation,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let is_bible_shaped = matches!(loc.book_number, Some(n) if n != 0);

    if is_bible_shaped {
        let existing: Option<i64> = tx
            .query_row(
                "SELECT LocationId FROM Location \
                 WHERE BookNumber = ?1 AND ChapterNumber = ?2 AND KeySymbol = ?3 AND MepsLanguage = ?4",
                params![loc.book_number, loc.chapter_number, loc.key_symbol, loc.meps_language],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        if let Some(id) = take_id(available, "Location") {
            tx.execute(
                "INSERT INTO Location \
                 (LocationId, BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, loc.book_number, loc.chapter_number, loc.key_symbol, loc.meps_language, loc.kind, loc.title, loc.specialty, loc.edition],
            )?;
            Ok(id)
        } else {
            tx.execute(
                "INSERT INTO Location \
                 (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![loc.book_number, loc.chapter_number, loc.key_symbol, loc.meps_language, loc.kind, loc.title, loc.specialty, loc.edition],
            )?;
            Ok(tx.last_insert_rowid())
        }
    } else {
        let existing: Option<i64> = tx
            .query_row(
                "SELECT LocationId FROM Location \
                 WHERE Track = ?1 AND IssueTagNumber = ?2 AND KeySymbol = ?3 AND MepsLanguage = ?4 AND Type = ?5",
                params![loc.track, loc.issue_tag_number, loc.key_symbol, loc.meps_language, loc.kind],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        if let Some(id) = take_id(available, "Location") {
            tx.execute(
                "INSERT INTO Location \
                 (LocationId, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![id, loc.document_id, loc.track, loc.issue_tag_number, loc.key_symbol, loc.meps_language, loc.kind, loc.title, loc.specialty, loc.edition],
            )?;
            Ok(id)
        } else {
            tx.execute(
                "INSERT INTO Location \
                 (DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![loc.document_id, loc.track, loc.issue_tag_number, loc.key_symbol, loc.meps_language, loc.kind, loc.title, loc.specialty, loc.edition],
            )?;
            Ok(tx.last_insert_rowid())
        }
    }
}

fn ensure_playlist_accuracy(tx: &Transaction, description: &str) -> Result<i64, ArchiveError> {
    tx.execute(
        "INSERT OR IGNORE INTO PlaylistItemAccuracy (Description) VALUES (?1)",
        params![description],
    )?;
    tx.query_row(
        "SELECT PlaylistItemAccuracyId FROM PlaylistItemAccuracy WHERE Description = ?1",
        params![description],
        |r| r.get(0),
    )
    .map_err(ArchiveError::from)
}

/// Resolves-or-creates the target's playlist `Tag (Type = 2, Name = ?)` —
/// ports `add_tag` (`JWLManager.py:2496-2503`).
fn ensure_playlist_tag(
    tx: &Transaction,
    playlist_name: &str,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT TagId FROM Tag WHERE Type = 2 AND Name = ?1",
            params![playlist_name],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    if let Some(id) = take_id(available, "Tag") {
        tx.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (?1, 2, ?2)",
            params![id, playlist_name],
        )?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Tag (Type, Name) VALUES (2, ?1)",
            params![playlist_name],
        )?;
        Ok(tx.last_insert_rowid())
    }
}

/// Applies a `.jwlplaylist` import inside the caller's transaction —
/// re-keys every `PlaylistItem`: a semantic `(Label, ThumbnailFilePath,
/// playlist Tag Name)` match reuses the target's existing row (counted as
/// `skipped`); a miss allocates a FRESH id from `available` and every
/// dependent row (media map, location map, marker sub-maps, TagMap) is
/// written with that NEW id — never the incoming `PlaylistItemId`.
///
/// `target_media_dir`: `None` for a dry run (no filesystem writes — D8-04's
/// "dry run touches nothing outside its own temp extraction"); `Some(dir)`
/// for a real apply, where every DB write is staged into `tx` first and
/// media files are copied in AFTER (PD-3) — a copy failure returns `Err`
/// and the caller must not commit, so the whole run rolls back atomically.
pub fn apply_import_playlist(
    tx: &Transaction,
    container: &PlaylistContainer,
    playlist_name: &str,
    target_media_dir: Option<&Path>,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<usize, ArchiveError> {
    let container_conn = Connection::open(&container.db_path)?;
    let tag_id = ensure_playlist_tag(tx, playlist_name, available)?;

    let item_ids: Vec<i64> = {
        let mut stmt =
            container_conn.prepare("SELECT PlaylistItemId FROM PlaylistItem ORDER BY PlaylistItemId")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let mut skipped = 0usize;

    for pi_id in item_ids {
        let item = load_source_playlist_item(&container_conn, pi_id)?;
        let media_rows = load_source_media_maps(&container_conn, pi_id)?;

        let mut new_media: Vec<(i64, i64, String)> = Vec::with_capacity(media_rows.len());
        for (media, duration) in &media_rows {
            let (new_media_id, new_fp) = resolve_target_media(
                tx,
                media,
                container.temp_dir.path(),
                target_media_dir,
                available,
            )?;
            new_media.push((new_media_id, *duration, new_fp));
        }

        // The thumbnail is resolved SEPARATELY from the media map (see
        // `load_source_thumbnail_media` docs) — `resolve_target_media`
        // dedups by `Hash`, so calling it a second time for a media row that
        // ALSO happens to be in `media_rows` is safe (reuses the same row,
        // never double-inserts).
        let mut new_thumbnail_fp: Option<String> = None;
        if let Some(thumb_fp) = &item.thumbnail_file_path {
            if let Some(media) = load_source_thumbnail_media(&container_conn, thumb_fp)? {
                let (_, new_fp) = resolve_target_media(
                    tx,
                    &media,
                    container.temp_dir.path(),
                    target_media_dir,
                    available,
                )?;
                new_thumbnail_fp = Some(new_fp);
            }
        }

        let accuracy_id = ensure_playlist_accuracy(tx, &item.accuracy_description)?;

        let existing_id: Option<i64> = tx
            .query_row(
                "SELECT pi.PlaylistItemId FROM PlaylistItem pi \
                 JOIN TagMap tm ON pi.PlaylistItemId = tm.PlaylistItemId \
                 JOIN Tag t ON tm.TagId = t.TagId \
                 WHERE pi.Label = ?1 AND pi.ThumbnailFilePath IS ?2 AND t.Name = ?3",
                params![item.label, new_thumbnail_fp, playlist_name],
                |r| r.get(0),
            )
            .optional()?;

        let new_pi_id = if let Some(id) = existing_id {
            skipped += 1;
            id
        } else if let Some(id) = take_id(available, "PlaylistItem") {
            tx.execute(
                "INSERT INTO PlaylistItem \
                 (PlaylistItemId, Label, StartTrimOffsetTicks, EndTrimOffsetTicks, Accuracy, EndAction, ThumbnailFilePath) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, item.label, item.start_trim, item.end_trim, accuracy_id, item.end_action, new_thumbnail_fp],
            )?;
            id
        } else {
            tx.execute(
                "INSERT INTO PlaylistItem \
                 (Label, StartTrimOffsetTicks, EndTrimOffsetTicks, Accuracy, EndAction, ThumbnailFilePath) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![item.label, item.start_trim, item.end_trim, accuracy_id, item.end_action, new_thumbnail_fp],
            )?;
            tx.last_insert_rowid()
        };

        // Media map — ALWAYS re-processed (thumbnail + any full-media rows),
        // regardless of whether the item itself was reused (`add_thumbnails`
        // runs unconditionally, `:2572`).
        for (new_media_id, duration, _) in &new_media {
            tx.execute(
                "INSERT INTO PlaylistItemIndependentMediaMap (PlaylistItemId, IndependentMediaId, DurationTicks) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(PlaylistItemId, IndependentMediaId) DO UPDATE SET DurationTicks = excluded.DurationTicks",
                params![new_pi_id, new_media_id, duration],
            )?;
        }

        // Location map (0 or 1 per item in practice).
        if let Some((mmt, bdt, loc)) = load_source_location_map(&container_conn, pi_id)? {
            let new_loc_id = find_or_insert_playlist_location(tx, &loc, available)?;
            tx.execute(
                "INSERT INTO PlaylistItemLocationMap (PlaylistItemId, LocationId, MajorMultimediaType, BaseDurationTicks) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(PlaylistItemId, LocationId) DO UPDATE SET \
                 MajorMultimediaType = excluded.MajorMultimediaType, BaseDurationTicks = excluded.BaseDurationTicks",
                params![new_pi_id, new_loc_id, mmt, bdt],
            )?;
        }

        // Marker + sub-maps.
        if let Some(marker) = load_source_marker(&container_conn, pi_id)? {
            let existing_marker_id: Option<i64> = tx
                .query_row(
                    "SELECT PlaylistItemMarkerId FROM PlaylistItemMarker WHERE PlaylistItemId = ?1",
                    params![new_pi_id],
                    |r| r.get(0),
                )
                .optional()?;
            let new_marker_id = if let Some(id) = existing_marker_id {
                id
            } else {
                tx.execute(
                    "INSERT INTO PlaylistItemMarker \
                     (PlaylistItemId, Label, StartTimeTicks, DurationTicks, EndTransitionDurationTicks) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![new_pi_id, marker.label, marker.start_time_ticks, marker.duration_ticks, marker.end_transition_duration_ticks],
                )?;
                tx.last_insert_rowid()
            };
            if let Some(verse_id) = marker.verse_id {
                tx.execute(
                    "INSERT INTO PlaylistItemMarkerBibleVerseMap (PlaylistItemMarkerId, VerseId) \
                     VALUES (?1, ?2) \
                     ON CONFLICT(PlaylistItemMarkerId, VerseId) DO UPDATE SET VerseId = excluded.VerseId",
                    params![new_marker_id, verse_id],
                )?;
            }
            if let Some((meps_doc, para_idx, marker_idx)) = marker.paragraph {
                tx.execute(
                    "INSERT INTO PlaylistItemMarkerParagraphMap \
                     (PlaylistItemMarkerId, MepsDocumentId, ParagraphIndex, MarkerIndexWithinParagraph) \
                     VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(PlaylistItemMarkerId, MepsDocumentId, ParagraphIndex, MarkerIndexWithinParagraph) DO UPDATE SET \
                     MepsDocumentId = excluded.MepsDocumentId, ParagraphIndex = excluded.ParagraphIndex, \
                     MarkerIndexWithinParagraph = excluded.MarkerIndexWithinParagraph",
                    params![new_marker_id, meps_doc, para_idx, marker_idx],
                )?;
            }
        }

        // TagMap — ported literally including the exact-tuple `WHERE NOT
        // EXISTS` guard (`:2508-2512`): Python's own guard checks the exact
        // (PlaylistItemId, TagId, Position) triple, not just the pair, since
        // `position` is freshly computed as `max(Position)+1` every call.
        let position: i64 = tx.query_row(
            "SELECT IFNULL(MAX(Position), -1) + 1 FROM TagMap WHERE TagId = ?1",
            params![tag_id],
            |r| r.get(0),
        )?;
        if let Some(tagmap_id) = take_id(available, "TagMap") {
            tx.execute(
                "INSERT INTO TagMap (TagMapId, PlaylistItemId, TagId, Position) \
                 SELECT ?1, ?2, ?3, ?4 WHERE NOT EXISTS \
                 (SELECT 1 FROM TagMap WHERE PlaylistItemId = ?2 AND TagId = ?3 AND Position = ?4)",
                params![tagmap_id, new_pi_id, tag_id, position],
            )?;
        } else {
            tx.execute(
                "INSERT INTO TagMap (PlaylistItemId, TagId, Position) \
                 SELECT ?1, ?2, ?3 WHERE NOT EXISTS \
                 (SELECT 1 FROM TagMap WHERE PlaylistItemId = ?1 AND TagId = ?2 AND Position = ?3)",
                params![new_pi_id, tag_id, position],
            )?;
        }
    }

    Ok(skipped)
}

/// Never-committed-transaction preview (SAFE-01 shape every `dry_run_*` in
/// this app uses) — leaves the target archive's row counts unchanged and
/// writes no file (media copy is skipped: `target_media_dir = None`).
pub fn dry_run_import_playlist(
    conn: &mut Connection,
    container: &PlaylistContainer,
    playlist_name: &str,
) -> Result<DryRunReport, ArchiveError> {
    let guard = crate::db::pragma_guard::PragmaGuard::new(conn)?;
    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; PRAGMA foreign_keys = 'OFF';",
    )?;
    let tx = conn.unchecked_transaction()?;

    let mut available = crate::db::ids::compute_available_ids(&tx)?;
    let before = crate::db::edit::snapshot_tables(&tx, crate::db::edit::PLAYLIST_IMPORT_SNAPSHOT_TABLES)?;
    let skipped = apply_import_playlist(&tx, container, playlist_name, None, &mut available)?;
    crate::db::trim::trim_sweep(&tx)?;
    let after = crate::db::edit::snapshot_tables(&tx, crate::db::edit::PLAYLIST_IMPORT_SNAPSHOT_TABLES)?;

    let mut report = crate::db::edit::diff_snapshots(&before, &after);
    if skipped > 0 {
        report.skipped.insert("PlaylistItem".to_string(), skipped);
    }

    drop(tx);
    drop(guard);

    Ok(report)
}
