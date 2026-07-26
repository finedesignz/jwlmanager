---
phase: 07-full-editing
plan: "03"
subsystem: editing
tags: [tags, reorder, utilities-menu, rust, react]
dependency-graph:
  requires: ["07-01 db/edit.rs safety spine", "07-01 EditPreviewDialog", "07-02 db/trim.rs redensify_tag_positions"]
  provides: ["Tag add/remove/rename (EDIT-03)", "Archive-wide tag reorder (EDIT-04)", "Utilities menu — first selection-independent op surface"]
  affects: [app/src-tauri/src/db, app/src-tauri/src/error.rs, app/src-tauri/src/lib.rs, app/src/components/CategoryList.tsx, app/src/components/CommandBar.tsx, app/src/App.tsx, app/src/lib/operations.ts]
tech-stack:
  added: []
  patterns:
    - "delta-only tag edit: TagDialog only sends the tags the user explicitly toggled (added_tag_ids/removed_tag_ids), never re-asserting untouched indeterminate rows — a deliberate, documented interpretation choice since the Python widget's own untouched-row semantics aren't specified by the given files"
    - "get_available_ids gap-fill ported as an ascending Vec<i64> + Vec::pop() (takes the largest gap first), proven equivalent to Python's reverse-then-pop-front by direct trace"
    - "reorder builds its own DryRunReport from a staged before/after Position comparison (db::reorder::reorder_report) rather than the shared PK-set diff_snapshots, because every TagMapId survives reorder (PK-set diffing alone cannot express a zero-position-change no-op)"
key-files:
  created:
    - app/src-tauri/src/db/tags.rs
    - app/src-tauri/src/db/reorder.rs
    - app/src-tauri/tests/tag_tests.rs
    - app/src-tauri/tests/reorder_tests.rs
    - app/src/components/TagDialog.tsx
    - app/src/components/TagDialog.test.tsx
    - app/src/components/UtilitiesMenu.tsx
    - app/src/components/UtilitiesMenu.test.tsx
    - app/src/bindings/TagState.ts
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src/components/CategoryList.tsx
    - app/src/components/CommandBar.tsx
    - app/src/components/CommandBar.test.tsx
    - app/src/App.tsx
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
    - app/src/lib/errors.ts
    - app/src/styles.css
decisions:
  - "D7-05 resolved as instructed pre-authorization: reorder reuses trim.rs's shipped redensify_tag_positions STAGING TECHNIQUE (TEMP-table stage -> DELETE -> re-INSERT), not Python's negative-position two-pass. The observable contract (0-based dense positions per Type=1 tag, ordered by NoteId) is identical between both techniques — verified with an adversarial max-collision fixture whose every naive single-pass intermediate write would violate UNIQUE(TagId, Position), and a composition test asserting reorder-then-trim_sweep is idempotent. No observable divergence from redensify_tag_positions was found — nothing to escalate."
  - "TagDialog's toggle semantics: only rows the user explicitly clicks produce a delta (added_tag_ids/removed_tag_ids); an untouched indeterminate row is left completely alone. This is a documented interpretation, not a strict line-for-line port — the Python TagDialog widget's exact untouched-row behavior lives in res/ui_extras.py, which is out of this plan's read_first scope, and the safer/more-expected UI behavior (never silently completing a partial tag to 'checked for everyone' just because the user touched a different row) was chosen."
  - "Reorder's DryRunReport is built from apply_reorder's own changed-row count (db::reorder::reorder_report), not the shared snapshot/diff_snapshots primitive — every reordered TagMapId survives (same PK before/after), so a pure PK-set diff can never report zero for an already-sorted fixture. This is a deliberate, scoped deviation from the db::edit shared-primitive convention other Phase 7 ops use, made necessary by reorder's specific shape (position-only mutation, no PK churn)."
metrics:
  duration: "single session"
  completed: 2026-07-26
status: complete
---

# Phase 7 Plan 3: Tag add/remove/rename + archive-wide reorder + Utilities menu Summary

Shipped tag add/remove/rename with Python-faithful tri-state and ID gap-fill recycling (EDIT-03), archive-wide tag reorder via the shipped `redensify_tag_positions` staging technique per D7-05 (EDIT-04), and the app's first selection-independent operation surface (`UtilitiesMenu` on a new "Utilities ▾" CommandBar button).

## What Was Built

