---
phase: 06-full-data-browsing
plan: 01
subsystem: db
tags: [refactor, labels, browse-row, ts-rs, notes]
requires:
  - db::notes label helpers (Phase 1 Notes tracer)
  - db::resources::ResourceCatalog
provides:
  - db::labels (pub(crate) process_code/process_color/process_detail/resolve_publication)
  - db::notes::BrowseRow (unified row type + BrowseRow.ts binding)
affects:
  - app/src-tauri/src/archive/mod.rs
  - app/src-tauri/src/lib.rs
  - app/src (NotesList, CommandBar, App)
tech-stack:
  added: []
  patterns:
    - "Shared label-synthesis module reused by all browse categories (D6-01)"
    - "Single unified BrowseRow with Option<String> nullable columns = merge_df fill_null analog (D6-02)"
key-files:
  created:
    - app/src-tauri/src/db/labels.rs
    - app/src/bindings/BrowseRow.ts
  modified:
    - app/src-tauri/src/db/notes.rs
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/src/archive/mod.rs
    - app/src/components/NotesList.tsx
    - app/src/components/NotesList.test.tsx
    - app/src/components/CommandBar.tsx
    - app/src/components/CommandBar.test.tsx
    - app/src/App.tsx
    - app/src-tauri/tests/notes_query_tests.rs
    - app/src-tauri/tests/merge_orchestration.rs
  deleted:
    - app/src/bindings/NotesRow.ts
decisions:
  - "Nullable columns (language/color/tags/modified) modeled as Option<String>; Notes wraps existing values in Some(...) for zero behavior change."
  - "Historical name NotesRow retained only in explanatory comments, never in code."
metrics:
  duration: ~35m
  completed: 2026-07-23
status: complete
---

# Phase 6 Plan 01: Extract db/labels.rs + Unify NotesRow to BrowseRow Summary

The enabling refactor for Phase 6: lifted the shared resources.db label-synthesis helpers out of `db/notes.rs` into a reusable `pub(crate)` `db/labels.rs` module (D6-01), and generalized the Notes-only `NotesRow` into the single unified `BrowseRow` (ts-rs) schema every category will collapse to (D6-02) — with the shipped Notes browse+delete slice staying fully green through both changes (zero behavior change).

## What was built

**Task 1 — `db/labels.rs` (commit 781e327e):** Moved verbatim from `notes.rs` the `CODE_YR`/`CODE_JWB` `LazyLock<Regex>` statics, the `DATED_PREFIX_EXCLUDED`/`BIBLE_APPENDIX_SYMBOLS`/`COLOR_NAMES` const arrays, and the four functions `process_code`/`process_color`/`process_detail`/`resolve_publication` (now `pub(crate)`), plus their 8 label-math unit tests. `notes.rs` imports them and dropped its now-unused `regex`/`LazyLock` imports. Registered `pub mod labels;` in `db/mod.rs`. Pure move — no private duplicate left behind.

**Task 2 — unified `BrowseRow` (commit 63c9ca65):** Renamed `NotesRow` to `BrowseRow` and made `language`, `color`, `tags`, `modified` each `Option<String>` (the `merge_df` `fill_null` analog for categories lacking those columns); `year`/`detail1`/`detail2` stay `Option<String>`, `short`/`full`/`type_group` stay `String`, `independent: bool` retained. Notes populates every column so the query functions wrap each value in `Some(...)` — byte-identical output. ts-rs `export_to` retargeted to `BrowseRow.ts` (regenerated); stale `NotesRow.ts` deleted. `open_archive`/`list_notes`/`reload_notes`/`query_notes` now return `Vec<BrowseRow>`. Frontend consumers (`NotesList`, `CommandBar`, `App` + their tests) import `BrowseRow`; the row render tolerates `null` tags/modified.

## Verification / DoD (all green)

- `cargo fmt --check` — clean (exit 0)
- `cargo clippy --all-targets -- -D warnings` — clean (exit 0). The two `failed to parse serde attribute` notes are a pre-existing ts-rs macro diagnostic on the derive (present before this plan), not clippy lints; gate exits 0.
- `cargo test --jobs 2` (full workspace) — **140 passed, 0 failed** (40 lib unit incl. 8 `db::labels` tests + integration binaries incl. `notes_query_tests`, `open_archive_tests`, `merge_orchestration`). `--jobs 2` used per host linker OOM guidance (`os error 1455`), an env limit not a code defect.
- `npm run build` (tsc + vite) — clean (227 kB bundle)
- `npx vitest run` — **43 passed** across 5 files (`NotesList.test.tsx`, `CommandBar.test.tsx`, etc.)
- No dependency added: `app/src-tauri/Cargo.toml` and `app/package.json` unchanged (git diff confirms).
- `app/src/bindings/NotesRow.ts` deleted; `app/src/bindings/BrowseRow.ts` exists.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Update out-of-plan `NotesRow` references broken by the rename**
- **Found during:** Task 2 (the rename breaks compilation of files the plan's `files_modified` did not list).
- **Issue:** `app/src-tauri/src/archive/mod.rs` (import + two signatures) and `app/src-tauri/tests/merge_orchestration.rs` (a `Vec<...::NotesRow>` return type) reference the renamed struct; `app/src-tauri/tests/notes_query_tests.rs` asserted `located_row.language == "English"` on what is now `Option<String>`.
- **Fix:** Renamed the type references to `BrowseRow`; changed the language assertion to `located_row.language.as_deref() == Some("English")`. No behavior change — the Genesis located note still yields `detail1 "01: Genesis"` and language `English`.
- **Files modified:** `app/src-tauri/src/archive/mod.rs`, `app/src-tauri/tests/merge_orchestration.rs`, `app/src-tauri/tests/notes_query_tests.rs`
- **Commit:** 63c9ca65

## TDD Gate Compliance

Task 2 is marked `tdd="true"`, but it is a pure rename/generalize refactor with **no new behavior** — the plan requires byte-identical output (Notes wraps every field in `Some(...)`). There is therefore no RED gate to fail first; the existing Rust (`notes_query_tests`, `open_archive_tests`) and TS (`NotesList.test.tsx`) suites are the regression net and stayed green throughout. Committed as `refactor(...)` accordingly. No new failing-then-passing test cycle was introduced because the contract is "no observable change."

## Known Stubs

None. The `Option<String>` "absent column" case is intentionally unexercised by data here (Notes populates every column); the absent case lands with the category getters in 06-02, as the plan specifies. This is a documented forward dependency, not a stub.

## Self-Check: PASSED

- `app/src-tauri/src/db/labels.rs` — FOUND
- `app/src/bindings/BrowseRow.ts` — FOUND
- `app/src/bindings/NotesRow.ts` — deleted (confirmed absent)
- Commit 781e327e — FOUND
- Commit 63c9ca65 — FOUND
