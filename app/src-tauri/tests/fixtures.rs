//! Tests for the synthetic fixture generators (QA-01, D-06, D-08). The
//! generators themselves live in `tests/common/mod.rs` so both this file and
//! `tests/open_archive_tests.rs` can reuse them without duplicating the
//! res/blank-seeding logic.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use rusqlite::Connection;
use std::path::Path;
use std::process::Command;

#[test]
fn test_fixture_generator_produces_valid_v16_archive() {
    let (_dir, archive_path) = common::generate_v16_fixture();
    assert!(archive_path.exists(), "fixture archive must exist on disk");

    let entries = common::list_zip_entries(&archive_path);
    for expected in [
        "manifest.json",
        "userData.db",
        "default_thumbnail.png",
        "media/test.png",
        "future_unknown.dat",
    ] {
        assert!(
            entries.iter().any(|e| e == expected),
            "fixture missing expected entry: {expected}"
        );
    }

    let (_extract_dir, extracted_path) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted_path.join("userData.db"))
        .expect("extracted userData.db must open");
    let schema_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("PRAGMA user_version must succeed");
    assert_eq!(
        schema_version, 16,
        "fixture must be seeded from a v16 database"
    );
}

#[test]
fn test_fixture_contains_located_and_independent_notes() {
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_extract_dir, extracted_path) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted_path.join("userData.db")).expect("db must open");

    let located: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Note WHERE LocationId IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("located-note count query");
    assert!(
        located >= 1,
        "fixture must contain at least one located note"
    );

    let independent: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Note WHERE LocationId IS NULL AND BlockType = 0",
            [],
            |row| row.get(0),
        )
        .expect("independent-note count query");
    assert!(
        independent >= 1,
        "fixture must contain at least one independent note (RESEARCH.md Pitfall 2)"
    );
}

#[test]
fn test_zip_slip_fixtures_cover_all_six_variants() {
    for variant in [
        common::ZipSlipVariant::UnixTraversal,
        common::ZipSlipVariant::AbsoluteUnix,
        common::ZipSlipVariant::AbsoluteWindows,
        common::ZipSlipVariant::BackslashTraversal,
        common::ZipSlipVariant::DuplicateEntry,
        common::ZipSlipVariant::SymlinkChain,
    ] {
        let (_dir, archive_path) = common::generate_zip_slip_fixture(variant);
        assert!(
            archive_path.exists(),
            "zip-slip fixture for {variant:?} must be generated"
        );
    }
}

/// Asserts a versioned pre-v16 fixture (v11-v15) has the reverse-mutated
/// pre-v16 shape: DB `PRAGMA user_version` matches, manifest `schemaVersion`
/// matches (D3-07 cross-check), `Location` lacks `Specialty`/`Edition`, and
/// `IX_Location_Media` is absent.
fn assert_pre_v16_shape(version: i64, archive_path: &Path) {
    let (_extract_dir, extracted_path) = common::extract_to_tempdir(archive_path);
    let conn = Connection::open(extracted_path.join("userData.db")).expect("db must open");

    let schema_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("PRAGMA user_version must succeed");
    assert_eq!(schema_version, version, "user_version must equal {version}");

    let columns: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(Location)")
            .expect("table_info(Location) must succeed");
        stmt.query_map([], |row| row.get::<_, String>(1))
            .expect("query table_info")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect column names")
    };
    assert!(
        !columns.iter().any(|c| c == "Specialty"),
        "v{version} fixture Location must NOT have Specialty column, got {columns:?}"
    );
    assert!(
        !columns.iter().any(|c| c == "Edition"),
        "v{version} fixture Location must NOT have Edition column, got {columns:?}"
    );

    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'IX_Location_Media'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master index query");
    assert_eq!(
        index_count, 0,
        "v{version} fixture must NOT have IX_Location_Media index"
    );

    // Manifest schemaVersion cross-check (D3-07): read manifest.json straight
    // out of the zip (not the extracted db dir).
    let file = std::fs::File::open(archive_path).expect("open archive for manifest read");
    let mut zip = zip::ZipArchive::new(file).expect("read zip archive");
    let mut manifest_entry = zip.by_name("manifest.json").expect("manifest.json entry");
    let mut manifest_str = String::new();
    std::io::Read::read_to_string(&mut manifest_entry, &mut manifest_str)
        .expect("read manifest.json");
    let expected_fragment = format!("\"schemaVersion\":{version}");
    assert!(
        manifest_str.contains(&expected_fragment),
        "manifest.json must declare schemaVersion {version}, got: {manifest_str}"
    );
}

