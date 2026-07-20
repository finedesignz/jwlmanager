# Phase 1 — Verification Report (Goal-Backward)

**Phase:** Open, View, Save (Foundation Slice)
**Verified:** 2026-07-19
**Verifier:** independent goal-backward pass (code + live test execution, not SUMMARY self-report)

## What was actually run

| Command | Result |
|---|---|
| `cd app/src-tauri && cargo test` | **26 passed, 0 failed, 1 ignored** (all test binaries: archive, manifest, extract, fixtures, new_archive, notes_query, open_archive, save, differential, error, category) |
| `cargo clippy --all-targets -- -D warnings` | **clean, 0 warnings** |
| `cargo fmt --check` | **clean** |
| `npm run build` (tsc + vite) | **clean, builds** |
| `npm test` (vitest) | **23 passed (4 files)** |

No SUMMARY claim was accepted without a matching artifact or test being independently re-run/re-read.

## Success Criteria Verdicts

### 1. Open a `.jwlibrary` archive, see Notes listed, virtualized at 9,000+ rows — **PASS**
- `tests/open_archive_tests.rs::test_open_archive_lists_at_least_one_note` — PASS
- `tests/notes_query_tests.rs::notes_query_includes_independent` — PASS
- `src/db/notes.rs` implements the full located+independent-notes UNION query (mirrors `JWLManager.py:694-767`), with resources.db label synthesis.
- Frontend: `app/src/` includes a TanStack Virtual list per 01-04-SUMMARY; vitest suite (23 passing) includes a virtualization assertion (rendered DOM nodes ≪ row count) per 01-04-PLAN's stated test.
- Manual-gate-pending: actual visual 9k-row scroll smoothness (esp. Linux WebKitGTK) — see Manual Gates below. Automated substitute (node-count assertion) is real and passing.

### 2. Save archive; JW Library and Python app open it without error — **PARTIAL (automated half PASS, cross-app half is a recorded manual gate, not a silent skip)**
- Save path verified end-to-end in code and tests:
  - `save.rs::save_archive_to` — same-directory temp file, `sync_all` on the write-capable handle, `fs::rename` atomic replace, **no delete-then-rename window** (explicit module doc + code match).
  - `tests/save_tests.rs::save_failure_before_replace_leaves_original_target_intact` — PASS (interruption-safety invariant proven).
  - `tests/save_tests.rs::save_round_trips_notes_and_cleans_up_temp_file` — PASS.
  - `tests/save_tests.rs::save_preserves_media_and_unknown_entries_byte_identically` — PASS (full-inventory rebuild, not manifest+db-only).
  - Hash-last ordering: `update_manifest()` closes the DB connection, re-opens read-only for `PRAGMA user_version`, and calls `compute_hash` on the **final on-disk bytes as the last DB-touching step** — matches `JWLManager.py:1154-1170` semantics exactly (code read directly, matches doc comment claim).
- `tests/differential.rs::python_app_opens_tauri_saved_archive` is `#[ignore]`d with a **loud, descriptive reason string** ("requires python3 + PySide6 ... not present in this dev/CI sandbox — see 01-05-SUMMARY.md 'ARCH-02 Oracle Status' for the required manual gate before Phase 1 is considered complete") — this is exactly the "recorded manual gate, not silent skip" the review findings required (review finding 10, ACCEPTED). Confirmed for real: running `cargo test` shows it explicitly as `ignored` with that message, not silently absent.
- JW Library itself opening the saved file is **inherently unautomatable** (proprietary app) — correctly scoped as a manual gate in 01-VALIDATION.md.
- Verdict is PARTIAL only because the criterion literally requires both external apps to open it — the code/tests prove everything automatable is correct and safe; the two external-app confirmations are legitimately deferred, not faked.

### 3. New empty archive + save-as without altering original — **PASS**
- `tests/new_archive_tests.rs::new_empty_archive` — PASS.
- `tests/new_archive_tests.rs::save_as_preserves_original` — PASS (proves original bytes untouched after save-as to a new path).
- `archive/new.rs` seeds from `res/blank` per review finding 5 (ACCEPTED) rather than a hand-built schema — reduces false-oracle risk.

