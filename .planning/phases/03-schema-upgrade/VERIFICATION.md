---
phase: 03-schema-upgrade
verified: 2026-07-20T21:50:00Z
status: passed
score: 3/3 success criteria verified; 2/2 requirements satisfied
overrides_applied: 0
---

# Phase 3: Schema Upgrade Verification Report

**Phase Goal:** Any archive a real user might hand the app (schema v12-16) opens correctly and is normalized to v16 in memory.
**Verified:** 2026-07-20 (commands actually executed this session, not taken from SUMMARY claims)
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Opening a v12/v13/v14/v15/v16 fixture archive succeeds and data displays correctly | VERIFIED | `test_gate_accepts_v12_through_v16` (schema_upgrade_tests.rs:27) passes; `test_pre_v16_fixtures_have_correct_version_and_shape` (fixtures.rs) passes; `test_upgrade_v14_to_v16` passes. Ran `cargo test` live — 17/17 schema_upgrade_tests green, 0 failed. |
| 2 | Opening a v11-or-earlier archive fails with a clear, actionable message | VERIFIED | `test_gate_rejects_v11` passes (mod.rs manifest+PRAGMA gate, `ArchiveError::SchemaTooOld`); `test_check_validity_accepts_v14_and_v16_rejects_out_of_range` (manifest.rs gate) passes; frontend copy for `schema_too_old` present in `app/src/lib/errors.ts:23` (verb-led, actionable, not generic). v17 too-new path also covered (`test_gate_rejects_v17`) and is a DISTINCT code from too-old, matching D3-09. |
| 3 | Any accepted archive is upgraded to v16 immediately on open, verified by round-trip test | VERIFIED | `open_and_validate` (archive/mod.rs:119-122) calls `upgrade::upgrade_to_v16` whenever `pragma_version < WORKING_SCHEMA_VERSION`, then always runs `validate_v16_contract`. `test_saved_upgraded_archive_reopen_round_trip` and `test_source_file_unchanged_after_open` pass. Differential oracle `python_app_opens_upgraded_v14_archive` — ACTUALLY RUN this session, PASSED (see Probe Execution below), proving the upgraded archive is accepted by the real Python `check_validity` oracle, not just the Rust reader. |

**Score:** 3/3 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `app/src-tauri/src/archive/upgrade.rs` | Transactional `upgrade_to_v16` + `validate_v16_contract` | VERIFIED | Exists, substantive (313 lines, real DDL port), wired into `archive/mod.rs::open_and_validate`. |
| `app/src-tauri/src/archive/mod.rs` | Widened 12-16 gate, upgrade-on-open wiring | VERIFIED | `MIN_SUPPORTED_SCHEMA_VERSION=12`, `MAX=16`, `WORKING=16` are the single source of truth; both manifest and PRAGMA checked independently before upgrade. |
| `app/src-tauri/src/archive/manifest.rs` | Independent gate sharing the same constants | VERIFIED | Imports `MAX_SUPPORTED_SCHEMA_VERSION, MIN_SUPPORTED_SCHEMA_VERSION, WORKING_SCHEMA_VERSION` from `archive::mod` (manifest.rs:20-22) — cannot drift, confirming reviewer finding 3 was actually fixed, not just claimed. |
| `app/src-tauri/tests/schema_upgrade_tests.rs` | Full threat-register test matrix | VERIFIED | 17 tests present and passing live: gate accept/reject, no-op at v16, reject direct call >v16, skip-existing-columns, Specialty/Edition preservation (value-level assertion, not just success), representative Location-type survival, foreign_keys assertion, typed-failure, rollback-to-original-version, contract-rejects-incomplete, in-range mismatch normalization (both directions), byte-identical source, save+reopen round trip. |
| `app/src-tauri/tests/differential.rs` | v14-upgrade Python oracle | VERIFIED | `python_app_opens_upgraded_v14_archive` exists, `#[ignore]`d (recorded manual gate), and was RE-RUN by this verifier — PASS. |
| `app/src/lib/errors.ts` | schema_too_old/too_new/upgrade_failed copy | VERIFIED | Three case arms present with actionable copy; `npm run build` and `npm test` both pass. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `archive/mod.rs::open_and_validate` | `archive/upgrade.rs::upgrade_to_v16` | direct call, conditional on `pragma_version < WORKING_SCHEMA_VERSION` | WIRED | Confirmed in source; not a dead/orphaned module. |
| `archive/mod.rs::open_and_validate` | `archive/upgrade.rs::validate_v16_contract` | direct call, unconditional after gate | WIRED | Always runs post-upgrade, even for already-v16 archives (no-op upgrade still validated). |
| `archive/manifest.rs::check_schema_gate` | `archive::{MIN,MAX,WORKING}_SUPPORTED_SCHEMA_VERSION` | `use` import | WIRED | Both gates read the same consts — single source of truth confirmed by direct source read, not just SUMMARY claim. |

