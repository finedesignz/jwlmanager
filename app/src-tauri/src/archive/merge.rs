//! Two-archive merge orchestration (MERGE-01/MERGE-02, 05-02-PLAN.md) — the
//! dry-run + commit machinery layered on Wave 1's
//! `jwlcore::merge::run_merge_with_lib_path` FFI wrapper. The direct analogue of
//! Phase 4's `dry_run_downgrade` + `save_v14_copy` throwaway-copy pattern.
//!
//! jwlCore has NO preview mode: the ONLY way to know what a merge does is to
//! run the REAL merge. So the dry-run runs `mergeDatabase` on a `fs::copy` of
//! the live session DB inside a throwaway directory and snapshot-diffs the
//! before/after, then discards the copy — the live session is never touched.
//! Because both the preview and the commit run the IDENTICAL operation
//! (the same merge on a bit-identical `fs::copy` of `session.db_path`, over an
//! extraction of the SAME source archive), jwlCore's determinism guarantees the
//! preview's after-state is byte-for-byte the commit's after-state — the
//! preview provably equals the committed effect (criterion "Preview counts
//! equal the committed merge's effect").
//!
//! CONTENT-SIGNATURE DIFF (not PK-set). jwlCore does not only ADD rows — it
//! also UPDATES matched rows IN PLACE (a Note/UserMark with a matching Guid
//! whose content/color/timestamp changes, a `Location.Title` update). A PK-set
//! diff (`db::delete::diff_snapshots`) reports those in-place updates as ZERO
//! overwrites, so the preview would lie about a headline stat. This module
//! therefore uses [`snapshot_signatures`] — a per-row content hash keyed by the
//! single i64 PK — so `overwritten` counts rows whose CONTENT changed at the
//! same PK, not mere PK-set membership.
//!
//! MEDIA FOLD-BACK (Open-Q1 / Pitfall 2). jwlCore MIGHT copy referenced media
//! blobs out of the source dir into the destination dir during a merge. Any
//! such file must be folded into `session.entries` (and `session.temp_dir`) or
//! the next Save would silently drop it. [`fold_back_media`] walks the staging
//! dir after a successful merge and reconciles every non-`userData.db`,
//! non-`merge/` file against the session inventory.
//!
//! EMPIRICAL OBSERVATION (Task 3, this host — Windows x64, real
//! `jwlCore-amd64.dll`): on the synthetic fixtures exercised here, jwlCore
//! wrote ONLY `userData.db` into the destination staging dir — it did NOT emit
//! any loose media blob alongside it. The fold-back loop therefore fired its
//! NO-OP branch (no new media, nothing already-present-but-changed). The loop
//! is retained as correct, empirically-grounded defense: if a future jwlCore
//! build (or a media-bearing archive shape not reproducible in a minimal
//! synthetic fixture) does relocate a blob, the fold-back captures it. See
//! `tests/merge_orchestration.rs::merge_media_verification` for the recorded
//! observation, and `tests/fold_merge_tests.rs::fold_media_intermediate_step`
//! for the SAME no-op observation repeated at an intermediate (non-final)
//! N-way fold position (10-02-PLAN.md D10-04/A3).
//!
//! PLAYLIST MERGE-TABLE COVERAGE (D10-06, RESOLVED): Phase 5 only ever tried
//! a MINIMAL synthetic `PlaylistItem` with no backing graph, which jwlCore
//! aborted ("key not found: 0", 05-01-SUMMARY.md). `tests/fold_merge_tests.rs
//! ::fold_playlist_graph_merge` (10-02-PLAN.md) retried with Phase 8's
//! PROVEN full playlist-graph row set (`PlaylistItemAccuracy`, a
//! `PlaylistItem` with `ThumbnailFilePath`, its backing `IndependentMedia`
//! row, a `Location`, and the linking `PlaylistItemLocationMap` row, plus the
//! thumbnail file), inserted directly into a fold source's `userData.db` —
//! and it merges successfully. PlaylistItem/PlaylistItemLocationMap/
//! IndependentMedia are provably carried into the final folded result.
//! NEW FINDING while closing this gap: jwlCore does NOT preserve the
//! source's `PlaylistItemId` verbatim during a merge (unlike `Note`/
//! `UserMark`, which are Guid-identity-matched and keep their own PK) — it
//! REMAPS `PlaylistItem` rows to fresh destination ids to avoid a PK
//! collision, since `PlaylistItem` carries no Guid identity column, and
//! correctly repoints `PlaylistItemLocationMap` at the new id (referentially
//! consistent, not a partial copy). `[MERGE_SNAPSHOT_TABLES]`'s
//! `PlaylistItem`/`PlaylistItemMarker` content-signature diff is therefore
//! keyed by a PK the merge itself reassigns — a fold's `PlaylistItem` "added"
//! count is trustworthy, but a signature comparison keyed by the SOURCE's
//! original `PlaylistItemId` would silently read as "deleted+added" rather
//! than "same logical row, remapped"; no caller currently relies on that
//! distinction, but a future one should be aware of it.
//!
//! FOLD-FAILURE STAGING RESIDUE ON WINDOWS (10-02-PLAN.md Task 2, empirical
//! finding): when jwlCore's OWN internal-exception abort path fires (e.g.
//! the "key not found: 0" orphan-`PlaylistItem` fixture), it does NOT close
//! its destination-db sqlite handle before returning the failure code.
//! `sqlite3_open` on Windows does not request `FILE_SHARE_DELETE` by
//! default, so that leaked handle blocks this module's best-effort
//! `fs::remove_dir_all` cleanup of the FAILING step's `userData.db` (and its
//! re-extracted `merge/userData.db`) for the REST OF THE PROCESS — verified
//! empirically (`tests/fold_merge_tests.rs::fold_step_failure_pristine`):
//! five retries over 1.5s all fail identically with Windows os error 32
//! ("used by another process"), so this is not a transient race. This is a
//! genuine jwlCore-side resource leak on ITS error path, not a defect in
//! this module's cleanup (which is already `let _ =` best-effort and never
//! blocks the caller, never touches the live session DB, and never leaves a
//! HALF-promoted DB — the Core Value invariant this plan exists to prove
//! remains intact). The residue does not grow across repeated failed
//! attempts in the SAME process (the same already-locked path is overwritten
//! in place each retry, never a new leak per attempt) and is confined to
//! `userData.db`-named files — both proven by the same test. Not fixable
//! from this side without jwlCore source (MIT-only vendored binary,
//! no-new-dependency constraint); the practical impact is a small amount of
//! leftover temp-dir garbage under a LONG-RUNNING app session after a fold
//! failure caused by a jwlCore internal abort specifically, cleared on the
//! next process restart (the OS releases the handle).

