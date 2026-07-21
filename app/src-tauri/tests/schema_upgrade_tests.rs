//! Full schema-upgrade test matrix (SCHEMA-01/02, 03-02-PLAN.md): the widened
//! 12-16 range gate (both `archive/mod.rs` and `archive/manifest.rs`),
//! transactional `upgrade_to_v16`, and the post-upgrade v16 contract
//! validator. This is the data-integrity core of Phase 3 — every behavior
//! here maps to a threat register entry in 03-02-PLAN.md's `<threat_model>`.
//!
//! Does NOT include the Python differential row — that is 03-03.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::archive::save::save_archive;
use jwlmanager_lib::archive::upgrade::{upgrade_to_v16, validate_v16_contract};
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Gate accept/reject range
// ---------------------------------------------------------------------------

type FixtureGenerator = fn() -> (tempfile::TempDir, std::path::PathBuf);

#[test]
fn test_gate_accepts_v12_through_v16() {
    let generators: Vec<(i64, FixtureGenerator)> = vec![
        (12, common::generate_v12_fixture),
        (13, common::generate_v13_fixture),
        (14, common::generate_v14_fixture),
        (15, common::generate_v15_fixture),
        (16, common::generate_v16_fixture),
    ];
    for (version, generator) in generators {
        let (_dir, archive_path) = generator();
        let result = open_and_validate(&archive_path, &dev_resources_db_path());
        assert!(
            result.is_ok(),
            "v{version} fixture must be accepted: {:?}",
            result.err()
        );
        let (session, _notes) = result.unwrap();
        assert_eq!(
            session.manifest.schema_version, 16,
            "v{version} session must report schema_version 16 after normalize/upgrade"
        );
    }
}

#[test]
fn test_gate_rejects_v11() {
    let (_dir, archive_path) = common::generate_v11_fixture();
    let result = open_and_validate(&archive_path, &dev_resources_db_path());
    match result {
        Err(ArchiveError::SchemaTooOld { version }) => assert_eq!(version, 11),
        other => panic!("expected SchemaTooOld {{ version: 11 }}, got {other:?}"),
    }
}

#[test]
fn test_gate_rejects_v17() {
    let (_dir, archive_path) = common::generate_fixture_v17_shape();
    let result = open_and_validate(&archive_path, &dev_resources_db_path());
    match result {
        Err(ArchiveError::SchemaTooNew { version }) => assert_eq!(version, 17),
        other => panic!("expected SchemaTooNew {{ version: 17 }}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Upgrade mechanics
// ---------------------------------------------------------------------------

#[test]
fn test_upgrade_v14_to_v16() {
    let (_dir, archive_path) = common::generate_v14_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    let conn = Connection::open(&session.db_path).expect("open working db");
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 16);

    let mut has_specialty = false;
    let mut has_edition = false;
    let mut stmt = conn.prepare("PRAGMA table_info(Location)").unwrap();
    let cols = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect::<Vec<_>>();
    for c in &cols {
        if c == "Specialty" {
            has_specialty = true;
        }
        if c == "Edition" {
            has_edition = true;
        }
    }
    assert!(
        has_specialty && has_edition,
        "Location must have Specialty+Edition after upgrade"
    );

    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='IX_Location_Media'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 1, "IX_Location_Media must exist after upgrade");
}

#[test]
fn test_upgrade_noop_on_v16() {
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");
    let conn = Connection::open(&session.db_path).expect("open working db");
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 16);
    let note_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM Note", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        note_count, 2,
        "notes must be intact, unchanged (idempotent no-op)"
    );
}

#[test]
fn test_upgrade_rejects_direct_call_above_v16() {
    let (_dir, archive_path) = common::generate_fixture_v17_shape();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let mut conn = Connection::open(extracted.join("userData.db")).expect("open extracted db");

    let result = upgrade_to_v16(&mut conn);
    match result {
        Err(ArchiveError::SchemaTooNew { version }) => assert_eq!(version, 17),
        other => panic!("expected SchemaTooNew {{ version: 17 }}, got {other:?}"),
    }

    // A v16 DB direct call is Ok with no mutation (belt-and-suspenders).
    let (_dir16, archive_path16) = common::generate_v16_fixture();
    let (_zip_dir16, extracted16) = common::extract_to_tempdir(&archive_path16);
    let mut conn16 = Connection::open(extracted16.join("userData.db")).expect("open extracted db");
    let result16 = upgrade_to_v16(&mut conn16);
    assert!(result16.is_ok(), "direct call on v16 DB must be Ok no-op");
}

#[test]
fn test_upgrade_skips_existing_columns() {
    // Take a v16 fixture DB (already has Specialty/Edition), rewind its
    // user_version to 14, then upgrade — must not error on "duplicate column".
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");
    {
        let conn = Connection::open(&db_path).expect("open extracted db");
        conn.pragma_update(None, "user_version", 14_i64).unwrap();
    }
    let mut conn = Connection::open(&db_path).expect("reopen extracted db");
    let result = upgrade_to_v16(&mut conn);
    assert!(
        result.is_ok(),
        "upgrade must skip already-present columns: {result:?}"
    );
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 16);
}

