---
phase: 11-platform-polish
plan: 04
subsystem: ui
tags: [i18n, react-context, tauri, localization, zero-dependency]

# Dependency graph
requires:
  - phase: 11-platform-polish
    plan: 03
    provides: "I18nProvider/useI18n, StringKey/{token} substitution, split-around-JSX convention, 9 locale files (en complete, 8 scaffolded), structural completeness-test technique"
provides:
  - "describeError(err, t) -- a pure function of (ErrorDto, t), all 39 codes (including two previously-unhandled: trim_failed, record_edit_failed) resolving through the errors.* catalog"
  - "Every component in app/src/components/ renders exclusively through t() -- CommandBar, TagDialog, FavoriteAddDialog, MediaAddDialog, EditPreviewDialog, FoldMergeDialog, RecordEditor, CategorySwitcher, CategoryList, ColorMenu, UtilitiesMenu, ErrorBanner, JwlCoreNotice"
  - "categoryLabel()/colorLabel() helpers (CategorySwitcher.tsx/ColorMenu.tsx) -- translated DISPLAY labels for Category/PALETTE enum values, strictly separate from the raw values driving onSelect/IPC/data-testid (D6-06/DATA-08)"
  - "A multi-return-block structural completeness scan (app/src/i18n/completeness.test.ts) proving all 13 components + native-dialog filters/title strings are free of stray hardcoded English"
  - "A Rust-source-derived describeError coverage test (app/src/lib/errors.test.ts) that fails automatically if a future to_dto match arm ships without a matching catalog key"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Multi-block structural scan: loop every `return (` in a file (not just the first) via paren-balance, since several components (TagDialog, ColorMenu, UtilitiesMenu, RecordEditor, FavoriteAddDialog) define more than one JSX-returning function/branch -- extends 11-03's single-block technique."
    - "Native-dialog option-literal scan: a SEPARATE regex pass over the whole file (not just the JSX return block) for `filters: [{ name: \"...\" }]` and `title: \"...\"`/`title: \\`...\\`` object-literal properties, which live in handler functions above the return statement and are invisible to the JSX-text/attr scan."
    - "Rust-source-derived coverage test: read the Rust `to_dto` match-arm body via a brace-balance function-body extractor, then regex-extract every `(\"code\", \"key\")` tuple (handling both single-line and multi-line-with-trailing-comma arm shapes) -- the code list is never hand-typed."
    - "Per-render dialog filters: CommandBar's FILTERS array is rebuilt every render from `t(...)` (not a module-level constant) since native-dialog option objects can't be built before the component's own `useI18n()` call runs."
    - "EditPreviewDialog's title/ariaLabel/confirmLabel/confirmPendingLabel defaults resolve via `prop ?? t(...)` inside the component body, not as destructuring defaults -- `t` isn't available until `useI18n()` executes."
    - "Read source files (both .tsx components and .rs Rust sources) at test time via Vite's `?raw` import suffix, never `node:fs` -- this project has no `@types/node` dependency (11-01's styles_tokens.test.ts precedent)."

key-files:
  created:
    - app/src/i18n/completeness.test.ts
    - app/src/lib/errors.test.ts
  modified:
    - app/src/i18n/en.ts
    - app/src/lib/errors.ts
    - app/src/components/ErrorBanner.tsx
    - app/src/components/ErrorBanner.test.tsx
    - app/src/components/JwlCoreNotice.tsx
    - app/src/components/CategorySwitcher.tsx
    - app/src/components/CategorySwitcher.test.tsx
    - app/src/components/CategoryList.tsx
    - app/src/components/CategoryList.test.tsx
    - app/src/components/ColorMenu.tsx
    - app/src/components/ColorMenu.test.tsx
    - app/src/components/UtilitiesMenu.tsx
    - app/src/components/UtilitiesMenu.test.tsx
    - app/src/components/CommandBar.tsx
    - app/src/components/CommandBar.test.tsx
    - app/src/components/TagDialog.tsx
    - app/src/components/TagDialog.test.tsx
    - app/src/components/FavoriteAddDialog.tsx
    - app/src/components/FavoriteAddDialog.test.tsx
    - app/src/components/MediaAddDialog.tsx
    - app/src/components/MediaAddDialog.test.tsx
    - app/src/components/EditPreviewDialog.tsx
    - app/src/components/EditPreviewDialog.test.tsx
    - app/src/components/FoldMergeDialog.tsx
    - app/src/components/FoldMergeDialog.test.tsx
    - app/src/components/RecordEditor.tsx
    - app/src/components/RecordEditor.test.tsx