/// Asserts the seeded `Location` table contains the representative Type
/// coverage from 03-REVIEWS.md finding 5: scripture (Type 0, Book+KeySymbol),
/// publication/document (Type 0, DocumentId), media/track (Type 0, Track),
/// Type 1, Type 2, and Type 3.
fn assert_representative_location_coverage(archive_path: &Path) {
    let (_extract_dir, extracted_path) = common::extract_to_tempdir(archive_path);
    let conn = Connection::open(extracted_path.join("userData.db")).expect("db must open");

    let scripture: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE Type = 0 AND BookNumber IS NOT NULL \
             AND BookNumber != 0 AND KeySymbol IS NOT NULL AND length(KeySymbol) > 0",
            [],
            |row| row.get(0),
        )
        .expect("scripture coverage query");
    assert!(
        scripture >= 1,
        "must have a scripture (Type 0) Location row"
    );

    let publication: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE Type = 0 AND DocumentId IS NOT NULL \
             AND DocumentId != 0",
            [],
            |row| row.get(0),
        )
        .expect("publication coverage query");
    assert!(
        publication >= 1,
        "must have a publication/document (Type 0, DocumentId) Location row"
    );

    let media: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE Type = 0 AND Track IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("media coverage query");
    assert!(
        media >= 1,
        "must have a media/track (Type 0, Track) Location row"
    );

    for type_num in [1, 2, 3] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM Location WHERE Type = ?1",
                [type_num],
                |row| row.get(0),
            )
            .expect("Type coverage query");
        assert!(count >= 1, "must have a Type {type_num} Location row");
    }
}

#[test]
fn test_pre_v16_fixtures_have_correct_version_and_shape() {
    for version in 11..16 {
        let (_dir, archive_path) = common::generate_fixture_pre_v16_shape(version);
        assert!(
            archive_path.exists(),
            "v{version} fixture must exist on disk"
        );
        assert_pre_v16_shape(version, &archive_path);
        assert_representative_location_coverage(&archive_path);
    }
}

#[test]
fn test_v17_fixture_reports_out_of_range_version() {
    let (_dir, archive_path) = common::generate_fixture_v17_shape();
    assert!(archive_path.exists(), "v17 fixture must exist on disk");

    let (_extract_dir, extracted_path) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted_path.join("userData.db")).expect("db must open");
    let schema_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("PRAGMA user_version must succeed");
    assert_eq!(
        schema_version, 17,
        "v17 fixture must report user_version 17"
    );

    let file = std::fs::File::open(&archive_path).expect("open archive for manifest read");
    let mut zip = zip::ZipArchive::new(file).expect("read zip archive");
    let mut manifest_entry = zip.by_name("manifest.json").expect("manifest.json entry");
    let mut manifest_str = String::new();
    std::io::Read::read_to_string(&mut manifest_entry, &mut manifest_str)
        .expect("read manifest.json");
    assert!(
        manifest_str.contains("\"schemaVersion\":17"),
        "manifest.json must declare schemaVersion 17, got: {manifest_str}"
    );

    assert_representative_location_coverage(&archive_path);
}

/// GDPR Art. 9 bright line (D-06): fail the build if any real `.jwlibrary`
/// file is ever tracked in git. Uses `git ls-files` rather than a filesystem
/// walk so untracked local scratch files (e.g. under target/) never trip it.
#[test]
fn test_no_real_archive_is_tracked_in_git() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .arg("ls-files")
        .arg("*.jwlibrary")
        .output()
        .expect("failed to run git ls-files");
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tracked = String::from_utf8_lossy(&output.stdout);
    assert!(
        tracked.trim().is_empty(),
        "GDPR Art. 9 bright line violated — real/scrubbed .jwlibrary file(s) tracked in git: {tracked}"
    );
}
