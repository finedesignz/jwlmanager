//! Byte-compatible `manifest.json` (ARCH-03). Mirrors `JWLManager.py:979-991`
//! (`new_file`'s manifest shape), `:1152-1170` (`update_manifest`, hash-last
//! ordering), and `:994-1008` (`check_validity`).
//!
//! Field order is an ordered `struct` (never `HashMap`/loose
//! `serde_json::Value`) so `serde_json::to_string` reproduces the exact
//! Python dict-literal key order. Serialization is compact — no whitespace —
//! matching Python's `separators=(',', ':')`. Unknown top-level and
//! `userDataBackup` keys are preserved via `#[serde(flatten)]` catch-all maps
//! (crate `serde_json` `preserve_order` feature keeps them in read order
//! rather than re-sorting alphabetically).
//!
//! Schema gate accepts the 12-16 range (SCHEMA-01/02, 03-02-PLAN.md,
//! finding 3), sharing `archive::{MIN,MAX,WORKING}_SUPPORTED_SCHEMA_VERSION`
//! with `archive/mod.rs`'s independent gate so the two CANNOT drift out of
//! lockstep. This mirrors the legacy Python `schemaVersion > 11` acceptance
//! in `JWLManager.py:994-1008` / FUNCTIONALITY-SPEC.md §2.3 for the lower
//! bound, plus an explicit upper bound the Python original didn't have.

use crate::archive::{
    MAX_SUPPORTED_SCHEMA_VERSION, MIN_SUPPORTED_SCHEMA_VERSION, WORKING_SCHEMA_VERSION,
};
use crate::error::ArchiveError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Top-level `manifest.json` shape. Field declaration order is load-bearing —
/// it is what makes `serde_json::to_string` emit Python's exact key order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub name: String,
    #[serde(rename = "creationDate")]
    pub creation_date: String,
    pub version: i64,
    #[serde(rename = "type")]
    pub archive_type: i64,
    #[serde(rename = "userDataBackup")]
    pub user_data_backup: UserDataBackup,
    /// Unknown top-level keys, preserved read->write (forward compat).
    /// Always empty for a freshly-constructed manifest (`Manifest::new`).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `userDataBackup` nested object shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserDataBackup {
    #[serde(rename = "lastModifiedDate")]
    pub last_modified_date: String,
    #[serde(rename = "deviceName")]
    pub device_name: String,
    #[serde(rename = "databaseName")]
    pub database_name: String,
    pub hash: String,
    /// Strictly typed as `i64` — a JSON string/bool/float here is a *parse
    /// error*, not a silently-coerced value (RESEARCH.md Security Domain:
    /// type-confusion tampering pattern; mirrors PATTERNS.md guidance to
    /// avoid `.get(...).unwrap_or(0)`-style loose reads).
    #[serde(rename = "schemaVersion")]
    pub schema_version: i64,
    /// Unknown `userDataBackup` keys, preserved read->write.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Manifest {
    /// Builds a fresh manifest for a brand-new archive, matching
    /// `JWLManager.py:979-989` (`new_file`). `hash` starts empty — it is only
    /// ever populated by `compute_hash` as the LAST step before save.
    pub fn new(name: &str, device_name: &str, now_iso8601: &str) -> Self {
        Manifest {
            name: name.to_string(),
            creation_date: now_iso8601.to_string(),
            version: 1,
            archive_type: 0,
            user_data_backup: UserDataBackup {
                last_modified_date: now_iso8601.to_string(),
                device_name: device_name.to_string(),
                database_name: "userData.db".to_string(),
                hash: String::new(),
                schema_version: WORKING_SCHEMA_VERSION,
                extra: serde_json::Map::new(),
            },
            extra: serde_json::Map::new(),
        }
    }

    /// Strictly parses `manifest.json` bytes into a `Manifest`. A
    /// type-confused field (e.g. `schemaVersion` as a JSON string) fails here
    /// as a `serde_json` error rather than being silently coerced.
    pub fn parse(bytes: &[u8]) -> Result<Manifest, ArchiveError> {
        Ok(serde_json::from_slice::<Manifest>(bytes)?)
    }

    /// Serializes compactly — no whitespace — matching Python's
    /// `json.dump(m, f, indent=None, separators=(',', ':'))`.
    pub fn to_compact_string(&self) -> Result<String, ArchiveError> {
        Ok(serde_json::to_string(self)?)
    }

    /// 12-16 range acceptance gate (widened from Phase 1's v16-ONLY narrowing
    /// of `check_validity`, `JWLManager.py:994-1008`; SCHEMA-01/02,
    /// 03-02-PLAN.md finding 3). Returns `Ok(())` iff
    /// `MIN_SUPPORTED_SCHEMA_VERSION <= userDataBackup.schemaVersion <=
    /// MAX_SUPPORTED_SCHEMA_VERSION`; below that is
    /// `ArchiveError::SchemaTooOld`, above it `ArchiveError::SchemaTooNew`.
    /// This gate does NOT perform the upgrade or in-range normalization
    /// itself — that lives in `archive::mod::open_and_validate`, which is
    /// the one place a manifest/PRAGMA mismatch is resolved (finding 4).
    pub fn check_schema_gate(&self) -> Result<(), ArchiveError> {
        let version = self.user_data_backup.schema_version;
        if version < MIN_SUPPORTED_SCHEMA_VERSION {
            return Err(ArchiveError::SchemaTooOld { version });
        }
        if version > MAX_SUPPORTED_SCHEMA_VERSION {
            return Err(ArchiveError::SchemaTooNew { version });
        }
        Ok(())
    }
}

/// End-to-end validity check over raw `manifest.json` bytes: strictly parses,
/// then applies the v16-only schema gate. A missing `userDataBackup` key is a
/// `serde_json` "missing field" parse error (surfaced as
/// `ArchiveError::MissingUserDataBackup` here, matching the specific
/// rejection reason `check_validity` gives in the Python original) rather
/// than a generic JSON error, so callers can distinguish "not shaped like a
/// manifest" from "manifest is a genuinely old/unsupported schema".
pub fn check_validity(manifest_bytes: &[u8]) -> Result<Manifest, ArchiveError> {
    // First pass: detect a structurally-present-but-differently-shaped
    // `userDataBackup` (or its absence) with a dedicated error, since a bare
    // `serde_json::Error` on the strict struct parse doesn't distinguish
    // "missing userDataBackup" from any other shape mismatch.
    let probe: serde_json::Value = serde_json::from_slice(manifest_bytes)?;
    if probe.get("userDataBackup").is_none() {
        return Err(ArchiveError::MissingUserDataBackup);
    }

    let manifest = Manifest::parse(manifest_bytes)?;
    manifest.check_schema_gate()?;
    Ok(manifest)
}

/// Computes `sha256(<whole file bytes>).hexdigest()` — must be called as the
/// LAST DB-touching step before serializing the manifest (mirrors
/// `JWLManager.py:1162-1168`: `UPDATE LastModified` -> commit/close -> hash
/// the final on-disk bytes -> write manifest). Any DB write after this call
/// invalidates the archive (FUNCTIONALITY-SPEC.md §3 Pitfall 5).
pub fn compute_hash(db_path: &Path) -> Result<String, ArchiveError> {
    let bytes = fs::read(db_path)?;
    let digest = Sha256::digest(&bytes);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(hex)
}
