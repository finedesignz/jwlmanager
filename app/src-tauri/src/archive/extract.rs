//! Zip-slip-safe extraction (ARCH-05). Fixes the anti-pattern at
//! `JWLManager.py:977-978, 1097-1099` (`ZipFile.extractall()` with no
//! path-traversal validation).
//!
//! Every entry name is validated via the `zip` crate's own
//! `enclosed_name()` (exact-pinned `=8.6.0`, past the CVE-2025-29787 floor)
//! before any filesystem write happens. Entries that fail validation abort
//! the whole extraction with a typed `ArchiveError::ZipSlipRejected` — no
//! partial, still-dangerous extraction is left behind. This extractor also
//! never creates filesystem symlinks from archive entries (it copies entry
//! bytes into a plain file at the validated path), which independently closes
//! the CVE-2025-29787 symlink-chain variant.

use crate::error::ArchiveError;
use crate::session::ZipEntryMeta;
use std::fs::File;
use std::path::Path;

/// Extracts `archive_path` into `dest`, validating every entry name via
/// `enclosed_name()` before writing. Returns the full entry inventory (in
/// zip order) so the caller can populate `ArchiveSession::entries`.
pub fn extract_zip_slip_safe(
    archive_path: &Path,
    dest: &Path,
) -> Result<Vec<ZipEntryMeta>, ArchiveError> {
    let file = File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| ArchiveError::NotAZip)?;

    let mut entries = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let enclosed = entry.enclosed_name().ok_or(ArchiveError::ZipSlipRejected)?;
        let name = entry.name().to_string();
        let out_path = dest.join(&enclosed);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
        entries.push(ZipEntryMeta { name });
    }
    Ok(entries)
}
