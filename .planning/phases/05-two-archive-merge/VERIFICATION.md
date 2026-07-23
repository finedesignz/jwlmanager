---
phase: 05-two-archive-merge
verified: 2026-07-23T00:00:00Z
status: passed
score: 4/4 success criteria verified (+ 6/6 data-integrity invariants)
behavior_unverified: 0
overrides_applied: 0
ship_verdict: SHIP
warnings:
  - "Playlist-table MERGE coverage deferred: source PlaylistItem/PlaylistItemMarker rows carried into the destination are NOT asserted. jwlCore aborts on a minimal synthetic PlaylistItem ('key not found: 0'); a full valid playlist graph fixture is needed. Snapshotted (empty no-op) on current fixtures, honestly documented in both summaries + module docs — never silently claimed as covered."
  - "ROADMAP bookkeeping lag: Phase 5 row still reads 'In Progress 2/3' and plan 05-03 is unchecked, but 05-03-SUMMARY.md exists (status: Complete) and all its code + tests are present and green. Stale checkbox, not a functional gap."
evidence_run:
  - "cargo test --jobs 2 (full workspace) → exit 0, 0 failed"
  - "cargo test --jobs 2 --test merge_ffi --test merge_orchestration → 1 + 5 passed, RAN against real jwlCore-amd64.dll (not skipped)"
  - "cargo test --jobs 2 --test differential -- --ignored → 4/4 pass incl rust_ffi_merge_matches_python_merge (real DLL + Python 3.13.3/PySide6)"
  - "npx vitest run → 43 passed (5 files); CommandBar.test.tsx → 20 passed"
---

# Phase 5: Two-Archive Merge — Verification Report

**Phase Goal:** User can merge two archives via the jwlCore native engine with the same safety net as any other destructive operation, and trust the result matches the proven Python app.
**Verified:** 2026-07-23
**Status:** PASSED — SHIP
**Re-verification:** No — initial verification
**Core Value guard:** Never lose or corrupt a user's archive. This phase calls a closed-source native lib that mutates a SQLite DB in place. Every mutation path was traced for a corruption window; none found.

## Verdict: SHIP

All 4 ROADMAP success criteria are VERIFIED in the codebase and proven by tests that RAN (not skipped) against the real `jwlCore-amd64.dll` and real Python 3.13.3. All 6 data-integrity invariants hold. One honestly-deferred gap (playlist-table merge coverage) is a WARNING, not a blocker — no success criterion depends on it, and it is documented rather than concealed.

## Success Criteria (ROADMAP contract)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | App loads correct jwlCore binary for host OS+CPU arch (incl arm64) automatically | ✓ PASS | `loader.rs::resolve_lib_name` maps windows/linux/macos × x86_64/aarch64; arm64-windows → typed `NoBinaryReason::Arm64Windows` (no crash). `merge_availability` checks arch FIRST before any load. 7 arch unit tests + `merge_ffi` loaded + merged against the REAL amd64 DLL on this host. The Windows `sqlite3_64.dll` PATH-prepend is reused verbatim from Phase 1 (not re-implemented). |
| 2 | User sees dry-run preview (add/overwrite/delete) before merge and can cancel | ✓ PASS | `dry_run_merge` runs the REAL merge on a throwaway `fs::copy`, snapshot-diffs, discards. Returns `DryRunReport{added, overwritten, deleted}`. `CommandBar.tsx` shows it via `DeletePreviewDialog`; confirm→`merge_commit`, cancel→no-op. Tests: `merge_dry_run_matches_commit` (preview == committed effect), `merge_overwrite_content_counted`, CommandBar vitest (preview shown, cancel never invokes commit). |
| 3 | Merging two fixtures matches Python app output, semantic round-trip | ✓ PASS | `rust_ffi_merge_matches_python_merge` RAN and PASSED: same synthetic pair merged via Rust FFI AND Python `jwlcore.merge_databases`, normalized table state EQUAL across snapshot tables, NEVER byte-diff. Real DLL + Python 3.13.3/PySide6. (Caveat: playlist tables empty in fixture — see WARNING.) |
| 4 | Missing/failed native lib → clear actionable error, not a crash | ✓ PASS | Typed `ArchiveError::MergeUnavailable` / `MergeFailed{reason}`; `to_dto` → codes `merge_unavailable`/`merge_failed` (reason never leaked). `merge_unavailable_is_actionable_not_a_crash` (non-ignored, always runs) proves arm64-windows + unsupported OS map to typed errors. CommandBar surfaces the ErrorDto. grep confirms NO `panic!`/`unwrap()`/`sys.exit`/`crash_box` on the merge data path (only in `#[cfg(test)]`). |

**Score: 4/4 verified.**

## Data-Integrity Core (Core Value invariants)

