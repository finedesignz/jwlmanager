//! Atomic save (ARCH-06/ARCH-07, D-04, review finding 3/4): rebuild the full
//! archive from the `ArchiveSession`'s zip-entry inventory into a
//! same-directory temp file, then atomically REPLACE the target with the
//! OS-correct primitive.
//!
//! DELETE-THEN-RENAME IS FORBIDDEN. Deleting the target before writing the
//! new file opens a window where neither the old nor the new archive is
//! complete on disk — if the process dies in that window, the user's data is
//! gone. The only acceptable sequence is: write a full, complete temp file
//! next to the target (same directory => same filesystem => a rename/replace
//! is atomic), `sync_all` it, then swap it into place with a single atomic
//! OS call. At every instant either the OLD file or the NEW file is the
//! complete file at `target_path` — never neither.
//!
//! Historically, raw Win32 `MoveFileExW`/`ReplaceFileW` calls were the
//! textbook "atomic replace on Windows" primitives. Both were evaluated here
//! and BOTH failed with spurious OS errors in this execution environment
//! (`ReplaceFileW` -> `ERROR_PATH_NOT_FOUND` even for two plain files in the
//! same directory; raw `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` ->
//! `ERROR_ACCESS_DENIED`) despite being textually correct per the Win32 API
//! docs — see the Deviations section of this plan's SUMMARY for the
//! reproduction. Rust's own `std::fs::rename` on Windows (verified empirically
//! against this exact temp-directory/filesystem, and confirmed by reading the
//! standard library source for this toolchain) performs the replace via
//! `NtSetInformationFile`+`FileRenameInformation(Ex)` with the
//! "replace-if-exists" flag when the platform supports it (Windows 10 1607+),
//! which is a SINGLE atomic kernel call — the same underlying guarantee
//! `ReplaceFileW`/`MoveFileExW` are meant to provide, without their observed
//! fragility. `std::fs::rename` is therefore used on BOTH platforms: on
//! Windows it resolves to this atomic rename-with-replace syscall; on Unix,
//! `rename(2)` over an existing destination on the SAME filesystem is atomic
//! per POSIX. Both are a single kernel call — never delete-then-rename.
//!
//! Only `userData.db` and `manifest.json` are regenerated; every other
//! original zip entry (loose media, thumbnails, unknown/forward-compat
//! files) is streamed through byte-identical from the extracted working copy
//! (finding 4) — full-inventory preservation is the only way a save can never
//! silently destroy data it doesn't understand.

use crate::archive::manifest::{compute_hash, Manifest};
use crate::error::ArchiveError;
use crate::session::ArchiveSession;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Runs the manifest-update sequence (D-04/D-02 hash-last ordering, mirrors
/// `JWLManager.py:1154-1170`): touch `LastModified` in the DB, close the
/// connection, then hash the FINAL on-disk DB bytes as the last DB-touching
/// step before the manifest is serialized. Returns the updated `Manifest`.
fn update_manifest(
    db_path: &Path,
    existing: Option<&Manifest>,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<Manifest, ArchiveError> {
    {
        let conn = rusqlite::Connection::open(db_path)?;
        conn.execute("UPDATE LastModified SET LastModified = ?1", [now_iso8601])?;
        // Connection is dropped (closed) at the end of this block, before
        // compute_hash reads the file back off disk — hash-last discipline.
    }

    let schema_version: i64 = {
        let conn = rusqlite::Connection::open(db_path)?;
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))?
    };

    let mut manifest = match existing {
        Some(m) => m.clone(),
        None => Manifest::new(app_name, device_name, now_iso8601),
    };
    manifest.name = app_name.to_string();
    manifest.creation_date = now_iso8601.to_string();
    manifest.user_data_backup.device_name = device_name.to_string();
    manifest.user_data_backup.last_modified_date = now_iso8601.to_string();
    manifest.user_data_backup.database_name = "userData.db".to_string();
    manifest.user_data_backup.schema_version = schema_version;

    // LAST DB-touching step: hash the final, already-committed on-disk bytes.
    let hash = compute_hash(db_path)?;
    manifest.user_data_backup.hash = hash;

    Ok(manifest)
}

