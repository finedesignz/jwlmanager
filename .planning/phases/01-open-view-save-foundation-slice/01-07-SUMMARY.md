---
phase: 01-open-view-save-foundation-slice
plan: 07
subsystem: archive-core
tags: [tauri, rust, rusqlite, zip, ts-rs, thiserror, ipc, security]

requires:
  - phase: 01-01
    provides: app/ scaffold, Cargo.toml/tauri.conf.json full dependency+capability declarations, res/blank-seeded v16 fixture generator, 6-variant zip-slip fixture generator, shared test harness, RED open_archive e2e test
provides:
  - Two-layer typed error surface (internal ArchiveError + IPC-safe ErrorDto)
  - Durable ArchiveSession managed-state object (TempDir + full zip-entry inventory + dirty flag)
  - Category enum with ts-rs generated TS bindings
  - Zip-slip-safe extraction primitive
  - v16-ONLY archive validity gate
  - Thin located-Note query over rusqlite
  - Registered open_archive Tauri command wired end-to-end (webview -> IPC -> Rust -> SQLite -> rendered rows)
affects: [01-02, 01-04, 01-05, 01-03]

tech-stack:
  added: []
  patterns:
    - "Two-layer error surface: internal thiserror enum (not Serialize) + a separate Serialize-able boundary DTO, never leaking paths/source Display over IPC"
    - "Session-as-managed-state: a single durable object (owning the TempDir + full zip-entry inventory) populated on open, consumed by later save/save-as"
    - "Schema-gate-before-DB-open: reject on manifest-declared schemaVersion before ever opening the (possibly hostile-shaped) SQLite file"

key-files:
  created:
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/session.rs
    - app/src-tauri/src/category.rs
    - app/src-tauri/src/archive/mod.rs
    - app/src-tauri/src/archive/extract.rs
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/db/notes.rs
    - app/src-tauri/tests/error_tests.rs
    - app/src-tauri/tests/category_tests.rs
    - app/src-tauri/tests/extract_tests.rs
    - app/src-tauri/tests/archive_validity_tests.rs
    - app/src/components/NotesList.tsx
    - app/src/bindings/Category.ts
    - app/src/bindings/NotesRow.ts
    - app/src/bindings/ErrorDto.ts
  modified:
    - app/src-tauri/Cargo.toml
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/open_archive_tests.rs
    - app/src/App.tsx
    - app/src/styles.css

key-decisions:
  - "tempfile moved from a dev-dependency to a regular dependency: ArchiveSession (production code, not test code) now owns a TempDir for the session lifetime"
  - "Schema gate checks manifest.schemaVersion first and returns UnsupportedSchema immediately on mismatch, before ever opening the extracted userData.db as SQLite — a hostile v14 fixture never gets a chance to exercise the SQLite open path"
  - "ErrorDto also derives ts-rs TS so the frontend catch-block is typed against a generated binding rather than `any`"
  - "db/notes.rs is thin by design: only the located-Note query (JWLManager.py:751-757 shape, no dupes CTE, no WHERE filter); the independent-notes UNION and resources.db label synthesis are explicitly deferred to 01-04, per plan non-negotiables"

patterns-established:
  - "Every Tauri command returns Result<T, ErrorDto>; the internal ArchiveError never crosses invoke_handler"
  - "extract_zip_slip_safe never creates a filesystem symlink from an archive entry (copies bytes into a plain file at the enclosed_name-validated path), closing the CVE-2025-29787 symlink-chain class independently of the enclosed_name check"

requirements-completed: [ARCH-01, ARCH-05, DATA-01, DATA-08, SAFE-05]

duration: ~70min
completed: 2026-07-19
---

# Phase 1 Plan 7: Core Primitives (ArchiveSession, Typed Errors, v16 Gate, Notes Query) Summary

Stood up the security-sensitive core of the Walking Skeleton: a two-layer typed-error boundary, a durable `ArchiveSession` managed-state object that owns the extracted working copy and full zip-entry inventory, a `ts-rs`-backed `Category` enum, zip-slip-safe extraction, a v16-ONLY schema gate, and a thin located-Notes query — then registered `open_archive` as a real Tauri command and wired the frontend to render actual rows, turning 01-01's RED end-to-end test green.

## Performance

- **Duration:** ~70 min
- **Tasks:** 2/2 completed
- **Files modified:** 20 (15 created, 5 modified)