use crate::archive::extract::extract_zip_slip_safe;
use crate::db::edit::DryRunReport;
use crate::error::ArchiveError;
use crate::session::{ArchiveSession, ZipEntryMeta};
use rusqlite::Connection;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// The merge-affected tables snapshotted for the content-signature before/after
/// diff — ONLY tables with a single-column integer PK (mirrors
/// `db::delete::TRACKED_TABLES`). `InputField` and every other composite-PK /
/// `WITHOUT ROWID` table is DELIBERATELY EXCLUDED: its key is composite
/// (`LocationId`, `TextTag`) with no single integer PK, so keying a signature
/// by a single i64 would be meaningless (and reading a PK column that does not
/// exist errors at runtime, breaking the whole dry-run — BLOCKER). These are
/// fixed compile-time identifiers, never user input (SAFE-02 / T-05-08).
const MERGE_SNAPSHOT_TABLES: &[(&str, &str)] = &[
    ("Note", "NoteId"),
    ("UserMark", "UserMarkId"),
    ("BlockRange", "BlockRangeId"),
    ("Bookmark", "BookmarkId"),
    ("Tag", "TagId"),
    ("TagMap", "TagMapId"),
    ("Location", "LocationId"),
    ("PlaylistItem", "PlaylistItemId"),
    ("PlaylistItemMarker", "PlaylistItemMarkerId"),
];

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::MergeFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Per-table map of `single-i64-PK -> content signature`, where the signature
/// is a hash of the row's FULL column tuple (all columns, in `SELECT *` order).
/// A row present in both a before- and after-snapshot whose signature CHANGED
/// was UPDATED in place by the merge — this is what a PK-set diff misses.
///
/// The hash is `DefaultHasher` (fixed-seed SipHash), so it is deterministic
/// within a single process — good enough because before/after are always
/// compared within one `dry_run_merge` / `content_diff` call, never persisted
/// or compared across processes. Reads are `SELECT * FROM <fixed const table>`
/// with no user input; the PK is bound only by column position.
fn snapshot_signatures(
    conn: &Connection,
    tables: &[(&str, &str)],
) -> Result<BTreeMap<String, BTreeMap<i64, u64>>, ArchiveError> {
    let mut out = BTreeMap::new();
    for (table, pk_col) in tables {
        let sql = format!("SELECT * FROM {table}");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| map_sqlite_err(e, "preparing signature scan"))?;
        let col_count = stmt.column_count();
        let pk_idx = stmt
            .column_names()
            .iter()
            .position(|c| c == pk_col)
            .ok_or_else(|| ArchiveError::MergeFailed {
                reason: format!("snapshot table {table} is missing PK column {pk_col}"),
            })?;

        let mut rows = stmt
            .query([])
            .map_err(|e| map_sqlite_err(e, "scanning signature rows"))?;
        let mut table_sigs: BTreeMap<i64, u64> = BTreeMap::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| map_sqlite_err(e, "reading signature row"))?
        {
            let pk: i64 = row
                .get(pk_idx)
                .map_err(|e| map_sqlite_err(e, "reading signature PK"))?;
            let mut hasher = DefaultHasher::new();
            for i in 0..col_count {
                let value: rusqlite::types::Value = row
                    .get(i)
                    .map_err(|e| map_sqlite_err(e, "reading signature column"))?;
                // Stable within-process rendering of the cell (mirrors the
                // test harness's `normalized_table_rows`): TEXT/INTEGER/REAL/
                // BLOB/NULL all Debug-render distinctly, so a content change in
                // any column changes the row signature.
                format!("{value:?}").hash(&mut hasher);
            }
            table_sigs.insert(pk, hasher.finish());
        }
        out.insert((*table).to_string(), table_sigs);
    }
    Ok(out)
}

