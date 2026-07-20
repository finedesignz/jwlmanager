//! `ArchiveSession` — the durable, per-session managed state (SKELETON.md
//! "Core State Object"). Populated by `open_archive` (and, later, 01-05's
//! `new_archive`), consumed by 01-05's `save_archive` / `save_as`.
//!
//! The `TempDir` is OWNED here so the extracted working copy survives for the
//! whole session — dropping it right after open would make save impossible.
//! `entries` is the FULL inventory of every original zip entry (not just
//! `userData.db`/`manifest.json`) so a later save can round-trip loose media
//! and unknown/forward-compat files untouched (D-03).

use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

/// Parsed manifest metadata relevant to session bookkeeping. The full
/// byte-compatible `Manifest` struct (field order, compact serialization)
/// lives in 01-02's `archive/manifest.rs` — this is intentionally minimal.
#[derive(Debug, Clone)]
pub struct ManifestMeta {
    pub name: String,
    pub schema_version: i64,
}

/// One entry from the original archive's zip directory, recorded during
/// extraction so save (01-05) can rebuild the archive losslessly.
#[derive(Debug, Clone)]
pub struct ZipEntryMeta {
    pub name: String,
}

/// The durable per-session state object. See module docs.
#[derive(Debug)]
pub struct ArchiveSession {
    /// Owns the extracted working-copy directory for the session lifetime.
    pub temp_dir: TempDir,
    /// The original opened file. Read-only; never mutated in place (D-03).
    pub source_path: PathBuf,
    /// Current save target. Starts equal to `source_path`; follows the new
    /// path after save-as (D-05).
    pub target_path: PathBuf,
    /// The extracted `userData.db` inside `temp_dir`.
    pub db_path: PathBuf,
    pub manifest: ManifestMeta,
    /// Full inventory of every original zip entry.
    pub entries: Vec<ZipEntryMeta>,
    /// Unsaved-changes flag.
    pub dirty: bool,
}

/// Tauri managed-state wrapper: `None` before any archive is opened.
pub type SessionState = Mutex<Option<ArchiveSession>>;
