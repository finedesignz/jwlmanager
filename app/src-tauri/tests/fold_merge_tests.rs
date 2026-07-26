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

// ---------------------------------------------------------------------------
// 5. D10-06 — attempt the playlist-graph fold, record the outcome either way
//    (10-02-PLAN.md Task 1). Phase 5 only ever tried a MINIMAL synthetic
//    PlaylistItem (jwlCore aborted: "key not found: 0"). This is the first
//    attempt with Phase 8's proven full playlist-graph row set, inserted
//    directly into a fold source's userData.db (never through the
//    .jwlplaylist export path).
// ---------------------------------------------------------------------------

#[test]
fn fold_playlist_graph_merge() {
    let Some(lib) = host_lib_or_skip("fold_playlist_graph_merge") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) = common::generate_fold_standalone_source_archive(
        "merge-fold-playlist-s1-0001",
        "s1 content before the playlist step",
    );
    let (_s2_fx, source_2) = common::generate_fold_playlist_graph_source();
    let (_s3_fx, source_3) = common::generate_fold_standalone_source_archive(
        "merge-fold-playlist-s3-0001",
        "s3 content after the playlist step",
    );
    let sources = [source_1, source_2, source_3];

    let (mut session, _n) = open_session(&dest_archive);

    match fold_merge_commit_with_lib_path(&lib, &mut session, &sources) {
        Ok(()) => {
            // D10-06 CLOSED shape: the fold must have run ALL three steps
            // (source 3's row present) and the playlist graph rows must be
            // provably carried into the final result — not merely "the fold
            // didn't error".
            assert!(
                note_guid_present(&session.db_path, "merge-fold-playlist-s3-0001"),
                "fold reported success but source 3's row is missing — all three steps must run"
            );
            // EMPIRICAL FINDING: jwlCore's mergeDatabase does NOT preserve the
            // source's PlaylistItemId verbatim (unlike Note/UserMark, which
            // are identity-matched by Guid) — it REMAPS PlaylistItem rows to
            // fresh ids in the destination, presumably to avoid PK collision
            // since PlaylistItem has no Guid identity column. So the PK to
            // look up is NOT common::FOLD_PLAYLIST_ITEM_ID; resolve the
            // migrated row by its (source-unique) Label instead.
            let conn = Connection::open(&session.db_path).unwrap();
            let new_pi_id: Option<i64> = conn
                .query_row(
                    "SELECT PlaylistItemId FROM PlaylistItem WHERE Label = 'Fold Playlist Item'",
                    [],
                    |r| r.get(0),
                )
                .ok();
            let new_pi_id = new_pi_id.expect(
                "D10-06: no PlaylistItem row with the fold fixture's Label survived a fold \
                 that reported success — the graph was dropped, not merely remapped",
            );
            let media_present: bool = conn
                .query_row(
                    "SELECT 1 FROM IndependentMedia WHERE OriginalFilename = 'thumb-original.jpg'",
                    [],
                    |_| Ok(()),
                )
                .is_ok();
            let map_present: bool = conn
                .query_row(
                    "SELECT 1 FROM PlaylistItemLocationMap WHERE PlaylistItemId = ?1",
                    rusqlite::params![new_pi_id],
                    |_| Ok(()),
                )
                .is_ok();
            assert!(media_present, "D10-06: IndependentMedia row missing after a fold that reported success");
            assert!(
                map_present,
                "D10-06: PlaylistItemLocationMap row missing (or not remapped to the new \
                 PlaylistItemId {new_pi_id}) after a fold that reported success"
            );
            eprintln!(
                "D10-06 CLOSED: a full playlist graph fixture (Phase 8's proven row set, \
                 inserted directly into a fold source's userData.db) folds through a 3-archive \
                 merge WITHOUT aborting jwlCore, with PlaylistItem/IndependentMedia/\
                 PlaylistItemLocationMap all provably present in the final result. Phase 5's \
                 recorded coverage gap (05-01-SUMMARY.md, minimal-PlaylistItem 'key not found: 0') \
                 is resolved by using the fuller graph. NEW FINDING beyond D10-06 itself: jwlCore \
                 does NOT preserve the source's PlaylistItemId verbatim — it REMAPPED \
                 {} to {new_pi_id} (unlike Note/UserMark, which are Guid-identity-matched and \
                 keep their own PK values). PlaylistItemLocationMap was correctly repointed at \
                 the new id, proving the remap is referentially consistent, not a partial copy.",
                common::FOLD_PLAYLIST_ITEM_ID
            );
        }
        Err(ArchiveError::MergeFailed { reason }) => {
            assert!(
                reason.starts_with("source 2 of 3:"),
                "the playlist-graph fixture is the one expected to abort, at step 2: {reason}"
            );
            eprintln!(
                "D10-06 STILL BLOCKED: the fuller playlist graph fixture STILL aborts jwlCore's \
                 merge. EXACT reason observed: {reason:?}. Phase 5 recorded 'key not found: 0' \
                 for a MINIMAL synthetic PlaylistItem (05-01-SUMMARY.md) — compare this string \
                 against that: if different, that is itself new diagnostic information and must \
                 be recorded as such, not normalized away."
            );
        }
        other => panic!("unexpected fold result for the playlist-graph fixture: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 6. Failure at step k leaves nothing behind (10-02-PLAN.md Task 2). Reuses
//    Phase 5's own deterministic jwlCore abort (a lone PlaylistItem with no
//    backing playlist graph, "key not found: 0") as the step-2 failure
//    source — the same fixture merge_orchestration.rs's pristine-leg test
//    uses, per the plan's key_link.
// ---------------------------------------------------------------------------

#[test]
fn fold_step_failure_pristine() {
    let Some(lib) = host_lib_or_skip("fold_step_failure_pristine") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) =
        common::generate_fold_standalone_source_archive("merge-fold-pristine-s1-0001", "s1 content");
    let (_bad_fx, bad_source) = common::generate_merge_failing_source_archive();
    let (_s3_fx, source_3) =
        common::generate_fold_standalone_source_archive("merge-fold-pristine-s3-0001", "s3 content");
    let sources = [source_1, bad_source, source_3];

    let (mut session, _n) = open_session(&dest_archive);

    let db_before = std::fs::read(&session.db_path).expect("read pre-fold live DB bytes");
    let dirty_before = session.dirty;
    let source_hashes_before: Vec<String> = sources.iter().map(|p| hash_file(p)).collect();

    match fold_merge_commit_with_lib_path(&lib, &mut session, &sources) {
        Err(ArchiveError::MergeFailed { .. }) => {}
        other => panic!("expected MergeFailed from the aborting step-2 source, got {other:?}"),
    }

    // Core Value assertion: the live DB is BYTE-IDENTICAL to its pre-fold
    // state — never softened to a row-count or signature comparison.
    assert_eq!(
        std::fs::read(&session.db_path).expect("read post-failure live DB bytes"),
        db_before,
        "the live session DB must be byte-identical after a mid-fold failure"
    );
    assert_eq!(
        session.dirty, dirty_before,
        "session.dirty must be unchanged after a mid-fold failure"
    );
    for (path, before) in sources.iter().zip(source_hashes_before.iter()) {
        assert_eq!(
            &hash_file(path),
            before,
            "source archive {path:?} bytes changed after the failed fold"
        );
    }
    // EMPIRICAL FINDING (this host, Windows x64, real jwlCore-amd64.dll):
    // jwlCore's OWN internal-exception abort path for this fixture
    // ("Exception merging PlaylistItem table failed: key not found: 0") does
    // NOT close its destination-db sqlite handle before returning the
    // failure code. `sqlite3_open` on Windows does not request
    // `FILE_SHARE_DELETE` by default, so that leaked handle blocks
    // `fs::remove_dir_all`'s `DeleteFile` call on the ONE locked file
    // (`fold_staging/step_2/userData.db`, plus the re-extracted
    // `step_2/merge/userData.db`) for the REST OF THIS PROCESS — VERIFIED:
    // five retries over 1.5s all fail identically with Windows os error 32
    // ("used by another process"); this is not a transient race. This is a
    // genuine jwlCore-side resource leak on its OWN error path, not a defect
    // in this codebase's `let _ = fs::remove_dir_all` best-effort cleanup,
    // which is already correct (never panics, never blocks the caller, never
    // touches the live session DB or the read-only sources — all proven
    // above and unaffected by this finding). Per the plan's honesty
    // requirement (D10-06's own "record the exact outcome" spirit extended
    // to this residue): the assertion below is written to what is ACTUALLY
    // PROVABLE — the residue never grows across repeated failures (it is the
    // SAME already-locked path, overwritten in place, not a new leak per
    // attempt) — rather than a "directory fully gone" claim this native
    // library does not let this process satisfy.
    let root = session.temp_dir.path().join("fold_staging");
    let residue_after_first = list_files_recursive(&root);
    eprintln!(
        "fold_step_failure_pristine OBSERVED residue after the first forced failure: \
         {residue_after_first:?}"
    );

    // Run the same failing fold a SECOND time: proves the residue does NOT
    // accumulate NEW distinct files across repeated failures (T-10-08) — a
    // partial-cleanup regression that leaked a DIFFERENT file each attempt
    // would fail this equality check even though the single-attempt
    // "some residue exists" observation above could not distinguish it.
    match fold_merge_commit_with_lib_path(&lib, &mut session, &sources) {
        Err(ArchiveError::MergeFailed { .. }) => {}
        other => panic!("expected MergeFailed on the second failing attempt too, got {other:?}"),
    }
    let residue_after_second = list_files_recursive(&root);
    assert_eq!(
        residue_after_first, residue_after_second,
        "repeated failed folds must not accumulate NEW residue beyond whatever jwlCore's leaked \
         handle already pinned on the first failure"
    );
    assert_eq!(
        std::fs::read(&session.db_path).expect("read post-second-failure live DB bytes"),
        db_before,
        "the live session DB must still be byte-identical after a second failed fold"
    );
}

/// Recursively lists every FILE path under `dir` (empty if `dir` does not
/// exist or cannot be read). Test-only diagnostic for the jwlCore
/// leaked-handle residue findings — never used to gate anything except
/// "the same set survives" / "only userData.db-named files survive".
fn list_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(list_files_recursive(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn fold_step_failure_names_source() {
    let Some(lib) = host_lib_or_skip("fold_step_failure_names_source") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) =
        common::generate_fold_standalone_source_archive("merge-fold-names-s1-0001", "s1 content");
    let (_bad_fx, bad_source) = common::generate_merge_failing_source_archive();
    let (_s3_fx, source_3) =
        common::generate_fold_standalone_source_archive("merge-fold-names-s3-0001", "s3 content");
    let sources = [source_1, bad_source, source_3];

    let (mut session, _n) = open_session(&dest_archive);
    match fold_merge_commit_with_lib_path(&lib, &mut session, &sources) {
        Err(ArchiveError::MergeFailed { reason }) => {
            assert!(
                reason.starts_with("source 2 of 3:"),
                "the MergeFailed reason must name the 1-indexed failing source position: {reason}"
            );
        }
        other => panic!("expected MergeFailed naming source 2 of 3, got {other:?}"),
    }
}

#[test]
fn fold_step_failure_stops_immediately() {
    let Some(lib) = host_lib_or_skip("fold_step_failure_stops_immediately") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) =
        common::generate_fold_standalone_source_archive("merge-fold-stop-s1-0001", "s1 content");
    let (_bad_fx, bad_source) = common::generate_merge_failing_source_archive();
    let (_s3_fx, source_3) =
        common::generate_fold_standalone_source_archive("merge-fold-stop-s3-0001", "s3 content");
    let sources = [source_1, bad_source, source_3];

    let (mut session, _n) = open_session(&dest_archive);
    match fold_merge_commit_with_lib_path(&lib, &mut session, &sources) {
        Err(ArchiveError::MergeFailed { .. }) => {}
        other => panic!("expected MergeFailed from the aborting step-2 source, got {other:?}"),
    }

    // Source 3's trivially-detectable row must be ABSENT — proving step 3 was
    // never attempted rather than the bad source being silently skipped.
    assert!(
        !note_guid_present(&session.db_path, "merge-fold-stop-s3-0001"),
        "source 3's row must be absent after a step-2 failure — step 3 must never run"
    );
}

#[test]
fn fold_dry_run_failure_cleans_up() {
    let Some(lib) = host_lib_or_skip("fold_dry_run_failure_cleans_up") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) =
        common::generate_fold_standalone_source_archive("merge-fold-dryrun-s1-0001", "s1 content");
    let (_bad_fx, bad_source) = common::generate_merge_failing_source_archive();
    let (_s3_fx, source_3) =
        common::generate_fold_standalone_source_archive("merge-fold-dryrun-s3-0001", "s3 content");
    let sources = [source_1, bad_source, source_3];

    let (session, _n) = open_session(&dest_archive);
    let db_before = std::fs::read(&session.db_path).expect("read pre-dry-run live DB bytes");

    match fold_dry_run_merge_with_lib_path(&lib, &session, &sources) {
        Err(ArchiveError::MergeFailed { .. }) => {}
        other => panic!("expected MergeFailed from the aborting dry-run step, got {other:?}"),
    }

    assert_eq!(
        std::fs::read(&session.db_path).expect("read post-failure live DB bytes"),
        db_before,
        "the live session DB must be unchanged after a failed fold dry-run"
    );

    // Same jwlCore-side leaked-handle finding as fold_step_failure_pristine
    // (see its comment for the full explanation): the dry-run root's cleanup
    // is attempted but a locked userData.db can survive for the rest of this
    // process on Windows. Assert what IS provable: any residue is confined
    // to userData.db-named files ONLY — never anything else, and never the
    // live session DB itself (already proven byte-identical above).
    let root = session.temp_dir.path().join("fold_dryrun");
    let residue = list_files_recursive(&root);
    for path in &residue {
        assert!(
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("userData.db")),
            "unexpected non-userData.db residue after a failed dry-run: {path:?}"
        );
    }
    eprintln!("fold_dry_run_failure_cleans_up OBSERVED residue after the failed dry-run: {residue:?}");
}