/// Content-aware before/after diff into a [`DryRunReport`]:
///   - `added`       = PKs only in AFTER (new rows the merge carried in),
///   - `deleted`     = PKs only in BEFORE (rows the merge removed),
///   - `overwritten` = PKs in BOTH whose signature CHANGED (in-place UPDATEs).
///
/// This is the merge-specific replacement for `db::delete::diff_snapshots`
/// (which is PK-set only and would report every in-place UPDATE as 0
/// overwrites). Emits the SAME `DryRunReport` shape so the frontend +
/// `DeletePreviewDialog` binding is unchanged.
fn diff_signatures(
    before: &BTreeMap<String, BTreeMap<i64, u64>>,
    after: &BTreeMap<String, BTreeMap<i64, u64>>,
) -> DryRunReport {
    let mut report = DryRunReport::default();
    for (table, before_sigs) in before {
        let empty = BTreeMap::new();
        let after_sigs = after.get(table).unwrap_or(&empty);

        let mut added = 0usize;
        let mut deleted = 0usize;
        let mut overwritten = 0usize;

        for (pk, before_sig) in before_sigs {
            match after_sigs.get(pk) {
                None => deleted += 1,
                Some(after_sig) if after_sig != before_sig => overwritten += 1,
                Some(_) => {}
            }
        }
        for pk in after_sigs.keys() {
            if !before_sigs.contains_key(pk) {
                added += 1;
            }
        }

        if added > 0 {
            report.added.insert(table.clone(), added);
        }
        if overwritten > 0 {
            report.overwritten.insert(table.clone(), overwritten);
        }
        if deleted > 0 {
            report.deleted.insert(table.clone(), deleted);
        }
    }
    report.total_deleted = report.deleted.values().sum();
    report
}

/// Copies the live session DB into `root/userData.db`, extracts the (untrusted,
/// READ-ONLY) `source_archive` zip-slip-safely under `root/merge/`, then runs
/// the REAL jwlCore merge (from the vendored lib at `lib_path`) of the source
/// INTO the copy (in place, on the copy). Shared verbatim by both
/// [`dry_run_merge_with_lib_path`] (throwaway copy) and
/// [`merge_commit_with_lib_path`] (staging copy) so the two can never diverge —
/// the whole preview==commit guarantee rests on this being the SAME operation.
///
/// The `source_archive` file is only ever READ (extraction reads it; the merge
/// reads `root/merge/userData.db`) — the source bytes are never mutated
/// (MERGE-02, T-05-05).
fn stage_and_merge(
    lib_path: &Path,
    session: &ArchiveSession,
    source_archive: &Path,
    root: &Path,
) -> Result<(), ArchiveError> {
    stage_and_merge_from(lib_path, &session.db_path, source_archive, root)
}

