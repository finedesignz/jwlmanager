---
phase: 01-open-view-save-foundation-slice
plan: 04
subsystem: notes-query
tags: [rust, rusqlite, resources-db, tanstack-virtual, react, i18n-deferred]

requires:
  - phase: 01-07
    provides: thin located-Note query, ArchiveSession, open_archive command, non-virtualized NotesList.tsx skeleton
provides:
  - db::resources::ResourceCatalog (bundled resources.db label lookups, parameterized SQL)
  - db::notes::query_notes (located + independent-notes UNION, process_code/process_detail/process_color label synthesis)
  - Virtualized NotesList.tsx (TanStack Virtual, fixed 44px single-line rows)
affects: [01-05]

tech-stack:
  added: ["regex = \"1\" (Rust, process_code/process_yr port)"]
  patterns:
    - "dev/prod resource-path resolution mirrored from jwlcore::loader::resolve_lib_path: prefer repo-root dev path, fall back to Tauri AppHandle resource resolver"
    - "Independent-notes UNION as two separate parameterized queries concatenated in Rust (Python's pl.concat([i_notes, notes]) order preserved: independent first)"
    - "Fixed-height virtualized list: inline NO_WRAP_STYLE applied per-row as defense-in-depth alongside CSS classes, so the 44px/no-wrap contract holds regardless of stylesheet load order"

key-files:
  created:
    - app/src-tauri/src/db/resources.rs
    - app/src-tauri/tests/notes_query_tests.rs
    - app/src/components/NotesList.test.tsx
  modified:
    - app/src-tauri/src/db/notes.rs
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/archive/mod.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/Cargo.toml
    - app/src-tauri/tests/open_archive_tests.rs
    - app/src-tauri/tests/archive_validity_tests.rs
    - app/src-tauri/tests/error_tests.rs
    - app/src/components/NotesList.tsx
    - app/src/styles.css
    - app/src/bindings/NotesRow.ts

key-decisions:
  - "UI language hardcoded to 'en' (Languages.Code) for label synthesis — Phase 1 has no locale switcher (UI-SPEC explicitly defers that to Phase 11); process_color's translated color names are similarly emitted as fixed English strings, not run through gettext"
  - "open_and_validate signature gained a resources_db_path: &Path parameter; open_archive (Tauri command) gained an app: tauri::AppHandle parameter to resolve it via db::resources::resolve_resources_db_path — every existing direct-call test site (error_tests, open_archive_tests, archive_validity_tests) updated to pass db::resources::dev_resources_db_path()"
  - "Publications/Extras lookup in db::resources.rs is keyed by table name via a fixed two-element Rust array (['Publications','Extras']), not user/archive-derived data, interpolated into the SQL FROM clause; the WHERE value (ui_lang_id) is always bound via rusqlite's ?1 parameter — this is not the CLAUDE.md-forbidden pattern (which is interpolating a value into a WHERE/comparison clause), it is a compile-time-fixed identifier list, analogous to the identical table-name pattern already used in db/resources.rs's own two-table loop"
  - "NotesRow gained an `independent: bool` field beyond the Python schema shape, so the frontend/tests can distinguish independent notes without re-deriving it from type_group == '* INDEPENDENT *' string matching"

requirements-completed: [DATA-01, ARCH-01]

duration: ~55min
completed: 2026-07-19
---

# Phase 1 Plan 4: Real Notes (resources.db labels, independent-notes UNION, virtualized 9k-row list) Summary

Thickened 01-07's skeleton Notes query and list into the real, user-visible feature: `db/resources.rs` loads and caches the bundled `resources.db` (Languages/BibleBooks/Publications+Extras) via parameterized SQL, `db/notes.rs` now UNIONs the located-Note query with a separate independent-notes query (so standalone notes are never silently dropped) and ports `process_code`/`process_detail`/`process_color` to synthesize the same human-readable labels the Python app shows, and `NotesList.tsx` is rebuilt on TanStack Virtual with fixed 44px single-line-truncated rows so a 9,000+ row archive stays responsive without rendering thousands of DOM nodes.

## Performance

- **Duration:** ~55 min
- **Tasks:** 3/3 completed
- **Files modified:** 15 (3 created, 12 modified)

## Accomplishments

