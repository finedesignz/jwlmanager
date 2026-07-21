---
phase: 03-schema-upgrade
plan: 03
subsystem: archive-schema-upgrade
tags: [differential-testing, python-interop, acceptance-gate]
dependency-graph:
  requires:
    - "03-01: generate_fixture_pre_v16_shape(version) versioned fixtures"
    - "03-02: upgrade_to_v16 wired into open_and_validate (v12-16 accepted, upgraded in-place)"
  provides:
    - "differential.rs::python_app_opens_upgraded_v14_archive — v14-upgrade oracle, VERIFIED PASSING"
    - "differential.rs::run_python_check_validity — shared python-oracle invocation helper"
    - "differential.rs::real_archive_round_trip_env_gated — now includes the D3-11 Python acceptance assertion"
  affects: []
tech-stack:
  added: []
  patterns:
    - "shared python-oracle helper (run_python_check_validity) factored out of the original v16-only test, reused by the v14-upgrade oracle and the real-archive gate"
    - "python3-availability probe (python3 --version) before the real-archive check_validity assertion, so the default (non-ignored) test skips visibly rather than panicking on machines without Python installed"
key-files:
  modified:
    - app/src-tauri/tests/differential.rs
decisions:
  - "Extended the existing differential.rs in place rather than a new test file — the plan's interfaces section explicitly names this file's existing functions to extend, and the shared helper keeps both oracle tests byte-identical in their Python invocation."
  - "The v14-upgrade oracle was actually run in this session (PySide6 + DLLs staged per the environment note) and is marked VERIFIED PASSING with today's date/environment, matching the existing v16 oracle's documentation convention — not left as 'pending human run' since it was, in fact, run and passed."
metrics:
  duration: "~25 min"
  completed: "2026-07-20"
---

# Phase 3 Plan 3: v14-Upgrade Differential Oracle + Env-Gated Real-v14 Acceptance Summary

Extended the ARCH-02 differential oracle (`tests/differential.rs`) to prove that a synthetic v14 archive, upgraded to v16 in-place by `open_and_validate` (03-02) and saved through the Tauri save path, is accepted by the Python app's own `check_validity` — the same cross-ecosystem acceptance test the v16 oracle already provided, now covering the upgrade path itself. Also wired the same Python acceptance assertion into the existing env-gated real-archive round trip, so a run with `JWLM_REAL_ARCHIVE` set against one of the owner's real v14 backups exercises the full D3-11 acceptance gate, not just the Rust-side round trip.

## What Was Built

**Task 1 — v14-upgrade differential oracle** (`app/src-tauri/tests/differential.rs`):
- Added `python_app_opens_upgraded_v14_archive`, mirroring the existing `python_app_opens_tauri_saved_archive` but seeded from `common::generate_fixture_pre_v16_shape(14)`. Asserts `session.manifest.schema_version == 16` immediately after `open_and_validate` (proving the upgrade actually ran, not just that the fixture claims v16) before saving and handing off to Python's `check_validity`.
- Factored the shared Python-oracle invocation (repo-root cwd, PATH-prepend for the `sqlite3_64.dll` static-import resolution, `ORACLE_RESULT:PASS/FAIL` sentinel parsing) out of the original test into `run_python_check_validity(archive_path: &Path) -> (bool, String, String)`, reused by both oracle tests. No duplication of the CRC/DLL-staging logic.
- `#[ignore]`d with the same "recorded manual gate, CI is Rust-only" reasoning as the existing test.

