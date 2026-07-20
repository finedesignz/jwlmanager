//! New empty v16 archive (ARCH-06, finding 5) + save-as (ARCH-07, D-05).
//!
//! `new_archive` seeds a brand-new session from `res/blank` — the REAL JW
//! Library v16 empty-archive seed (`userData.db` already at
//! `PRAGMA user_version = 16`, plus `default_thumbnail.png`; no inner
//! `manifest.json`, per 01-01's inspection). This deliberately does NOT
//! hand-build a minimal schema: a hand-built DB would be an unverified guess
//! at JW Library's real empty-archive shape and would make the ARCH-02
//! differential oracle circular (this app's own guess "verifying" itself).
//!
//! `save_as` writes to a caller-chosen path via the same atomic full-inventory
//! save as a normal save, but the SOURCE file (if any) is never touched —
//! the archive was opened read-only into `session.temp_dir` (D-03), so
//! writing anywhere other than `session.source_path` cannot mutate it. The
//! session's `target_path` is updated to the new path afterward (D-05).

use crate::archive::manifest::Manifest;
use crate::archive::save::save_archive_to;
use crate::error::ArchiveError;
use crate::session::{ArchiveSession, ManifestMeta, ZipEntryMeta};
use std::path::{Path, PathBuf};

/// Repo-bundled real v16 empty-archive seed. In development this resolves
/// relative to the crate manifest dir; the Tauri resource resolution for a
/// packaged build is out of scope for this plan (bundling config is a
/// packaging-phase concern) — `res_blank_path` mirrors the dev-mode pattern
/// already used by `db::resources::dev_resources_db_path`.
fn dev_res_blank_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../res/blank")
}

/// Builds a brand-new `ArchiveSession` for an empty v16 archive: copies
/// `res/blank`'s `userData.db` (already v16) and `default_thumbnail.png` into
/// a fresh temp dir, then writes a compact `manifest.json` via `Manifest::new`
/// (hash starts empty — `save_archive` fills it in on first save, hash-last).
/// `target_path` is the caller-supplied path the user will save to; the
/// session has no `source_path` distinct from it since nothing was opened
/// from disk.
pub fn new_archive(
    target_path: &Path,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<ArchiveSession, ArchiveError> {
    new_archive_from_seed(
        &dev_res_blank_path(),
        target_path,
        app_name,
        device_name,
        now_iso8601,
    )
}

/// Testable core: same as `new_archive` but takes an explicit seed path so
/// tests can point at a synthetic/known-good `res/blank` copy if desired
/// (in practice tests use the real repo `res/blank` via the same resolution).
pub fn new_archive_from_seed(
    seed_path: &Path,
    target_path: &Path,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<ArchiveSession, ArchiveError> {
    let temp_dir = tempfile::TempDir::new()?;

    let seed_file = std::fs::File::open(seed_path)?;
    let mut seed_zip = zip::ZipArchive::new(seed_file).map_err(|_| ArchiveError::NotAZip)?;
    let mut entries = Vec::new();
    for name in ["userData.db", "default_thumbnail.png"] {
        let mut entry = seed_zip
            .by_name(name)
            .map_err(|_| ArchiveError::MissingUserDataBackup)?;
        let mut buf = Vec::new();
        std::io::copy(&mut entry, &mut buf)?;
        std::fs::write(temp_dir.path().join(name), &buf)?;
        entries.push(ZipEntryMeta {
            name: name.to_string(),
        });
    }

    let db_path = temp_dir.path().join("userData.db");
    let pragma_version: i64 = {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))?
    };

    let manifest = Manifest::new(app_name, device_name, now_iso8601);
    let manifest_bytes = manifest.to_compact_string()?.into_bytes();
    std::fs::write(temp_dir.path().join("manifest.json"), &manifest_bytes)?;
    entries.push(ZipEntryMeta {
        name: "manifest.json".to_string(),
    });

    Ok(ArchiveSession {
        source_path: target_path.to_path_buf(),
        target_path: target_path.to_path_buf(),
        db_path,
        manifest: ManifestMeta {
            name: manifest.name.clone(),
            schema_version: pragma_version,
        },
        entries,
        dirty: true,
        temp_dir,
    })
}

/// Save-as: writes `session` to `new_target_path` via the standard atomic
/// full-inventory save, leaves `session.source_path`'s on-disk bytes
/// untouched (the source was only ever read into `session.temp_dir`, D-03),
/// and returns the fresh `Manifest`. The caller is responsible for updating
/// the LIVE `ArchiveSession` in managed state (`target_path` = new path,
/// `dirty` = false) once this returns `Ok` — this function takes `&self` by
/// shared reference and does not mutate the session in place.
pub fn save_as(
    session: &ArchiveSession,
    new_target_path: &Path,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<Manifest, ArchiveError> {
    save_archive_to(session, new_target_path, app_name, device_name, now_iso8601)
}
