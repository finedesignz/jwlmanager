//! Playlist item delete — two-pass media reference counting tests
//! (08-06-PLAN.md Task 3, D8-07). Synthetic fixtures only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::media::{
    delete_playlist_items_db, dry_run_delete_playlist_items, remove_media_files,
};
use jwlmanager_lib::db::playlist_io::NonEmptyPlaylistItemIds;
use rusqlite::Connection;
use std::fs;

fn table_count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn media_exists(conn: &Connection, file_path: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM IndependentMedia WHERE FilePath = ?1",
        rusqlite::params![file_path],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

/// Seeds `PlaylistItemAccuracy(1, 'Exact')` (the FK every `PlaylistItem` row
/// needs) and writes `bytes` to `media_dir/name`, returning nothing — a thin
/// per-test setup helper.
fn seed_accuracy(conn: &Connection) {
    conn.execute(
        "INSERT INTO PlaylistItemAccuracy (PlaylistItemAccuracyId, Description) VALUES (1, 'Exact')",
        [],
    )
    .unwrap();
}

fn insert_media(conn: &Connection, id: i64, file_path: &str, hash: &str) {
    conn.execute(
        "INSERT INTO IndependentMedia (IndependentMediaId, OriginalFilename, FilePath, MimeType, Hash) \
         VALUES (?1, ?2, ?3, 'image/png', ?4)",
        rusqlite::params![id, file_path, file_path, hash],
    )
    .unwrap();
}

fn insert_item(conn: &Connection, id: i64, label: &str, thumb: Option<&str>) {
    conn.execute(
        "INSERT INTO PlaylistItem (PlaylistItemId, Label, Accuracy, EndAction, ThumbnailFilePath) \
         VALUES (?1, ?2, 1, 1, ?3)",
        rusqlite::params![id, label, thumb],
    )
    .unwrap();
}

fn insert_media_map(conn: &Connection, item_id: i64, media_id: i64) {
    conn.execute(
        "INSERT INTO PlaylistItemIndependentMediaMap (PlaylistItemId, IndependentMediaId, DurationTicks) \
         VALUES (?1, ?2, 40000000)",
        rusqlite::params![item_id, media_id],
    )
    .unwrap();
}

fn fresh_db_and_media_dir() -> (tempfile::TempDir, std::path::PathBuf, tempfile::TempDir) {
    let (db_dir, db_path) = common::fresh_v16_db();
    let media_dir = tempfile::TempDir::new().unwrap();
    (db_dir, db_path, media_dir)
}

#[test]
fn shared_thumbnail_survives_deletion_of_one_of_its_referencing_items() {
    let (_db_dir, db_path, media_dir) = fresh_db_and_media_dir();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    seed_accuracy(&conn);

    insert_media(&conn, 1, "shared_thumb.png", "hash-shared-thumb");
    fs::write(media_dir.path().join("shared_thumb.png"), b"shared-thumb-bytes").unwrap();

    // Item 100 (KEPT) and item 200 (DELETED) share the same thumbnail.
    insert_item(&conn, 100, "Kept Item", Some("shared_thumb.png"));
    insert_item(&conn, 200, "Deleted Item", Some("shared_thumb.png"));

    let ids = NonEmptyPlaylistItemIds::try_from(vec![200]).unwrap();
    let tx = conn.transaction().unwrap();
    let outcome = delete_playlist_items_db(&tx, &ids).unwrap();
    tx.commit().unwrap();

    assert!(outcome.removed_files.is_empty(), "shared thumbnail must not be removed");
    assert_eq!(outcome.kept_count, 1);
    assert!(media_exists(&conn, "shared_thumb.png"));
    assert!(media_dir.path().join("shared_thumb.png").exists());
    assert_eq!(table_count(&conn, "PlaylistItem"), 1);
}

#[test]
fn media_referenced_only_by_deleted_items_is_removed_from_db_and_disk() {
    let (_db_dir, db_path, media_dir) = fresh_db_and_media_dir();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    seed_accuracy(&conn);

    insert_media(&conn, 1, "orphan_thumb.png", "hash-orphan");
    fs::write(media_dir.path().join("orphan_thumb.png"), b"orphan-bytes").unwrap();
    insert_item(&conn, 300, "Deleted Item", Some("orphan_thumb.png"));

    let ids = NonEmptyPlaylistItemIds::try_from(vec![300]).unwrap();
    let tx = conn.transaction().unwrap();
    let outcome = delete_playlist_items_db(&tx, &ids).unwrap();
    tx.commit().unwrap();

    assert_eq!(outcome.removed_files, vec!["orphan_thumb.png".to_string()]);
    assert_eq!(outcome.kept_count, 0);
    assert!(!media_exists(&conn, "orphan_thumb.png"));

    // File removal happens ONLY via `remove_media_files`, in the apply path,
    // after commit — `delete_playlist_items_db` alone never touches disk.
    assert!(media_dir.path().join("orphan_thumb.png").exists());
    remove_media_files(media_dir.path(), &outcome.removed_files);
    assert!(!media_dir.path().join("orphan_thumb.png").exists());
}

#[test]
fn thumbnail_and_full_media_used_sets_are_evaluated_independently() {
    // A single file F is: item 400's (KEPT) THUMBNAIL, item 410's (KEPT)
    // FULL MEDIA (via PlaylistItemIndependentMediaMap), AND the DELETED item
    // 500 references F in BOTH roles at once (its own ThumbnailFilePath AND
    // its own media-map row). D8-07: the thumbnail loop protects F because
    // it finds F in `used_thumbs` (item 400's thumbnail); the full-media
    // loop INDEPENDENTLY protects the SAME F because it finds F in
    // `used_files` (item 410's media map) — proving the two used-sets are
    // evaluated separately, and that a file protected by BOTH is counted
    // exactly ONCE in `kept_count`, never twice.
    let (_db_dir, db_path, media_dir) = fresh_db_and_media_dir();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    seed_accuracy(&conn);

    insert_media(&conn, 1, "dual_role.png", "hash-dual-role");
    fs::write(media_dir.path().join("dual_role.png"), b"dual-role-bytes").unwrap();

    insert_item(&conn, 400, "Kept Item (uses as thumb)", Some("dual_role.png"));
    insert_item(&conn, 410, "Kept Item (uses as full media)", None);
    insert_media_map(&conn, 410, 1);
    insert_item(&conn, 500, "Deleted Item (uses both roles)", Some("dual_role.png"));
    insert_media_map(&conn, 500, 1);

    let ids = NonEmptyPlaylistItemIds::try_from(vec![500]).unwrap();
    let tx = conn.transaction().unwrap();
    let outcome = delete_playlist_items_db(&tx, &ids).unwrap();
    tx.commit().unwrap();

    assert!(outcome.removed_files.is_empty(), "shared-by-role media must survive");
    assert_eq!(outcome.kept_count, 1, "protected by both used-sets, but counted once");
    assert!(media_exists(&conn, "dual_role.png"));
    // Only item 500's own map row is gone; item 410's survives.
    assert_eq!(table_count(&conn, "PlaylistItemIndependentMediaMap"), 1);
}

#[test]
fn a_missing_on_disk_file_during_apply_does_not_fail_the_operation() {
    let (_db_dir, db_path, media_dir) = fresh_db_and_media_dir();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    seed_accuracy(&conn);

    insert_media(&conn, 1, "already_gone.png", "hash-already-gone");
    // Deliberately never written to `media_dir` — simulates a file already
    // missing from disk (matches Python's bare `except: pass`).
    insert_item(&conn, 600, "Deleted Item", Some("already_gone.png"));

    let ids = NonEmptyPlaylistItemIds::try_from(vec![600]).unwrap();
    let tx = conn.transaction().unwrap();
    let outcome = delete_playlist_items_db(&tx, &ids).unwrap();
    tx.commit().unwrap();

    assert_eq!(outcome.removed_files, vec!["already_gone.png".to_string()]);
    // Must not panic/error even though the file was never on disk.
    remove_media_files(media_dir.path(), &outcome.removed_files);
    assert!(!media_exists(&conn, "already_gone.png"));
}

#[test]
fn dry_run_leaves_every_media_file_present_on_disk_and_row_counts_unchanged() {
    let (_db_dir, db_path, media_dir) = fresh_db_and_media_dir();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    seed_accuracy(&conn);

    insert_media(&conn, 1, "would_be_removed.png", "hash-would-be-removed");
    fs::write(media_dir.path().join("would_be_removed.png"), b"bytes").unwrap();
    insert_item(&conn, 700, "Deleted Item", Some("would_be_removed.png"));

    let before_media = table_count(&conn, "IndependentMedia");
    let before_items = table_count(&conn, "PlaylistItem");

    let ids = NonEmptyPlaylistItemIds::try_from(vec![700]).unwrap();
    let report = dry_run_delete_playlist_items(&mut conn, &ids).unwrap();

    assert_eq!(report.media_removed, 1);
    assert_eq!(report.media_kept, 0);
    assert_eq!(table_count(&conn, "IndependentMedia"), before_media);
    assert_eq!(table_count(&conn, "PlaylistItem"), before_items);
    assert!(
        media_dir.path().join("would_be_removed.png").exists(),
        "dry run must never touch the filesystem"
    );
}

#[test]
fn dry_run_never_calls_remove_file() {
    // Structural guarantee (D8-07): `dry_run_delete_playlist_items` calls
    // ONLY `delete_playlist_items_db` and discards the returned path list —
    // asserted at the source level via `grep` in this plan's verification,
    // and behaviorally here by proving the file survives across a dry run.
    let (_db_dir, db_path, media_dir) = fresh_db_and_media_dir();
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    seed_accuracy(&conn);
    insert_media(&conn, 1, "untouched.png", "hash-untouched");
    fs::write(media_dir.path().join("untouched.png"), b"bytes").unwrap();
    insert_item(&conn, 800, "Deleted Item", Some("untouched.png"));

    let ids = NonEmptyPlaylistItemIds::try_from(vec![800]).unwrap();
    let _ = dry_run_delete_playlist_items(&mut conn, &ids).unwrap();

    assert!(media_dir.path().join("untouched.png").exists());
}
