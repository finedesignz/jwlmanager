---
phase: 01-open-view-save-foundation-slice
plan: 06
subsystem: frontend-shell
tags: [react, typescript, tauri-ipc, error-surface, vitest]

requires:
  - phase: 01-05
    provides: save_archive/save_as/new_archive Tauri commands (no-arg save, path-arg save-as/new)
  - phase: 01-03
    provides: check_jwlcore command + unified JwlCoreStatus { loaded, arch, version, reason }
  - phase: 01-07
    provides: ErrorDto { code, operation, safe_file_name, message_key } — the only error shape crossing IPC; open_archive
  - phase: 01-04
    provides: NotesList this plan sits above unchanged
provides:
  - CommandBar.tsx wiring Open/New/Save/Save As to the real IPC commands with pending/cancel/double-click discipline
  - lib/errors.ts ErrorDto code -> actionable sentence mapping (SAFE-05)
  - ErrorBanner.tsx sticky error surface with distinct zip-slip security copy
  - JwlCoreNotice.tsx informational arm64 capability notice (D-13a)
  - App.tsx composed shell (CommandBar + JwlCoreNotice + ErrorBanner + NotesList/empty-state)
affects: []

tech-stack:
  added: []
  patterns:
    - "Double-click guard via a synchronous ref (not React state): the guard check and set happen before any await, so a second click dispatched in the same tick as the first (before React re-renders the disabled button) is still caught"
    - "Cancel = dismissed native dialog: open()/save() resolving null/non-string is the cancel affordance for file actions (per 01-06-PLAN.md's own action text), not a separate abort/cancel button — no in-flight-invoke abort mechanism exists for these near-instant local filesystem operations"
    - "ErrorDto code (snake_case string emitted by ArchiveError::to_dto, e.g. \"not_a_zip\") is the sentence-mapping key, never message_key or a raw Display string"
    - "JwlCoreStatus.loaded === false renders the notice; a genuine check_jwlcore Err (or loaded === true) never does — Ok/non-loaded and Err are kept strictly distinct in the UI, matching 01-03's own Ok-not-Err design for the arm64 case"

key-files:
  created:
    - app/src/components/CommandBar.tsx
    - app/src/components/CommandBar.test.tsx
    - app/src/components/ErrorBanner.tsx
    - app/src/components/ErrorBanner.test.tsx
    - app/src/components/JwlCoreNotice.tsx
    - app/src/lib/errors.ts
  modified:
    - app/src/App.tsx
    - app/src/App.test.tsx
    - app/src/styles.css

key-decisions:
  - "No shadcn/Lucide dependency added: 01-UI-SPEC.md documents shadcn init as deferred and app/ has no components.json; 01-01's hand-authored CSS-token stylesheet is the standing substitute (per this plan's own non-negotiables), so CommandBar/ErrorBanner/JwlCoreNotice use plain semantic HTML + the existing .toolbar-button/.error-banner CSS classes, matching App.tsx's pre-existing pattern exactly rather than introducing a new component registry mid-plan"
  - "save_archive takes no path argument (session already tracks target_path per 01-05); Save invokes it bare, while New/Save As invoke the native save() dialog first to obtain a path, matching each command's real Rust signature"
  - "new_archive's Ok(()) return carries no notes payload, so the frontend optimistically sets notes to [] on success (a brand-new archive genuinely has zero notes) rather than issuing a second query round-trip"
  - "Empty-state no longer duplicates Open/New Archive buttons — CommandBar is the single source of those actions now that it exists; the empty-state body text still names both actions per the UI-SPEC copy contract"

requirements-completed: [SAFE-05, DATA-01]

metrics:
  duration: "~50 minutes"
  completed: "2026-07-19"
---

# Phase 1 Plan 6: Command Bar + Typed Error Surface + jwlCore Capability Notice Summary

Closed the Walking Skeleton's UI surface: a command bar wiring Open/New/Save/Save As to the real Tauri IPC commands with explicit pending state, a synchronous double-click guard, and clean-cancel handling for dismissed native dialogs; an error surface that maps every sanitized `ErrorDto` to the specific actionable sentence from the UI-SPEC copywriting contract (with a distinct zip-slip security message); and an informational jwlCore capability notice that renders only when the unified `JwlCoreStatus.loaded` is `false`, never as a red error.