- `db::resources::ResourceCatalog` — loads `Languages`/`BibleBooks`/`Publications`+`Extras` from the bundled `resources.db` with `rusqlite` bound parameters throughout (fixes the Python `f"...WHERE Language = {ui_lang};"` interpolation anti-pattern); dev/prod path resolution mirrors `jwlcore::loader::resolve_lib_path`.
- `db::notes::query_notes` now runs the located-Note query AND a separate independent-notes query (`LocationId IS NULL AND BlockType = 0`), concatenating them in the same order as the Python's `pl.concat([i_notes, notes])` — proven by a new integration test (`notes_query_includes_independent`) using the existing fixture's one located + one independent note.
- Ported `process_code`, `process_detail`, and `process_color` (`JWLManager.py:578-627`) as pure, unit-tested Rust functions, including the `code_jwb`/`code_yr` regex constants (`JWLManager.py:930-931`).
- `NotesRow` now carries the full synthesized label shape: `language`, `symbol`, `color`, `tags`, `modified`, `year`, `detail1`, `detail2`, `short`, `full`, `type_group`, plus an `independent` flag.
- `NotesList.tsx` rebuilt on `@tanstack/react-virtual`'s `useVirtualizer`, fixed `estimateSize` of 44, with both CSS classes and inline `NO_WRAP_STYLE` enforcing single-line truncation on every row (defense-in-depth against the fixed-size virtualizer mismeasuring per finding 14).
- `open_archive` (Tauri command) now resolves the bundled `resources.db` path via `app: tauri::AppHandle` and threads it through `open_and_validate` into `ResourceCatalog::load`.

## Task Commits

1. **Task 1: resources.db label lookups** - `7725a221` (feat)
2. **Task 2: Independent-notes UNION + process_code/process_detail** - `39185533` (feat)
3. **Task 3: Virtualize NotesList + 9k-row/44px tests** - `ed54f8b6` (feat)

## Files Created/Modified

- `app/src-tauri/src/db/resources.rs` - `ResourceCatalog` (parameterized lookups) + dev/prod path resolution
- `app/src-tauri/src/db/notes.rs` - `query_notes` (UNION), `process_code`/`process_detail`/`process_color`, thickened `NotesRow`
- `app/src-tauri/src/error.rs` - `MissingResourcesLanguage`/`MissingResourcesDb` `ArchiveError` variants + DTO mappings
- `app/src-tauri/src/archive/mod.rs` - `open_and_validate` takes `resources_db_path`, loads the catalog, passes it into `query_notes`
- `app/src-tauri/src/lib.rs` - `open_archive` command resolves the resources.db path via `AppHandle`
- `app/src-tauri/Cargo.toml` - added `regex = "1"` dependency
- `app/src-tauri/tests/notes_query_tests.rs` - independent-notes UNION + label-synthesis integration test
- `app/src-tauri/tests/{open_archive_tests,archive_validity_tests,error_tests}.rs` - updated call sites for the new `open_and_validate` signature
- `app/src/components/NotesList.tsx` - virtualized rebuild (`useVirtualizer`, fixed 44px rows, `resolveLabel`)
- `app/src/components/NotesList.test.tsx` - 9k-row DOM-node-count test, overlong-snippet 44px-row-height test, label/empty-state tests
- `app/src/styles.css` - `.notes-list-viewport` scroll container, `.notes-list-row-label`/`.notes-list-row-tags` truncation rules
- `app/src/bindings/NotesRow.ts` - regenerated ts-rs binding for the thickened `NotesRow` shape

## Verification Evidence