## Adversarial Review-Finding Verification (03-REVIEWS.md, 9 findings)

All 9 accepted findings checked directly against code, not SUMMARY prose:

1. **Specialty/Edition preservation (HIGH)** — CONFIRMED FIXED. `run_upgrade_ddl` (upgrade.rs:154-170) builds `specialty_src`/`edition_src` conditionally on `column_exists`, never hardcoding `NULL, NULL`. Test `test_upgrade_preserves_existing_specialty_edition` asserts the exact surviving VALUES (`"known-specialty-1"`/`"known-edition-1"`), not merely upgrade success — this is the correct depth of test per the review's own complaint that "the D3-04 test as written checks success/version, not row preservation." PASS live.
2. **Post-upgrade schema contract validator (HIGH)** — CONFIRMED. `validate_v16_contract` (upgrade.rs:254-313) checks PRAGMA version, 6 required tables, 12 required Location columns, and `IX_Location_Media`. Wired unconditionally into `open_and_validate`. `test_post_upgrade_contract_rejects_incomplete_db` PASS live — drops the index and confirms rejection.
3. **manifest.rs gate + tests updated (MEDIUM)** — CONFIRMED. `manifest_tests.rs::test_check_validity_accepts_v14_and_v16_rejects_out_of_range` replaces the old v14-reject assertion; reject boundary moved to v11/v17.
4. **In-range manifest/PRAGMA mismatch normalized, not rejected (MEDIUM)** — CONFIRMED. Both directions tested: `test_in_range_manifest_pragma_mismatch_normalizes_manifest_low` and `..._normalizes_db_low`, both PASS live.
5. **Representative Location-type coverage (MEDIUM)** — CONFIRMED. `insert_representative_locations` seeds 6 rows (publication/doc, media/track, Type 1/2/3, NULL-heavy) at LocationId 20-25; `test_upgrade_preserves_representative_location_types` asserts row-count unchanged AND all 6 IDs survive. PASS live.
6. **foreign_keys explicitly OFF, never enabled (LOW/MEDIUM)** — CONFIRMED. `upgrade_to_v16` runs `PRAGMA foreign_keys = OFF` on the connection before opening the transaction (a documented pragma-inside-transaction no-op, correctly handled outside it). `test_foreign_keys_off_around_rebuild` asserts `fk == 0` after upgrade. PASS live.
7. **No-op exactly at v16, reject direct call >16 (LOW)** — CONFIRMED. `upgrade_to_v16` (upgrade.rs:205-217): `==16` returns `Ok(())`, `>16` returns `Err(SchemaTooNew)`. `test_upgrade_noop_on_v16` and `test_upgrade_rejects_direct_call_above_v16` both PASS live.
8. **Strengthened round trip (LOW)** — CONFIRMED. `test_saved_upgraded_archive_reopen_round_trip` PASS live.
9. **Honest v12/v13 language (LOW)** — CONFIRMED. VALIDATION.md's "Known Coverage Limit" section and 03-03-SUMMARY's "Known Coverage Limit" both explicitly scope v12/v13 as synthetic-only/same-code-path, not independently real-data-verified. This is a recorded, honest scoping decision — not a gap.

**The `except: pass` defect (D3-02) is NOT reproduced.** `run_upgrade_ddl` propagates every SQLite error through `map_sqlite_err` into `ArchiveError::SchemaUpgradeFailed`; nothing in `upgrade.rs` catches-and-discards. `test_upgrade_rollback_leaves_original_version` poisons the DB mid-rebuild (renames `Location` away), asserts `PRAGMA user_version` is unchanged (still 14) AND the poisoned table's normalized row content is byte-identical before/after the failed attempt — this is exactly the depth the objective demanded ("find the rollback test, confirm it asserts original version"). PASS live.

### Requirements Coverage

| Requirement | Description | Status | Evidence |
|-------------|-------------|--------|----------|
| SCHEMA-01 | App accepts schema versions 12-16 and rejects <=11 with a clear message | SATISFIED | Dual gates (mod.rs + manifest.rs) share one const range; `test_gate_accepts_v12_through_v16` / `test_gate_rejects_v11` / `test_gate_rejects_v17` all PASS live; error copy in errors.ts is actionable. |
| SCHEMA-02 | App upgrades any accepted archive to working version 16 on open | SATISFIED | `open_and_validate` upgrades unconditionally when below v16, validates the v16 contract unconditionally, and the real Python oracle (`python_app_opens_upgraded_v14_archive`) confirms the result is accepted by the actual JW-Library-compatible app, not just this app's own reader. |

### Behavioral Spot-Checks / Full Suite Execution (actually run this session)

