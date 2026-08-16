---
phase: 11-platform-polish
plan: 03
subsystem: ui
tags: [i18n, react-context, tauri, localization, zero-dependency]

# Dependency graph
requires:
  - phase: 11-platform-polish
    plan: 01
    provides: "SettingsProvider's updateSettings(patch) functional write-through, ThemeProvider's controlled-component shape to mirror, AppSettings.language persisted end to end"
provides:
  - "I18nProvider/useI18n -- dependency-free TypeScript catalog + React context, catalogs[locale]?.[key] ?? en[key] fallback mechanism"
  - "9 locale files (en complete, 8 scaffolded empty) keyed by StringKey = keyof typeof en"
  - "Functional language <select> in SettingsDialog, persisted through 11-01's existing settings.json"
  - "Split-around-JSX mixed-markup convention (bodyBefore/<code>/bodyAfter) for plan 11-04 to reuse"
  - "Structural source-scan test pattern (return-block extraction + brace-strip) for retrofit completeness, reusable by plan 11-04"
affects: [11-04-locale-catalog]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Controlled I18nProvider (locale/setLocale as props, no own state) nested inside ThemeProvider, mirroring ThemeProvider's own controlled shape exactly"
    - "catalogs[locale]?.[key] ?? en[key] -- the ONE fallback expression covering both a missing key in a real locale and a wholly unrecognized locale code"
    - "Return-block extraction (paren-balance) + brace-strip source scan for structural JSX-literal-completeness tests, parallel to 11-01's extractBlock (brace-balance) technique in styles_tokens.test.ts"

key-files:
  created:
    - app/src/i18n/strings.ts
    - app/src/i18n/en.ts
    - app/src/i18n/de.ts
    - app/src/i18n/es.ts
    - app/src/i18n/fr.ts
    - app/src/i18n/it.ts
    - app/src/i18n/pl.ts
    - app/src/i18n/pt.ts
    - app/src/i18n/ru.ts
    - app/src/i18n/uk.ts
    - app/src/i18n/locales.ts
    - app/src/i18n/I18nContext.tsx
    - app/src/i18n/I18nContext.test.tsx
  modified:
    - app/src/settings/SettingsProvider.tsx
    - app/src/components/SettingsDialog.tsx
    - app/src/components/SettingsDialog.test.tsx
    - app/src/App.tsx
    - app/src/App.test.tsx

key-decisions:
  - "ThemeProvider is the OUTER of the two providers, I18nProvider nests INSIDE it (matches the plan's literal instruction). saveError's ErrorBanner moved inside I18nProvider's subtree, alongside {children}, so plan 11-04's t()-aware describeError retrofit covers it too."
  - "SettingsDialog's setLocale comes directly from useI18n() (== SettingsProvider's setLanguage, passed through as I18nProvider's setLocale prop) -- a genuinely thin call-through, zero new persistence logic."
  - "The empty-state sentence's <code>.jwlibrary</code> segment is kept on a single JSX line ({bodyBefore}<code>.jwlibrary</code>{bodyAfter}) rather than split across lines, to avoid relying on JSX's whitespace-collapse rules for the exact rendered string the completeness test asserts against."

patterns-established:
  - "Structural completeness test: extract the `return ( ... )` JSX block by paren-balance, strip {...} expression slots by brace-balance, then regex-scan for non-empty JSX text nodes and aria-label/title/placeholder literal attributes. Duplicated (not shared) across App.test.tsx and SettingsDialog.test.tsx per each file's own allowlist -- plan 11-04 will need the same technique for its 13 components."

requirements-completed: [PLAT-03]

