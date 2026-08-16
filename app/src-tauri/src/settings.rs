//! App-level settings persistence (language + theme), D11-04 -- the FIRST
//! code in this app that writes to disk outside an explicit user-initiated
//! archive save (Core Value: never lose or corrupt a user's archive -- this
//! module never touches that path at all). The settings file lives
//! exclusively under Tauri's OS-managed app-data directory, resolved fresh
//! on every call, never derived from or joined with any user-chosen path.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use thiserror::Error;
use ts_rs::TS;

use crate::error::ErrorDto;

/// The settings file's name under the resolved app-data directory.
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// The two supported themes (D11-03). Serialized lowercase so the JSON file
/// and the frontend's `data-theme` attribute value read identically
/// (`"light"`/`"dark"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/Theme.ts")]
pub enum Theme {
    Light,
    Dark,
}

/// The app's ENTIRE persisted shape -- exactly two keys, by design (D11-04
/// scope discipline). Adding a third key (window geometry, last-selected
/// category, sort-column, ...) is explicitly out of scope for this phase --
/// see 11-CONTEXT.md's Phase Boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/AppSettings.ts")]
pub struct AppSettings {
    pub language: String,
    pub theme: Theme,
}

impl Default for AppSettings {
    /// English + Dark (D11-04's fixed default pair) -- what a brand-new
    /// install with no settings file yet gets, and what any degrade-on-
    /// failure path below falls back to.
    fn default() -> Self {
        AppSettings {
            language: "en".to_string(),
            theme: Theme::Dark,
        }
    }
}

/// Internal, typed settings-domain error -- its OWN separate, lower-stakes
/// error domain (D11-04), mirroring `error::ArchiveError`'s thiserror style
/// WITHOUT reusing that type or crossing IPC directly (same two-layer
/// convention `error.rs`'s module docs establish for
/// `ArchiveError`/`ErrorDto`; see [`SettingsError::to_dto`] below).
#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("could not resolve the app-data directory")]
    AppDataDirUnavailable,
    #[error("failed to read the settings file: {reason}")]
    ReadFailed { reason: String },
    #[error("failed to write the settings file: {reason}")]
    WriteFailed { reason: String },
    #[error("failed to parse the settings file: {reason}")]
    ParseFailed { reason: String },
}

impl SettingsError {
    /// Maps to the sanitized boundary DTO -- reuses `ErrorDto`'s SHAPE (the
    /// only type that is ever meant to cross IPC, per `error.rs`'s module
    /// docs), never a raw path or the wrapped source error's `Display`, so a
    /// failed save routes through the app's ALREADY-existing
    /// `ErrorBanner`/`describeError` path rather than inventing a second one
    /// (11-01-PLAN.md Task 1).
    pub fn to_dto(&self, operation: &str) -> ErrorDto {
        let (code, message_key) = match self {
            SettingsError::AppDataDirUnavailable => (
                "settings_app_data_dir_unavailable",
                "error.settings.app_data_dir_unavailable",
            ),
            SettingsError::ReadFailed { .. } => {
                ("settings_read_failed", "error.settings.read_failed")
            }
            SettingsError::WriteFailed { .. } => {
                ("settings_write_failed", "error.settings.write_failed")
            }
            SettingsError::ParseFailed { .. } => {
                ("settings_parse_failed", "error.settings.parse_failed")
            }
        };
        ErrorDto {
            code: code.to_string(),
            operation: operation.to_string(),
            safe_file_name: None,
            message_key: message_key.to_string(),
        }
    }
}

/// Loads persisted settings from the OS-managed app-data directory,
/// absorbing EVERY failure (missing file -- e.g. first launch --, unreadable,
/// corrupt/truncated JSON, or JSON of the wrong shape) into
/// `AppSettings::default()`. Returns a plain `AppSettings`, never a
/// `Result`: a brand-new user with no settings file is not in an error
/// state, and this command MUST NEVER block, delay, or abort app startup,
/// nor surface an error banner or dialog for a missing/unreadable file
/// (Integration Point/risk, D11-04).
#[tauri::command]
pub fn load_settings(app: AppHandle) -> AppSettings {
    let result: Result<AppSettings, SettingsError> = (|| {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|_| SettingsError::AppDataDirUnavailable)?;
        let path = dir.join(SETTINGS_FILE_NAME);
        let raw = std::fs::read_to_string(&path).map_err(|e| SettingsError::ReadFailed {
            reason: e.to_string(),
        })?;
        serde_json::from_str(&raw).map_err(|e| SettingsError::ParseFailed {
            reason: e.to_string(),
        })
    })();
    result.unwrap_or_default()
}

/// Persists `settings` to the OS-managed app-data directory, creating the
/// directory tree if it does not yet exist. UNLIKE [`load_settings`], a
/// failed save DOES return a typed error -- a failed save would otherwise
/// silently discard the user's choice, which the app's Core Value (never
/// lose or corrupt user state) forbids (D11-04).
#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), ErrorDto> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| SettingsError::AppDataDirUnavailable.to_dto("save_settings"))?;
    std::fs::create_dir_all(&dir).map_err(|e| {
        SettingsError::WriteFailed {
            reason: e.to_string(),
        }
        .to_dto("save_settings")
    })?;
    let path = dir.join(SETTINGS_FILE_NAME);
    let raw = serde_json::to_string(&settings).map_err(|e| {
        SettingsError::WriteFailed {
            reason: e.to_string(),
        }
        .to_dto("save_settings")
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        SettingsError::WriteFailed {
            reason: e.to_string(),
        }
        .to_dto("save_settings")
    })
}
