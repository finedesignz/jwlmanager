//! `.jwlplaylist` export tests (08-05-PLAN.md Task 1) — synthetic fixtures
//! only, per this plan's prohibition (`test_no_real_archive_is_tracked_in_git`
//! lives in `archive_validity_tests.rs` and covers the whole repo; this file
//! never writes or reads a real user archive).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::playlist_io::{export_playlist_from_seed, NonEmptyPlaylistItemIds};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn res_blank_playlist_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../res/blank_playlist")
}

/// Seeds a fresh v16 db (via `common::fresh_v16_db`) with one full playlist
/// item: a thumbnail + full-media `IndependentMedia` pair, a scripture
/// `Location`, a marker with both sub-maps, and a `TagMap` association —
/// covering every export behavior in one fixture. Returns the work dir (also
/// the media source dir), the db path, and the seeded `PlaylistItemId`.
fn seed_full_playlist_item(with_media_files_on_disk: bool) -> (tempfile::TempDir, PathBuf, i64) {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    conn.execute(
        "INSERT INTO PlaylistItemAccuracy (PlaylistItemAccuracyId, Description) VALUES (1, 'Exact')",
        [],
    )
    .unwrap();

    let pi_id = 5000_i64;
    conn.execute(
        "INSERT INTO PlaylistItem (PlaylistItemId, Label, StartTrimOffsetTicks, EndTrimOffsetTicks, \
         Accuracy, EndAction, ThumbnailFilePath) VALUES (?1, 'My Song', NULL, NULL, 1, 1, 'thumb.jpg')",
        rusqlite::params![pi_id],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO IndependentMedia (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
         VALUES (10, 'thumb-original.jpg', 'thumb.jpg', 'image/jpeg', 'hash-thumb')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO IndependentMedia (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
         VALUES (11, 'full-original.mp4', 'full.mp4', 'video/mp4', 'hash-full')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO PlaylistItemIndependentMediaMap (PlaylistItemId, IndependentMediaId, DurationTicks) \
         VALUES (?1, 11, 12345)",
        rusqlite::params![pi_id],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (500, 1, 1, NULL, NULL, 0, 'nwt', 0, 0, 'Genesis 1:1', NULL, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO PlaylistItemLocationMap (PlaylistItemId, LocationId, MajorMultimediaType, BaseDurationTicks) \
         VALUES (?1, 500, 1, 12345)",
        rusqlite::params![pi_id],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO PlaylistItemMarker (PlaylistItemMarkerId, PlaylistItemId, Label, StartTimeTicks, \
         DurationTicks, EndTransitionDurationTicks) VALUES (900, ?1, 'Marker', 0, 100, 0)",
        rusqlite::params![pi_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO PlaylistItemMarkerBibleVerseMap (PlaylistItemMarkerId, VerseId) VALUES (900, 1001001)",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (700, 2, 'Existing Playlist')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (800, ?1, NULL, NULL, 700, 0)",
        rusqlite::params![pi_id],
    )
    .unwrap();

    if with_media_files_on_disk {
        fs::write(dir.path().join("thumb.jpg"), b"fake-thumb-bytes").unwrap();
        fs::write(dir.path().join("full.mp4"), b"fake-full-media-bytes").unwrap();
    }

    (dir, db_path, pi_id)
}

#[test]
fn export_produces_compact_manifest_with_correct_hash() {
    let (dir, db_path, pi_id) = seed_full_playlist_item(true);
    let conn = Connection::open(&db_path).expect("open db");
    let dest = dir.path().join("My Export.jwlplaylist");

    let report = export_playlist_from_seed(
        &res_blank_playlist_path(),
        &conn,
        dir.path(),
        &NonEmptyPlaylistItemIds::try_from(vec![pi_id]).unwrap(),
        &dest,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-01T00:00:00Z",
    )
    .expect("export should succeed");

    assert_eq!(report.item_count, 1);
    assert!(report.warnings.is_empty(), "warnings: {:?}", report.warnings);

    let file = fs::File::open(&dest).expect("open produced zip");
    let mut zip = zip::ZipArchive::new(file).expect("valid zip");

    let manifest_bytes = {
        let mut entry = zip.by_name("manifest.json").expect("manifest.json present");
        let mut buf = Vec::new();
        std::io::copy(&mut entry, &mut buf).unwrap();
        buf
    };
    let manifest_str = String::from_utf8(manifest_bytes.clone()).unwrap();
    assert!(!manifest_str.contains(", "), "manifest must not contain ', '");
    assert!(!manifest_str.contains(": "), "manifest must not contain ': '");
    assert!(!manifest_str.contains('\n'), "manifest must not contain a newline");

    let db_bytes = {
        let mut entry = zip.by_name("userData.db").expect("userData.db present");
        let mut buf = Vec::new();
        std::io::copy(&mut entry, &mut buf).unwrap();
        buf
    };
    let recomputed_hash = {
        let digest = Sha256::digest(&db_bytes);
        digest.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let manifest_json: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(
        manifest_json["userDataBackup"]["hash"].as_str().unwrap(),
        recomputed_hash
    );
    assert_eq!(manifest_json["type"].as_i64().unwrap(), 1);
}

#[test]
fn export_writes_single_hardcoded_tag_named_after_destination_stem() {
    let (dir, db_path, pi_id) = seed_full_playlist_item(true);
    let conn = Connection::open(&db_path).expect("open db");
    let dest = dir.path().join("StemName.jwlplaylist");

    export_playlist_from_seed(
        &res_blank_playlist_path(),
        &conn,
        dir.path(),
        &NonEmptyPlaylistItemIds::try_from(vec![pi_id]).unwrap(),
        &dest,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-01T00:00:00Z",
    )
    .expect("export should succeed");

    let (_extract_dir, extracted_db) = extract_playlist_db(&dest);
    let mini = Connection::open(&extracted_db).expect("open mini db");

    let (tag_id, kind, name): (i64, i64, String) = mini
        .query_row("SELECT TagId, Type, Name FROM Tag", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .expect("exactly one Tag row");
    assert_eq!(tag_id, 1);
    assert_eq!(kind, 2);
    assert_eq!(name, "StemName");

    let count: i64 = mini
        .query_row("SELECT COUNT(*) FROM Tag", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn export_tagmap_positions_are_dense_zero_based() {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn.execute(
        "INSERT INTO PlaylistItemAccuracy (PlaylistItemAccuracyId, Description) VALUES (1, 'Exact')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (700, 2, 'Some Playlist')",
        [],
    )
    .unwrap();

    // Two playlist items, both tagged (Position 0 and 5 in the SOURCE, with a
    // gap) — the mini-archive must renumber to a dense 0/1 sequence.
    for (pi_id, position) in [(1_i64, 0_i64), (2_i64, 5_i64)] {
        conn.execute(
            "INSERT INTO PlaylistItem (PlaylistItemId, Label, StartTrimOffsetTicks, EndTrimOffsetTicks, \
             Accuracy, EndAction, ThumbnailFilePath) VALUES (?1, 'Song', NULL, NULL, 1, 1, NULL)",
            rusqlite::params![pi_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (?1, ?2, NULL, NULL, 700, ?3)",
            rusqlite::params![pi_id + 100, pi_id, position],
        )
        .unwrap();
    }

    let dest = dir.path().join("Dense.jwlplaylist");
    export_playlist_from_seed(
        &res_blank_playlist_path(),
        &conn,
        dir.path(),
        &NonEmptyPlaylistItemIds::try_from(vec![1, 2]).unwrap(),
        &dest,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-01T00:00:00Z",
    )
    .expect("export should succeed");

    let (_extract_dir, extracted_db) = extract_playlist_db(&dest);
    let mini = Connection::open(&extracted_db).expect("open mini db");
    let mut stmt = mini
        .prepare("SELECT Position FROM TagMap ORDER BY Position")
        .unwrap();
    let positions: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(positions, vec![0, 1]);
}

#[test]
fn export_captures_both_thumbnail_and_full_media_rows() {
    let (dir, db_path, pi_id) = seed_full_playlist_item(true);
    let conn = Connection::open(&db_path).expect("open db");
    let dest = dir.path().join("Media.jwlplaylist");

    export_playlist_from_seed(
        &res_blank_playlist_path(),
        &conn,
        dir.path(),
        &NonEmptyPlaylistItemIds::try_from(vec![pi_id]).unwrap(),
        &dest,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-01T00:00:00Z",
    )
    .expect("export should succeed");

    let (_extract_dir, extracted_db) = extract_playlist_db(&dest);
    let mini = Connection::open(&extracted_db).expect("open mini db");
    let mut stmt = mini
        .prepare("SELECT FilePath FROM IndependentMedia ORDER BY FilePath")
        .unwrap();
    let paths: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(paths, vec!["full.mp4".to_string(), "thumb.jpg".to_string()]);
}

#[test]
fn export_missing_media_file_yields_warning_not_a_failure() {
    let (dir, db_path, pi_id) = seed_full_playlist_item(false); // no files on disk
    let conn = Connection::open(&db_path).expect("open db");
    let dest = dir.path().join("Missing.jwlplaylist");

    let report = export_playlist_from_seed(
        &res_blank_playlist_path(),
        &conn,
        dir.path(),
        &NonEmptyPlaylistItemIds::try_from(vec![pi_id]).unwrap(),
        &dest,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-01T00:00:00Z",
    )
    .expect("export should still succeed despite missing media files");

    assert!(!report.warnings.is_empty(), "expected at least one warning");
    assert!(dest.is_file(), "the zip must still be written");
}

fn extract_playlist_db(zip_path: &Path) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let file = fs::File::open(zip_path).expect("open zip");
    let mut zip = zip::ZipArchive::new(file).expect("valid zip");
    zip.extract(dir.path()).expect("extract");
    let db_path = dir.path().join("userData.db");
    (dir, db_path)
}
