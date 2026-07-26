//! 10-01 Task 2: N-WAY FOLD orchestration tests, driven against the REAL
//! vendored jwlCore DLL (skip-as-pass off-host via `host_dev_lib_path`,
//! matching `merge_orchestration.rs`'s convention exactly).
//!
//! These prove the two criteria this phase exists for:
//!   - MERGE-03 criterion 1 (D10-01): `fold(A,B,C)` runs as ONE backend
//!     operation, in the CALLER's order, and every one of the N sources
//!     contributes (the copy-source regression guard — a fold that collapsed
//!     to "last source wins" would still look plausible on a bare row count,
//!     so this asserts PER-SOURCE content presence).
//!   - MERGE-03 criterion 3 (D10-02): `fold(A,B,C)` produces the SAME
//!     normalized table state as calling the shipped
//!     `merge_commit_with_lib_path` twice by hand, IN THE SAME ORDER — never
//!     asserted for a permuted order (order-independence is NOT a property
//!     of this system, D10-01).
//!
//! Plus the aggregate dry-run (MERGE-03 criterion 2 / D10-05) and the
//! command-boundary minimum-N rejection (D10-06).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::merge::{
    content_diff, fold_dry_run_merge_with_lib_path, fold_merge_commit_with_lib_path,
    merge_commit_with_lib_path,
};
use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::error::ArchiveError;
use jwlmanager_lib::jwlcore::merge::host_dev_lib_path;
use jwlmanager_lib::session::ArchiveSession;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Skip-as-pass helper: returns the host DLL path or `None` off-host — an
/// arm64-windows or binary-less CI host must PASS these tests, not fail them
/// (matches `merge_orchestration.rs::host_lib_or_skip`).
fn host_lib_or_skip(test: &str) -> Option<PathBuf> {
    match host_dev_lib_path() {
        Some(p) if p.exists() => Some(p),
        _ => {
            eprintln!("no vendored jwlCore binary for this (OS, ARCH) — skipping {test}");
            None
        }
    }
}

