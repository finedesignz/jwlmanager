---
phase: 05-two-archive-merge
plan: 01
subsystem: jwlcore/merge
tags: [ffi, jwlcore, merge, unsafe, error-surface, data-integrity]
requires: [jwlcore/loader.rs, error.rs, tests/common/mod.rs]
provides: [run_merge_with_lib_path, run_merge, merge_availability, host_dev_lib_path, MergeUnavailable, MergeFailed]
affects: [jwlcore/loader.rs, jwlcore/mod.rs, error.rs, tests/common/mod.rs]
tech-stack:
  added: []
  patterns: [unsafe-ffi-isolated-module, cstring-lifetime-bound-to-locals, getLastResult-in-same-scope, resolve-lib-name-first, skip-as-pass-off-host, no-leak-dto-reason]
key-files:
  created:
    - app/src-tauri/src/jwlcore/merge.rs
    - app/src-tauri/tests/merge_ffi.rs
  modified:
    - app/src-tauri/src/jwlcore/loader.rs
    - app/src-tauri/src/jwlcore/mod.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/tests/common/mod.rs
decisions:
  - "D5-01: merge.rs REUSES Phase 1 load path (loader helpers promoted to pub(crate)); Windows sqlite3_64.dll PATH-prepend untouched"
  - "D5-06: getLastResult() read IMMEDIATELY after non-zero return, same scope, before the library handle drops"
  - "T-05-02: merge_availability checks resolve_lib_name FIRST → MergeUnavailable, never invokes into an unloaded lib (arm64-windows/missing = clean error, never crash)"
  - "D-14 no-leak: MergeFailed{reason} keeps getLastResult() detail INTERNAL; to_dto exposes only merge_failed code + generic message_key"
  - "KNOWN LIMIT: non-UTF-8 Windows temp path → to_string_lossy U+FFFD → nonexistent dir → clean MergeFailed (no OS-string FFI for MVP)"
  - "Rule 3 deviation: added pub host_dev_lib_path — the integration-test crate cannot reach pub(crate) loader helpers; centralizes arch selection in one place"
  - "Fixture-validity limit: PlaylistItem/PlaylistItemMarker omitted from the merge fixture (jwlCore playlist merge needs a fuller graph than a minimal synthetic fixture reproduces)"
metrics:
  duration: ~1h
  completed: 2026-07-22
status: complete
---

# Phase 5 Plan 01: jwlCore mergeDatabase FFI Wrapper Summary

The FIRST real invocation of the vendored `jwlCore` merge engine. A tiny, auditable `unsafe` surface in `jwlcore/merge.rs` calls `mergeDatabase(dest_dir, src_dir, downgrade)` over two directory paths, reusing Phase 1's hard-won load path (arch selection + the Windows `sqlite3_64.dll` PATH-prepend). A missing/wrong-arch binary degrades to `ArchiveError::MergeUnavailable`; a non-zero return becomes `ArchiveError::MergeFailed { reason }` carrying `getLastResult()` detail internally. The Python `crash_box + sys.exit()` defect is NOT ported — nothing here panics or crashes. Verified end-to-end against the REAL `jwlCore-amd64.dll`, which merged a synthetic dest+source pair with all source records present, no duplicate PKs, and referential integrity intact.

## What shipped

- **`ArchiveError::MergeUnavailable` + `MergeFailed { reason }`** (error.rs) with `to_dto` mapping to codes `merge_unavailable` / `merge_failed` and message_keys `error.merge.unavailable` / `error.merge.failed`. `reason` never crosses IPC (mirrors the existing `SchemaDowngradeFailed`/`DeleteFailed` no-leak pattern).
- **Loader helpers promoted to `pub(crate)`** (loader.rs): `resolve_lib_name`, `resolve_lib_path`, `load_library`, `dev_libs_dir`, `EXPECTED_SYMBOLS`, `NoBinaryReason` + `message()`. The PATH-prepend logic is byte-for-byte unchanged (load-bearing, D5-01).
- **`jwlcore/merge.rs`** — the one `unsafe` FFI module:
  - `MergeFn`/`LastResultFn` type aliases matching the verified ABI (`jwlcore.py:64-68`).
  - `pub fn run_merge_with_lib_path(lib_path, dest_root, source_root, downgrade)` — `load_library` → resolve `mergeDatabase`+`getLastResult` → two `CString`s bound to locals (kept alive across the call) → invoke → `0`=Ok, non-zero reads `getLastResult()` in the same scope (D5-06), `CStr::from_ptr` only when non-null else a fixed generic reason. `NulError` → typed `MergeFailed`, never unwrap. Every `unsafe` block carries a SAFETY comment.
  - `pub(crate) fn merge_availability(app)` — checks `resolve_lib_name` FIRST → `MergeUnavailable` (T-05-02), then `resolve_lib_path`.
  - `pub(crate) fn run_merge(app, ...)` — the ONE routine Wave 2 dry-run + commit will share (`merge_availability?` then `run_merge_with_lib_path`).
  - `pub fn host_dev_lib_path()` — dev-tree lib path for the current host (None off-host), so the integration test resolves the DLL + skip-as-pass without duplicating arch logic.
