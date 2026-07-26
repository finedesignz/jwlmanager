---
phase: 10-n-way-merge-fold
plan: 03
subsystem: frontend
tags: [react, typescript, merge, ui]

requires:
  - phase: 10-n-way-merge-fold
    plan: 01
    provides: fold_merge_dry_run, fold_merge_commit (Tauri commands, source_paths: Vec<String>)
provides:
  - FoldMergeDialog (ordered, reorderable N-way merge source list)
  - "Merge Multiple Archives…" CommandBar action wired to fold_merge_dry_run / fold_merge_commit
affects: []

tech-stack:
  added: []
  patterns:
    - "FoldMergeDialog is a controlled component with no internal path-list state — the parent's array is unambiguously the array later sent to the backend (mirrors the plan's own key-link contract)."
    - "Two-stage preview-then-commit state in CommandBar (foldSources for the list stage, foldPreview for the confirm stage), mirroring MergePreview/V14Preview's existing shape one-for-one."
    - "A local ref (foldContinueBusyRef) guards the FoldMergeDialog Continue click since that call isn't wrapped by runAction's busyRef (the picker phase already was, but Continue fires later after user reordering)."

key-files:
  created:
    - app/src/components/FoldMergeDialog.tsx
    - app/src/components/FoldMergeDialog.test.tsx
  modified:
    - app/src/components/CommandBar.tsx
    - app/src/components/CommandBar.test.tsx
    - app/src/styles.css

key-decisions:
  - "FoldMergeDialog rows use up/down icon buttons (▲▼) plus a remove (✕) button rather than drag-and-drop — no new frontend dependency, matches the plan's explicit prohibition."
  - "On a rejected fold_merge_dry_run, the source list dialog is left open (not cleared) so the user can retry without re-picking every archive — the plan's behavior spec only required onError + no preview, this is the least-destructive way to satisfy it."
  - "CSS follows the exact .edit-preview-dialog/.tag-dialog/.media-add-file-row radius/surface/padding/row-height conventions already shipped — no new token, radius, or spacing value introduced."

requirements-completed: [MERGE-03]

coverage:
  - id: D-UI-1
    description: "User picks 3+ archives in one action and sees them as an ordered list before anything runs"
    requirement: "MERGE-03"
    verification:
      - kind: unit
        ref: "app/src/components/FoldMergeDialog.test.tsx, app/src/components/CommandBar.test.tsx#choosing files opens FoldMergeDialog..."
        status: pass
    human_judgment: false
  - id: D-UI-2
    description: "List order IS the fold order, stated in plain language, reorderable via Move up/down; the reordered array is what fold_merge_dry_run receives"
    requirement: "MERGE-03"
    verification:
      - kind: unit
        ref: "app/src/components/CommandBar.test.tsx#reordering in the dialog before Continue changes the array..."
        status: pass
    human_judgment: false
  - id: D-UI-3
    description: "Confirming the list runs fold_merge_dry_run once and shows ONE aggregate preview via EditPreviewDialog, naming the source count"
    requirement: "MERGE-03"
    verification:
      - kind: unit
        ref: "app/src/components/CommandBar.test.tsx#Continue calls fold_merge_dry_run once..."
        status: pass
    human_judgment: false
  - id: D-UI-4
    description: "Only Confirm calls fold_merge_commit, with the SAME array; cancel at either stage writes nothing"
    requirement: "MERGE-03"
    verification:
      - kind: unit
        ref: "app/src/components/CommandBar.test.tsx#Preview Confirm calls fold_merge_commit..., #Preview Cancel makes no fold_merge_commit call, #choosing files opens FoldMergeDialog... (no backend call)"
        status: pass
    human_judgment: false
  - id: D-UI-5
    description: "After successful commit, the session refreshes via list_notes / onOpened, exactly as the two-archive merge"
    requirement: "MERGE-03"
    verification:
      - kind: unit
        ref: "app/src/components/CommandBar.test.tsx#Preview Confirm calls fold_merge_commit with the SAME array, then list_notes, then onOpened"
        status: pass
    human_judgment: false
  - id: D-UI-6
    description: "Fewer than 3 files does not reach the backend — Continue is unavailable"
    requirement: "MERGE-03"
    verification:
      - kind: unit
        ref: "app/src/components/FoldMergeDialog.test.tsx#Continue is unavailable with fewer than 3 rows..."
        status: pass
    human_judgment: false
  - id: D-UI-7
    description: "merge_unavailable / merge failure ErrorDto surfaces through onError, never a crash or silent no-op"
    requirement: "MERGE-03"
    verification:
      - kind: unit
        ref: "app/src/components/CommandBar.test.tsx#a rejected fold_merge_dry_run..., #a rejected fold_merge_commit..."
        status: pass
    human_judgment: false

duration: ~35min
completed: 2026-07-26
status: complete
---

# Phase 10 Plan 03: N-Way Merge Fold — UI Summary

**"Merge Multiple Archives…" gives the fold a surface: a multi-select picker feeds a new `FoldMergeDialog` where the user sees and reorders the fold order before anything runs, Continue fires one aggregate `fold_merge_dry_run` into the shipped `EditPreviewDialog`, and only its Confirm fires `fold_merge_commit` with the identical array — mirroring the two-archive merge flow one-for-one.**

## Performance

