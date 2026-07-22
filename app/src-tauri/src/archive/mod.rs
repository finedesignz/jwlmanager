//! Archive-open orchestration: extract -> 12-16 range validity gate ->
//! upgrade-on-open -> post-upgrade v16 contract validation -> raw Notes
//! query -> `ArchiveSession`.
//!
//! This is intentionally NOT the full byte-compatible manifest handling —
//! that (Manifest struct, strict `check_validity`, hash-last save ordering)
//! lands in 01-02's `archive/manifest.rs`. No file overlap with that plan.
//!
//! Schema gate widened to the 12-16 range (SCHEMA-01/02, 03-02-PLAN.md,
//! finding 3): a manifest/PRAGMA version `< MIN_SUPPORTED_SCHEMA_VERSION` is
//! rejected as `ArchiveError::SchemaTooOld`, `> MAX_SUPPORTED_SCHEMA_VERSION`
//! as `ArchiveError::SchemaTooNew`. `archive::manifest`'s independent gate
//! (`check_schema_gate`) shares these SAME constants so the two gates can
//! never drift out of lockstep. An in-range manifest/PRAGMA MISMATCH is
//! NORMALIZED (finding 4) rather than rejected: the DB is upgraded (if
//! below `WORKING_SCHEMA_VERSION`) and `ManifestMeta.schema_version` is set
//! from the FINAL post-upgrade `PRAGMA user_version`, not the manifest's
//! original claim.

pub mod downgrade;
pub mod extract;
pub mod manifest;
pub mod merge;
pub mod new;
pub mod save;
pub mod upgrade;

use crate::db::notes::{query_notes, NotesRow};
use crate::db::resources::ResourceCatalog;
use crate::error::ArchiveError;
use crate::session::{ArchiveSession, ManifestMeta};
use serde::Deserialize;
use std::path::Path;

/// Fixed UI language for label synthesis (`resources.db` Languages.Code).
/// Phase 1 has no locale switcher (UI-SPEC defers that to Phase 11).
const UI_LANG_CODE: &str = "en";

/// Oldest schema version this app accepts (inclusive). Below this ->
/// `ArchiveError::SchemaTooOld`. Single source of truth shared with
/// `archive::manifest`'s independent gate (finding 3, 03-02-PLAN.md).
pub const MIN_SUPPORTED_SCHEMA_VERSION: i64 = 12;

/// Newest schema version this app accepts (inclusive). Above this ->
/// `ArchiveError::SchemaTooNew`. Single source of truth shared with
/// `archive::manifest`'s independent gate (finding 3, 03-02-PLAN.md).
pub const MAX_SUPPORTED_SCHEMA_VERSION: i64 = 16;

/// The schema version every accepted archive is normalized to on open
/// (upgraded if below this). Also what a freshly-created archive starts at.
pub const WORKING_SCHEMA_VERSION: i64 = 16;

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

    // Manifest-declared out-of-range version alone is enough to reject
    // without ever needing to open the (possibly untrusted-shape) database.
    // An IN-RANGE manifest version is not itself sufficient to accept —
    // the PRAGMA check below is the authority; an in-range mismatch between
    // the two is normalized (finding 4), not rejected here.
    let manifest_version = manifest.user_data_backup.schema_version;
    if manifest_version < MIN_SUPPORTED_SCHEMA_VERSION {
        return Err(ArchiveError::SchemaTooOld {
            version: manifest_version,
        });
    }
    if manifest_version > MAX_SUPPORTED_SCHEMA_VERSION {
        return Err(ArchiveError::SchemaTooNew {
            version: manifest_version,
        });
    }

    let db_path = temp_dir.path().join("userData.db");
    let mut conn = rusqlite::Connection::open(&db_path)?;
    let pragma_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if pragma_version < MIN_SUPPORTED_SCHEMA_VERSION {
        return Err(ArchiveError::SchemaTooOld {
            version: pragma_version,
        });
    }
    if pragma_version > MAX_SUPPORTED_SCHEMA_VERSION {
        return Err(ArchiveError::SchemaTooNew {
            version: pragma_version,
        });
    }

    // In-range: upgrade to the working version if below it (finding 4,
    // NORMALIZE — never reject an in-range manifest/PRAGMA mismatch).
    if pragma_version < WORKING_SCHEMA_VERSION {
        upgrade::upgrade_to_v16(&mut conn)?;
    }
    upgrade::validate_v16_contract(&conn)?;

    // Re-read the PRAGMA as the authoritative final version (finding 4): the
    // saved/session manifest's schema_version always reflects the actual
    // on-disk DB state, never the original manifest's possibly-stale claim.
    let final_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    let catalog = ResourceCatalog::load(resources_db_path, UI_LANG_CODE)?;
    let notes = query_notes(&conn, &catalog)?;

    let session = ArchiveSession {
        source_path: path.to_path_buf(),
        target_path: path.to_path_buf(),
        db_path,
        manifest: ManifestMeta {
            name: manifest.name,
            schema_version: final_version,
        },
        entries,
        dirty: false,
        temp_dir,
    };

    Ok((session, notes))
}
