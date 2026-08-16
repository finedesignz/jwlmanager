//! Playlist media add (content-hash dedup, magic-byte gate, staged-DB-then-
//! files commit) and playlist item delete (two-pass media reference
//! counting) — IO-02, 08-06-PLAN.md. This module contains the project's
//! FIRST on-disk file copies and FIRST irreversible on-disk removals
//! (T-08-30/T-08-31/T-08-32/T-08-33/T-08-34).
//!
//! **Media add** ports `add_images`'s `update_db` (`JWLManager.py:3528-3600`):
//! a file whose content hash already exists in `IndependentMedia` is a
//! `Duplicate` and contributes zero rows/copies; a file whose magic bytes
//! match none of the supported raster formats is a typed `Unsupported`
//! rejection (HEIC explicitly, a documented parity gap — no mature
//! pure-Rust decoder); a genuinely new file gets TWO `IndependentMedia` rows
//! (the original + a thumbnail that is a BYTE-FOR-BYTE COPY of the source,
//! never a 250x250 resize — PD-1, since the `image` crate could not be
//! legitimacy-verified, 08-RESEARCH.md's addendum) plus a `PlaylistItem`,
//! its `PlaylistItemIndependentMediaMap` row (`DurationTicks = 40000000`
//! literal), and the playlist Tag's `TagMap` row. [`apply_media_add`] stages
//! every DB write into the caller's transaction and every file copy into a
//! `Vec<PendingCopy>`; [`perform_staged_copies`] performs the copies AFTER
//! the DB half is staged (PD-3) and, on any copy failure, deletes every file
//! it had already written THIS call before returning `Err` — the caller
//! must not commit its transaction on that `Err`, so neither a phantom row
//! nor a half-written batch survives.
//!
//! **Media delete** ports `delete_playlist_items`
//! (`JWLManager.py:3627-3656`): [`delete_playlist_items_db`] performs ALL
//! and ONLY the DB work — reference-counting each media file against the
//! REMAINING (not-selected) playlist items with TWO INDEPENDENT used-sets
//! (`used_thumbs`/`used_files`, D8-07), so a file that is a surviving item's
//! thumbnail AND a deleted item's full media is evaluated by each set
//! separately and never double-counted — then deletes rows in Python's
//! exact table order and RETURNS the list of files whose `IndependentMedia`
//! rows it removed. It performs NO filesystem write of any kind.
//! [`remove_media_files`] is a SEPARATE function that performs the
//! best-effort `std::fs::remove_file` (a missing file is silently ignored,
//! matching Python's bare `except: pass`) — [`dry_run_delete_playlist_items`]
//! calls only [`delete_playlist_items_db`] and discards the returned path
//! list, so it is STRUCTURALLY incapable of reaching [`remove_media_files`]
//! at all (D8-07's "not merely unreached" requirement).

use crate::db::edit::{
    diff_snapshots, snapshot_tables, DryRunReport, MEDIA_DELETE_SNAPSHOT_TABLES,
};
use crate::db::ids::take_id;
use crate::db::playlist_io::NonEmptyPlaylistItemIds;
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use ts_rs::TS;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::MediaAddFailed {
        reason: format!("{context}: {err}"),
    }
}

fn map_delete_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::MediaDeleteFailed {
        reason: format!("{context}: {err}"),
    }
}

// ---------------------------------------------------------------------------
// Magic-byte sniffing (the `puremagic` equivalent — a fixed-length prefix
// table, never a decoder, needs no dependency, PD-1).
// ---------------------------------------------------------------------------

/// The raster formats `add_images` accepts (`JWLManager.py:3522`:
/// `['bmp', 'gif', 'heic', 'jpg', 'jpeg', 'png']`). `Heic` is recognised only
/// so [`media_precheck`] can return the explicit typed rejection naming it —
/// this app never decodes or copies a HEIC file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFormat {
    Bmp,
    Gif,
    Jpeg,
    Png,
    Heic,
}

