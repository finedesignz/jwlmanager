//! 05-02 Task 3: merge dry-run + commit ORCHESTRATION tests, driven against the
//! REAL vendored jwlCore DLL (skip-as-pass off-host via `host_dev_lib_path`).
//!
//! These exercise the throwaway-copy preview + atomic-promote commit layered on
//! Wave 1's FFI wrapper:
//!   - source archive bytes are immutable across dry-run + commit (MERGE-02),
//!   - the dry-run's content-signature report EQUALS the committed effect
//!     (preview == commit, Pitfall 4),
//!   - an in-place row UPDATE at the same PK is counted as `overwritten`
//!     (content-signature diff vs a PK-set diff, HIGH),
//!   - the commit promotes atomically (session DB is pristine-or-fully-merged,
//!     never truncated — Core Value),
//!   - the media fold-back behavior is empirically observed + asserted (Open-Q1).
//!
//! Because integration tests cannot obtain a `tauri::AppHandle` / packaged
//! resource dir, they resolve the host DLL via `host_dev_lib_path()` and drive
//! the `*_with_lib_path` cores — identical to Wave 1's `merge_ffi.rs` approach.
//!
//! Semantic assertions only — the source archive is the ONLY byte-hash
//! assertion (it must be untouched); merged DB state is compared semantically.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::merge::{
    content_diff, dry_run_merge_with_lib_path, merge_commit_with_lib_path,
};
use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::error::ArchiveError;
use jwlmanager_lib::jwlcore::merge::host_dev_lib_path;
use jwlmanager_lib::session::ArchiveSession;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Skip-as-pass helper: returns the host DLL path or `None` off-host.
fn host_lib_or_skip(test: &str) -> Option<PathBuf> {
    match host_dev_lib_path() {
        Some(p) if p.exists() => Some(p),
        _ => {
            eprintln!("no vendored jwlCore binary for this (OS, ARCH) — skipping {test}");
            None
        }
    }
}

/// Serializes every test in THIS binary that invokes the real vendored
/// jwlCore DLL. Cargo's default multi-threaded test runner executes all
/// `#[test]` functions in one binary concurrently on a thread pool; the
/// production app never does this (a single `Mutex<Option<ArchiveSession>>`
/// serializes every merge/fold Tauri command through one critical section,
/// see `lib.rs` `merge_commit`/`fold_merge_commit`, D5-06) so this is a
/// TEST-ONLY concurrency concern, not a production defect.
///
/// Root cause (10-CONCURRENCY-FINDING.md): jwlCore's `mergeDatabase` is not
/// safe to invoke concurrently from multiple threads in one process —
/// `loader::load_library` prepends the DLL's directory to the process-global
/// `PATH` env var for the duration of the load (a second thread's load can
/// observe/restore a mutated `PATH`), and `run_merge_with_lib_path`'s own
/// doc comment already establishes that `getLastResult()` is
/// "process-global native memory that a later merge would overwrite"
/// (D5-06). Windows' loader also hands back the SAME already-loaded module
/// handle to concurrent callers (same DLL path), so two threads' merges can
/// run inside the same native library instance at once. This mutex is
/// per-binary — `cargo test` runs each test binary as its own process, so
/// cross-binary parallelism is already safe; the race is only ever between
/// threads within one binary (see `fold_merge_tests.rs::JWLCORE_TEST_LOCK`
/// for the sibling copy in that binary).
///
/// DO NOT remove this lock as a "cleanup" — that reintroduces the flake this
/// mutex exists to close. Every test below that calls `host_lib_or_skip` and
/// then touches the real DLL acquires `JWLCORE_TEST_LOCK` first.
static JWLCORE_TEST_LOCK: Mutex<()> = Mutex::new(());

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

fn note_guids(db: &Path) -> std::collections::BTreeSet<String> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare("SELECT Guid FROM Note WHERE Guid IS NOT NULL")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn integrity_ok(db: &Path) -> bool {
    let conn = Connection::open(db).unwrap();
    let res: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    res == "ok"
}

// ---------------------------------------------------------------------------
// 1. Source archive bytes immutable across dry-run + commit (MERGE-02, D5-11)
// ---------------------------------------------------------------------------

#[test]
fn merge_source_immutable() {
    let Some(lib) = host_lib_or_skip("merge_source_immutable") else {
        return;
    };
    let _lock = JWLCORE_TEST_LOCK.lock().unwrap();
    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_src_fx, source_archive) = common::generate_merge_source_archive();

    let source_hash_before = hash_file(&source_archive);

    let (mut session, _notes) = open_session(&dest_archive);

    dry_run_merge_with_lib_path(&lib, &session, &source_archive).expect("dry run must succeed");
    assert_eq!(
        hash_file(&source_archive),
        source_hash_before,
        "source archive bytes changed after dry-run"
    );

    merge_commit_with_lib_path(&lib, &mut session, &source_archive).expect("commit must succeed");
    assert_eq!(
        hash_file(&source_archive),
        source_hash_before,
        "source archive bytes changed after commit"
    );
}