/// Generalized [`stage_and_merge`]: copies `copy_from` into `root/userData.db`
/// (rather than always `session.db_path`), extracts the (untrusted, READ-ONLY)
/// `source_archive` zip-slip-safely under `root/merge/`, then runs the REAL
/// jwlCore merge of the source INTO the copy in place. `stage_and_merge` is a
/// one-line delegation to this function with `copy_from = &session.db_path`,
/// so every Phase 5 call site and test behaves bit-identically.
///
/// This is the primitive an N-way fold chains: step 1's `copy_from` is
/// `session.db_path` (the live session, untouched); step i>1's `copy_from` is
/// step (i-1)'s own `userData.db` — never `session.db_path` again (that would
/// collapse the fold to "last source wins", RESEARCH Pitfall 2).
///
/// The `source_archive` file is only ever READ (extraction reads it; the merge
/// reads `root/merge/userData.db`) — the source bytes are never mutated
/// (MERGE-02, T-05-05 / T-10-03).
fn stage_and_merge_from(
    lib_path: &Path,
    copy_from: &Path,
    source_archive: &Path,
    root: &Path,
) -> Result<(), ArchiveError> {
    fs::copy(copy_from, root.join("userData.db"))?;
    let merge_dir = root.join("merge");
    // D5-03/D11 zip-slip-safe extraction of the untrusted source archive.
    extract_zip_slip_safe(source_archive, &merge_dir)?;
    // jwlCore merges `<merge_dir>/userData.db` INTO `<root>/userData.db`
    // (downgrade = false, D5-07/D10-schema). MergeFailed propagates.
    crate::jwlcore::merge::run_merge_with_lib_path(lib_path, root, &merge_dir, false)
}

/// Opens `before_db` + `after_db` read-only, snapshots the content signatures
/// of [`MERGE_SNAPSHOT_TABLES`] in each, and returns the content-aware
/// [`DryRunReport`]. Both handles are scoped-and-dropped before returning.
///
/// `pub`: the orchestration test reuses this to compute the committed merge's
/// effect the SAME way the dry-run does, proving preview == commit.
pub fn content_diff(before_db: &Path, after_db: &Path) -> Result<DryRunReport, ArchiveError> {
    let before = {
        let conn = Connection::open(before_db)?;
        snapshot_signatures(&conn, MERGE_SNAPSHOT_TABLES)?
    };
    let after = {
        let conn = Connection::open(after_db)?;
        snapshot_signatures(&conn, MERGE_SNAPSHOT_TABLES)?
    };
    Ok(diff_signatures(&before, &after))
}

/// Previews a merge of `source_archive` into the open `session` WITHOUT
/// mutating the live session (criteria: preview is non-destructive; runs the
/// REAL merge on a bit-identical copy then discards it).
///
/// Flow: snapshot the live session DB (BEFORE — bit-identical to the copy
/// `stage_and_merge` is about to make); `stage_and_merge` into a throwaway dir
/// under `session.temp_dir`; snapshot the merged copy (AFTER); content-diff.
/// The throwaway is best-effort deleted on EVERY path (mirrors
/// `save_v14_copy`). No handle to `session.db_path` is held past its snapshot,
/// and the throwaway copy handle is dropped before returning.
///
/// The BEFORE snapshot reads `session.db_path` directly (read-only, immediately
/// dropped) rather than a separate copy: `stage_and_merge`'s first act is
/// `fs::copy(session.db_path -> root/userData.db)`, so the copy's pre-merge
/// content IS `session.db_path`'s content — snapshotting the live DB read-only
/// yields the identical BEFORE without a redundant copy, and a dry-run performs
/// no promote so holding a transient read handle on the live DB is safe here.
pub fn dry_run_merge(
    app: &tauri::AppHandle,
    session: &ArchiveSession,
    source_archive: &Path,
) -> Result<DryRunReport, ArchiveError> {
    // Resolve the vendored lib for THIS host first (T-05-02: arm64-windows /
    // missing binary -> MergeUnavailable, never a crash), then delegate to the
    // lib-path core.
    let lib_path = crate::jwlcore::merge::merge_availability(app)?;
    dry_run_merge_with_lib_path(&lib_path, session, source_archive)
}

/// Lib-path core of [`dry_run_merge`] (see it for the flow + safety rationale).
///
/// `pub` (not `pub(crate)`): the Task 3 orchestration integration test in
/// `tests/merge_orchestration.rs` links this crate as an EXTERNAL crate and
/// cannot obtain a `tauri::AppHandle` / a packaged resource dir, so it resolves
/// the host DLL via `jwlcore::merge::host_dev_lib_path()` and drives this entry
/// point — matching Wave 1's `run_merge_with_lib_path` deviation for exactly the
/// same reason (05-01-SUMMARY.md, Rule 3).
pub fn dry_run_merge_with_lib_path(
    lib_path: &Path,
    session: &ArchiveSession,
    source_archive: &Path,
) -> Result<DryRunReport, ArchiveError> {
    let root = session.temp_dir.path().join("merge_dryrun");

    let result = (|| {
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;

        // BEFORE: the live session DB (bit-identical to the copy below).
        let before = {
            let conn = Connection::open(&session.db_path)?;
            snapshot_signatures(&conn, MERGE_SNAPSHOT_TABLES)?
        };

        stage_and_merge(lib_path, session, source_archive, &root)?;

        // AFTER: the merged throwaway copy. Do NOT trim between merge and this
        // snapshot (Pitfall 4) — trim runs on the next Save, not here.
        let after = {
            let conn = Connection::open(root.join("userData.db"))?;
            snapshot_signatures(&conn, MERGE_SNAPSHOT_TABLES)?
        };

        Ok(diff_signatures(&before, &after))
    })();

    // Best-effort cleanup on every path — the throwaway is discarded.
    let _ = fs::remove_dir_all(&root);
    result
}

