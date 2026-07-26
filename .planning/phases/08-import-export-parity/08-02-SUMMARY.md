---
phase: 08-import-export-parity
plan: 02
subsystem: import-export-io
tags: [wire-format, bookmarks, annotations, id-recycling, tauri-commands]
dependency-graph:
  requires:
    - db/io module tree (header/export/import) — 08-01
    - db/ids archive-wide id-gap recycler — 08-01
    - DryRunReport.skipped field — 08-01
  provides:
    - export_bookmarks / import_bookmarks_dry_run / import_bookmarks_apply commands
    - export_annotations / import_annotations_dry_run / import_annotations_apply commands
    - db::edit::BOOKMARK_SNAPSHOT_TABLES / ANNOTATION_SNAPSHOT_TABLES
  affects:
    - app/src/components/CategoryList.tsx (EXPORT_COMMANDS/IMPORT_COMMANDS maps)
    - app/src/lib/operations.ts (LIVE set)
tech-stack:
  added: []
  patterns:
    - "Boundary-scan record parsing (no regex lookahead available in Rust's regex crate) for Annotations' bracket-tag records"
    - "Three separate, never-collapsed location-dedup predicates for Bookmarks (scripture / publication / bookmark-container)"
    - "Per-category narrow SNAPSHOT_TABLES constants so diff_snapshots' `overwritten` count stays meaningful for an UPDATE-in-place upsert"
key-files:
  created:
    - app/src-tauri/tests/fixtures/wire/bookmarks_golden.txt
    - app/src-tauri/tests/fixtures/wire/annotations_golden.txt
  modified:
    - app/src-tauri/src/db/edit.rs
    - app/src-tauri/src/db/io/export.rs
    - app/src-tauri/src/db/io/import.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/export_wireformat_tests.rs
    - app/src-tauri/tests/import_wireformat_tests.rs
    - app/src-tauri/tests/import_failfast_tests.rs
    - app/src/components/CategoryList.tsx
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
decisions:
  - "Annotations' `ImportMalformed.line` field is reused to carry a 1-indexed RECORD number rather than a source line number — bracket-tag records have no single meaningful source line, and reusing the existing DTO field avoided an architectural change to `ArchiveError`."
  - "Annotations' DOC bracket rendering: `str(DocumentId)` renders the literal 'None' when NULL (matching Python's `str()`-wrapped DOC), while PUB (KeySymbol) is rendered raw/unwrapped like Python — a NULL KeySymbol would crash Python's own exporter (a TypeError on string concat); Rust renders an empty string instead of reproducing that crash, a documented harmless strengthening for an orphaned-InputField edge case that never occurs in valid archives."
  - "Bookmark/Annotation location-dedup helpers are three (Bookmarks) and one (Annotations) separate functions, never collapsed into a generic 'find or insert Location' helper, per D8-04's explicit prohibition."
  - "apply_import_bookmarks/apply_import_annotations return Result<(), ArchiveError> rather than a skip-count like apply_import_favorites — neither category has a Favorites-style string-level dup-skip; a re-import lands via `diff_snapshots`' intersection-based `overwritten` count naturally, since both upserts UPDATE the existing PK in place rather than delete+reinsert."
metrics:
  duration: "~1 session"
  completed: 2026-07-26
status: complete
---

# Phase 8 Plan 2: Bookmarks + Annotations Export/Import Summary

Extends the 08-01 export/import spine to the two remaining non-range-merge `.txt` categories, covering both wire-format shapes (flat pipe rows vs. bracket-tag records) and both `{END}`-sentinel states before Highlights/Notes (which add range-merge risk) follow in later plans.

## What was built

