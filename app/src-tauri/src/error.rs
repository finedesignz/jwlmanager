//! Two-layer error surface (D-14, SAFE-05, finding 6 in 01-CONTEXT.md).
//!
//! `ArchiveError` is the internal, rich error type: it wraps `std::io::Error`
//! / `rusqlite::Error` / `zip::result::ZipError` / `serde_json::Error` via
//! `thiserror`'s `#[from]` and stays entirely inside the Rust core. It does
//! NOT derive `Serialize` (its sources don't either) and must never cross the
//! Tauri IPC boundary directly.
//!
//! `ErrorDto` is the ONLY error type that crosses IPC: `Serialize`-able,
//! carrying a stable `code`, the `operation` that failed, an optional
//! `safe_file_name` (base name only — never an absolute path), and a
//! `message_key` the frontend maps to a translated, actionable message. It
//! never includes the wrapped source error's `Display` output (which can leak
//! filesystem layout or SQL fragments).

use serde::Serialize;
use std::path::Path;
use thiserror::Error;
use ts_rs::TS;

/// Internal, rich error type. Intentionally NOT `Serialize` — see module docs.
#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("selected file is not a valid zip archive")]
    NotAZip,
    #[error("archive is missing manifest.json")]
    MissingManifest,
    #[error("archive is missing its userData.db backup")]
    MissingUserDataBackup,
    #[error("schema version {version} is too old to open (minimum supported: 12)")]
    SchemaTooOld { version: i64 },
    #[error("schema version {version} is newer than this app supports")]
    SchemaTooNew { version: i64 },
    #[error("schema upgrade failed: {reason}")]
    SchemaUpgradeFailed { reason: String },
    #[error("schema downgrade failed: {reason}")]
    SchemaDowngradeFailed { reason: String },
    #[error("orphan sweep / trim failed: {reason}")]
    TrimFailed { reason: String },
    #[error("note delete failed: {reason}")]
    DeleteFailed { reason: String },
    #[error("favorite operation failed: {reason}")]
    FavoriteFailed { reason: String },
    #[error("favorite already exists for edition {edition} in language {language}")]
    FavoriteDuplicate { language: String, edition: String },
    #[error("color change failed: {reason}")]
    ColorFailed { reason: String },
    #[error("tag edit failed: {reason}")]
    TagFailed { reason: String },
    #[error("tag reorder failed: {reason}")]
    ReorderFailed { reason: String },
    #[error("jwlCore merge engine is unavailable on this platform")]
    MergeUnavailable,
    #[error("archive merge failed: {reason}")]
    MergeFailed { reason: String },
    #[error("archive entry rejected: possible path traversal (zip-slip)")]
    ZipSlipRejected,
    #[error("session state lock was poisoned")]
    StatePoisoned,
    #[error("bundled resources.db is missing the UI language row")]
    MissingResourcesLanguage,
    #[error("could not resolve a path for the bundled resources.db")]
    MissingResourcesDb,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Internal jwlCore load-path error (01-03, D-12/D-13). Reserved for
/// genuinely unexpected load faults — the arm64-windows "no binary" and
/// other unsupported-platform cases are NOT represented here; they surface
/// as a non-loaded `JwlCoreStatus` (Ok, not Err) at the command boundary
/// per finding 12.
#[derive(Debug, Error, Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/bindings/JwlCoreError.ts")]
pub enum JwlCoreError {
    #[error("failed to load jwlCore library: {reason}")]
    LoadFailed { reason: String },
    #[error("jwlCore library is missing an expected symbol: {symbol}")]
    MissingSymbol { symbol: String },
    #[error("could not resolve a path for the jwlCore library")]
    PathResolutionFailed,
}

/// Sanitized, `Serialize`-able error that crosses the Tauri IPC boundary.
/// Never includes a raw absolute path or the wrapped source error's Display.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/ErrorDto.ts")]
pub struct ErrorDto {
    pub code: String,
    pub operation: String,
    pub safe_file_name: Option<String>,
    pub message_key: String,
}

