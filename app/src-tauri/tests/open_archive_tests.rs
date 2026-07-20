//! End-to-end test for the `open_archive` core primitive.
//!
//! Formerly RED and `#[ignore]`d (01-01): the test body only exercised the
//! fixture directly because `open_archive` didn't exist yet. 01-07 lands
//! `archive::open_and_validate` (the logic the `open_archive` Tauri command
//! wraps) and this test now drives it for real, un-ignored and green.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;

/// The res/blank-seeded synthetic fixture, once opened through the real
/// `open_and_validate` primitive, yields a Notes list with at least one row
/// and a fully populated `ArchiveSession` (finding 1, 01-07-PLAN.md).
#[test]
fn test_open_archive_lists_at_least_one_note() {
    let (_dir, archive_path) = common::generate_v16_fixture();

    let (session, notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed for a v16 fixture");

    assert!(
        !notes.is_empty(),
        "fixture must list at least one note once opened"
    );

    // ArchiveSession must be fully populated (TempDir kept alive, full zip
    // entry inventory, dirty=false) — not a bare Vec<NotesRow>.
    assert_eq!(session.manifest.schema_version, 16);
    assert!(
        session.entries.len() >= 5,
        "session must inventory every original zip entry (manifest, db, thumbnail, media, unknown), got {}",
        session.entries.len()
    );
    assert!(
        session.db_path.exists(),
        "extracted userData.db must exist on disk"
    );
    assert!(!session.dirty, "a freshly opened archive must not be dirty");
    assert_eq!(session.source_path, archive_path);
    assert_eq!(session.target_path, archive_path);
}