#[test]
fn test_upgrade_preserves_existing_specialty_edition() {
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");
    {
        let conn = Connection::open(&db_path).expect("open extracted db");
        conn.execute(
            "UPDATE Location SET Specialty = 'known-specialty-1', Edition = 'known-edition-1' \
             WHERE LocationId = 1",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 14_i64).unwrap();
    }
    let mut conn = Connection::open(&db_path).expect("reopen extracted db");
    upgrade_to_v16(&mut conn).expect("upgrade must succeed");

    let (specialty, edition): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT Specialty, Edition FROM Location WHERE LocationId = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(specialty.as_deref(), Some("known-specialty-1"));
    assert_eq!(edition.as_deref(), Some("known-edition-1"));
}

#[test]
fn test_upgrade_preserves_representative_location_types() {
    let (_dir, archive_path) = common::generate_v14_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let before_count: i64 = {
        let conn = Connection::open(&db_path).unwrap();
        conn.query_row("SELECT COUNT(*) FROM Location", [], |r| r.get(0))
            .unwrap()
    };

    let mut conn = Connection::open(&db_path).expect("open extracted db");
    upgrade_to_v16(&mut conn).expect("upgrade must succeed");

    let after_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM Location", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        before_count, after_count,
        "Location row count must be unchanged"
    );

    // Representative rows seeded by insert_representative_locations: 20-25.
    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT LocationId FROM Location WHERE LocationId BETWEEN 20 AND 25 ORDER BY LocationId")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert_eq!(
        ids,
        vec![20, 21, 22, 23, 24, 25],
        "all representative Location rows must survive"
    );
}

#[test]
fn test_foreign_keys_off_around_rebuild() {
    // upgrade_to_v16 must explicitly disable foreign_keys before the
    // DROP TABLE Location rebuild (this build's bundled SQLite does not
    // default to OFF, per 03-01-SUMMARY.md's empirical finding) and must
    // never leave it enabled afterward — it only ever disables, never
    // enables (finding 6's actual constraint).
    let (_dir, archive_path) = common::generate_v14_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let mut conn = Connection::open(extracted.join("userData.db")).expect("open extracted db");
    upgrade_to_v16(&mut conn).expect("upgrade must succeed");
    let fk: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        fk, 0,
        "foreign_keys must be OFF after the rebuild, never re-enabled"
    );
}

#[test]
fn test_upgrade_failure_is_typed_error() {
    let (_dir, archive_path) = common::generate_v14_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");
    {
        // Poison the pre-state: rename Location away so the DDL cannot find it.
        let conn = Connection::open(&db_path).expect("open extracted db");
        conn.execute_batch("ALTER TABLE Location RENAME TO Location_poisoned;")
            .unwrap();
    }
    let mut conn = Connection::open(&db_path).expect("reopen extracted db");
    let result = upgrade_to_v16(&mut conn);
    match result {
        Err(ArchiveError::SchemaUpgradeFailed { .. }) => {}
        other => panic!("expected SchemaUpgradeFailed, got {other:?}"),
    }
}

#[test]
fn test_upgrade_rollback_leaves_original_version() {
    let (_dir, archive_path) = common::generate_v14_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");
    {
        let conn = Connection::open(&db_path).expect("open extracted db");
        conn.execute_batch("ALTER TABLE Location RENAME TO Location_poisoned;")
            .unwrap();
    }
    let mut conn = Connection::open(&db_path).expect("reopen extracted db");
    let before_rows = common::normalized_table_rows(&conn, "Location_poisoned");
    let result = upgrade_to_v16(&mut conn);
    assert!(result.is_err());
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        version, 14,
        "PRAGMA user_version must be unchanged after a rolled-back failure"
    );
    let after_rows = common::normalized_table_rows(&conn, "Location_poisoned");
    assert_eq!(
        before_rows, after_rows,
        "Location_poisoned rows must be unchanged after rollback"
    );
}

// ---------------------------------------------------------------------------
// Post-upgrade contract validator
// ---------------------------------------------------------------------------

#[test]
fn test_post_upgrade_contract_rejects_incomplete_db() {
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");
    {
        // Drop the required unique index post-upgrade to simulate an
        // incomplete v13-ish DB that was mislabeled v16.
        let conn = Connection::open(&db_path).expect("open extracted db");
        conn.execute_batch("DROP INDEX IX_Location_Media;").unwrap();
    }
    let conn = Connection::open(&db_path).expect("reopen extracted db");
    let result = validate_v16_contract(&conn);
    assert!(
        result.is_err(),
        "contract validator must reject a DB missing IX_Location_Media"
    );
}

// ---------------------------------------------------------------------------
// In-range mismatch normalization
// ---------------------------------------------------------------------------