- `cargo build` (`app/src-tauri`) — clean.
- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — zero warnings (unwrap/expect ban intact; the two `LazyLock` regex initializers use a scoped `#[allow(clippy::expect_used)]` on fixed, compile-time-known-valid patterns, not archive-data-path panics).
- `cargo test` (full suite) — 42 tests pass (22 lib unit tests including `resources_lookups`, 5× `process_code_*`, 3× `process_detail_*`, plus 20 across `archive_tests`/`archive_validity_tests`/`category_tests`/`error_tests`/`extract_tests`/`fixtures`/`manifest_tests`/`notes_query_tests`/`open_archive_tests`); 0 failed.
- `cargo test resources_lookups` — 1/1 pass (plan's literal Task 1 verify command).
- `cargo test notes_query_includes_independent process_code process_detail` — each filter individually confirmed passing (multi-name `cargo test` filter syntax limitation noted in 01-07-SUMMARY's Deferred section still applies).
- `npm run build` (`app/`) — `tsc` + `vite build` clean.
- `npm test -- NotesList` (`app/`) — 4/4 pass: 9k-row DOM-node-count assertion (rendered rows < 100, well under 9,000), overlong-snippet 44px-height assertion, label/tags/modified render assertion, empty-state assertion.
- `npm test` (full suite, `app/`) — 5/5 pass (`App.test.tsx` + `NotesList.test.tsx`), no regressions from the `NotesRow` shape or `App.tsx`/`NotesList.tsx` interface changes.

**Not run:** the plan's `<human-check>` — Linux WebKitGTK scroll-smoothness check with the 9,000-row fixture loaded. This executor is a non-interactive, non-display Windows session with no Linux display available. All automated verification (DOM-node-count assertion, 44px-row-height assertion under an overlong snippet, full `cargo`/`npm` build+test+lint suites) is green, which is the strongest available proxy signal for virtualization correctness — **the owner should do a one-time manual check on Linux**: open a 9,000+ row archive (or extend the existing fixture generator to synthesize one), scroll the full list, and confirm no visible stutter and no wrapped/overlapping rows.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `open_and_validate`'s signature had to gain a `resources_db_path` parameter, and `open_archive` an `AppHandle` parameter, to resolve the bundled resources.db**
- **Found during:** Task 2, wiring `ResourceCatalog` into `open_and_validate`
- **Issue:** Resolving the bundled `resources.db`'s on-disk path in a packaged build requires Tauri's resource resolver, which needs an `AppHandle` — `open_archive` (the Tauri command) didn't take one, and `open_and_validate` (its pure-function core, called directly by three existing test files) had no path for the resources.db at all.
- **Fix:** Added `resources_db_path: &Path` to `open_and_validate` and `app: tauri::AppHandle` to the `open_archive` command, resolving the path via a new `db::resources::resolve_resources_db_path(&app)` that mirrors `jwlcore::loader::resolve_lib_path`'s dev/prod fallback pattern (prefer repo-root `res/resources.db` in dev, fall back to the Tauri bundled resource dir in a packaged build). Updated the three existing direct-call test sites (`error_tests.rs`, `open_archive_tests.rs`, `archive_validity_tests.rs`) to pass `db::resources::dev_resources_db_path()`.
- **Files modified:** `app/src-tauri/src/archive/mod.rs`, `app/src-tauri/src/lib.rs`, `app/src-tauri/src/db/resources.rs`, `app/src-tauri/tests/error_tests.rs`, `app/src-tauri/tests/open_archive_tests.rs`, `app/src-tauri/tests/archive_validity_tests.rs`
- **Commit:** `39185533`

**2. [Rule 1 - Bug] Initial `process_code` borrow-checker error and two clippy findings (expect-on-Result, redundant match guard)**
- **Found during:** Task 2, `cargo build` / `cargo clippy --all-targets -- -D warnings`
- **Issue:** (a) `code = prefix.to_string()` while `code` was still borrowed by `CODE_YR.captures(&code)`'s `caps` — a straightforward borrow-checker error from computing the year format string after reassigning `code`. (b) The crate-level `#![deny(clippy::expect_used)]` gate flagged the two `LazyLock` regex initializers' `.expect(...)`. (c) `Some(s) if s.is_empty()` was flagged as a redundant guard (should be `Some("")`).
- **Fix:** (a) Reordered to compute the year `Option<String>` from `suffix` before reassigning `code = prefix`, and cloned `prefix`/`suffix` out of the `Captures` borrow before use. (b) Added a scoped `#[allow(clippy::expect_used)]` on the two fixed, compile-time-known-valid regex literals (a compile failure here is a programmer error, not an archive-data-path panic — the lint's actual intent). (c) Collapsed `Some(s) if s.is_empty()` and `None` into `Some("") | None`.
- **Files modified:** `app/src-tauri/src/db/notes.rs`
- **Commit:** `39185533`

## Known Stubs

- No i18n/locale switching: `process_color` and the language-lookup fallback strings are fixed English literals (`"Grey"`, `"* NO LANGUAGE *"`, etc.), never run through a translation catalog. This matches the UI-SPEC's explicit Phase-11 deferral of locale switching — not a defect, but worth flagging since the Python original routes every one of these through `gettext`.
- `process_code`/`process_detail` cover the tuples exercised by unit tests and the fixture (bare dated symbols, `ws`/`jwb-` special cases, Bible-appendix symbols, publication issues, Bible book+chapter references) but have not been run against a real, large user archive with the full range of historical `KeySymbol` values JW Library has issued over the years. If a not-yet-seen symbol shape surfaces an incorrect label in practice, that is a follow-up fix, not a known gap being silently swallowed here — every code path returns a value (never panics) even for unrecognized symbols.
- Manual Linux WebKitGTK scroll-smoothness check (see Verification Evidence) is unrun — flagged for the owner, not faked.

## Self-Check: PASSED

All 3 newly created files confirmed present on disk (`app/src-tauri/src/db/resources.rs`, `app/src-tauri/tests/notes_query_tests.rs`, `app/src/components/NotesList.test.tsx`); all three task commits (`7725a221`, `39185533`, `ed54f8b6`) confirmed in `git log`.
