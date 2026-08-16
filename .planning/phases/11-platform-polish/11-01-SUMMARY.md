---
phase: 11-platform-polish
plan: 01
subsystem: ui
tags: [tauri, react, theme, settings, persistence, css-custom-properties, ts-rs]

# Dependency graph
requires:
  - phase: 01-open-view-save-foundation-slice
    provides: app_version-at-runtime pattern (env!("CARGO_PKG_VERSION")), app/src/styles.css token system, Tauri command/ErrorDto conventions
provides:
  - "load_settings/save_settings Tauri command pair, app-data-dir-scoped, disjoint from ArchiveSession"
  - "app_version command (first callable version command)"
  - "SettingsProvider React context (persisted {language, theme}, single write-through path)"
  - "ThemeContext (data-theme DOM attribute effect)"
  - "light theme CSS token block, [data-theme='light']"
  - "SettingsDialog (theme switcher + About + language slot for plan 11-03)"
affects: [11-03-i18n-layer, 11-04-locale-catalog]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Directory-taking core + AppHandle-resolving command wrapper (settings.rs), mirrors the Phase 5/10 merge-core public-for-testing pattern"
    - "Load/save Result asymmetry: load_settings infallible (degrades to defaults), save_settings returns Result<(), ErrorDto> (failure surfaced)"
    - "React functional setState updater with the IPC write-through call issued FROM INSIDE the updater, so back-to-back state changes in one tick never lose a field"
    - "CSS-only theme switch via :root[data-theme] attribute selector, zero JS-computed colour"

key-files:
  created:
    - app/src-tauri/src/settings.rs
    - app/src-tauri/tests/settings_persistence.rs
    - app/src/settings/SettingsProvider.tsx
    - app/src/settings/SettingsProvider.test.tsx
    - app/src/theme/ThemeContext.tsx
    - app/src/theme/ThemeContext.test.tsx
    - app/src/theme/styles_tokens.test.ts
    - app/src/components/SettingsDialog.tsx
    - app/src/components/SettingsDialog.test.tsx
    - app/src/bindings/AppSettings.ts
    - app/src/bindings/Theme.ts
    - app/src/vite-env.d.ts
  modified:
    - app/src-tauri/src/lib.rs
    - app/src/App.tsx
    - app/src/main.tsx
    - app/src/styles.css
    - app/src/lib/errors.ts
    - app/vitest.config.ts

key-decisions:
  - "SettingsProvider is lifted above App in main.tsx (not nested inside App) so its own ErrorBanner instance can surface a rejected save_settings without App.tsx needing to know settings persistence exists."
  - "Language default is the locale code \"en\" (not the display name \"English\") since plan 11-03's I18nContext will key its catalog by locale code."
  - "The concurrent write-through correctness proof (theme+language in one tick) is satisfied by firing the save_settings invoke call from INSIDE the setState functional updater, not by reading state a second time after calling setState."

patterns-established:
  - "Settings-domain errors get their own thiserror enum (SettingsError) mapped to the shared ErrorDto shape via to_dto, never reusing ArchiveError -- same two-layer error convention, new domain."

requirements-completed: [PLAT-04]

coverage:
  - id: D1
    description: "Theme switches instantly across the whole app via CSS-only :root[data-theme] cascade, zero JS-computed colour"
    requirement: "PLAT-04"
    verification:
      - kind: unit
        ref: "app/src/theme/ThemeContext.test.tsx#ThemeContext — DOM data-theme attribute effect (D11-03)"
        status: pass
      - kind: unit
        ref: "app/src/theme/styles_tokens.test.ts#styles_tokens — dark/light colour token parity (D11-03)"
        status: pass
    human_judgment: true
    rationale: "Automated tests prove the DOM-attribute mechanism and token-name parity, but the actual visual repaint in a running Tauri window was not driven in this headless execution environment -- a human should confirm the live app repaints instantly with no reload."
  - id: D2
    description: "Theme + language choice persists across app restart via a Rust-side settings.json under Tauri's app-data directory"
    requirement: "PLAT-04"
    verification:
      - kind: integration
        ref: "app/src-tauri/tests/settings_persistence.rs#settings_round_trip"
        status: pass
      - kind: unit
        ref: "app/src/settings/SettingsProvider.test.tsx#SettingsProvider — write-through (D11-04)"
        status: pass
    human_judgment: true
    rationale: "Round-trip and write-through are proven by test against tempdir/mocked IPC, but a real app-restart cycle against the OS app-data directory was not exercised in this headless environment."
  - id: D3
    description: "Missing/corrupt/truncated settings file degrades silently to English+Dark defaults, never blocking startup or showing an error"
    requirement: "PLAT-04"
    verification:
      - kind: integration
        ref: "app/src-tauri/tests/settings_persistence.rs#settings_missing_file_returns_defaults"
        status: pass
      - kind: integration
        ref: "app/src-tauri/tests/settings_persistence.rs#settings_corrupt_file_returns_defaults"
        status: pass
      - kind: integration
        ref: "app/src-tauri/tests/settings_persistence.rs#settings_truncated_file_returns_defaults"
        status: pass
      - kind: unit
        ref: "app/src/settings/SettingsProvider.test.tsx#SettingsProvider — defaults on load rejection (D11-04)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Settings domain is structurally disjoint from archive/session state (Core Value adjacency)"
    requirement: "PLAT-04"
    verification:
      - kind: integration
        ref: "app/src-tauri/tests/settings_persistence.rs#settings_source_has_no_archive_references"
        status: pass
    human_judgment: false
  - id: D5
    description: "About region shows the runtime crate version via a new app_version command, never a literal"
    verification:
      - kind: unit
        ref: "app/src/components/SettingsDialog.test.tsx#renders a version string sourced from the runtime app_version command, never a literal"
        status: pass
    human_judgment: false

