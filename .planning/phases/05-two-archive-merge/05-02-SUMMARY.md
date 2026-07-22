---
phase: 05-two-archive-merge
plan: 02
subsystem: archive/merge
tags: [merge, jwlcore, dry-run, atomic-replace, content-signature-diff, media-fold-back, tauri-command, ffi]
requires:
  - phase: 05-01
    provides: [run_merge_with_lib_path, merge_availability, host_dev_lib_path, MergeUnavailable, MergeFailed, generate_merge_pair]
  - phase: 04-02
    provides: [save_v14_copy throwaway-copy pattern, atomic_replace]
  - phase: 02-02
    provides: [DryRunReport, snapshot_tables, diff_snapshots]
provides:
  - "archive::merge::dry_run_merge / merge_commit orchestration (throwaway-copy preview + atomic-promote commit)"
  - "content-signature diff (snapshot_signatures / diff_signatures / content_diff) — overwrite = in-place UPDATE"
  - "media fold-back into session.entries (empirically a no-op on jwlCore's dir-pair merge)"
  - "merge_dry_run + merge_commit Tauri commands"
affects: [phase-05-merge-ui, frontend-merge-flow, save]
tech-stack:
  added: []
  patterns:
    - "content-signature snapshot diff (per-row hash keyed by single i64 PK) — catches in-place UPDATEs a PK-set diff misses"
    - "app-resolves-availability + lib-path-core split for FFI testability (mirrors Wave 1 run_merge_with_lib_path)"
    - "atomic-promote-not-copy for live-DB mutation (reuse save::atomic_replace rename-with-replace)"
    - "empirically-grounded media fold-back (observe jwlCore staging output, assert branch, document)"
key-files:
  created:
    - app/src-tauri/src/archive/merge.rs
    - app/src-tauri/tests/merge_orchestration.rs
  modified:
    - app/src-tauri/src/archive/mod.rs
    - app/src-tauri/src/archive/save.rs
    - app/src-tauri/src/jwlcore/merge.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/common/mod.rs
key-decisions:
  - "D5-02a: merge dry-run uses a CONTENT-signature diff (snapshot_signatures), NOT the PK-set diff_snapshots — jwlCore UPDATEs matched rows in place, which a PK-set diff reports as 0 overwrites (proven by merge_overwrite_content_counted)"
  - "D5-02b: MERGE_SNAPSHOT_TABLES = single-i64-PK tables ONLY; InputField (composite PK) EXCLUDED (a single-PK read on it errors at runtime)"
  - "D5-02c: merge_commit promotes via save::atomic_replace (fs::rename-with-replace), NEVER fs::copy — a crash mid-copy truncates the live DB (Core Value); made atomic_replace pub(crate)"
  - "D5-02d: BEFORE snapshot reads session.db_path read-only (bit-identical to the staged copy stage_and_merge makes) — no redundant copy; dry-run holds no promote-blocking handle"
  - "Rule 3 deviation: removed Wave 1's app-taking run_merge; split into merge_availability + run_merge_with_lib_path, added pub dry_run_merge_with_lib_path / merge_commit_with_lib_path so the integration test drives the real DLL without an AppHandle"
  - "Playlist-table merge coverage DEFERRED (jwlCore playlist merge needs a fuller graph than a minimal synthetic fixture; a lone PlaylistItem aborts it) — reused as the deterministic failure source for the atomic-promote pristine leg"
patterns-established:
  - "content_diff(before_db, after_db): reusable content-signature report used to prove preview == commit"
  - "media fold-back walk: skip merge/ input subdir + userData.db* family; compare-then-replace same-name blobs"
requirements-completed: [MERGE-01, MERGE-02]
metrics:
  duration: ~1.5h
  completed: 2026-07-22
status: complete
---

# Phase 5 Plan 02: Merge Dry-Run + Commit Orchestration Summary

The dry-run preview + safe commit built on Wave 1's `mergeDatabase` FFI wrapper — the merge analogue of Phase 4's `dry_run_downgrade` + `save_v14_copy` throwaway-copy machinery. jwlCore has no preview mode, so the preview runs the REAL merge on a bit-identical `fs::copy` of the live session DB inside a throwaway dir and snapshot-diffs it, then discards the copy; the commit runs the identical merge on a staging copy and promotes the result onto `session.db_path` with the same atomic rename-with-replace a Save uses. Because preview and commit run the SAME operation on a bit-identical start, the preview provably equals the committed effect (proven by test, not just argued). Exposed as `merge_dry_run` + `merge_commit` Tauri commands. Verified end-to-end against the REAL `jwlCore-amd64.dll` on Windows x64.

## What shipped