#[test]
fn test_in_range_manifest_pragma_mismatch_normalizes_manifest_low() {
    // manifest schemaVersion == 14, DB PRAGMA == 16.
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    // Rewrite the manifest.json inside the ALREADY-BUILT archive to claim 14
    // while the DB stays 16, then re-zip via a fresh archive using the same
    // helper shape is overkill here — instead directly craft the fixture.
    drop(extracted);

    // Build a fixture: v16 DB, manifest claims 14.
    let (mismatch_dir, mismatch_path) = build_mismatch_fixture(16, 14);
    let result = open_and_validate(&mismatch_path, &dev_resources_db_path());
    assert!(
        result.is_ok(),
        "in-range mismatch must normalize, not reject: {:?}",
        result.err()
    );
    let (session, _notes) = result.unwrap();
    assert_eq!(session.manifest.schema_version, 16);
    drop(mismatch_dir);
}

#[test]
fn test_in_range_manifest_pragma_mismatch_normalizes_db_low() {
    // manifest schemaVersion == 16, DB PRAGMA == 14 -> must upgrade to 16.
    let (mismatch_dir, mismatch_path) = build_mismatch_fixture(14, 16);
    let result = open_and_validate(&mismatch_path, &dev_resources_db_path());
    assert!(
        result.is_ok(),
        "in-range mismatch must normalize (upgrade), not reject: {:?}",
        result.err()
    );
    let (session, _notes) = result.unwrap();
    assert_eq!(session.manifest.schema_version, 16);
    drop(mismatch_dir);
}

/// Builds a fixture whose DB PRAGMA user_version is `db_version` (12-16,
/// upgraded from a v14-shape fixture if lower) while the manifest declares
/// `manifest_version` — used to test finding-4 mismatch normalization.
fn build_mismatch_fixture(
    db_version: i64,
    manifest_version: i64,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let (_dir, source_path) = if db_version == 16 {
        common::generate_v16_fixture()
    } else {
        common::generate_fixture_pre_v16_shape(db_version)
    };
    let (extract_dir, extracted) = common::extract_to_tempdir(&source_path);
    let db_bytes = std::fs::read(extracted.join("userData.db")).unwrap();

    let archive_dir = tempfile::TempDir::new().unwrap();
    let archive_path = archive_dir.path().join("mismatch.jwlibrary");
    let file = std::fs::File::create(&archive_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();

    let manifest_json = format!(
        r#"{{"name":"JWL Manager Fixture","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0,"userDataBackup":{{"lastModifiedDate":"2026-01-01T00:00:00Z","deviceName":"JWL Manager Fixture_test","databaseName":"userData.db","hash":"0000000000000000000000000000000000000000000000000000000000000000","schemaVersion":{manifest_version}}}}}"#
    );
    writer.start_file("manifest.json", options).unwrap();
    std::io::Write::write_all(&mut writer, manifest_json.as_bytes()).unwrap();

    writer.start_file("userData.db", options).unwrap();
    std::io::Write::write_all(&mut writer, &db_bytes).unwrap();

    let thumb_path = extracted.join("default_thumbnail.png");
    if thumb_path.exists() {
        let thumb_bytes = std::fs::read(thumb_path).unwrap();
        writer.start_file("default_thumbnail.png", options).unwrap();
        std::io::Write::write_all(&mut writer, &thumb_bytes).unwrap();
    }

    writer.finish().unwrap();
    drop(extract_dir);
    (archive_dir, archive_path)
}

// ---------------------------------------------------------------------------
// Source-unchanged + reopen round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_source_file_unchanged_after_open() {
    let (_dir, archive_path) = common::generate_v14_fixture();
    let before = common::read_file_bytes(&archive_path);
    let _ = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");
    let after = common::read_file_bytes(&archive_path);
    assert_eq!(
        before, after,
        "source file must be byte-identical after an upgrade-on-open"
    );
}

#[test]
fn test_saved_upgraded_archive_reopen_round_trip() {
    let (_dir, archive_path) = common::generate_v14_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    let pre_save_notes = {
        let conn = Connection::open(&session.db_path).unwrap();
        common::normalized_table_rows(&conn, "Note")
    };

    let manifest = save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save must succeed");
    assert_eq!(manifest.user_data_backup.schema_version, 16);

    let (reopened_session, _reopened_notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("reopen must succeed");
    assert_eq!(reopened_session.manifest.schema_version, 16);

    let conn = Connection::open(&reopened_session.db_path).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 16);

    let mut has_specialty = false;
    let mut stmt = conn.prepare("PRAGMA table_info(Location)").unwrap();
    for c in stmt.query_map([], |r| r.get::<_, String>(1)).unwrap() {
        if c.unwrap() == "Specialty" {
            has_specialty = true;
        }
    }
    assert!(
        has_specialty,
        "reopened archive Location must have Specialty column"
    );

    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='IX_Location_Media'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 1);

    let post_save_notes = common::normalized_table_rows(&conn, "Note");
    assert_eq!(
        pre_save_notes, post_save_notes,
        "notes must be unchanged across save+reopen"
    );

    // Saved manifest.json on disk (inside the archive) must read schemaVersion 16.
    let (_reopen_extract_dir, reopened_extracted) = common::extract_to_tempdir(&archive_path);
    let manifest_bytes = std::fs::read(reopened_extracted.join("manifest.json")).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest_json["userDataBackup"]["schemaVersion"], 16);
}
