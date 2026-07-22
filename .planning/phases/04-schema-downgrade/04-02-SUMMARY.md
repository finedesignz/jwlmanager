---
phase: 04-schema-downgrade
plan: 02
subsystem: archive/schema-downgrade
tags: [schema-downgrade, dry-run, atomic-save, session-isolation]
requires:
  - "04-01: downgrade_to_v14 + ArchiveError::SchemaDowngradeFailed"
  - "02-02: DryRunReport + snapshot/diff helpers (delete.rs)"
  - "02-01: trim_sweep / trim_db"
  - "01-05: atomic save (save_archive_to, rebuild_zip, atomic_replace)"
provides:
  - "dry_run_downgrade: trim-first v16->v14 preview reusing DryRunReport"
  - "save_v14_copy: throwaway-copy v14 export, live session stays v16"
  - "write_archive_from_db_source: session-untouching atomic writer"
  - "Tauri commands downgrade_dry_run + save_v14_copy"
affects:
  - "Wave 3 frontend (v14 export UI) consumes both commands"
tech-stack:
  added: []
  patterns:
    - "throwaway std::fs::copy + downgrade-on-copy keeps live session byte-identical"
    - "trim-FIRST order shared by preview and save so survivor selection never diverges"
    - "semantic before/after PK diff reused for downgrade preview"
key-files:
  created:
    - app/src-tauri/tests/downgrade_orchestration_tests.rs
  modified:
    - app/src-tauri/src/db/delete.rs
    - app/src-tauri/src/archive/downgrade.rs
    - app/src-tauri/src/archive/save.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/common/mod.rs
    - app/src-tauri/tests/schema_downgrade_tests.rs
decisions:
  - "Dedicated stable-survivor fixture for exact repoint-count assertions; raw Wave-1 fixture reused as the HIGH-2 trim-eligible-lowest case"
  - "overwritten[target] = repointed - dedup-deleted; dedup-deleted + merged-away rows land in deleted (HIGH-3)"
metrics:
  duration: ~1h
  completed: 2026-07-22
---

# Phase 4 Plan 02: Dry-Run Preview + Throwaway-Copy v14 Save Summary

Non-destructive v14 export: `save_v14_copy` runs the lossy v16->v14 downgrade on a `std::fs::copy` throwaway so the live session stays byte-identical at v16, writing v14 bytes to the user's chosen path through a new session-untouching atomic writer; `dry_run_downgrade` reuses Phase 2's `DryRunReport` with the SAME trim-first order so the preview faithfully matches the save (survivor, exact repoint counts, dedup-DELETED study rows surfaced as loss).

## What was built

- **`db::delete` refactor:** `snapshot_pks`, `snapshot_all`, `diff_snapshots`, `TRACKED_TABLES` made `pub(crate)`; new `pub(crate) snapshot_tables(tx, tables)` that `snapshot_all` delegates to — the diff logic is reused, never copy-pasted.
- **`archive::downgrade::dry_run_downgrade`:** `PragmaGuard` + FK-off + rolled-back `unchecked_transaction`. Order inside the tx is identical to `save_v14_copy`: `trim_sweep` FIRST, snapshot post-trim, compute exact per-target repoint counts (`COUNT(*) WHERE col IN merged-old-ids`, separate count for `Bookmark.PublicationLocationId`) + per-composite-target dedup-delete counts, run the REAL `run_downgrade_ddl`, re-snapshot, `diff_snapshots`, then set `overwritten[target] = repoint - dedup` and surface dedup-deleted rows of the non-single-PK targets in `deleted`. Never VACUUMs; working DB left byte-unchanged.
- **`compute_merge_groups` extraction:** the grouping (typed key, lowest-id survivor) is now shared by `run_downgrade_ddl` and the preview, so survivor selection can never diverge.
- **`archive::save::write_archive_from_db_source` (MED-5):** reads userData.db bytes from an arbitrary `db_source`, builds the manifest off that source (schema_version + hash from the copy), streams all other entries read-only from `session.temp_dir`, atomic-replaces the target — never trims, never writes manifest.json into `session.temp_dir`. `save_archive_to` now delegates to it (normal path keeps its trim + temp_dir manifest sync).
- **`archive::downgrade::save_v14_copy`:** `fs::copy` session DB to a throwaway, `trim_db` THEN `downgrade_to_v14` on the copy, write via the helper, best-effort delete the copy. Does not set `session.dirty`/`target_path`.
- **Tauri commands:** `downgrade_dry_run` + `save_v14_copy` registered in `invoke_handler!`, both mapping typed errors to `ErrorDto`.