/// Commits a merge of `source_archive` into the open `session` SAFELY: runs the
/// merge on a STAGING copy under `session.temp_dir`, folds any new staging media
/// into `session.entries`, then ATOMICALLY promotes the merged DB onto
/// `session.db_path` via rename-with-replace (never `fs::copy`). Marks the
/// session dirty. The source archive is only READ.
///
/// ATOMIC PROMOTE (Core Value): the staging DB and `session.db_path` both live
/// under `session.temp_dir` (same filesystem), so
/// `archive::save::atomic_replace` (fs::rename-with-replace) is a single atomic
/// kernel call — a crash mid-promote leaves `session.db_path` as EITHER the
/// pristine pre-merge DB OR the fully-merged DB, never a truncated short file. A
/// `fs::copy` onto the live DB would open exactly that truncation window and is
/// forbidden. No open handle to `session.db_path` exists at promote time
/// (`stage_and_merge` only `fs::copy`s FROM it, closing immediately; the media
/// fold-back touches only `session.temp_dir` files), which Windows requires for
/// a replace.
///
/// Does NOT VACUUM/trim here — trim runs on the next Save via the existing path
/// (D5-Discretion: no double-trim).
pub fn merge_commit(
    app: &tauri::AppHandle,
    session: &mut ArchiveSession,
    source_archive: &Path,
) -> Result<(), ArchiveError> {
    // Resolve availability BEFORE staging (T-05-02), then delegate.
    let lib_path = crate::jwlcore::merge::merge_availability(app)?;
    merge_commit_with_lib_path(&lib_path, session, source_archive)
}

/// Lib-path core of [`merge_commit`] (see it for the atomic-promote rationale).
///
/// `pub`: driven by the Task 3 orchestration integration test — same rationale
/// as [`dry_run_merge_with_lib_path`].
pub fn merge_commit_with_lib_path(
    lib_path: &Path,
    session: &mut ArchiveSession,
    source_archive: &Path,
) -> Result<(), ArchiveError> {
    let staging = session.temp_dir.path().join("merge_staging");
    let staged_db = staging.join("userData.db");

    let result = (|| {
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging)?;

        stage_and_merge(lib_path, session, source_archive, &staging)?;

        // Fold any NEW media jwlCore wrote into the staging dir back into the
        // session inventory BEFORE the staging dir is cleaned (Pitfall 2).
        fold_back_media(session, &staging)?;

        // PROMOTE atomically — rename the merged staging DB onto the live DB.
        crate::archive::save::atomic_replace(&staged_db, &session.db_path)?;

        session.dirty = true;
        Ok(())
    })();

    // Best-effort cleanup of the staging dir on every path. On the success path
    // the merged DB has already been renamed OUT of staging onto the live DB;
    // this removes only the leftover `merge/` extraction (and any media already
    // folded back into `session.temp_dir`).
    let _ = fs::remove_dir_all(&staging);
    result
}

// ---------------------------------------------------------------------------
// N-WAY FOLD (MERGE-03, 10-01-PLAN.md) — the pairwise machinery above
// generalized to N sources: `dest = merge(merge(merge(dest, s1), s2), s3)`,
// in the CALLER's list order. NOT a new algorithm: `run_fold_chain` is
// `sources.len()` correctly-sequenced calls to `stage_and_merge_from`, shared
// verbatim by the aggregate dry-run and the commit so the two can never
// diverge — the same preview==commit guarantee `stage_and_merge` already
// gives the Phase 5 pair.
// ---------------------------------------------------------------------------

/// Minimum sources an N-way fold accepts at the command boundary (D10-06):
/// 1-2 archives already have the shipped `dry_run_merge`/`merge_commit` path;
/// fewer than this is a caller bug and is rejected with `MergeFailed`, never
/// silently degraded to a Phase-5-equivalent single merge.
const FOLD_MIN_SOURCES: usize = 3;

fn require_fold_sources(sources: &[PathBuf]) -> Result<(), ArchiveError> {
    if sources.len() < FOLD_MIN_SOURCES {
        return Err(ArchiveError::MergeFailed {
            reason: format!(
                "fold requires at least {FOLD_MIN_SOURCES} sources, got {}",
                sources.len()
            ),
        });
    }
    Ok(())
}

