---
phase: 03-schema-upgrade
plan: 01
subsystem: test-fixtures, error-surface
tags: [fixtures, sqlite-schema, error-dto, frontend-copy]
dependency-graph:
  requires: []
  provides:
    - "generate_fixture_pre_v16_shape(version) — v11-v15 fixtures, reverse-mutated from res/blank"
    - "generate_fixture_v17_shape() — out-of-range reject fixture"
    - "ArchiveError::SchemaTooOld / SchemaTooNew / SchemaUpgradeFailed + to_dto arms"
    - "errors.ts copy for schema_too_old / schema_too_new / schema_upgrade_failed"
  affects:
    - "03-02 (widened gate + upgrade transaction) consumes both the fixtures and the error variants"
tech-stack:
  added: []
  patterns:
    - "Reverse-mutation of a v16 seed (drop Specialty/Edition + IX_Location_Media, PRAGMA user_version) rather than a second fixture generator"
    - "PRAGMA foreign_keys = OFF explicitly set before a Location table rebuild in test fixtures"
key-files:
  created: []
  modified:
    - app/src-tauri/tests/common/mod.rs
    - app/src-tauri/tests/fixtures.rs
    - app/src-tauri/src/error.rs
    - app/src/lib/errors.ts
decisions:
  - "v12/v13 fixtures apply only the documented, verified v16<->v14 delta (Specialty/Edition + IX_Location_Media) — they are reverse-mutated v16 shapes for exercising the upgrade CODE PATH, not independently-verified v12/v13 archives. No further schema difference below v14 is known or claimed."
  - "SchemaTooOld/SchemaTooNew frontend copy is generic (does not embed the dynamic version number) — ErrorDto's shape (code/operation/safe_file_name/message_key) was left unchanged rather than adding a version field, matching the existing unsupported_schema precedent which also doesn't embed the found version."
metrics:
  duration: "~50 min"
  completed: "2026-07-20"
---

# Phase 3 Plan 1: Versioned Fixtures + Schema Error Surface Summary

Extended the existing res/blank-seeded fixture generator to emit v11-v15 (reverse-mutated pre-v16 shape) and v17 (out-of-range) `.jwlibrary` fixtures carrying representative Location Type coverage, and added the three typed schema error variants (too-old/too-new/upgrade-failed) with matching frontend copy — the two shared foundations 03-02's widened gate and upgrade transaction build on.

## What Was Built