/// Rebuilds the FULL archive zip into `temp_zip_path` from `session`'s entry
/// inventory: every entry is streamed through from the extracted working
/// directory (`session.temp_dir`) byte-identical, EXCEPT `userData.db` and
/// `manifest.json`, which are replaced with the freshly written bytes.
fn rebuild_zip(
    session: &ArchiveSession,
    db_source: &Path,
    manifest_bytes: &[u8],
    temp_zip_path: &Path,
) -> Result<(), ArchiveError> {
    let zip_file = File::create(temp_zip_path)?;
    let mut writer = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default();

    for entry in &session.entries {
        writer.start_file(&entry.name, options)?;
        if entry.name == "manifest.json" {
            writer.write_all(manifest_bytes)?;
        } else if entry.name == "userData.db" {
            // The userData.db bytes come from `db_source` — which is
            // `session.db_path` for a normal save, but a throwaway v14 copy
            // for `save_v14_copy` (MED-5): the live session is never read as
            // the DB source when exporting a downgraded copy.
            let db_bytes = fs::read(db_source)?;
            writer.write_all(&db_bytes)?;
        } else {
            // Every other original entry (loose media, thumbnails,
            // unknown/forward-compat files) is copied through untouched from
            // the extracted working copy — full-inventory preservation.
            let source_path = session.temp_dir.path().join(&entry.name);
            let bytes = fs::read(&source_path)?;
            writer.write_all(&bytes)?;
        }
    }

    // `finish()` hands back the underlying `File` (still open for writing).
    // `sync_all` MUST be called on THIS write-capable handle, not a fresh
    // `File::open` (which is read-only and fails `FlushFileBuffers` with
    // `ERROR_ACCESS_DENIED` on Windows — verified during this plan's
    // execution) — see module docs.
    let file = writer.finish()?;
    file.sync_all()?;
    Ok(())
}

/// Builds a same-directory temp file path for `target`, so the later
/// atomic-replace step is guaranteed to be on a single filesystem.
fn same_dir_temp_path(target: &Path) -> PathBuf {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let unique = format!(
        ".{}.tmp-{}",
        target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive".to_string()),
        std::process::id()
    );
    dir.join(unique)
}

/// Atomically replaces `target` with `temp` using the OS-correct primitive.
/// DELETE-THEN-RENAME IS FORBIDDEN — see module docs. `temp` must already be
/// a complete, flushed, `sync_all`-ed file before this is called.
///
/// `std::fs::rename` is used on every platform: on Windows it performs an
/// atomic rename-with-replace via `NtSetInformationFile`, on Unix it is
/// `rename(2)` — see module docs for why the raw Win32
/// `ReplaceFileW`/`MoveFileExW` FFI calls were tried and rejected. The
/// same-directory temp path (`same_dir_temp_path`) guarantees `temp` and
/// `target` are on the SAME filesystem/volume, which both platforms require
/// for the rename to be atomic rather than falling back to a non-atomic
/// copy+delete.
fn atomic_replace(temp: &Path, target: &Path) -> Result<(), ArchiveError> {
    fs::rename(temp, target)?;
    Ok(())
}