coverage:
  - id: D1
    description: "StringKey is derived (keyof typeof en); the 8 non-English locales type-check as Partial<Record<StringKey,string>> and are empty"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "npx tsc --noEmit (all 9 locale files + strings.ts)"
        status: pass
    human_judgment: false
  - id: D2
    description: "catalogs[locale]?.[key] ?? en[key] resolves correctly for a missing key in a real locale AND a wholly unregistered locale code, never throwing"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/i18n/I18nContext.test.tsx#I18nContext fallback"
        status: pass
      - kind: unit
        ref: "app/src/i18n/I18nContext.test.tsx#I18nContext unknown locale"
        status: pass
    human_judgment: false
  - id: D3
    description: "{token} param substitution replaces a matched token and leaves an unmatched one literal"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/i18n/I18nContext.test.tsx#I18nContext param substitution"
        status: pass
    human_judgment: false
  - id: D4
    description: "The language select persists through 11-01's existing updateSettings/save_settings write-through path and re-renders visible text on the same interaction"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/components/SettingsDialog.test.tsx#SettingsDialog language switch"
        status: pass
    human_judgment: true
    rationale: "The write-through and same-tick re-render are proven against mocked IPC; a real app restart cycle confirming the choice survives was not exercised in this headless environment (same caveat 11-01 recorded for D2)."
  - id: D5
    description: "App.tsx and SettingsDialog.tsx render exclusively through t(), except the product name and the <code> element's own text"
    requirement: "PLAT-03"
    verification:
      - kind: unit
        ref: "app/src/App.test.tsx#App structural completeness"
        status: pass
      - kind: unit
        ref: "app/src/components/SettingsDialog.test.tsx#SettingsDialog structural completeness"
        status: pass
    human_judgment: false

# Metrics
duration: ~25min
completed: 2026-08-16
status: complete
---

# Phase 11 Plan 3: i18n Architecture Summary

**A dependency-free TypeScript `Record<string,string>` catalog + React context (`I18nProvider`/`useI18n`), English complete by construction, 8 locales scaffolded empty, wired end to end through a working, persisted language switcher and a full retrofit of App.tsx + SettingsDialog.tsx.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2 (tracer + TDD-gated tests)
- **Files created/modified:** 18