- **`archive/merge.rs`** — the orchestration module:
  - `MERGE_SNAPSHOT_TABLES` — single-i64-PK tables only (Note, UserMark, BlockRange, Bookmark, Tag, TagMap, Location, PlaylistItem, PlaylistItemMarker). InputField and other composite-PK/`WITHOUT ROWID` tables are excluded (a single-PK read on them errors at runtime — BLOCKER).
  - `snapshot_signatures(conn, tables)` — per-row content hash (`DefaultHasher` over every column, keyed by the single i64 PK). `diff_signatures` classifies `added` (PK only in AFTER), `deleted` (PK only in BEFORE), `overwritten` (PK in BOTH, signature CHANGED). This is the **content-aware** replacement for the PK-set `diff_snapshots` — the whole point is that jwlCore UPDATEs matched rows in place, which a PK-set diff reports as 0 overwrites.
  - `stage_and_merge(lib_path, session, source, root)` — the ONE shared operation: `fs::copy(session.db_path -> root/userData.db)`, `extract_zip_slip_safe(source -> root/merge)`, `run_merge_with_lib_path(root, root/merge, false)`. Source archive is READ-ONLY.
  - `dry_run_merge` / `dry_run_merge_with_lib_path` — BEFORE snapshot (live DB, bit-identical to the copy), stage_and_merge into a throwaway under `session.temp_dir`, AFTER snapshot, content-diff; best-effort cleanup on every path. No trim between merge and AFTER (Pitfall 4).
  - `merge_commit` / `merge_commit_with_lib_path` — stage_and_merge into a staging dir, `fold_back_media`, then `save::atomic_replace(staging/userData.db -> session.db_path)` (rename, never copy), `session.dirty = true`; best-effort staging cleanup. No open handle to `session.db_path` at promote time (Windows requirement).
  - `content_diff(before_db, after_db)` — `pub` helper the test uses to compute the committed effect the SAME way the dry-run does (proves preview == commit).
  - `fold_back_media` + `collect_staging_media` — walk the staging dir for files jwlCore wrote beyond `userData.db*` and the `merge/` input subdir; new name -> copy into `session.temp_dir` + push `ZipEntryMeta`; already-present name -> compare content, replace stale copy if different.
  - 4 co-located unit tests (diff classification, same-PK-changed-content overwrite, rel-name slashing, composite-PK-table exclusion).
- **`merge_dry_run` + `merge_commit` Tauri commands** (`lib.rs`) — lock SessionState, `as_ref`/`as_mut`, map every `ArchiveError` via `to_dto` (no `reason` leak), registered in `generate_handler!`. FFI + `getLastResult` read happen under the single lock critical section (D5-06).
- **`save::atomic_replace` promoted to `pub(crate)`** so the merge commit reuses the exact rename-with-replace primitive (never a byte copy onto the live DB).
- **`jwlcore/merge.rs`**: removed Wave 1's now-superseded app-taking `run_merge`; `merge_availability` + `run_merge_with_lib_path` are the two building blocks (`archive::merge` resolves availability once, then drives the lib-path core).
- **Orchestration fixtures** (`tests/common/mod.rs`): `generate_merge_dest_archive`, `generate_merge_source_archive`, `generate_merge_overwrite_pair_archives` (shared identities with CHANGED content + newer LastModified), `generate_media_bearing_merge_source` (IndependentMedia row + loose blob), `generate_merge_failing_source_archive` (lone PlaylistItem -> deterministic jwlCore abort).
- **`tests/merge_orchestration.rs`** — 5 real-DLL tests (skip-as-pass off-host via `host_dev_lib_path`).

## Verification (DoD)

- `cargo fmt --check` — **clean** (after `cargo fmt`).
- `cargo clippy --all-targets -- -D warnings` — **clean** (exit 0; only the pre-existing ts-rs `try_from` macro-parse note, not a lint failure).
- `cargo test` (full workspace) — **~139 passed, 0 failed, 6 ignored** (all ignored are pre-existing env/manual-gated: differential Python oracles, real-archive round-trip, delete/trim ignored cases). `tests/merge_orchestration.rs`: **5 passed**; `tests/merge_ffi.rs`: **1 passed**; lib unit tests: **40 passed** (includes the 4 new merge.rs unit tests).
- **Real DLL loaded for the orchestration tests: YES** — host Windows x64, vendored `libs/jwlCore-amd64.dll` (+ co-located `sqlite3_64.dll` via the loader PATH-prepend). All 5 orchestration tests RAN (not skipped) and passed against the real binary.
- `npm run build` — **not run**: no frontend files touched (backend + tests only; the DryRunReport/ErrorDto bindings are unchanged).

## Empirical findings (resolved open questions)