/// Runs the shared N-way fold chain: `dest = merge(merge(merge(seed_db, s1),
/// s2), s3)`, in the CALLER's `sources` order verbatim — never sorted,
/// deduplicated, or normalized (D10-01). Order is REAL: `jwlCore.mergeDatabase`
/// does in-place content UPDATEs at matched identity keys (module docs at the
/// top of this file), so a LATER source in the list wins a contested key over
/// an earlier one. `fold(A,B,C) != fold(A,C,B)` is CORRECT behaviour by
/// design, not a defect — reordering the source list legitimately changes the
/// result, and this function must never be changed to paper over that.
///
/// Shared verbatim by both [`fold_dry_run_merge_with_lib_path`] (throwaway
/// root) and [`fold_merge_commit_with_lib_path`] (staging root) — the single
/// point that makes the aggregate preview and the committed result unable to
/// diverge.
///
/// Creates `root/step_1` .. `root/step_N` in order. Step 1 seeds from
/// `seed_db` (the caller passes `session.db_path` — the live session, read
/// only); step i>1 seeds from step (i-1)'s OWN `userData.db`, tracked
/// explicitly as `prev_step_db` rather than recomputed from a path formula —
/// re-seeding step i>1 from `seed_db` again would collapse the fold to "last
/// source wins" (RESEARCH Pitfall 2, and a `MUST NOT` prohibition of this
/// plan). Never touches `session.db_path` other than that one step-1 read —
/// no promote happens here; that is the caller's job, exactly once, after
/// this returns `Ok`.
///
/// Calls `on_step_ok(step_dir)` after each successful step (the commit path
/// uses this to fold media back after EVERY step, D10-04). On any step's
/// error (from staging the merge OR from `on_step_ok`), wraps a `MergeFailed`
/// reason with the 1-indexed source position (`source {i} of {n}: {inner}`,
/// RESEARCH Open Question 2) and returns immediately — later steps never run.
/// On success, returns the final step's `userData.db` path.
fn run_fold_chain(
    lib_path: &Path,
    seed_db: &Path,
    sources: &[PathBuf],
    root: &Path,
    mut on_step_ok: impl FnMut(&Path) -> Result<(), ArchiveError>,
) -> Result<PathBuf, ArchiveError> {
    let wrap_step_reason = |step_num: usize, err: ArchiveError| -> ArchiveError {
        match err {
            ArchiveError::MergeFailed { reason } => ArchiveError::MergeFailed {
                reason: format!("source {step_num} of {}: {reason}", sources.len()),
            },
            other => other,
        }
    };

    let mut prev_step_db = seed_db.to_path_buf();

    for (idx, source_archive) in sources.iter().enumerate() {
        let step_num = idx + 1;
        let step_dir = root.join(format!("step_{step_num}"));
        fs::create_dir_all(&step_dir)?;

        stage_and_merge_from(lib_path, &prev_step_db, source_archive, &step_dir)
            .map_err(|err| wrap_step_reason(step_num, err))?;

        on_step_ok(&step_dir).map_err(|err| wrap_step_reason(step_num, err))?;

        prev_step_db = step_dir.join("userData.db");
    }

    Ok(prev_step_db)
}

/// Previews an N-way fold of `sources` into the open `session`, in list order,
/// WITHOUT mutating the live session (MERGE-03 criterion 2 / D10-05): runs the
/// SAME [`run_fold_chain`] the commit uses, under a throwaway root, then
/// `content_diff`s the ORIGINAL session DB against the FINAL folded state — so
/// a row overwritten at step 2 and again at step 3 is reported ONCE, with the
/// step-3 content. Rejects fewer than [`FOLD_MIN_SOURCES`] sources before
/// creating any directory.
pub fn fold_dry_run_merge(
    app: &tauri::AppHandle,
    session: &ArchiveSession,
    sources: &[PathBuf],
) -> Result<DryRunReport, ArchiveError> {
    let lib_path = crate::jwlcore::merge::merge_availability(app)?;
    fold_dry_run_merge_with_lib_path(&lib_path, session, sources)
}

