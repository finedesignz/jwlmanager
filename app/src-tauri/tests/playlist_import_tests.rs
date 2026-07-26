//! `.jwlplaylist` import tests (08-05-PLAN.md Task 2) — synthetic fixtures
//! only. Containers under test are built by round-tripping through
//! [`export_playlist_from_seed`] (the same production export path) rather
//! than hand-authoring zip bytes, except for the two structural-rejection
//! tests, which construct a hostile/incomplete zip directly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::playlist_io::{
    apply_import_playlist, dry_run_import_playlist, export_playlist_from_seed,
    read_playlist_container, NonEmptyPlaylistItemIds,
};
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

fn res_blank_playlist_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../res/blank_playlist")
}

/// Builds a real `.jwlplaylist` container (via the production export path)
/// carrying one playlist item with a thumbnail, a location, and a tag.
/// Returns the work dir (keep alive) and the container's path.
fn build_container(pi_id: i64, label: &str, container_name: &str) -> (tempfile::TempDir, PathBuf) {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open source db");
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn.execute(
        "INSERT INTO PlaylistItemAccuracy (PlaylistItemAccuracyId, Description) VALUES (1, 'Exact')",
        [],
    )
    .unwrap();
    conn.execute(
        &format!(
            "INSERT INTO PlaylistItem (PlaylistItemId, Label, StartTrimOffsetTicks, EndTrimOffsetTicks, \
             Accuracy, EndAction, ThumbnailFilePath) VALUES ({pi_id}, ?1, NULL, NULL, 1, 1, 'thumb.jpg')"
        ),
        rusqlite::params![label],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO IndependentMedia (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
         VALUES (10, 'thumb-original.jpg', 'thumb.jpg', 'image/jpeg', 'hash-thumb-fixed')",
        [],
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
        &format!(
            "INSERT INTO PlaylistItemLocationMap (PlaylistItemId, LocationId, MajorMultimediaType, BaseDurationTicks) \
             VALUES ({pi_id}, 500, 1, 12345)"
        ),
        [],
    )
    .unwrap();
    fs::write(dir.path().join("thumb.jpg"), b"fake-thumb-bytes").unwrap();

    let dest = dir.path().join(container_name);
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
    .expect("fixture export must succeed");

    (dir, dest)
}

fn table_count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn incoming_playlist_item_id_collision_never_overwrites_existing_row() {
    let (_container_dir, container_path) = build_container(5000, "Container Song", "Collide.jwlplaylist");

    let (target_dir, target_db) = common::fresh_v16_db();
    let target_conn = Connection::open(&target_db).expect("open target db");
    target_conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    // Pre-seed a colliding row at the SAME id (5000) the container's own
    // PlaylistItemId happens to be — this must never be touched.
    target_conn
        .execute(
            "INSERT INTO PlaylistItemAccuracy (PlaylistItemAccuracyId, Description) VALUES (1, 'Exact')",
            [],
        )
        .unwrap();
    target_conn
        .execute(
            "INSERT INTO PlaylistItem (PlaylistItemId, Label, StartTrimOffsetTicks, EndTrimOffsetTicks, \
             Accuracy, EndAction, ThumbnailFilePath) VALUES (5000, 'Pre-Existing Unrelated Song', NULL, NULL, 1, 1, NULL)",
            [],
        )
        .unwrap();

    let container = read_playlist_container(&container_path).expect("read container");

    let mut target_conn = target_conn;
    let tx = target_conn.transaction().unwrap();
    let mut available = compute_available_ids(&tx).unwrap();
    let skipped = apply_import_playlist(
        &tx,
        &container,
        "Collide",
        Some(target_dir.path()),
        &mut available,
    )
    .expect("apply should succeed");
    tx.commit().unwrap();
    assert_eq!(skipped, 0, "no existing row semantically matches — nothing skipped");

    let label: String = target_conn
        .query_row(
            "SELECT Label FROM PlaylistItem WHERE PlaylistItemId = 5000",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(label, "Pre-Existing Unrelated Song", "the existing row must be untouched");

    let imported_id: i64 = target_conn
        .query_row(
            "SELECT PlaylistItemId FROM PlaylistItem WHERE Label = 'Container Song'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(imported_id, 5000, "the imported item must receive a DIFFERENT id");
}

#[test]
fn dependent_rows_reference_the_newly_allocated_id() {
    let (_container_dir, container_path) = build_container(5000, "Dependent Song", "Dependent.jwlplaylist");
    let (target_dir, target_db) = common::fresh_v16_db();
    let mut target_conn = Connection::open(&target_db).expect("open target db");
    target_conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    let container = read_playlist_container(&container_path).expect("read container");
    let tx = target_conn.transaction().unwrap();
    let mut available = compute_available_ids(&tx).unwrap();
    apply_import_playlist(&tx, &container, "Dependent", Some(target_dir.path()), &mut available)
        .expect("apply should succeed");
    tx.commit().unwrap();

    let new_pi_id: i64 = target_conn
        .query_row(
            "SELECT PlaylistItemId FROM PlaylistItem WHERE Label = 'Dependent Song'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    let media_map_pi: i64 = target_conn
        .query_row(
            "SELECT PlaylistItemId FROM PlaylistItemLocationMap",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(media_map_pi, new_pi_id);

    // The thumbnail's own IndependentMedia row was deduped-in (media_add
    // dedup is by Hash) — its resolved FilePath is what PlaylistItem.
    // ThumbnailFilePath must carry.
    let thumb_fp: Option<String> = target_conn
        .query_row(
            "SELECT ThumbnailFilePath FROM PlaylistItem WHERE PlaylistItemId = ?1",
            rusqlite::params![new_pi_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(thumb_fp.as_deref(), Some("thumb.jpg"));
}

#[test]
fn semantically_identical_reimport_is_reused_and_reported_as_skipped() {
    let (_container_dir, container_path) = build_container(5000, "Repeat Song", "Repeat.jwlplaylist");
    let (target_dir, target_db) = common::fresh_v16_db();
    let mut target_conn = Connection::open(&target_db).expect("open target db");
    target_conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    let container = read_playlist_container(&container_path).expect("read container");

    {
        let tx = target_conn.transaction().unwrap();
        let mut available = compute_available_ids(&tx).unwrap();
        let skipped =
            apply_import_playlist(&tx, &container, "Repeat", Some(target_dir.path()), &mut available)
                .unwrap();
        tx.commit().unwrap();
        assert_eq!(skipped, 0, "first import: nothing pre-exists");
    }

    let count_after_first = table_count(&target_conn, "PlaylistItem");

    let container2 = read_playlist_container(&container_path).expect("re-read container");
    {
        let tx = target_conn.transaction().unwrap();
        let mut available = compute_available_ids(&tx).unwrap();
        let skipped = apply_import_playlist(
            &tx,
            &container2,
            "Repeat",
            Some(target_dir.path()),
            &mut available,
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(skipped, 1, "second import: the exact same item is reused");
    }

    let count_after_second = table_count(&target_conn, "PlaylistItem");
    assert_eq!(
        count_after_first, count_after_second,
        "a reused item must not create a duplicate PlaylistItem row"
    );
}

#[test]
fn zip_slip_container_is_rejected_and_writes_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let hostile_path = dir.path().join("hostile.jwlplaylist");
    {
        let file = fs::File::create(&hostile_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("../evil.txt", options).unwrap();
        use std::io::Write;
        writer.write_all(b"path traversal payload").unwrap();
        writer.finish().unwrap();
    }

    let result = read_playlist_container(&hostile_path);
    assert!(matches!(result, Err(ArchiveError::ZipSlipRejected)));
}

#[test]
fn container_missing_user_data_db_fails_before_any_transaction() {
    let dir = tempfile::TempDir::new().unwrap();
    let incomplete_path = dir.path().join("incomplete.jwlplaylist");
    {
        let file = fs::File::create(&incomplete_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        use std::io::Write;
        writer.write_all(b"{}").unwrap();
        writer.finish().unwrap();
    }

    let result = read_playlist_container(&incomplete_path);
    assert!(
        matches!(result, Err(ArchiveError::PlaylistImportFailed { .. })),
        "a container missing userData.db must fail fast, before any transaction ever opens"
    );
}

#[test]
fn new_ids_consume_the_seeded_gap_set_before_autoincrement() {
    let (_container_dir, container_path) = build_container(5000, "Gap Song", "Gap.jwlplaylist");
    let (target_dir, target_db) = common::fresh_v16_db();
    let mut target_conn = Connection::open(&target_db).expect("open target db");
    target_conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();

    // Seed PlaylistItem ids 1 and 3, leaving a gap at 2 — `compute_available_ids`
    // must surface `2` as recyclable for the `PlaylistItem` table.
    target_conn
        .execute(
            "INSERT INTO PlaylistItemAccuracy (PlaylistItemAccuracyId, Description) VALUES (1, 'Exact')",
            [],
        )
        .unwrap();
    for id in [1_i64, 3] {
        target_conn
            .execute(
                &format!(
                    "INSERT INTO PlaylistItem (PlaylistItemId, Label, StartTrimOffsetTicks, EndTrimOffsetTicks, \
                     Accuracy, EndAction, ThumbnailFilePath) VALUES ({id}, 'Filler', NULL, NULL, 1, 1, NULL)"
                ),
                [],
            )
            .unwrap();
    }

    let container = read_playlist_container(&container_path).expect("read container");
    let tx = target_conn.transaction().unwrap();
    let mut available = compute_available_ids(&tx).unwrap();
    apply_import_playlist(&tx, &container, "Gap", Some(target_dir.path()), &mut available).unwrap();
    tx.commit().unwrap();

    let new_id: i64 = target_conn
        .query_row(
            "SELECT PlaylistItemId FROM PlaylistItem WHERE Label = 'Gap Song'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_id, 2, "the recycled gap id must be used before autoincrement (4)");
}

#[test]
fn dry_run_leaves_every_affected_table_row_count_unchanged() {
    let (_container_dir, container_path) = build_container(5000, "Dry Song", "Dry.jwlplaylist");
    let (_target_dir, target_db) = common::fresh_v16_db();
    let mut target_conn = Connection::open(&target_db).expect("open target db");

    let container = read_playlist_container(&container_path).expect("read container");

    const TABLES: [&str; 5] = ["Tag", "TagMap", "PlaylistItem", "Location", "IndependentMedia"];
    let before: Vec<i64> = TABLES.iter().map(|t| table_count(&target_conn, t)).collect();

    let report = dry_run_import_playlist(&mut target_conn, &container, "Dry").expect("dry run");

    let after: Vec<i64> = TABLES.iter().map(|t| table_count(&target_conn, t)).collect();

    assert_eq!(before, after, "dry run must leave every tracked table's row count unchanged");

    // The report should still show what WOULD have happened (from inside the
    // dry run's own rolled-back transaction).
    assert!(
        report.added.values().sum::<usize>() > 0,
        "the dry-run's own report must show the would-be effect"
    );
}
