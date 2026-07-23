---
phase: 06-full-data-browsing
plan: 04
subsystem: frontend
tags: [react, integration, category-switch, DATA-07, phase-complete]
status: complete
requires:
  - "06-02: list_category dispatch + identity-PK per category"
  - "06-03: operations.ts + CategorySwitcher + CategoryList"
provides:
  - "App.tsx category-aware shell: owns {category, rows}, wires CategorySwitcher + list_category + CategoryList"
  - "App.test.tsx DATA-07 end-to-end integration proof over mocked IPC"
affects:
  - "Phase 7 editing builds on the generic operation set surfaced per (category, selection)"
tech-stack:
  added: []
  patterns:
    - "App owns {category, rows}; switch re-fetches via list_category, last-write-wins swap (T-06-09)"
    - "Selection reset delegated to CategoryList's category-prop effect — App never hoists selection (D6-05)"
    - "list_category failure routes through existing ErrorBanner, prior view intact"
key-files:
  created:
    - app/src/App.test.tsx
  modified:
    - app/src/App.tsx
  removed:
    - app/src/components/NotesList.tsx
    - app/src/components/NotesList.test.tsx
decisions:
  - "CategorySwitcher lives in App's main area (not CommandBar) so the open/save file-command flow stays untouched"
  - "Selection reset NOT hoisted into App — passing the new category to CategoryList is sufficient (06-03 resets on category-prop change)"
  - "Preserved the existing empty-state App test and added the DATA-07 integration suite in the same file"
metrics:
  duration: "~8m"
  completed: 2026-07-23
  tasks: 2
  files: 3
---

# Phase 6 Plan 04: Wire CategorySwitcher + list_category into the App shell Summary

Final integration wave of Phase 6: `App.tsx` generalized from Notes-only to category-aware. It now owns `{category, rows}`, renders `CategorySwitcher` above `CategoryList` in the archive-open branch, invokes `list_category` on switch to swap the rendered rows, and retires the superseded `NotesList`. DATA-07 is proven end-to-end by a new `App.test.tsx` integration suite over mocked IPC. `open_archive` still yields the initial Notes view; the shipped Notes delete flow still works end-to-end.

## Tasks

| Task | Name | Commit | Key files |
| ---- | ---- | ------ | --------- |
| 1 | Wire category state + switcher + list_category into App.tsx; retire NotesList (D6-05/06/09) | b8de742c | App.tsx (+); NotesList.tsx/.test (−) |
| 2 | App.test.tsx — DATA-07 end-to-end integration (mocked invoke) | 381f35f9 | App.test.tsx |

## What was built

- **`App.tsx`** — replaced `notes: BrowseRow[] | null` with `rows: BrowseRow[] | null` + `category: Category` (default `"Notes"`). `handleOpened`/`handleNewArchive` set `category = "Notes"` and seed `rows`; `handleRowsChanged` (renamed from `handleNotesChanged`) updates rows after a delete; `handleSelectCategory(next)` invokes `invoke<BrowseRow[]>("list_category", { category: next })`, and on success sets `category = next` + `rows = result` (failures route through `handleError` → `ErrorBanner`, leaving the prior view intact — T-06-09). Renders `CategorySwitcher` (active = current category, `onSelect = handleSelectCategory`) above `CategoryList` (`rows`, `category`, `onRowsChanged`, `onError`). `CommandBar` untouched.
- **Selection reset** — delegated, not hoisted: `CategoryList` already resets its `Set<bigint>` selection on the `category` prop change (06-03), so passing the new category is sufficient. No stale integer key ever crosses categories (D6-05).
- **`NotesList` retired** — `NotesList.tsx` / `NotesList.test.tsx` deleted (`git rm`); its assertions live in `CategoryList.test.tsx` (06-03). Only a doc-comment reference remains in `CategoryList.tsx` ("the generalized successor to `NotesList`") — no import.
- **`App.test.tsx`** — kept the existing empty-state test; added a DATA-07 suite driving the real `CommandBar` open path (mocked `invoke` + plugin-dialog `open`/`save`, `ResizeObserver`/`clientHeight` jsdom stubs so the virtualizer measures). Covers: open → Notes view default; select Highlights → `list_category` invoked + rows swap (criterion 1); switch resets prior selection (D6-05); op set live-Delete for Notes-with-selection vs deferred/disabled after switch (criterion 3); Notes delete end-to-end (dry-run → confirm → row removed locally); `list_category` failure leaves the prior view intact + surfaces the error (T-06-09).

## Deviations from Plan

None. No new backend mutation, no new dependency. Row-label test matchers use regex (labels join `full — detail1 — detail2`, so exact-string `findByText` would not match) — a test-authoring detail, not a plan deviation.

## Verification (DoD)

- `npm run build` (tsc + vite): clean, 31 modules transformed, built in 184ms.
- `npx vitest run` (full suite): **7 files, 63 tests, all green** — incl. the 7 App.test.tsx integration tests (empty-state + 6 DATA-07).
- `cargo test`: **not run — no binding/backend/Rust file touched** (frontend-only wiring; `src-tauri/**` and `app/package.json` unchanged).
- No `NotesList` import references remain under `app/src` (only a comment in `CategoryList.tsx`).
- Threat T-06-SC: `app/package.json` unchanged across both commits.
- **Notes browse + delete still works end-to-end**: verified by the `App.test.tsx` "Notes delete still works end-to-end through the shell" test (dry-run → preview dialog → confirm → apply → local row removal) and the preserved `CategoryList.test.tsx` Notes delete tests.

## Phase 6 completion — all 3 ROADMAP criteria met

1. **Browse all 5 new categories with real data** — `CategorySwitcher` (six enum variants) + `handleSelectCategory` → `list_category` (06-02 verbatim getters) → `CategoryList` virtualized render. Criterion 1 proven for Highlights in `App.test.tsx`; the same code path serves Bookmarks/Annotations/Favorites/Playlists (single generic dispatch, no per-category branch).
2. **Select one or many** — `CategoryList` `Set<bigint>` selection, multi-select verified per-category in `CategoryList.test.tsx`; App integration verifies selection + reset-on-switch.
3. **Op set updates with selection** — `operationSet(category, selection.size)` (06-03) drives the contextual bar; App test asserts live Notes-delete (with selection) vs deferred elsewhere.

Boundary held: only `Notes:delete` is wired to a real backend mutation; per-category deletes/edits remain Phase 7.

## Manual visual gate (STATE todo, non-blocking — Phase 1 convention)

`npm run tauri dev`, open a real `.jwlibrary`, switch across all six categories, confirm each renders real data, multi-select works, and Delete appears only for Notes with a selection.

## Self-Check: PASSED
