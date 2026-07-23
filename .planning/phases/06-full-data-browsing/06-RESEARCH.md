# Phase 6: Full Data Browsing - Research

**Researched:** 2026-07-23
**Domain:** Read-side SQLite category queries + generalized virtualized React list (Tauri v2, Rust core + web frontend)
**Confidence:** HIGH (codebase-grounded; every claim verified against the Python source and the shipped Phase 1/2 Rust+TS code)

## Summary

Phase 6 generalizes the single proven Notes browse slice (Phase 1) to all six categories — Annotations, Bookmarks, Favorites, Highlights, Notes, Playlists. The technical work is almost entirely PORTING and GENERALIZING code that already exists and is tested: five more SQL statements ported verbatim from the Python `get_*` getters, the shared label-synthesis helpers (`process_code`/`process_detail`/`process_color`/`resolve_publication`) lifted out of `db/notes.rs` into a shared module, one unified over-IPC row struct (the Python's `merge_df` already collapses all six categories into a single schema), one generic `list_category` Tauri command, and one generalized virtualized list + a category switcher on the frontend. No native/FFI code, no new external dependency, no new mutation.

The valid-operations criterion (criterion 3) is satisfied by an operation-capability descriptor derived from the Python `disable_options` table, gated on `(category, selection.size)`. Only one operation is wired to a real backend mutation — Notes delete, already shipped in Phase 2. Per-category delete/edit backends are Phase 7; import/export is Phase 8. Phase 6 surfaces capability, it does not implement it.

**Primary recommendation:** Extract `db/notes.rs`'s label helpers into `db/labels.rs`; add `db/browse.rs` with the five ported getters returning a unified `BrowseRow`; add one `list_category(category)` command; generalize `NotesList.tsx` into `CategoryList` + add `CategorySwitcher`; drive the contextual operation set off a capability map keyed by the existing `Category` enum. Verify identity-PK correctness per category in query tests over an extended synthetic fixture.

## User Constraints (from CONTEXT.md)

### Locked Decisions
- D6-01: One shared label-synthesis module (`db/labels.rs`); extract `process_*`/regexes/consts from `notes.rs`, reuse across all getters — no per-category copies.
- D6-02: One unified `BrowseRow` (the `merge_df` 12-field schema: `Id, Language, Symbol, Color, Tags, Modified, Year, Detail1, Detail2, Short, Full, Type` + `independent`); each getter fills its subset, defaults the rest.
- D6-03: Port the five getter SQLs verbatim as static, parameter-free `const &str`. Identity keys: Annotations→LocationId, Bookmarks→BookmarkId, Favorites→TagMapId, Highlights→BlockRangeId, Playlists→PlaylistItemId (Notes→NoteId already done).
- D6-04: resources.db lookups required for Annotations/Bookmarks/Favorites/Highlights; NOT for Playlists. Pass `ResourceCatalog` uniformly. Only names/refs (metadata), never publication body text.
- D6-05: Per-category selection = `Set<PK>` keyed by the identity column; reset on category switch. Selection PK MUST equal the future Phase 7 delete/edit dispatch key.
- D6-06: Category switcher over the six `Category` enum variants; never key control flow off translated strings.
- D6-07: Generalize `NotesList` → one virtualized `CategoryList`; TanStack Virtual MANDATORY for every category (Linux WebKitGTK); fixed-height no-wrap rows.
- D6-08: Operation-capability descriptor drives the contextual set = `f(category, selection.size)`; only Notes-delete is wired; other operations surfaced-but-deferred (Phase 7/8).
- D6-09: One generic `list_category(category)` command, not six; `open_archive` keeps returning the initial Notes view.
- D6-10: Per-category query tests over an extended synthetic fixture; assert identity PK + label synthesis; frontend vitest for switch/select/virtualize/contextual-ops. Synthetic fixtures ONLY.

### Claude's Discretion
Module layout (`db/labels.rs` + `db/browse.rs` vs a file per category; recommend the former), rename `NotesRow`→`BrowseRow` vs superseding alias, frontend component names (`CategoryList`, `CategorySwitcher`), deferred-op rendering (disabled-with-tooltip vs hidden), color swatch as chip vs text, switcher form (segmented buttons vs `<select>`), per-category column layout in the shared row renderer.

### Deferred Ideas (OUT OF SCOPE)
All editing + per-category delete backends → Phase 7. Import/export → Phase 8. Duplicate-notes CTE, grouping/tree, title-view, sort → Phase 7+/polish. Data-viewer → Phase 7. Localized labels, persisted last-category, theme → Phase 11.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DATA-02 | Browse Highlights | `get_highlights` SQL (`JWLManager.py:682`), identity BlockRangeId, one row per BlockRange; carries Color via `process_color`. |
| DATA-03 | Browse Bookmarks | `get_bookmarks` SQL (`:656`), identity BookmarkId; resources.db label synthesis. |
| DATA-04 | Browse Annotations | `get_annotations` SQL (`:643`), identity LocationId; resources.db label synthesis. |
| DATA-05 | Browse Favorites | `get_favorites` SQL (`:669`, `WHERE tm.NoteId IS NULL ORDER BY tm.Position`), identity TagMapId. |
| DATA-06 | Browse Playlists | `get_playlists` SQL (`:770`, `Tag.Type=2`), identity PlaylistItemId; NO resources.db lookup. |
| DATA-07 | Select one/many + valid-operations set updates with selection | Generalized `Set<PK>` selection (`NotesList.tsx:65`) + capability descriptor from `disable_options` (`:510-547`) gated on `(category, selection.size)`; Notes-delete the one live op. |

*(DATA-01, DATA-08 delivered in Phase 1: Notes browse + the `Category` enum.)*
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Per-category SQL query | Rust core (`db/browse.rs`) | resources.db (label lookup) | All archive DB access is Rust-side over the extracted `userData.db`; the frontend never touches SQLite. |
| Label synthesis (code/detail/color/publication) | Rust core (`db/labels.rs`) | resources.db (`ResourceCatalog`) | Deterministic pure functions ported from Python `process_*`; already Rust-side for Notes. |
| Category dispatch | Rust command (`list_category`) | `Category` enum (ts-rs) | Single command keyed by the shared enum; mirrors Python `regroup`/`switchboard`. |
| Row rendering + virtualization | Frontend (`CategoryList`) | `@tanstack/react-virtual` | DOM windowing is a client concern; Linux WebKitGTK perf makes it mandatory. |
| Selection state | Frontend (`Set<PK>`) | — | Ephemeral UI state, keyed by identity PK, reset on category switch. |
| Contextual operation set | Frontend (capability map) | Rust (Notes-delete command only) | Capability presentation is UI; the one live mutation crosses into Rust. |

## Standard Stack

### Core (all already in the project — no new dependencies)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `rusqlite` | as-pinned (Phase 1) | Read the extracted `userData.db`; parameterized queries | Already the DB layer (`db/notes.rs`, `db/resources.rs`). [VERIFIED: codebase] |
| `ts-rs` | as-pinned | Export `BrowseRow`/`Category` to TypeScript bindings | Single-source Rust→TS types already used for `NotesRow`/`Category`. [VERIFIED: codebase] |
| `serde` | as-pinned | Serialize rows over Tauri IPC | Established. [VERIFIED: codebase] |
| `regex` | as-pinned | `CODE_YR`/`CODE_JWB` in label synthesis | Already used in `notes.rs`. [VERIFIED: codebase] |
| `@tanstack/react-virtual` | as-pinned (`useVirtualizer`) | Window the category list | Already the Notes list virtualizer (`NotesList.tsx:2,69`). [VERIFIED: codebase] |
| `@tauri-apps/api` (`invoke`) | as-pinned | Call `list_category` from the frontend | Established IPC path. [VERIFIED: codebase] |

**Installation:** none — Phase 6 adds no package. Confirm no new dependency creeps into `app/package.json` or `app/src-tauri/Cargo.toml`.

## Package Legitimacy Audit

Not applicable — Phase 6 installs no external package. All libraries are already vendored and used by Phases 1-5. [VERIFIED: codebase — no `npm install`/`cargo add` required]

## Architecture Patterns

### System Architecture Diagram

```
                 ┌─────────────────────────────────────────────┐
   user picks    │  CategorySwitcher  (6 Category enum variants)│
   a category →  └───────────────┬─────────────────────────────┘
                                  │ invoke("list_category",{category})
                                  ▼
              ┌───────────────────────────────────────────────┐
   Rust core  │ list_category(cat)                             │
              │   lock SessionState → open userData.db conn    │
              │   load/borrow ResourceCatalog (resources.db)   │
              │   match cat → db::browse::query_<cat>()        │
              └───────────────┬───────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────────────────┐
          ▼                   ▼                                ▼
  static per-cat SQL   db::labels::process_*     resources.db lookups
  (Annotations/Book-   (code/detail/color)       (BibleBooks, Publications
   marks/Favorites/    + resolve_publication      /Extras, Languages)
   Highlights/Play-    → merge_df fallbacks       [skipped for Playlists]
   lists; Notes done)          │
          └───────────────────┬┘
                              ▼
                    Vec<BrowseRow>  (unified 12-field schema)
                              │ IPC (serde/ts-rs)
                              ▼
              ┌───────────────────────────────────────────────┐
   Frontend   │ CategoryList (TanStack Virtual, fixed 44px)   │
              │   render viewport rows: [checkbox] label       │
              │   [color?] [tags?] [modified?]                 │
              │   selection: Set<PK>  (reset on switch)        │
              └───────────────┬───────────────────────────────┘
                              ▼
              ┌───────────────────────────────────────────────┐
   contextual │ OperationSet = f(category, selection.size)     │
   op bar     │   capability map (disable_options)             │
              │   → Delete(Notes) LIVE  |  others: deferred     │
              └───────────────────────────────────────────────┘
```

### Recommended Project Structure
```
app/src-tauri/src/db/
├── labels.rs      # NEW: process_code/detail/color, resolve_publication, regexes, consts (from notes.rs)
├── browse.rs      # NEW: BrowseRow + query_annotations/bookmarks/favorites/highlights/playlists
├── notes.rs       # KEEP: query_notes reworked to return BrowseRow, importing labels.rs
├── resources.rs   # unchanged (ResourceCatalog)
└── ...
app/src/
├── components/
│   ├── CategoryList.tsx      # NEW: NotesList generalized over BrowseRow + category
│   ├── CategorySwitcher.tsx  # NEW: six-variant selector
│   └── NotesList.tsx         # fold into CategoryList (or keep as a thin Notes wrapper)
├── lib/operations.ts         # NEW: capability map + f(category, selection)
└── App.tsx                   # holds current category + rows + per-category selection
```

### Pattern 1: Unified getter → `BrowseRow`
**What:** Each category getter runs its static SQL, maps raw columns through `db::labels`, and returns `Vec<BrowseRow>`. Absent columns are `Option::None`/empty (the `merge_df` `fill_null` analog).
**When to use:** Every category. Notes reuses its existing located+independent logic, just returning `BrowseRow`.
**Example (Bookmarks, ported verbatim from `JWLManager.py:654-665`):**
```rust
// Source: JWLManager.py:656 (get_bookmarks SQL)
const BOOKMARKS_SQL: &str = "SELECT LocationId, l.KeySymbol, l.MepsLanguage, \
    l.IssueTagNumber, BookmarkId, l.BookNumber, l.ChapterNumber, l.Title \
    FROM Bookmark b JOIN Location l USING (LocationId)";

pub fn query_bookmarks(conn: &Connection, catalog: &ResourceCatalog)
    -> Result<Vec<BrowseRow>, ArchiveError>
{
    let mut stmt = conn.prepare(BOOKMARKS_SQL)?;
    let rows = stmt.query_map([], |r| Ok(RawBookmark {
        location_id: r.get(0)?, symbol: r.get(2 - 1)?, /* KeySymbol */
        meps_language: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
        issue: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
        bookmark_id: r.get(4)?, book: r.get(5)?, chapter: r.get(6)?,
    }))?;
    let mut out = Vec::new();
    for raw in rows {
        let raw = raw?;
        let language = catalog.lang_name(raw.meps_language)
            .map(str::to_string).unwrap_or_else(|| format!("#{}", raw.meps_language));
        let (code, year) = labels::process_code(raw.symbol.as_deref(), raw.issue);
        let symbol = if code.is_empty() { "* OTHER *".into() } else { code };
        let (detail1, year, detail2) =
            labels::process_detail(&symbol, raw.book, raw.chapter, raw.issue, year, catalog);
        let (short, full, type_group, year) = labels::resolve_publication(catalog, &symbol, year);
        out.push(BrowseRow {
            id: raw.bookmark_id,        // IDENTITY = BookmarkId (§3.3) — selection/delete key
            language, symbol,
            color: None, tags: None, modified: None,   // bookmarks have none
            year: year.or(Some("* NO YEAR *".into())),
            detail1, detail2, short, full, type_group,
            independent: false,
        });
    }
    Ok(out)
}
```

### Pattern 2: Category-keyed dispatch command
```rust
// Mirrors lib.rs:36-75 (open_archive/list_notes) — session lock + resources resolve + ErrorDto map.
#[tauri::command]
fn list_category(category: Category, app: tauri::AppHandle, state: tauri::State<SessionState>)
    -> Result<Vec<BrowseRow>, ErrorDto>
{
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|e| e.to_dto("list_category", None))?;
    let guard = state.lock().map_err(|_| ArchiveError::StatePoisoned.to_dto("list_category", None))?;
    let session = guard.as_ref()
        .ok_or_else(|| ArchiveError::MissingUserDataBackup.to_dto("list_category", None))?;
    let conn = Connection::open(session.db_path.join("userData.db"))
        .map_err(|e| ArchiveError::from(e).to_dto("list_category", None))?;   // pattern per open path
    let catalog = ResourceCatalog::load(&resources_db_path, "en")
        .map_err(|e| e.to_dto("list_category", None))?;
    let rows = match category {
        Category::Notes       => db::notes::query_notes(&conn, &catalog),
        Category::Bookmarks   => db::browse::query_bookmarks(&conn, &catalog),
        Category::Favorites   => db::browse::query_favorites(&conn, &catalog),
        Category::Highlights  => db::browse::query_highlights(&conn, &catalog),
        Category::Annotations => db::browse::query_annotations(&conn, &catalog),
        Category::Playlists   => db::browse::query_playlists(&conn, &catalog),
    }.map_err(|e| e.to_dto("list_category", None))?;
    Ok(rows)
}
```
*(Exact session→connection access should follow whatever `archive::reload_notes`/`open_and_validate` already do — reuse that helper rather than re-opening by hand.)*

### Pattern 3: Contextual operation set (frontend)
```ts
// Capability from JWLManager.py:510-547 (disable_options) + :497-505 (tree_selection).
type Op = "delete" | "export" | "view" | "color" | "tag" | "add" | "import";
const CAPABILITY: Record<Category, Op[]> = {
  Notes:       ["delete","export","import","view","color","tag"],
  Highlights:  ["delete","export","import","color"],
  Bookmarks:   ["delete","export","import"],
  Annotations: ["delete","export","import","view"],
  Favorites:   ["delete","export","import","add"],
  Playlists:   ["delete","export","import","add"],
};
const NEEDS_SELECTION: Set<Op> = new Set(["delete","export","view","color","tag"]);
// Phase 6: only Notes-delete is LIVE; everything else renders deferred.
const LIVE: Set<`${Category}:${Op}`> = new Set(["Notes:delete"]);

function operationSet(cat: Category, selectionSize: number) {
  return CAPABILITY[cat].map(op => ({
    op,
    enabled: (!NEEDS_SELECTION.has(op) || selectionSize > 0) && LIVE.has(`${cat}:${op}`),
    deferred: !LIVE.has(`${cat}:${op}`),
  }));
}
```

### Anti-Patterns to Avoid
- **Six per-category row structs / six commands / two list components.** The Python collapses to one frame (`merge_df`) and one `regroup`; mirror that with one `BrowseRow`, one `list_category`, one `CategoryList`.
- **Keying control flow off translated display strings** (`if category === "Notes"`). Use the `Category` enum — the whole reason it exists (`category.rs` docstring).
- **`GROUP BY`-ing the Highlights query to "dedupe" multi-block rows.** One row per BlockRange is intended (identity = BlockRangeId).
- **Dropping virtualization for "small" categories.** One code path, always virtualized.
- **String-interpolating any SQL.** The five queries are static; the only bound values are resources.db `ui_lang_id` (already parameterized).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Windowed rendering of large lists | A manual scroll/slice | `@tanstack/react-virtual` (already used) | Linux WebKitGTK perf; the fixed-height contract is already solved in `NotesList`. |
| Publication/verse/language labels | A hand-maintained map | `ResourceCatalog` (`db/resources.rs`) | Bundled `res/resources.db` is the source of truth; already loaded + cached. |
| Year/code/detail derivation | Re-deriving per category | `db::labels` (extracted `process_*`) | Ported + unit-tested; six copies would drift. |
| Rust→TS row types | Hand-written `.ts` interfaces | `ts-rs` `#[ts(export)]` (as `NotesRow`/`Category`) | Single source; drift-proof. |
| Empty-selection guard | Runtime `if len==0` scattered | The `NonEmptyNoteIds` newtype pattern | Empty selection unrepresentable at IPC deserialization (Phase 2, `delete.rs:54-85`). |

**Key insight:** Phase 6 is a generalization exercise. The correct instinct is "reuse the Notes machinery for five more SQL statements," not "design a browsing system."

## Common Pitfalls

### Pitfall 1: Wrong identity/selection column per category
**What goes wrong:** Using `LocationId` for Bookmarks, or `UserMarkId` for Highlights, browses fine but silently mis-targets every Phase 7 delete/edit.
**Why it happens:** Several getters SELECT `LocationId` first (for the join/labels) but the identity is a LATER column (`BookmarkId`, `BlockRangeId`, `TagMapId`).
**How to avoid:** Take identity from the exact column FUNCTIONALITY-SPEC §3.3 lists: Annotations→LocationId, Bookmarks→BookmarkId, Favorites→TagMapId, Highlights→BlockRangeId, Notes→NoteId, Playlists→PlaylistItemId. Assert it in each query test.
**Warning signs:** A query test where `row.id` equals a Location/UserMark id instead of the category's own PK.

### Pitfall 2: Highlights row multiplicity treated as a bug
**What goes wrong:** A dev "fixes" the `UserMark ... , BlockRange ... USING (UserMarkId)` cross join to one row per UserMark, losing per-range selection.
**Why it happens:** One visual highlight can span multiple BlockRanges → multiple rows looks like duplication.
**How to avoid:** Keep one row per BlockRange (identity = BlockRangeId). It matches the Python and the delete key. Test with a UserMark carrying two BlockRanges → expect two rows.

### Pitfall 3: Favorites `WHERE tm.NoteId IS NULL` dropped
**What goes wrong:** Omitting the predicate lists note-tag mappings as "favorites."
**Why it happens:** A Favorite is a TagMap row with NULL NoteId — non-obvious.
**How to avoid:** Port the `WHERE tm.NoteId IS NULL ORDER BY tm.Position` exactly (`:669`).

### Pitfall 4: Running resources.db synthesis for Playlists
**What goes wrong:** Passing a Location-derived symbol through `process_detail` for playlist rows produces wrong labels.
**Why it happens:** Playlists have no Location; the Python hardcodes `Language=None`, `Symbol='* OTHER *'`, label from `Name`/`Label`.
**How to avoid:** Playlist getter does NOT call `process_detail`/`process_code`; it sets the fixed fields and puts `Label` in `Detail1`, `Name` in `Tags` (`:771`). The catalog is still passed (uniform signature) but unused.

### Pitfall 5: Selection not reset on category switch
**What goes wrong:** A `BookmarkId` selected, then switch to Highlights — the stale id now "selects" an unrelated BlockRange with the same integer.
**How to avoid:** Clear the `Set` on every category switch (integers collide across categories). Frontend vitest: switch clears selection.

### Pitfall 6: Non-uniform row height / wrapping breaks the virtualizer
**What goes wrong:** A category row wraps to two lines → the fixed-size virtualizer mismeasures, rows overlap.
**How to avoid:** Keep every category row at the fixed 44px, single-line, no-wrap (`NotesList.tsx:15-23` `NO_WRAP_STYLE`). Absent columns render nothing, never a taller row.

## Code Examples

### Category identity keys (the load-bearing table)
```text
// Source: JWLManager.py:643,656,669,682,751,770 + delete_items:3658-3671; FUNCTIONALITY-SPEC §3.3
Annotations  → Location.LocationId
Bookmarks    → Bookmark.BookmarkId
Favorites    → TagMap.TagMapId        (TagMap row WHERE NoteId IS NULL)
Highlights   → BlockRange.BlockRangeId (one row per BlockRange)
Notes        → Note.NoteId            (already implemented)
Playlists    → PlaylistItem.PlaylistItemId
```

### The five queries, verbatim (identity column bolded in comment)
```sql
-- Annotations (JWLManager.py:643)  identity: LocationId
SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, TextTag,
       l.BookNumber, l.ChapterNumber, l.Title
FROM InputField JOIN Location l USING (LocationId);

-- Bookmarks (JWLManager.py:656)  identity: BookmarkId
SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, BookmarkId,
       l.BookNumber, l.ChapterNumber, l.Title
FROM Bookmark b JOIN Location l USING (LocationId);

-- Favorites (JWLManager.py:669)  identity: TagMapId
SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, TagMapId
FROM TagMap tm JOIN Location l USING (LocationId)
WHERE tm.NoteId IS NULL ORDER BY tm.Position;

-- Highlights (JWLManager.py:682)  identity: BlockRangeId (one row per BlockRange)
SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, b.BlockRangeId,
       u.UserMarkId, u.ColorIndex, l.BookNumber, l.ChapterNumber
FROM UserMark u JOIN Location l USING (LocationId), BlockRange b USING (UserMarkId);

-- Playlists (JWLManager.py:770)  identity: PlaylistItemId (no resources.db lookup)
SELECT PlaylistItemId, Name, Position, Label
FROM PlaylistItem JOIN TagMap USING (PlaylistItemId) JOIN Tag t USING (TagId)
WHERE t.Type = 2 ORDER BY Name, Position;
```

### Per-category surfaced columns (post-`merge_df`)
```text
// Which BrowseRow fields each category populates (others None/empty):
Category     Language Symbol Color Tags Modified Year Detail1 Detail2 Short Full Type
Notes           ✓      ✓      ✓     ✓      ✓       ✓     ✓       ✓      ✓    ✓    ✓
Highlights      ✓      ✓      ✓     -      -       ✓     ✓       ✓      ✓    ✓    ✓
Bookmarks       ✓      ✓      -     -      -       ✓     ✓       ✓      ✓    ✓    ✓
Annotations     ✓      ✓      -     -      -       ✓     ✓       ✓      ✓    ✓    ✓
Favorites       ✓      ✓      -     -      -       ✓     ✓       ✓      ✓    ✓    ✓
Playlists       -    *OTHER*  -   Name     -       -   Label     -      -    -    -
```

## State of the Art

| Old Approach (Python) | Current Approach (Tauri port) | When Changed | Impact |
|-----------------------|-------------------------------|--------------|--------|
| `if category == _('Notes')` translated-string dispatch | `Category` enum (ts-rs single source) | Phase 1 | Kills the i18n control-flow bug class; the switcher + `list_category` use the enum. |
| One `Window` god-object holding `current_data` (polars) | Rust getters return typed `BrowseRow` over IPC; React holds view state | Phase 1 | Testable per-category queries; no shared mutable god state. |
| `str(list).replace('[','(')` inline `IN ()` | `NonEmptyNoteIds` newtype (empty unrepresentable) | Phase 2 | No `IN ()` syntax hazard; carry to Phase 7 per-category deletes. |
| polars `merge_df` join for labels | `ResourceCatalog` HashMap lookups | Phase 1 | Same unified schema, no polars dependency. |

**Deprecated/outdated:** none introduced here. The `self.dupes` CTE (`:707-750`), grouping/tree, and title-view are intentionally NOT ported in Phase 6 (flat list, like Phase 1 Notes).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The session exposes an open path to `userData.db` reusable by `list_category` (via `archive::reload_notes`/`open_and_validate` helper) analogous to how `list_notes` re-queries. | Pattern 2 | LOW — if the helper signature differs, adjust the connection acquisition; the query logic is unaffected. |
| A2 | The main `generate_v16_fixture` needs extending to seed Bookmark/Favorite/Highlight/Annotation/Playlist rows (collision fixtures already seed most of these, `common/mod.rs:540-581`). | D6-10 | LOW — worst case add a few INSERTs; building blocks exist. |
| A3 | Deferred (non-Notes) operations render as disabled affordances rather than being hidden, to make criterion 3 visibly demonstrable per category. | D6-08 | LOW — pure UX choice; either satisfies the criterion. |

**If this table is empty:** it is not — three LOW-risk implementation assumptions, all resolvable during planning by reading the exact helper signatures.

## Open Questions

1. **Should `NotesList.tsx` be deleted or kept as a thin wrapper?**
   - What we know: `CategoryList` supersedes it; `App.tsx` currently renders `NotesList` directly.
   - What's unclear: whether Phase 2's `DeletePreviewDialog` wiring is easier to keep in a Notes-specific branch of `CategoryList` or a wrapper.
   - Recommendation: fold into `CategoryList` with the delete flow gated on `category === Notes`; drop `NotesList` (its tests migrate). Discretion.

2. **Color swatch: chip or text?**
   - What we know: `process_color` yields the English color name.
   - Recommendation: text label now (cheap), real chip is polish. Discretion (D6-07/specifics).

## Environment Availability

Skipped — Phase 6 is code/config only (Rust query modules + React components + tests). No new external tool, service, runtime, or CLI beyond the already-present Rust/Node toolchain used by Phases 1-5. [VERIFIED: no external dependency]

## Validation Architecture

*(nyquist_validation assumed enabled — no `.planning/config.json` override found stating otherwise.)*

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `cargo test` (integration tests in `app/src-tauri/tests/`) + Vitest (frontend, `app/src/**/*.test.tsx`) |
| Config file | `app/src-tauri/Cargo.toml` (test targets); `app/` Vitest config (per Phase 1 setup, `setupTests.ts`) |
| Quick run command | `cargo test -p jwlmanager_lib browse` ; `npm --prefix app test -- CategoryList` |
| Full suite command | `cargo test` (in `app/src-tauri`) ; `npm --prefix app test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DATA-02 | Highlights query returns 1 row/BlockRange, identity BlockRangeId, Color set | unit/integration | `cargo test highlights_query` | ❌ Wave 0 |
| DATA-03 | Bookmarks query, identity BookmarkId, labels synthesized | integration | `cargo test bookmarks_query` | ❌ Wave 0 |
| DATA-04 | Annotations query, identity LocationId | integration | `cargo test annotations_query` | ❌ Wave 0 |
| DATA-05 | Favorites query, `NoteId IS NULL`, identity TagMapId, Position order | integration | `cargo test favorites_query` | ❌ Wave 0 |
| DATA-06 | Playlists query, `Tag.Type=2`, identity PlaylistItemId, no resources lookup | integration | `cargo test playlists_query` | ❌ Wave 0 |
| DATA-07 | Category switch swaps rows + resets selection; multi-select; op set = f(cat,sel) | frontend | `npm --prefix app test -- CategoryList` | ❌ Wave 0 |
| DATA-07 | Virtualization renders only viewport rows for a large category | frontend | `npm --prefix app test -- CategoryList` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p jwlmanager_lib browse` + the touched frontend test.
- **Per wave merge:** full `cargo test` (in `app/src-tauri`) + `npm --prefix app test`.
- **Phase gate:** full suite green before `/gsd-verify-work`; verify identity-PK asserts present for all five categories.

### Wave 0 Gaps
- [ ] `app/src-tauri/tests/browse_query_tests.rs` — per-category query + identity-PK + label-synthesis asserts (covers DATA-02..06).
- [ ] Extend `app/src-tauri/tests/common/mod.rs` `generate_v16_fixture` (or a new `generate_v16_all_categories_fixture`) to seed one row per category.
- [ ] `app/src/components/CategoryList.test.tsx` — switch/select/virtualize/contextual-ops (covers DATA-07); migrate `NotesList.test.tsx` asserts.
- [ ] `app/src/components/CategorySwitcher.test.tsx` — enum-driven switching, selection reset.

## Security Domain

`security_enforcement` not explicitly disabled → included.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | The five category queries are static (no user input in SQL); resources.db `ui_lang_id` is parameterized (`resources.rs`). Category is a typed enum over IPC, not a raw string. |
| V6 Cryptography | no | No crypto in the read path (save-path hashing is Phase 1). |
| V2/V3/V4 Auth/Session/Access | no | Local desktop app, single user, no auth surface. |
| V12 Files/Resources | yes (indirect) | Archive extraction (zip-slip-safe) is Phase 1; Phase 6 only reads the already-extracted, validated `userData.db`. |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| SQL injection via category dispatch | Tampering | `Category` is a Rust enum over IPC; queries are static consts — no interpolation. [VERIFIED: D6-03] |
| Malicious archive DB content rendered as a label | Tampering/DoS | Labels are text-only, no HTML injection into React (React escapes by default); no publication body text surfaced. |
| Untrusted archive triggering panic | DoS | Typed errors, no `unwrap`/`panic` on the archive-data path (established Phase 1/2 posture). |

## Sources

### Primary (HIGH confidence)
- `JWLManager.py:578-639, 641-775` — `process_*`/`merge_df` + all five getter SQLs (verbatim port source). [VERIFIED: codebase read]
- `.planning/research/FUNCTIONALITY-SPEC.md` §1.2, §3.3, §3.4 — capability table + identity keys + query definitions. [VERIFIED]
- `app/src-tauri/src/db/notes.rs`, `db/resources.rs`, `category.rs`, `db/delete.rs`, `lib.rs:36-75` — the Phase 1/2 template. [VERIFIED: codebase read]
- `app/src/components/NotesList.tsx`, `CommandBar.tsx`, `App.tsx`, `styles.css` — frontend template + design tokens. [VERIFIED]
- `app/src-tauri/tests/notes_query_tests.rs`, `fixtures.rs`, `common/mod.rs:540-581` — test scaffolding. [VERIFIED]

### Secondary (MEDIUM confidence)
- none required — every claim resolved against the codebase.

### Tertiary (LOW confidence)
- none.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; all libraries already in use.
- Architecture: HIGH — direct generalization of shipped Phase 1 code; Python source is the authoritative spec.
- Pitfalls: HIGH — derived from the exact identity-key/multiplicity/predicate warts documented in FUNCTIONALITY-SPEC.

**Research date:** 2026-07-23
**Valid until:** stable (internal codebase + frozen Python spec; ~90 days) — re-verify only if `db/notes.rs` or the `Category` enum changes before planning.
</content>
