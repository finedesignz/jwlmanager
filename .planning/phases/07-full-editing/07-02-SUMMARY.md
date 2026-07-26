---
phase: 07-full-editing
plan: "02"
subsystem: editing
tags: [highlights, recolor, geometric-merge, rust, react]
dependency-graph:
  requires: ["07-01 db/edit.rs safety spine", "07-01 EditPreviewDialog"]
  provides: ["Highlight/Note recolor end-to-end (EDIT-02)", "merge_block_ranges standalone primitive (Phase 8 import dependency)", "Highlights delete (D7-10)"]
  affects: [app/src-tauri/src/db, app/src-tauri/src/error.rs, app/src-tauri/src/lib.rs, app/src/components/CategoryList.tsx, app/src/lib/operations.ts]
tech-stack:
  added: []
  patterns:
    - "pure geometry fn (plan_merge) + thin SQL executor, separated so DELETE-on-predicate logic is testable without a DB"
    - "hand-rolled dependency-free RFC-4122-v4-shaped GUID (guid.rs) seeded via a threaded u64, mirroring time.rs's no-new-dep precedent"
    - "tagged-enum non-empty selection (ColorSelection) so two categories share one command pair while each keeps its own empty-unrepresentable identity-PK wrapper"
key-files:
  created:
    - app/src-tauri/src/db/highlights.rs
    - app/src-tauri/src/db/color.rs
    - app/src-tauri/src/guid.rs
    - app/src-tauri/tests/highlight_merge_tests.rs
    - app/src-tauri/tests/color_tests.rs
    - app/src/components/ColorMenu.tsx
    - app/src/components/ColorMenu.test.tsx
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/db/delete.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src/components/CategoryList.tsx
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
    - app/src/lib/errors.ts
    - app/src/styles.css
    - app/src/App.test.tsx
decisions:
  - "D7-03 resolved as option-a, strict Python parity (pre-authorized by the team lead before execution, not paused on): merge_block_ranges ships as a standalone exhaustively-tested primitive; recolor never invokes it. ROADMAP criterion 1 is satisfied by the primitive's existence + round-trip tests, not by recolor merging."
  - "UserMark GUID synthesis uses a hand-rolled RFC-4122-v4-shaped formatter (guid.rs, SplitMix64-seeded) rather than a new uuid/rand dependency, following time.rs's established precedent — no package-legitimacy checkpoint needed."
  - "ColorSelection is a tagged enum ({category:'Highlights',ids}|{category:'Notes',ids}) rather than a bare Vec<i64>+Category param, so the empty-selection-unrepresentable guarantee holds per category at IPC deserialization."
metrics:
  duration: "single session"
  completed: 2026-07-26
status: complete
---

# Phase 7 Plan 2: Highlight recolor + merge_block_ranges + Highlights delete Summary

Shipped highlight/note recolor (EDIT-02) with both Python side-effects (Note→UserMark synthesis, Highlights+Grey no-op) faithfully ported, the geometric `merge_block_ranges` union-merge primitive built and exhaustively tested as a standalone unit with zero recolor trigger (D7-03 strict-parity resolution), and Highlights delete (D7-10) targeting `BlockRange` only.

## What Was Built

**Task 1 (D7-03 checkpoint) — resolved pre-authorized, not paused on.** Per the team lead's brief, option-a (strict Python parity) was applied without stopping: `merge_block_ranges` ships standalone; `db::color` never invokes it (enforced by a negative source grep — verified no textual reference to the primitive's name anywhere in `color.rs`, including doc comments).

**Task 2 (commit `6206e55a`) — `merge_block_ranges` geometric primitive.** `db/highlights.rs` ports the union-merge from `add_usermark` (`JWLManager.py:2160-2184`): `plan_merge(existing: &[(i64,i64,i64)], ns, ne) -> (Vec<i64>, (i64,i64))` is a pure function (no SQL, no rusqlite type) implementing the exact overlap predicate `ce >= ns && ne >= cs`, iterating to a fixed point so chained/transitive overlaps of 3+ ranges all absorb in one call. `merge_block_ranges(tx, identifier, location_id, ns, ne, block_type, user_mark_id)` is the thin SQL executor: SELECTs existing ranges joined through `UserMark` for `LocationId` (BlockRange itself has no LocationId column), computes the plan, DELETEs absorbed ids (placeholder-count-only dynamic SQL), INSERTs one merged row carrying `block_type` through (never defaulted). 13 unit tests cover every boundary case (empty, non-overlapping both directions, touching-boundary both directions, one-token-past-boundary miss both directions, fully-contained both directions, chained triple-overlap, color-agnostic grouping) plus 2 SQL-layer round-trip tests on a synthetic v16 fixture asserting absorbed rows are gone, exactly one merged row remains, its `BlockType` is carried through, and a disjoint range is untouched.