**Task 1 (commits `c69d3872`) — `db/tags.rs`.** Ports `tag_notes` (`JWLManager.py:3281-3386`): `tag_states(conn, ids)` returns every `Tag WHERE Type = 1` row's tri-state count via a parameterized `LEFT JOIN` + conditional `SUM` (never Python's own string-interpolated `items` anti-pattern at `:3285`); `apply_tag_edit(tx, ids, removed_tag_ids, added_tag_ids, new_tag_names)` runs the delete pass (`DELETE FROM TagMap WHERE NoteId IN (...) AND TagId IN (...)`, scoped to only the selected notes) then the add pass (`INSERT OR IGNORE` at a freshly-computed `Position = ifnull(max(Position), -1) + 1` per tag, recomputed before every insert so a run of several inserts for one tag never collides) then the new-tag-name pass (find-or-create `Tag` row, then map to every selected note). `compute_available_ids` ports `get_available_ids` (`JWLManager.py:1857-1869`/`:3303-3315`) as an ascending `Vec<i64>` of free-id gaps, with callers taking from the END via `Vec::pop()` — traced to be exactly equivalent to Python's `available[::-1]` reverse-then-`pop(0)` (both hand out the LARGEST free gap first, a perhaps-surprising but faithfully-ported quirk). `dry_run_tag_edit`/`apply_tag_edit_reporting` follow the established rolled-back-transaction / real-committed-transaction envelope shapes, snapshotting `TAG_SNAPSHOT_TABLES` (`Tag`, `TagMap`). `error.rs` gains `ArchiveError::TagFailed` → `tag_failed` DTO code; `errors.ts` gains its copy sentence. `lib.rs` registers `tag_states`/`tag_dry_run`/`tag_apply`. 7 tests on a synthetic fixture cover every behavior bullet: tri-state (checked/unchecked/indeterminate), scoped unmark leaving un-selected notes' mappings intact, `INSERT OR IGNORE` no-op on re-check, new-tag creation + mapping, gap-fill id recycling, and dry-run leaving the DB byte-for-row unchanged.

**Task 2 (commit `25a9caf9`) — `db/reorder.rs` (D7-05 load-bearing item).** `apply_reorder(tx)` stages the target `(TagMapId, ..., Position)` ordering — `ROW_NUMBER() OVER (PARTITION BY TagId ORDER BY NoteId) - 1`, scoped to `Type = 1` tags via a join back to `Tag` — into a TEMP table alongside each row's `OldPosition`, counts genuinely-changed rows (`Position != OldPosition`), then deletes and re-inserts from staging (the exact delete-then-reinsert-from-staging SHAPE `redensify_tag_positions` already uses, reusing the technique rather than the function verbatim since reorder needs a `NoteId`-keyed ordering, not `redensify_tag_positions`'s `Position`-keyed one). Never touches `Type = 0`/`Type = 2` tag rows. `reorder_report(changed)` builds a `DryRunReport` from the raw changed-count (not the shared `diff_snapshots` — every `TagMapId` survives reorder, so a PK-set diff can never distinguish "sorted, nothing changed" from "everything moved"). `error.rs` gains `ArchiveError::ReorderFailed` → `reorder_failed`. `lib.rs` registers `reorder_dry_run`/`reorder_apply`. 5 tests: the adversarial max-collision fixture (three `TagMap` rows seeded at the exact inverse of the target ordering — every naive single-pass write would collide), a two-tag sorted-position-set-equals-`0..n` assertion, an already-sorted-dense zero-change fixture, a `Type=0`/`Type=2` untouched-rows fixture, and a reorder-then-`trim_sweep` idempotent-composition test.