// ---------------------------------------------------------------------------
// 2. Dry-run report EQUALS the committed effect (preview == commit, Pitfall 4)
// ---------------------------------------------------------------------------

#[test]
fn merge_dry_run_matches_commit() {
    let Some(lib) = host_lib_or_skip("merge_dry_run_matches_commit") else {
        return;
    };
    let _lock = JWLCORE_TEST_LOCK.lock().unwrap();
    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_src_fx, source_archive) = common::generate_merge_source_archive();

    // Preview on session A.
    let (session_a, _n) = open_session(&dest_archive);
    let preview = dry_run_merge_with_lib_path(&lib, &session_a, &source_archive)
        .expect("dry run must succeed");
    // Dry-run is non-destructive: session A's DB is unchanged.
    assert_eq!(
        note_guids(&session_a.db_path),
        note_guids(
            &common::extract_to_tempdir(&dest_archive)
                .1
                .join("userData.db")
        ),
        "dry-run must not mutate the live session DB"
    );

    // Commit into a bit-identical session B; capture its BEFORE bytes.
    let (mut session_b, _n) = open_session(&dest_archive);
    let before_copy = session_b.temp_dir.path().join("before_commit.db");
    std::fs::copy(&session_b.db_path, &before_copy).unwrap();

    merge_commit_with_lib_path(&lib, &mut session_b, &source_archive).expect("commit must succeed");

    // Compute the committed effect the SAME way the dry-run does.
    let committed = content_diff(&before_copy, &session_b.db_path).expect("content_diff");

    assert_eq!(preview.added, committed.added, "added counts diverge");
    assert_eq!(
        preview.overwritten, committed.overwritten,
        "overwritten counts diverge"
    );
    assert_eq!(preview.deleted, committed.deleted, "deleted counts diverge");
    assert_eq!(preview.total_deleted, committed.total_deleted);

    // Non-vacuous: the source carried disjoint records, so SOMETHING was added.
    assert!(
        preview.added.values().sum::<usize>() >= 1,
        "expected the merge to add at least one source-only record"
    );
}

// ---------------------------------------------------------------------------
// 3. In-place content UPDATE is counted as `overwritten` (content-sig, HIGH)
// ---------------------------------------------------------------------------

#[test]
fn merge_overwrite_content_counted() {
    let Some(lib) = host_lib_or_skip("merge_overwrite_content_counted") else {
        return;
    };
    let _lock = JWLCORE_TEST_LOCK.lock().unwrap();
    let ((_dest_fx, dest_archive), (_src_fx, source_archive)) =
        common::generate_merge_overwrite_pair_archives();

    let (session, _n) = open_session(&dest_archive);
    let report =
        dry_run_merge_with_lib_path(&lib, &session, &source_archive).expect("dry run must succeed");

    let total_overwritten: usize = report.overwritten.values().sum();
    assert!(
        total_overwritten >= 1,
        "content-signature diff must report >=1 overwrite for the in-place UPDATE \
         (a PK-set diff would report 0); got overwritten={:?}, added={:?}, deleted={:?}",
        report.overwritten,
        report.added,
        report.deleted
    );
}

// ---------------------------------------------------------------------------
// 4. Commit promotes atomically: pristine-or-fully-merged, never truncated
//    (Core Value)
// ---------------------------------------------------------------------------