## Accomplishments
- Two-layer error surface: `ArchiveError` (thiserror, wraps io/rusqlite/zip/json, intentionally not `Serialize`) and `ErrorDto` (Serialize + ts-rs, code/operation/safe_file_name/message_key only — no raw path or source `Display` ever crosses IPC)
- `ArchiveSession` managed state: owns the `TempDir` for the session lifetime, records the full original zip-entry inventory, tracks `dirty`; registered via `tauri::State<Mutex<Option<ArchiveSession>>>` and populated by `open_archive`
- v16-ONLY validity gate: manifest `schemaVersion` checked before the extracted DB is ever opened as SQLite; `PRAGMA user_version` cross-checked once the manifest passes
- `open_archive` registered as a real Tauri command, wired through the native file-open dialog (never a raw JS path string) to a live `NotesList` render
- 01-01's `#[ignore]`d RED `open_archive` e2e test is un-ignored and green, now driving the real `archive::open_and_validate` primitive and asserting the session is fully populated (not a bare `Vec<NotesRow>`)

## Task Commits

1. **Task 1: Core primitives** - `e7a0aa03` (feat)
2. **Task 2: Register open_archive command + Notes render** - `039c7127` (feat)

## Files Created/Modified

- `app/src-tauri/src/error.rs` - `ArchiveError` (internal) + `ErrorDto` (IPC boundary, Serialize + ts-rs)
- `app/src-tauri/src/session.rs` - `ArchiveSession` managed-state struct + `SessionState` type alias
- `app/src-tauri/src/category.rs` - `Category` enum, ts-rs exported to `app/src/bindings/Category.ts`
- `app/src-tauri/src/archive/extract.rs` - `extract_zip_slip_safe`: enclosed_name-validated extraction, no symlink creation
- `app/src-tauri/src/archive/mod.rs` - `open_and_validate`: extract -> presence checks -> v16-only gate -> Notes query -> `ArchiveSession`
- `app/src-tauri/src/db/notes.rs` - `NotesRow` + `query_notes` (thin located-Note query, ts-rs exported)
- `app/src-tauri/src/lib.rs` - registers `open_archive` Tauri command, manages `Mutex<Option<ArchiveSession>>`
- `app/src/App.tsx` - wires Open Archive to `@tauri-apps/plugin-dialog`'s native picker + `invoke("open_archive", ...)`, renders `NotesList` or a sanitized error banner
- `app/src/components/NotesList.tsx` - thin (not-yet-virtualized) 44px-row Notes render
- `app/src-tauri/Cargo.toml` - `tempfile` moved from `[dev-dependencies]` to `[dependencies]`

## Verification Evidence

