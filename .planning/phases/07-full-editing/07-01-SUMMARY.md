---
phase: 07-full-editing
plan: "01"
subsystem: editing
tags: [favorites, edit-safety, tracer, rust, react]
dependency-graph:
  requires: [Phase 2 delete safety envelope, Phase 6 Favorites identity PK/browse]
  provides: ["Favorites mark/unmark end-to-end (EDIT-05)", "db/edit.rs shared spine for 07-02..05"]
  affects: [app/src-tauri/src/db, app/src/components/CategoryList.tsx, app/src/lib/operations.ts]
tech-stack:
  added: []
  patterns:
    - "typed non-empty selection wrapper + apply_*(tx)/dry_run_*(conn under PragmaGuard, unchecked_transaction) + command pair"
    - "adjust state during render (not useEffect) to reset per-category UI state in the same commit as the prop change"
key-files:
  created:
    - app/src-tauri/src/db/edit.rs
    - app/src-tauri/src/db/favorites.rs
    - app/src-tauri/tests/favorites_tests.rs
    - app/src/components/FavoriteAddDialog.tsx
    - app/src/components/FavoriteAddDialog.test.tsx
    - app/src/components/EditPreviewDialog.tsx (renamed from DeletePreviewDialog.tsx)
    - app/src/components/EditPreviewDialog.test.tsx
  modified:
    - app/src-tauri/src/db/resources.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src/components/CategoryList.tsx
    - app/src/lib/operations.ts
    - app/src/lib/errors.ts
    - app/src/styles.css
decisions:
  - "Selection reset on category switch (D6-05) moved from useEffect to React's 'adjust state during render' pattern — a genuine race, not a test artifact (see Deviations)."
metrics:
  duration: "resumed session; ~1 task remaining"
  completed: 2026-07-26
status: complete
---

# Phase 7 Plan 1: Favorites mark/unmark tracer slice Summary

Landed the Phase 7 safety spine (`db/edit.rs`) and shipped Bible-edition Favorites mark/unmark end-to-end — SQL through Tauri commands through a preview-then-confirm React dialog — proving the whole architecture (typed non-empty selection, transactional apply, rolled-back dry-run under `PragmaGuard`, command pair, `EditPreviewDialog`, `LIVE` capability flip, semantic round-trip test) before plans 02-05 build four more op groups on top of it.

## What Was Built

**Task 1 (committed `fbc55a8c`) — Favorites unmark, shared spine.** `db/edit.rs` generalizes the Phase 2 delete primitives (`snapshot_pks`/`snapshot_tables`/`diff_snapshots`, tracked-table vocabulary extended with `InputField`/`Bookmark`). `db/favorites.rs` adds `NonEmptyTagMapIds`, `apply_favorite_remove`, `dry_run_favorite_remove`. `favorite_remove_dry_run`/`favorite_remove_apply` commands registered. `DeletePreviewDialog.tsx` renamed to `EditPreviewDialog.tsx` (CSS classes, test-ids, callers updated in lockstep), with Esc/click-outside-to-cancel added, gated by the preserved synchronous `busyRef` guard. `Favorites:delete` flipped `LIVE`.

**Task 2 (committed `4743a626`) — Favorites mark backend + catalog loader.** `ResourceCatalog::load_favorite_editions` reads the bundled `Favorites` VIEW. `apply_favorite_add`/`dry_run_favorite_add` port `add_favorite` (JWLManager.py:3391-3460): ensure the system `Tag(Type=0,Name='Favorite')`, find-or-insert `Location`, explicit pre-INSERT duplicate SELECT raising `ArchiveError::FavoriteDuplicate` before ever attempting the `TagMap` INSERT (never relies on catching the `UNIQUE(TagId, LocationId)` violation), then INSERT at `Position = max+1`. `favorite_add_dry_run`/`favorite_add_apply` commands registered.

**Task 3 (this session, committed `a1a1a237`) — FavoriteAddDialog + op-bar wiring.** `FavoriteAddDialog.tsx`: native language `<select>` (defaults to English), 44px-row non-virtualized edition list with `title=` truncation, "Loading editions…" affordance, empty-editions sentence, dry-run-to-`EditPreviewDialog`-to-apply internal flow, Escape/click-outside/Cancel-button all route through one `busyRef`-guarded cancel. `CategoryList.tsx`: Favorites' `add` op resolved to "Add Favorite" via `resolveOpLabel`, wired to `FavoriteAddDialog`; the toolbar no longer early-returns when `rows.length === 0` (a fresh archive's empty Favorites list must still expose "Add Favorite"). `operations.ts`: `Favorites:add` flipped `LIVE`. `styles.css`: `.favorite-dialog*` classes using only existing design tokens.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Category-switch selection reset raced against the async category-fetch flow**
- **Found during:** Finishing Task 3 — `npx vitest run` showed `App.test.tsx`'s pre-existing D6-05 test ("switching category resets the prior selection") failing intermittently once `CategoryList.tsx` grew with the Favorites `add` wiring.
- **Issue:** The D6-05 selection reset lived in a `useEffect([category])`. `App.handleSelectCategory` awaits `list_category` then sets `category` and `rows` together in one commit; the `useEffect`-based reset only fires in the *following* commit. Any caller reading `selected` in the window between those two commits sees the stale selection from the prior category. This is a genuine race in the original Phase 6 shape, not something introduced by this plan — but the enlarged component (extra render work, extra state) shifted React's effect-scheduling timing enough for the test's polling `findByText` to observe the DOM mid-race, which is what surfaced it now.
- **Fix:** Replaced the `useEffect` reset with React's documented "adjust state during render" pattern — a `renderedCategory` state compared against the `category` prop, with the reset (`setSelected`/`setReport`/`setShowFavoriteDialog`) applied synchronously in the same render as the category change, closing the race window entirely rather than relying on effect-scheduling order.
- **Files modified:** `app/src/components/CategoryList.tsx`
- **Commit:** `a1a1a237`

None of the other three tasks required deviations from the plan's committed shape (Tasks 1-2 were already landed correctly by the interrupted prior session; verified via `git show` diffs before continuing).

## Test Output (actual, this session)

```
$ cd app/src-tauri && cargo test --jobs 2
... (48+ passed across all suites: db, resources, error bindings, favorites_tests,
     schema_upgrade_tests, trim_tests, etc.)
test result: ok. 48 passed; 0 failed
... (every other suite: ok, 0 failed; one intentionally-ignored Python-parity test)

$ cd app/src-tauri && cargo clippy --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.41s
(only non-blocking ts-rs `try_from` serde-attribute-parse warnings, not -D warnings failures)

$ cd app && npx vitest run
 Test Files  8 passed (8)
      Tests  82 passed (82)
```

Both suites green.

## Self-Check: PASSED

- `app/src-tauri/src/db/edit.rs` — FOUND
- `app/src-tauri/src/db/favorites.rs` — FOUND
- `app/src-tauri/tests/favorites_tests.rs` — FOUND
- `app/src/components/EditPreviewDialog.tsx` — FOUND
- `app/src/components/FavoriteAddDialog.tsx` — FOUND
- `git log --oneline` shows `fbc55a8c`, `4743a626`, `a1a1a237` — FOUND

## Known Stubs

None — Favorites mark/unmark is fully wired, no placeholder data paths.
