---
phase: 06-full-data-browsing
plan: 02
subsystem: database
tags: [rusqlite, sqlite, browse, tauri-command, ts-rs, sql-port]

# Dependency graph
requires:
  - phase: 06-01
    provides: unified BrowseRow type + shared labels.rs (process_code/process_color/process_detail/resolve_publication) + Category enum
provides:
  - Five verbatim category getters (query_annotations/bookmarks/favorites/highlights/playlists) returning BrowseRow
  - Single generic list_category(category) Tauri command dispatching all six categories by the Category enum
  - Correct per-category identity PK surfaced as row.id (BookmarkId/TagMapId/BlockRangeId/LocationId/PlaylistItemId)
  - generate_v16_all_categories_fixture synthetic seed + browse_query_tests.rs identity-PK proof
affects: [phase-07-delete-edit, phase-06-frontend-browse-ui]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Static const &str SQL ported structurally-verbatim from Python get_* getters (zero interpolation)"
    - "Shared PubLabel synthesis helper factoring the notes.rs located-row pipeline across four located categories"
    - "One generic enum-keyed Tauri command instead of N per-category commands"

key-files:
  created:
    - app/src-tauri/src/db/browse.rs
    - app/src-tauri/tests/browse_query_tests.rs
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/common/mod.rs

key-decisions:
  - "Identity PK per category surfaced as row.id from the correct column (BookmarkId col 4, TagMapId col 4, BlockRangeId col 4), never the first-SELECTed LocationId — asserted by exact literal distinct from LocationId 500."
  - "Highlights ports the bare `code` (JWLManager.py:688, no `* OTHER *`-on-empty); Annotations/Bookmarks/Favorites apply the `* OTHER *`-on-empty rule per plan + Python `code or _('* OTHER *')`."
  - "Favorites note-tag exclusion tested with a REAL note-tag mapping (NoteId 700, LocationId NULL) — the TagMap one-of CHECK forbids a LocationId+NoteId row, so a valid archive excludes note-tags at the Location JOIN; WHERE tm.NoteId IS NULL ported verbatim."
  - "list_category dispatch reuses reload_notes connection-acquisition (Connection::open(session.db_path) + ResourceCatalog::load(path,'en')) inline in lib.rs; archive/mod.rs untouched."

patterns-established:
  - "PubLabel bundle + synthesize_pub_label(): single located-category label pipeline reused by 4 getters."
  - "resolve_language(catalog, meps, no_language_fallback): per-category miss sentinel ('* NO LANGUAGE *' for Annotations vs '#id' for the rest)."

requirements-completed: [DATA-02, DATA-03, DATA-04, DATA-05, DATA-06]

coverage:
  - id: D1
    description: "query_bookmarks returns the seeded Bookmark with row.id == BookmarkId (not LocationId)"
    requirement: "DATA-03"
    verification:
      - kind: integration
        ref: "tests/browse_query_tests.rs#bookmarks_query"
        status: pass
    human_judgment: false
  - id: D2
    description: "query_favorites returns the NULL-NoteId TagMap (row.id == TagMapId) and excludes note-tag mappings"
    requirement: "DATA-04"
    verification:
      - kind: integration
        ref: "tests/browse_query_tests.rs#favorites_query"
        status: pass
    human_judgment: false
  - id: D3
    description: "query_highlights yields one row per BlockRange (2 rows, distinct BlockRangeIds) with color"
    requirement: "DATA-02"
    verification:
      - kind: integration
        ref: "tests/browse_query_tests.rs#highlights_query"
        status: pass
    human_judgment: false
  - id: D4
    description: "query_annotations returns the InputField row with row.id == LocationId and synthesized labels"
    requirement: "DATA-05"
    verification:
      - kind: integration
        ref: "tests/browse_query_tests.rs#annotations_query"
        status: pass
    human_judgment: false
  - id: D5
    description: "query_playlists returns the PlaylistItem (row.id == PlaylistItemId) with labels synthesized without resources.db"
    requirement: "DATA-06"
    verification:
      - kind: integration
        ref: "tests/browse_query_tests.rs#playlists_query"
        status: pass
    human_judgment: false
  - id: D6
    description: "list_category(category) command registered once, dispatches all six getters keyed by the Category enum"
    verification:
      - kind: integration
        ref: "cargo test --jobs 2 (full workspace, lib + all integration binaries green)"
        status: pass
    human_judgment: false

# Metrics
duration: 30min
completed: 2026-07-23
status: complete
---

# Phase 6 Plan 02: Full Data Browsing (backend read side) Summary

**Five category queries (Highlights/Bookmarks/Annotations/Favorites/Playlists) ported verbatim from the Python `get_*` getters into `db/browse.rs`, each returning the unified `BrowseRow` with the CORRECT identity PK, dispatched through one generic `list_category` command and proven by per-category identity-PK tests.**

## Performance

- **Duration:** ~30 min
- **Completed:** 2026-07-23
- **Tasks:** 3/3
- **Files modified:** 5 (2 created, 3 modified)

## Accomplishments

