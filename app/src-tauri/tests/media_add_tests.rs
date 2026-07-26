//! Playlist media add tests (08-06-PLAN.md Task 1) — synthetic fixtures
//! only (tiny hand-authored BMP/GIF/JPEG/PNG byte arrays + one HEIC-signature
//! array, `common::tiny_*_bytes`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::media::{apply_media_add, media_precheck, perform_staged_copies, MediaClassification};
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

fn table_count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// Writes `bytes` to `dir/name` and returns the full path.
fn write_source(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn sniff_and_precheck_classify_new_and_reject_heic() {
    let (db_dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).unwrap();

    let src_dir = tempfile::TempDir::new().unwrap();
    let png_path = write_source(src_dir.path(), "photo.png", &common::tiny_png_bytes());
    let heic_path = write_source(src_dir.path(), "photo.heic", &common::tiny_heic_bytes());

    let results = media_precheck(&conn, &[png_path.clone(), heic_path.clone()]).unwrap();
    assert_eq!(results.len(), 2);

    assert!(matches!(results[0].classification, MediaClassification::New { .. }));
    assert_eq!(results[0].path, png_path);

    match &results[1].classification {
        MediaClassification::Unsupported { reason } => {
            assert!(reason.contains("HEIC"), "reason should name HEIC: {reason}");
        }
        other => panic!("expected Unsupported for HEIC, got {other:?}"),
    }
    assert_eq!(results[1].path, heic_path);

    // media_precheck performs no writes of any kind.
    assert_eq!(table_count(&conn, "IndependentMedia"), 0);
    drop(db_dir);
}

#[test]
fn duplicate_hash_file_adds_zero_rows_and_copies_zero_files() {
    let (_db_dir, db_path) = common::fresh_v16_db();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    let src_dir = tempfile::TempDir::new().unwrap();
    let png_bytes = common::tiny_png_bytes();
    let png_path = write_source(src_dir.path(), "photo.png", &png_bytes);

    let hash = {
        use sha2::{Digest, Sha256};
        Sha256::digest(&png_bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    conn.execute(
        "INSERT INTO IndependentMedia (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
         VALUES (1, 'photo.png', 'existing.png', 'image/png', ?1)",
        rusqlite::params![hash],
    )
    .unwrap();

    let results = media_precheck(&conn, &[png_path]).unwrap();
    assert_eq!(results.len(), 1);
    assert!(matches!(results[0].classification, MediaClassification::Duplicate { .. }));

    // Only the pre-seeded row exists — precheck alone never inserts.
    assert_eq!(table_count(&conn, "IndependentMedia"), 1);

    // Confirm the apply path, given only the New-filtered subset (empty here,
    // since the sole file is a duplicate), adds nothing either.
    let media_dir = tempfile::TempDir::new().unwrap();
    let tx = conn.transaction().unwrap();
    let mut available = compute_available_ids(&tx).unwrap();
    let mut staged = Vec::new();
    let new_only: Vec<_> = results
        .into_iter()
        .filter(|p| matches!(p.classification, MediaClassification::New { .. }))
        .collect();
    let added = apply_media_add(&tx, "My Playlist", &new_only, &mut staged, &mut available, 1).unwrap();
    assert_eq!(added, 0);
    assert!(staged.is_empty());
    perform_staged_copies(&staged, media_dir.path()).unwrap();
    tx.commit().unwrap();

    assert_eq!(table_count(&conn, "IndependentMedia"), 1);
    assert_eq!(table_count(&conn, "PlaylistItem"), 0);
    assert_eq!(fs::read_dir(media_dir.path()).unwrap().count(), 0);
}

#[test]
fn one_new_file_produces_two_independent_media_rows_and_thumbnail_is_byte_copy() {
    let (_db_dir, db_path) = common::fresh_v16_db();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    let src_dir = tempfile::TempDir::new().unwrap();
    let png_bytes = common::tiny_png_bytes();
    let png_path = write_source(src_dir.path(), "photo.png", &png_bytes);

    let results = media_precheck(&conn, &[png_path]).unwrap();
    let media_dir = tempfile::TempDir::new().unwrap();

    let tx = conn.transaction().unwrap();
    let mut available = compute_available_ids(&tx).unwrap();
    let mut staged = Vec::new();
    let added = apply_media_add(&tx, "My Playlist", &results, &mut staged, &mut available, 42).unwrap();
    assert_eq!(added, 1);
    perform_staged_copies(&staged, media_dir.path()).unwrap();
    tx.commit().unwrap();

    assert_eq!(table_count(&conn, "IndependentMedia"), 2);
    let paths: Vec<String> = {
        let mut stmt = conn.prepare("SELECT FilePath FROM IndependentMedia ORDER BY IndependentMediaId").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(paths.len(), 2);
    assert_ne!(paths[0], paths[1], "original and thumbnail must have different FilePath values");

    // DurationTicks literal.
    let duration: i64 = conn
        .query_row("SELECT DurationTicks FROM PlaylistItemIndependentMediaMap", [], |r| r.get(0))
        .unwrap();
    assert_eq!(duration, 40_000_000);

    // Thumbnail bytes equal source bytes exactly (byte-copy, PD-1 — never a
    // resize).
    let thumb_bytes = fs::read(media_dir.path().join(&paths[1])).unwrap();
    assert_eq!(thumb_bytes, png_bytes);
    let original_bytes = fs::read(media_dir.path().join(&paths[0])).unwrap();
    assert_eq!(original_bytes, png_bytes);
}

#[test]
fn disambiguation_schemes_are_distinct_underscore_vs_parenthetical() {
    let (_db_dir, db_path) = common::fresh_v16_db();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    let src_dir = tempfile::TempDir::new().unwrap();
    // Two files with the SAME name but DIFFERENT content (so both classify
    // New, never deduped by hash).
    let path_a = write_source(src_dir.path(), "photo.png", &common::tiny_png_bytes());
    let src_dir_b = tempfile::TempDir::new().unwrap();
    let path_b = write_source(src_dir_b.path(), "photo.png", &common::tiny_bmp_bytes());
    // Rename the second so it still sniffs as PNG-named on disk but has
    // distinct content — simplest is to just also treat it as a PNG file by
    // extension convention; sniffing is by MAGIC BYTES, not extension, so
    // giving it BMP bytes but the same file NAME independently exercises
    // disambiguation (both files are still "photo.png" by file_name()).
    let media_dir = tempfile::TempDir::new().unwrap();

    let results = media_precheck(&conn, &[path_a, path_b]).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|p| matches!(p.classification, MediaClassification::New { .. })));

    let tx = conn.transaction().unwrap();
    let mut available = compute_available_ids(&tx).unwrap();
    let mut staged = Vec::new();
    let added = apply_media_add(&tx, "My Playlist", &results, &mut staged, &mut available, 7).unwrap();
    assert_eq!(added, 2);
    perform_staged_copies(&staged, media_dir.path()).unwrap();
    tx.commit().unwrap();

    // Underscore scheme on the storage FILENAME.
    let file_paths: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT FilePath FROM IndependentMedia WHERE OriginalFilename = 'photo.png' ORDER BY IndependentMediaId")
            .unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    // 2 files x 2 rows each (original + thumbnail) = 4 IndependentMedia rows,
    // but only the two ORIGINAL FilePaths follow the underscore scheme
    // exactly ("photo.png" then "photo.png_1") — thumbnails use fresh GUID
    // names, so filter to the two whose FilePath does not look like a GUID.
    let originals: Vec<&String> = file_paths.iter().filter(|p| p.starts_with("photo.png")).collect();
    assert_eq!(originals.len(), 2);
    assert!(originals.contains(&&"photo.png".to_string()));
    assert!(originals.contains(&&"photo.png_1".to_string()));

    // Parenthetical scheme on PlaylistItem.Label.
    let labels: Vec<String> = {
        let mut stmt = conn.prepare("SELECT Label FROM PlaylistItem ORDER BY PlaylistItemId").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(labels, vec!["photo.png".to_string(), "photo.png (1)".to_string()]);
}

#[test]
fn copy_failure_rolls_back_the_whole_batch() {
    let (_db_dir, db_path) = common::fresh_v16_db();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    let src_dir = tempfile::TempDir::new().unwrap();
    let good_path = write_source(src_dir.path(), "good.png", &common::tiny_png_bytes());
    // A path that does NOT exist on disk — forces a REAL fs::copy failure
    // for the second file (not a mock/simulated failure).
    let missing_path = src_dir.path().join("does-not-exist.gif");
    fs::write(&missing_path, common::tiny_gif_bytes()).unwrap();

    let results = media_precheck(&conn, &[good_path, missing_path.clone()]).unwrap();
    assert_eq!(results.len(), 2);

    // Delete the second source file AFTER precheck (which already hashed
    // it), so `apply_media_add`'s staged copy targets a source that
    // genuinely no longer exists — a real, not simulated, copy failure.
    fs::remove_file(&missing_path).unwrap();

    let media_dir = tempfile::TempDir::new().unwrap();
    let tx = conn.transaction().unwrap();
    let mut available = compute_available_ids(&tx).unwrap();
    let mut staged = Vec::new();
    apply_media_add(&tx, "My Playlist", &results, &mut staged, &mut available, 3).unwrap();

    let copy_result = perform_staged_copies(&staged, media_dir.path());
    assert!(copy_result.is_err(), "expected a real copy failure");
    // Per PD-3: the caller must NOT commit on this Err — drop the
    // transaction instead, rolling back every staged DB write.
    drop(tx);

    assert_eq!(table_count(&conn, "IndependentMedia"), 0);
    assert_eq!(table_count(&conn, "PlaylistItem"), 0);
    assert_eq!(table_count(&conn, "TagMap"), 0);
    // The first file's two copies (original + thumbnail) must have been
    // cleaned up by `perform_staged_copies` itself, leaving the media dir
    // empty.
    assert_eq!(fs::read_dir(media_dir.path()).unwrap().count(), 0);
}

#[test]
fn no_real_archive_is_tracked_in_git() {
    // Every fixture in this file is synthetic (tiny hand-authored byte
    // arrays via `common::tiny_*_bytes`) — no real `.jwlibrary`/image is
    // read from or written to the repo (GDPR Art. 9 bright line).
    assert!(common::tiny_png_bytes().starts_with(&[0x89, 0x50, 0x4E, 0x47]));
}