/// Lib-path core of [`fold_dry_run_merge`] — `pub` for the same
/// externally-linked-integration-test reason as [`dry_run_merge_with_lib_path`].
pub fn fold_dry_run_merge_with_lib_path(
    lib_path: &Path,
    session: &ArchiveSession,
    sources: &[PathBuf],
) -> Result<DryRunReport, ArchiveError> {
    require_fold_sources(sources)?;
    let root = session.temp_dir.path().join("fold_dryrun");

    let result = (|| {
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;

        let final_step_db =
            run_fold_chain(lib_path, &session.db_path, sources, &root, |_step_dir| Ok(()))?;

        content_diff(&session.db_path, &final_step_db)
    })();

    // Best-effort cleanup on every path — the ONE fold root, never per-step.
    let _ = fs::remove_dir_all(&root);
    result
}

/// Commits an N-way fold of `sources` into the open `session`, in list order
/// (MERGE-03 criterion 1 / D10-01): runs `sources.len()` sequential merges
/// under ONE staging root via [`run_fold_chain`] (step i>1 seeded from step
/// (i-1)'s own `userData.db`, never re-seeded from `session.db_path` —
/// T-10-02 / Pitfall 2), folding media back after EVERY completed step
/// (D10-04 — conservative by decision: an intermediate step's media would
/// otherwise be dropped by the next step's re-seed). Only after the FINAL
/// step returns `Ok` does this call `archive::save::atomic_replace` EXACTLY
/// ONCE, promoting the final step's `userData.db` onto `session.db_path`
/// (rename-with-replace, never `fs::copy` — a mid-promote crash must not
/// truncate the live DB); only after THAT returns `Ok` does it set
/// `session.dirty = true`. A step failure leaves `session.db_path` untouched
/// and `session.dirty` unset (Core Value). Rejects fewer than
/// [`FOLD_MIN_SOURCES`] sources before creating any directory.
pub fn fold_merge_commit(
    app: &tauri::AppHandle,
    session: &mut ArchiveSession,
    sources: &[PathBuf],
) -> Result<(), ArchiveError> {
    let lib_path = crate::jwlcore::merge::merge_availability(app)?;
    fold_merge_commit_with_lib_path(&lib_path, session, sources)
}

/// Lib-path core of [`fold_merge_commit`] — `pub` for the same
/// externally-linked-integration-test reason as [`merge_commit_with_lib_path`].
pub fn fold_merge_commit_with_lib_path(
    lib_path: &Path,
    session: &mut ArchiveSession,
    sources: &[PathBuf],
) -> Result<(), ArchiveError> {
    require_fold_sources(sources)?;
    let root = session.temp_dir.path().join("fold_staging");

    let result = (|| {
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;

        // Read-only snapshot of the seed path BEFORE the loop borrows `session`
        // mutably (for the per-step media fold-back) — the fold never re-reads
        // `session.db_path` after step 1 (T-10-02).
        let seed_db = session.db_path.clone();
        let final_step_db = run_fold_chain(lib_path, &seed_db, sources, &root, |step_dir| {
            fold_back_media(session, step_dir)
        })?;

        // PROMOTE atomically, EXACTLY ONCE, after the LAST step succeeded —
        // never inside the loop.
        crate::archive::save::atomic_replace(&final_step_db, &session.db_path)?;

        session.dirty = true;
        Ok(())
    })();

    // Best-effort cleanup of the ONE fold root on every path (never per-step).
    let _ = fs::remove_dir_all(&root);
    result
}

/// Folds media jwlCore may have written into the staging dir back into the
/// session inventory so a later Save keeps it (Open-Q1 / T-05-06). Walks every
/// file under `staging` EXCEPT the merged `userData.db*` and the `merge/`
/// source-extraction subdir:
///   - name NOT in `session.entries`: copy it into `session.temp_dir` at the
///     same relative path and push a `ZipEntryMeta` so `rebuild_zip` includes
///     it.
///   - name ALREADY in `session.entries`: COMPARE content (size then bytes); if
///     DIFFERENT, copy the staging version OVER the stale `session.temp_dir`
///     copy (no new `ZipEntryMeta`). Skipping a name-present entry would let
///     Save zip the STALE blob — a silent media mismatch. If jwlCore names
///     media content-addressed (same name => same bytes), the compare is a
///     cheap no-op — but we compare, never assume.
///
/// Empirically a NO-OP on this host's fixtures (jwlCore wrote only
/// `userData.db`) — see module docs.
fn fold_back_media(session: &mut ArchiveSession, staging: &Path) -> Result<(), ArchiveError> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    collect_staging_media(staging, staging, &mut found)?;

    for (rel, staged_path) in found {
        let temp_target = session.temp_dir.path().join(&rel);
        let already_present = session.entries.iter().any(|e| e.name == rel);

        if already_present {
            if files_differ(&staged_path, &temp_target)? {
                if let Some(parent) = temp_target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&staged_path, &temp_target)?;
            }
        } else {
            if let Some(parent) = temp_target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&staged_path, &temp_target)?;
            session.entries.push(ZipEntryMeta { name: rel });
        }
    }
    Ok(())
}