impl MediaFormat {
    fn mime(self) -> &'static str {
        match self {
            MediaFormat::Bmp => "image/bmp",
            MediaFormat::Gif => "image/gif",
            MediaFormat::Jpeg => "image/jpeg",
            MediaFormat::Png => "image/png",
            MediaFormat::Heic => "image/heic",
        }
    }

    /// The thumbnail's fresh-GUID-named file extension (`JWLManager.py:3577`:
    /// `f'{unique_id}.{ext}'`).
    fn extension(self) -> &'static str {
        match self {
            MediaFormat::Bmp => "bmp",
            MediaFormat::Gif => "gif",
            MediaFormat::Jpeg => "jpg",
            MediaFormat::Png => "png",
            MediaFormat::Heic => "heic",
        }
    }

    fn label(self) -> &'static str {
        match self {
            MediaFormat::Bmp => "BMP",
            MediaFormat::Gif => "GIF",
            MediaFormat::Jpeg => "JPEG",
            MediaFormat::Png => "PNG",
            MediaFormat::Heic => "HEIC",
        }
    }
}

/// Sniffs `bytes`' leading magic signature against the five formats
/// [`MediaFormat`] recognises. A fixed-length prefix comparison, not a
/// decoder — needs no dependency (PD-1, T-08-SC).
pub fn sniff_format(bytes: &[u8]) -> Option<MediaFormat> {
    if bytes.starts_with(b"BM") {
        return Some(MediaFormat::Bmp);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(MediaFormat::Gif);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(MediaFormat::Jpeg);
    }
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(MediaFormat::Png);
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        const HEIC_BRANDS: [&[u8]; 9] = [
            b"heic", b"heix", b"hevc", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"msf1",
        ];
        if HEIC_BRANDS.contains(&&bytes[8..12]) {
            return Some(MediaFormat::Heic);
        }
    }
    None
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Pre-check (no writes of any kind — the dialog's confirm surface).
// ---------------------------------------------------------------------------

/// A [`media_precheck`] classification for one selected file.
#[derive(Debug, Clone)]
pub enum MediaClassification {
    New {
        hash: String,
        mime: String,
        format: MediaFormat,
    },
    Duplicate {
        existing_media_id: i64,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct MediaPrecheck {
    pub path: PathBuf,
    pub classification: MediaClassification,
}

/// The Tauri-facing DTO for one [`MediaPrecheck`] row — `status` is one of
/// `"new"` / `"duplicate"` / `"unsupported"`, `reason` is set only for
/// `"unsupported"` (the UI-SPEC's per-file rejection text).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/MediaPrecheckResult.ts")]
pub struct MediaPrecheckResult {
    pub path: String,
    pub status: String,
    pub reason: Option<String>,
}

impl MediaPrecheck {
    pub fn to_dto(&self) -> MediaPrecheckResult {
        let path = self.path.to_string_lossy().into_owned();
        match &self.classification {
            MediaClassification::New { .. } => MediaPrecheckResult {
                path,
                status: "new".to_string(),
                reason: None,
            },
            MediaClassification::Duplicate { .. } => MediaPrecheckResult {
                path,
                status: "duplicate".to_string(),
                reason: None,
            },
            MediaClassification::Unsupported { reason } => MediaPrecheckResult {
                path,
                status: "unsupported".to_string(),
                reason: Some(reason.clone()),
            },
        }
    }
}

/// Classifies every `paths` entry as `New` / `Duplicate` / `Unsupported`
/// against a SINGLE preload of `IndependentMedia`'s existing hashes
/// (`current_hashes`, `JWLManager.py:3558-3560`). Performs NO writes of any
/// kind — this is what [`crate::db::media`]'s `media_add_precheck` command
/// renders as the confirm surface (D8-06, UI-SPEC).
pub fn media_precheck(
    conn: &Connection,
    paths: &[PathBuf],
) -> Result<Vec<MediaPrecheck>, ArchiveError> {
    let existing: HashMap<String, i64> = {
        let mut stmt = conn
            .prepare("SELECT Hash, IndependentMediaId FROM IndependentMedia")
            .map_err(|e| map_sqlite_err(e, "media_precheck: prepare"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| map_sqlite_err(e, "media_precheck: query"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| map_sqlite_err(e, "media_precheck: read rows"))?
    };

    let mut results = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(err) => {
                results.push(MediaPrecheck {
                    path: path.clone(),
                    classification: MediaClassification::Unsupported {
                        reason: format!("could not read file: {err}"),
                    },
                });
                continue;
            }
        };

        let Some(format) = sniff_format(&bytes) else {
            results.push(MediaPrecheck {
                path: path.clone(),
                classification: MediaClassification::Unsupported {
                    reason: "unreadable — not a supported image".to_string(),
                },
            });
            continue;
        };

        if format == MediaFormat::Heic {
            results.push(MediaPrecheck {
                path: path.clone(),
                classification: MediaClassification::Unsupported {
                    reason: format!(
                        "{} is not a supported format (no mature pure-Rust decoder)",
                        format.label()
                    ),
                },
            });
            continue;
        }

        let hash = sha256_hex(&bytes);
        if let Some(&existing_media_id) = existing.get(&hash) {
            results.push(MediaPrecheck {
                path: path.clone(),
                classification: MediaClassification::Duplicate { existing_media_id },
            });
        } else {
            results.push(MediaPrecheck {
                path: path.clone(),
                classification: MediaClassification::New {
                    hash,
                    mime: format.mime().to_string(),
                    format,
                },
            });
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Apply — staged DB writes, then staged file copies (PD-3).
// ---------------------------------------------------------------------------

/// One file copy `apply_media_add` deferred — `dst_name` is the name already
/// disambiguated/resolved against the archive's media directory.
#[derive(Debug, Clone)]
pub struct PendingCopy {
    pub src: PathBuf,
    pub dst_name: String,
}

/// Underscore disambiguation for storage FILENAMEs (`check_name`,
/// `JWLManager.py:3530-3536`): `name`, `name_1`, `name_2`, ... A DISTINCT
/// scheme from [`disambiguate_label`] — never unified (D8-06).
fn disambiguate_filename(name: &str, current: &HashSet<String>) -> String {
    if !current.contains(name) {
        return name.to_string();
    }
    let mut ext = 0u32;
    loop {
        ext += 1;
        let candidate = format!("{name}_{ext}");
        if !current.contains(&candidate) {
            return candidate;
        }
    }
}

/// Parenthetical disambiguation for `PlaylistItem.Label` (`check_label`,
/// `JWLManager.py:3538-3544`): `label`, `label (1)`, `label (2)`, ... A
/// DISTINCT scheme from [`disambiguate_filename`] — never unified (D8-06).
fn disambiguate_label(label: &str, current: &HashSet<String>) -> String {
    if !current.contains(label) {
        return label.to_string();
    }
    let mut ext = 0u32;
    loop {
        ext += 1;
        let candidate = format!("{label} ({ext})");
        if !current.contains(&candidate) {
            return candidate;
        }
    }
}

/// Resolves-or-creates the target's playlist `Tag (Type = 2, Name = ?)` —
/// an explicit SELECT-then-INSERT (`JWLManager.py:3550-3556`'s
/// try/except-as-control-flow is deliberately NOT ported; see
/// `db::playlist_io::ensure_playlist_tag` for the identical precedent).
fn ensure_playlist_tag(
    tx: &Transaction,
    playlist_name: &str,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT TagId FROM Tag WHERE Type = 2 AND Name = ?1",
            params![playlist_name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_err(e, "ensure_playlist_tag: lookup"))?;
    if let Some(id) = existing {
        return Ok(id);
    }
    if let Some(id) = take_id(available, "Tag") {
        tx.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (?1, 2, ?2)",
            params![id, playlist_name],
        )
        .map_err(|e| map_sqlite_err(e, "ensure_playlist_tag: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO Tag (Type, Name) VALUES (2, ?1)",
            params![playlist_name],
        )
        .map_err(|e| map_sqlite_err(e, "ensure_playlist_tag: insert new id"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Applies a media-add batch inside the caller's transaction: resolves the
/// playlist Tag, then for each `prechecked_new` entry (the CALLER must have
/// already filtered to `MediaClassification::New` — any other variant is
/// skipped defensively) inserts the original `IndependentMedia` row, a
/// SECOND `IndependentMedia` row for the thumbnail (a byte-for-byte copy of
/// the source, PD-1 — never a resize), the `PlaylistItem`
/// (`Accuracy = 1`, `EndAction = 1`), its
/// `PlaylistItemIndependentMediaMap` row (`DurationTicks = 40000000`
/// literal), and the playlist Tag's `TagMap` row. Records every required
/// file copy into `staged` rather than performing it — PD-3's ordering is
/// enforced by the CALLER (open tx -> this function -> stage copies -> copy
/// -> commit only on success).
///
/// Two selected files sharing identical content within ONE batch (both
/// classified `New` relative to the archive's pre-existing media) reuse the
/// SAME `IndependentMediaId`/thumbnail pair rather than inserting duplicate
/// rows or copying the source twice — tracked via `batch_hashes`, mirroring
/// Python's own `current_hashes.append(hash256)` running-list semantics
/// (`:3569-3570`).
pub fn apply_media_add(
    tx: &Transaction,
    playlist_name: &str,
    prechecked_new: &[MediaPrecheck],
    staged: &mut Vec<PendingCopy>,
    available: &mut HashMap<&'static str, Vec<i64>>,
    guid_seed: u64,
) -> Result<usize, ArchiveError> {
    let tag_id = ensure_playlist_tag(tx, playlist_name, available)?;

    let mut current_files: HashSet<String> = {
        let mut stmt = tx
            .prepare("SELECT FilePath FROM IndependentMedia")
            .map_err(|e| map_sqlite_err(e, "apply_media_add: prepare current_files"))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| map_sqlite_err(e, "apply_media_add: query current_files"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| map_sqlite_err(e, "apply_media_add: read current_files"))?
    };
    let mut current_labels: HashSet<String> = {
        let mut stmt = tx
            .prepare(
                "SELECT Label FROM PlaylistItem JOIN TagMap USING (PlaylistItemId) WHERE TagId = ?1",
            )
            .map_err(|e| map_sqlite_err(e, "apply_media_add: prepare current_labels"))?;
        let rows = stmt
            .query_map(params![tag_id], |r| r.get::<_, String>(0))
            .map_err(|e| map_sqlite_err(e, "apply_media_add: query current_labels"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| map_sqlite_err(e, "apply_media_add: read current_labels"))?
    };

    // hash -> (IndependentMediaId of the ORIGINAL, thumbnail FilePath) —
    // reused across this batch only; see docs above.
    let mut batch_hashes: HashMap<String, (i64, String)> = HashMap::new();

    let mut added = 0usize;
    for (idx, item) in prechecked_new.iter().enumerate() {
        let (hash, mime, format) = match &item.classification {
            MediaClassification::New { hash, mime, format } => {
                (hash.clone(), mime.clone(), *format)
            }
            // Defensive only — callers (the `media_add_apply` command) are
            // the ones that pre-filter to `New`; a non-New entry here is a
            // caller bug, not a user-facing case, so it is silently skipped
            // rather than surfaced as a typed error.
            _ => continue,
        };

        let src_name = item
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "media".to_string());

        let (media_id, thumb_name) = if let Some((id, thumb)) = batch_hashes.get(&hash) {
            (*id, thumb.clone())
        } else {
            let new_name = disambiguate_filename(&src_name, &current_files);
            current_files.insert(new_name.clone());

            let media_id = if let Some(id) = take_id(available, "IndependentMedia") {
                tx.execute(
                    "INSERT INTO IndependentMedia (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, src_name, new_name, mime, hash],
                )
                .map_err(|e| map_sqlite_err(e, "apply_media_add: insert original media (recycled id)"))?;
                id
            } else {
                tx.execute(
                    "INSERT INTO IndependentMedia (OriginalFilename, FilePath, MimeType, Hash) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![src_name, new_name, mime, hash],
                )
                .map_err(|e| map_sqlite_err(e, "apply_media_add: insert original media"))?;
                tx.last_insert_rowid()
            };

            // Thumbnail: a fresh-GUID-named byte-for-byte COPY of the source
            // (`JWLManager.py:3576-3583`'s `Image.open`/`.thumbnail((250,
            // 250))`/`.save` is DELIBERATELY not ported — PD-1, the `image`
            // crate could not be legitimacy-verified, 08-RESEARCH.md's ⚠
            // Addendum option (b)). This diverges from the Python on file
            // SIZE only — schema, row count, hash correctness and wire
            // format all match. TODO(future phase): add real
            // aspect-preserving 250x250 resizing once a vetted image
            // decode/encode crate is approved.
            let thumb_name = format!(
                "{}.{}",
                crate::guid::format_guid_v4(guid_seed.wrapping_add(idx as u64)),
                format.extension()
            );
            current_files.insert(thumb_name.clone());
            tx.execute(
                "INSERT INTO IndependentMedia (OriginalFilename, FilePath, MimeType, Hash) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![src_name, thumb_name, mime, hash],
            )
            .map_err(|e| map_sqlite_err(e, "apply_media_add: insert thumbnail media"))?;

            batch_hashes.insert(hash.clone(), (media_id, thumb_name.clone()));
            staged.push(PendingCopy {
                src: item.path.clone(),
                dst_name: new_name,
            });
            staged.push(PendingCopy {
                src: item.path.clone(),
                dst_name: thumb_name.clone(),
            });

            (media_id, thumb_name)
        };

        let label = disambiguate_label(&src_name, &current_labels);
        current_labels.insert(label.clone());

        let item_id = if let Some(id) = take_id(available, "PlaylistItem") {
            tx.execute(
                "INSERT INTO PlaylistItem (PlaylistItemId, Label, Accuracy, EndAction, ThumbnailFilePath) \
                 VALUES (?1, ?2, 1, 1, ?3)",
                params![id, label, thumb_name],
            )
            .map_err(|e| map_sqlite_err(e, "apply_media_add: insert PlaylistItem (recycled id)"))?;
            id
        } else {
            tx.execute(
                "INSERT INTO PlaylistItem (Label, Accuracy, EndAction, ThumbnailFilePath) \
                 VALUES (?1, 1, 1, ?2)",
                params![label, thumb_name],
            )
            .map_err(|e| map_sqlite_err(e, "apply_media_add: insert PlaylistItem"))?;
            tx.last_insert_rowid()
        };

        tx.execute(
            "INSERT INTO PlaylistItemIndependentMediaMap (PlaylistItemId, IndependentMediaId, DurationTicks) \
             VALUES (?1, ?2, 40000000)",
            params![item_id, media_id],
        )
        .map_err(|e| map_sqlite_err(e, "apply_media_add: insert media map"))?;

        let position: i64 = tx
            .query_row(
                "SELECT IFNULL(MAX(Position), -1) + 1 FROM TagMap WHERE TagId = ?1",
                params![tag_id],
                |r| r.get(0),
            )
            .map_err(|e| map_sqlite_err(e, "apply_media_add: compute TagMap position"))?;
        if let Some(tagmap_id) = take_id(available, "TagMap") {
            tx.execute(
                "INSERT INTO TagMap (TagMapId, PlaylistItemId, TagId, Position) VALUES (?1, ?2, ?3, ?4)",
                params![tagmap_id, item_id, tag_id, position],
            )
            .map_err(|e| map_sqlite_err(e, "apply_media_add: insert TagMap (recycled id)"))?;
        } else {
            tx.execute(
                "INSERT INTO TagMap (PlaylistItemId, TagId, Position) VALUES (?1, ?2, ?3)",
                params![item_id, tag_id, position],
            )
            .map_err(|e| map_sqlite_err(e, "apply_media_add: insert TagMap"))?;
        }

        added += 1;
    }

    Ok(added)
}

/// Performs every `staged` copy into `media_dir`. On the FIRST failure,
/// deletes every file THIS call had already written (PD-3) and returns
/// `Err` naming the failing file — the caller must not commit its
/// transaction on this `Err`, so the whole batch rolls back atomically and
/// no committed row can ever point at a file that was never written.
pub fn perform_staged_copies(staged: &[PendingCopy], media_dir: &Path) -> Result<(), ArchiveError> {
    let mut written: Vec<PathBuf> = Vec::new();
    for copy in staged {
        let dst = media_dir.join(&copy.dst_name);
        match fs::copy(&copy.src, &dst) {
            Ok(_) => written.push(dst),
            Err(err) => {
                for w in &written {
                    let _ = fs::remove_file(w);
                }
                let _ = fs::remove_file(&dst); // any partially-written bytes at dst itself
                return Err(ArchiveError::MediaAddFailed {
                    reason: format!("copying \"{}\": {err}", copy.dst_name),
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Playlist item delete — two-pass media reference counting (D8-07).
// ---------------------------------------------------------------------------

fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(",")
}

/// The DB-only result of [`delete_playlist_items_db`]: the `FilePath`s whose
/// `IndependentMedia` rows were removed (for [`remove_media_files`], apply
/// path only) and the count of files evaluated-but-KEPT because a surviving
/// item still references them (for the delete-preview summary, D8-07).
#[derive(Debug, Clone, Default)]
pub struct PlaylistMediaDeleteOutcome {
    pub removed_files: Vec<String>,
    pub kept_count: usize,
}

/// Performs ALL and ONLY the DB work of deleting `ids`' `PlaylistItem`s and
/// their orphaned media — ports `delete_playlist_items`
/// (`JWLManager.py:3627-3656`) exactly, including its table order. Computes
/// `used_thumbs` (the `ThumbnailFilePath`s of items NOT in `ids`) and
/// `used_files` (the `FilePath`s reached through `IndependentMedia` JOIN
/// `PlaylistItemIndependentMediaMap` for items NOT in `ids`) as TWO
/// INDEPENDENT sets in two independent loops (D8-07) — a file that is a
/// surviving item's thumbnail AND a deleted item's full media is evaluated
/// by EACH set separately and never double-counted against either. Performs
/// NO filesystem operation of any kind — [`remove_media_files`] is a
/// SEPARATE function this one never calls, which is what makes
/// [`dry_run_delete_playlist_items`] structurally incapable of touching the
/// filesystem (D8-07).
pub fn delete_playlist_items_db(
    tx: &Transaction,
    ids: &NonEmptyPlaylistItemIds,
) -> Result<PlaylistMediaDeleteOutcome, ArchiveError> {
    let ph = placeholders(ids.len());
    let id_vals: Vec<i64> = ids.iter().copied().collect();

    let used_thumbs: HashSet<String> = {
        let sql = format!(
            "SELECT ThumbnailFilePath FROM PlaylistItem \
             WHERE PlaylistItemId NOT IN ({ph}) AND ThumbnailFilePath IS NOT NULL"
        );
        let mut stmt = tx
            .prepare(&sql)
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: prepare used_thumbs"))?;
        let rows = stmt
            .query_map(params_from_iter(id_vals.iter()), |r| r.get::<_, String>(0))
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: query used_thumbs"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: read used_thumbs"))?
    };
    let used_files: HashSet<String> = {
        let sql = format!(
            "SELECT FilePath FROM IndependentMedia \
             JOIN PlaylistItemIndependentMediaMap USING (IndependentMediaId) \
             WHERE PlaylistItemId NOT IN ({ph})"
        );
        let mut stmt = tx
            .prepare(&sql)
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: prepare used_files"))?;
        let rows = stmt
            .query_map(params_from_iter(id_vals.iter()), |r| r.get::<_, String>(0))
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: query used_files"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: read used_files"))?
    };

    let mut removed_files: Vec<String> = Vec::new();
    let mut kept: HashSet<String> = HashSet::new();

    // Thumbnail pass — INDEPENDENT of the full-media pass below.
    {
        let sql = format!(
            "SELECT ThumbnailFilePath FROM PlaylistItem \
             WHERE PlaylistItemId IN ({ph}) AND ThumbnailFilePath IS NOT NULL"
        );
        let mut stmt = tx
            .prepare(&sql)
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: prepare thumb scan"))?;
        let mut rows = stmt
            .query(params_from_iter(id_vals.iter()))
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: query thumb scan"))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: read thumb row"))?
        {
            let fp: String = row
                .get(0)
                .map_err(|e| map_delete_err(e, "delete_playlist_items_db: read thumb FilePath"))?;
            if used_thumbs.contains(&fp) {
                kept.insert(fp);
                continue;
            }
            tx.execute(
                "DELETE FROM IndependentMedia WHERE FilePath = ?1",
                params![fp],
            )
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: delete thumb media"))?;
            removed_files.push(fp);
        }
    }

    // Full-media pass — INDEPENDENT of the thumbnail pass above (D8-07).
    {
        let sql = format!(
            "SELECT FilePath FROM IndependentMedia \
             JOIN PlaylistItemIndependentMediaMap USING (IndependentMediaId) \
             WHERE PlaylistItemId IN ({ph})"
        );
        let mut stmt = tx
            .prepare(&sql)
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: prepare media scan"))?;
        let mut rows = stmt
            .query(params_from_iter(id_vals.iter()))
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: query media scan"))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: read media row"))?
        {
            let fp: String = row
                .get(0)
                .map_err(|e| map_delete_err(e, "delete_playlist_items_db: read media FilePath"))?;
            if used_files.contains(&fp) {
                kept.insert(fp);
                continue;
            }
            tx.execute(
                "DELETE FROM IndependentMedia WHERE FilePath = ?1",
                params![fp],
            )
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: delete full media"))?;
            removed_files.push(fp);
        }
    }

    for (table, field) in [
        ("PlaylistItemIndependentMediaMap", "PlaylistItemId"),
        ("PlaylistItemLocationMap", "PlaylistItemId"),
        ("TagMap", "PlaylistItemId"),
    ] {
        let sql = format!("DELETE FROM {table} WHERE {field} IN ({ph})");
        tx.execute(&sql, params_from_iter(id_vals.iter()))
            .map_err(|e| map_delete_err(e, "delete_playlist_items_db: delete join/map table"))?;
    }

    tx.execute(
        &format!(
            "DELETE FROM PlaylistItemMarkerBibleVerseMap WHERE PlaylistItemMarkerId IN \
             (SELECT PlaylistItemMarkerId FROM PlaylistItemMarker WHERE PlaylistItemId IN ({ph}))"
        ),
        params_from_iter(id_vals.iter()),
    )
    .map_err(|e| map_delete_err(e, "delete_playlist_items_db: delete marker bible-verse map"))?;
    tx.execute(
        &format!(
            "DELETE FROM PlaylistItemMarkerParagraphMap WHERE PlaylistItemMarkerId IN \
             (SELECT PlaylistItemMarkerId FROM PlaylistItemMarker WHERE PlaylistItemId IN ({ph}))"
        ),
        params_from_iter(id_vals.iter()),
    )
    .map_err(|e| map_delete_err(e, "delete_playlist_items_db: delete marker paragraph map"))?;
    tx.execute(
        &format!("DELETE FROM PlaylistItemMarker WHERE PlaylistItemId IN ({ph})"),
        params_from_iter(id_vals.iter()),
    )
    .map_err(|e| map_delete_err(e, "delete_playlist_items_db: delete PlaylistItemMarker"))?;
    tx.execute(
        &format!("DELETE FROM PlaylistItem WHERE PlaylistItemId IN ({ph})"),
        params_from_iter(id_vals.iter()),
    )
    .map_err(|e| map_delete_err(e, "delete_playlist_items_db: delete PlaylistItem"))?;

    Ok(PlaylistMediaDeleteOutcome {
        removed_files,
        kept_count: kept.len(),
    })
}

/// Best-effort file removal — the ONLY place in this module (or, by
/// construction, in [`dry_run_delete_playlist_items`]'s call graph) that
/// touches the filesystem for a delete. A missing file is silently ignored,
/// matching Python's bare `except: pass` (`JWLManager.py:3636-3637`,
/// `:3646-3647`). Called ONLY from the apply path, AFTER the DB transaction
/// commits (D8-07/PD-3).
pub fn remove_media_files(media_dir: &Path, files: &[String]) {
    for f in files {
        let _ = fs::remove_file(media_dir.join(f));
    }
}

/// The Tauri-facing report for BOTH the media-delete dry-run and apply: the
/// standard [`DryRunReport`] plus the two D8-07 counts the UI-SPEC's
/// "shared media survives" summary needs — `media_removed` (files whose
/// `IndependentMedia` row was deleted) and `media_kept` (files evaluated but
/// kept because a surviving item still references them).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/PlaylistDeleteReport.ts")]
pub struct PlaylistDeleteReport {
    pub report: DryRunReport,
    pub media_removed: usize,
    pub media_kept: usize,
}

/// Never-committed-transaction preview (SAFE-01) — calls ONLY
/// [`delete_playlist_items_db`] and discards its returned file list, so this
/// function is STRUCTURALLY incapable of reaching [`remove_media_files`]
/// (D8-07). Leaves every media file on disk and every table's row count
/// unchanged.
pub fn dry_run_delete_playlist_items(
    conn: &mut Connection,
    ids: &NonEmptyPlaylistItemIds,
) -> Result<PlaylistDeleteReport, ArchiveError> {
    let guard = PragmaGuard::new(conn).map_err(|e| map_delete_err(e, "snapshotting pragmas"))?;
    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_delete_err(e, "setting dry-run pragmas"))?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_delete_err(e, "opening dry-run transaction"))?;

    let before = snapshot_tables(&tx, MEDIA_DELETE_SNAPSHOT_TABLES)?;
    let outcome = delete_playlist_items_db(&tx, ids)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, MEDIA_DELETE_SNAPSHOT_TABLES)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
    drop(guard);

    Ok(PlaylistDeleteReport {
        report,
        media_removed: outcome.removed_files.len(),
        media_kept: outcome.kept_count,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn sniff_format_recognizes_every_supported_signature() {
        assert_eq!(sniff_format(b"BMxxxxxxxx"), Some(MediaFormat::Bmp));
        assert_eq!(sniff_format(b"GIF89axxxx"), Some(MediaFormat::Gif));
        assert_eq!(
            sniff_format(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(MediaFormat::Jpeg)
        );
        assert_eq!(
            sniff_format(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            Some(MediaFormat::Png)
        );
        let mut heic = vec![0u8, 0, 0, 24];
        heic.extend_from_slice(b"ftypheic");
        assert_eq!(sniff_format(&heic), Some(MediaFormat::Heic));
        assert_eq!(sniff_format(b"not an image"), None);
    }

    #[test]
    fn disambiguate_filename_and_label_use_distinct_schemes() {
        let mut files = HashSet::new();
        files.insert("photo.png".to_string());
        assert_eq!(disambiguate_filename("photo.png", &files), "photo.png_1");

        let mut labels = HashSet::new();
        labels.insert("photo.png".to_string());
        assert_eq!(disambiguate_label("photo.png", &labels), "photo.png (1)");
    }
}