- **Duration:** ~35 min
- **Tasks:** 2 completed
- **Files created:** 2 (`FoldMergeDialog.tsx`, `FoldMergeDialog.test.tsx`)
- **Files modified:** 3 (`CommandBar.tsx`, `CommandBar.test.tsx`, `styles.css`)

## Accomplishments

- Built `FoldMergeDialog` as a controlled component: renders the chosen paths in order with a 1-based position number, base file name, and Move-up / Move-down / Remove controls per row (index-swap / filtered-copy over a local array copy, never sorted or deduped). States the order rule in plain language ("Archives merge in the order shown, top to bottom. When two archives change the same record, the one lower in the list wins."). Continue is disabled below 3 entries with a visible reason (`fold-merge-reason`).
- Wired `CommandBar` with a new "Merge Multiple Archives…" action: `handleFoldMerge` opens a `multiple: true` picker (cancel → `onCancelled`, zero invokes); the chosen paths open `FoldMergeDialog` with no backend call yet; `handleFoldContinue` calls `fold_merge_dry_run` once with the dialog's exact order and opens `EditPreviewDialog`; `handleFoldConfirm` calls `fold_merge_commit` with the SAME array, then `list_notes`, then `onOpened`. Both dry-run and commit rejections route through `onError`; a commit rejection also closes the preview (`finally` clause), a dry-run rejection leaves the source list open for retry.
- Preview summary copy names the number of source archives and states the counts are "the combined effect of N archives in the shown order" — never implies order-independence, per the plan's prohibition.
- Added `.fold-merge-*` CSS following the exact `.edit-preview-dialog`/`.tag-dialog`/`.media-add-file-row` radius, surface, padding, and 44px row-height conventions already shipped — no new token/radius/spacing value.
- Extended `CommandBar.test.tsx` with 11 new cases covering every `<behavior>` bullet from Task 2, including explicit assertions that `fold_merge_dry_run` and `fold_merge_commit` receive the identical (order-included) array, and that a mid-flow reorder changes what's sent.

## Task Commits

1. **Task 1: FoldMergeDialog — the ordered, reorderable source list** - `fa86909e` (feat)
2. **Task 2: CommandBar wiring — multi-pick, aggregate preview, single commit** - `f1dc61d7` (feat)

## Files Created/Modified

- `app/src/components/FoldMergeDialog.tsx` - new controlled component, source list with reorder/remove
- `app/src/components/FoldMergeDialog.test.tsx` - 11 tests covering every `<behavior>` bullet
- `app/src/components/CommandBar.tsx` - "Merge Multiple Archives…" action, two-stage fold state (`foldSources`/`foldPreview`), handlers, dialog rendering
- `app/src/components/CommandBar.test.tsx` - 11 new fold-merge test cases
- `app/src/styles.css` - `.fold-merge-dialog`/`.fold-merge-title`/`.fold-merge-order-note`/`.fold-merge-list`/`.fold-merge-row`/`.fold-merge-row-position`/`.fold-merge-row-label`/`.fold-merge-reason`/`.fold-merge-actions`

## Decisions Made

- **Reorder controls are Move-up/Move-down/Remove buttons, not drag-and-drop** — satisfies the plan's explicit "MUST NOT add a frontend dependency" prohibition while still giving the user full order control.
- **A rejected `fold_merge_dry_run` leaves the source list dialog open** rather than clearing it, so a `merge_unavailable` error (or any transient failure) doesn't force the user to re-pick every archive to retry.
- **`foldContinueBusyRef`** guards the Continue button's dry-run call synchronously, since that call happens after the picker phase's `runAction` guard has already released (the user is now interacting with the dialog, not waiting on the picker).

## Deviations from Plan

None — plan executed exactly as written. No new npm dependency was needed; the existing "Merge Archive…" action and its tests are untouched.

## Issues Encountered

None. Both tasks compiled and passed on the first implementation.

## Verification

- `cd app && npx vitest run` — **164 tests passed (14 test files)**, including all 11 new `FoldMergeDialog` tests and all 11 new fold-merge `CommandBar` tests. Actual output:
  ```
  Test Files  14 passed (14)
       Tests  164 passed (164)
  ```
- `npx tsc --noEmit` — clean, no output.
- `git diff app/package.json app/package-lock.json` — empty, confirmed no dependency change.
- `cargo test --jobs 2` — **not run in this plan.** This plan's file scope is frontend-only (`FoldMergeDialog.tsx`/`.test.tsx`, `CommandBar.tsx`/`.test.tsx`), no Rust files were touched, and a sibling agent was concurrently working in `app/src-tauri/` per the execution brief's explicit file-scope boundary ("A sibling agent is concurrently working in `app/src-tauri/` — do NOT touch Rust files"). The Rust-side fold commands were already proven end-to-end against the real jwlCore DLL in plan 10-01 (4/4 tests passing). Running `cargo test` here would race a concurrently-edited tree rather than verify this plan's own changes.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

The fold is now drivable end-to-end from the UI: pick 3+ archives, see and reorder the fold list, approve one aggregate preview, and land on a refreshed grid, with cancel at either stage writing nothing and errors surfacing through the existing `ErrorBanner`. This is the last plan for Phase 10 per the wave plan (10-01 tracer + fold engine, 10-02 failure envelope, 10-03 this UI) — no blockers for phase closeout.

---
*Phase: 10-n-way-merge-fold*
*Completed: 2026-07-26*

## Self-Check: PASSED

All created/modified files confirmed present on disk; both task commit hashes (`fa86909e`, `f1dc61d7`) confirmed in `git log`.