**Task 2 — env-gated real-v14 acceptance path** (same file):
- `real_archive_round_trip_env_gated` (already existed, ran the Rust-only open→save-as→reopen round trip against `JWLM_REAL_ARCHIVE`) now additionally runs the Python `check_validity` acceptance assertion against the round-tripped output, using the same `run_python_check_validity` helper.
- Guarded by a `python3 --version` probe: if python3 isn't on PATH, the assertion is skipped with a visible `eprintln!` (not a silent pass, not a panic) — this test is NOT `#[ignore]`d (it already self-gates on `JWLM_REAL_ARCHIVE`), so it must degrade gracefully on machines that have the real archive path set but not the full Python stack.
- Doc comment records this as the D3-11 acceptance gate with its exact run command (`JWLM_REAL_ARCHIVE=<path> cargo test --test differential`) and the standalone `cargo run --example roundtrip` alternative.

## Oracle Results (run this session)

Both real Python oracle runs were executed in this session (Windows x64, Python 3.13.3, PySide6 6.9.3, jwlCore v0.32.1, `jwlCore-amd64.dll` + `sqlite3_64.dll` staged at repo root):

```
cargo test --test differential -- --ignored --nocapture
running 2 tests
test python_app_opens_upgraded_v14_archive ... ok
test python_app_opens_tauri_saved_archive ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**`python_app_opens_upgraded_v14_archive` — VERIFIED PASSING.** A synthetic v14 fixture, upgraded to v16 by `open_and_validate` and saved through the Tauri save path, was accepted by `JWLManager.Window.check_validity`. This closes the phase's core proof obligation: the schema upgrade doesn't just satisfy our own Rust reader, it produces something the actual Python/JW Library ecosystem accepts.

The env-gated real-v14 test (`real_archive_round_trip_env_gated`) was NOT run against a real archive in this session — `JWLM_REAL_ARCHIVE` was left unset, so it took the correct skip path (verified: Rust round trip ran and passed; the new Python-acceptance branch is dormant until the env var is set). This is the correct, honest state — it is a manual gate the owner runs locally against actual iPad backups, never in CI, never with synthetic data substituted.

## Known Coverage Limit (per VALIDATION.md, not overclaimed)

v14 and v16 are now real-oracle-verified (both via the differential Python `check_validity` handoff). v11/v12/v13/v15/v17 exercise the same upgrade/gate CODE PATH via `generate_fixture_pre_v16_shape`'s reverse-mutation, but are **synthetic-only** — no independent verification that v12/v13 real archives have identical schema deltas to v14/v15. This limitation is unchanged from 03-01/03-02 and is not claimed as resolved by this plan.

## Deviations from Plan

None. Plan executed as written — the plan anticipated the v14-upgrade oracle might need to be left "pending the owner's local run," but since PySide6 + the DLLs were actually available in this session, it was run for real and marked VERIFIED PASSING per the same documentation convention as the existing v16 oracle, rather than artificially deferred.

## Self-Check: PASSED

- `app/src-tauri/tests/differential.rs` — FOUND (modified)
- Commit `e0b6e0ed` (Task 1: v14-upgrade oracle added) — FOUND
- Commit `6dbf4ca2` (Task 2: D3-11 gate on real-archive test + VERIFIED PASSING doc update) — FOUND

## Verification Evidence

- `cargo test --test differential -- --ignored --nocapture` — 2 passed (v16 oracle, v14-upgrade oracle), both accepted by Python `check_validity`
- `cargo test --test differential` (default) — 1 passed (real-archive test, correctly skipped Python branch with JWLM_REAL_ARCHIVE unset), 2 ignored (both oracle tests, as designed)
- `cargo test` (full workspace) — all suites green, 0 failed, including 17 `schema_upgrade_tests`
- `cargo build` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo fmt --check` — clean
- `npm run build` (tsc + vite) — clean

## Known Stubs

None.

## Threat Flags

None. T-03-08 (repudiation via silently-passing oracle) and T-03-09 (real archive leaking into repo/CI) are both mitigated exactly as designed: the v14-upgrade oracle is `#[ignore]`d with an explicit recorded-manual-gate reason and was only marked VERIFIED PASSING after actually running it; the real-v14 path skips visibly (never silently) both when `JWLM_REAL_ARCHIVE` is unset and when python3 is unavailable.