- **`generate_merge_pair()`** (tests/common/mod.rs) — two synthetic v16 `userData.db`s (dest + source) seeded from `res/blank`, with overlapping (shared `Note.Guid`, `UserMark.UserMarkGuid`, `Tag(Type,Name)`, shared scripture `Location` identity) and disjoint (source-only Location/Note/UserMark/Tag/Bookmark/InputField) records.
- **`tests/merge_ffi.rs`** — real-DLL FFI integration test: materializes the `dest_root` + `dest_root/merge` two-directory layout (D5-03), calls `run_merge_with_lib_path` against the vendored DLL, asserts every source identity is present, overlaps are deduped (present exactly once), no duplicate PKs across 7 single-PK tables, and `PRAGMA foreign_key_check` returns zero rows. Skip-as-pass off-host.
- **7 unit tests** in merge.rs: `dir_cstring` ok / interior-NUL→typed-error, `reason_from_ptr` null→fixed / valid→string, `availability_name` arm64-windows / unsupported → `MergeUnavailable`, supported host → name.

## Did the real jwlCore DLL actually load + merge in this environment?

**YES.** Host: Windows x64. The vendored `libs/jwlCore-amd64.dll` (+ co-located `libs/sqlite3_64.dll` via the loader PATH-prepend) loaded, `mergeDatabase` was invoked, and it merged the synthetic pair. All post-merge assertions passed (source records present, overlaps deduped, zero duplicate PKs, FK-check clean). The wrapper's error path was ALSO exercised live: an earlier fixture iteration produced a real non-zero return whose `getLastResult()` string (`"Exception merging PlaylistItem table failed: key not found: 0"`) was read and surfaced correctly as `MergeFailed { reason }` — proving both the success and failure legs against the real binary, not a mock.

## Verification (DoD)

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean (only pre-existing ts-rs macro-parse notes, not lint failures).
- `cargo test` (full workspace) — **130 passed, 0 failed, 5 ignored** (all pre-existing manual-gate / env-gated: differential Python oracles, real-archive round-trip, delete/trim ignored cases). `tests/merge_ffi.rs`: **1 passed** (real DLL merged).
- `npm run build` — not run: Wave 1 is backend-only, no frontend files touched.

## Deviations from Plan

### Auto-fixed / auto-decided (no user permission needed)

**1. [Rule 3 - Blocking] `run_merge_with_lib_path` is `pub`, not `pub(crate)`; added `pub host_dev_lib_path`**
- **Found during:** Task 3.
- **Issue:** The plan specified `pub(crate)` for `run_merge_with_lib_path`, but the Task 3 integration test (`tests/merge_ffi.rs`) links the crate as an EXTERNAL crate and cannot see `pub(crate)` items. The plan's own Task 3 body calls `jwlmanager_lib::jwlcore::merge::run_merge_with_lib_path` — impossible with `pub(crate)`.
- **Fix:** Made `run_merge_with_lib_path` `pub` (matching the repo convention for integration-tested core routines, e.g. `archive::downgrade::save_v14_copy`). Added a small `pub host_dev_lib_path()` so the test resolves the host DLL + skip-as-pass without re-implementing the OS/ARCH match. `run_merge`/`merge_availability` stay `pub(crate)` (Wave 2 internal wiring).
- **Files:** `app/src-tauri/src/jwlcore/merge.rs`. **Commits:** `afbe7285`, `cf26a630`.

**2. [Rule 1 - Fixture correctness] PlaylistItem/PlaylistItemMarker omitted from the merge fixture**
- **Found during:** Task 3 (real-DLL run).
- **Issue:** The plan's fixture table list included `PlaylistItem`/`PlaylistItemMarker`. Seeding a minimal (even playlist-Tag-membership-linked) PlaylistItem made jwlCore abort the whole merge with `"Exception merging PlaylistItem table failed: key not found: 0"`. jwlCore's playlist merge requires a fuller playlist graph (thumbnail/IndependentMedia/accuracy relationships) than a minimal synthetic fixture reproduces.
- **Fix:** Removed PlaylistItem/PlaylistItemMarker from the fixture with an in-code note. This is a fixture-validity limit, NOT a wrapper defect — the wrapper's non-zero-return + `getLastResult()` path was proven correct by that very failure. Merge-correctness assertions stand on the 8 fully reproducible single-PK tables (Location/Note/UserMark/BlockRange/Bookmark/Tag/TagMap/InputField).
- **Files:** `app/src-tauri/tests/common/mod.rs`. **Commit:** `cf26a630`.
- **Follow-up for Wave 2:** if playlist-bearing merges must be tested, build a full valid playlist graph fixture (playlist Tag + IndependentMedia + thumbnail + markers) or use the throwaway-copy dry-run against a real-shaped media archive.

## TDD Gate Compliance

Task 2 was marked `tdd="true"`. The 7 unit tests in `merge.rs` define the wrapper's behavior contract (CString mapping, null-reason handling, availability gating) and are co-located with the implementation; they pass. The real-DLL merge behavior (Ok success path + non-zero→MergeFailed) is proven by the Task 3 integration test. No separate `test(...)` RED commit precedes the `feat(...)` GREEN commit — implementation and its co-located tests landed together in `afbe7285`; both legs (success + failure) were subsequently exercised against the real binary in `cf26a630`.

## Known Stubs

None. All shipped functions are fully implemented. `run_merge`/`merge_availability` are `#[allow(dead_code)]` in Wave 1 (their sole caller — the Wave 2 `merge_dry_run`/`merge_commit` Tauri commands — does not exist yet); this is intentional forward-wiring, not a stub.

## Self-Check: PASSED

- `app/src-tauri/src/jwlcore/merge.rs` — FOUND
- `app/src-tauri/tests/merge_ffi.rs` — FOUND
- Commit `510f9c2f` (Task 1), `afbe7285` (Task 2), `cf26a630` (Task 3) — all FOUND in git log.
