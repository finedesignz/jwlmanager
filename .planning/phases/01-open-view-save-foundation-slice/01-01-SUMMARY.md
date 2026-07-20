---
phase: 01-open-view-save-foundation-slice
plan: 01
subsystem: app-scaffold
tags: [tauri, vite, react, rust, testing, fixtures]
dependency_graph:
  requires: []
  provides:
    - app/ Tauri v2 + Vite + React/TS scaffold
    - Full Cargo.toml dependency list for the whole phase
    - Full tauri.conf.json (dialog capability + bundle resources)
    - vitest frontend test tooling
    - tests/common/mod.rs shared harness (extraction, semantic diff, fixture generators)
    - res/blank-seeded synthetic v16 fixture generator
    - 6-variant zip-slip fixture generator
    - #[ignore]d compile-safe RED open_archive e2e test
  affects:
    - 01-02 (CI matrix, npm test / cargo test steps)
    - 01-03 (jwlCore loader, consumes Cargo.toml/tauri.conf.json resources)
    - 01-04 (resources.db, consumes bundle resource declaration)
    - 01-05 (save/new/save-as, consumes media+unknown fixture entries)
    - 01-07 (open_archive command, un-ignores the RED e2e test)
tech_stack:
  added:
    - tauri 2.11 / tauri-plugin-dialog 2.7
    - rusqlite 0.40 (bundled)
    - zip 8.6.0 (exact pin, >=2.3.0 CVE-2025-29787 fixed)
    - libloading 0.9, thiserror 2, sha2 0.11, ts-rs 12, tempfile 3
    - vite 8 / @vitejs/plugin-react 6 / vitest 4 / @testing-library/react 16
    - @tanstack/react-virtual 3
  patterns:
    - Ordered-struct manifest serialization (not HashMap/Value) reserved for 01-05
    - Shared tests/common/mod.rs fixture generators reused across test binaries
    - res/blank-seeded synthetic fixtures, never a committed real archive
key_files:
  created:
    - app/package.json
    - app/vite.config.ts
    - app/vitest.config.ts
    - app/tsconfig.json
    - app/index.html
    - app/src/main.tsx
    - app/src/App.tsx
    - app/src/App.test.tsx
    - app/src/setupTests.ts
    - app/src/styles.css
    - app/src-tauri/Cargo.toml
    - app/src-tauri/Cargo.lock
    - app/src-tauri/tauri.conf.json
    - app/src-tauri/capabilities/default.json
    - app/src-tauri/build.rs
    - app/src-tauri/src/main.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/common/mod.rs
    - app/src-tauri/tests/fixtures.rs
    - app/src-tauri/tests/open_archive_tests.rs
  modified: []
decisions:
  - "zip pinned exact `=8.6.0` (well past the >=2.3.0 CVE-2025-29787 floor) rather than an open range, per finding 13"
  - "shadcn init skipped (non-interactive registry init unreliable in this environment); hand-authored CSS custom-property tokens matching 01-UI-SPEC.md substitute for it — deferred to a future UI-focused plan"
  - "vite/vitest bumped to the actual current registry majors (vite 8, vitest 4, @vitejs/plugin-react 6) instead of RESEARCH.md's [ASSUMED] older minors, verified via npm view before pinning"
  - "duplicate-entry zip-slip fixture hand-assembled as raw zip bytes because zip::ZipWriter itself now refuses to write a duplicate filename — corroborates the crate's own hardening but means that one variant can't be produced through the normal writer API"
metrics:
  duration: "~90 minutes"
  completed: "2026-07-20"
---

# Phase 1 Plan 1: Walking Skeleton Scaffold + Wave-0 Harness Summary

Stood up the Tauri v2 + Vite + React/TS `app/` scaffold as a booting empty-state shell with the full phase-wide `Cargo.toml`/`tauri.conf.json` declared up front, committed lockfiles, vitest tooling, and a res/blank-seeded synthetic v16 fixture + 6-variant zip-slip fixture + shared test harness with a compile-safe `#[ignore]`d RED `open_archive` integration test.

## What Was Built

**Task 1 — Scaffold:** `app/` now contains a working Vite + React 19 + TypeScript frontend (empty-state shell: "No archive open" + Open/New Archive CTAs, dark-first tokens per 01-UI-SPEC.md) and an `app/src-tauri` Rust crate with the FULL dependency list for the entire phase (tauri, rusqlite bundled, zip pinned exact `=8.6.0`, libloading, thiserror, serde/serde_json, sha2, tempfile, ts-rs). `tauri.conf.json` declares the dialog capability and bundle resources for `libs/jwlCore*` and `res/resources.db` so 01-03/01-04 only consume, never re-edit, this file. `main.rs`/`lib.rs` are a bare Tauri builder with an empty `invoke_handler` and crate-level `#![deny(clippy::unwrap_used, clippy::expect_used)]`. vitest + `@testing-library/react` + jsdom are wired with a passing smoke test, and both `package-lock.json` and `Cargo.lock` are committed.