# Metrics
duration: 40min
completed: 2026-08-16
status: complete
---

# Phase 11 Plan 1: Theme End to End Summary

**PLAT-04 delivered end to end: a new Rust settings.rs command pair (app-data-dir-scoped, disjoint from ArchiveSession), a SettingsProvider/ThemeContext React layer with a single write-through path, a CSS-only light/dark token switch, and a SettingsDialog hosting it plus a runtime-version About region.**

## Performance

- **Duration:** ~40 min
- **Tasks:** 3 (tracer + 2 TDD-gated)
- **Files modified/created:** 18

## Accomplishments
- Theme switches instantly across the whole app via `:root[data-theme="light"]` CSS cascade -- zero JS-computed colour, proven by both an automated DOM-attribute test and a demonstrated red/green token-parity check.
- Theme + language choice persists to `settings.json` under Tauri's OS-managed app-data directory (`load_settings`/`save_settings`), provably isolated from `ArchiveSession`/archive state by a structural source-scan test.
- Missing, corrupt, and truncated settings files all degrade silently to English + Dark -- never blocking startup, never surfacing an error -- proven at both the Rust integration-test layer and the React unit-test layer.
- A failed *save* IS surfaced (Core Value: never silently drop a user's choice), routed through the app's existing `ErrorBanner`/`describeError` mechanism via a new `SettingsError -> ErrorDto` mapping.
- Concurrent `setTheme`/`setLanguage` calls in the same tick are proven never to drop a field, via a `setState` functional-updater pattern that fires the `save_settings` IPC call from inside the updater itself.
- `app_version` is the first callable, registered Tauri command exposing the runtime crate version; `SettingsDialog`'s About region uses it, never a literal.
- Zero new Cargo/npm dependencies across all 3 tasks (`git diff` on every manifest/lockfile empty throughout).

## Task Commits

Each task was committed atomically:

1. **Task 1: Theme end to end (tracer)** - `4aa444f3` (feat)
2. **Task 2 RED: failing settings-persistence test** - `31e31010` (test)
2. **Task 2 GREEN: extracted directory-taking helpers** - `f0b8c463` (feat)
3. **Task 3: frontend theme tests** - `37155847` (test)

_Note: Task 3 has no separate GREEN commit -- see TDD Gate Compliance below._

## Files Created/Modified
- `app/src-tauri/src/settings.rs` - `AppSettings`/`Theme`/`SettingsError`, directory-taking helpers, `load_settings`/`save_settings` commands
- `app/src-tauri/tests/settings_persistence.rs` - 6 named behaviours + 1 structural archive-isolation test, all passing
- `app/src-tauri/src/lib.rs` - `settings` module declaration, `app_version` command, 3 new `generate_handler!` entries
- `app/src/settings/SettingsProvider.tsx` - loads once on mount, single `updateSettings` write-through path, its own `ErrorBanner` for save failures
- `app/src/theme/ThemeContext.tsx` - controlled `{theme, setTheme}`, the sole `data-theme` DOM-attribute effect
- `app/src/components/SettingsDialog.tsx` - theme switcher, About (runtime version), labelled non-functional language slot for plan 11-03
- `app/src/styles.css` - `:root[data-theme="light"]` block (same 8 colour tokens, sourced from `res/light.qss`), `.app-header`/`.settings-dialog-*` classes
- `app/src/App.tsx` - "Settings…" affordance (CommandBar itself is out of this plan's file scope) opening `SettingsDialog`
- `app/src/main.tsx` - `SettingsProvider` wraps `App`
- `app/src/lib/errors.ts` - `describeError` cases for the 4 new `settings_*` codes
- `app/src/bindings/AppSettings.ts`, `app/src/bindings/Theme.ts` - ts-rs generated bindings
- `app/src/vite-env.d.ts` (new, not in original file list) - ambient `*.css?raw` declaration
- `app/vitest.config.ts` - `test.css.include` scoped to `styles.css` so the raw import isn't stubbed empty

## Decisions Made
- `SettingsProvider` sits above `App` in `main.tsx` (per plan's explicit "lift the provider above App" instruction) so its own `ErrorBanner` reuses the existing `describeError` path without `App.tsx` needing any settings-awareness.
- `updateSettings`'s `save_settings` IPC call is fired from *inside* the `setState` functional updater (not after a second read of `settings`) -- the only implementation that provably survives React's queued-updater semantics for the concurrent-write-through requirement.
- Default `language` is the locale code `"en"`, not a display name, anticipating plan 11-03's locale-code-keyed catalog.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `app/src/vite-env.d.ts` added; `app/vitest.config.ts` `test.css` scoped**
- **Found during:** Task 3 (token-parity test)
- **Issue:** The plan's token-parity test needs to read `styles.css` at test time. This project has no `@types/node` (and adding it would violate the zero-new-dependency constraint), so `node:fs`/`node:path`/`node:url` fail `tsc --noEmit` with `TS2307`. Vite's built-in `?raw` import needed an ambient module declaration to type-check, and Vitest's default CSS-import stub (to speed up tests) returned an empty string for the raw import.
- **Fix:** Added a local ambient declaration (`declare module "*.css?raw"`) in a new `app/src/vite-env.d.ts`, and scoped `vitest.config.ts`'s `test.css.include` to `styles.css` only (not a blanket `css: true`, to minimize the perf/behavior footprint on the rest of the suite).
- **Files modified:** `app/src/vite-env.d.ts` (new), `app/vitest.config.ts`
- **Verification:** `npx tsc --noEmit` clean, `npx vitest run` green (176/176), `npm run build` clean, zero new Cargo/npm dependencies (`git diff` on manifests/lockfiles empty).
- **Committed in:** `37155847` (Task 3 commit)

---

**Total deviations:** 1 auto-fixed (Rule 3 - blocking, zero-dependency-preserving)
**Impact on plan:** Necessary to satisfy the plan's own required verification commands (`npx tsc --noEmit`, `npx vitest run`) without adding a dependency. No scope creep beyond the two small config/declaration files.

## TDD Gate Compliance

- **Task 2** followed the full RED -> GREEN cycle: `31e31010` (test) added `settings_persistence.rs` importing `load_settings_from_dir`/`save_settings_to_dir`, which did not exist yet -- confirmed genuine RED via `E0432: unresolved import` (a compile failure, the Rust-equivalent of a failing test). `f0b8c463` (feat) extracted the directory-taking helpers from Task 1's `settings.rs`, turning all 7 tests green.
- **Task 3** has NO separate RED commit. Task 1 (the tracer) already implemented `ThemeContext`, `SettingsProvider`, and `SettingsDialog` completely and correctly as part of proving PLAT-04 end to end -- that is what a tracer task IS. When Task 3's 4 test files were written against that already-complete implementation, all 12 tests passed on the first run; there was no failing state to commit separately from the passing one. This is an inherent, expected property of a tracer-first plan structure (Task 1 explicitly typed `tracer`, not `auto`/`tdd`), not a shortcut -- the fail-fast rule ("a test that passes unexpectedly during RED means the test may be wrong") does not apply here, since the reason for the pass is fully understood (the tracer built the real thing) and independently confirmed by demonstrating the token-parity test genuinely goes red when a token is removed from one theme block (see Acceptance Criteria below).
- All Rust tests pass (`cargo test --jobs 2`, full suite, 0 failed), `cargo clippy --jobs 2 --all-targets -- -D warnings` clean, no `unwrap`/`expect`/`panic` in `settings.rs` non-test code.

## Issues Encountered
- `cargo fmt --check` reports extensive PRE-EXISTING formatting drift across the codebase (dozens of files: `lib.rs`, `db/io/export.rs`, `archive/merge.rs`, test files, etc.) unrelated to this plan's changes. Confirmed `settings.rs` and `settings_persistence.rs` (both new files) are NOT in the diff list, and the diff hunks in `lib.rs` do not touch the lines this plan added (`mod settings;`, `app_version`, the 3 new `generate_handler!` entries). Per the SCOPE BOUNDARY rule (only auto-fix issues directly caused by this task's changes), this pre-existing drift was left untouched and is not this plan's regression.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `SettingsProvider`/`AppSettings.language` is wired and persisted but not yet consumed -- plan 11-03's `I18nContext` is its first real consumer, exactly as the plan's `key_links` anticipated.
- `SettingsDialog`'s labelled, non-functional language slot is ready for plan 11-03 to fill with a real `<select>`.
- Manual verification of the live Tauri app (open settings, flip to light, confirm instant repaint; restart, confirm persistence; corrupt the settings file, confirm silent degradation) was NOT performed in this headless execution environment -- flagged in `coverage:` D1/D2 as needing human judgment. All underlying mechanisms are proven by automated test at both the Rust and React layers.

---
*Phase: 11-platform-polish*
*Completed: 2026-08-16*

## Self-Check: PASSED
All created files verified present on disk; all 4 task commit hashes (`4aa444f3`, `31e31010`, `f0b8c463`, `37155847`) verified present in git log.
