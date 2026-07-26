---
phase: 08-import-export-parity
plan: 01
subsystem: import-export-io
tags: [wire-format, favorites, id-recycling, tauri-commands]
dependency-graph:
  requires: []
  provides:
    - db/io module tree (header/export/import)
    - db/ids archive-wide id-gap recycler
    - DryRunReport.skipped field
    - export_favorites / import_favorites_dry_run / import_favorites_apply commands
  affects:
    - app/src-tauri/src/db/edit.rs (DryRunReport shape)
    - app/src/components/CategoryList.tsx (Export…/Import… wiring)
    - app/src/lib/operations.ts (NEEDS_SELECTION correction)
tech-stack:
  added: []
  patterns:
    - "Two-stage import: parse fully before any transaction opens (D8-04 fail-fast-whole-transaction)"
    - "Injected ExportHeaderCtx (archive_name/app_version/timestamp) for deterministic golden-fixture byte comparison"
    - "Archive-wide id-gap recycler threaded by &mut HashMap through the whole import run (D8-08)"
key-files:
  created:
    - app/src-tauri/src/db/ids.rs
    - app/src-tauri/src/db/io/mod.rs
    - app/src-tauri/src/db/io/header.rs
    - app/src-tauri/src/db/io/export.rs
    - app/src-tauri/src/db/io/import.rs
    - app/src-tauri/tests/fixtures/wire/favorites_golden.txt
    - app/src-tauri/tests/ids_tests.rs
    - app/src-tauri/tests/export_wireformat_tests.rs
    - app/src-tauri/tests/import_wireformat_tests.rs
    - app/src-tauri/tests/import_failfast_tests.rs
  modified:
    - app/src-tauri/src/db/edit.rs
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/db/reorder.rs
    - app/src-tauri/src/db/scrub.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/src/time.rs
    - app/src-tauri/tests/common/mod.rs
    - app/src/components/CategoryList.tsx
    - app/src/lib/operations.ts
    - app/src/lib/errors.ts
    - app/src/bindings/DryRunReport.ts
decisions:
  - "PD-2 applied: DryRunReport.skipped: BTreeMap<String, usize>, Default-derived; every existing constructor across lib.rs/reorder.rs/scrub.rs updated to set it explicitly to BTreeMap::new()."
  - "Location id recycling for Favorites import deliberately generalizes beyond Python (which never recycles Location ids in import_favorites, only TagMap) per the plan's explicit instruction — a documented, intentional strengthening, not a divergence bug."
  - "Timestamp format function now_export_header_timestamp() uses UTC (matching the existing now_iso8601_utc precedent) rather than Python's local time — non-load-bearing since the header body is never parsed back on import."
  - "import_malformed's ErrorDto does not carry line/reason (kept within the established two-layer 'reason is internal-only' boundary, D-14) — the UI-SPEC's literal line/reason interpolation is not implemented; errors.ts renders a generic-but-actionable sentence instead. Acceptance criteria did not require the interpolation, only the code's presence."
metrics:
  duration: "~1 session"
  completed: 2026-07-26
status: complete
---

# Phase 8 Plan 1: Export/Import Wire Spine + ID Recycler (Favorites tracer) Summary

Lands the whole Phase 8 interchange spine — shared wire-format header writer, archive-wide ID-gap recycler, `DryRunReport.skipped`, the export/import Tauri command triple, and CategoryList Export…/Import… wiring — proven end to end through Favorites, the simplest of the five `.txt` categories.

## What was built

**`db/io/header.rs`** — `build_export_header`/`ExportHeaderCtx` port `export_header` (`JWLManager.py:1367-1369`) byte-for-byte: the category tag line, the load-bearing single-space UTF-8-forcing line, `Exported from {archive}`/`by {app} ({version}) on {timestamp}`, and a 76-`*` divider with no trailing newline. Every value (archive name, app version, timestamp) is injected by the caller, never read from the wall clock inside the formatter, so the golden-fixture byte comparison is deterministic.

**`db/io/export.rs`** — `join_row` ports the `'None'`-sentinel pipe-join (`'|'.join(str(x) if x is not None else 'None' for x in row)`); `export_favorites` runs the exact Python SQL (selection-optional via `AND TagMapId IN (...)`, `ORDER BY Position`), writes UTF-8 with no BOM and no `{END}` sentinel (Favorites never writes one — Annotations, a later plan, does). `read_favorite_lines` is shared with `db/io/import.rs`'s dup-check so both directions format like-with-like.

