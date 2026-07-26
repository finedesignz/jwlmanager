---
phase: 09-incremental-export
plan: 02
subsystem: db/io (export diff engine, three flat categories)
tags: [incremental-export, favorites, bookmarks, highlights, sha256-diff, tauri-command]
dependency:
  requires: ["09-01"]
  provides:
    - db::io::diff::{favorites_identity, bookmarks_identity, highlights_identity, split_prior_lines}
    - db::io::diff::{export_favorites_incremental, export_bookmarks_incremental, export_highlights_incremental}
    - db::io::export::{read_favorite_id_lines, read_bookmark_id_lines, read_highlight_id_lines}
    - export_favorites_incremental / export_bookmarks_incremental / export_highlights_incremental Tauri commands
  affects:
    - app/src-tauri/src/db/io/export.rs (read_<cat>_lines now thin projections over read_<cat>_id_lines)
tech-stack:
  added: []
  patterns:
    - "id-carrying read path: one SQL column list per category, a leading PK column, read_<cat>_lines projects it away"
    - "flat-category identity key: pipe-split + select fixed indices + join with the diff engine's unit separator; falls back to the whole line on an unexpected field count"
key-files:
  created:
    - app/src-tauri/tests/fixtures/wire/favorites_prior.txt
    - app/src-tauri/tests/fixtures/wire/bookmarks_prior.txt
    - app/src-tauri/tests/fixtures/wire/highlights_prior.txt
  modified:
    - app/src-tauri/src/db/io/diff.rs
    - app/src-tauri/src/db/io/export.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/incremental_export_tests.rs
decisions:
  - "Identity keys are built by a single generic build_flat_identity_key(line, indices, expected_field_count) helper shared by all three categories, rather than three hand-rolled splitters — a mismatched field count falls back to the WHOLE line as the key (T-09-07), never indexes out of bounds."
  - "split_prior_lines uses the same 'a line containing a pipe is a data line' filter parse_favorites_file/parse_bookmarks_file/parse_highlights_file already apply, rather than re-deriving each category's own stricter per-line shape check (e.g. Highlights' 6-leading-digit-groups regex) — sufficient because the file was already validated by the matching parse_<category>_file gate before split_prior_lines ever runs on it."
  - "Favorites' 'never reports modified' structural property (no mutable wire field) is asserted directly: favorites_never_reports_modified simulates the only possible archive-side change (repointing the TagMap at an entirely different Location) and asserts modified stays 0 even then."
metrics:
  duration: "~1 session"
  completed: "2026-07-26"
status: complete
---

# Phase 9 Plan 2: Favorites/Bookmarks/Highlights Incremental Export Summary

Extended the Notes incremental-export diff engine (09-01) to the three flat pipe-delimited categories: `export_favorites_incremental`, `export_bookmarks_incremental`, `export_highlights_incremental`, each backed by a per-category identity key and a single-source-of-truth `read_<cat>_id_lines` read path.

## What was built

**`db/io/export.rs`** (Task 1): `read_favorite_id_lines`, `read_bookmark_id_lines`, `read_highlight_id_lines` each run the SAME query the shipped `read_<cat>_lines` used, with the category's primary key (`TagMapId`, `BookmarkId`, `b.BlockRangeId`) added as a leading SELECT column. The existing `read_<cat>_lines` functions were rewritten as thin projections over these new functions, dropping the id — so exactly one SQL column list exists per category and the diff engine's live-side text can never drift from what `export_favorites`/`export_bookmarks`/`export_highlights` actually write. `export_wireformat_tests.rs`'s 23 golden-fixture tests stayed green with unedited fixtures, proving zero byte-output change.

**`db/io/diff.rs`** (Task 1): `build_flat_identity_key` (a generic pipe-split + fixed-index-join helper, falling back to the whole line on a field-count mismatch) backs `favorites_identity` (all 6 fields), `bookmarks_identity` (fields 0-7, excluding Title/Snippet/BlockType/BlockIdentifier), and `highlights_identity` (fields 0-3 + 6-12, excluding ColorIndex/Version) — matching the plan's `<identity_key_specification>` exactly. `split_prior_lines` extracts a flat category's data lines from a CRLF-normalized prior file using the same "line contains `|`" filter the shipped parsers apply.