**Task 3 (commit `e5389f6a`) — recolor backend + Highlights delete + ColorMenu.** `db/color.rs` ports `set_color` (`JWLManager.py:3237-3278`): `ColorSelection` tagged enum (`Highlights{ids: NonEmptyBlockRangeIds}` | `Notes{ids: NonEmptyNoteIds}`) drives `apply_color(tx, selection, color_index, guid_seed)` — Highlights branch resolves UserMarkIds from selected BlockRangeIds then `UPDATE UserMark SET ColorIndex`, with a Grey (ColorIndex 0) early-return no-op; Notes branch synthesizes a UserMark (`StyleIndex 0`, fresh GUID, `Version 1`, the note's LocationId, chosen color) for every selected note with a LocationId but no UserMarkId, links it via `Note.UserMarkId`, then resolves and recolors every selected note's UserMark (including ones already linked before the call) — Notes+Grey is explicitly NOT a no-op, it still synthesizes. `dry_run_color`/`apply_color_reporting` follow the established rolled-back-transaction / real-committed-transaction envelope shapes. `guid.rs` hand-rolls a SplitMix64-seeded RFC-4122-v4-shaped GUID formatter (no `uuid`/`rand` dependency), seeded per-note (`guid_seed ^ note_id`) so multiple notes in one call never collide while the same seed reproduces the same GUID (verified by a same-seed-twice acceptance test). `delete.rs` gains `delete_highlights`/`dry_run_delete_highlights` (BlockRange only, never UserMark — rule #9), reusing `color.rs`'s `NonEmptyBlockRangeIds` rather than a second identical type. `error.rs` gains `ArchiveError::ColorFailed` → `color_failed` DTO code. `lib.rs` registers `color_dry_run`/`color_apply`/`highlight_delete_dry_run`/`highlight_delete_apply`, with a `guid_seed_now()` helper deriving the real seed from wall-clock nanos (never called from core `db::color` functions directly — those always take an explicit `guid_seed: u64` param). `ColorMenu.tsx` is a 7-swatch popover (8px radius, first `box-shadow` in the app per UI-SPEC, 20×20px 4px-radius swatches) anchored below-left of the Color toolbar button via a `position: relative` wrapper in `CategoryList.tsx`; Grey renders `disabled` with the exact UI-SPEC tooltip when `category === "Highlights"`, stays enabled for Notes; picking a color closes the popover and immediately fires `color_dry_run` → `EditPreviewDialog` with the conditional two-clause summary (recolor-only vs recolor+synthesis) per UI-SPEC. `operations.ts` flips `Notes:color`, `Highlights:color`, `Highlights:delete` to `LIVE`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `stmt.query_map(...)?.collect()` temporary-lifetime errors across four call sites**
- **Found during:** Initial `cargo check` after writing `color.rs`/`highlights.rs`.
- **Issue:** `let mut stmt = tx.prepare(...)?; stmt.query_map(...).collect()` inside a block-expression used as the block's tail value fails to borrow-check (E0597) — the `MappedRows` iterator's lifetime is tied to `stmt`, which the compiler drops before the block's return value is fully evaluated in this exact shape.
- **Fix:** Bind the collected `Vec` to a local (`let rows = stmt.query_map(...)...?; rows`) before the block ends, per the compiler's own suggested fix. Applied identically in `color.rs` (3 sites) and `highlights.rs` (1 site).
- **Files modified:** `app/src-tauri/src/db/color.rs`, `app/src-tauri/src/db/highlights.rs`
- **Commits:** `6206e55a`, `e5389f6a`

**2. [Rule 1 - Bug] Literal string "merge_block_ranges" in `color.rs`'s own module-doc comment tripped the plan's negative grep**
- **Found during:** Explicitly running the plan's stated verification command (`grep -n merge_block_ranges app/src-tauri/src/db/color.rs`) before considering Task 3 done.
- **Issue:** The module doc explaining WHY recolor doesn't merge referenced the primitive by its literal function name, which the negative grep (correctly, per its literal text) flagged as a match — even though no code path invokes it.
- **Fix:** Reworded the doc comment to describe the primitive by role ("the geometric range-union-merge primitive `db::highlights` exposes") instead of by name, preserving the same explanation with zero textual occurrence of the string.
- **Files modified:** `app/src-tauri/src/db/color.rs`
- **Commit:** `e5389f6a`

**3. [Rule 1 - Bug] Two pre-existing tests asserted the OLD (pre-this-plan) LIVE set**
- **Found during:** `npx vitest run` after wiring `operations.ts`'s new LIVE entries.
- **Issue:** `operations.test.ts` hard-coded a local `LIVE_PAIRS` mirror-list (by design, per its own comment, so a silent LIVE-set drift fails loudly) that predated `Notes:color`/`Highlights:color`/`Highlights:delete`. `App.test.tsx`'s criterion-3 test asserted Highlights had NO live delete button, which this plan makes false.
- **Fix:** Updated `LIVE_PAIRS` and the Notes-at-0-selected per-op assertion in `operations.test.ts` to include the three newly-live pairs. Rewrote the `App.test.tsx` Highlights-switch assertion to check the now-live delete button is present and substitute `export` (still deferred) as the deferred-affordance example, preserving the test's actual intent (op-bar renders both live and deferred correctly) rather than deleting coverage.
- **Files modified:** `app/src/lib/operations.test.ts`, `app/src/App.test.tsx`
- **Commit:** `e5389f6a`

### Threat Flags

None beyond what the plan's own threat register (T-07-07..T-07-12, T-07-SC) already anticipated and mitigated.

## Test Output (actual, this session)

```
$ cd app/src-tauri && cargo test --jobs 2
running 68 tests (unit, lib) ... test result: ok. 68 passed; 0 failed
running 2 tests (highlight_merge_tests) ... test result: ok. 2 passed; 0 failed
running 7 tests (color_tests) ... test result: ok. 7 passed; 0 failed
... every other suite (delete_tests, favorites_tests, trim_tests, schema_upgrade_tests,
    schema_downgrade_tests, downgrade_orchestration_tests, save_tests, merge_orchestration,
    fixtures, manifest_tests, dryrun_tests, error_tests, extract_tests, merge_ffi,
    new_archive_tests, notes_query_tests, open_archive_tests, differential): ok, 0 failed
    (a handful of environment-gated Python-parity tests remain intentionally ignored,
    same as before this plan)

$ cd app/src-tauri && cargo clippy --jobs 2 --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.67s
(only pre-existing non-blocking ts-rs try_from serde-attribute-parse warnings)

$ cd app && npx vitest run
 Test Files  9 passed (9)
      Tests  91 passed (91)
```

All three commands green, zero failures, run in this session on this exact code.

## Self-Check: PASSED

- `app/src-tauri/src/db/highlights.rs` — FOUND
- `app/src-tauri/src/db/color.rs` — FOUND
- `app/src-tauri/src/guid.rs` — FOUND
- `app/src-tauri/tests/highlight_merge_tests.rs` — FOUND
- `app/src-tauri/tests/color_tests.rs` — FOUND
- `app/src/components/ColorMenu.tsx` — FOUND
- `app/src/components/ColorMenu.test.tsx` — FOUND
- `git log --oneline` shows `6206e55a`, `e5389f6a` — FOUND

## Known Stubs

None — recolor, Highlights delete, and the merge primitive are all fully wired with real backend, real tests, and real UI; no placeholder data paths.

## Verification of Plan Prohibitions

- `grep -n "merge_block_ranges" app/src-tauri/src/db/color.rs` → no matches (verified after the doc-comment fix).
- `grep -rn "DELETE FROM UserMark" app/src-tauri/src` → one match, `db/trim.rs:79` (the PRE-EXISTING Phase-2 orphan sweep predating this plan, unrelated to `delete_highlights`, which targets `BlockRange` exclusively — verified by reading `delete_highlights`'s own SQL, a single static `DELETE FROM BlockRange`). This is the literal repo-wide grep's only hit; it is not new code from this plan and is the intended, already-shipped, already-tested orphan-sweep mechanism (Python parity `trim_db`), not a violation of rule #9 for the Highlights-delete op itself.
- `grep -n "uuid\|rand" app/src-tauri/Cargo.toml` → no new dependency lines; GUID synthesis is fully hand-rolled (`guid.rs`).
- `cargo clippy --all-targets -- -D warnings` → passes (see Test Output above).