#[test]
fn merge_commit_promote_atomic() {
    let Some(lib) = host_lib_or_skip("merge_commit_promote_atomic") else {
        return;
    };
    let _lock = JWLCORE_TEST_LOCK.lock().unwrap();

    // (a) SUCCESS leg: after a successful commit the live session DB is FULLY
    //     merged and passes integrity_check (never a truncated short file).
    {
        let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
        let (_src_fx, source_archive) = common::generate_merge_source_archive();
        let (mut session, _n) = open_session(&dest_archive);

        assert!(integrity_ok(&session.db_path), "pre-commit integrity");
        merge_commit_with_lib_path(&lib, &mut session, &source_archive)
            .expect("commit must succeed");

        assert!(
            integrity_ok(&session.db_path),
            "post-commit session DB must pass integrity_check (never truncated)"
        );
        assert!(session.dirty, "commit must mark the session dirty");
        // Fully merged: a source-only identity is now present in the live DB.
        assert!(
            note_guids(&session.db_path).contains(common::MERGE_SRC_ONLY_NOTE_GUID),
            "committed session DB must contain the merged source-only note"
        );
    }

    // (b) FAILURE leg: a source that aborts jwlCore's merge fails BEFORE the
    //     atomic promote — so the live session DB is left PRISTINE (byte-
    //     identical to pre-commit) and still passes integrity_check. This
    //     proves the promote is all-or-nothing: never a partial/truncated DB.
    {
        let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
        let (_bad_fx, bad_source) = common::generate_merge_failing_source_archive();
        let (mut session, _n) = open_session(&dest_archive);

        let hash_before = hash_file(&session.db_path);
        match merge_commit_with_lib_path(&lib, &mut session, &bad_source) {
            Err(ArchiveError::MergeFailed { .. }) => {}
            other => panic!("expected MergeFailed from the aborting source, got {other:?}"),
        }
        assert_eq!(
            hash_file(&session.db_path),
            hash_before,
            "a failed merge must leave the live session DB byte-identical (pristine)"
        );
        assert!(
            integrity_ok(&session.db_path),
            "live session DB must still pass integrity_check after a failed commit"
        );
        assert!(
            !session.dirty,
            "a failed commit must NOT mark the session dirty"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Media fold-back: empirically observe what jwlCore writes to the staging
//    dir, and assert the post-commit session state (Open-Q1 / Pitfall 2).
// ---------------------------------------------------------------------------
//
// OBSERVED (this host, Windows x64, real jwlCore-amd64.dll): jwlCore wrote ONLY
// `userData.db` into the destination staging dir during the merge — it did NOT
// emit or relocate the source's loose media blob (MERGE_SOURCE_MEDIA_NAME) into
// the dest root. So branch (a) fired: the fold-back is a correct no-op, no new
// ZipEntryMeta was added, and the destination's pre-existing loose media
// (media/test.png, default_thumbnail.png, future_unknown.dat) is retained
// unchanged in session.entries. Branch (b) (a same-name-different-content blob
// replaced) was NOT observed on these fixtures. This resolves A1/Open-Q1: media
// fold-back is defensive, empirically a no-op for jwlCore's directory-pair merge
// of these synthetic archives. (Playlist media relocation via a full valid
// PlaylistItem graph remains DEFERRED — see 05-01/05-02 summaries.)

#[test]
fn merge_media_verification() {
    use jwlmanager_lib::jwlcore::merge::run_merge_with_lib_path;

    let Some(lib) = host_lib_or_skip("merge_media_verification") else {
        return;
    };
    let _lock = JWLCORE_TEST_LOCK.lock().unwrap();

    // --- Direct empirical observation: manually stage + merge (like merge_ffi)
    //     and LIST exactly what jwlCore wrote to the dest root beyond
    //     userData.db + the merge/ input subdir. --------------------------------
    let (_dest_fx, dest_archive) = common::generate_merge_dest_archive();
    let (_media_fx, media_source) = common::generate_media_bearing_merge_source();

    let (_dd, dest_extract) = common::extract_to_tempdir(&dest_archive);
    let stage = tempfile::TempDir::new().unwrap();
    std::fs::copy(
        dest_extract.join("userData.db"),
        stage.path().join("userData.db"),
    )
    .unwrap();
    let merge_dir = stage.path().join("merge");
    jwlmanager_lib::archive::extract::extract_zip_slip_safe(&media_source, &merge_dir).unwrap();

    run_merge_with_lib_path(&lib, stage.path(), &merge_dir, false)
        .expect("media-bearing merge must succeed (no PlaylistItem rows to abort it)");

    // Enumerate files jwlCore left at the staging ROOT beyond userData.db and
    // the merge/ input subdir.
    let mut extra_root_files: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(stage.path()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().unwrap().is_file() && !name.starts_with("userData.db") {
            extra_root_files.push(name);
        }
    }
    eprintln!(
        "merge_media_verification OBSERVED extra staging-root files: {extra_root_files:?} \
         (empty => branch (a): jwlCore wrote only userData.db; the source media blob \
         '{}' stayed in the merge/ input subdir)",
        common::MERGE_SOURCE_MEDIA_NAME
    );

    // On this host we observe branch (a): jwlCore relocated no loose media.
    // Assert the observed reality (documented above); if a future jwlCore build
    // relocates media, this assertion is the tripwire that flags it for review.
    assert!(
        extra_root_files.is_empty(),
        "jwlCore relocated loose media into the staging root ({extra_root_files:?}); \
         the fold-back branch (b) path now needs live verification"
    );
    let src_blob_at_root = stage.path().join(common::MERGE_SOURCE_MEDIA_NAME);
    assert!(
        !src_blob_at_root.exists(),
        "source media blob was copied to the dest root — fold-back branch (b) fired"
    );

    // --- End-to-end commit: the fold-back must be a correct no-op here, the
    //     session stays consistent, and the dest's pre-existing loose media is
    //     retained in session.entries. -----------------------------------------
    let (mut session, _n) = open_session(&dest_archive);
    let entries_before: Vec<String> = session.entries.iter().map(|e| e.name.clone()).collect();

    merge_commit_with_lib_path(&lib, &mut session, &media_source).expect("media commit succeeds");

    assert!(integrity_ok(&session.db_path), "post-commit integrity");
    // Branch (a): no new media entry was folded in (jwlCore wrote no loose media).
    let entries_after: Vec<String> = session.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(
        entries_after, entries_before,
        "no staging media => session.entries must be unchanged (fold-back no-op)"
    );
    // The dest's pre-existing loose media is still present + on disk.
    for name in [
        "media/test.png",
        "default_thumbnail.png",
        "future_unknown.dat",
    ] {
        assert!(
            entries_after.iter().any(|e| e == name),
            "pre-existing media entry {name} dropped from inventory"
        );
        assert!(
            session.temp_dir.path().join(name).exists(),
            "pre-existing media file {name} missing from temp_dir"
        );
    }
}