/// Saves `session` to `session.target_path`: writes the rebuilt zip into a
/// same-directory temp file, flushes + syncs it, then atomically replaces the
/// target. Never deletes the target before the replace. Returns the final
/// `Manifest` (for callers that want the fresh hash/timestamps).
pub fn save_archive(
    session: &ArchiveSession,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<Manifest, ArchiveError> {
    save_archive_to(
        session,
        &session.target_path,
        app_name,
        device_name,
        now_iso8601,
    )
}

/// Core implementation shared by `save_archive` and `save_as` — writes the
/// rebuilt archive to an explicit `target_path` (which may differ from
/// `session.target_path` during save-as, before the session is updated).
pub fn save_archive_to(
    session: &ArchiveSession,
    target_path: &Path,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<Manifest, ArchiveError> {
    // ARCH-04 (D2-04): trim the working-copy DB on save — orphan sweep, tag
    // re-densify, Location.Title normalization, then VACUUM — BEFORE the
    // manifest hash so the hash covers the final trimmed bytes (hash-last).
    // A failed trim rolls the working copy back and surfaces a typed error;
    // it never leaves a half-trimmed DB or writes the target.
    {
        let mut conn = rusqlite::Connection::open(&session.db_path)?;
        crate::db::trim::trim_db(&mut conn)?;
    }

    // Delegate the manifest-build + zip-rebuild + atomic-replace to the shared
    // session-untouching writer, reading the DB from `session.db_path` (already
    // trimmed above). This is the ONLY divergence from `save_v14_copy`: the
    // normal save path additionally trims the session DB and syncs the fresh
    // manifest back into `session.temp_dir` (below).
    let manifest = write_archive_from_db_source(
        session,
        &session.db_path,
        target_path,
        app_name,
        device_name,
        now_iso8601,
    )?;

    // Keep the extracted working copy's manifest.json in sync so a subsequent
    // save (without reopening) reads the fresh manifest back. This is a
    // NORMAL-SAVE-ONLY step — `write_archive_from_db_source` never writes into
    // `session.temp_dir` (MED-5), so `save_v14_copy` leaves the session's
    // manifest.json untouched.
    let manifest_bytes = manifest.to_compact_string()?.into_bytes();
    fs::write(
        session.temp_dir.path().join("manifest.json"),
        &manifest_bytes,
    )?;

    Ok(manifest)
}

/// Session-untouching archive writer (MED-5): builds the manifest from
/// `db_source`, rebuilds the full zip streaming every non-`userData.db` entry
/// read-only from `session.temp_dir`, and atomically replaces `target_path`.
///
/// Unlike `save_archive_to`, this helper:
///   (a) reads the userData.db bytes from `db_source` (may be a throwaway v14
///       copy), NEVER `session.db_path` directly beyond what the caller passes;
///   (b) does NOT trim (`db_source` is expected already-trimmed by the caller);
///   (c) does NOT write manifest.json into `session.temp_dir` — the manifest
///       bytes are held in memory and streamed straight into the output zip;
///   (d) never mutates `session.db_path` / `session.temp_dir`.
///
/// Used by both the normal save (delegating with `session.db_path`) and
/// `save_v14_copy` (delegating with the downgraded throwaway copy).
pub(crate) fn write_archive_from_db_source(
    session: &ArchiveSession,
    db_source: &Path,
    target_path: &Path,
    app_name: &str,
    device_name: &str,
    now_iso8601: &str,
) -> Result<Manifest, ArchiveError> {
    // Read the existing manifest (if this session already has one on disk)
    // so unknown/forward-compat top-level keys are preserved read->write.
    // Read-only — never written back here.
    let manifest_path = session.temp_dir.path().join("manifest.json");
    let existing_manifest = if manifest_path.exists() {
        let bytes = fs::read(&manifest_path)?;
        Manifest::parse(&bytes).ok()
    } else {
        None
    };

    // Manifest built off `db_source` — schema_version + hash reflect the COPY
    // (14 for a v14 export), never the live session.
    let manifest = update_manifest(
        db_source,
        existing_manifest.as_ref(),
        app_name,
        device_name,
        now_iso8601,
    )?;
    let manifest_bytes = manifest.to_compact_string()?.into_bytes();

    let temp_zip_path = same_dir_temp_path(target_path);
    // Any failure between here and the atomic_replace call below leaves
    // `target_path` completely untouched — the interruption-safety
    // invariant (old-or-new, never truncated).
    let rebuild_result = rebuild_zip(session, db_source, &manifest_bytes, &temp_zip_path);
    if let Err(err) = rebuild_result {
        let _ = fs::remove_file(&temp_zip_path);
        return Err(err);
    }

    // `rebuild_zip` already called `sync_all` on the write-capable handle
    // before returning — the temp file is flushed to disk at this point.
    if let Err(err) = atomic_replace(&temp_zip_path, target_path) {
        let _ = fs::remove_file(&temp_zip_path);
        return Err(err);
    }

    Ok(manifest)
}