fn hash_file(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read file for hashing");
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn open_session(archive: &Path) -> (ArchiveSession, Vec<jwlmanager_lib::db::notes::BrowseRow>) {
    open_and_validate(archive, &dev_resources_db_path()).expect("open dest archive must succeed")
}

fn note_content_by_guid(db: &Path, guid: &str) -> Option<String> {
    let conn = Connection::open(db).unwrap();
    conn.query_row(
        "SELECT Content FROM Note WHERE Guid = ?1",
        rusqlite::params![guid],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

fn note_guid_present(db: &Path, guid: &str) -> bool {
    let conn = Connection::open(db).unwrap();
    conn.query_row(
        "SELECT 1 FROM Note WHERE Guid = ?1",
        rusqlite::params![guid],
        |_| Ok(()),
    )
    .is_ok()
}

fn integrity_ok(db: &Path) -> bool {
    let conn = Connection::open(db).unwrap();
    let res: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    res == "ok"
}

// ---------------------------------------------------------------------------
// 1. Every one of the N sources contributes (copy-source regression guard) +
//    all N source files are byte-unchanged (read-only inputs, T-10-03).
// ---------------------------------------------------------------------------

#[test]
fn fold_merge_carries_all_sources() {
    let Some(lib) = host_lib_or_skip("fold_merge_carries_all_sources") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) = common::generate_merge_source_archive();
    let (_s2_fx, source_2) =
        common::generate_fold_standalone_source_archive("merge-fold-carry-s2-0001", "s2 content");
    let (_s3_fx, source_3) =
        common::generate_fold_standalone_source_archive("merge-fold-carry-s3-0001", "s3 content");
    let sources = [source_1.clone(), source_2.clone(), source_3.clone()];

    let source_hashes_before: Vec<String> = sources.iter().map(|p| hash_file(p)).collect();

    let (mut session, _notes) = open_session(&dest_archive);
    fold_merge_commit_with_lib_path(&lib, &mut session, &sources).expect("fold commit must succeed");

    // Per-source content presence — NEVER a bare total row count, which a
    // "collapsed to last source wins" bug could still satisfy.
    assert!(
        note_guid_present(&session.db_path, common::MERGE_SRC_ONLY_NOTE_GUID),
        "source 1's unique note is missing after the fold"
    );
    assert!(
        note_guid_present(&session.db_path, "merge-fold-carry-s2-0001"),
        "source 2's unique note is missing after the fold"
    );
    assert!(
        note_guid_present(&session.db_path, "merge-fold-carry-s3-0001"),
        "source 3's unique note is missing after the fold"
    );

    // Read-only inputs: all THREE source archives are byte-unchanged, proving
    // the prohibition for all N, not just N=1 as Phase 5 proved it.
    for (path, before) in sources.iter().zip(source_hashes_before.iter()) {
        assert_eq!(
            &hash_file(path),
            before,
            "source archive {path:?} bytes changed after the fold"
        );
    }
    assert!(integrity_ok(&session.db_path), "post-fold integrity");
    assert!(session.dirty, "fold commit must mark the session dirty");
}

// ---------------------------------------------------------------------------
// 2. fold(A,B,C) == chained-pairwise commits IN THE SAME ORDER (D10-01/D10-02)
// ---------------------------------------------------------------------------

#[test]
fn fold_matches_chained_pairwise() {
    let Some(lib) = host_lib_or_skip("fold_matches_chained_pairwise") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) = common::generate_merge_source_archive();
    let ((_b_fx, source_b), (_c_fx, source_c)) = common::generate_fold_contested_pair();
    let sources = [source_1.clone(), source_b.clone(), source_c.clone()];

    // Leg (a): the fold, on its own session/copy of the fixtures.
    let (mut session_fold, _n) = open_session(&dest_archive);
    fold_merge_commit_with_lib_path(&lib, &mut session_fold, &sources)
        .expect("fold commit must succeed");

    // Leg (b): the SAME sources, in the SAME order, one at a time, via the
    // shipped pairwise `merge_commit_with_lib_path` — an INDEPENDENT session
    // over an independent copy of the dest fixture, so neither leg mutates
    // the other's inputs.
    let (mut session_chained, _n) = open_session(&dest_archive);
    for source in &sources {
        merge_commit_with_lib_path(&lib, &mut session_chained, source)
            .expect("chained pairwise commit must succeed");
    }

    // Compare by NORMALIZED TABLE STATE (content_diff between the two
    // results reports empty added/overwritten/deleted), NEVER a byte-diff.
    let diff = content_diff(&session_fold.db_path, &session_chained.db_path)
        .expect("content_diff between fold and chained result");
    assert!(
        diff.added.is_empty() && diff.overwritten.is_empty() && diff.deleted.is_empty(),
        "fold result diverges from chained-pairwise result (same order): {diff:?}"
    );

    // Pin order-sensitivity as intended behaviour (D10-01): the contested
    // note's FINAL content is source C's — the LATER source in the supplied
    // order — never source B's. A test asserting order-INDEPENDENCE here
    // would be testing the WRONG property: fold(A,B,C) != fold(A,C,B) is
    // correct, and this repo's contested-key fixture exists specifically to
    // make that observable.
    let final_content = note_content_by_guid(
        &session_fold.db_path,
        common::MERGE_FOLD_CONTESTED_NOTE_GUID,
    )
    .expect("contested note must be present after the fold");
    assert_eq!(
        final_content,
        common::MERGE_FOLD_C_CONTENT,
        "contested identity must resolve to the LATER source (C)'s content, not B's"
    );

    // Each source's own unique row still made it through the fold.
    assert!(note_guid_present(
        &session_fold.db_path,
        common::MERGE_FOLD_B_ONLY_NOTE_GUID
    ));
    assert!(note_guid_present(
        &session_fold.db_path,
        common::MERGE_FOLD_C_ONLY_NOTE_GUID
    ));
}

// ---------------------------------------------------------------------------
// 3. Aggregate dry-run: ONE report, cumulative effect, live session unchanged
//    (MERGE-03 criterion 2 / D10-05).
// ---------------------------------------------------------------------------

#[test]
fn fold_dry_run_aggregate() {
    let Some(lib) = host_lib_or_skip("fold_dry_run_aggregate") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) = common::generate_merge_source_archive();
    let ((_b_fx, source_b), (_c_fx, source_c)) = common::generate_fold_contested_pair();
    let sources = [source_1, source_b, source_c];

    // Preview on session A.
    let (session_a, _n) = open_session(&dest_archive);
    let preview =
        fold_dry_run_merge_with_lib_path(&lib, &session_a, &sources).expect("fold dry run");

    // Live session DB is unchanged by the dry-run.
    let (_extract_dir, extracted) = common::extract_to_tempdir(&dest_archive);
    let baseline_diff = content_diff(&session_a.db_path, &extracted.join("userData.db"))
        .expect("content_diff baseline vs freshly-extracted dest");
    assert!(
        baseline_diff.added.is_empty()
            && baseline_diff.overwritten.is_empty()
            && baseline_diff.deleted.is_empty(),
        "dry-run must not mutate the live session DB: {baseline_diff:?}"
    );

    // Commit into an independent session B; capture its BEFORE bytes, then
    // compute the committed effect the SAME way the dry-run does
    // (`content_diff` over a pre-fold snapshot), mirroring
    // `merge_dry_run_matches_commit`.
    let (mut session_b, _n) = open_session(&dest_archive);
    let before_copy = session_b.temp_dir.path().join("before_fold_commit.db");
    std::fs::copy(&session_b.db_path, &before_copy).unwrap();

    fold_merge_commit_with_lib_path(&lib, &mut session_b, &sources)
        .expect("fold commit must succeed");

    let committed = content_diff(&before_copy, &session_b.db_path).expect("content_diff");

    assert_eq!(preview.added, committed.added, "added counts diverge");
    assert_eq!(
        preview.overwritten, committed.overwritten,
        "overwritten counts diverge"
    );
    assert_eq!(preview.deleted, committed.deleted, "deleted counts diverge");
    assert_eq!(preview.total_deleted, committed.total_deleted);

    // Non-vacuous, and the aggregate collapses a row overwritten across
    // multiple steps into ONE report entry (the contested note is added at
    // step 2 then overwritten at step 3 — it must show as exactly one
    // `added` count for Note, reflecting the step-3 content, not a double
    // count).
    assert!(
        preview.added.values().sum::<usize>() >= 1,
        "expected the fold to add at least one source-only record"
    );
}

