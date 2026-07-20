//! Proves the v16-ONLY validity gate (finding 2, 01-07-PLAN.md): a v16
//! fixture is accepted, a v14 fixture is rejected as `UnsupportedSchema` —
//! v12-15 acceptance/upgrade is SCHEMA-01/02 in Phase 3.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::error::ArchiveError;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// A minimal, synthetic (never-real) v14 fixture built purely in test code:
/// a manifest declaring `schemaVersion: 14` is sufficient to trip the gate
/// before the (not otherwise valid) `userData.db` entry is ever opened.
fn generate_v14_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let archive_path = dir.path().join("synthetic-v14.jwlibrary");
    let file = std::fs::File::create(&archive_path).expect("create fixture archive");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    let manifest = r#"{"name":"JWL Manager v14 Fixture","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0,"userDataBackup":{"lastModifiedDate":"2026-01-01T00:00:00Z","deviceName":"JWL Manager Fixture_test","databaseName":"userData.db","hash":"0000000000000000000000000000000000000000000000000000000000000000","schemaVersion":14}}"#;
    writer
        .start_file("manifest.json", options)
        .expect("start manifest.json");
    writer
        .write_all(manifest.as_bytes())
        .expect("write manifest.json");

    // Presence-only placeholder — the schema-version gate rejects this
    // fixture from the manifest alone, before the DB is ever opened.
    writer
        .start_file("userData.db", options)
        .expect("start userData.db");
    writer
        .write_all(b"not-a-real-sqlite-file")
        .expect("write userData.db");

    writer.finish().expect("finish v14 fixture zip");
    (dir, archive_path)
}

#[test]
fn schema_v16_only() {
    let (_dir16, archive_path16) = common::generate_v16_fixture();
    let accepted = open_and_validate(&archive_path16, &dev_resources_db_path());
    assert!(
        accepted.is_ok(),
        "a v16 fixture must be accepted: {:?}",
        accepted.err()
    );

    let (_dir14, archive_path14) = generate_v14_fixture();
    let rejected = open_and_validate(&archive_path14, &dev_resources_db_path());
    match rejected {
        Err(ArchiveError::UnsupportedSchema { version }) => {
            assert_eq!(
                version, 14,
                "rejection must report the actual declared version"
            )
        }
        other => panic!("expected UnsupportedSchema for a v14 fixture, got {other:?}"),
    }
}
