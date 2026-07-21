---
phase: 02-safe-delete
plan: 03
subsystem: frontend delete UI (NotesList selection + DeletePreviewDialog)
tags: [delete, dry-run, confirm-dialog, react, vitest, safe-delete]
requires: [02-02]
provides: [components::NotesList (selection+delete gating), components::DeletePreviewDialog]
affects: [phase-04-downgrade-preview, phase-05-merge-preview]
tech-stack:
  added: []
  patterns:
    - "Reusable destructive-confirm dialog (DeletePreviewDialog) taking a general DryRunReport — Phase 4/5 can reuse unchanged (D2-07)"
    - "Selection keyed by NoteId (Set<bigint>), not virtual row index, so it survives TanStack Virtual windowing"
    - "Confirm button double-click guard mirrors CommandBar's synchronous busyRef pattern (T-02-10)"
key-files:
  created:
    - app/src/components/DeletePreviewDialog.tsx
    - app/src/components/DeletePreviewDialog.test.tsx
  modified:
    - app/src/components/NotesList.tsx
    - app/src/components/NotesList.test.tsx
    - app/src/App.tsx
    - app/src/lib/errors.ts
    - app/src/styles.css
decisions:
  - "NotesList owns the whole delete flow (selection, dry-run invoke, dialog mount) rather than lifting selection state to App — keeps App.tsx a thin shell (notes/error only) and keeps the dialog's data dependency (DryRunReport) local to where the dry-run is triggered."
  - "On DeletePreviewDialog Confirm error, the dialog closes (report cleared) and the ErrorDto routes to App's existing ErrorBanner rather than staying open for retry — consistent with how CommandBar surfaces every other command error; a retry is a fresh Delete click (re-runs dry-run, always current)."
metrics:
  duration: "~40m"
  completed: "2026-07-21"
---

# Phase 2 Plan 03: Delete Preview/Confirm UI Summary

Row selection (keyed by `NoteId`, survives virtualization) plus a Delete affordance disabled unless `selection.size >= 1` (SAFE-03 UI-tier defense-in-depth), wired to `delete_notes_dry_run`. A new reusable `DeletePreviewDialog` renders the `DryRunReport`'s per-table counts in plain language, requires an explicit double-click-guarded Confirm to call `delete_notes_apply`, and Cancel is a pure no-op. On successful apply, the deleted `NoteId`s are filtered out of the in-memory list locally (no full reload) and selection is cleared.

## What shipped

- **`app/src/components/NotesList.tsx`**: added a `Set<bigint>` selection state keyed by `note.id`, a per-row checkbox (`data-testid="notes-list-row-checkbox"`), a Delete button (`data-testid="notes-list-delete-button"`) disabled unless `selection.size >= 1`. Clicking Delete invokes `delete_notes_dry_run({ ids })` and stores the returned `DryRunReport`, mounting `DeletePreviewDialog` when present. Confirm invokes `delete_notes_apply({ ids })`; on success, `onNotesChanged` is called with `notes.filter(note => !selected.has(note.id))` and selection is cleared (finding 8). Cancel just clears the report (no invoke). Errors from either invoke route through the new `onError` prop.
- **`app/src/components/DeletePreviewDialog.tsx`**: general-purpose destructive-confirm dialog (reusable for Phase 4/5 per D2-07) rendering `report.deleted`'s non-zero per-table counts as a plain-language sentence ("This will remove 1 Note, 1 TagMap (2 rows total)..."), plus `report.total_deleted`. Confirm mirrors `CommandBar`'s synchronous `busyRef` double-click guard (T-02-10) and shows a "Deleting…" pending state; Cancel is disabled while pending and otherwise a pure no-op.
- **`app/src/App.tsx`**: added `handleNotesChanged` (sets `notes` from the post-delete list) and wired `onNotesChanged`/`onError` into `NotesList`.
- **`app/src/lib/errors.ts`**: added `delete_failed` -> "Couldn't delete the selected notes — the archive is unchanged. Try again, or close and reopen the archive if the problem continues." (verb-led, notes the archive is unchanged, per Copywriting Contract).
- **`app/src/styles.css`**: added `.notes-list-container`/`.notes-list-toolbar`/`.notes-list-row-checkbox` and `.delete-preview-*` rules — `--bg-secondary` card, hairline border, `rounded` 12px corners, `--destructive` used only as a restrained accent on the Confirm button (never a full red flood), matching 01-UI-SPEC's calm-destructive-confirm tone.

## Tests (vitest)

- `NotesList.test.tsx` (5 new cases, 9 total): Delete disabled at 0 selected / enabled at >=1 with the correct count; selecting multiple rows updates the count; clicking Delete invokes `delete_notes_dry_run` with the selected `NoteId`s; Confirm in the mounted dialog invokes `delete_notes_apply`, removes the deleted row from the list via `onNotesChanged`, closes the dialog, and resets selection; Cancel invokes no apply and leaves `onNotesChanged` uncalled.
- `DeletePreviewDialog.test.tsx` (4 new cases): renders per-table deleted counts + total from a `DryRunReport`; Confirm invokes `onConfirm` exactly once even under a rapid double-click; Confirm disables the button while pending; Cancel invokes `onCancel` and never `onConfirm`.
- Full frontend suite: `npm test` — 5 test files, 32 tests, all green.

## Deviations from Plan

None — plan executed as written. One minor scope note: `NotesList` owns the entire delete flow (selection state, dry-run invoke, dialog mount) rather than being split across `App.tsx`; the plan allowed either ("or lift to App if cleaner") and this keeps `App.tsx` a thin shell.

## Manual verification gate (owner, deferred)

No display available in this environment for a visual Tauri boot. The automated frontend suite (vitest) and `npm run build` both pass, and backend `cargo test` shows no regressions (100 tests, same as 02-02), but the actual click-through (select a row -> Delete -> preview shows real counts -> Confirm -> row disappears -> Save -> reopen and confirm trimmed) against a real `.jwlibrary` archive per 02-03-PLAN.md's Manual-Only verification step still needs the owner (or a windowed CI runner) to execute `npm run tauri dev` and drive it visually. Flagging per 02-VALIDATION.md's Manual-Only category — not part of this executor's automatable Definition of Done.

## Verification

- `npm test -- NotesList` — 9 tests pass (4 pre-existing virtualization/render tests + 5 new selection/gating/dry-run tests).
- `npm test -- DeletePreviewDialog` — 4 tests pass.
- `npm test` — full suite: 5 files, 32 tests, 0 failed.
- `npm run build` — clean (tsc + vite build).
- `cargo test` (app/src-tauri) — 100 tests pass, 1 ignored (unchanged from 02-02-SUMMARY.md's baseline; no backend regressions from this frontend-only plan).

## Self-Check

- FOUND: app/src/components/DeletePreviewDialog.tsx
- FOUND: app/src/components/DeletePreviewDialog.test.tsx
- FOUND: app/src/components/NotesList.tsx (modified)
- FOUND: app/src/components/NotesList.test.tsx (modified)
- FOUND commit eedff73b (Task 1)
- FOUND commit 1827726d (Task 2)

## Self-Check: PASSED