| Invariant | Status | Evidence |
|-----------|--------|----------|
| Merge runs on a COPY; SOURCE never mutated | ✓ PASS | `stage_and_merge`: `fs::copy(session.db_path → root/userData.db)`, source archive only READ (extraction + merge read `root/merge/userData.db`). `merge_source_immutable` asserts source SHA-256 unchanged after both dry-run AND commit. |
| DEST promoted ATOMICALLY (rename, not fs::copy); mid-merge/mid-promote failure leaves live DB pristine-or-fully-merged | ✓ PASS | `merge_commit_with_lib_path` promotes via `save::atomic_replace` (`fs::rename`), never `fs::copy` onto the live DB. Staging DB and `session.db_path` both live under `session.temp_dir` (confirmed: `db_path` is inside `temp_dir`) → same filesystem → rename is a single atomic kernel op. Merge failure returns BEFORE promote. `merge_commit_promote_atomic` both legs: (a) success → live DB passes `integrity_check`, fully merged, dirty; (b) aborting source → live DB byte-identical to pre-commit, still `integrity_check` ok, NOT dirty. |
| Overwrite count is CONTENT-signature based (not PK-set) | ✓ PASS | `snapshot_signatures` hashes the FULL row tuple keyed by single-i64 PK; `diff_signatures` classifies in-both-changed-signature as `overwritten`. `merge_overwrite_content_counted` proves an in-place UPDATE reports `overwritten >= 1` where a PK-set diff would report 0. |
| Media fold-back handles new + same-name-different-content blobs; empirically a guarded no-op | ✓ PASS | `fold_back_media`: new name → copy + push `ZipEntryMeta`; present name → compare content, replace if different. `merge_media_verification` empirically observed jwlCore wrote ONLY `userData.db` (extra staging-root files == []); branch-(a) no-op fired; pre-existing dest media retained. An assertion tripwire flags branch (b) if a future jwlCore build relocates media. |
| Typed MergeUnavailable/MergeFailed; Python crash_box/sys.exit NOT ported | ✓ PASS | error.rs variants + `to_dto` no-leak mapping. grep confirms no panic/unwrap/sys.exit/crash_box on merge path. |
| Semantic verification, never byte-diff | ✓ PASS | Parity oracle uses `normalized_table_rows` (sorted row→count over `SELECT *`) across `MERGE_PARITY_TABLES`; only the SOURCE archive is byte-hashed (immutability), never the merged DBs. |

## Adversarial trace — corruption windows

- **Dry-run read handle on live DB:** `Connection::open(&session.db_path)` is scoped inside the `before` block and dropped before `stage_and_merge`; dry-run performs NO promote. No promote-blocking handle held on Windows. Safe.
- **Commit promote precondition:** `stage_and_merge` only `fs::copy`s FROM `session.db_path` (handle closed immediately); `fold_back_media` touches only `session.temp_dir` files — no open handle on `session.db_path` at rename time (Windows replace requirement met). Safe.
- **Failure ordering:** FFI merge failure, media-fold failure, or CString/NUL error all return `Err` BEFORE `atomic_replace` — live DB never touched on any failure path. Verified by test leg (b).
- **Process-global jwlCore state:** merge commands serialize under the `SessionState` mutex; `getLastResult()` is read immediately after a non-zero return in the same scope, before the library handle drops (D5-06). Safe.
- **Non-UTF-8 Windows path:** documented known limit — `to_string_lossy` → U+FFFD → nonexistent dir → clean `MergeFailed`, never UB. Acceptable MVP posture.

## Test Execution (this environment — real DLL + real Python)

| Suite | Command | Result |
|-------|---------|--------|
| Full workspace | `cargo test --jobs 2` | exit 0, 0 failed |
| FFI wrapper | `cargo test --jobs 2 --test merge_ffi` | `merge_databases_ffi_merges_synthetic_pair` ok (real DLL) |
| Orchestration | `cargo test --jobs 2 --test merge_orchestration` | 5/5 RAN + passed against real DLL: source_immutable, dry_run_matches_commit, overwrite_content_counted, commit_promote_atomic, media_verification |
| Parity oracle | `cargo test --jobs 2 --test differential -- --ignored` | 4/4 incl. `rust_ffi_merge_matches_python_merge` (Rust FFI == Python, normalized; Python 3.13.3) |
| Frontend | `npx vitest run` | 43 passed (5 files); CommandBar merge + cancel-no-op + merge_unavailable ErrorDto covered |

`--jobs 2` used per instruction (default parallelism OOMs the linker — os error 1455, an env resource limit, not a code defect). Confirmed exit 0.

## Warnings (non-blocking)

1. **Playlist-table merge coverage DEFERRED.** Source `PlaylistItem`/`PlaylistItemMarker` rows carried into the destination are NOT asserted anywhere. jwlCore aborts on a minimal synthetic `PlaylistItem` (`"key not found: 0"`) — it needs a fuller playlist graph (thumbnail/IndependentMedia/markers/maps) than a minimal synthetic fixture reproduces. These tables ARE in the snapshot/parity sets (harmless empty-table no-op on current fixtures), and the abort was reused as the deterministic failure source for the atomic-promote pristine leg. Honestly documented in 05-01-SUMMARY, 05-02-SUMMARY, and `archive/merge.rs` module docs — never silently claimed. A future plan wanting playlist-merge parity must build a full valid playlist graph fixture. No ROADMAP Phase 5 criterion depends on this.

2. **ROADMAP bookkeeping lag.** The progress table still shows Phase 5 as "In Progress 2/3" and plan 05-03 unchecked `[ ]`, but `05-03-SUMMARY.md` exists (status: Complete), the frontend merge action + parity oracle are present, and all tests are green. Stale checkbox only — recommend flipping to Complete 3/3.

## Gaps Summary

No blockers. All 4 success criteria and all 6 data-integrity invariants are verified against the real native library and the real Python oracle. The single deferred item (playlist-table merge) is out of the phase's success-criteria scope and is documented transparently. The live working copy is provably never corrupted: source is read-only, merge runs on copies, and the commit promote is a same-filesystem atomic rename with an all-or-nothing failure guarantee proven by test.

---

_Verified: 2026-07-23_
_Verifier: Claude (gsd-verifier)_
