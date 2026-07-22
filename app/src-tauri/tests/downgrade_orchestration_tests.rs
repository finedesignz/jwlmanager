//! Throwaway-copy v14-save orchestration tests (SCHEMA-03/05, 04-02-PLAN.md):
//! the live session provably stays byte-identical at v16 while a v14 copy is
//! written to a chosen path (MED-5), the dry-run preview matches the actual save
//! under trim-first order (HIGH-2), dedup-DELETED study rows surface as data
//! loss (HIGH-3), and an un-downgradeable archive fails with a typed error and
//! writes nothing (HIGH-1 end-to-end).
//!
//! Semantic assertions only — never byte-diff the archive (save is not
//! byte-preserving); the ONLY byte-hash assertion is on the LIVE session DB,
//! which MUST be untouched.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::downgrade::{dry_run_downgrade, save_v14_copy};
use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const APP: &str = "JWL Manager";
const DEVICE: &str = "JWL Manager_test";
const NOW: &str = "2026-01-02T00:00:00Z";

fn hash_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("read file for hashing");
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn user_version_of(db_path: &Path) -> i64 {
    let conn = Connection::open(db_path).unwrap();
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn location_ids_of(db_path: &Path) -> Vec<i64> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT LocationId FROM Location ORDER BY LocationId")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn has_column(db_path: &Path, column: &str) -> bool {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn.prepare("PRAGMA table_info(Location)").unwrap();
    let found = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .any(|c| c.unwrap() == column);
    found
}

// ---------------------------------------------------------------------------
// 1. Session stays v16 + byte-untouched; output is a valid merged v14 (MED-5)
// ---------------------------------------------------------------------------

#[test]
fn test_session_stays_v16_and_bytes_untouched_output_is_v14() {
    let (_fx, archive) = common::generate_v16_dryrun_collision_archive();
    let (session, _notes) =
        open_and_validate(&archive, &dev_resources_db_path()).expect("open must succeed");

    let session_hash_before = hash_file(&session.db_path);
    assert_eq!(session.manifest.schema_version, 16);

    let out_dir = TempDir::new().unwrap();
    let target = out_dir.path().join("export_v14.jwlibrary");
    save_v14_copy(&session, &target, APP, DEVICE, NOW).expect("v14 save must succeed");

    // LIVE session untouched: same v16, same manifest meta, byte-identical DB.
    assert_eq!(
        user_version_of(&session.db_path),
        16,
        "session must stay v16"
    );
    assert_eq!(session.manifest.schema_version, 16);
    assert_eq!(
        hash_file(&session.db_path),
        session_hash_before,
        "session.db_path bytes MUST be identical after a v14 side-export (MED-5)"
    );

    // OUTPUT is a valid downgraded v14 archive: uv 14, Specialty/Edition gone,
    // collision group merged to the lowest-id survivor (20).
    let (_od, out_extract) = common::extract_to_tempdir(&target);
    let out_db = out_extract.join("userData.db");
    assert_eq!(user_version_of(&out_db), 14, "output must be v14");
    assert!(!has_column(&out_db, "Specialty"));
    assert!(!has_column(&out_db, "Edition"));
    assert_eq!(
        location_ids_of(&out_db),
        vec![20],
        "merged to lowest-id survivor 20"
    );
}

// ---------------------------------------------------------------------------
// 2. Dry-run matches save under trim-first order (HIGH-2)
// ---------------------------------------------------------------------------

#[test]
fn test_dry_run_survivor_matches_save_with_trim_eligible_lowest() {
    // The collision group's lowest-id member (20) is UNREFERENCED, so trim-first
    // sweeps it and the survivor shifts to 50 — in BOTH the preview and the save.
    let (_fx, archive) = common::generate_v16_collision_archive();
    let (session, _notes) =
        open_and_validate(&archive, &dev_resources_db_path()).expect("open must succeed");

    // Preview.
    let mut conn = Connection::open(&session.db_path).unwrap();
    let report = dry_run_downgrade(&mut conn).expect("dry run must succeed");
    // Post-trim group is {50,90}; ONE Location (90) is merged away.
    assert_eq!(report.deleted.get("Location"), Some(&1));
    // Preview is non-destructive: session still v16 with all 3 group Locations.
    assert_eq!(user_version_of(&session.db_path), 16);
    assert_eq!(location_ids_of(&session.db_path), vec![20, 50, 90]);

    // Actual save.
    let out_dir = TempDir::new().unwrap();
    let target = out_dir.path().join("export_v14.jwlibrary");
    save_v14_copy(&session, &target, APP, DEVICE, NOW).expect("v14 save must succeed");

    let (_od, out_extract) = common::extract_to_tempdir(&target);
    let out_db = out_extract.join("userData.db");
    // Trim-first survivor is 50 (NOT 20): proves preview and save agree, and
    // that without trim-first the wrong survivor (20) would have been chosen.
    assert_eq!(user_version_of(&out_db), 14);
    assert_eq!(
        location_ids_of(&out_db),
        vec![50],
        "trim-first survivor is 50"
    );
}

// ---------------------------------------------------------------------------
// 3. Un-downgradeable surfaces cleanly, writes nothing (HIGH-1 end-to-end)
// ---------------------------------------------------------------------------

#[test]
fn test_undowngradeable_fails_typed_and_writes_no_output() {
    let (_fx, archive) = common::generate_high1_undowngradeable_archive();
    let (session, _notes) =
        open_and_validate(&archive, &dev_resources_db_path()).expect("open must succeed");
    let session_hash_before = hash_file(&session.db_path);

    let out_dir = TempDir::new().unwrap();
    let target = out_dir.path().join("export_v14.jwlibrary");
    match save_v14_copy(&session, &target, APP, DEVICE, NOW) {
        Err(ArchiveError::SchemaDowngradeFailed { .. }) => {}
        other => panic!("expected SchemaDowngradeFailed, got {other:?}"),
    }

    assert!(
        !target.exists(),
        "no output file must be written on failure"
    );
    assert_eq!(user_version_of(&session.db_path), 16, "session stays v16");
    assert_eq!(
        hash_file(&session.db_path),
        session_hash_before,
        "session.db_path bytes unchanged after a failed v14 save"
    );
}

// ---------------------------------------------------------------------------
// 4. v14 output round-trips: reopens (v14->v16 upgrade) with merged state kept
// ---------------------------------------------------------------------------

#[test]
fn test_v14_output_round_trips_through_open_and_validate() {
    let (_fx, archive) = common::generate_v16_dryrun_collision_archive();
    let (session, _notes) =
        open_and_validate(&archive, &dev_resources_db_path()).expect("open must succeed");

    let out_dir = TempDir::new().unwrap();
    let target = out_dir.path().join("export_v14.jwlibrary");
    save_v14_copy(&session, &target, APP, DEVICE, NOW).expect("v14 save must succeed");

    // Reopen the v14 output: the open path upgrades v14 -> v16 (Phase 3), so the
    // reopened session is v16 with the merged-Location state preserved
    // semantically (survivor 20 only), never byte-compared.
    let (reopened, _notes) =
        open_and_validate(&target, &dev_resources_db_path()).expect("v14 output must reopen");
    assert_eq!(
        reopened.manifest.schema_version, 16,
        "v14 upgraded to v16 on open"
    );
    assert_eq!(
        common::normalized_table_rows(&Connection::open(&reopened.db_path).unwrap(), "Location")
            .values()
            .sum::<usize>(),
        1,
        "exactly the single merged survivor Location survives the round-trip"
    );
}