## Accomplishments
- `StringKey = keyof typeof en` is derived, never a separately hand-maintained union -- referencing a key not in `en` is a compile error, and every locale file's type-checks against this same union (proven structurally by `npx tsc --noEmit`, no runtime scan needed for that half of the completeness guarantee).
- `catalogs[locale]?.[key] ?? en[key]` is the ONE fallback expression, proven to cover both a missing key in a real (empty scaffolded) locale and a wholly unregistered locale code, without throwing in either case.
- A working, functional language `<select>` in `SettingsDialog` lists all 9 locales by native name (English, Deutsch, Español, Français, Italiano, Polski, Português, Русский, Українська), replaces 11-01's "coming soon" placeholder, and persists through 11-01's existing `updateSettings`/`save_settings` write-through with zero new Tauri commands and zero Rust files touched.
- App.tsx's shell chrome and SettingsDialog's own strings render exclusively through `t()`, including the one mixed-markup sentence (the `.jwlibrary` empty-state line), split via the `bodyBefore`/literal-`<code>`/`bodyAfter` convention plan 11-04 will reuse for CommandBar's JSX-embedded summary sentences.
- `{token}` param substitution (used by `settings.versionLine`'s `{version}`) is a single bounded regex pass; an unmatched token is left literal rather than throwing.
- Two structural source-scan tests (App.tsx, SettingsDialog.tsx) prove zero stray hardcoded JSX-text or `aria-label`/`title`/`placeholder` literals remain outside `t()` calls -- each demonstrated red (stray string added) then green (removed) per the acceptance criteria.
- Zero new npm/Cargo dependencies (`git diff` on all manifests/lockfiles empty throughout both tasks); zero Rust files touched.

## Task Commits

Each task was committed atomically:

1. **Task 1: i18n catalog + context + language switcher (tracer)** - `e5dcc7b6` (feat)
2. **Task 2: i18n context tests** - `e16db3e9` (test)

## Files Created/Modified
- `app/src/i18n/strings.ts` - `StringKey = keyof typeof en`
- `app/src/i18n/en.ts` - the one complete catalog (12 keys: `app.*` x4, `settings.*` x8)
- `app/src/i18n/{de,es,fr,it,pl,pt,ru,uk}.ts` - 8 scaffolded, empty `Partial<Record<StringKey,string>>` catalogs, identically commented, deliberately NOT machine-translated
- `app/src/i18n/locales.ts` - `SUPPORTED_LOCALES` (native names, fixed order)
- `app/src/i18n/I18nContext.tsx` - `I18nProvider`/`useI18n`, the fallback + param-substitution mechanism
- `app/src/i18n/I18nContext.test.tsx` - fallback, unknown-locale, param-substitution, throw-outside-provider tests
- `app/src/settings/SettingsProvider.tsx` - nests `I18nProvider` inside `ThemeProvider`; moved `saveError`'s `ErrorBanner` inside `I18nProvider`'s subtree
- `app/src/components/SettingsDialog.tsx` - functional language `<select>`, full string retrofit through `t()`
- `app/src/components/SettingsDialog.test.tsx` - language-switch behavior test + structural completeness scan
- `app/src/App.tsx` - shell chrome retrofit through `t()`, mixed-markup `.jwlibrary` sentence split
- `app/src/App.test.tsx` - app-shell-retrofit behavior test + structural completeness scan; also wraps all `render(<App />)` call sites in `SettingsProvider` (Rule 3 fix, see Deviations)

## Decisions Made
- `ThemeProvider` stays the outer provider; `I18nProvider` nests inside it, per the plan's literal instruction -- `locale={settings.language}`/`setLocale={setLanguage}` passed as controlled props, mirroring `ThemeProvider`'s own `theme`/`setTheme` shape exactly.
- The `saveError && <ErrorBanner />` line moved from being a sibling of `ThemeProvider` to being inside `I18nProvider`'s subtree (alongside `{children}`), so every `ErrorBanner` instance in the tree -- including this provider's own -- sits below `I18nProvider`, which plan 11-04's `describeError` retrofit depends on.
- `SettingsDialog`'s `<select onChange>` calls `setLocale` obtained directly from `useI18n()` (which is exactly `setLanguage`, passed through as `I18nProvider`'s `setLocale` prop) -- no new call-through logic, just the plan's literal wiring instruction.
- Kept `{bodyBefore}<code>.jwlibrary</code>{bodyAfter}` on one JSX line (not split across lines as originally drafted) to avoid depending on JSX's whitespace-collapse-at-tag-boundary rule for the exact concatenated string the retrofit-completeness test asserts against character-for-character.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `App.test.tsx` did not wrap `<App />` in `SettingsProvider`**
- **Found during:** Task 1 (running the plan's own required `npx vitest run` verification)
- **Issue:** App.tsx's new `useI18n()` call throws `"useI18n must be used within an I18nProvider"` when rendered without a `SettingsProvider` (which nests `I18nProvider`) in the tree. `App.test.tsx` previously rendered `<App />` bare in all three of its render call sites, since App.tsx had no settings/i18n dependency before this plan. This is not in Task 1's declared `<files>` list, but leaving it unfixed would have failed Task 1's own mandated verification (`npx vitest run` as part of `npx tsc --noEmit && npx vitest run && npm run build`).
- **Fix:** Wrapped all three `render(<App />)` call sites (`openArchive()`'s helper, and the standalone empty-state-shell test) in `<SettingsProvider>`, and added `load_settings`/`save_settings` invoke-mock cases so `SettingsProvider`'s own mount-time `load_settings` call resolves to valid `AppSettings` rather than silently degrading to defaults via its existing catch-and-default behavior (11-01's `SettingsProvider`).
- **Files modified:** `app/src/App.test.tsx`
- **Verification:** `npx vitest run` (176/176) green immediately after the fix, before Task 2 added any new tests.
- **Committed in:** `e5dcc7b6` (Task 1 commit, documented there rather than deferred to Task 2's commit since it was required for Task 1's own verify command to pass)

---

**Total deviations:** 1 auto-fixed (Rule 3 - blocking, required for the plan's own Task 1 verification to pass)
**Impact on plan:** `App.test.tsx` is officially Task 2's file per the plan's task-level `<files>` lists, but the fix was mechanically necessary the moment Task 1's App.tsx retrofit landed. Task 2 then extended the same (already-fixed) file with its own new named-behavior tests, exactly as its `<action>`'s "extend, do not replace" instruction anticipated.

## TDD Gate Compliance

- **Task 2** has NO separate RED commit. Task 1 (`type="tracer"`) already implemented the full `I18nProvider`/`useI18n` mechanism, the 9 locale files, the language switcher, and the complete App.tsx/SettingsDialog.tsx retrofit as part of proving PLAT-03 end to end -- that is what a tracer task IS. When Task 2's test files were written against that already-complete implementation, all new tests passed on first run; there was no failing state to commit separately. This is the identical, expected property 11-01-PLAN.md's own Task 3 documented for the same tracer-first plan structure, and the same fail-fast caveat applies: it does not apply here because the reason for the pass is fully understood (the tracer built the real thing), independently confirmed by demonstrating BOTH structural-completeness tests genuinely go red when a stray hardcoded string is (temporarily) reintroduced into the scanned file -- see Acceptance Criteria evidence below.
- `npx tsc --noEmit`, `npx vitest run` (184/184, full suite), `npm run build`, `cargo test --jobs 2` (all Rust tests green, 0 Rust files touched), and `cargo clippy --jobs 2 --all-targets -- -D warnings` (clean) all pass.

## Acceptance Criteria Evidence
- Demonstrated red/green for the App.tsx structural completeness test: temporarily appended `" Stray hardcoded text"` inside the Settings button's JSX text node, ran `npx vitest run src/App.test.tsx -t "structural completeness"` -- failed with `JSX text node: "Stray hardcoded text"` reported as expected -- then reverted the change and re-ran the full suite (184/184 green). No trace of the temporary string remains in the committed diff.
- `git diff app/package.json app/package-lock.json app/src-tauri/Cargo.toml app/src-tauri/Cargo.lock` is empty across both task commits -- zero new dependency, and no Rust file appears in either diff at all.
- `cargo test --jobs 2` (full Rust suite) and `cargo clippy --jobs 2 --all-targets -- -D warnings` both run clean as a regression check, confirming this plan genuinely touched no Rust file.

## Issues Encountered
None beyond the Rule 3 fix documented above. Pre-existing `ts-rs` "failed to parse serde attribute" warnings during `cargo test`/`cargo clippy` (unrelated `try_from = "Vec<i64>"` attributes on existing types) are unchanged from before this plan and out of scope per the SCOPE BOUNDARY rule (not caused by this plan's changes).

## User Setup Required
None -- no external service configuration required.

## Next Phase Readiness
- Plan 11-04 has the exact API this plan ships: `t(key, params?)`/`useI18n()`, `StringKey`, `SUPPORTED_LOCALES`, and the split-around-JSX mixed-markup convention proven on App.tsx's empty-state sentence.
- Plan 11-04's own completeness test (per its scope boundary) can reuse the return-block-extraction + brace-strip structural-scan technique established here for its 13 remaining components plus `lib/errors.ts`'s `describeError` copy.
- Manual verification of the live Tauri app (open Settings, cycle through all 9 locales, confirm the App shell and Settings dialog text stays legible via English fallback for every non-English selection, confirm the selection survives a restart) was NOT performed in this headless execution environment -- flagged in `coverage:` D4 as needing human judgment, matching 11-01's identical caveat pattern for its own manual-verification items.

---
*Phase: 11-platform-polish*
*Completed: 2026-08-16*

## Self-Check: PASSED
All created files verified present on disk; both task commit hashes (`e5dcc7b6`, `e16db3e9`) verified present in git log.