## Tests

- `schema_downgrade_tests.rs` (+2 = 16): exact repoint counts + `deleted["Location"]==2` on a stable-survivor fixture; dedup-deleted TagMap row surfaced in `deleted`, absent from `overwritten` (HIGH-3).
- `downgrade_orchestration_tests.rs` (new, 4): session stays v16 + SHA-256 byte-identical after save, output is valid merged v14 (MED-5); dry-run survivor == save survivor with a trim-eligible lowest-id Location, survivor shifts 20->50 under trim-first (HIGH-2); un-downgradeable fails `SchemaDowngradeFailed` with no output file + session unchanged (HIGH-1); v14 output round-trips through `open_and_validate` (v14->v16) with merged state preserved semantically.
- Full workspace: **all green** — `jwlmanager_lib` unit 29; integration binaries incl. downgrade_orchestration 4, schema_downgrade 16, save 4, trim 14, schema_upgrade 17 (0 failed across all binaries; 4 pre-existing ignored).

## Verification

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean (the `ts-rs`/`try_from` parse note is a pre-existing informational warning, not a clippy lint).
- `cargo test`: full workspace green.
- Frontend not touched (`npm run build` not required).

## Deviations from Plan

### Auto-fixed / design adjustments (Rule 1 — correctness)

**1. [Rule 1 - Fixture correctness] Dedicated stable-survivor dry-run fixture.**
- **Found during:** Task 1.
- **Issue:** The plan's Task-1 behavior asserts `deleted["Location"]==2` with exact per-target repoint counts "on the Wave-1 collision fixture." But that fixture's lowest-id survivor (20) and its scripture Location (100) are UNREFERENCED, so the mandated trim-FIRST pass sweeps them — shifting the survivor to 50 and making `deleted["Location"]` == 1, not 2. Under trim-first the raw fixture is precisely a HIGH-2 trim-eligible-lowest case, not a stable-count case.
- **Fix:** Added `generate_v16_dryrun_collision_db` / `_archive` — a group whose survivor is kept trim-stable (content-bearing Note) and whose remap targets are each trim-stable and reference a non-survivor id — for the exact-count assertions. Reused the raw Wave-1 collision fixture (via a new `generate_v16_collision_archive`) as the HIGH-2 trim-eligible-lowest test, where preview and save both correctly shift the survivor to 50.
- **Files:** `tests/common/mod.rs`, `tests/schema_downgrade_tests.rs`, `tests/downgrade_orchestration_tests.rs`.

**2. [Rule 1 - Fixture correctness] HIGH-1 archive references its colliding Locations.**
- **Found during:** Task 3.
- **Issue:** `save_v14_copy` trims BEFORE downgrading. The bare HIGH-1 fixture's two U2-colliding Locations are unreferenced, so trim would sweep them and the collision would vanish, making the archive downgradeable — defeating the end-to-end un-downgradeable test.
- **Fix:** `generate_high1_undowngradeable_archive` adds content-bearing Notes referencing both colliding Locations so the collision persists past trim and the downgrade preflight genuinely fails.
- **Files:** `tests/common/mod.rs`.

No architectural changes. No new dependencies (T-04-SC). No auth gates.

## Known Stubs

None.

## Threat Flags

None — no new trust-boundary surface beyond the two planned commands (both `ErrorDto`-sanitized).

## Self-Check: PASSED

- `app/src-tauri/tests/downgrade_orchestration_tests.rs` — FOUND.
- `save_v14_copy` + `dry_run_downgrade` in `archive/downgrade.rs`, `write_archive_from_db_source` in `archive/save.rs`, both commands in `lib.rs` — FOUND (compiled + tested).
- Commits present: dry_run_downgrade, save helper/commands, orchestration tests.