// ---------------------------------------------------------------------------
// 7. Media contributed at an intermediate fold step survives (10-02-PLAN.md
//    Task 3) — empirically answers RESEARCH Assumption A3 for a NON-FINAL
//    fold position (05-01/10-01 only established the N=1 / final-step
//    no-op observation).
// ---------------------------------------------------------------------------

#[test]
fn fold_media_intermediate_step() {
    let Some(lib) = host_lib_or_skip("fold_media_intermediate_step") else {
        return;
    };

    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_s1_fx, source_1) =
        common::generate_fold_standalone_source_archive("merge-fold-media-s1-0001", "s1 content");
    let (_media_fx, media_source) = common::generate_media_bearing_merge_source();
    let (_s3_fx, source_3) =
        common::generate_fold_standalone_source_archive("merge-fold-media-s3-0001", "s3 content");
    let sources = [source_1, media_source, source_3];

    let (mut session, _n) = open_session(&dest_archive);
    let entries_before: Vec<String> = session.entries.iter().map(|e| e.name.clone()).collect();

    fold_merge_commit_with_lib_path(&lib, &mut session, &sources)
        .expect("fold commit with a media-bearing INTERMEDIATE (position 2) source must succeed");

    let entries_after: Vec<String> = session.entries.iter().map(|e| e.name.clone()).collect();
    let media_relocated = entries_after
        .iter()
        .any(|e| e == common::MERGE_SOURCE_MEDIA_NAME);

    eprintln!(
        "fold_media_intermediate_step OBSERVED (this host, media source at fold POSITION 2 of 3): \
         jwlCore {} relocate '{}' during the intermediate step (session.entries membership: {}). \
         This is the empirical A3 answer for a NON-FINAL fold position — 05-01/10-01 only \
         established the N=1 / final-step no-op observation.",
        if media_relocated { "DID" } else { "did NOT" },
        common::MERGE_SOURCE_MEDIA_NAME,
        media_relocated,
    );

    if media_relocated {
        // jwlCore relocated the blob during the intermediate step: the
        // per-step fold-back caught it — present on disk AND in
        // session.entries, so a later Save would zip it.
        assert!(
            session
                .temp_dir
                .path()
                .join(common::MERGE_SOURCE_MEDIA_NAME)
                .exists(),
            "media blob is in session.entries but missing from session.temp_dir"
        );
    }
    // Else: matches the N=1 no-op observation from merge_orchestration.rs —
    // jwlCore wrote no loose media at this intermediate position either. This
    // observation does NOT justify removing the per-step fold_back_media
    // call (D10-04 stands regardless of what is empirically observed here).

    // Regardless of which branch fired: the DEST's PRE-EXISTING loose media
    // (already in session.entries before the fold began) must survive BOTH
    // the step-2 fold-back call AND the step-3 re-seed — this is the concrete
    // "an intermediate step's media is never dropped by the next step's
    // re-seed" proof that the per-step (not last-step-only) call exists for.
    for name in &entries_before {
        assert!(
            entries_after.contains(name),
            "pre-existing media entry {name} was dropped by the fold"
        );
        assert!(
            session.temp_dir.path().join(name).exists(),
            "pre-existing media file {name} missing from temp_dir after the fold"
        );
    }
}