/// Recursively collects `(zip-style relative name, absolute path)` for every
/// file under `dir`, skipping the `merge/` source-extraction subdir and the
/// merged `userData.db*` family at the staging root. Relative names use `/`
/// separators to match zip entry names.
fn collect_staging_media(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), ArchiveError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let rel = rel_name(root, &path);

        if file_type.is_dir() {
            // Skip the source extraction — it is the merge INPUT, not output.
            if rel == "merge" {
                continue;
            }
            collect_staging_media(root, &path, out)?;
        } else if file_type.is_file() {
            // The merged dest DB (and any -wal/-shm/-journal sibling) is
            // promoted separately, never folded as media.
            if !rel.contains('/') && rel.starts_with("userData.db") {
                continue;
            }
            out.push((rel, path));
        }
    }
    Ok(())
}

/// Builds a zip-style (`/`-separated) relative name for `path` under `root`.
fn rel_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// True if `a` and `b` differ in size or bytes (a missing `b` counts as
/// different). Cheap size check first, byte compare only on size match.
fn files_differ(a: &Path, b: &Path) -> Result<bool, ArchiveError> {
    if !b.exists() {
        return Ok(true);
    }
    let (meta_a, meta_b) = (fs::metadata(a)?, fs::metadata(b)?);
    if meta_a.len() != meta_b.len() {
        return Ok(true);
    }
    Ok(fs::read(a)? != fs::read(b)?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn diff_signatures_classifies_added_deleted_overwritten() {
        let mut before: BTreeMap<String, BTreeMap<i64, u64>> = BTreeMap::new();
        let mut after: BTreeMap<String, BTreeMap<i64, u64>> = BTreeMap::new();

        // Note: pk 1 unchanged, pk 2 content-changed (overwrite), pk 3 deleted,
        // pk 4 added.
        before.insert(
            "Note".to_string(),
            BTreeMap::from([(1, 10), (2, 20), (3, 30)]),
        );
        after.insert(
            "Note".to_string(),
            BTreeMap::from([(1, 10), (2, 99), (4, 40)]),
        );

        let report = diff_signatures(&before, &after);
        assert_eq!(report.added.get("Note"), Some(&1));
        assert_eq!(report.overwritten.get("Note"), Some(&1));
        assert_eq!(report.deleted.get("Note"), Some(&1));
        assert_eq!(report.total_deleted, 1);
    }

    #[test]
    fn diff_signatures_same_pk_changed_content_is_overwrite_not_zero() {
        // The core soundness property: a row present in BOTH at the same PK
        // whose signature changed must count as `overwritten` (a PK-set diff
        // would report 0 here).
        let before: BTreeMap<String, BTreeMap<i64, u64>> =
            BTreeMap::from([("Location".to_string(), BTreeMap::from([(1, 111)]))]);
        let after: BTreeMap<String, BTreeMap<i64, u64>> =
            BTreeMap::from([("Location".to_string(), BTreeMap::from([(1, 222)]))]);
        let report = diff_signatures(&before, &after);
        assert_eq!(report.overwritten.get("Location"), Some(&1));
        assert!(report.added.is_empty());
        assert!(report.deleted.is_empty());
    }

    #[test]
    fn rel_name_uses_forward_slashes() {
        let root = Path::new("/tmp/stage");
        let path = Path::new("/tmp/stage/media/blob.bin");
        assert_eq!(rel_name(root, path), "media/blob.bin");
    }

    #[test]
    fn require_fold_sources_rejects_fewer_than_three() {
        let sources: Vec<PathBuf> = vec![PathBuf::from("a"), PathBuf::from("b")];
        match require_fold_sources(&sources) {
            Err(ArchiveError::MergeFailed { reason }) => {
                assert!(reason.contains('2'), "reason should mention the count: {reason}");
            }
            other => panic!("expected MergeFailed, got {other:?}"),
        }
        assert!(require_fold_sources(&[
            PathBuf::from("a"),
            PathBuf::from("b"),
            PathBuf::from("c")
        ])
        .is_ok());
    }

    #[test]
    fn merge_snapshot_tables_excludes_composite_pk_tables() {
        // InputField (composite PK LocationId,TextTag) must never be in the
        // single-i64-PK snapshot set (feeding it a single-PK read would break
        // the dry-run at runtime).
        assert!(!MERGE_SNAPSHOT_TABLES
            .iter()
            .any(|(t, _)| *t == "InputField"));
        assert!(MERGE_SNAPSHOT_TABLES.iter().any(|(t, _)| *t == "Note"));
    }
}