impl ArchiveError {
    /// Maps this internal error to the sanitized boundary DTO. `file`, if
    /// given, contributes only its base file name — never the full path.
    pub fn to_dto(&self, operation: &str, file: Option<&Path>) -> ErrorDto {
        let safe_file_name = file
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let (code, message_key) = match self {
            ArchiveError::NotAZip => ("not_a_zip", "error.archive.not_a_zip"),
            ArchiveError::MissingManifest => ("missing_manifest", "error.archive.missing_manifest"),
            ArchiveError::MissingUserDataBackup => (
                "missing_user_data_backup",
                "error.archive.missing_user_data_backup",
            ),
            ArchiveError::SchemaTooOld { .. } => ("schema_too_old", "error.archive.schema_too_old"),
            ArchiveError::SchemaTooNew { .. } => ("schema_too_new", "error.archive.schema_too_new"),
            // `reason` is an internal detail and MUST NOT leak into the DTO
            // (module docs above) — only the stable code + message_key cross
            // IPC; the frontend copy is generic ("could not be completed").
            ArchiveError::SchemaUpgradeFailed { .. } => (
                "schema_upgrade_failed",
                "error.archive.schema_upgrade_failed",
            ),
            // `reason` is an internal detail and MUST NOT leak into the DTO
            // (module docs above) — only the stable code + message_key cross
            // IPC; the frontend copy is generic (some archives cannot be
            // downgraded to the older format).
            ArchiveError::SchemaDowngradeFailed { .. } => (
                "schema_downgrade_failed",
                "error.archive.schema_downgrade_failed",
            ),
            // `reason` is internal-only (module docs) — the DTO exposes only
            // the stable code + message_key; the frontend copy is generic.
            ArchiveError::TrimFailed { .. } => ("trim_failed", "error.archive.trim_failed"),
            // `reason` is internal-only (module docs) — the DTO exposes only
            // the stable code + message_key; the frontend copy is generic.
            ArchiveError::DeleteFailed { .. } => ("delete_failed", "error.archive.delete_failed"),
            // `reason` is internal-only (module docs) — the DTO exposes only
            // the stable code + message_key; the frontend copy is generic.
            ArchiveError::FavoriteFailed { .. } => {
                ("favorite_failed", "error.archive.favorite_failed")
            }
            // `language`/`edition` are internal-only (module docs) — the DTO
            // exposes only the stable code + message_key. The caller already
            // knows which language it asked about (it is the one that picked
            // it), so the frontend supplies the `{Language}` interpolation
            // itself rather than round-tripping it through this DTO.
            ArchiveError::FavoriteDuplicate { .. } => {
                ("favorite_duplicate", "error.archive.favorite_duplicate")
            }
            // `reason` is internal-only (module docs) — the DTO exposes only
            // the stable code + message_key; the frontend copy is generic.
            ArchiveError::ColorFailed { .. } => ("color_failed", "error.archive.color_failed"),
            // `reason` is internal-only (module docs) — the DTO exposes only
            // the stable code + message_key; the frontend copy is generic.
            ArchiveError::TagFailed { .. } => ("tag_failed", "error.archive.tag_failed"),
            // `reason` is internal-only (module docs) — the DTO exposes only
            // the stable code + message_key; the frontend copy is generic.
            ArchiveError::ReorderFailed { .. } => {
                ("reorder_failed", "error.archive.reorder_failed")
            }
            // A missing/wrong-arch jwlCore binary degrades to a typed error
            // (never the Python `crash_box + sys.exit()` defect) — the DTO
            // exposes only the stable code + generic message_key.
            ArchiveError::MergeUnavailable => ("merge_unavailable", "error.merge.unavailable"),
            // `reason` carries getLastResult() detail (internal path/SQL
            // fragments) and MUST NOT leak into the DTO (module docs / D-14) —
            // the DTO exposes only the stable code + generic message_key.
            ArchiveError::MergeFailed { .. } => ("merge_failed", "error.merge.failed"),
            ArchiveError::ZipSlipRejected => {
                ("zip_slip_rejected", "error.archive.zip_slip_rejected")
            }
            ArchiveError::StatePoisoned => ("state_poisoned", "error.internal.state_poisoned"),
            ArchiveError::MissingResourcesLanguage => (
                "missing_resources_language",
                "error.archive.missing_resources_language",
            ),
            ArchiveError::MissingResourcesDb => {
                ("missing_resources_db", "error.archive.missing_resources_db")
            }
            ArchiveError::Io(_) => ("io_error", "error.archive.io_error"),
            ArchiveError::Sqlite(_) => ("sqlite_error", "error.archive.sqlite_error"),
            ArchiveError::Zip(_) => ("zip_error", "error.archive.zip_error"),
            ArchiveError::Json(_) => ("json_error", "error.archive.json_error"),
        };
        ErrorDto {
            code: code.to_string(),
            operation: operation.to_string(),
            safe_file_name,
            message_key: message_key.to_string(),
        }
    }
}