**`export_favorites_incremental` / `export_bookmarks_incremental` / `export_highlights_incremental`** (Task 2, `diff.rs` + thin `lib.rs` command wrappers): each mirrors `export_notes_incremental`'s two-layer shape — the prior text is run through its category's `parse_<category>_file` FIRST as a fail-fast validation gate (a malformed prior file returns `ImportMalformed` before any output is written), the exported id set is decided PURELY by hash-set membership over the raw wire line text (never the identity key), and `diff_records` runs separately, keyed by the identity function, only to label the summary's added/modified counts. An empty exported selection still writes a valid header-only file via the same `-1`-sentinel-id pattern 09-01 established (no `NonEmpty*Ids` variant needs a new "empty" case). Registered all three as Tauri commands in `lib.rs` and `generate_handler!`.

**`incremental_export_tests.rs`** (Task 2): 22 tests total (7 pre-existing Notes tests + 15 new) covering, per category: no-change (zero counts, valid empty output file), no-prior-file (byte-identical to a full export, D9-05), malformed-prior-file (typed error, no output written); plus Bookmarks' Title-change→modified, Highlights' ColorIndex-change→modified, Favorites' added/removed/never-modified, and `highlights_incremental_converges` (export→re-import→export again against the same prior converges to zero, explicitly proving Phase 8's accepted `UserMark`-row-growth-on-reimport property never surfaces as a false modified count, since `UserMarkId` is not on the Highlights wire).

## Deviations from Plan

None — plan executed as written.

## Test output (actual)

```
cargo test --jobs 2 db::io::diff --lib
  test result: ok. 16 passed; 0 failed

cargo test --jobs 2 --test export_wireformat_tests
  test result: ok. 23 passed; 0 failed   (golden fixtures unchanged)

cargo test --jobs 2 --test incremental_export_tests
  test result: ok. 22 passed; 0 failed
  (7 pre-existing Notes tests + 15 new: favorites_no_change_*,
   favorites_added_reports_one_added, favorites_removed_reports_deleted_candidate_never_modified,
   favorites_never_reports_modified, favorites_no_prior_file_exports_all,
   favorites_malformed_prior_file_aborts, bookmarks_no_change_*,
   bookmarks_title_change_reports_one_modified, bookmarks_no_prior_file_exports_all,
   bookmarks_malformed_prior_file_aborts, highlights_no_change_*,
   highlights_colorindex_change_reports_one_modified, highlights_no_prior_file_exports_all,
   highlights_malformed_prior_file_aborts, highlights_incremental_converges)

cargo test --jobs 2   (full suite, 43 test binaries)
  all binaries: 0 failed (146 lib unit tests + hundreds more across integration binaries)

cargo clippy --all-targets -- -D warnings
  clean

cd app && npx tsc --noEmit
  clean (no output)

cd app && npx vitest run
  Test Files  13 passed (13) | Tests  139 passed (139)
```

## Self-Check: PASSED

- `app/src-tauri/src/db/io/diff.rs` — FOUND (modified, +identity keys, +3 incremental functions)
- `app/src-tauri/src/db/io/export.rs` — FOUND (modified, +3 id-carrying read paths)
- `app/src-tauri/src/lib.rs` — FOUND (modified, +3 Tauri commands, registered)
- `app/src-tauri/tests/incremental_export_tests.rs` — FOUND (modified, +15 tests)
- `app/src-tauri/tests/fixtures/wire/favorites_prior.txt` — FOUND
- `app/src-tauri/tests/fixtures/wire/bookmarks_prior.txt` — FOUND
- `app/src-tauri/tests/fixtures/wire/highlights_prior.txt` — FOUND
- Commits `21197e24` (Task 1), `9c2b2671` (Task 2) — FOUND in `git log --oneline`

## Known Stubs

None. The frontend `CategoryList.tsx` "Export changed…" affordance is Favorites/Bookmarks/Highlights-agnostic by design — 09-01's `INCREMENTAL_EXPORT_COMMANDS` map is a data table wiring category → command name, so exposing these three new commands to the UI is a follow-on map-entry change, out of this plan's stated file scope (`files_modified` in the plan frontmatter lists only backend files + fixtures). Not tracked as a stub since the plan never scoped frontend wiring to this wave.

## Threat Flags

None — every new surface (three more prior-file parses, three more identity-key builders, three extended SELECT column lists) was already named in the plan's own `<threat_model>` (T-09-06 through T-09-09, T-09-SC) and mitigated as designed: fail-fast `parse_<category>_file` gates before any write, over-conservative whole-line fallback on identity-key field-count mismatch, parameterized SQL with only a leading column added to each existing query, and zero new Cargo dependencies.
