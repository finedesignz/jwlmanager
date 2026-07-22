---
phase: 04-schema-downgrade
verified: 2026-07-22T12:00:00Z
status: passed
score: 4/4 success criteria verified (+ 4/4 data-integrity core checks)
verifier: gsd-verifier (goal-backward, adversarial)
ship_verdict: SHIP
---

# Phase 4: Schema Downgrade — Verification Report

**Phase Goal:** User who needs v14 compatibility can explicitly opt into a downgraded save without losing data integrity.
**Verified:** 2026-07-22 (adversarial, goal-backward; claims re-run, not trusted)
**Status:** PASSED — 4/4 success criteria, all data-integrity core checks hold.

## Evidence Re-Run (not trusted from SUMMARY)

| Check | Command | Result |
|-------|---------|--------|
| Downgrade unit matrix | `cargo test --test schema_downgrade_tests` | 16 passed / 0 failed |
| Differential (env-gated) | `cargo test --test differential` | 2 passed, 3 ignored |
| Cross-impl equivalence | `rust_downgrade_matches_python_downgrade_normalized` | PASS (ran vs real python3 3.13.3) |
| **Real Python app opens Rust v14 output** | `cargo test --test differential -- --ignored python_app_opens_downgraded_v14_archive` | **PASS** (PySide6 present, `check_validity` accepts the Rust-downgraded archive) |
| Frontend command bar | `npx vitest run CommandBar.test.tsx` | 14 passed |

The strongest oracle — the actual PySide6 JW Library app opening the Rust-produced v14 archive — was executed and passed. Establisher's claim confirmed independently.

## Success Criteria

| # | Criterion | Verdict | Evidence |
|---|-----------|---------|----------|
| 1 | Explicit opt-in v14 save (never default/implicit) | **PASS** | Dedicated `save_v14_copy` command + separate "Save v14-compatible copy…" button (`CommandBar.tsx:235-244`), distinct from `save`/`save_as`. Never on any default path. |
| 2 | Dry-run preview before downgrade save (Phase 2 mechanism) | **PASS** | `handleSaveV14` calls `downgrade_dry_run` → reuses `DryRunReport`/`DeletePreviewDialog`; only `handleV14Confirm` invokes `save_v14_copy`. Cancel/dismiss writes nothing (`CommandBar.tsx:153-191`). |
| 3 | 7-column LocationId remap closure → semantically correct v14, round-trip verified | **PASS** | `remap_location` covers all 7 FK columns (Bookmark ×2, Note, UserMark, InputField, TagMap, PlaylistItemLocationMap) in Python-verbatim order; `test_round_trip_all_dependents_repoint_to_survivor` + real Python app opens the output. |
| 4 | After v14 save, working copy stays v16 (backup/restore) | **PASS** | `save_v14_copy` runs `trim_db`+`downgrade_to_v14` on a throwaway `fs::copy`; session accessed `as_ref`, never mutated (`downgrade.rs:600-630`, `lib.rs:241-259`). No `dirty`/`target_path` mutation. |

## Data-Integrity Core (the reason this phase exists)

| Check | Verdict | Evidence |
|-------|---------|----------|
| D4-01 deterministic survivor (ORDER BY LocationId, lowest), stable across runs | **PASS** | `ORDER BY LocationId` in grouping query (`downgrade.rs:221`); `test_deterministic_lowest_id_survivor` + `test_survivor_is_pure_function_run_twice`. |
| 4 composite-key collision targets dedup-then-repoint (no dangling FK / UNIQUE/PK violation) | **PASS** | DELETE-collider-then-plain-UPDATE for Bookmark(Pub,Slot), InputField(Loc,TextTag), TagMap(Tag,Loc), PlaylistItemLocationMap(Item,Loc); 4 composite tests pass. Never `UPDATE OR IGNORE`. |
| Un-downgradeable archive → typed error + full rollback (session stays v16), never partial write | **PASS** | `check_downgradeable` preflight (both v14 uniques) + INSERT belt-and-suspenders → `SchemaDowngradeFailed`; single-tx rollback-on-drop; `test_high1_undowngradeable_rolls_back` + `test_rollback_on_poisoned_state` assert user_version==16 and Location row-set unchanged. |
| Semantic verification (never byte-diff); Python differential oracle passes | **PASS** | `rust_downgrade_matches_python_downgrade_normalized` (normalized table state, ran) + `python_app_opens_downgraded_v14_archive` (ran, PASS). No byte-diffing anywhere. |
| MED-4 correctness win over Python (`'None'` vs NULL) | **PASS** | Typed `Option`-tuple `GroupKey`; `test_med4_none_string_not_merged_with_null`. Rust is *more* correct than the Python oracle, deliberately and tested. |

## Adversarial Notes (Core Value: never lose/corrupt an archive)

- **No mutation of the live session.** The lossy transform provably runs only on a `fs::copy`; the live `db_path` is opened `as_ref`. A failed downgrade cannot corrupt the open archive — it rolls back on drop and the copy is deleted best-effort on every path.
- **Data loss is surfaced, not hidden.** Dedup-DELETEd study rows land in `deleted` (real loss), repointed rows in `overwritten` (moved) — the preview cannot understate loss (`dry_run_downgrade` step 7, HIGH-3). User sees this before confirming.
- **Preview cannot diverge from the save.** Both share `compute_merge_groups` and trim-FIRST ordering (HIGH-2), so the survivor chosen in preview equals the one written.
- **Error opacity is correct.** Internal `reason` never crosses IPC; DTO exposes only stable code + generic message_key (`error.rs:114`).

## Ship Verdict

**SHIP.** All 4 roadmap success criteria PASS. All data-integrity core guarantees hold under re-run, including the definitive oracle (real PySide6 JW Library app opens the Rust-produced v14 archive). No partial-write, no session mutation, no dangling FK, deterministic survivor. No gaps, no blockers, no human-verification items outstanding.

---
_Verified: 2026-07-22 — Claude (gsd-verifier)_