**Task 2 — Fixtures/harness:** `tests/common/mod.rs` holds the shared extraction/semantic-diff helpers plus two fixture generators: `generate_v16_fixture()` (copies `res/blank`'s real v16 `userData.db`, INSERTs a located Note + an independent Note + a Tag/TagMap, and rebuilds a `.jwlibrary` zip with `manifest.json` + `default_thumbnail.png` + a loose `media/test.png` entry + an unknown `future_unknown.dat` entry) and `generate_zip_slip_fixture(variant)` covering all 6 malicious-entry variants. `tests/open_archive_tests.rs` holds the `#[ignore]`d RED end-to-end test that 01-07 will un-ignore and rewire through the real `open_archive` command. `tests/fixtures.rs` holds the repo-scan test enforcing the GDPR Art. 9 bright line (no `*.jwlibrary` ever tracked in git).

## Verification Evidence

- `cargo check` — clean, no `error[` lines.
- `cargo build` — debug binary builds clean.
- `cargo clippy --all-targets -- -D warnings` — zero warnings.
- `cargo fmt --check` — clean (all files pre-formatted with `cargo fmt`).
- `cargo test` (full suite) — 4/4 fixtures tests pass, `open_archive_tests` shows 1 ignored (RED, as designed); `cargo test --test open_archive_tests -- --include-ignored` passes 1/1.
- `npm run build` — `tsc` + `vite build` clean, no `error TS`.
- `npm test` — 1/1 vitest smoke test passes.
- `package-lock.json` and `app/src-tauri/Cargo.lock` committed.
- `git ls-files '*.jwlibrary'` — empty (also asserted by `test_no_real_archive_is_tracked_in_git`).

**Not run:** `npm run tauri dev` visual boot check (the plan's `<human-check>` step) — this executor is a non-interactive, non-display sequential session with no way to visually confirm a GUI window renders. `cargo build` producing a clean debug binary plus `cargo check`/`clippy`/`fmt` all green, combined with the frontend building and testing clean, is the strongest available proxy signal; the owner should do a one-time `npm run tauri dev` visual sanity check.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `Tag`/`TagMap` synthetic-row IDs collided with res/blank's pre-seeded Favorite tag**
- **Found during:** Task 2, `cargo test --test fixtures`
- **Issue:** `res/blank`'s `userData.db` already seeds `Tag(TagId=1, Name='Favorite')`; the fixture generator's synthetic Tag insert used `TagId=1` too, causing a `UNIQUE constraint failed` panic.
- **Fix:** Moved synthetic Tag/TagMap IDs to 100+ to avoid collision with `res/blank`'s existing seed data.
- **Files modified:** `app/src-tauri/tests/common/mod.rs`
- **Commit:** `0aeebc75`

**2. [Rule 1 - Bug] `zip::ZipWriter` rejects duplicate filenames, breaking the duplicate-entry zip-slip variant**
- **Found during:** Task 2, `cargo test --test fixtures`
- **Issue:** `zip` 8.6.0's `ZipWriter::start_file` now itself refuses to write a second entry with an already-used name (`InvalidArchive("Duplicate filename")`), so the planned "just call `start_file` twice" approach for the duplicate-entry zip-slip fixture panicked.
- **Fix:** Hand-assembled that one fixture variant as raw zip bytes (manual local file headers + central directory + EOCD, with a small bitwise CRC-32) to bypass the writer's own guard and still produce a genuinely duplicate-entry archive for the extractor test to reject.
- **Files modified:** `app/src-tauri/tests/common/mod.rs`
- **Commit:** `0aeebc75`

**3. [Rule 3 - Blocking] npm dependency version conflicts (vite 8 vs `@vitejs/plugin-react`/vitest peer ranges)**
- **Found during:** Task 1, `npm install`
- **Issue:** RESEARCH.md's `[ASSUMED]` versions were stale; `npm view` showed vite at major 8 with `@vitejs/plugin-react@4` (needing vite 4-7) and `@vitejs/plugin-react@6` (needing vite 8) in tension with an initially-picked mismatched trio.
- **Fix:** Verified exact current majors via `npm view <pkg> version`/`versions` before pinning: `vite@^8.1.5`, `@vitejs/plugin-react@^6.0.3`, `vitest@^4.1.10` — a mutually-compatible set, confirmed by a clean `npm install`.
- **Files modified:** `app/package.json`
- **Commit:** `259161e9`

### Deferred (Rule 2, documented not applied — informational)

- **shadcn init deferred:** `npx shadcn init` requires an interactive registry-selection flow this non-interactive session couldn't drive reliably; substituted hand-authored CSS custom-property tokens (`app/src/styles.css`) matching 01-UI-SPEC.md's spacing/color/typography contract exactly (dark-first `--bg-primary`/`--bg-secondary` triad, blue-600 accent, 44px touch targets). A future UI-focused plan can run the real shadcn init and migrate the toolbar/empty-state components to shadcn `Button`/`Alert` without changing the app's structure — no bright-line risk, this is presentational only.

## Known Stubs

- All four toolbar buttons (Open/New/Save/Save As) and the two empty-state CTAs are rendered `disabled` — no IPC wiring exists yet by design (01-07 wires `open_archive`; 01-05 wires save/new/save-as). This matches the plan's explicit scope boundary ("no IPC wiring yet — that is 01-07") and is not a defect.

## Self-Check: PASSED

All 9 key files confirmed present on disk; both task commits (`259161e9`, `0aeebc75`) confirmed in `git log`.