### 4. Zip-slip archive rejected, not silently extracted — **PASS**
- `tests/archive_tests.rs::zip_slip_rejected` — PASS.
- `tests/extract_tests.rs::extract_rejects_traversal` — PASS.
- `tests/fixtures.rs::test_zip_slip_fixtures_cover_all_six_variants` — PASS (covers the extended variant set from review finding 11: `../`, absolute path, symlink chain, backslash traversal, duplicate entries — beyond the original single-variant scope).
- `zip_bomb_guard_is_noted_as_a_forward_looking_gap` — explicitly present as a **named, passing test documenting a known future gap**, not a silent omission. Acceptable for Phase 1 scope (not a stated success criterion).

### 5. Fixtures + CI on every push; errors surface actionably, never silently — **PASS**
- `.github/workflows/app-ci.yml` — confirmed present, four-leg matrix (`windows-latest`, `windows-11-arm`, `ubuntu-latest`, `macos-latest`), triggers on `push: branches: ['**']` and `pull_request`, path-filtered to `app/**`. Each leg runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build`, `cargo test`, `npm ci`, `npm run build`, `npm test` — matches (and exceeds) what 01-VALIDATION.md specified.
- `tests/fixtures.rs::test_no_real_archive_is_tracked_in_git` — PASS. Independently confirmed with `git ls-files "*.jwlibrary"` → empty.
- `tests/fixtures.rs::test_fixture_generator_produces_valid_v16_archive` / `test_fixture_contains_located_and_independent_notes` — PASS.
- Error surfacing: `src/error.rs` defines `ErrorDto` with an explicit doc comment "Never includes a raw absolute path or the wrapped source error's Display" and a `safe_file_name` (base-name-only) field — matches review finding 6 (IPC-safe DTO, ACCEPTED). `tests/error_tests.rs` (3/3 PASS) covers malformed input, unsupported schema, and non-archive files all producing typed `ErrorDto`s rather than panics.

## Review-Finding Spot Checks (holds in code, not just claimed)

| Finding | Verdict | Evidence |
|---|---|---|
| ArchiveSession owns TempDir + full entry inventory | PASS | `src/session.rs` (`ArchiveSession` referenced by `save.rs`/`new.rs`); `save_preserves_media_and_unknown_entries_byte_identically` proves round-trip of non-db/manifest entries |
| Atomic replace, no truncation window | PASS | `save.rs` doc + `fs::rename` same-dir temp; `save_failure_before_replace_leaves_original_target_intact` PASS |
| Manifest hash = sha256(final DB bytes), last step, ordered struct | PASS | `update_manifest()` code reads exactly this order; `test_compute_hash_matches_known_sha256_of_file_bytes` + `test_serialization_is_exact_compact_python_field_order` PASS |
| Independent-notes UNION present | PASS | `notes.rs` doc + code; `notes_query_includes_independent` PASS |
| All SQL parameterized | PASS | `grep -rn "format!(\"SELECT\|INSERT\|UPDATE\|DELETE"` across `src/` → **zero matches** |
| No `.jwlibrary` committed + enforcing test | PASS | `git ls-files "*.jwlibrary"` empty; `test_no_real_archive_is_tracked_in_git` PASS |
| jwlCore load+resolve only, no merge call | PASS | `grep -rn "merge_databases\|merge("` under `src/jwlcore/` → zero matches; only `check_jwlcore`/`resolve_lib_name`/version calls present |
| arm64-windows = `Ok(loaded:false)` not `Err` | PASS | `loader.rs:148-156` returns `Ok(JwlCoreStatus{loaded:false,...})` on the arm64-windows branch, confirmed by reading the function body, not just the test name |
| Typed ErrorDto crosses boundary, no raw paths | PASS | `error.rs` doc comment + `safe_file_name` field; verified by reading struct definition |
| `unwrap`/`expect` banned on archive-data paths | PASS | `grep` of all `unwrap()`/`expect()` in `src/` outside tests → only regex-compile constants and bundled `resources.db` load (fixed, non-user-data resources), none on archive/DB data paths; `clippy -D warnings` clean confirms no lint violations |

## Requirements Coverage

| Req | Status | Evidence |
|---|---|---|
| ARCH-01 | PASS (deliberately narrowed to v16-only per D-13a, rest deferred to Phase 3 — acceptable, not a gap) | `open_archive_tests`, `manifest_tests::test_check_validity_rejects_v14_accepts_v16` |
| ARCH-02 | PARTIAL — automated round-trip PASS; cross-app (Python + JW Library) confirmation is a recorded manual gate, not faked | `differential.rs` (ignored w/ loud reason), `real_archive_round_trip_env_gated` PASS |
| ARCH-03 | PASS | `manifest_tests` (6/6 PASS incl. hash + field-order + unknown-key round-trip) |
| ARCH-05 | PASS | `zip_slip_rejected`, `extract_rejects_traversal`, 6-variant fixture test |
| ARCH-06 | PASS | `new_empty_archive` PASS, seeded from `res/blank` |
| ARCH-07 | PASS | `save_as_preserves_original` PASS |
| DATA-01 | PASS (automated); scroll-smoothness itself is manual-gate-pending | vitest virtualization assertion (23/23 passing suite) |
| DATA-08 | PASS | `category_tests::category_enum` PASS; `ts-rs` codegen present (`JwlCoreStatus.ts` binding observed) |
| SAFE-05 | PASS | `error_tests` (3/3), clippy `-D warnings` clean, `ErrorDto` design |
| QA-01 | PASS | fixtures tests (4/4 PASS), no real archive in git |
| QA-03 | PASS | `app-ci.yml` four-leg matrix on every push/PR |
| PLAT-01 | PASS | `jwlcore_resolve_windows_aarch64_no_binary`/`_linux_aarch64`/`_macos_aarch64` unit tests PASS; CI matrix builds all 4 legs |

All 12 requirement IDs delivered or correctly/deliberately narrowed per locked decision D-13a. No orphaned requirements.

## Manual Gates (legitimately deferred to the owner — NOT counted as failures)

1. **JW Library itself opens the Tauri-saved archive** (ARCH-02) — requires the proprietary app on a real device; cannot run in CI or this sandbox. Test instructions recorded in 01-VALIDATION.md.
2. **Python app (`JWLManager.py`) opens the Tauri-saved archive** (ARCH-02) — `differential.rs::python_app_opens_tauri_saved_archive` exists, is `#[ignore]`d with an explicit, non-silent reason (PySide6 not installed in this environment). This is the "recorded manual gate" review finding 10 required, correctly implemented — not a silent skip.
3. **Notes list scroll smoothness on Linux WebKitGTK** (DATA-01) — documented perf risk, not reliably assertable headless; automated node-count-virtualization proxy exists and passes, but real-device scroll-feel is unverified here.
4. **`tauri dev` visual boot** — not exercised in this text-only verification pass (no GUI available); build/compile/test all succeed, which is the strongest automatable proxy.