## What Was Built

**Task 1 — `CommandBar.tsx`:** Four actions (Open/New/Save/Save As) each invoke their real backend command (`open_archive`, `new_archive`, `save_archive`, `save_as`) — New and Save As first resolve a path via the native `save()` dialog, Open via `open()`, Save takes no argument (uses the session's tracked target path). A `runAction` wrapper enforces: a `busyRef` (plain `useRef`, not state) checked and set synchronously before any `await`, so a second click dispatched in the same tick as the first is caught even before React re-renders the disabled button — proven by a test that fires two clicks while the dialog promise is still pending and asserts the dialog opener was called exactly once. Pending state disables all four buttons and swaps the clicked one's label to a verb-ing form ("Opening…", "Saving…", etc.). Save/Save As are disabled outright when no archive is open. A dismissed dialog (`open()`/`save()` resolving `null`) calls `onCancelled()` and never touches `invoke` or `onError` — proven by a test asserting zero `invoke` calls and no error callback.

**Task 2 — `lib/errors.ts`, `ErrorBanner.tsx`, `JwlCoreNotice.tsx`:** `describeError()` switches on the real snake_case `code` string `ArchiveError::to_dto` emits (`not_a_zip`, `zip_slip_rejected`, `unsupported_schema`, etc. — read directly from `error.rs`, not guessed) and returns the what-happened/why/next-step sentence from the UI-SPEC. Every known code maps to a distinct, non-blank sentence (tested); an unrecognized code still gets a generic actionable fallback, never a blank string or the bare code. `ErrorBanner.tsx` renders the mapped sentence plus `safe_file_name` when present, is sticky by construction (no timer/effect clears it — `App.tsx` only clears `error` on the next user action), and the zip-slip code gets a distinct `error-banner-security` class and copy (tested to differ from the generic open-failure sentence). `JwlCoreNotice.tsx` calls `check_jwlcore` once on mount and renders the muted, dismissible notice **only** when `status.loaded === false`; renders nothing when `loaded === true` and nothing on a genuine `check_jwlcore` rejection (tested as three distinct cases: loaded-false-with-reason shows the notice, loaded-true shows nothing, a rejected promise shows nothing — proving the Ok/non-loaded vs. Err distinction from 01-03 is preserved in the UI).

**`App.tsx`** now composes `CommandBar` + `JwlCoreNotice` + `ErrorBanner` + `NotesList`/empty-state, replacing the four inline disabled buttons and manual `open_archive` wiring 01-07 had left as a placeholder.

## Task Commits

1. **Task 1: Command bar wired to IPC with pending/cancel/double-click guarding** — `8f97b359` (feat)
2. **Task 2: ErrorDto surface + jwlCore capability notice, App.tsx wiring** — `91bbece1` (feat)

## Files Created/Modified

- `app/src/components/CommandBar.tsx` — Open/New/Save/Save As, pending state, double-click guard, cancel handling
- `app/src/components/CommandBar.test.tsx` — command invocation, pending/disabled state, double-click guard, cancel, error-surfacing tests
- `app/src/lib/errors.ts` — `describeError`/`isZipSlipRejection`, ErrorDto code -> sentence mapping
- `app/src/components/ErrorBanner.tsx` — sticky error rendering, zip-slip distinct styling
- `app/src/components/ErrorBanner.test.tsx` — full code coverage, zip-slip distinctness, sticky behavior, JwlCoreNotice loaded-true/false/Err tests
- `app/src/components/JwlCoreNotice.tsx` — informational arm64 capability notice, dismissible, mount-time `check_jwlcore` call
- `app/src/App.tsx` — composes CommandBar/JwlCoreNotice/ErrorBanner/NotesList
- `app/src/App.test.tsx` — mocks `@tauri-apps/api/core`/`@tauri-apps/plugin-dialog` (now required transitively via CommandBar/JwlCoreNotice)
- `app/src/styles.css` — `.toolbar-button:disabled`, `.jwlcore-notice`/`.jwlcore-notice-dismiss`, `.error-banner-security`

## Verification Evidence

- `npm test` (`app/`) — 4 test files, 23/23 tests pass (`App.test.tsx`, `NotesList.test.tsx`, `CommandBar.test.tsx` [9/9], `ErrorBanner.test.tsx` [9/9]).
- `npm test -- CommandBar` — passes (plan's literal Task 1 verify).
- `npm test -- ErrorBanner` — passes (plan's literal Task 2 verify).
- `npm run build` (`app/`) — `tsc` + `vite build` clean, no `error TS` lines.
- `cargo test` (`app/src-tauri`, full suite) — unaffected by this frontend-only plan; still green (0 failed across all binaries — save_tests, new_archive_tests, notes_query_tests, open_archive_tests, manifest_tests, fixtures, plus lib unit tests).

**Not run:** `npm run tauri dev` visual boot check (double-click Open rapidly, cancel the file dialog, confirm open/save/save-as/new all work end-to-end visually; confirm the jwlCore notice on an arm64-Windows build). This executor is a non-interactive, non-display sequential session with no way to launch a GUI window. All automated verification (component tests exercising the real invoke/dialog call shapes with mocked IPC, full build, full Rust suite) is green — the owner should do a one-time manual `npm run tauri dev` pass per this plan's `<human-check>` before considering Phase 1 fully closed. This is the same category of open manual gate 01-04/01-05 already recorded (Linux WebKitGTK scroll check, ARCH-02 Python oracle).

## Deviations from Plan

None — plan executed as written. The only interpretive decision (documented above under key-decisions) was reading `error.rs`'s actual `code` strings directly from source rather than the plan's PascalCase examples (`NotAZip`, `MissingManifest`, ...) — the plan's own text names these as illustrative variant names, and the real wire values are the lowercase `to_dto` strings; `lib/errors.ts` matches the real IPC payload, not the Rust enum's Rust-side identifier casing.

## Known Stubs

None. All four command-bar actions are fully wired end-to-end to their real backend commands; the error surface covers every `ArchiveError` variant `to_dto` can emit; the jwlCore notice consumes the real `check_jwlcore` command.

## Manual Gate Required Before Phase 1 Completion

- `npm run tauri dev` visual boot + interaction check (this plan's own `<human-check>`): exercise each toolbar action, double-click Open rapidly (confirm exactly one open), cancel the file dialog (confirm no error banner), confirm Save/Save As write files and New clears to an empty archive with the command bar's pending/label states visible.
- On an arm64-Windows build specifically: confirm the muted jwlCore notice appears (`loaded: false`, reason present) and is dismissible; on x64 confirm it never appears.
- This joins the previously recorded open gates from 01-04 (Linux WebKitGTK scroll smoothness) and 01-05 (ARCH-02 Python differential oracle with PySide6 installed, real JW Library open) as the full set of manual checks the owner must close before Phase 1 is considered shippable.

## Threat Flags

None — the only new surface (`ErrorBanner.tsx` rendering `ErrorDto` fields, and the command bar's `invoke`/dialog calls) was already registered in this plan's own `<threat_model>` (T-06-01 information disclosure, T-06-02 repudiation/silent-swallow, T-06-03 double-click DoS) and is mitigated exactly as designed: curated sentences only (no raw Display), full code coverage with a test asserting distinct non-blank sentences per code, and the synchronous ref guard against duplicate concurrent invokes.

## Self-Check: PASSED

- `app/src/components/CommandBar.tsx` — FOUND
- `app/src/components/CommandBar.test.tsx` — FOUND
- `app/src/components/ErrorBanner.tsx` — FOUND
- `app/src/components/ErrorBanner.test.tsx` — FOUND
- `app/src/components/JwlCoreNotice.tsx` — FOUND
- `app/src/lib/errors.ts` — FOUND
- Commit `8f97b359` (feat: Task 1) — FOUND in `git log`
- Commit `91bbece1` (feat: Task 2) — FOUND in `git log`