**Task 1 — Fixture generator extension** (`app/src-tauri/tests/common/mod.rs`):
- `insert_representative_locations(db_path)`: adds 6 Location rows (LocationId 20-25) covering publication/document (Type 0 + DocumentId), media/track (Type 0 + Track), Type 1, Type 2, Type 3, and a NULL-heavy Type 2 row — each authored to legally satisfy the `Location_new` UNIQUE + three CHECK constraints ported from `JWLManager.py:1026-1062`. Combined with the existing scripture row (`LocationId = 1` from `insert_synthetic_notes`), this gives full finding-5 coverage.
- `synthetic_manifest_json_for(version)`: parameterized manifest builder; `synthetic_manifest_json()` now delegates to it with `version = 16`.
- `reverse_mutate_to_pre_v16_shape(db_path, version)`: drops `IX_Location_Media`, rebuilds `Location` without `Specialty`/`Edition` (matching `JWLManager.py:1016-1070`'s reverse), sets `PRAGMA user_version = version`. Explicitly sets `PRAGMA foreign_keys = OFF` first — the Location rebuild tripped FK enforcement without it (see Deviations).
- `build_fixture_archive(work_dir, manifest_json)`: factored zip-assembly step shared by all fixture generators (loose-media + unknown-entry inclusion unchanged).
- `generate_fixture_pre_v16_shape(version: i64)` (11-15) + by-name wrappers `generate_v11_fixture()`..`generate_v15_fixture()`.
- `generate_fixture_v17_shape()` for the too-new reject path (keeps full v16 Location shape — a real newer schema would only add to v16, never regress it).
- `generate_v16_fixture()` refactored to use `build_fixture_archive`; public behavior unchanged (verified: existing tests pass unmodified).

**Task 2 — Fixture tests** (`app/src-tauri/tests/fixtures.rs`):
- `test_pre_v16_fixtures_have_correct_version_and_shape`: for v11-v15, asserts `PRAGMA user_version`, manifest `schemaVersion` cross-check, absence of `Specialty`/`Edition` columns, absence of `IX_Location_Media`, plus representative Location Type coverage.
- `test_v17_fixture_reports_out_of_range_version`: asserts v17 fixture reports 17 in both DB and manifest, and still carries representative coverage.
- Existing `test_no_real_archive_is_tracked_in_git` GDPR guard untouched and still green.

**Task 3 — Error variants** (`app/src-tauri/src/error.rs`, `app/src/lib/errors.ts`):
- `ArchiveError::SchemaTooOld { version: i64 }`, `SchemaTooNew { version: i64 }`, `SchemaUpgradeFailed { reason: String }` added with `#[error(...)]` messages.
- `to_dto` match arms emit `schema_too_old` / `schema_too_new` / `schema_upgrade_failed` codes with matching `message_key`s. `SchemaUpgradeFailed.reason` is never read in the DTO construction — it stays internal (T-03-01 mitigation).
- Existing `UnsupportedSchema` variant and its `error.archive.unsupported_schema_phase3` key left untouched (03-02 retires it when it rewrites the gate).
- `errors.ts` `describeError` gained three matching `case` arms with actionable, verb-led copy naming the situation and next step.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `PRAGMA foreign_keys = OFF` needed before the Location rebuild**
- **Found during:** Task 2 (first fixture test run)
- **Issue:** `reverse_mutate_to_pre_v16_shape`'s `DROP TABLE Location; ALTER TABLE Location_old RENAME TO Location;` sequence failed with `SqliteFailure(ConstraintViolation, extended_code: 787)` — a foreign-key constraint violation — even though 03-RESEARCH.md verified `foreign_keys` defaults OFF in this codebase's SQLite connections generally.
- **Fix:** Added an explicit `PRAGMA foreign_keys = OFF;` at the top of the reverse-mutation's `execute_batch` so the rebuild is not sensitive to any per-connection or per-build default difference (mirrors the belt-and-suspenders pattern the plan itself asked for — "foreign_keys defaults OFF... do not change that" is honored; this only makes the OFF state explicit and local to the mutation).
- **Files modified:** `app/src-tauri/tests/common/mod.rs`
- **Commit:** part of `9a46f0b6` (test commit, since the fixture tests are what surfaced it)

Or: nothing else — the rest of the plan executed as written.

## Self-Check: PASSED

- `app/src-tauri/tests/common/mod.rs` — FOUND
- `app/src-tauri/tests/fixtures.rs` — FOUND
- `app/src-tauri/src/error.rs` — FOUND
- `app/src/lib/errors.ts` — FOUND
- Commit `48746da6` — FOUND
- Commit `9a46f0b6` — FOUND
- Commit `333348c8` — FOUND

## Verification Evidence

- `cargo fmt --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --test fixtures` — 6/6 passed
- `cargo test --test error_tests` — 3/3 passed
- `cargo build` — clean
- `npm run build` (tsc + vite) — clean
- `npm test` (vitest) — 23/23 passed across 4 files
- `cargo test --test fixtures --test error_tests` — all 9 passed together (no cross-test interference)

## Known Stubs

None. No hardcoded empty UI values or placeholder copy introduced.

## Threat Flags

None beyond what's already tracked in this plan's own `<threat_model>` (T-03-01, T-03-02, T-03-12, T-03-SC) — all mitigated as designed. No new network endpoints, auth paths, or trust-boundary schema changes introduced.
