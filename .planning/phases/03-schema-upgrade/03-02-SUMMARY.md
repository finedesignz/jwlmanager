---
phase: 03-schema-upgrade
plan: 02
subsystem: archive-schema-upgrade
tags: [rusqlite, transaction, schema-migration, data-integrity]
dependency-graph:
  requires:
    - "03-01: versioned fixtures (v11-v15,v17) + SchemaTooOld/SchemaTooNew/SchemaUpgradeFailed error variants"
  provides:
    - "upgrade::upgrade_to_v16(&mut Connection) — transactional DDL port of JWLManager.py:1016-1070, rollback-safe"
    - "upgrade::validate_v16_contract(&Connection) — post-upgrade v16 shape validator"
    - "archive::{MIN,MAX,WORKING}_SUPPORTED_SCHEMA_VERSION — single source of truth for the 12-16 range"
    - "open_and_validate now accepts 12-16, upgrades in-place, normalizes manifest/PRAGMA mismatch"
  affects:
    - "03-03 (Python differential test) opens real v14 owner archives against this upgrade path"
tech-stack:
  added: []
  patterns:
    - "rusqlite::Transaction wraps the entire DDL rebuild; any Err causes rollback-on-drop, never a partial commit"
    - "PRAGMA foreign_keys=OFF set on the connection BEFORE opening the transaction (pragma changes are a no-op inside an active transaction)"
    - "Conditional INSERT..SELECT source columns (real column vs literal NULL) instead of the Python original's unconditional NULL,NULL"
key-files:
  created:
    - app/src-tauri/src/archive/upgrade.rs
    - app/src-tauri/tests/schema_upgrade_tests.rs
  modified:
    - app/src-tauri/src/archive/mod.rs
    - app/src-tauri/src/archive/manifest.rs
    - app/src-tauri/src/error.rs
    - app/src/lib/errors.ts
    - app/src-tauri/tests/manifest_tests.rs
    - app/src-tauri/tests/archive_validity_tests.rs
    - app/src-tauri/tests/error_tests.rs
decisions:
  - "ArchiveError::SchemaUpgradeFailed is reused for the post-upgrade contract validator rather than adding a separate SchemaContractViolation variant — one less variant to keep in sync, and the DTO/copy is already generic ('upgrading its internal database format failed... original file unchanged')."
  - "foreign_keys does NOT default OFF in this build's bundled rusqlite/SQLite (contra 03-RESEARCH.md's assumption, reconfirmed from 03-01's empirical finding) — upgrade_to_v16 now explicitly disables it on the connection before opening the transaction, and never re-enables it."
  - "UnsupportedSchema variant removed entirely (not deprecated) once both gates were confirmed to have zero remaining producers — its unsupported_schema_phase3 message_key and errors.ts case are retired alongside it."
metrics:
  duration: "~65 min"
  completed: "2026-07-20"
---

# Phase 3 Plan 2: Transactional Schema Upgrade + Contract Validator + Dual-Gate Widening Summary

Ported `JWLManager.py:1016-1070`'s Location-table rebuild DDL into a transactional `upgrade_to_v16` (never the original's silent `except: pass`), added a post-upgrade v16 contract validator, and widened both independent schema gates (`archive/mod.rs` and `archive/manifest.rs`) from v16-only to a shared 12-16 range — closing the lockout that had 19 of the owner's 32 real archives (all v14) rejected.

## What Was Built

**Task 1 — RED test matrix** (`app/src-tauri/tests/schema_upgrade_tests.rs`): 17 tests encoding every behavior in the plan's threat register — gate accept (v12-16) / reject (v11 too-old, v17 too-new), transactional upgrade mechanics (no-op at v16, reject direct calls above v16, skip pre-existing columns), Specialty/Edition data preservation with exact-value assertions, representative Location-type survival (6 rows spanning every CHECK-constraint branch), `foreign_keys` assertion, typed-failure + rollback-to-original-version, post-upgrade contract rejecting an incomplete DB, in-range manifest/PRAGMA mismatch normalization (both directions), source-file byte-identity, and a full save+reopen round trip. Confirmed RED (`unresolved import jwlmanager_lib::archive::upgrade`).

**Task 2 — GREEN implementation** (`app/src-tauri/src/archive/upgrade.rs`, new):
- `upgrade_to_v16(&mut Connection) -> Result<(), ArchiveError>`: reads `PRAGMA user_version`; `==16` is a no-op `Ok`; `>16` is `Err(SchemaTooNew)` (finding 7, belt-and-suspenders against the range gate). Otherwise disables `foreign_keys` on the connection, opens a `rusqlite::Transaction`, guards each `ALTER TABLE ADD COLUMN` with a `column_exists` check (D3-04), builds the `INSERT..SELECT` source-column list CONDITIONALLY per column (finding 1 — real column when it already exists, literal `NULL` only when freshly added), runs the verbatim `CREATE Location_new` DDL (UNIQUE + 3 CHECK constraints) / `INSERT..SELECT` / `DROP` / `RENAME` / 3 `CREATE INDEX` (incl. `IX_Location_Media`), sets `user_version=16`, and commits. Any error anywhere in that sequence maps to `ArchiveError::SchemaUpgradeFailed { reason }` and the transaction rolls back on drop — never `Ok`, never a bare propagated `rusqlite::Error`.
- `validate_v16_contract(&Connection) -> Result<(), ArchiveError>` (finding 2): checks `user_version==16`, all six tables Phase 1 reads/writes are present, `Location` has all 12 v16 columns including `Specialty`/`Edition`, and `IX_Location_Media` exists — any gap is `Err(SchemaUpgradeFailed)`, never silent acceptance.
- Widened `archive/mod.rs`: replaced `SUPPORTED_SCHEMA_VERSION` with `MIN_SUPPORTED_SCHEMA_VERSION=12` / `MAX_SUPPORTED_SCHEMA_VERSION=16` / `WORKING_SCHEMA_VERSION=16` (public consts, single source of truth). `open_and_validate` now: rejects out-of-range manifest/PRAGMA versions independently, upgrades in-place when the PRAGMA is below 16, always runs `validate_v16_contract`, and derives the session's `ManifestMeta.schema_version` from the FINAL post-upgrade PRAGMA rather than the manifest's original claim (finding 4 — in-range mismatches normalize, never reject).
- Widened `archive/manifest.rs`'s independent `check_schema_gate` in lockstep, importing the same three constants from `archive::mod` (finding 3) so the two gates cannot drift.