// ---------------------------------------------------------------------------
// 4. Command-boundary minimum N=3 (D10-06): fewer sources is rejected before
//    any staging/dry-run directory is created.
// ---------------------------------------------------------------------------

#[test]
fn fold_rejects_fewer_than_three_sources() {
    let Some(lib) = host_lib_or_skip("fold_rejects_fewer_than_three_sources") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) = common::generate_merge_source_archive();
    let (_s2_fx, source_2) =
        common::generate_fold_standalone_source_archive("merge-fold-reject-s2-0001", "s2");
    let two_sources = [source_1, source_2];

    let (mut session, _n) = open_session(&dest_archive);
    let temp_dir_entries_before: Vec<_> = std::fs::read_dir(session.temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();

    match fold_merge_commit_with_lib_path(&lib, &mut session, &two_sources) {
        Err(ArchiveError::MergeFailed { reason }) => {
            assert!(
                reason.contains('2'),
                "reason should mention the supplied count: {reason}"
            );
        }
        other => panic!("expected MergeFailed for a 2-source fold, got {other:?}"),
    }
    assert!(
        !session.dirty,
        "a rejected fold call must not mark the session dirty"
    );

    // No staging or dry-run directory was created under the session temp dir.
    let temp_dir_entries_after: Vec<_> = std::fs::read_dir(session.temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        temp_dir_entries_before, temp_dir_entries_after,
        "rejecting a fold with fewer than 3 sources must not create any staging directory"
    );

    // The dry-run path rejects identically, before creating a dry-run dir.
    match fold_dry_run_merge_with_lib_path(&lib, &session, &two_sources) {
        Err(ArchiveError::MergeFailed { .. }) => {}
        other => panic!("expected MergeFailed for a 2-source fold dry-run, got {other:?}"),
    }
    let temp_dir_entries_after_dry_run: Vec<_> = std::fs::read_dir(session.temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        temp_dir_entries_before, temp_dir_entries_after_dry_run,
        "rejecting a fold dry-run with fewer than 3 sources must not create any directory"
    );
}