key-decisions:
  - "ErrorBanner.tsx calls useI18n() itself and passes t into describeError(error, t) -- it is the ONE call site (confirmed by a whole-tree grep for describeError(), which found no other production call sites), so no prop-drilling of t through SettingsProvider.tsx was needed; SettingsProvider.tsx required zero code changes despite being in Task 1's file list."
  - "describeError gained two new branches (trim_failed, record_edit_failed) for Rust-emitted codes the pre-existing switch never handled -- the Rust source already emitted them (db/trim.rs, db/record_edit.rs); without these two branches, the new describeError-full-coverage test would fail by design. Documented as a Rule 2 addition, not a plan deviation, since Task 1's action explicitly said 'for every describeError case' -- deriving from source naturally surfaced the gap."
  - "A common.* catalog namespace (cancel/preparing/delete/add/saving/deleting/confirmDelete/color/ok) consolidates words rendered byte-identically across many components (e.g. 'Cancel' in six dialogs, 'Preparing…' in five), rather than one key per component -- reduces catalog size and keeps translations consistent without changing any established naming convention (still a namespaced key, per the plan's own convention)."
  - "Category and PALETTE color enum values get a translated DISPLAY label via categoryLabel()/colorLabel() helper functions (exported from CategorySwitcher.tsx/ColorMenu.tsx respectively, imported by CategoryList.tsx/RecordEditor.tsx) rather than a second catalog lookup pattern -- keeps the enum-to-label mapping single-sourced per enum, reused by every consumer."
  - "The two new test files were written as `.test.ts` (not `.test.tsx`), per the plan's literal filenames -- the 'language switch, multi-component' Harness component is built with React.createElement rather than JSX, since esbuild's .ts loader does not parse JSX syntax."
  - "completeness.test.ts's structural scan loops EVERY `return (` block in a file (not just the first, as 11-03's App.test.tsx/SettingsDialog.test.tsx did) -- several of the 13 files define more than one JSX-returning function or early-return branch (TagDialog.tsx's TriStateCheckbox helper plus its own preview/main returns; ColorMenu.tsx/UtilitiesMenu.tsx/RecordEditor.tsx/FavoriteAddDialog.tsx's preview-vs-picker early returns). Scanning only the first block would have silently under-scanned most of these files."

patterns-established:
  - "Multi-return-block structural completeness scan (generalizes 11-03's single-block technique to any file with more than one JSX-returning function)."
  - "Native-dialog option-literal scan as a distinct pass from the JSX-text/attr scan, for strings that live in handler code above the return statement."
  - "Rust-source-derived test coverage list (regex-extracted from to_dto match arms) as the pattern for any future TS test that must stay honest against a Rust enum/match without a hand-duplicated list."

requirements-completed: [PLAT-03]

coverage:
  - id: D1
    description: "describeError(err, t) resolves all 39 Rust-emitted error codes (including two previously-unhandled: trim_failed, record_edit_failed) through the errors.* catalog, never a literal string"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/lib/errors.test.ts#describeError full coverage (11-04-PLAN.md task 3) every code the Rust source actually emits resolves..."
        status: pass
      - kind: unit
        ref: "app/src/lib/errors.test.ts#describeError full coverage (11-04-PLAN.md task 3) the missing-branch guard genuinely fires..."
        status: pass
    human_judgment: false
  - id: D2
    description: "All 13 retrofitted components (CommandBar, TagDialog, FavoriteAddDialog, MediaAddDialog, EditPreviewDialog, FoldMergeDialog, RecordEditor, CategorySwitcher, CategoryList, ColorMenu, UtilitiesMenu, ErrorBanner, JwlCoreNotice) render exclusively through t(), including native-dialog filters[].name/title strings"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/i18n/completeness.test.ts#completeness all components (13 per-file tests + 2 red/green demonstrations)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Category/PALETTE enum values stay control-flow-pure (onSelect/IPC/data-testid untouched) while their rendered labels are translated (D6-06/DATA-08)"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/i18n/completeness.test.ts#category enum isolation (structural)"
        status: pass
      - kind: unit
        ref: "app/src/components/CategorySwitcher.test.tsx, app/src/components/CategoryList.test.tsx (existing enum-driven assertions, unmodified, still passing)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Switching language re-renders CommandBar and ErrorBanner on the same interaction, falling back to English"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/i18n/completeness.test.ts#language switch, multi-component"
        status: pass
    human_judgment: true
    rationale: "The live-context re-render is proven against mocked IPC in a headless test; a real Tauri app session cycling through all 9 locales with the command bar, an open dialog, and an error banner all visible together (this plan's own <verification> Manual bullet) was not exercised in this headless execution environment -- same caveat 11-01/11-03 recorded for their own manual-verification items."
  - id: D5
    description: "Zero new npm/Cargo dependencies; zero Rust files touched"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "git diff app/package.json app/package-lock.json app/src-tauri (empty across all three task commits)"
        status: pass
    human_judgment: false