**Task 3 — updated existing suites + retired placeholder**: `manifest_tests.rs` and `archive_validity_tests.rs` flipped their v14-reject assertions to v14-accept-and-upgraded, moving the reject boundary to v11/v17. `error_tests.rs` replaced the `unsupported_schema` DTO test with `SchemaTooOld`/`SchemaTooNew` distinct-code coverage plus an explicit assertion that `SchemaUpgradeFailed.reason` never crosses the DTO boundary. Confirmed (via `grep`) `ArchiveError::UnsupportedSchema` had zero remaining producers after the gate widen, so removed the variant entirely along with its `error.archive.unsupported_schema_phase3` message_key and the `errors.ts` `unsupported_schema` case.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `foreign_keys` does not default OFF in this build — explicit disable required**
- **Found during:** Task 2, first full test run (9 of 17 tests failing with `FOREIGN KEY constraint failed` on the `DROP TABLE Location` swap)
- **Issue:** The plan's finding 6 (and 03-RESEARCH.md) assumed `foreign_keys` defaults OFF for this codebase's rusqlite connections, so `upgrade_to_v16` was written to only *assert* the default, never set it. Empirically that assumption is wrong for this bundled SQLite build — `test_foreign_keys_off_around_rebuild` originally asserting the raw default returned `1`, and every DDL-rebuild test failed with an FK violation. This exactly reproduces the deviation 03-01 already logged for its fixture generator's `reverse_mutate_to_pre_v16_shape`.
- **Fix:** `upgrade_to_v16` now explicitly runs `PRAGMA foreign_keys = OFF;` on the connection BEFORE opening the `rusqlite::Transaction` (a pragma change is a documented no-op inside an active transaction, so it cannot live inside the transaction itself). This never *enables* `foreign_keys` — finding 6's actual constraint — it only makes the OFF state explicit and local to the rebuild instead of relying on an unverified default. The test was updated to assert `foreign_keys==0` AFTER calling `upgrade_to_v16`, documenting that the disable is this module's responsibility, not an ambient default.
- **Files modified:** `app/src-tauri/src/archive/upgrade.rs`, `app/src-tauri/tests/schema_upgrade_tests.rs`
- **Commit:** `b5b0dde4`

**2. [Rule 2 - Missing functionality] Removed the now-dead `UnsupportedSchema` variant proactively**
- **Found during:** Task 3
- **Issue:** The plan made removal conditional on `grep` confirming zero remaining producers. After widening both gates to emit `SchemaTooOld`/`SchemaTooNew` instead, `grep -r UnsupportedSchema src/` showed only the variant's own declaration and `to_dto` arm — no producer anywhere.
- **Fix:** Removed the variant, its `to_dto` arm, the `error.archive.unsupported_schema_phase3` message_key, and the corresponding `errors.ts` case, per the plan's explicit instruction for this exact condition.
- **Files modified:** `app/src-tauri/src/error.rs`, `app/src/lib/errors.ts`
- **Commit:** `7b4fcd95`

## Self-Check: PASSED

- `app/src-tauri/src/archive/upgrade.rs` — FOUND
- `app/src-tauri/tests/schema_upgrade_tests.rs` — FOUND
- `app/src-tauri/src/archive/mod.rs` — FOUND (modified)
- `app/src-tauri/src/archive/manifest.rs` — FOUND (modified)
- Commit `5de8619f` (RED) — FOUND
- Commit `b5b0dde4` (GREEN) — FOUND
- Commit `7b4fcd95` (suite updates) — FOUND

## Verification Evidence

- `cargo fmt --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test` (full workspace) — 24 unit + 2 archive_tests + 1 archive_validity_tests + 1 category_tests + 1 differential (1 ignored, doc-noted) + 4 error_tests + 1 extract_tests + 6 fixtures + 6 manifest_tests + 2 new_archive_tests + 1 notes_query_tests + 1 open_archive_tests + 4 save_tests + **17 schema_upgrade_tests** — all green, 0 failed
- `cargo build` — clean
- `npm run build` (tsc + vite) — clean
- `npm test` (vitest) — 23/23 passed across 4 files

## Known Stubs

None. No hardcoded empty UI values or placeholder copy introduced.

## Threat Flags

None beyond what's already tracked in this plan's own `<threat_model>` (T-03-03 through T-03-11, T-03-SC) — all mitigated as designed and proven by `schema_upgrade_tests.rs`. No new network endpoints, auth paths, or trust-boundary schema changes beyond the documented widened 12-16 gate.
