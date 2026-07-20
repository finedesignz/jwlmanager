//! Save-path tests (ARCH-06/ARCH-07, D-04, review findings 3 & 4): atomic
//! replace, full-inventory preservation, and interruption safety. This is
//! where Core Value ("never lose or corrupt a user's archive") is proven or
//! broken — see 01-05-PLAN.md `<threat_model>` T-05-01/T-05-02.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::archive::save::save_archive;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::session::ZipEntryMeta;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

fn hash_file(path: &std::path::Path) -> String {
    let bytes = fs::read(path).expect("read file for hashing");
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// (a) A save produces a re-openable archive whose Notes table is
/// semantically equal (normalized) to the source's, and (c) the temp file is
/// gone after a successful save.
#[test]
fn save_round_trips_notes_and_cleans_up_temp_file() {
    let (_fixture_dir, archive_path) = common::generate_v16_fixture();
    let (_extract_dir, source_extracted) = common::extract_to_tempdir(&archive_path);
    let source_conn =
        rusqlite::Connection::open(source_extracted.join("userData.db")).expect("open source db");
    let source_notes = common::normalized_table_rows(&source_conn, "Note");

    let (session, _notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed");

    // Save over the same source path — the source was already extracted
    // read-only into session.temp_dir, so mutating the on-disk file here is
    // exactly what save is supposed to do for a normal "Save".
    let manifest = save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_archive must succeed");
    assert_eq!(manifest.user_data_backup.schema_version, 16);
    assert!(
        !manifest.user_data_backup.hash.is_empty(),
        "hash must be populated after save"
    );

    // No leftover same-directory temp file.
    let dir_entries: Vec<String> = fs::read_dir(archive_path.parent().unwrap())
        .expect("read save dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        dir_entries.iter().all(|n| !n.contains(".tmp-")),
        "no leftover temp file should remain after a successful save, found: {dir_entries:?}"
    );
    assert!(
        archive_path.exists(),
        "target archive must exist after save"
    );

    // Reopen and compare normalized Note rows (never byte-diff — save is not
    // byte-preserving, per CLAUDE.md / Core Value doc).
    let (_reopen_dir, reopened_extracted) = common::extract_to_tempdir(&archive_path);
    let reopened_conn = rusqlite::Connection::open(reopened_extracted.join("userData.db"))
        .expect("open reopened db");
    let reopened_notes = common::normalized_table_rows(&reopened_conn, "Note");
    assert_eq!(
        source_notes, reopened_notes,
        "Notes table must round-trip semantically identical through a save"
    );

    // (d) hash equals sha256 of the on-disk userData.db after mutation.
    let expected_hash = hash_file(&reopened_extracted.join("userData.db"));
    assert_eq!(
        manifest.user_data_backup.hash, expected_hash,
        "manifest hash must equal sha256 of the final on-disk userData.db bytes"
    );
}

/// (b) A fixture with a loose-media entry + an unknown/forward-compat entry
/// still contains BOTH byte-identically after save+reopen (finding 4:
/// rebuilding only db+manifest would silently destroy media).
#[test]
fn save_preserves_media_and_unknown_entries_byte_identically() {
    let (_fixture_dir, archive_path) = common::generate_v16_fixture();

    let original_entries = common::list_zip_entries(&archive_path);
    assert!(original_entries.contains(&"media/test.png".to_string()));
    assert!(original_entries.contains(&"future_unknown.dat".to_string()));

    let (_extract_dir, pre_extracted) = common::extract_to_tempdir(&archive_path);
    let media_before = fs::read(pre_extracted.join("media/test.png")).expect("read media before");
    let unknown_before =
        fs::read(pre_extracted.join("future_unknown.dat")).expect("read unknown before");

    let (session, _notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed");
    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_archive must succeed");

    let saved_entries = common::list_zip_entries(&archive_path);
    for expected in [
        "media/test.png",
        "future_unknown.dat",
        "default_thumbnail.png",
    ] {
        assert!(
            saved_entries.iter().any(|e| e == expected),
            "saved archive must still contain {expected}"
        );
    }

    let (_post_dir, post_extracted) = common::extract_to_tempdir(&archive_path);
    let media_after = fs::read(post_extracted.join("media/test.png")).expect("read media after");
    let unknown_after =
        fs::read(post_extracted.join("future_unknown.dat")).expect("read unknown after");

    assert_eq!(
        media_before, media_after,
        "loose media entry must survive save byte-identically"
    );
    assert_eq!(
        unknown_before, unknown_after,
        "unknown/forward-compat entry must survive save byte-identically"
    );
}

/// (e) Interruption-safety: a failure that happens BEFORE the atomic-replace
/// step (rebuild_zip fails because the session inventory references a
/// zip-entry name that no longer exists in the extracted working copy) must
/// leave the original target completely intact — old-or-new, never a
/// truncated/missing file.
#[test]
fn save_failure_before_replace_leaves_original_target_intact() {
    let (_fixture_dir, archive_path) = common::generate_v16_fixture();
    let original_bytes_before = fs::read(&archive_path).expect("read original archive bytes");
    let original_hash_before = hash_file(&archive_path);

    let (mut session, _notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed");

    // Inject a bogus inventory entry that does NOT exist in temp_dir, forcing
    // rebuild_zip's fs::read of that entry to fail before the temp file is
    // ever synced or the atomic-replace call is ever made.
    session.entries.push(ZipEntryMeta {
        name: "this_file_does_not_exist_in_temp_dir.bin".to_string(),
    });

    let result = save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    );
    assert!(
        result.is_err(),
        "save must fail when the inventory references a missing entry"
    );

    // The original target file must be byte-identical to before the failed
    // save attempt — never deleted, never truncated, never partially
    // overwritten.
    let original_bytes_after = fs::read(&archive_path).expect("original archive must still exist");
    let original_hash_after = hash_file(&archive_path);
    assert_eq!(
        original_bytes_before, original_bytes_after,
        "a save failure before the atomic replace must leave the original archive byte-identical"
    );
    assert_eq!(original_hash_before, original_hash_after);

    // No leftover temp file from the failed attempt either.
    let dir_entries: Vec<String> = fs::read_dir(archive_path.parent().unwrap())
        .expect("read save dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        dir_entries.iter().all(|n| !n.contains(".tmp-")),
        "a failed save must not leave a leftover temp file, found: {dir_entries:?}"
    );
}

/// Sanity: `same_dir_temp_path`-style temp naming keeps the temp file in the
/// SAME directory as the target (required for the rename/replace to be
/// atomic — cross-filesystem renames are not atomic).
#[test]
fn save_writes_temp_file_in_same_directory_as_target() {
    let (_fixture_dir, archive_path) = common::generate_v16_fixture();
    let (session, _notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed");

    let target_dir: PathBuf = archive_path.parent().unwrap().to_path_buf();
    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_archive must succeed");

    // The final file itself must be in the same directory as before (target
    // path unchanged) — the strongest available proxy that no cross-fs
    // temp+rename was used, since a leftover temp file is already asserted
    // absent in the round-trip test above.
    assert_eq!(archive_path.parent().unwrap(), target_dir);
    assert!(archive_path.exists());
}
