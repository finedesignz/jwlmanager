---
phase: 06-full-data-browsing
plan: 03
subsystem: frontend
tags: [react, virtualization, selection, capability-descriptor, DATA-07]
status: complete
requires:
  - "06-01: unified BrowseRow binding"
  - "06-02: list_category dispatch + identity-PK per category"
provides:
  - "lib/operations.ts operationSet(category, selectionSize) capability descriptor"
  - "CategorySwitcher enum-driven six-category selector"
  - "CategoryList virtualized selectable list over BrowseRow + category"
affects:
  - "06-04 will wire CategorySwitcher + CategoryList into App.tsx and remove NotesList"
tech-stack:
  added: []
  patterns:
    - "TanStack Virtual fixed-44px always-virtualized single code path (D6-07)"
    - "Set<bigint> selection keyed by identity PK, reset-on-category-change (D6-05)"
    - "capability descriptor f(category, selection.size) drives visible op set (D6-08)"
key-files:
  created:
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
    - app/src/components/CategorySwitcher.tsx
    - app/src/components/CategorySwitcher.test.tsx
    - app/src/components/CategoryList.tsx
    - app/src/components/CategoryList.test.tsx
  modified:
    - app/src/styles.css
decisions:
  - "W1: Playlists primary label special-cased to prefer detail1 (PlaylistItem.Label) over the '* OTHER *' full/short/symbol sentinel"
  - "Deferred ops render as disabled toolbar buttons with data-deferred=true + '(soon)' label"
  - "Selection count exposed as an always-rendered testid so selection size is observable for every category (not only Notes)"
metrics:
  duration: "~10m"
  completed: 2026-07-23
  tasks: 3
  files: 7
---

# Phase 6 Plan 03: Category browsing frontend (operations + switcher + virtualized list) Summary

Frontend half of DATA-07 built as three additive, tested pieces: a `(category, selection.size)` operation-capability descriptor, an enum-driven six-category switcher, and a `NotesList`-generalized virtualized selectable `CategoryList` whose contextual operation set updates with selection — with only `Notes:delete` wired to a real backend mutation. Purely additive: `App.tsx` untouched and `NotesList` retained (06-04 rewires and removes it).

## Tasks

| Task | Name | Commit | Key files |
| ---- | ---- | ------ | --------- |
| 1 | operations.ts capability descriptor (D6-08) | 95ae3098 | operations.ts, operations.test.ts |
| 2 | CategorySwitcher enum-driven selector (D6-06) | 5acf07e7 | CategorySwitcher.tsx(.test), styles.css |
| 3 | CategoryList virtualized list (D6-05, D6-07, D6-08) | e890c025 | CategoryList.tsx(.test), styles.css |

## What was built

- **`operations.ts`** — `Op` union, per-category `CAPABILITY` map (ported from Python option tables), `NEEDS_SELECTION` gate (delete/export/view/color/tag), `LIVE` set = exactly `Notes:delete`. `operationSet(cat, size)` returns `{ op, enabled, deferred }` per capability op; `deferred` is true exactly when the `(category, op)` pair is not LIVE, and `enabled` requires live AND selection precondition.
- **`CategorySwitcher.tsx`** — segmented control over the six `Category` variants, single-sourced as a typed `Category[]` (keys off the enum, never translated labels — D6-06); `aria-pressed` marks active; `onSelect` emits the enum value; active-click is a no-op; `disabled` parks the whole control.
- **`CategoryList.tsx`** — generalizes `NotesList` over `BrowseRow` + `category`: same fixed-44px `ROW_HEIGHT` / `overscan: 8` / `NO_WRAP_STYLE` virtualization, applied to EVERY category (no per-category opt-out). Selection `Set<bigint>` keyed by `row.id`, reset to empty on `category` change. Operation bar rendered from `operationSet`; `Notes:delete` drives the reused `delete_notes_dry_run` -> `DeletePreviewDialog` -> `delete_notes_apply` + local row filter; all other ops render deferred/disabled. Per-category columns (color/tags/modified) render only when non-null so absent columns never grow a row; per-category empty state.

## W1 decision (plan-check fix)

**Chosen: special-case the playlist primary label to prefer `detail1`.** For Playlists the backend sets `full`/`short`/`symbol` all to the `"* OTHER *"` sentinel (`db::browse::query_playlists`) and puts the real label in `detail1` (`PlaylistItem.Label`). The generic `[full, detail1, detail2]` join would surface `"* OTHER *"` as primary. `resolveLabel(row, category)` special-cases Playlists to return `detail1 ?? full`; every other category keeps the verbatim publication-title join. The playlist Name still surfaces via the tags column. A dedicated test (`surfaces the playlist Label (detail1) as primary`) asserts the sentinel never appears as the primary label.

## Deviations from Plan

None beyond the sanctioned W1 discretion. No new backend mutation, no new dependency.

## Verification (DoD)

- `npm run build` (tsc + vite): clean, 29 modules, built in 148ms.
- `npx vitest run` (full suite): **8 files, 66 tests, all green** — incl. the 9,000-row virtualization test (rendered DOM row nodes < 100 vs 9,000), the 2,000-char fixed-44px/no-wrap test, the op-set-updates-with-selection (deferred-op) test, and switch-resets-selection.
  - operations.test.ts: 4 · CategorySwitcher.test.tsx: 5 · CategoryList.test.tsx: 14.
- `cargo test`: **not run — no binding/backend/Rust file touched** (plan is purely additive frontend; `app/package.json` and all `src-tauri/**` unchanged).
- Threat T-06-SC: `app/package.json` unchanged across all three commits (verified via `git diff`).

## Notes for 06-04

- `NotesList.tsx`/`NotesList.test.tsx` intentionally retained (still green) — 06-04 removes them after rewiring `App.tsx` to `CategorySwitcher` + `CategoryList`.
- `CategoryList` testids: `category-list-container`, `category-list-empty`, `category-list-viewport`, `category-list-row`, `category-list-row-label`, `category-list-row-checkbox`, `category-list-selection-count`, `category-list-delete-button` (Notes only), `category-list-op-<op>` (deferred).

## Self-Check: PASSED