**`db/io/export.rs`** — `export_bookmarks` runs the exact 12-column Bookmarks SQL (`REPLACE(...,"|","¦")` performed IN SQL, matching Python's layer), writes no `{END}` sentinel (`BOOKMARKS_WRITES_END_SENTINEL = false`). `export_annotations` runs the exact `ORDER BY doc, i` SQL, writes each record as `\n==={PUB=…}[{ISSUE=…}]{DOC=…}{LABEL=…}===\n<Value>`, terminating with the literal `\n==={END}===` and no trailing newline (`ANNOTATIONS_WRITES_END_SENTINEL = true`, the counterpart asymmetry to Bookmarks).

**`db/io/import.rs`** — `parse_bookmarks_file` splits on `|`, requires exactly 12 fields, unwraps the `'None'` sentinel ONLY at Python's five specific indices (BookNumber/ChapterNumber/DocumentId/Snippet/BlockIdentifier), never touching the `¦` escaping. `apply_import_bookmarks` ports the THREE distinct location-dedup predicates as three separate functions (scripture: KeySymbol+MepsLanguage+BookNumber+ChapterNumber; publication: KeySymbol+MepsLanguage+IssueTagNumber+DocumentId+Type; bookmark-container: KeySymbol+MepsLanguage+Type=1) and upserts the Bookmark on `(PublicationLocationId, Slot)`.

`parse_annotations_file` finds record boundaries via an explicit forward scan for the literal `\n===\{` sequence (Rust's `regex` crate has no lookahead, unlike Python's `(?=\n==={)`), correctly treating the `{END}` block as a terminator that is never itself parsed as a data record. `apply_import_annotations` dedups the Location on `DocumentId+IssueTagNumber(fill-null-to-0)+KeySymbol+MepsLanguage IS NULL+Type=0` and upserts `InputField` via `ON CONFLICT(LocationId, TextTag) DO UPDATE`.

**`db/edit.rs`** — Adds `BOOKMARK_SNAPSHOT_TABLES` (`Location`, `Bookmark`) and `ANNOTATION_SNAPSHOT_TABLES` (`Location`, `InputField` via `rowid`) — narrow per-op sets (07-01 precedent) so `diff_snapshots`' `overwritten` count reflects only these categories' own rows.

**Tauri commands**: `export_bookmarks`, `import_bookmarks_dry_run`/`import_bookmarks_apply`, `export_annotations`, `import_annotations_dry_run`/`import_annotations_apply` — same shape as 08-01's Favorites triple, all registered in `generate_handler![]`.

**Frontend**: `operations.ts` flips `Bookmarks:export/import` and `Annotations:export/import` LIVE. `CategoryList.tsx`'s `EXPORT_COMMANDS`/`IMPORT_COMMANDS` maps gain both categories — the render/dispatch logic itself is fully generic and needed no changes.

## Deviations from Plan

### Process deviation

**Per-task commit granularity not achieved.** The plan calls for one commit per task; because both Bookmarks (Task 1) and Annotations (Task 2) additions landed in the same shared files (`export.rs`, `import.rs`, `lib.rs`, and all three test files) via single multi-hunk edits, the work landed as one combined commit (`ef12a7d8`) rather than two. No functional deviation — both tasks are fully implemented and independently verifiable via their own test names.

### Auto-fixed Issues

**1. [Rule 1 - Bug] Initial Bookmark fixture rows violated the `BlockType` NOT NULL / CHECK constraint**
- **Found during:** Task 1 test authoring — several hand-authored `BOOKMARKS` import fixture lines used the literal string `None` for `BlockType` (index 10, which Python does NOT unwrap to NULL), producing a text value that failed `Bookmark`'s `NOT NULL`/CHECK constraint on insert.
- **Fix:** Corrected fixture lines to use `0` for `BlockType` (a real, unwrapped value) with `None`→NULL only at `BlockIdentifier` (index 11, which IS unwrapped) — matching the CHECK's `(BlockType = 0 AND BlockIdentifier IS NULL)` branch.
- **Files modified:** `app/src-tauri/tests/export_wireformat_tests.rs`, `app/src-tauri/tests/import_wireformat_tests.rs`.

**2. [Rule 1 - Bug] Annotations import test fixtures used an unrealistic `DOC=None`/no-Track Location shape**
- **Found during:** Task 2 test authoring — `find_or_insert_annotation_location`'s INSERT (ported verbatim from Python's `add_location`, which also omits `Track`) violated `Location`'s `Type=0` CHECK when `DocumentId` was NULL and no `Track`/Book+Chapter branch was satisfied. This is a genuine Python-shared edge case (a `DOC=None` annotation import would ALSO violate Python's own schema) — real annotations always attach to a `DocumentId`-bearing publication.
- **Fix:** Changed import-test fixtures to use a realistic `DOC=1001` (non-NULL `DocumentId`), which satisfies the CHECK's first branch without needing `Track`. No production code changed — this was purely a test-fixture realism fix.
- **Files modified:** `app/src-tauri/tests/import_wireformat_tests.rs`.

## Test output (actual, run at completion)

```
cd app/src-tauri && cargo test --jobs 2
  → ALL binaries: test result: ok (0 failed across every suite), including:
    export_wireformat_tests: 12 passed (was 4 before this plan)
    import_wireformat_tests: 11 passed (was 5 before this plan)
    import_failfast_tests: 9 passed (was 4 before this plan)

cd app/src-tauri && cargo clippy --all-targets -- -D warnings
  → clean (only the pre-existing ts-rs try_from attribute-parse warning,
    unrelated to this plan, present since 08-01)

cd app && npx tsc --noEmit
  → clean, zero errors

cd app && npx vitest run
  → Test Files  12 passed (12)
    Tests  123 passed (123)
    (operations.test.ts updated: 2 pre-existing assertions that hardcoded
     the old LIVE set now include Bookmarks/Annotations export+import)

git diff --stat app/src-tauri/Cargo.toml app/package.json
  → (empty — no dependency additions, PD/prohibition satisfied)

grep -n "generate_handler" -A 70 app/src-tauri/src/lib.rs
  → lists export_bookmarks, import_bookmarks_dry_run, import_bookmarks_apply,
    export_annotations, import_annotations_dry_run, import_annotations_apply
```

## Known Stubs

None — Bookmarks and Annotations export/import are fully live end to end (op bar → dialog → command → disk/DB → refresh), no stubbed data paths.

## Self-Check: PASSED

- FOUND: app/src-tauri/tests/fixtures/wire/bookmarks_golden.txt
- FOUND: app/src-tauri/tests/fixtures/wire/annotations_golden.txt
- FOUND: `export_bookmarks`/`export_annotations`/`import_bookmarks_dry_run`/`import_bookmarks_apply`/`import_annotations_dry_run`/`import_annotations_apply` in `app/src-tauri/src/lib.rs` generate_handler![]
- FOUND commit ef12a7d8 (Bookmarks + Annotations export/import wire-format parity)