# Metrics
duration: ~90min
completed: 2026-08-16
status: complete
---

# Phase 11 Plan 4: Full i18n Retrofit (PLAT-03 completion) Summary

**Every component in `app/src/components/` and `lib/errors.ts`'s 39-code error catalog now render exclusively through `t()`, proven by a multi-return-block structural scan and a Rust-source-derived `describeError` coverage test — PLAT-03 is complete, not partial.**

## Performance

- **Duration:** ~90 min
- **Tasks:** 3 (error catalog + 6 components, 7 dialog-heavy components, completeness/coverage tests)
- **Files modified:** 28 (2 new test files, 26 modified — 13 components + 13 matching test files, `en.ts`, `lib/errors.ts`)

## Accomplishments
- `describeError` is now a pure `(err, t) => string` function; its ONE production call site (`ErrorBanner.tsx`, confirmed by a whole-tree grep) pulls `t` from `useI18n()` — `SettingsProvider.tsx` needed zero code changes since its own `ErrorBanner` instance already sits inside `I18nProvider`'s subtree (11-03).
- Two Rust-emitted error codes (`trim_failed`, `record_edit_failed`) that the pre-existing `describeError` switch never handled now have their own catalog-sourced branches — surfaced by deriving the code list from the real Rust source rather than trusting a hand-maintained list.
- All 13 remaining components render exclusively through `t()`: `CommandBar`, `TagDialog`, `FavoriteAddDialog`, `MediaAddDialog`, `EditPreviewDialog`, `FoldMergeDialog`, `RecordEditor`, `CategorySwitcher`, `CategoryList`, `ColorMenu`, `UtilitiesMenu`, `ErrorBanner`, `JwlCoreNotice` — including native `@tauri-apps/plugin-dialog` `filters[].name` and `title` strings (CommandBar's `FILTERS`, CategoryList's five `open`/`save` calls, MediaAddDialog's `open` call).
- Category (`Category` enum) and highlight-color (`PALETTE`) display names are translated for PRESENTATION ONLY via new `categoryLabel()`/`colorLabel()` helpers — every `onSelect` payload, IPC argument, and `data-testid` still keys off the raw enum value, verified both by the existing enum-driven component tests (passing unmodified) and a new structural source-text guard.
- A structural completeness test (`app/src/i18n/completeness.test.ts`) extends 11-03's single-return-block scan to loop EVERY `return (` block per file (several of the 13 define more than one JSX-returning function/branch) plus a second pass for native-dialog option literals — 13 per-file assertions plus three demonstrated red/green guards (a stray JSX attribute literal, a stray dialog filter name, a translated value routed into `onSelect`).
- A Rust-source-derived coverage test (`app/src/lib/errors.test.ts`) regex-extracts all 39 `("code", "message_key")` tuples from `error.rs`'s and `settings.rs`'s `to_dto` match arms at test time and asserts every one resolves to a distinct, non-generic, catalog-sourced sentence — a future `to_dto` arm added without a matching `describeError` branch fails this test automatically (demonstrated: an unmapped code falls through to the generic default fallback; every real code does not).
- A live multi-component test proves the retrofit is genuinely wired, not merely present in source: switching the active locale re-renders `CommandBar` and `ErrorBanner` together, both falling back to English (since the target locale's catalog stays empty per D11-02).
- Zero new npm/Cargo dependencies and zero Rust files touched across all three task commits (`git diff` on every manifest/lockfile empty throughout).

## Task Commits

Each task was committed atomically:

1. **Task 1: Error catalog + six lower-complexity components** — `946be043` (feat)
2. **Task 2: CommandBar + six dialog-heavy components** — `5b7e57cb` (feat)
3. **Task 3: Completeness + describeError full-coverage tests** — `75a677d4` (test)

## Files Created/Modified
- `app/src/i18n/en.ts` — extended with `errors.*` (39 keys + `default`), `common.*`, `category.*`, `color.*`, and one namespace per remaining component (`jwlCoreNotice.*`, `colorMenu.*`, `utilitiesMenu.*`, `commandBar.*`, `editPreviewDialog.*`, `foldMergeDialog.*`, `tagDialog.*`, `favoriteDialog.*`, `mediaAddDialog.*`, `recordEditor.*`, `categoryList.*`)
- `app/src/lib/errors.ts` — `describeError(err, t)`, all 39 codes resolved through the catalog
- `app/src/lib/errors.test.ts` — new; Rust-source-derived `describeError` coverage test
- `app/src/i18n/completeness.test.ts` — new; multi-block structural scan + native-dialog-literal scan + language-switch + category-isolation tests across all 13 components
- `app/src/components/{CommandBar,TagDialog,FavoriteAddDialog,MediaAddDialog,EditPreviewDialog,FoldMergeDialog,RecordEditor,CategorySwitcher,CategoryList,ColorMenu,UtilitiesMenu,ErrorBanner,JwlCoreNotice}.tsx` — full `t()` retrofit
- matching `*.test.tsx` files for all 13 components — wrapped every `render(...)`/`rerender(...)` call site in `I18nProvider` (a component calling `useI18n()` throws outside one), and updated `describeError(...)` call sites (`ErrorBanner.test.tsx`, `FavoriteAddDialog.test.tsx`) to pass a real `t` built from the actual `en` catalog

## Decisions Made
- `ErrorBanner.tsx` pulls `t` from `useI18n()` itself rather than receiving it as a prop from every caller — simpler, and correct since it is the sole production call site of `describeError`.
- `trim_failed`/`record_edit_failed` gained real describeError branches (Rule 2) rather than silently falling through to the generic `default` case, since the coverage test derived from the actual Rust source would otherwise fail for them.
- A `common.*` catalog namespace consolidates words rendered byte-identically across many components ("Cancel", "Preparing…", "Delete", "Saving…", "Deleting…", "Confirm delete", "Color", "OK") — still a namespaced key per the plan's convention, just shared rather than duplicated per component.
- `categoryLabel()`/`colorLabel()` are exported helper functions (from `CategorySwitcher.tsx`/`ColorMenu.tsx` respectively) reused by `CategoryList.tsx`/`RecordEditor.tsx` — single-sourced enum-to-label mapping per enum.
- `completeness.test.ts` and `errors.test.ts` are `.test.ts` files (matching the plan's literal filenames) — the multi-component language-switch harness is built with `React.createElement`, since `.ts` files aren't parsed as JSX by esbuild.
- Both new test files read source (component `.tsx` files and Rust `.rs` files) via Vite's `?raw` import suffix rather than `node:fs`, since `@types/node` is not a project dependency (11-01's `styles_tokens.test.ts` precedent) and this plan adds zero new dependencies.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added `errors.trimFailed`/`errors.recordEditFailed` catalog keys + describeError branches**
- **Found during:** Task 1, while enumerating every Rust `to_dto` code to move into the catalog
- **Issue:** `ArchiveError::TrimFailed`/`ArchiveError::RecordEditFailed` are real, already-emitted Rust error codes (`db/trim.rs`, `db/record_edit.rs`) with no corresponding branch in the pre-existing `describeError` switch — a genuine pre-11-04 gap, not something this plan's retrofit introduced, but one that would leave two real error paths rendering the app's generic "Couldn't complete this operation" fallback instead of an actionable, specific sentence.
- **Fix:** Added `errors.trimFailed`/`errors.recordEditFailed` catalog entries (matching the established "archive is unchanged, try again" phrasing family) and their `describeError` branches.
- **Files modified:** `app/src/i18n/en.ts`, `app/src/lib/errors.ts`
- **Verification:** `app/src/lib/errors.test.ts`'s coverage test (derived from the real Rust source) passes for both codes; both resolve to distinct, non-generic sentences.
- **Committed in:** `946be043` (Task 1 commit)

**2. [Rule 3 - Blocking] Wrapped every component test file's `render`/`rerender` in `I18nProvider`**
- **Found during:** Tasks 1-2, running each task's own mandated `npx vitest run` verification
- **Issue:** Every retrofitted component now calls `useI18n()`, which throws `"useI18n must be used within an I18nProvider"` when rendered bare — all 12 affected `*.test.tsx` files previously called `render(<Component .../>)` (and, in `CategoryList.test.tsx`/`EditPreviewDialog.test.tsx`, `rerender(...)`) with no provider in the tree.
- **Fix:** Shadowed the `render` import in each file with a local wrapper (`render(ui) { return rtlRender(<I18nProvider locale="en" setLocale={() => {}}>{ui}</I18nProvider>) }`), and wrapped the handful of direct `rerender(...)` call sites the same way.
- **Files modified:** `CommandBar.test.tsx`, `TagDialog.test.tsx`, `FavoriteAddDialog.test.tsx`, `MediaAddDialog.test.tsx`, `EditPreviewDialog.test.tsx`, `FoldMergeDialog.test.tsx`, `RecordEditor.test.tsx`, `CategorySwitcher.test.tsx`, `CategoryList.test.tsx`, `ColorMenu.test.tsx`, `UtilitiesMenu.test.tsx`, `ErrorBanner.test.tsx`
- **Verification:** Full `npx vitest run` green (207/207) after each task's changes.
- **Committed in:** `946be043` (Task 1's affected files), `5b7e57cb` (Task 2's affected files)

**3. [Rule 3 - Blocking] Fixed the `describeError(err, t)` signature change's ripple into two test files**
- **Found during:** Task 1's own verification, after `describeError`'s signature changed
- **Issue:** `ErrorBanner.test.tsx` and `FavoriteAddDialog.test.tsx` both call `describeError(...)` directly with the old one-argument signature (found by the plan's own mandated whole-tree grep for `describeError(`) — a compile error under the new signature.
- **Fix:** Added a local `realT` helper (a real `t` built from the actual `en` catalog, matching `I18nContext.tsx`'s own substitution algorithm — not a mock that trivially echoes the key) to each file and passed it to every `describeError(...)` call.
- **Files modified:** `ErrorBanner.test.tsx`, `FavoriteAddDialog.test.tsx`
- **Verification:** `npx tsc --noEmit` clean; both test files' assertions on the resolved sentence text pass unmodified.
- **Committed in:** `946be043` (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (1 Rule 2 - missing critical functionality, 2 Rule 3 - blocking)
**Impact on plan:** All three were mechanically necessary for the plan's own stated goals (full `describeError` coverage; every test file compiling and passing under the new `useI18n()`/`describeError(err, t)` contracts) — no scope creep, no architectural change.

## Issues Encountered
- The initial `completeness.test.ts`/`errors.test.ts` drafts had two mechanical bugs, both caught and fixed by the plan's own verification loop before commit: (1) the Rust-source tuple-extraction regex didn't account for the multi-line arm shape's trailing comma before the closing paren (missed 4 of 35 `error.rs` codes until fixed — `\(\s*"([a-z_]+)"\s*,\s*"([a-z_.]+)"\s*,?\s*\)`); (2) the first "scan genuinely guards" demonstration tampered a literal INSIDE a `{ternary}` JS expression slot, which the scan correctly (by design) strips before scanning — the demo was rewritten to tamper a bare JSX attribute literal instead, matching how 11-03's own demo worked. Neither bug reached a commit; both were resolved via the plan's own iterate-until-green verification step.
- Pre-existing `ts-rs` "failed to parse serde attribute" warnings during `cargo test`/`cargo clippy` (unrelated `try_from = "Vec<i64>"` attributes on existing types) are unchanged from before this plan and out of scope per the SCOPE BOUNDARY rule — same warnings 11-03's SUMMARY recorded as pre-existing.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- PLAT-03 ("all user-facing strings are localized") is now complete across the entire running app, not just the App shell and Settings dialog — the 8 non-English locale files remain deliberately empty/scaffolded (D11-02), falling back to English for every key.
- Manual verification of the live Tauri app (launch with an archive open, trigger a merge dry-run preview and an error banner, open Settings, switch language, confirm the command bar/open dialog/error banner all re-render legibly with no reload and no blank/undefined text) was NOT performed in this headless execution environment — flagged in `coverage:` D4 as needing human judgment, matching 11-01/11-03's identical caveat pattern for their own manual-verification items.
- No further i18n-architecture work is anticipated for this phase; any future phase adding a new component should follow the established `t()`/`{token}` convention and extend `app/src/i18n/completeness.test.ts`'s `SOURCES`/`ALLOWLIST` maps with the new file.

---
*Phase: 11-platform-polish*
*Completed: 2026-08-16*

## Self-Check: PASSED
All created/modified files verified present on disk; all three task commit hashes (`946be043`, `5b7e57cb`, `75a677d4`) verified present in git log.
