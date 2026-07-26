---
phase: 08-import-export-parity
plan: 03
subsystem: import-export-io
tags: [wire-format, highlights, range-merge, id-recycling, tauri-commands]
dependency-graph:
  requires:
    - db/io module tree (header/export/import) — 08-01
    - db/ids archive-wide id-gap recycler — 08-01
    - db::highlights::merge_block_ranges (Phase 7's range union-merge primitive) — 07-02
  provides:
    - export_highlights / import_highlights_dry_run / import_highlights_apply commands
    - db::io::usermark (synthesize_usermark / merge_range_into) — the shared import-side
      UserMark synthesis + range-merge call site Notes import (08-04) will reuse unchanged
    - db::highlights::merge_block_ranges's recycled_id parameter (id-recycling support)
    - db::edit::HIGHLIGHT_SNAPSHOT_TABLES
  affects:
    - app/src/components/CategoryList.tsx (EXPORT_COMMANDS/IMPORT_COMMANDS maps)
    - app/src/lib/operations.ts (LIVE set)
tech-stack:
  added: []
  patterns:
    - "Import-side UserMark synthesis + range-merge extracted to its own shared module (db/io/usermark.rs) so Notes import (08-04) reuses it unchanged rather than re-deriving the merge call"
    - "merge_block_ranges gained an id-recycling parameter (D8-08) without touching its geometry — the single merge implementation stays singular"
    - "Blanket 'None'->'' string replacement (not a per-field None-check) ported verbatim for Highlights, producing raw String fields instead of Option<String> for the Location-predicate columns"
key-files:
  created:
    - app/src-tauri/src/db/io/usermark.rs
    - app/src-tauri/tests/fixtures/wire/highlights_golden.txt
    - app/src-tauri/tests/import_range_merge_tests.rs
  modified:
    - app/src-tauri/src/db/highlights.rs
    - app/src-tauri/src/db/io/export.rs
    - app/src-tauri/src/db/io/import.rs
    - app/src-tauri/src/db/io/mod.rs
    - app/src-tauri/src/db/edit.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/export_wireformat_tests.rs
    - app/src-tauri/tests/import_wireformat_tests.rs
    - app/src-tauri/tests/import_failfast_tests.rs
    - app/src-tauri/tests/highlight_merge_tests.rs
    - app/src-tauri/tests/edit_roundtrip_tests.rs
    - app/src/components/CategoryList.tsx
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
    - app/src/App.test.tsx
decisions:
  - "merge_block_ranges gained a new trailing `recycled_id: Option<i64>` parameter (Rule 2 auto-fix, not in the plan's files_modified list) — the shipped Phase 7 primitive had no id-recycling support because it had no production caller yet; Highlights import is IO-03/D8-08's first caller requiring recycled BlockRange ids, and the ONLY way to satisfy 'usermark.rs calls the shipped merge_block_ranges — a second implementation would fork the merge' while also satisfying D8-08 was to extend the one primitive's INSERT target, not its geometry. plan_merge (the pure overlap/absorb function) is completely untouched. The two pre-existing call sites (highlight_merge_tests.rs, edit_roundtrip_tests.rs) were updated to pass `None`, preserving their prior autoincrement behavior exactly."
  - "Highlights' six numeric fields (BlockType/Identifier/StartToken/EndToken/ColorIndex/Version) are parsed to typed i64 at parse time, not kept as raw strings like Favorites/Bookmarks/Annotations — `synthesize_usermark`/`merge_range_into`/`merge_block_ranges` all require typed i64 parameters (the merge arithmetic and grouping keys are genuinely numeric operations, not just SQL binds), and the schema declares all six NOT NULL on the UserMark/BlockRange side, so a genuine file never legitimately carries anything else there. An unparseable value (including i64 overflow) is ImportMalformed, matching Python's own bare `except`+ROLLBACK around `int(attribs[2])`/`int(attribs[3])`."
  - "The seven Location-predicate fields (BookNumber/ChapterNumber/DocumentId/IssueTagNumber/KeySymbol/MepsLanguage/Type) stay raw String (never Option<String>) because parse_highlights_file ports Python's BLANKET 'None'->'' replacement verbatim (RESEARCH assumption A5) rather than a per-field None-check — an actual NULL renders as an empty string, not Option::None, reproducing the exact fragile-but-intentional behavior including the 'None'-substring-corruption edge case."
  - "Highlights' scripture/publication Location dedup predicates are two NEW functions (find_or_insert_highlight_scripture_location / find_or_insert_highlight_publication_location), never sharing code with Bookmarks' identically-shaped scripture predicate from 08-02 — each category ports its own Python function 1:1, per the established D8-04 never-collapse convention."
metrics:
  duration: "~1 session"
  completed: 2026-07-26
status: complete
---

# Phase 8 Plan 3: Highlights Export/Import + Range-Merge Call Site Summary

Ships Highlights `.txt` export/import and puts the Phase 7 `merge_block_ranges` range union-merge primitive into production for the first time, via a new shared `db/io/usermark.rs` module that Notes import (08-04) will reuse unchanged.

## What was built

**`db/io/export.rs`** — `export_highlights` runs the exact 13-column Highlights SQL (`UserMark JOIN Location JOIN BlockRange`), no `{END}` sentinel (`HIGHLIGHTS_WRITES_END_SENTINEL = false`), reusing `join_row`/`build_export_header` from the shared spine, selection typed over `db::color::NonEmptyBlockRangeIds` (the same wrapper Highlights recolor/delete already share).

**`db/io/usermark.rs`** (new) — the shared import-side UserMark-synthesis + range-merge call site:
- `synthesize_usermark` always inserts a FRESH `UserMark` row (`StyleIndex = 0` fixed, `format_guid_v4`-generated GUID, `take_id`-recycled id before autoincrement) — never looks up/reuses an existing UserMark, matching Python's `add_usermark` exactly (the source of the accepted non-idempotency).
- `merge_range_into` delegates the geometry entirely to `db::highlights::merge_block_ranges`, recycling a `BlockRange` id via `take_id` first and threading it through the primitive's new `recycled_id` parameter.

**`db/highlights.rs`** — `merge_block_ranges` gained a trailing `recycled_id: Option<i64>` parameter (see Deviations) so the merged row's `BlockRangeId` can come from the recycled-gap pool (D8-08) instead of always autoincrementing; `plan_merge`'s geometry is untouched.

**`db/io/import.rs`** — `parse_highlights_file` applies the `^(\d+\|){6}` line-shape guard (implemented as an explicit digit/pipe scan, no regex crate needed) to select data lines without any line-count offset bookkeeping, then applies Python's blanket `'None'`->`''` string replacement BEFORE splitting into exactly 13 fields (verbatim, not "fixed" into a per-field check — RESEARCH A5), parsing the six numeric fields to `i64` and leaving the seven Location-predicate fields as raw `String`. `apply_import_highlights` resolves the scripture-or-publication Location (Python's own `if attribs[6]:` truthiness check on BookNumber), synthesizes a fresh UserMark, then merges the range — in FILE order, threading one `guid_seed` XORed per-record-index through the whole call (mirrors `db::color::apply_color`'s `guid_seed ^ note_id` pattern). `dry_run_import_highlights` follows the established never-committed-transaction shape over the new `HIGHLIGHTS_SNAPSHOT_TABLES`.

**`db/edit.rs`** — Adds `HIGHLIGHT_SNAPSHOT_TABLES` (`Location`, `UserMark`, `BlockRange`).

**Tauri commands**: `export_highlights`, `import_highlights_dry_run`/`import_highlights_apply` — same shape as the prior categories, registered in `generate_handler![]`, threading a wall-clock `guid_seed_now()` through for the UserMark GUIDs.

**Frontend**: `operations.ts` flips `Highlights:export`/`Highlights:import` LIVE. `CategoryList.tsx`'s `EXPORT_COMMANDS`/`IMPORT_COMMANDS` maps gain Highlights — render/dispatch logic needed no changes.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] `merge_block_ranges` had no id-recycling support**
- **Found during:** Task 2 — implementing `merge_range_into`, which the plan's `key_links` require to call the shipped `db::highlights::merge_block_ranges` (prohibiting a second merge implementation).
- **Issue:** The Phase 7 primitive's merged-row INSERT was always plain autoincrement — it had no production caller yet in Phase 7 (recolor deliberately never calls it), so id recycling for the new `BlockRange` row was never added. Highlights import is IO-03/D8-08's first caller requiring recycled ids for ALL new Location/UserMark/BlockRange rows, including the merged range.
- **Fix:** Added a trailing `recycled_id: Option<i64>` parameter to `merge_block_ranges` — `Some(id)` INSERTs with that explicit id (a gap the caller already popped via `take_id`), `None` falls back to the original autoincrement path. `plan_merge`'s geometry (the actual overlap/absorb/union algorithm) is completely untouched — this is purely an INSERT-target change, so the primitive remains the single merge implementation the prohibitions require.
- **Files modified:** `app/src-tauri/src/db/highlights.rs` (signature + insert branch), plus the two pre-existing call sites updated to pass `None`: `app/src-tauri/tests/highlight_merge_tests.rs`, `app/src-tauri/tests/edit_roundtrip_tests.rs`.
- **Commit:** included in this plan's Task 2 commit.

### Process deviation

**Per-task commit granularity not fully achieved for the reason above.** The `merge_block_ranges` signature change is architecturally required by Task 2 but lives in a file the plan's frontmatter did not list under `files_modified` for this plan (it's Phase 7's file). It is included in Task 2's commit rather than split out, since it has no independent meaning without `usermark.rs`'s new caller.

## Test output (actual, run at completion)

```
cd app/src-tauri && cargo test --jobs 2
  → ALL binaries: test result: ok (0 failed across every suite, 104 lib tests + all
    integration suites), including:
    export_wireformat_tests: 17 passed (was 12 before this plan)
    import_wireformat_tests: (Bookmarks/Annotations/Favorites suite) + 6 new Highlights tests
    import_failfast_tests: 15 passed (was 9 before this plan)
    import_range_merge_tests: 6 passed (new file — overlap, cross-color, chain-merge,
      disjoint-no-merge, re-import convergence, id-recycling)
    highlight_merge_tests: 2 passed (updated call sites, unchanged assertions)
    edit_roundtrip_tests: 8 passed (updated call site, unchanged assertions)

cd app/src-tauri && cargo clippy --all-targets -- -D warnings
  → clean (only the pre-existing ts-rs try_from attribute-parse warning, unrelated to
    this plan, present since 08-01)

cd app && npx tsc --noEmit
  → clean, zero errors

cd app && npx vitest run
  → Test Files  12 passed (12)
    Tests  124 passed (124)
    (App.test.tsx: one pre-existing assertion that asserted Highlights:export stayed
     deferred was updated — that assertion's actual load-bearing claim, "the op-bar
     renders a deferred branch for whatever isn't live," is preserved by moving it to
     Notes:export, which is still deferred; operations.test.ts's hardcoded LIVE_PAIRS
     list gained Highlights:export/import, plus one new dedicated test)

git diff --stat app/src-tauri/Cargo.toml app/package.json
  → (empty — no dependency additions, PD/prohibition satisfied)

grep -n "generate_handler" -A 90 app/src-tauri/src/lib.rs
  → lists export_highlights, import_highlights_dry_run, import_highlights_apply

grep -n "merge_block_ranges" app/src-tauri/src/db/io/usermark.rs
  → matches (the single call site this plan's prohibition requires)
```

## Known Stubs

None — Highlights export/import is fully live end to end (op bar → dialog → command → disk/DB → refresh), no stubbed data paths.

## Self-Check: PASSED

- FOUND: app/src-tauri/src/db/io/usermark.rs
- FOUND: app/src-tauri/tests/fixtures/wire/highlights_golden.txt
- FOUND: app/src-tauri/tests/import_range_merge_tests.rs
- FOUND: `export_highlights`/`import_highlights_dry_run`/`import_highlights_apply` in `app/src-tauri/src/lib.rs` generate_handler![]
- FOUND: `merge_block_ranges` call in `app/src-tauri/src/db/io/usermark.rs`
- All test suites green (see Test output above); no commit hash recorded in this summary body since the executor commits after writing this file per the standard task_commit_protocol — see git log for `08-03` commits.
