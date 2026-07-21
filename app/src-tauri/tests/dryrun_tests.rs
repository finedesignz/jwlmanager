//! Semantic dry-run preview coverage (SAFE-01, D2-07, 02-02-PLAN.md Task 1).
//!
//! Never asserts on raw `changes()` counts — only SEMANTIC per-table
//! before/after primary-key-set diffs, and never byte-diffs the working
//! copy (Core Value: save is not byte-preserving; here dry-run isn't either
//! kind of mutating op at all, but the same "no byte comparisons" discipline
//! applies to the pre/post-dry-run file hash check below).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::delete::{dry_run_delete_notes, NonEmptyNoteIds};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

fn file_hash(path: &std::path::Path) -> Vec<u8> {
    let bytes = common::read_file_bytes(path);
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hasher.finalize().to_vec()
}

/// SAFE-01: dry-run leaves the working-copy DB byte-identical (hash before
/// == hash after) — the rolled-back transaction commits nothing, and
/// `dry_run_delete_notes` never calls `trim_db`/`VACUUM` (Pitfall 2).
#[test]
fn test_dry_run_leaves_working_db_byte_identical() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let before_hash = file_hash(&db_path);

    let mut conn = Connection::open(&db_path).expect("open extracted db");
    let ids = NonEmptyNoteIds::try_from(vec![900_i64]).unwrap();
    let _report = dry_run_delete_notes(&mut conn, &ids).expect("dry run must succeed");
    drop(conn);

    let after_hash = file_hash(&db_path);
    assert_eq!(
        before_hash, after_hash,
        "dry-run must never mutate the working-copy DB file"
    );
}

/// D2-07: the TagMap re-densify (DELETE-all + reinsert with the SAME
/// TagMapId for surviving mappings) must classify as `overwritten`, never
/// `deleted` — a fixture with valid preserved tag mappings must report 0
/// TagMap `deleted`.
#[test]
fn test_dry_run_semantic_counts_no_false_tagmap_deletes() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let mut conn = Connection::open(&db_path).expect("open extracted db");
    // Delete one of the gapped-position notes (902) whose TagMap (902) is
    // its ONLY tag mapping under Tag 901 — Tag 901 still has TagMap 903/904
    // surviving, so the re-densify preserves those TagMapIds.
    let ids = NonEmptyNoteIds::try_from(vec![902_i64]).unwrap();
    let report = dry_run_delete_notes(&mut conn, &ids).expect("dry run must succeed");

    // TagMap 903 and 904 survive (their Note is untouched); their TagMapIds
    // must appear as `overwritten` (position re-densified), never counted
    // among `deleted`. TagMap 902 (owned by the deleted Note 902) is
    // genuinely gone, so *some* TagMap deletion is expected, but it must be
    // exactly 1 (Note 902's own mapping) — never inflated by the redensify
    // wiping and reinserting the survivors.
    let tagmap_deleted = report.deleted.get("TagMap").copied().unwrap_or(0);
    assert_eq!(
        tagmap_deleted, 1,
        "only the deleted note's own TagMap row may count as deleted, \
         never the re-densify's DELETE-all-then-reinsert of survivors: {:?}",
        report.deleted
    );
    let tagmap_overwritten = report.overwritten.get("TagMap").copied().unwrap_or(0);
    assert!(
        tagmap_overwritten >= 2,
        "surviving TagMap rows (903, 904) must be counted as overwritten, not deleted: {:?}",
        report.overwritten
    );
}

/// Finding 1 corrected scope: `delete_notes` alone (no trim) must NOT touch
/// UserMark/BlockRange rows — they are only swept later if genuinely
/// orphaned. This exercises the CORE fn directly, before any trim runs.
#[test]
fn test_delete_notes_does_not_touch_usermark_blockrange() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let conn = Connection::open(&db_path).expect("open extracted db");
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let ids = NonEmptyNoteIds::try_from(vec![900_i64]).unwrap();

    let tx = conn.unchecked_transaction().expect("open tx");
    jwlmanager_lib::db::delete::delete_notes(&tx, &ids).expect("delete_notes must succeed");

    let usermark_exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM UserMark WHERE UserMarkId = 900)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let blockrange_exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM BlockRange WHERE BlockRangeId = 900)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        usermark_exists,
        "delete_notes must NOT delete UserMark 900 — only trim sweeps genuine orphans"
    );
    assert!(
        blockrange_exists,
        "delete_notes must NOT delete BlockRange 900 — only trim sweeps genuine orphans"
    );

    tx.rollback().unwrap();
}

/// Dry-run must leave the connection's PRAGMA state restored, matching the
/// PragmaGuard contract already proven for `trim_db` (Plan 01, finding 4).
#[test]
fn test_dry_run_restores_pragmas() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let mut conn = Connection::open(&db_path).expect("open extracted db");
    let fk_before: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();

    let ids = NonEmptyNoteIds::try_from(vec![900_i64]).unwrap();
    let _report = dry_run_delete_notes(&mut conn, &ids).expect("dry run must succeed");

    let fk_after: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        fk_before, fk_after,
        "dry-run must restore the connection's prior foreign_keys pragma"
    );
}