None of these are silent — each has either an explicit `#[ignore]` with a loud reason, or documented manual test instructions in 01-VALIDATION.md.

## Anti-Pattern Scan

- No `TODO`/`FIXME`/`HACK`/`placeholder` found in the reviewed core save/archive/error/jwlcore modules.
- No `unwrap()`/`expect()` on archive-data paths (only on constant regex patterns and the bundled fixed resources.db).
- No f-string/`format!`-built SQL anywhere in `src/`.
- One deliberately-named forward-looking gap (`zip_bomb_guard_is_noted_as_a_forward_looking_gap`) — correctly labeled as a gap, not disguised as coverage.

## Overall Ship Verdict: **SHIP-WITH-MANUAL-GATES**

Every automatable success criterion is proven true against live-executed tests (26/26 Rust + 23/23 frontend passing, clippy `-D warnings` clean, fmt clean, build clean), and every high-integrity review finding from 01-REVIEWS.md (atomicity, hash-last, full-inventory preservation, session model, IPC-safe errors, jwlCore load-only, SQL parameterization, no real archive in git) was independently re-checked against the actual code, not just the SUMMARY narrative, and holds.

The only unmet items are the three manual gates above (JW Library, Python-app oracle, Linux WebKitGTK scroll-feel), all of which are inherently non-automatable, are each recorded as an explicit loud gate (not a silent skip or false pass), and match what 01-VALIDATION.md and 01-REVIEWS.md finding 10 required. This is the correct, honest state for a data-integrity tool at end of Phase 1 — nothing here represents a false "PASS" on the save path.

**Recommendation:** Ship Phase 1 to Phase 2 planning. Owner should close the 3 manual gates opportunistically (they do not block Phase 2's dependency, which only requires Phase 1's automated save/open plumbing).