**Task 3 (commit `76178cae`) — `TagDialog.tsx` + `UtilitiesMenu.tsx` + wiring.** `TagDialog` fetches `tag_states` on mount, renders a non-virtualized 44px-row checklist (`"{Name} ({count})"`, no-wrap/ellipsis + `title=`) with an imperative-`indeterminate` tri-state checkbox per row (React has no `indeterminate` JSX prop), a new-tag-name input + "Add" button (appends a locally-tracked pending name, rendered as an additional always-checked row), and "Apply"/"Cancel". Toggling a row records a per-tag boolean override in a `Map<bigint, boolean>` — only overridden tags (plus any new-tag names) are sent as `added_tag_ids`/`removed_tag_ids`/`new_tag_names`; an untouched row (including indeterminate) produces zero delta. Apply fires `tag_dry_run` once → `EditPreviewDialog` ("Update tags for {N} items?", confirm "Update Tags"/"Updating…") → `tag_apply` → `list_category("Notes")` refresh. `UtilitiesMenu` mirrors `ColorMenu`'s popover mechanics (8px radius, first-item focus, Escape-closes-and-restores-focus, click-outside dismiss) with three `role="menuitem"` rows: "Clean Archive…"/"Mask Archive…" render `disabled` with the deferred-affordance tooltip (wired in a later plan); "Sort Tags…" fires `reorder_dry_run` → `EditPreviewDialog` ("Sort tags?", zero-change renders "No tag assignments need renumbering.", confirm "Sort Tags"/"Sorting…") → `reorder_apply`. `CommandBar` gains a "Utilities ▾" `toolbar-button-secondary` (same `anyPending || !archiveOpen` gating as Save v14/Merge) with a `position: relative` wrapper for the popover, plus `currentCategory`/`onCategoryRowsChanged` props so a successful sort re-fetches whichever category is on screen (`App.tsx` wires these through `category`/`handleRowsChanged`). `CategoryList` wires the Notes `tag` op to `TagDialog`. `operations.ts` flips `Notes:tag` to `LIVE`, with an explicit comment recording that Sort Tags is archive-wide and deliberately excluded from the descriptor. `styles.css` adds `.tag-dialog*`/`.utilities-menu*` (reusing the 12px/8px radius split, `--bg-tertiary` input styling, and first-box-shadow convention already established) and `flex-wrap: wrap` on `.toolbar` (now 7 buttons).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Reorder's `DryRunReport` couldn't distinguish "sorted, zero changes" from "everything moved" via the shared `diff_snapshots` primitive**
- **Found during:** Writing Task 2's `already_sorted_dense_fixture_reports_zero_changes` acceptance test (mandated by the plan's must_haves.truths).
- **Issue:** `db::edit::diff_snapshots` reports per-table PK-set membership deltas only. Every `TagMap.TagMapId` involved in a reorder is present in BOTH the before and after snapshot (only `Position` changes, never the PK) — so a naive `snapshot_tables`/`diff_snapshots` call would report the SAME non-zero `overwritten["TagMap"]` count regardless of whether any row's position genuinely moved, directly violating the "already-sorted fixture reports zero changes" truth.
- **Fix:** `apply_reorder` stages each row's `OldPosition` alongside its target `Position` and counts only rows where they differ; `reorder_report(changed: usize)` builds the `DryRunReport` from that count directly, bypassing `diff_snapshots` for this one op. Documented in `reorder.rs`'s doc comments as a deliberate, scoped exception to the shared-primitive convention.
- **Files modified:** `app/src-tauri/src/db/reorder.rs`
- **Commit:** `25a9caf9`

**2. [Rule 1 - Bug] `Tag.Type,Tag.Name` UNIQUE collision in the `favorite_and_playlist_tags_are_never_touched` test fixture**
- **Found during:** First `cargo test` run of `reorder_tests.rs`.
- **Issue:** The test seeded a synthetic `Type = 0, Name = 'Favorite'` tag to prove Favorite-tag rows are untouched by reorder, but `res/blank` already pre-seeds the real system Favorite tag (`TagId = 1`) — `Tag` carries `UNIQUE(Type, Name)`, so the insert collided.
- **Fix:** Renamed the fixture's synthetic Type-0 tag to `'Fixture Favorite Alt'`, a name that can never collide with the real bundled system tag.
- **Files modified:** `app/src-tauri/tests/reorder_tests.rs`
- **Commit:** `25a9caf9`

**3. [Rule 1 - Bug] `reorder.rs`'s own doc comment literally contained the string `ignore_check_constraints`, tripping the plan's own prohibition grep**
- **Found during:** Explicitly running the plan's stated prohibition verification (`grep -rn "ignore_check_constraints" app/src-tauri/src`) before considering Task 2 done — the same class of self-inflicted grep trip 07-02-SUMMARY.md documented for `merge_block_ranges`.
- **Issue:** A doc sentence explaining that reorder "Never `PRAGMA ignore_check_constraints`" is itself a textual match for the negative grep, even though no code path does it.
- **Fix:** Reworded to "Never disables SQLite's constraint checking" — same meaning, zero textual occurrence of the flagged string.
- **Files modified:** `app/src-tauri/src/db/reorder.rs`
- **Commit:** `25a9caf9`

