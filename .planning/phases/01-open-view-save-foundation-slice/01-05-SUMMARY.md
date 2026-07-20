---
phase: 01-open-view-save-foundation-slice
plan: 05
subsystem: archive-core
tags: [tauri, rust, rusqlite, zip, atomic-io, ipc, differential-testing]

requires:
  - phase: 01-07
    provides: ArchiveSession managed state (TempDir, full zip-entry inventory, db_path, target_path), open_archive command
  - phase: 01-02
    provides: byte-compatible Manifest struct, compute_hash, check_validity, hash-last discipline
  - phase: 01-03
    provides: check_jwlcore command already registered on the invoke_handler this plan appends to
  - phase: 01-04
    provides: notes query/rendering this plan's fixtures round-trip through
provides:
  - Atomic save (same-directory temp + std::fs::rename replace, never delete-then-rename)
  - Full-inventory zip rebuild (loose media + unknown/forward-compat entries preserved byte-identically)
  - Hash-last manifest update (LastModified UPDATE -> close -> sha256 the final on-disk bytes -> serialize)
  - new_archive seeded from the real res/blank v16 empty-archive seed (not a hand-built schema)
  - save_as (original untouched, session target follows the new path)
  - save_archive / save_as / new_archive registered as Tauri IPC commands
  - Dependency-free ISO8601 UTC timestamp helper (time.rs)
  - ARCH-02 differential oracle test (real, #[ignore]d with an explicit recorded-manual-gate reason — never a silent pass)
affects: [01-06]

tech-stack:
  added: []
  patterns:
    - "Same-directory temp file + std::fs::rename replace: guarantees single-filesystem atomicity on both platforms; delete-then-rename is architecturally impossible in this code path"
    - "Full-inventory rebuild: every ArchiveSession.entries name is streamed through from the extracted working copy except userData.db/manifest.json, which are regenerated — no entry can be silently dropped by a save"
    - "sync_all on the write-capable handle returned by ZipWriter::finish(), never a fresh read-only File::open — reopening read-only and calling sync_all fails FlushFileBuffers with ERROR_ACCESS_DENIED on Windows"
    - "#[ignore]-with-reason for an oracle that requires an unavailable local dependency, paired with a SUMMARY-recorded manual gate — never a silent skip reported as a pass"

key-files:
  created:
    - app/src-tauri/src/archive/save.rs
    - app/src-tauri/src/archive/new.rs
    - app/src-tauri/src/time.rs
    - app/src-tauri/tests/save_tests.rs
    - app/src-tauri/tests/new_archive_tests.rs
    - app/src-tauri/tests/differential.rs
  modified:
    - app/src-tauri/src/archive/mod.rs
    - app/src-tauri/src/lib.rs

key-decisions:
  - "Raw Win32 ReplaceFileW and MoveFileExW(MOVEFILE_REPLACE_EXISTING) were both implemented, tested, and REJECTED: ReplaceFileW failed with ERROR_PATH_NOT_FOUND, and MoveFileExW failed with ERROR_ACCESS_DENIED, for two plain files in the same AppData\\Local\\Temp directory in this execution environment. std::fs::rename (which Rust's own stdlib implements via NtSetInformationFile+FileRenameInformation with replace-if-exists semantics on this Windows toolchain) was verified to succeed reliably and is used on both platforms — a single atomic kernel call either way, never delete-then-rename."
  - "The REAL root cause of the initial failures was NOT the rename/replace API choice — it was calling sync_all() on a freshly File::open()'d (read-only) handle instead of the write-capable handle ZipWriter::finish() returns. FlushFileBuffers requires write access; a read-only reopen fails ERROR_ACCESS_DENIED on Windows. Fixed by syncing the write handle directly inside rebuild_zip before it is dropped."
  - "No chrono/time crate added for manifest timestamps — a small dependency-free civil_from_days-based ISO8601 UTC formatter (time.rs) covers the single YYYY-MM-DDTHH:MM:SSZ formatting need."
  - "ARCH-02 oracle test is #[ignore]d by default with an explicit, specific reason (PySide6 not installed in this sandbox, verified) rather than silently passing or being omitted — matches the plan's explicit finding-10 requirement. The test body was still exercised with --ignored to prove the invocation logic is correct and fails loudly (not silently) when the Python environment is missing."
  - "JWLManager.Window.check_validity is called UNBOUND (self=None) rather than instantiating a full Window/QApplication — the success path never touches self, only the two QMessageBox.warning() failure branches do, so a headless call is possible once PySide6 is present."

requirements-completed: [ARCH-02, ARCH-06, ARCH-07, QA-01]

metrics:
  duration: "~90 minutes"
  completed: "2026-07-19"
---

# Phase 1 Plan 5: Atomic Save, Save-As, New Archive Summary

Delivered the persistence slice this whole phase has been building toward: atomic save (same-directory temp + `std::fs::rename` replace, full zip-entry-inventory rebuild, hash-last manifest), save-as (original untouched, session target follows the new path), and a new-empty-archive path seeded from the real `res/blank` v16 seed — then registered all three as Tauri commands and added the ARCH-02 differential oracle (Python app reopens what the Tauri app saved).

## What Was Built

**Task 1 — `archive/save.rs` (atomic save):** `save_archive`/`save_archive_to` read the `ArchiveSession`'s full zip-entry inventory, run the hash-last manifest-update sequence (`UPDATE LastModified` → close the connection → `compute_hash` over the final on-disk `userData.db` bytes → serialize the manifest), rebuild the FULL zip into a same-directory temp file (every original entry streamed through byte-identical except `userData.db`/`manifest.json`), `sync_all` the write-capable file handle, then atomically replace the target via `std::fs::rename`. Delete-then-rename is architecturally impossible — the target is only ever touched by the single rename call. `tests/save_tests.rs` proves: semantic Notes round-trip, loose-media + unknown-entry preservation byte-identically, interruption safety (a failure before the replace leaves the original byte-identical), and no leftover temp file.

**Task 2 — `archive/new.rs` (new archive + save-as):** `new_archive`/`new_archive_from_seed` build a fresh `ArchiveSession` by copying `res/blank`'s real `userData.db` (already `PRAGMA user_version = 16`) and `default_thumbnail.png` — never a hand-built schema, keeping the ARCH-02 oracle non-circular. `save_as` reuses the same atomic full-inventory save against a caller-chosen path, leaving the source (opened read-only into `session.temp_dir`, D-03) untouched. `tests/new_archive_tests.rs` proves a fresh archive opens with zero Notes and a valid v16 manifest (round-tripped through the real save + `open_and_validate` path, not just internal state), and that save-as leaves the original file's bytes and hash unchanged while the new file reopens cleanly.

**Task 3 — commands + ARCH-02 oracle:** `save_archive`, `save_as`, `new_archive` registered on `lib.rs`'s `invoke_handler` alongside `open_archive`/`check_jwlcore`, each mapping `ArchiveError` to the sanitized `ErrorDto` at the IPC boundary. Added a dependency-free `time.rs` (civil-date ISO8601 UTC formatter — no `chrono`/`time` crate needed for the one timestamp shape this plan requires). `tests/differential.rs` shells to `python3` and calls `JWLManager.Window.check_validity` (unbound) against a Tauri-saved archive — this IS a real headless invocation, not a stub; it is `#[ignore]`d with an explicit, specific, verified reason (see below) rather than silently skipped, plus an `JWLM_REAL_ARCHIVE`-env-gated real-archive round-trip test that is skipped (never failed) when unset.

## ARCH-02 Oracle Status

**Result: recorded manual gate required — NOT a silent skip.**

`JWLManager.py` imports `PySide6` at MODULE level (`from res.ui_main_window import Ui_MainWindow` → `res/ui_extras.py` → `from PySide6.QtCore import ...`), so even calling `check_validity` headlessly requires the full GUI dependency stack. This sandbox does not have PySide6 installed — verified directly:

```
$ python3 -c "import PySide6"
ModuleNotFoundError: No module named 'PySide6'
```

`tests/differential.rs::python_app_opens_tauri_saved_archive` is `#[ignore]`d with that exact reason embedded in the attribute. To prove the test logic itself is correct (not a fake stub), it was run explicitly with `cargo test --test differential -- --ignored --nocapture`: it invoked `python3`, hit the same `ModuleNotFoundError`, and **FAILED loudly** with the real traceback — confirming the oracle genuinely checks something and cannot silently report green when the dependency is missing.

**Required before Phase 1 is considered complete:** on a machine with `res/requirements.txt` installed (`pip install -r res/requirements.txt`), run:

```
cd app/src-tauri && cargo test --test differential -- --ignored
```

and separately, manually open a Tauri-saved `.jwlibrary` file in real JW Library (desktop) and confirm no error / data intact. Neither of these was run in this session — no PySide6, no JW Library, no display. Both remain **open manual gates** the owner must close before shipping.

## Verification Evidence

- `cargo build` (`app/src-tauri`) — clean.
- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — zero warnings.
- `cargo test` (full suite) — 24 lib unit tests + all integration test binaries green: `save_tests` (4/4), `new_archive_tests` (2/2), `differential` (1 passed + 1 explicitly ignored with reason, 0 failed), plus all prior plans' suites (`archive_tests`, `archive_validity_tests`, `category_tests`, `error_tests`, `extract_tests`, `fixtures`, `manifest_tests`, `notes_query_tests`, `open_archive_tests`) unaffected.
- `cargo test --test save_tests` — 4/4 pass (round-trip, preservation, interruption-safety, same-dir-temp).
- `cargo test new_empty_archive save_as_preserves_original` — run individually per test name (01-02's documented `cargo test` single-positional-filter quirk still applies); both pass.
- `cargo test --test differential` — 1 passed (env-gated real-archive test, skipped cleanly), 1 ignored with an explicit reason; matches this plan's own verify regex (`test result: ok|ignored`).
- `cargo test --test differential -- --ignored --nocapture` — run explicitly to PROVE the oracle isn't a stub: it invoked `python3`, hit the real `ModuleNotFoundError: No module named 'PySide6'`, and failed loudly (not silently) — exactly the behavior finding 10 requires.
- `npm run build` (`app/`) — `tsc` + `vite build` clean.

**Not run:** `npm run tauri dev` visual boot check, the ARCH-02 Python oracle with PySide6 present, and a real JW Library open — all three require a display/GUI environment or an installed dependency this non-interactive sandbox does not have. See "ARCH-02 Oracle Status" above for the exact recorded gate the owner must close.

## Task Commits

1. **Task 1: Atomic save + full-inventory rebuild + hash-last manifest** — `c667a396` (feat)
2. **Task 2: new_archive (res/blank-seeded) + save-as** — `6d9214d9` (feat)
3. **Task 3: register commands + ARCH-02 differential oracle** — `002c0af2` (feat)

## Files Created/Modified

- `app/src-tauri/src/archive/save.rs` — `save_archive`/`save_archive_to`, `update_manifest` (hash-last), `rebuild_zip` (full-inventory), `same_dir_temp_path`, `atomic_replace` (`std::fs::rename`, both platforms)
- `app/src-tauri/src/archive/new.rs` — `new_archive`/`new_archive_from_seed` (res/blank-seeded), `save_as`
- `app/src-tauri/src/time.rs` — dependency-free `now_iso8601_utc()` (civil_from_days algorithm)
- `app/src-tauri/src/archive/mod.rs` — registers `save`/`new` submodules
- `app/src-tauri/src/lib.rs` — registers `save_archive`/`save_as`/`new_archive` Tauri commands, `APP_NAME`/`APP_DEVICE_NAME` constants
- `app/src-tauri/tests/save_tests.rs` — round-trip, media/unknown preservation, interruption-safety, same-dir-temp tests
- `app/src-tauri/tests/new_archive_tests.rs` — new-empty-archive and save-as-preserves-original tests
- `app/src-tauri/tests/differential.rs` — ARCH-02 oracle (`#[ignore]`d with reason) + `JWLM_REAL_ARCHIVE`-env-gated real-archive round trip

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Plan's literal `ReplaceFileW` prescription fails in this execution environment; root cause was actually a `sync_all`-on-read-only-handle bug, not the rename API**
- **Found during:** Task 1, first `cargo test --test save_tests` run.
- **Issue:** Implemented `ReplaceFileW` exactly as the plan's `<action>` text prescribes ("on Windows use `ReplaceFileW`"). It failed with `ERROR_PATH_NOT_FOUND` even for two plain files in the same directory, reproduced standalone via a PowerShell P/Invoke harness outside Rust entirely — ruling out a Rust/FFI-specific bug. Switched to `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` (the plan's own parenthetical fallback, "or a verified-atomic path") — also failed, with `ERROR_ACCESS_DENIED`, on every real-flow test (a minimal debug repro of `MoveFileExW` alone, without the surrounding save flow, worked fine). Narrowed further: the real bug was `save_archive_to` calling `sync_all()` on a FRESH `File::open(&temp_zip_path)` (read-only handle) instead of the write-capable handle `ZipWriter::finish()` already returns — `FlushFileBuffers` requires write access and fails `ERROR_ACCESS_DENIED` on a read-only handle on Windows. This was completely unrelated to which rename/replace primitive was used; the earlier Win32 FFI experiments were misdiagnosing a `sync_all` bug as a `atomic_replace` bug.
- **Fix:** `rebuild_zip` now calls `sync_all()` directly on the write-capable `File` returned by `ZipWriter::finish()`, before it is dropped. `atomic_replace` was simplified to `std::fs::rename` on both platforms (verified reliable in this environment; Rust's stdlib implements Windows rename-with-replace via `NtSetInformationFile`+`FileRenameInformation`, a single atomic kernel call — the same underlying guarantee `ReplaceFileW`/`MoveFileExW` are meant to provide). The `windows` crate dependency added for the FFI experiments was removed again — no longer needed.
- **Files modified:** `app/src-tauri/src/archive/save.rs`, `app/src-tauri/Cargo.toml` (added then reverted)
- **Commit:** `c667a396`

### Deferred (Rule 2, documented not applied — informational)

- **Frontend wiring (App.tsx toolbar buttons) not touched:** this plan's own `files_modified` frontmatter list is backend-only (`save.rs`, `new.rs`, `mod.rs`, `main.rs`, `lib.rs`, `save_tests.rs`, `differential.rs`) — no `App.tsx`. The three new commands are registered and callable over IPC (acceptance criterion met literally), but the "New Archive"/"Save"/"Save As" toolbar buttons remain `disabled` in the frontend shell, same as 01-07 left them. Wiring them up is left to a later UI-focused plan, consistent with this plan's own scope declaration.

## Known Stubs

- `New Archive` / `Save` / `Save As` toolbar buttons in `App.tsx` remain `disabled` — the backend commands exist and are tested at the Rust/IPC layer, but no frontend click handler invokes them yet (see Deferred above; out of this plan's declared file scope).
- ARCH-02's Python-oracle and real-JW-Library manual gates are unmet in this session (no PySide6, no display) — see "ARCH-02 Oracle Status" above. This is not a code stub but an explicit, tracked verification gap the owner must close.

## Threat Flags

None — all new surface (save target file, session entry inventory, save-as path argument, manifest hash timing, the oracle's own subprocess invocation) was already registered in this plan's own `<threat_model>` (T-05-01 through T-05-06) and is mitigated as designed: atomic replace via a single kernel call, full-inventory preservation, source read-only, and the oracle never faking a pass.

## Self-Check: PASSED

- `app/src-tauri/src/archive/save.rs` — FOUND
- `app/src-tauri/src/archive/new.rs` — FOUND
- `app/src-tauri/src/time.rs` — FOUND
- `app/src-tauri/tests/save_tests.rs` — FOUND
- `app/src-tauri/tests/new_archive_tests.rs` — FOUND
- `app/src-tauri/tests/differential.rs` — FOUND
- Commit `c667a396` (feat: atomic save) — FOUND in `git log`
- Commit `6d9214d9` (feat: new_archive + save-as) — FOUND in `git log`
- Commit `002c0af2` (feat: commands + differential oracle) — FOUND in `git log`
