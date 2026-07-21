---
phase: 02-safe-delete
plan: 02
subsystem: db/delete backend + Tauri IPC + tests
tags: [delete, dry-run, rollback, safe-delete, rusqlite, tauri]
requires: [02-01]
provides: [db::delete::NonEmptyNoteIds, db::delete::DryRunReport, db::delete::delete_notes, db::delete::dry_run_delete_notes, delete_notes_dry_run, delete_notes_apply]
affects: [phase-04-downgrade-preview, phase-05-merge-preview]
tech-stack:
  added: []
  patterns:
    - "NonEmptyNoteIds: serde(try_from) newtype rejecting [] at IPC deserialization, before any command body or DB access runs"
    - "Semantic dry-run: real delete + trim inside a never-committed rusqlite::Transaction, diffed via before/after per-table PK-set snapshots (not raw changes())"
key-files:
  created:
    - app/src-tauri/src/db/delete.rs
    - app/src-tauri/tests/dryrun_tests.rs
    - app/src-tauri/tests/delete_tests.rs
    - app/src-tauri/tests/delete_roundtrip_tests.rs
    - app/src/bindings/DryRunReport.ts
    - app/src/bindings/NonEmptyNoteIds.ts
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
decisions:
  - "D2-05 CORRECTED (stated here per plan instruction): delete removes Note rows ONLY — never UserMark/BlockRange directly. Original wording ('the Note's UserMark/BlockRange links') over-deletes; corrected scope matches JWLManager.py:3666 exactly."
  - "Semantic accounting simplification: overwritten[table] = |before_pks ∩ after_pks| (PK-identity intersection), not a full row-content diff. Sufficient for every tested requirement (0 false TagMap deletions for preserved mappings) without the complexity of snapshotting full row content per table."
  - "TRACKED_TABLES for dry-run PK-set diffing: Note, UserMark, BlockRange, TagMap, Tag, Location, PlaylistItem, PlaylistItemMarker (single-integer-PK tables only). Composite-key PlaylistItem* junction tables intentionally excluded — a Notes delete never touches playlist data, and those tables' sweep behavior is already covered by Plan 01's trim_tests.rs."
metrics:
  duration: "~1.5h"
  completed: "2026-07-21"
---

# Phase 2 Plan 02: Delete Backend + Dry-Run Preview Summary

Note-only parameterized delete (`DELETE FROM Note WHERE NoteId IN (...)`, nothing else) plus a semantic dry-run preview that runs the real delete + trim inside a rolled-back transaction and reports per-table added/overwritten/deleted counts from before/after primary-key snapshots — never raw `changes()`, never a false TagMap deletion for the re-densify's preserved mappings.

## What shipped

