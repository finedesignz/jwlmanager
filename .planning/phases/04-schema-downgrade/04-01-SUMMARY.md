---
phase: 04-schema-downgrade
plan: 01
subsystem: archive/schema
tags: [schema, downgrade, sqlite, remap, data-integrity]
requires: [pragma_guard, error.rs, archive/upgrade.rs]
provides: [downgrade_to_v14, SchemaDowngradeFailed]
affects: [archive/mod.rs, error.rs, errors.ts]
tech-stack:
  added: []
  patterns: [PragmaGuard-FK-off, unchecked_transaction, dedup-then-repoint, typed-tuple-group-key]
key-files:
  created:
    - app/src-tauri/src/archive/downgrade.rs
    - app/src-tauri/tests/schema_downgrade_tests.rs
  modified:
    - app/src-tauri/src/error.rs
    - app/src/lib/errors.ts
    - app/src-tauri/src/archive/mod.rs
    - app/src-tauri/tests/common/mod.rs
    - app/src-tauri/tests/error_tests.rs
decisions:
  - "D4-01: ORDER BY LocationId → deterministic lowest-id survivor (pure function of input)"
  - "MED-4: typed Option-tuple GroupKey, not Python's stringified key — 'None' string ≠ SQL NULL"
  - "Composite-key targets use dedup-then-repoint (DELETE colliding then plain UPDATE), never UPDATE OR IGNORE"
  - "HIGH-1: GROUP-BY preflight with all-columns-NOT-NULL filter to match SQLite UNIQUE NULL-distinctness; + belt-and-suspenders INSERT UNIQUE catch"
metrics:
  duration: ~1h
  completed: 2026-07-22
---

# Phase 4 Plan 01: v16→v14 Schema Downgrade Core Transform Summary

`downgrade_to_v14(conn)` ports `JWLManager.py:1172-1236` to a single-transaction Rust transform that merges v14-U2-colliding Locations onto the deterministic lowest LocationId, repoints all 7 FK columns (dedup-then-repoint on the four composite-key targets), refuses un-downgradeable archives with a typed error + full rollback, drops Specialty/Edition, and stamps user_version=14.

## What shipped

- **`ArchiveError::SchemaDowngradeFailed { reason }`** mirroring `SchemaUpgradeFailed` — DTO `code="schema_downgrade_failed"`, reason never crosses IPC (asserted). `errors.ts` case added; frontend build clean.
- **`archive/downgrade.rs`**: `downgrade_to_v14` (version gate 14=no-op / !=16=err / 16=run) → `PragmaGuard` snapshot + `PRAGMA foreign_keys=OFF` + `unchecked_transaction` → `run_downgrade_ddl` (typed-tuple grouping w/ `ORDER BY LocationId`, `remap_location` 7-column closure, `check_downgradeable` preflight, `CREATE_LOCATION_V14` rebuild) → commit / rollback-on-drop.
- **Test matrix (14 tests) + 8 synthetic fixture builders** in `common/mod.rs`.

## Deviations from Plan

### Structural (not behavioral)

**1. [Rule 3 — clarity] Tasks 1 and 2 committed together.** The plan sequenced a Task-1 `todo!()` skeleton then a Task-2 fill. Writing the complete module in one pass and committing Tasks 1+2 as one `feat` commit (with error_tests passing) was cleaner than an intentionally-broken intermediate. Task 3 (fixtures+tests) is its own commit. All three tasks' DoD verified.

### Auto-fixed

**2. [Rule 1 — fixture correctness] Colliding-group Locations must differ by Specialty.** The v16 `res/blank` carries `IX_Location_Media` — a UNIQUE index over the six U2 columns **plus** `COALESCE(Specialty,'')`/`COALESCE(Edition,'')`. Seeding a collision group with identical Specialty/Edition violated that v16 index at insert time. Fixed by giving each group row a distinct `Specialty` (`s<id>`). This is not a workaround — it is precisely the real-world un-downgradeable-collision shape: two rows legal in v16 *because* they differ only by Specialty/Edition, which v14 drops, collapsing them under U2. Documented in `insert_collision_group`.

### Preflight implementation choice

**3. HIGH-1 preflight uses the GROUP-BY form** (plan offered GROUP-BY *or* catch-the-UNIQUE-error). Implemented the explicit GROUP-BY preflight for a clear named message, with the correct SQLite NULL semantics (exclude rows where any indexed column is NULL, since UNIQUE treats NULLs as distinct while GROUP BY treats them as equal). Kept a belt-and-suspenders UNIQUE-error catch on the `Location_new` INSERT (`map_downgrade_insert_err`). MED-4 test proves no false positive from NULL columns.

## Verification

- `cargo fmt --check` — clean (exit 0)
- `cargo clippy --all-targets -- -D warnings` — clean (only pre-existing ts-rs `try_from` parse warning, unrelated)
- `cargo test` full workspace — **green**: schema_downgrade_tests 14/14, error_tests 5/5, schema_upgrade_tests 17/17, fixtures 6/6, trim_tests 14/14, plus all others (0 failures across all binaries)
- `npm run build` (app/) — clean (tsc + vite, errors.ts case compiles)

## Commits

- `0427316b` feat(04-01): downgrade_to_v14 transform + SchemaDowngradeFailed error
- `5a2a287c` test(04-01): schema_downgrade fixtures + full v16→v14 test matrix

## Self-Check: PASSED

- FOUND: app/src-tauri/src/archive/downgrade.rs (`pub fn downgrade_to_v14`)
- FOUND: app/src-tauri/tests/schema_downgrade_tests.rs (14 tests)
- FOUND: error.rs `SchemaDowngradeFailed`
- FOUND commit 0427316b, 5a2a287c
