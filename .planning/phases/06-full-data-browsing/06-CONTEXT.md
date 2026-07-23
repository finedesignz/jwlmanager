# Phase 6: Full Data Browsing - Context

**Gathered:** 2026-07-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 1 shipped a single category — **Notes** — as a virtualized, selectable list. Phase 6 generalizes that one proven slice to **all six** categories the archive holds: **Annotations, Bookmarks, Favorites, Highlights, Notes, Playlists**. A user picks a category, sees that category's real archive data rendered from the same resources.db label synthesis Notes already uses, selects one or many items, and the contextual operation set the UI presents updates with the current (category, selection) — e.g. a bulk-delete affordance is present only when items are selected and only for a category that supports it.

This is a **BROWSE + SELECT + surface-valid-operations** phase. It ports the five not-yet-built category queries, generalizes the list/selection/virtualization component, adds a category switcher, and models per-category operation capability. It does **not** implement any new mutation: editing (color, tags, order, favorites, clean/mask, raw record edit) is Phase 7; import/export is Phase 8. The only mutation that stays live is the Notes delete already shipped in Phase 2.

**In scope:**
- Port the 5 remaining category queries verbatim from the Python `get_*` getters (Notes is already done): Annotations (`JWLManager.py:641-652`), Bookmarks (`:654-665`), Favorites (`:667-678`), Highlights (`:680-692`), Playlists (`:768-775`). Each as a static, fully-parameter-free `&str` SQL const (they take no runtime parameters — no interpolation of any kind).
- Extract the shared label-synthesis helpers (`process_code`, `process_detail`, `process_color`, `resolve_publication`) currently PRIVATE inside `db/notes.rs` into a shared `db` module reused by every category getter (they are byte-for-byte the same math every category runs — Python shares them as closures inside `regroup`).
- One unified over-IPC row shape (the 12-field post-`merge_df` schema every category collapses to) exported via ts-rs, replacing/superseding the Notes-only `NotesRow`.
- A generic `list_category(category)` Tauri command dispatching to the per-category getter; `open_archive` continues to return the initial (Notes) view.
- A generalized virtualized list component (TanStack Virtual, fixed-height rows) — the Notes list generalized, NOT a second design system.
- A category switcher control (the six categories) that re-queries and resets selection on switch.
- Per-category selection: a `Set<PK>` keyed by that category's identity column (Annotations→LocationId, Bookmarks→BookmarkId, Favorites→TagMapId, Highlights→BlockRangeId, Notes→NoteId, Playlists→PlaylistItemId), reset on category switch.
- Per-category empty state.
- An operation-capability model (per category × selection) that drives the contextual operation set. The one operation WIRED to a real backend mutation remains Notes delete (Phase 2); other operations are surfaced per capability but not yet executable (Phase 7/8).
- Per-category query tests over synthetic fixtures + frontend list/selection/switcher/virtualization vitest.

**Out of scope (own phases / deferred):**
- Any new mutation: color change, tagging, reorder, favorite add, clean/mask, raw-record edit → **Phase 7**.
- Per-category delete backends (Bookmark/Favorite/Highlight/Annotation/Playlist delete) → **Phase 7** (only Notes delete, from Phase 2, is live here).
- Import / export of any category → **Phase 8**.
- The duplicate-notes CTE filter (`self.dupes`, `JWLManager.py:707-750`), grouping/tree hierarchy (`combo_grouping`), title-view modes, sort — all Phase 7+/polish; Phase 6 renders each category as a FLAT selectable list, same as Phase 1 Notes.
- Data-viewer / raw record inspector (`data_viewer`, `:2697+`) → Phase 7.
- Localization of the synthesized labels (English strings only, per Phase 1) → Phase 11.

**Requirements:** DATA-02, DATA-03, DATA-04, DATA-05, DATA-06, DATA-07 (per ROADMAP Phase 6).

**Depends on:** Phase 1 (`db/notes.rs` query + `process_*` helpers to extract, `db/resources.rs` `ResourceCatalog`, `Category` enum already listing all six, `NotesRow`/`NotesList.tsx` TanStack-Virtual list, `open_archive`/`list_notes` commands, `ErrorDto`/`ErrorBanner`, `ArchiveSession`/`SessionState`), Phase 2 (`NonEmptyNoteIds`, `DryRunReport`, `DeletePreviewDialog`, delete-for-Notes — the reference for the ONE live operation). All complete.

