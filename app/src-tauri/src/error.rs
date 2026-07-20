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
    #[error("unsupported schema version {version}")]
    UnsupportedSchema { version: i64 },
    #[error("archive entry rejected: possible path traversal (zip-slip)")]
    ZipSlipRejected,
    #[error("session state lock was poisoned")]
    StatePoisoned,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
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
            ArchiveError::UnsupportedSchema { .. } => (
                "unsupported_schema",
                "error.archive.unsupported_schema_phase3",
            ),
            ArchiveError::ZipSlipRejected => {
                ("zip_slip_rejected", "error.archive.zip_slip_rejected")
            }
            ArchiveError::StatePoisoned => ("state_poisoned", "error.internal.state_poisoned"),
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