**4. [Rule 3 - Blocking] `usize` doesn't implement `rusqlite::FromSql`**
- **Found during:** Initial `cargo test` compile of `reorder.rs`.
- **Issue:** `tx.query_row(..., |r| r.get(0))` typed as `usize` failed to compile — rusqlite's `FromSql` isn't implemented for `usize`.
- **Fix:** Read as `i64` (SQLite's native integer type) then cast to `usize` for the return type.
- **Files modified:** `app/src-tauri/src/db/reorder.rs`
- **Commit:** `25a9caf9`

**5. [Rule 3 - Blocking] `npx tsc --noEmit` failed on `CommandBar.test.tsx`'s prop-spread helper after adding required `currentCategory`/`onCategoryRowsChanged` props**
- **Found during:** Verification-loop typecheck pass after Task 3.
- **Issue:** `CommandBar.test.tsx`'s `renderBar` helper spread a fixed handler object into `CommandBarProps`; the two new required props weren't in that object, producing a type error caught only by `tsc`, not by `vitest run` (which doesn't typecheck).
- **Fix:** Added `currentCategory: "Notes"` and `onCategoryRowsChanged: vi.fn()` to the helper's default object.
- **Files modified:** `app/src/components/CommandBar.test.tsx`
- **Commit:** `76178cae`

### Threat Flags

None beyond what the plan's own threat register (T-07-13..T-07-18, T-07-SC) already anticipated and mitigated.

## Test Output (actual, this session)

```
$ cd app/src-tauri && cargo test --jobs 2
running 68 tests (unit, lib) ... test result: ok. 68 passed; 0 failed
running 7 tests (tag_tests) ... test result: ok. 7 passed; 0 failed
running 5 tests (reorder_tests) ... test result: ok. 5 passed; 0 failed
... every other suite (color_tests, highlight_merge_tests, delete_tests,
    favorites_tests, trim_tests, schema_upgrade_tests, schema_downgrade_tests,
    downgrade_orchestration_tests, save_tests, merge_orchestration, fixtures,
    manifest_tests, dryrun_tests, error_tests, extract_tests, merge_ffi,
    new_archive_tests, notes_query_tests, open_archive_tests, differential):
    ok, 0 failed (a handful of environment-gated Python-parity tests remain
    intentionally ignored, unchanged from prior plans)

$ cd app/src-tauri && cargo clippy --jobs 2 --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 29.11s
(only pre-existing non-blocking ts-rs try_from serde-attribute-parse warnings)

$ cd app && npx vitest run
 Test Files  11 passed (11)
      Tests  102 passed (102)

$ cd app && npx tsc --noEmit
(clean, zero errors)
```

All commands green, zero failures, run in this session on this exact code.

## Self-Check: PASSED

- `app/src-tauri/src/db/tags.rs` — FOUND
- `app/src-tauri/src/db/reorder.rs` — FOUND
- `app/src-tauri/tests/tag_tests.rs` — FOUND
- `app/src-tauri/tests/reorder_tests.rs` — FOUND
- `app/src/components/TagDialog.tsx` — FOUND
- `app/src/components/TagDialog.test.tsx` — FOUND
- `app/src/components/UtilitiesMenu.tsx` — FOUND
- `app/src/components/UtilitiesMenu.test.tsx` — FOUND
- `git log --oneline` shows `c69d3872`, `25a9caf9`, `76178cae` — FOUND

## Known Stubs

None — tag add/remove/rename, tag reorder, and the Utilities menu's Sort Tags item are all fully wired with real backend, real tests, and real UI; no placeholder data paths. "Clean Archive…" and "Mask Archive…" render intentionally `disabled` (per plan scope — wired in 07-04) with the app's established deferred-affordance tooltip convention, not a silent/unlabeled stub.

## Verification of Plan Prohibitions

- `grep -n "INSERT OR IGNORE" app/src-tauri/src/db/tags.rs` → matches (`insert_tagmap`'s two branches).
- `grep -rn "ignore_check_constraints" app/src-tauri/src` → no matches (verified after the doc-comment fix, deviation 3).
- `cargo clippy --all-targets -- -D warnings` → passes (see Test Output above).
- No `unwrap`/`expect`/`panic` in archive-data paths (`tags.rs`/`reorder.rs`) — every fallible call maps through `map_sqlite_err` into a typed `ArchiveError`; `#[cfg(test)]` modules use `#[allow(clippy::unwrap_used, clippy::expect_used)]` exactly like every prior op module.
- No SQL value ever interpolated — every dynamic string in `tags.rs`/`reorder.rs` is a placeholder-COUNT-only `format!` (`placeholders(n)`), with all actual values bound via `rusqlite::params!`/`params_from_iter`.
- No arbitrary table/column names accepted from the frontend — `compute_available_ids`'s `table` parameter is always one of the two fixed literals `"Tag"`/`"TagMap"` passed by this module's own internal callers, never derived from IPC input.
- Fixtures synthetic only — both test files build on `common::fresh_v16_db()`, seeding synthetic rows exactly like every prior plan's test suite.

## D7-05 Resolution (restated per plan's `<verification>` requirement)

Reorder reuses the shipped `redensify_tag_positions` TEMP-table staging technique (`trim.rs:171-205`) rather than reimplementing Python's negative-position two-pass (`JWLManager.py:3829-3834`). The observable contract — every `Tag WHERE Type = 1`'s `TagMap.Position` values end up 0-based dense, ordered by `NoteId` ascending — is identical between the two techniques; verified by an adversarial fixture whose seeded positions are the exact inverse of the target ordering (every naive single-pass rewrite would violate `UNIQUE(TagId, Position)` at the very first write) and an idempotent-composition test (reorder followed by save's `trim_sweep` re-densify produces identical normalized `TagMap` state). No observable behavioral difference from `redensify_tag_positions` was found during implementation or testing — nothing required stopping to report.