| Command | Result | Status |
|---------|--------|--------|
| `cargo fmt --check` | clean, no diff | PASS |
| `cargo clippy --all-targets -- -D warnings` | clean, 0 warnings | PASS |
| `cargo test` (full workspace) | all suites green; 17/17 `schema_upgrade_tests`; 4/4 `error_tests`; 6/6 `manifest_tests`; 6/6 `fixtures`; 4/4 `save_tests`; 0 failed overall | PASS |
| `cargo test --test differential -- --ignored --nocapture` | `python_app_opens_upgraded_v14_archive ... ok`, `python_app_opens_tauri_saved_archive ... ok` — 2 passed, 0 failed | PASS |
| `npm run build` (tsc + vite) | clean build, 221KB bundle | PASS |
| `npm test` (vitest) | 23/23 passed across 4 files | PASS |

### Anti-Patterns Found

None found in the phase's modified files. No `TODO`/`FIXME`/`XXX`/`HACK`/`PLACEHOLDER` markers, no swallowed errors, no hardcoded empty stub returns in `upgrade.rs`, `archive/mod.rs`, `archive/manifest.rs`, `schema_upgrade_tests.rs`, or `differential.rs`. Error handling is exhaustively typed (`ArchiveError::SchemaUpgradeFailed`/`SchemaTooOld`/`SchemaTooNew`), matching the Core Value guardrail (never lose or corrupt a user's archive) and explicitly avoiding the ported Python `except: pass` defect.

### Manual Gates (recorded, not yet run — correctly deferred, not a phase gap)

Per VALIDATION.md's "Manual-Only Verifications" table, two items remain owner-run-only and were correctly NOT run in CI or by this verifier (real archives are never committed, per GDPR Art. 9 / D-06/D-07):

1. **A REAL v14 archive (from the owner's own library) opens/upgrades/saves and is accepted by the Python app** — `real_archive_round_trip_env_gated` in `differential.rs` exists and is env-gated (`JWLM_REAL_ARCHIVE`); confirmed NOT run this session (env var unset) — took the correct skip path per 03-03-SUMMARY. **Manual-gate-pending.**
2. **Upgraded archive opens in real JW Library app (the vendor iOS/Android/desktop app)** — requires the vendor app itself; no automated path exists or should exist. **Manual-gate-pending.**

These are honestly recorded gaps in *real-world* verification breadth, not gaps in the phase's own code/test correctness — the synthetic-fixture and differential-oracle coverage that CAN be automated has all been run and passes.

### Known, Recorded Scoping Limit (not a failure)

**v12/v13 upgrade correctness is verified only against synthetic (reverse-mutated v16) fixtures**, per VALIDATION.md's explicit "Known Coverage Limit" section, carried through 03-01-SUMMARY and 03-03-SUMMARY without ever being overclaimed as real-data-verified. v14 and v16 ARE real-oracle-verified via the differential Python `check_validity` handoff (confirmed live this session). This is honest, pre-declared scoping — exactly matching the objective's instruction to treat it as a recorded decision, not a defect.

### Human Verification Required

None. All 3 ROADMAP success criteria and both requirements (SCHEMA-01/02) are verifiable and were verified by automated tests actually executed this session. The two manual-gate items above are pre-declared, environment-dependent (real user archive, vendor app), and explicitly out of scope for automated/CI verification per the phase's own VALIDATION.md — they do not block the phase's own goal achievement, which concerns the app's own upgrade correctness.

## Ship Verdict: SHIP-WITH-MANUAL-GATES

**Justification:** All 3 ROADMAP success criteria are VERIFIED against real, currently-passing code and tests (not SUMMARY narrative) — including live re-execution of `cargo fmt`, `cargo clippy -D warnings`, the full `cargo test` suite (17/17 schema_upgrade_tests + full workspace, 0 failures), the differential Python oracle (`--ignored`, 2/2 passed against real PySide6/jwlCore), and the frontend build+test suite (23/23). All 9 codex review findings were independently re-derived from source and confirmed fixed — not merely claimed fixed — including the two HIGH-severity data-loss/corruption vectors (Specialty/Edition preservation and the post-upgrade contract validator), with the rollback test confirmed to assert both the original PRAGMA version AND unchanged row content. Requirements SCHEMA-01 and SCHEMA-02 are satisfied. The only open items are the two explicitly pre-declared, environment-dependent manual gates (real owner archive, vendor JW Library app) and the honestly-scoped v12/v13 synthetic-only limitation — neither is a phase defect; both were flagged by the phase's own planning artifacts before this verification ran, and this verifier found no evidence contradicting that honest scoping.

---

_Verified: 2026-07-20T21:50:00Z_
_Verifier: Claude (gsd-verifier)_