- `cargo build` — clean.
- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — zero warnings (unwrap/expect ban intact; test files carry their own `#[allow]` as established in 01-01).
- `cargo test` (full suite, `app/src-tauri`) — 11 tests pass across `error_tests`, `category_tests`, `extract_tests`, `archive_validity_tests`, `fixtures`, `open_archive_tests`, plus 3 ts-rs `export_bindings_*` unit tests; 0 ignored (the formerly-`#[ignore]`d `test_open_archive_lists_at_least_one_note` now runs and passes).
- `cargo test --test error_tests` — 3/3 pass (plan's literal verify command).
- `cargo test category_enum` / `cargo test extract_rejects_traversal` / `cargo test schema_v16_only` (run individually — see Deviations) — each 1/1 pass.
- `npm run build` (`app/`) — `tsc` + `vite build` clean, no `error TS` lines; ts-rs-generated `app/src/bindings/*.ts` type-check against `App.tsx`/`NotesList.tsx`.
- `npm test` (`app/`) — 1/1 vitest smoke test passes (empty-state shell still renders; Open Archive is no longer `disabled`).

**Not run:** `npm run tauri dev` visual boot check (the plan's `<human-check>` step) — this executor is a non-interactive, non-display sequential session with no way to visually confirm a GUI window renders or that clicking Open Archive + selecting a fixture shows a rendered row. All automated verification (`cargo build/fmt/clippy/test`, `npm run build/test`) is green, which is the strongest available proxy signal; the owner should do a one-time `npm run tauri dev` visual sanity check — open a fixture built via a small Rust snippet or the owner's real archive (env-gated, D-07) and confirm at least one Note row renders with the empty state showing beforehand.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `tempfile` was only a dev-dependency; `ArchiveSession` (production code) needs it**
- **Found during:** Task 1, `cargo build`
- **Issue:** 01-01 declared `tempfile = "3"` under `[dev-dependencies]` (it was only used by the test harness at that point). This plan's `ArchiveSession` owns a `tempfile::TempDir` in non-test, production code, so the crate failed to compile (`unresolved import tempfile`).
- **Fix:** Moved `tempfile = "3"` from `[dev-dependencies]` to `[dependencies]` in `app/src-tauri/Cargo.toml`. No version change, no new crate — same dependency, correct dependency section.
- **Files modified:** `app/src-tauri/Cargo.toml`
- **Commit:** `e7a0aa03`

**2. [Rule 1 - Bug] `ArchiveSession` needed `Debug` for test-side `expect_err`/`{:?}` formatting**
- **Found during:** Task 1, `cargo clippy --all-targets`
- **Issue:** `error_tests.rs` and `archive_validity_tests.rs` call `.expect_err(...)` / format `Result<(ArchiveSession, Vec<NotesRow>), ArchiveError>` with `{:?}`, which requires `ArchiveSession: Debug`. It didn't derive `Debug`.
- **Fix:** Added `#[derive(Debug)]` to `ArchiveSession` in `session.rs` (all its fields — `TempDir`, `PathBuf`, `ManifestMeta`, `Vec<ZipEntryMeta>`, `bool` — already implement `Debug`).
- **Files modified:** `app/src-tauri/src/session.rs`
- **Commit:** `e7a0aa03`

### Deferred (Rule 2, documented not applied — informational)

- **Plan's Task 1 verify command `cargo test category_enum extract_rejects_traversal schema_v16_only` is not runnable as written:** `cargo test` only accepts a single positional `TESTNAME` filter argument; passing three fails with `error: unexpected argument 'extract_rejects_traversal' found`. Each filter was verified individually instead (`cargo test category_enum`, `cargo test extract_rejects_traversal`, `cargo test schema_v16_only` — each finds and passes exactly the intended test, confirmed above). No code change needed; this is a plan-authoring quirk in the verify script syntax, not a defect in the implementation. Worth fixing at the plan-template level for future plans that chain multiple `cargo test` name filters in one invocation.
- **ts-rs `export_to` path required one extra `../` than the plan's file-path table implied:** ts-rs 12 resolves a relative `export_to` against `<CARGO_MANIFEST_DIR>/bindings/`, not `<CARGO_MANIFEST_DIR>/`. `export_to = "../src/bindings/X.ts"` therefore landed at `app/src-tauri/src/bindings/X.ts` (wrong side of the Rust/frontend boundary) until corrected to `"../../src/bindings/X.ts"`, which correctly lands at `app/src/bindings/X.ts`. Purely a path-arithmetic correction, caught by inspecting the actually-generated file location before committing; no behavior change beyond output path.

## Known Stubs

- `Save` / `Save As` / `New Archive` toolbar buttons remain `disabled` — by design, 01-05 wires them. Not a defect (matches 01-01's `Known Stubs` note and this plan's explicit non-negotiable: "do not implement save here").
- `db/notes.rs` returns only located notes (no independent-notes UNION, no resources.db label resolution) — by design, per this plan's non-negotiables; 01-04 thickens the query. The fixture's independent note (`NoteId=2`) is present in `userData.db` but intentionally not yet surfaced by this thin query.
- `app/src/bindings/*.ts` are generated artifacts (ts-rs `// Do not edit this file manually`), committed so `npm run build` type-checks without requiring `cargo test` to have run first in a fresh checkout. Regenerated automatically whenever `cargo test` runs the `export_bindings_*` unit tests.

## Self-Check: PASSED

All 15 created files confirmed present on disk (`error.rs`, `session.rs`, `category.rs`, `archive/mod.rs`, `archive/extract.rs`, `db/mod.rs`, `db/notes.rs`, `error_tests.rs`, `category_tests.rs`, `extract_tests.rs`, `archive_validity_tests.rs`, `NotesList.tsx`, `bindings/Category.ts`, `bindings/NotesRow.ts`, `bindings/ErrorDto.ts`); both task commits (`e7a0aa03`, `039c7127`) confirmed in `git log`.