**`db/ids.rs`** — `compute_available_ids`/`take_id` generalize `db::tags`'s single-table gap-scan to all nine of Python's `get_available_ids` tables (`Location, Bookmark, UserMark, Note, BlockRange, TagMap, PlaylistItem, IndependentMedia, Tag`). Builds the gap list ascending and never reverses it — `Vec::pop()` hands out the largest gap first, the proven-equivalent (07-03-SUMMARY.md) of Python's `available[::-1]` + `.pop()`.

**`db/io/import.rs`** — `parse_favorites_file` runs entirely before any transaction opens: line 1 must contain `{FAVORITES}` (unanchored substring search), every subsequent `|`-containing line must split into exactly 6 fields or the whole parse fails with a typed `ImportMalformed{category, line, reason}` naming the exact line. `apply_import_favorites` resolves/creates the system `Tag(Type=0,Name='Favorite')`, string-level-skips exact duplicates (via `read_favorite_lines`), and finds-or-inserts each new record's publication `Location` + `TagMap` row, threading one `&mut HashMap` of recycled ids through the whole run. `dry_run_import_favorites` wraps this in the established `PragmaGuard` + never-committed `unchecked_transaction` shape.

**`DryRunReport.skipped: BTreeMap<String, usize>`** (PD-2) — added with `Default`, so every existing Phase 2/7 constructor kept compiling; only the import paths populate it.

