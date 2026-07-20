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