- **Media-blob open question (A1 / Open-Q1) — RESOLVED.** `merge_media_verification` staged a media-bearing source (IndependentMedia row + loose `src_blob.bin`) and listed exactly what jwlCore wrote to the destination staging root beyond `userData.db`:

  > `merge_media_verification OBSERVED extra staging-root files: []` (empty)

  **jwlCore's directory-pair merge wrote ONLY `userData.db`** — it did NOT emit or relocate the source's loose media blob into the dest root. So the media fold-back fired its **branch (a)** no-op: no new `ZipEntryMeta`, and the destination's pre-existing loose media (`media/test.png`, `default_thumbnail.png`, `future_unknown.dat`) is retained unchanged in `session.entries`. Branch (b) (same-name-different-content blob replaced) was NOT observed on these fixtures. The fold-back loop is retained as correct, empirically-grounded defense: the test asserts `extra_root_files.is_empty()`, so if a future jwlCore build DOES relocate media the assertion trips and flags branch (b) for live verification.
- **In-place UPDATE is real.** `merge_overwrite_content_counted` proved jwlCore updates a matched row's content at the same PK (Note content / UserMark color / Location.Title changed with a newer source LastModified) and the content-signature diff reports `overwritten >= 1` — a PK-set diff would have reported 0. This validates the D5-02a decision to NOT reuse `diff_snapshots`.

## Playlist-coverage decision

**DEFERRED, documented honestly (not silently claimed).** Wave 1 found that a minimal synthetic `PlaylistItem` aborts jwlCore's playlist merge (`"key not found: 0"`), needing a fuller playlist graph (thumbnail/IndependentMedia/markers/maps) than a minimal synthetic fixture reproduces. Rather than seed a fragile full graph, this wave:
- Keeps `PlaylistItem`/`PlaylistItemMarker` in `MERGE_SNAPSHOT_TABLES` (they are snapshotted — a harmless empty-table no-op on these fixtures, so the diff is correct if a real archive DOES carry playlist rows), and
- Reuses the abort as a feature: `generate_merge_failing_source_archive` (a lone PlaylistItem) is the deterministic failure source for `merge_commit_promote_atomic`'s pristine leg, exercising the `MergeFailed` path end-to-end.
The module docs + this summary record that playlist-table MERGE coverage (source playlist rows carried into dest) is not yet asserted, and why. A future plan wanting it must build a full valid playlist graph fixture.

## Deviations from Plan

### Auto-fixed / auto-decided (no user permission needed)

**1. [Rule 3 - Blocking] Split `run_merge` into `merge_availability` + `run_merge_with_lib_path`; added `pub dry_run_merge_with_lib_path` / `merge_commit_with_lib_path`.**
- **Found during:** Task 3.
- **Issue:** The plan's public `dry_run_merge`/`merge_commit` take `&tauri::AppHandle` (to resolve the lib path via `resolve_lib_path(app)`). The Task 3 integration test links the crate as an EXTERNAL crate and cannot construct an `AppHandle` or a packaged resource dir — it can only resolve the host DLL via the `pub host_dev_lib_path()` Wave 1 shipped for exactly this reason.
- **Fix:** The `app`-taking public functions resolve `merge_availability(app)?` once, then delegate to a `pub *_with_lib_path` core that the test drives with `host_dev_lib_path()`. Wave 1's combined app-taking `run_merge` became unused and was removed (its two building blocks remain). This mirrors Wave 1's own `run_merge_with_lib_path` deviation verbatim (05-01-SUMMARY.md).
- **Files:** `app/src-tauri/src/archive/merge.rs`, `app/src-tauri/src/jwlcore/merge.rs`. **Commit:** `3cdea5da`.

**2. [Rule 3 - Blocking] Made `save::atomic_replace` `pub(crate)`.**
- **Found during:** Task 1.
- **Issue:** The plan requires `merge_commit` to promote via `archive::save::atomic_replace`, but it was a private `fn` in `save.rs`.
- **Fix:** Promoted to `pub(crate)` with a doc note explaining the merge-commit reuse (same-filesystem rename-with-replace; never `fs::copy` onto the live DB). **Files:** `app/src-tauri/src/archive/save.rs`. **Commit:** `ac743f70`.

## Known Stubs

None. Every shipped function is fully implemented and exercised against the real DLL. The media fold-back loop is a verified no-op on jwlCore's current behavior (branch a), not a stub — its branch-(b) path is defensive and guarded by an assertion tripwire.

## Self-Check: PASSED

- `app/src-tauri/src/archive/merge.rs` — FOUND
- `app/src-tauri/tests/merge_orchestration.rs` — FOUND
- Commit `ac743f70` (Task 1), `e1bf115a` (Task 2), `3cdea5da` (Task 3) — all present in git log.
- `cargo test` full workspace green; 5 merge_orchestration tests passed against the real `jwlCore-amd64.dll`.