</domain>

<decisions>
## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit.

### The query layer — generalize `notes.rs`, don't fork it (DATA-02..DATA-06)

- **D6-01 (one shared label-synthesis module):** `process_code`, `process_detail`, `process_color`, `resolve_publication`, plus the `CODE_YR`/`CODE_JWB` regexes and the `DATED_PREFIX_EXCLUDED`/`BIBLE_APPENDIX_SYMBOLS`/`COLOR_NAMES` consts are currently PRIVATE `fn`s inside `db/notes.rs`. Every other category getter runs the SAME `process_code`/`process_detail`/`process_color`/`merge_df` math (Python defines them ONCE as closures inside `regroup` and all six getters call them — `JWLManager.py:578-639`). Extract these into a shared `db/labels.rs` (`pub(crate)`), and have `notes.rs` + the new category getters import them. Do NOT copy-paste the label math per category.
  `[auto] shared helpers — Q: "Copy process_* per category, or extract to a shared module?" → Selected: "Extract to db/labels.rs, reuse across all getters" (recommended default)`
  **Rationale:** Six divergent copies of the year/detail/color derivation would drift; the Python keeps one copy. Extraction is a pure refactor of already-tested code (`notes.rs` unit tests move with it).

- **D6-02 (one unified row struct, mirroring `merge_df`'s single frame):** The Python runs every category through `merge_df` (`:629-639`), which collapses all six into ONE polars schema: `Id, Language, Symbol, Color, Tags, Modified, Year, Detail1, Detail2, Short, Full, Type`. `NotesRow` (`db/notes.rs:21-44`) is already exactly this shape (+ an `independent` flag). Generalize it into a single `BrowseRow` (rename or supersede `NotesRow`) reused by all six getters; each getter fills the subset of columns its category produces and defaults the rest (Bookmarks/Annotations/Favorites have no Color/Tags/Modified; Playlists has no Language/Color and carries its Label in `Detail1`). Keep `independent` (only Notes sets it true).
  `[auto] row shape — Q: "Per-category row structs, or one unified BrowseRow?" → Selected: "One unified BrowseRow (the merge_df schema)" (recommended default)`
  **Rationale:** The Python's own data model is a single unified frame; per-category structs would multiply ts-rs bindings and force six frontend renderers. One row shape = one list renderer. Fields a category doesn't use are `Option`/empty, exactly as `merge_df`'s `fill_null` does.

- **D6-03 (the five queries — verbatim, static, parameter-free):** Port each getter's SQL as a `const &str`, structurally identical to the Python. None of these five take a runtime parameter, so there is nothing to interpolate — they are fixed query text (the CLAUDE.md no-f-string rule is satisfied trivially; the only bound values in the whole browse path are the resources.db `ui_lang_id` lookups, already parameterized in `resources.rs`):
  - **Annotations** (`:643`): `SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, TextTag, l.BookNumber, l.ChapterNumber, l.Title FROM InputField JOIN Location l USING (LocationId)` — identity = **LocationId** (one row per InputField, but selection/delete key is LocationId per §3.3).
  - **Bookmarks** (`:656`): `SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, BookmarkId, l.BookNumber, l.ChapterNumber, l.Title FROM Bookmark b JOIN Location l USING (LocationId)` — identity = **BookmarkId**.
  - **Favorites** (`:669`): `SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, TagMapId FROM TagMap tm JOIN Location l USING (LocationId) WHERE tm.NoteId IS NULL ORDER BY tm.Position` — identity = **TagMapId**; a Favorite is a TagMap row with NULL NoteId; `process_detail` called with `book=None, chapter=None`.
  - **Highlights** (`:682`): `SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, b.BlockRangeId, u.UserMarkId, u.ColorIndex, l.BookNumber, l.ChapterNumber FROM UserMark u JOIN Location l USING (LocationId), BlockRange b USING (UserMarkId)` — identity = **BlockRangeId**; one row PER BlockRange (a multi-block highlight is multiple rows); carries `Color` via `process_color`.
  - **Playlists** (`:770`): `SELECT PlaylistItemId, Name, Position, Label FROM PlaylistItem JOIN TagMap USING (PlaylistItemId) JOIN Tag t USING (TagId) WHERE t.Type = 2 ORDER BY Name, Position` — identity = **PlaylistItemId**; Language=None, Symbol=`* OTHER *`, Tags=`Name` (playlist tag), Detail1=`Label`.
  **Notes is already ported** (`db/notes.rs`, `:694-767`) — reuse as-is under the generalized row struct.

- **D6-04 (resources.db lookups — needed for 4 of the 5 new categories):** Annotations, Bookmarks, Favorites, and Highlights all run `process_detail` (BibleBooks name lookup), `merge_df` (Publications/Extras title+type+year lookup by Symbol), and `lang_name` (Languages) — so they REQUIRE the `ResourceCatalog` exactly like Notes. **Playlists needs NO resources.db lookup**: it emits `Language=None`, `Symbol='* OTHER *'`, uses no Location, and its `merge_df` join finds nothing and falls back — its labels come entirely from `PlaylistItem.Name`/`Label`. Pass the already-loaded `ResourceCatalog` to every getter uniformly (harmless for Playlists); do not special-case loading. Only publication **names/refs** (metadata) are read — never publication body text (project constraint).

### Selection + category switching (DATA-07, criterion 2)

- **D6-05 (per-category selection keyed by the identity PK, reset on switch):** Generalize `NotesList`'s `useState<Set<bigint>>` selection (`NotesList.tsx:65`). The Set holds that category's identity column values (D6-03). On category switch the selection MUST reset to empty (a BookmarkId means nothing in the Highlights list). Selection survives virtualization (keyed by PK, not row index) — the existing pattern already guarantees this. Single-select and multi-select are the same checkbox mechanism; no separate "single" mode.
  **Critical invariant:** the selection PK per category MUST equal the future delete/edit dispatch key (§3.3 / `delete_items` `:3658-3671`) — Annotations delete keys on LocationId, Favorites on TagMapId, Highlights on BlockRangeId, etc. Getting the identity column wrong here silently mis-targets every Phase 7 mutation.

- **D6-06 (category switcher = a segmented control over the six enum variants):** A control listing the six `Category` variants; selecting one invokes `list_category(category)`, swaps the rendered rows, and resets selection. Drive it off the single-sourced `Category` enum (already exported via ts-rs, `category.rs`), NEVER off translated display strings (the Python's `if category == _('Notes')` string-compare is the exact latent-bug class the enum exists to kill — `category.rs` docstring). Persisted last-category is a nicety, deferred (Phase 11 settings).

### The list + virtualization (DATA-02, Linux perf constraint)

- **D6-07 (generalize `NotesList` → one virtualized `CategoryList`; TanStack Virtual is MANDATORY, not optional):** `NotesList.tsx` already uses `@tanstack/react-virtual` (`useVirtualizer`, fixed 44px `ROW_HEIGHT`, `overscan: 8`, no-wrap rows) precisely because Linux WebKitGTK degrades on DOM-heavy grids (CLAUDE.md platform constraint; `NotesList.tsx:47-62` cites it). Generalize this ONE component to render `BrowseRow[]` for any category — do NOT hand-roll a second list or drop virtualization for the "smaller" categories (Highlights can be as large as Notes; Bookmarks/Favorites are usually small but MUST still virtualize for consistency and to keep one code path). Every category row stays a fixed height, single-line, no-wrap so the fixed-size virtualizer never mismeasures (finding 14, Phase 1). The row renderer shows: optional checkbox, resolved label (`Full`/`Detail1`/`Detail2`), optional color swatch (Notes/Highlights), optional tags (Notes/Playlists), optional modified (Notes) — columns present per category, absent columns simply not rendered.
  `[auto] list component — Q: "New per-category lists, or generalize NotesList?" → Selected: "Generalize NotesList into CategoryList" (recommended default)`
  **Rationale:** One virtualized component, one perf story, one set of tests. The design language (dark tokens, 44px rows, `toolbar-button`, `styles.css`) is already established — reuse it, invent nothing.

### Contextual operations (DATA-07, criterion 3)

- **D6-08 (an operation-capability descriptor drives the contextual set; only Notes-delete is wired):** Model per-category operation capability from the Python `disable_options` table (`JWLManager.py:510-547`, FUNCTIONALITY-SPEC §1.2): which operations a category supports (view/color/tag/add/export/import/delete) and which additionally require a non-empty selection (`tree_selection`, `:497-505`: delete/export need selection; view/color/tag also need it AND a supporting category). The contextual operation set the UI renders = `f(category, selection.size)`. For Phase 6, the ONE operation bound to a real backend mutation is **Delete, for Notes only** (Phase 2's `delete_notes_dry_run`/`delete_notes_apply` + `DeletePreviewDialog`). Every other operation is surfaced per capability but rendered NOT-YET-AVAILABLE (disabled, marked deferred to Phase 7/8) — Phase 6 builds the descriptor and the selection-gated presentation, not the Phase 7 mutations.
  `[auto] valid-ops surfacing — Q: "Wire all per-category deletes now, or surface capability + keep only Notes-delete live?" → Selected: "Capability descriptor + Notes-delete live; others surfaced-but-deferred" (recommended default)`
  **Rationale:** Criterion 3 is satisfied by the operation set VISIBLY updating with (category, selection) — demonstrable via Notes delete appearing only when Notes rows are selected. Implementing Bookmark/Favorite/Highlight/Annotation/Playlist deletes now would be pulling Phase 7 forward and would need each one's own dry-run/round-trip safety net (Phase 2 rigor per category). Keep the boundary clean: descriptor now, mutations in Phase 7. Whether deferred ops render disabled-with-tooltip vs hidden is Claude's discretion.
  **Note (no empty-selection hazard):** the Python builds `IN (...)` by string-mangling a Python list and relies on buttons being disabled to avoid `IN ()` (wart #20, FUNCTIONALITY-SPEC §4). This rewrite already dodges that via `NonEmptyNoteIds` (empty selection unrepresentable at IPC deserialization) — carry that posture to any future per-category delete.

### Backend command surface

- **D6-09 (one generic `list_category` command, not six):** Add a single `#[tauri::command] fn list_category(category: Category, ...) -> Result<Vec<BrowseRow>, ErrorDto>` that locks the session, resolves the `ResourceCatalog`, and dispatches to the per-category getter. `open_archive` keeps returning the initial Notes view (back-compat with the existing frontend open flow); the switcher calls `list_category` thereafter. `list_notes` (the merge-refresh command) can be re-expressed as `list_category(Notes)` or left as-is (discretion). Keep `lib.rs`'s `generate_handler![]` growth minimal — one new command, not six.
  **Rationale:** Six near-identical commands bloat the handler registry and the IPC surface. One command keyed by the already-exported `Category` enum matches the Python's single `regroup`/`switchboard` dispatch.

### Verification (QA — synthetic fixtures only)

- **D6-10 (per-category query tests over an extended synthetic fixture):** Extend the synthetic v16 fixture generator (`tests/common/mod.rs`) to seed at least one row per category: a Bookmark, a Favorite (TagMap with NULL NoteId, a `Type=0`/`Name='Favorite'` tag), a Highlight (UserMark + BlockRange), an Annotation (InputField), and a Playlist (PlaylistItem + `Tag Type=2` + TagMap). Building blocks already exist in the downgrade fixtures (`generate_v16_collision_fixture` seeds Bookmark/UserMark/BlockRange/InputField/TagMap/PlaylistItemLocationMap — `tests/common/mod.rs:540-581`). For each category assert: the query returns the seeded row, the identity PK column is correct (§3.3), labels are synthesized via resources.db (not raw IDs), and Highlights yields one row per BlockRange. Frontend vitest: category switch swaps rows + resets selection, per-category multi-select, virtualization renders only viewport rows, contextual operation set updates with (category, selection) — generalizing `NotesList.test.tsx`/`CommandBar.test.tsx` patterns. **Synthetic fixtures ONLY — never a real `.jwlibrary`** (the git-tracked-archive bright-line test `test_no_real_archive_is_tracked_in_git` already guards this).

### Claude's Discretion
Module layout (`db/labels.rs` + one `db/browse.rs` with per-category getters, vs a file per category under `db/browse/`; recommend `db/labels.rs` + `db/browse.rs`), whether to rename `NotesRow`→`BrowseRow` or add a superseding alias, exact frontend component names (`CategoryList` generalizing `NotesList`, `CategorySwitcher`), whether deferred operations render disabled-with-tooltip vs hidden, whether the color swatch is a real color chip or a text label for now, the switcher's exact form (segmented buttons vs `<select>`), and the precise column layout per category within the shared row renderer.

</decisions>

<canonical_refs>
## Canonical References — downstream agents MUST read

### The per-category query source of truth (port verbatim)
- `JWLManager.py:641-652` — `get_annotations` SQL + processing (identity LocationId).
- `JWLManager.py:654-665` — `get_bookmarks` SQL + processing (identity BookmarkId).
- `JWLManager.py:667-678` — `get_favorites` SQL (`WHERE tm.NoteId IS NULL ORDER BY tm.Position`, identity TagMapId).
- `JWLManager.py:680-692` — `get_highlights` SQL (UserMark×BlockRange, one row per BlockRange, identity BlockRangeId, carries Color).
- `JWLManager.py:768-775` — `get_playlists` SQL (`Tag.Type=2`, identity PlaylistItemId, no resources lookup).
- `JWLManager.py:629-639` — `merge_df`: the single unified schema + publication join + `fill_null` fallbacks every category collapses to (D6-02).
- `JWLManager.py:578-627` — `process_code`/`process_color`/`process_detail`: the shared label math (already ported private in `db/notes.rs`; extract per D6-01).
- `.planning/research/FUNCTIONALITY-SPEC.md` §3.3 (category → identity key table), §3.4 (category query definitions), §1.2 (`disable_options` capability table).

### The Phase 1 template to generalize
- `app/src-tauri/src/db/notes.rs` — the reference getter: `NotesRow` (the 12-field row, `:21-44`), `process_code`/`process_detail`/`process_color`/`resolve_publication` (`:94-217`, extract to `db/labels.rs`), the located+independent query pattern (`:341-348`).
- `app/src-tauri/src/db/resources.rs` — `ResourceCatalog::load` (Languages/BibleBooks/Publications+Extras), all parameterized; pass to every getter (D6-04).
- `app/src-tauri/src/category.rs` — the `Category` enum (all six variants already present, ts-rs exported); the switcher + `list_category` dispatch key (D6-05, D6-09).
- `app/src/components/NotesList.tsx` — TanStack-Virtual list + `Set<bigint>` selection + selection-gated toolbar to generalize into `CategoryList` (D6-05, D6-07).
- `app/src/components/CommandBar.tsx` — the toolbar/pending/dialog-wiring style to match; where a `CategorySwitcher` sits alongside file actions.
- `app/src/App.tsx` — top-level state (`notes`/`error`); generalize to hold current category + rows + per-category selection.
- `app/src-tauri/src/lib.rs:36-75` — `open_archive`/`list_notes` command shape (session lock, resources resolve, `ErrorDto` mapping) to mirror for `list_category`; `:344` `generate_handler![]` registration.

### Selection + delete reference (the ONE live operation)
- `app/src-tauri/src/db/delete.rs` — `NonEmptyNoteIds` (empty-selection-unrepresentable, `:54-85`), `DryRunReport`, dry-run pattern; the Notes-delete backend surfaced in the contextual set (D6-08).
- `app/src/components/DeletePreviewDialog.tsx` + `NotesList.tsx:88-119` — the delete confirm/cancel flow reused unchanged for Notes.
- `JWLManager.py:510-547` (`disable_options`/`switchboard`) + `:497-505` (`tree_selection`) — the per-category × selection capability matrix for the operation descriptor (D6-08).

### Test scaffolding
- `app/src-tauri/tests/notes_query_tests.rs` — the per-category query-test template (fixture → extract → query → assert labels/identity).
- `app/src-tauri/tests/common/mod.rs:540-581` — existing multi-category fixture seeding (Bookmark/UserMark/BlockRange/InputField/TagMap/PlaylistItemLocationMap) to extend for D6-10.
- `app/src/components/NotesList.test.tsx`, `CommandBar.test.tsx` — frontend selection/virtualization/toolbar vitest patterns to generalize.

</canonical_refs>

<code_context>
## Existing Code Insights
- The hard part (resources.db label synthesis, `process_code`/`detail`/`color`, TanStack-Virtual list, `Set`-based selection, `ErrorDto` surface, the `Category` enum, session locking) is ALREADY built and tested for Notes. Phase 6 is "run the same machinery over five more SQL statements," not new architecture.
- The Python defines `process_*` + `merge_df` ONCE and all six getters call them; the Rust port currently hides them inside `notes.rs`. Extraction (D6-01) is the enabling refactor.
- `merge_df` proves the six categories share ONE row schema — the unified `BrowseRow` (D6-02) is faithful to the source, not an invention.
- The `Category` enum already enumerates all six and is ts-rs exported — the switcher and `list_category` dispatch are mostly wiring.

## Established Patterns
- Typed errors (`ErrorDto`), no `unwrap`/`panic` on the archive-data path.
- All SQL parameterized; here the five category queries are static/parameter-free, and the only bound values (resources.db `ui_lang_id`) are already parameterized in `resources.rs`.
- Virtualize every list (TanStack Virtual, fixed row height, no-wrap) — mandatory for Linux WebKitGTK.
- Single-sourced `Category` enum drives control flow; translated strings never do (kills the Python `if category == _('Notes')` bug class).
- Synthetic fixtures only; a git-tracked real archive fails the build.

## Integration Point / risk
- **Identity-column correctness is the load-bearing risk.** Each category's selection PK (Annotations→LocationId, Bookmarks→BookmarkId, Favorites→TagMapId, Highlights→BlockRangeId, Notes→NoteId, Playlists→PlaylistItemId) MUST match the future Phase 7 delete/edit dispatch key (§3.3, `delete_items:3658-3671`). A wrong key browses fine but mis-targets every later mutation. Assert it in the query tests.
- **Highlights row multiplicity:** the UserMark×BlockRange cross join yields one row per BlockRange, so a single visual highlight spanning blocks appears as multiple selectable items — intended (matches Python + the BlockRangeId delete key). Do not `GROUP BY` it away.
- **Favorites identity subtlety:** a Favorite is a `TagMap` row with NULL NoteId; the selection key is `TagMapId`, and export/delete narrows to the literal `Type=0`/`Name='Favorite'` tag (FUNCTIONALITY-SPEC §3.4) — relevant to Phase 8, noted so the browse query's `WHERE tm.NoteId IS NULL` isn't "corrected" away.
</code_context>

<specifics>
## Specific Ideas
- Keep the FFI/native lib entirely out of this phase — Phase 6 is pure read-side SQL + frontend. No jwlCore, no merge.
- One shared `db/labels.rs`, one `db/browse.rs` (five getters + Notes re-exported), one unified `BrowseRow`, one `list_category` command, one generalized `CategoryList`, one `CategorySwitcher`. Resist per-category proliferation everywhere.
- The color swatch for Notes/Highlights can start as a text label ("Yellow"/"Blue") — `process_color` already yields the English name; a real color chip is a cheap polish, discretion.

## Constraints in force (project)
- Parameterize all SQL; the five category queries are static text (nothing to interpolate).
- Virtualize every category list (Linux WebKitGTK).
- Typed errors, never silent-swallow or crash (the Python's bare `except:`/`sys.exit()` are defects not ported).
- No publication body TEXT — only names/refs from resources.db (metadata) are surfaced.
- Synthetic fixtures ONLY — never a real `.jwlibrary` in tests.
- MIT — jwlCore binary only; no jwlFusion source ingested (irrelevant here, no native code touched).

</specifics>

<deferred>
## Deferred Ideas
- All editing (color/tags/reorder/favorite-add/clean/mask/raw-edit) + per-category delete backends → Phase 7.
- Import/export of any category → Phase 8.
- Duplicate-notes CTE filter, grouping/tree hierarchy, title-view modes, sort → Phase 7+/polish.
- Data-viewer / raw record inspector → Phase 7.
- Localized labels, persisted last-category, theme → Phase 11.
</deferred>

---

*Phase: 6-Full Data Browsing*
*Context gathered: 2026-07-23*
</content>
</invoke>