- **`db/browse.rs`** — five public getters `query_annotations/bookmarks/favorites/highlights/playlists`, each `(&Connection, &ResourceCatalog) -> Result<Vec<BrowseRow>, ArchiveError>`. Each SQL is a static `const &str` ported structurally-verbatim from `JWLManager.py:643/656/669/682/770` (no interpolation → CLAUDE.md no-f-string rule satisfied trivially). Label synthesis reuses `labels.rs` for the four located categories; Playlists synthesizes with NO resources.db lookup (`Tag.Name`/`PlaylistItem.Label`). No `.unwrap()`/`.expect()` on the archive-data path (crate deny gate).
- **Identity PK correctness (load-bearing)** — each getter surfaces the FUNCTIONALITY-SPEC §3.3 dispatch key as `row.id`: Annotations=LocationId, Bookmarks=BookmarkId, Favorites=TagMapId, Highlights=BlockRangeId, Playlists=PlaylistItemId — never the first-SELECTed LocationId.
- **`list_category(category)`** — one generic `#[tauri::command]` keyed by the ts-rs `Category` enum (not six commands, never a translated display string), dispatching to all six getters (Notes reuses `db::notes::query_notes`), registered once in `generate_handler!`.
- **`browse_query_tests.rs` + `generate_v16_all_categories_fixture`** — one seeded row per category on a shared scripture Location (500, Genesis 1:1, English), each identity chosen distinct from LocationId 500 so an identity/join mix-up FAILS by exact-literal assert. Highlights: 1 UserMark × 2 BlockRanges → 2 rows with distinct BlockRangeIds. Favorites: excludes a real note-tag mapping. Playlists: labels without resources lookup.

## Verification Evidence (DoD)

- `cargo fmt --check` — clean (exit 0).
- `cargo clippy --all-targets -- -D warnings` — clean (exit 0). (One doc-list overindent lint surfaced in the new fixture doc comment and was fixed by reflowing to single-line bullets.)
- `cargo test --jobs 2` — full workspace GREEN. Counts: lib unit **40 passed**; `browse_query_tests` **5 passed** (annotations/bookmarks/favorites/highlights/playlists); `notes_query_tests` **1 passed** (no regression); all other integration binaries pass. Totals across the run: 0 failed, 6 ignored (pre-existing manual/PySide6 gates: differential 4, delete_roundtrip 1, trim 1). `--jobs 2` used per plan to avoid the linker OOM (os error 1455 — environment linker limit, not a code defect).
- No new dependency: `app/src-tauri/Cargo.toml` unchanged (T-06-SC accept verified).
- Bindings: neither `BrowseRow` nor `Category` changed, so no ts-rs binding regen and no `npm run build` required (plan's conditional not triggered); the `export_bindings_*` lib tests pass.

## Deviations from Plan

### Auto-fixed / plan-corrected

**1. [Rule 3 - Blocking] Corrected the Task 1/2 verify command package flag.**
- **Found during:** Task 1 build.
- **Issue:** The plan's `cargo build -p jwlmanager_lib` fails — `jwlmanager_lib` is the *lib name*, not the package name (package is `jwlmanager`).
- **Fix:** Built via `cargo build --lib` (equivalent, correct). No source impact.

**2. [Plan-directed] Highlights symbol handling kept faithful to Python `:688` (bare `code`).**
- The plan gives the `* OTHER *`-on-empty rule explicitly for Annotations and "same as Annotations" for Bookmarks; Python favorites also uses `code or _('* OTHER *')`. Highlights (`JWLManager.py:688`) uses a bare `code`, and the plan is silent on it — so `query_highlights` passes `other_on_empty=false` to preserve the Python behavior. Cosmetic only (affects the displayed symbol solely in the empty-KeySymbol edge case); identity PK unaffected. Tests use non-empty symbols.

**3. [Fixture constraint] Favorites note-tag exclusion fixture shape.**
- **Found during:** Task 3 fixture design.
- **Issue:** The plan's illustrative "TagMap with LocationId + NoteId" is impossible in a valid v16 archive — the TagMap CHECK enforces exactly one of PlaylistItemId/LocationId/NoteId non-null.
- **Fix:** Seeded a REAL note-tag mapping (`TagMap 623`, NoteId 700, LocationId NULL). Favorites excludes it (Location JOIN + the verbatim `WHERE tm.NoteId IS NULL`); the test asserts exactly 1 favorite row (id 622) and that neither LocationId 500 nor TagMapId 623 appears. Fixture stays constraint-valid per the file's own bright-line guidance.

No other deviations — the five SQL getters, the `list_category` dispatch, and the identity-PK asserts landed as specified.

## Known Stubs

None. Every getter is fully wired to real archive queries; no placeholder/empty-data paths introduced.

## Threat Flags

None. No new network endpoints, auth paths, or trust-boundary surface. The five getter SQLs are static `const &str` with zero interpolation (T-06-03); archive-data reads are `Option`/`unwrap_or`-defaulted with typed `ArchiveError` propagation (T-06-04); only names/refs metadata are read, never publication body text (T-06-05).

## Self-Check: PASSED

- `app/src-tauri/src/db/browse.rs` — FOUND
- `app/src-tauri/tests/browse_query_tests.rs` — FOUND
- Commit f71c5bf (Task 1), a88d07d (Task 2), 45e95c2 (Task 3) — FOUND