**Tauri commands**: `export_favorites` (no dry-run pair, never mutates, never sets `session.dirty`), `import_favorites_dry_run`/`import_favorites_apply` (standard dry-run/apply pair, re-parsing the file on each call per D8-10's accepted double-parse).

**Frontend**: `operations.ts` flips `Favorites:export`/`Favorites:import` LIVE. `CategoryList.tsx` wires Export… to the Tauri save dialog (direct write, transient "Exported" flash, no preview dialog per D8-09) and Import… to the open dialog → dry-run → `EditPreviewDialog` (added/updated/skipped summary, composed via the existing `summary` prop override, no new `EditPreviewDialog` prop needed) → confirm/apply → `list_category` refresh. `errors.ts` gains `export_failed`/`import_malformed`/`import_failed`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `NEEDS_SELECTION` incorrectly gated `export` on a nonzero selection**
- **Found during:** Task 3 (CategoryList wiring) — the Export… button rendered disabled at selection size 0 for Favorites.
- **Issue:** `operations.ts`'s pre-existing `NEEDS_SELECTION` set included `"export"` from Phase 6 (when it was still fully deferred and untested against a live category). The 08-UI-SPEC/plan both assert "export/import were already excluded from it, correctly modeling Export's selection-OPTIONAL nature" — this assumption was factually wrong against the shipped code.
- **Fix:** Removed `"export"` from `NEEDS_SELECTION`. `export` is now selection-optional for every category (D8-10): with a selection it exports exactly those rows, with none it exports the whole category.
- **Files modified:** `app/src/lib/operations.ts`, `app/src/lib/operations.test.ts` (updated two tests' expectations to reflect `export` being enabled at 0 selected).
- **Commit:** b0c7d60e

**2. [Rule 1 - Bug] `Location` CHECK constraint violations in freshly-authored fixtures**
- **Found during:** Task 1/3 test authoring — `Location`'s `Type`-scoped CHECK constraints (Type=0/1/2/3 branches) rejected several initially-authored fixture rows (e.g. `Type=1` with a non-null `DocumentId`).
- **Fix:** Re-shaped fixture rows to satisfy the real schema CHECK (Bible-edition rows use `Type=1` with `DocumentId`/`Track` both NULL; publication/track rows use `Type=0`'s "Track branch"). Golden fixture regenerated deterministically via a one-off Python script rather than hand-typing star counts.
- **Files modified:** `app/src-tauri/tests/common/mod.rs` (`seed_one_favorite`), `app/src-tauri/tests/export_wireformat_tests.rs`, `app/src-tauri/tests/fixtures/wire/favorites_golden.txt`.

**3. [Rule 1 - Bug] `TagMap`'s one-of CHECK violated by the id-gap fixture**
- **Found during:** Task 2 test authoring — `seed_id_gap_fixture`'s synthetic `TagMap` rows had all three of `PlaylistItemId`/`LocationId`/`NoteId` NULL, violating the "exactly one non-null" CHECK.
- **Fix:** Set a dangling `LocationId` (FK enforcement is OFF for this fixture, matching every other fixture in the file).
- **Files modified:** `app/src-tauri/tests/common/mod.rs`.

**4. [Rule 2 - Missing functionality] Every existing `DryRunReport { ... }` struct literal needed `skipped`**
- **Found during:** Adding the `skipped` field to `DryRunReport` (Task 2) — 8 explicit struct-literal constructors across `lib.rs`/`reorder.rs`/`scrub.rs` failed to compile since they don't use `..Default::default()`.
- **Fix:** Added `skipped: BTreeMap::new()` to every one, plus `skipped: {}` to 6 frontend TypeScript test fixtures whose `makeReport`/inline literals are explicitly typed as `DryRunReport`.
- **Files modified:** `app/src-tauri/src/lib.rs`, `db/reorder.rs`, `db/scrub.rs`; `app/src/components/{ColorMenu,EditPreviewDialog,FavoriteAddDialog,RecordEditor,TagDialog,UtilitiesMenu}.test.tsx`.

### Design note (not a bug, recorded for the next planner)

**`import_malformed`'s line/reason are NOT threaded through `ErrorDto`.** The UI-SPEC's copy for this code (`"Couldn't read this file — line {line}: {reason}..."`) assumes a payload field `ErrorDto` doesn't have — the shipped DTO is a fixed 4-field shape (`code`/`operation`/`safe_file_name`/`message_key`), and every other `*Failed` variant in `error.rs` follows the same "reason is internal-only, frontend copy is generic" posture (D-14). Rather than special-case one error code with new DTO fields (an architectural change outside this plan's scope — Rule 4 territory), `errors.ts`'s `import_malformed` sentence is generic-but-actionable ("doesn't look like a file exported from JW Library or JWL Manager") instead of interpolating the exact line/reason. This satisfies every stated acceptance criterion (the code exists, the ErrorBanner fires before the dialog) but is a lower-fidelity copy than the UI-SPEC literally describes. Flagging for whoever plans the next `.txt` category import in case the interpolation is actually wanted — it would require a documented, deliberate `ErrorDto` extension.

## Test output (actual, run at completion)

```
cd app/src-tauri && cargo test --jobs 2
  → all binaries: test result: ok (0 failed across every suite, including
    ids_tests: 5 passed; export_wireformat_tests: 4 passed;
    import_wireformat_tests: 5 passed; import_failfast_tests: 4 passed)

cd app/src-tauri && cargo clippy --all-targets -- -D warnings
  → clean (only the pre-existing ts-rs `try_from` attribute-parse warning,
    unrelated to this plan, present since NonEmptyTagMapIds/NonEmptyBlockRangeIds
    etc. were introduced in earlier phases)

cd app && npx tsc --noEmit
  → clean, zero errors

cd app && npx vitest run
  → Test Files  12 passed (12)
    Tests  123 passed (123)

git diff --stat app/src-tauri/Cargo.toml app/package.json
  → (empty — no dependency additions, PD/prohibition satisfied)
```

## Known Stubs

None — Favorites export/import is fully live end to end (op bar → dialog → command → disk/DB → refresh), no stubbed data paths.

## Self-Check: PASSED

- FOUND: app/src-tauri/src/db/ids.rs
- FOUND: app/src-tauri/src/db/io/mod.rs
- FOUND: app/src-tauri/src/db/io/header.rs
- FOUND: app/src-tauri/src/db/io/export.rs
- FOUND: app/src-tauri/src/db/io/import.rs
- FOUND: app/src-tauri/tests/fixtures/wire/favorites_golden.txt
- FOUND: app/src-tauri/tests/ids_tests.rs
- FOUND: app/src-tauri/tests/export_wireformat_tests.rs
- FOUND: app/src-tauri/tests/import_wireformat_tests.rs
- FOUND: app/src-tauri/tests/import_failfast_tests.rs
- FOUND commit d129ef9e (export spine + ids recycler + DryRunReport.skipped)
- FOUND commit b0c7d60e (Favorites import + CategoryList wiring)
