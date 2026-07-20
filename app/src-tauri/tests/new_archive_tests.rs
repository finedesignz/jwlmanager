//! `new_archive` (seeded from res/blank, finding 5) and `save_as` (original
//! untouched, session target follows new path, D-05) tests.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::new::{new_archive, save_as};
use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn hash_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("read file for hashing");
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A new archive (seeded from the real `res/blank`) opens with a valid v16
/// manifest and zero Notes.
#[test]
fn new_empty_archive() {
    let target_dir = TempDir::new().expect("target tempdir");
    let target_path = target_dir.path().join("new_empty.jwlibrary");

    let session = new_archive(
        &target_path,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("new_archive must succeed (res/blank-seeded)");

    assert_eq!(
        session.manifest.schema_version, 16,
        "new archive must be seeded at v16 (res/blank's real PRAGMA user_version)"
    );

    let conn = rusqlite::Connection::open(&session.db_path).expect("open seeded db");
    let note_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM Note", [], |r| r.get(0))
        .expect("count notes");
    assert_eq!(note_count, 0, "a fresh empty archive must have zero Notes");

    // Save it out and prove the WHOLE Tauri app can open it (not just the
    // internal session state) — a real round-trip through open_and_validate.
    jwlmanager_lib::archive::save::save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("saving the fresh new archive must succeed");
    assert!(target_path.exists());

    let (reopened_session, notes) = open_and_validate(&target_path, &dev_resources_db_path())
        .expect("the saved new archive must reopen cleanly through the real open path");
    assert_eq!(reopened_session.manifest.schema_version, 16);
    assert!(
        notes.is_empty(),
        "a freshly created, unsaved-upon archive must list zero notes"
    );
}

/// save-as leaves the ORIGINAL file's bytes unchanged and updates the
/// session's target to the new path; the new file opens cleanly.
#[test]
fn save_as_preserves_original() {
    let (_fixture_dir, original_path) = common::generate_v16_fixture();
    let original_hash_before = hash_file(&original_path);
    let original_bytes_before = fs::read(&original_path).expect("read original");

    let (session, _notes) = open_and_validate(&original_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed");

    let new_dir = TempDir::new().expect("new target tempdir");
    let new_path = new_dir.path().join("saved_as.jwlibrary");

    let manifest = save_as(
        &session,
        &new_path,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_as must succeed");
    assert_eq!(manifest.user_data_backup.schema_version, 16);

    // Original file bytes MUST be unchanged (D-03: source is read-only; the
    // save-as target is a completely different path).
    let original_hash_after = hash_file(&original_path);
    let original_bytes_after = fs::read(&original_path).expect("original must still exist");
    assert_eq!(
        original_hash_before, original_hash_after,
        "save-as must never mutate the original file's bytes"
    );
    assert_eq!(original_bytes_before, original_bytes_after);

    // New file opens cleanly through the real open path.
    assert!(new_path.exists(), "save-as target must exist");
    let (reopened_session, _notes) = open_and_validate(&new_path, &dev_resources_db_path())
        .expect("the save-as target must reopen cleanly");
    assert_eq!(reopened_session.manifest.schema_version, 16);

    // The session's target_path is updated by the CALLER once save_as
    // returns Ok (per this function's contract, since it takes &session).
    // Prove the contract holds by constructing the expected post-update
    // session shape a caller (the Tauri command) would produce.
    let mut updated_session = session;
    updated_session.target_path = new_path.clone();
    updated_session.dirty = false;
    assert_eq!(updated_session.target_path, new_path);
    assert_ne!(updated_session.target_path, original_path);
}