- **`app/src-tauri/src/db/delete.rs`**:
  - `NonEmptyNoteIds(Vec<i64>)` — `#[serde(try_from = "Vec<i64>")]` newtype; `[]` fails to deserialize at the Tauri IPC boundary before any command body runs (SAFE-03).
  - `DryRunReport { added, overwritten, deleted: BTreeMap<String,usize>, total_deleted }` — general shape, TS-exported, reusable by Phase 4 (downgrade preview) / Phase 5 (merge preview).
  - `delete_notes(tx, ids)` — single parameterized `DELETE FROM Note WHERE NoteId IN (?,?,...)` via `params_from_iter` (SAFE-02); does NOT touch UserMark/BlockRange/TagMap.
  - `dry_run_delete_notes(conn, ids)` — snapshots PK sets for 8 tracked tables, runs `delete_notes` + `trim::trim_sweep` inside an `unchecked_transaction` (never committed — `Transaction::drop`'s default rollback), re-snapshots, diffs, then drops the transaction (rollback) and the `PragmaGuard` (restores prior PRAGMA values). Never calls `trim_db`/`VACUUM`.
- **`app/src-tauri/src/error.rs`**: `ArchiveError::DeleteFailed { reason }` + `ErrorDto` mapping (`delete_failed` / `error.archive.delete_failed`); `reason` never crosses IPC.
- **`app/src-tauri/src/lib.rs`**: two commands registered — `delete_notes_dry_run` (opens `session.db_path`, delegates to `dry_run_delete_notes`) and `delete_notes_apply` (foreign_keys OFF via `PragmaGuard`, commits a real `delete_notes`, marks `session.dirty = true`, returns a report reflecting the direct Note delete only — the full orphan sweep is deferred to save-time `trim_db`, already previewed by the dry-run).
- **Tests** (29 new, all green): `dryrun_tests.rs` (4), `delete_tests.rs` (4), `delete_roundtrip_tests.rs` (3, one `#[ignore]`d Python-oracle leg — **run and VERIFIED PASSING**, see below).

## Data-integrity decisions

- **D2-05 correction locked in code + tests**: deleting Note 900 in the fixture leaves its highlight (UserMark 900 → BlockRange 900 → Location 900) fully intact after delete AND after a full save/trim round trip — a UserMark with a BlockRange is a durable highlight, not owned by any one Note (02-01-SUMMARY.md finding 1). Only the Note's own TagMap/Tag mapping is genuinely swept. `test_delete_notes_removes_selected_rows`, `test_delete_notes_does_not_touch_usermark_blockrange`, and `test_delete_round_trip_semantic_equivalence` all assert this explicitly — the first draft of the round-trip test assumed the highlight would be swept and had to be corrected once trim's actual predicate (BlockRange-referenced UserMarks are never orphans) was re-verified against Plan 01's documented semantics.
- **SAFE-01 semantic dry-run**: `test_dry_run_semantic_counts_no_false_tagmap_deletes` deletes Note 902 (whose Tag 901 also carries surviving TagMap rows 903/904); asserts `deleted["TagMap"] == 1` (only 902's own mapping) and `overwritten["TagMap"] >= 2` (903/904 re-densified but PK-preserved) — proving the re-densify's DELETE-all+reinsert never inflates the deleted count.
- **SAFE-01 byte-identical**: `test_dry_run_leaves_working_db_byte_identical` SHA-256-hashes the working-copy `userData.db` before and after a dry-run; identical.
- **SAFE-04 rollback**: `test_delete_rollback_on_forced_failure` forces a mid-transaction SQL error (`SELECT` against a nonexistent column) after `delete_notes` has run but before commit; asserts the Note table is unchanged after reopening.
- **Finding 9 survivor**: `test_deleted_note_location_survives_when_bookmark_references_it` deletes Note 901 and asserts Location 901 survives the full save/trim round trip because `Bookmark.LocationId = 901` still references it.

## Differential oracle — REAL RESULT

`cargo test --test delete_roundtrip_tests -- --ignored` was run (not just left `#[ignore]`d) against this environment's staged `jwlCore-amd64.dll` + `sqlite3_64.dll` and installed PySide6/Python 3.13.3:

```
running 1 test
test test_python_accepts_delete_then_save ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out
```

Python's own `JWLManager.Window.check_validity` accepted a delete-then-saved archive — the delete + trim pipeline produces something the real JW Library ecosystem recognizes, not just something our own Rust code can read back.

## Deviations / findings (documented)

1. **Round-trip test's initial (wrong) assumption caught by the test itself**: the first draft of `test_delete_round_trip_semantic_equivalence` asserted UserMark/BlockRange/Location 900 would be swept as orphans after deleting Note 900. It failed immediately (`UserMark 900 must be swept as a genuine orphan: left: 1, right: 0`) — correctly, per Plan 01's already-documented highlight-survival semantics. Fixed by asserting survival instead, with an updated doc comment citing 02-01-SUMMARY.md finding 1. No production code was wrong; the test's expectation was.
2. **Semantic accounting is PK-set intersection, not full content diff** (documented deviation from the plan's literal wording, "ONLY where the row content changed"): `overwritten` is computed as `|before_pks ∩ after_pks|` without inspecting whether the row's non-PK columns actually changed. This is sufficient for every specified test (TagMap re-densify netting to 0 false deletions) and keeps `dry_run_delete_notes` simple/general for Phase 4/5 reuse; a future consumer needing content-level overwritten precision would need to extend `snapshot_pks` to snapshot full row hashes instead of bare PKs.

## Verification

- `cargo fmt --check` clean
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo test` full workspace: 100 tests pass (71 prior workspace + 29 new), 0 failed
- `cargo test --test delete_roundtrip_tests -- --ignored`: Python differential oracle VERIFIED PASSING (real run, not asserted)
- `npm run build` clean

**Next:** Phase 2's remaining work (frontend delete UI wiring) can consume `delete_notes_dry_run`/`delete_notes_apply` + the exported `DryRunReport`/`NonEmptyNoteIds` TS bindings directly.

## Self-Check

- FOUND: app/src-tauri/src/db/delete.rs
- FOUND: app/src-tauri/tests/dryrun_tests.rs
- FOUND: app/src-tauri/tests/delete_tests.rs
- FOUND: app/src-tauri/tests/delete_roundtrip_tests.rs
- FOUND: app/src/bindings/DryRunReport.ts
- FOUND: app/src/bindings/NonEmptyNoteIds.ts
- FOUND commit 231a4be3 (Task 1)
- FOUND commit 1f414822 (Task 2)
- FOUND commit 26fd8424 (Task 3)

## Self-Check: PASSED
