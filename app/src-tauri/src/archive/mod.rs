//! Minimal skeleton archive-open orchestration: extract -> v16-ONLY validity
//! gate -> raw Notes query -> `ArchiveSession`.
//!
//! This is intentionally NOT the full byte-compatible manifest handling —
//! that (Manifest struct, strict `check_validity`, hash-last save ordering)
//! lands in 01-02's `archive/manifest.rs`. No file overlap with that plan.
//!
//! Phase-1 schema gate is narrowed to v16 ONLY (finding 2, 01-07-PLAN.md):
//! both the manifest's `schemaVersion` AND the extracted DB's
//! `PRAGMA user_version` must equal 16, or the archive is rejected with a
//! typed `ArchiveError::UnsupportedSchema`. v12-15 acceptance/upgrade is
//! SCHEMA-01/02 in Phase 3 — this deliberately narrows ARCH-01 for Phase 1.

pub mod extract;
pub mod manifest;

use crate::db::notes::{query_notes, NotesRow};
use crate::db::resources::ResourceCatalog;
use crate::error::ArchiveError;
use crate::session::{ArchiveSession, ManifestMeta};
use serde::Deserialize;
use std::path::Path;

/// Fixed UI language for label synthesis (`resources.db` Languages.Code).
/// Phase 1 has no locale switcher (UI-SPEC defers that to Phase 11).
const UI_LANG_CODE: &str = "en";

/// The only schema version Phase 1 accepts. See module docs.
const SUPPORTED_SCHEMA_VERSION: i64 = 16;

#[derive(Debug, Deserialize)]
struct ManifestJson {
    name: String,
    #[serde(rename = "userDataBackup")]
    user_data_backup: UserDataBackup,
}

#[derive(Debug, Deserialize)]
struct UserDataBackup {
    #[serde(rename = "schemaVersion")]
    schema_version: i64,
}

/// Extracts, validates (v16-only gate), and queries `path`, returning both
/// the populated `ArchiveSession` (managed state, later consumed by save)
/// and the fully labeled Notes rows (located + independent) for the
/// frontend's first render. `resources_db_path` is the bundled resources.db
/// used to synthesize human-readable labels (`db::resources`).
pub fn open_and_validate(
    path: &Path,
    resources_db_path: &Path,
) -> Result<(ArchiveSession, Vec<NotesRow>), ArchiveError> {
    let temp_dir = tempfile::TempDir::new()?;
    let entries = extract::extract_zip_slip_safe(path, temp_dir.path())?;

    if !entries.iter().any(|e| e.name == "manifest.json") {
        return Err(ArchiveError::MissingManifest);
    }
    if !entries.iter().any(|e| e.name == "userData.db") {
        return Err(ArchiveError::MissingUserDataBackup);
    }

    let manifest_bytes = std::fs::read(temp_dir.path().join("manifest.json"))?;
    let manifest: ManifestJson = serde_json::from_slice(&manifest_bytes)?;

    // Manifest-declared version alone is enough to reject a non-v16 archive
    // without ever needing to open the (possibly untrusted-shape) database.
    if manifest.user_data_backup.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ArchiveError::UnsupportedSchema {
            version: manifest.user_data_backup.schema_version,
        });
    }

    let db_path = temp_dir.path().join("userData.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    let pragma_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if pragma_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ArchiveError::UnsupportedSchema {
            version: pragma_version,
        });
    }

    let catalog = ResourceCatalog::load(resources_db_path, UI_LANG_CODE)?;
    let notes = query_notes(&conn, &catalog)?;

    let session = ArchiveSession {
        source_path: path.to_path_buf(),
        target_path: path.to_path_buf(),
        db_path,
        manifest: ManifestMeta {
            name: manifest.name,
            schema_version: manifest.user_data_backup.schema_version,
        },
        entries,
        dirty: false,
        temp_dir,
    };

    Ok((session, notes))
}
