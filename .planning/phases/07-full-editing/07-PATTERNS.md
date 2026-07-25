# Phase 7: Full Editing - Pattern Map

**Mapped:** 2026-07-24
**Files analyzed:** 33 (14 Rust backend, 9 frontend, 10 test)
**Analogs found:** 30 / 33 (3 have no analog — `merge_block_ranges`, the mask RNG, the raw-record editor UI)

Every new mutation backend in this phase is an instance of ONE already-shipped pattern:
`app/src-tauri/src/db/delete.rs`. Read that file once; it is the template for eight new
modules. The frontend equivalent is `app/src/components/DeletePreviewDialog.tsx` +
`CategoryList.tsx`. This document pins, per new file, exactly which analog to copy and
which lines carry the load.

---

## Schema + dependency facts verified this pass (corrections to 07-RESEARCH.md)

The planner MUST fold these in — three RESEARCH assumptions were wrong or incomplete.

| # | RESEARCH claim | Verified reality | Impact |
|---|----------------|------------------|--------|
| **A2** | `TagMap` enforces `UNIQUE(TagId, Position)` and "effectively" `UNIQUE(TagId, NoteId)` | **CONFIRMED and EXTENDED.** `res/blank` `userData.db` declares THREE named constraints: `CONSTRAINT TagId_Position UNIQUE (TagId, Position)`, `CONSTRAINT TagId_NoteId UNIQUE (TagId, NoteId)`, `CONSTRAINT TagId_LocationId UNIQUE (TagId, LocationId)` — plus a `CHECK` enforcing that EXACTLY ONE of `PlaylistItemId`/`NoteId`/`LocationId` is non-NULL | D7-05 two-pass is **mandatory** (confirmed). D7-06's favorites dup-check is not merely a Python nicety — `UNIQUE(TagId, LocationId)` means a duplicate favorite is a hard DB error; the app must reject it with a typed error BEFORE the INSERT. D7-04's `INSERT OR IGNORE` guards `TagId_NoteId`. |
| **A3** | `InputField` has no single-column integer PK; dry-run needs a synthetic string key (`LocationId \|\| '\x1f' \|\| TextTag`) | **CONFIRMED but SIMPLER than feared.** `CONSTRAINT LocationId_TextTag PRIMARY KEY (LocationId, TextTag)` is a non-INTEGER PK, so the table is **still a rowid table** (not `WITHOUT ROWID`). `snapshot_pks(tx, "InputField", "rowid")` works verbatim with zero new code | **No synthetic-key helper is needed for `InputField`.** Add `("InputField", "rowid")` to the tracked-table set. Same trick covers any future composite table. Caveat: `rowid` is not stable across a `VACUUM`, so it is valid ONLY inside a single dry-run transaction — which is exactly the usage. Document that. |
| **A4** | resources.db has a bundled `favorites` **table** | **CORRECTED — it is a VIEW.** `res/resources.db` has tables `BibleBooks, Extras, Languages, Publications, Types` and one VIEW: `CREATE VIEW Favorites AS SELECT p.Language, p.Symbol, p.ShortTitle AS Short, l.Name AS Lang FROM Publications p LEFT JOIN Languages l ON p.Language = l.Language WHERE p.Favorite = 1 UNION ALL SELECT e.Language, e.Symbol, e.ShortTitle AS Short, l.Name AS Lang FROM Extras e LEFT JOIN Languages l ON e.Language = l.Language WHERE e.Favorite = 1`. Columns `(Language, Symbol, Short, Lang)`. Matches `JWLManager.py:4052` `pl.read_database("SELECT * FROM Favorites;", con)` | `ResourceCatalog` (`db/resources.rs:28-35`) loads Languages/BibleBooks/Publications only — it does **not** load `Favorites`. A new loader method is required (pattern below). |
| **A5 / stack** | "`uuid` (in repo)" and "`regex` (in repo)" | `regex = "1"` IS a declared dependency (`Cargo.toml:38`). **`uuid` is NOT** — it appears in `Cargo.lock:4073` only transitively via tauri. **`rand` is absent from `Cargo.lock` entirely** (only `getrandom`). `fancy-regex` absent | Both D7-02 (`UserMarkGuid` synthesis) and D7-08 (mask word choice) need a capability the manifest does not declare. Two paths: declare the dep (a `checkpoint:human-verify` per the legitimacy protocol) OR follow the repo's own no-new-dep precedent — `app/src-tauri/src/time.rs` (see "Shared Pattern 6"). |

Additional schema constraints that bind new SQL (all from `res/blank` `userData.db`):

```sql
-- UserMark: the synthesized GUID must be UNIQUE, and LocationId is NOT NULL
CREATE TABLE UserMark ( UserMarkId INTEGER NOT NULL PRIMARY KEY, ColorIndex INTEGER NOT NULL,
  LocationId INTEGER NOT NULL, StyleIndex INTEGER NOT NULL, UserMarkGuid TEXT NOT NULL UNIQUE,
  Version INTEGER NOT NULL, FOREIGN KEY (UserMarkId->) REFERENCES Location (LocationId) )

-- BlockRange: BlockType is CHECK-constrained; merge_block_ranges' INSERT must satisfy it
CREATE TABLE BlockRange ( BlockRangeId INTEGER NOT NULL PRIMARY KEY, BlockType INTEGER NOT NULL,
  Identifier INTEGER NOT NULL, StartToken INTEGER, EndToken INTEGER, UserMarkId INTEGER NOT NULL,
  CHECK (BlockType BETWEEN 1 AND 2), FOREIGN KEY (UserMarkId) REFERENCES UserMark (UserMarkId) )

-- Tag: UNIQUE(Type,Name) + non-empty name + Type ∈ {0,1,2} are DB-enforced
CREATE TABLE Tag ( TagId INTEGER NOT NULL PRIMARY KEY, Type INTEGER NOT NULL, Name TEXT NOT NULL,
  UNIQUE (Type, Name), CHECK (length(Name) > 0), CHECK (Type IN (0, 1, 2)) )

-- Location: the favorites add_location INSERT..WHERE NOT EXISTS leans on this UNIQUE
CREATE TABLE Location ( ... UNIQUE (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type), ... )
```

`Tag`'s `CHECK (length(Name) > 0)` means an empty tag name is a `SQLITE_CONSTRAINT` — reject it
with a typed `ArchiveError` at the command boundary, never let it surface as a raw sqlite error.

---

## File Classification

### Rust backend

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `src/db/edit.rs` (new) | utility / shared | transform | `src/db/delete.rs:48-194` | exact |
| `src/db/color.rs` (new) | service | CRUD | `src/db/delete.rs` (envelope) + `src/db/labels.rs:73-79` (color) | exact |
| `src/db/highlights.rs` (new) | service | transform (geometric) | `src/archive/downgrade.rs` `compute_merge_groups`/dedup (only in-repo "group-then-delete-on-predicate") | partial |
| `src/db/tags.rs` (new) | service | CRUD | `src/db/delete.rs` + `src/db/trim.rs:171-205` (redensify staging) | role-match |
| `src/db/reorder.rs` (new) | service | batch | `src/db/trim.rs:171-205` `redensify_tag_positions` | exact |
| `src/db/favorites.rs` (new) | service | CRUD | `src/db/delete.rs` (unmark = delete) + `src/db/resources.rs:40+` (edition source) | exact |
| `src/db/scrub.rs` (new) | service | batch / transform | `src/db/trim.rs:155-166` `run_labeled_sweep` + `src/db/labels.rs:20-40` (regex statics) | role-match |
| `src/db/record_edit.rs` (new) | service | CRUD | `src/db/delete.rs` + `src/time.rs:29` (`now_iso8601_utc` for `LastModified`) | exact |
| `src/db/delete.rs` (modified) | service | CRUD | itself — five new per-category delete fns (D7-10) | exact |
| `src/db/mod.rs` (modified) | config | — | itself (8-line `pub mod` list) | exact |
| `src/db/resources.rs` (modified) | service | request-response | `ResourceCatalog::load` (`:40-113`) — add a `Favorites` VIEW loader | exact |
| `src/error.rs` (modified) | model | — | `ArchiveError::DeleteFailed` + `to_dto` arm (`:40-41`, `:127`) | exact |
| `src/lib.rs` (modified) | controller | request-response | `delete_notes_dry_run`/`delete_notes_apply` (`:187-272`), handler list (`:386-400`) | exact |
| `Cargo.toml` (modified, conditional) | config | — | `Cargo.toml:24-38` (every dep carries a why-comment) | exact |

### Frontend

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `src/components/EditPreviewDialog.tsx` (new — rename of) | component | request-response | `src/components/DeletePreviewDialog.tsx` (whole file) | exact |
| `src/components/ColorMenu.tsx` (new) | component | request-response | `DeletePreviewDialog.tsx` (busy-ref + testid shape) | role-match |
| `src/components/TagDialog.tsx` (new) | component | request-response | `DeletePreviewDialog.tsx` + `CategoryList.tsx:117-127` (Set-based toggle) | role-match |
| `src/components/FavoriteAddDialog.tsx` (new) | component | request-response | `DeletePreviewDialog.tsx` | role-match |
| `src/components/RecordEditor.tsx` (new) | component | CRUD | none (no form/editor component exists yet) | **no analog** |
| `src/components/MaskConfirmDialog.tsx` (new) | component | request-response | `DeletePreviewDialog.tsx` (superset: adds typed-confirm gate) | role-match |
| `src/lib/operations.ts` (modified) | config | — | itself — `LIVE` set at `:54` | exact |
| `src/components/CategoryList.tsx` (modified) | component | request-response | itself — `:129-160` dry-run/confirm/cancel triad, `:183-213` op-bar dispatch | exact |
| `src/bindings/*.ts` | generated | — | ts-rs `#[ts(export, export_to = ...)]` — never hand-authored | exact |

### Tests

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `tests/common/mod.rs` (modified) | test fixture | — | `insert_all_categories_rows` (`:520-570`), `generate_composite_*_db` (`:889-978`) | exact |
| `tests/color_tests.rs` (new) | test | — | `tests/delete_tests.rs` (whole file) | exact |
| `tests/highlight_merge_tests.rs` (new) | test | — | `tests/delete_tests.rs` + table-driven cases | role-match |
| `tests/tag_tests.rs` (new) | test | — | `tests/delete_tests.rs` | exact |
| `tests/reorder_tests.rs` (new) | test | — | `tests/trim_tests.rs` (re-densify assertions) | exact |
| `tests/favorites_tests.rs` (new) | test | — | `tests/delete_tests.rs` | exact |
| `tests/scrub_tests.rs` (new) | test | — | `tests/delete_tests.rs` | exact |
| `tests/record_edit_tests.rs` (new) | test | — | `tests/delete_tests.rs` | exact |
| `tests/edit_roundtrip_tests.rs` (new) | test | — | `tests/delete_roundtrip_tests.rs` (whole file) | exact |
| `src/components/*.test.tsx` (new) | test | — | `DeletePreviewDialog.test.tsx`, `CategoryList.test.tsx:1-70` | exact |

---

## Pattern Assignments

### `src/db/edit.rs` (utility, transform) — the shared generalization

**Analog:** `app/src-tauri/src/db/delete.rs`

**Module-doc pattern.** Every `db/*.rs` in this repo opens with a `//!` block that cites the
Python line range it ports, names the requirement IDs, and calls out deliberate deviations.
`delete.rs:1-32` and `trim.rs:1-33` are the two exemplars. Copy the shape — the planner should
treat the module doc as a deliverable, not decoration.

**Error-mapping helper** (`delete.rs:42-46`) — one per module, first item after the imports:

```rust
fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::DeleteFailed {
        reason: format!("{context}: {err}"),
    }
}
```

**Non-empty selection wrapper** (`delete.rs:48-85`) — instantiate once per identity PK
(`BlockRangeId`, `BookmarkId`, `TagMapId`, `LocationId`, `NoteId`). Note the `#[ts(export,
export_to = ...)]` path shape — bindings land in `../../src/bindings/`:

```rust
/// A non-empty selection of `Note.NoteId` values. Constructed only via
/// `TryFrom<Vec<i64>>`/`serde`'s `try_from` container attribute, which
/// rejects an empty `Vec` — an empty selection is impossible by
/// construction, not merely a runtime-checked precondition (SAFE-03, D2-06).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyNoteIds.ts")]
pub struct NonEmptyNoteIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyNoteIds {
    type Error = String;

    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyNoteIds(ids))
        }
    }
}

impl NonEmptyNoteIds {
    pub fn iter(&self) -> impl Iterator<Item = &i64> { self.0.iter() }
    pub fn len(&self) -> usize { self.0.len() }
    /// Always `false` by construction — kept only to satisfy
    /// `clippy::len_without_is_empty`.
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
}
```

> The `is_empty` + its comment are load-bearing: the crate denies `clippy::unwrap_used` /
> `expect_used` (`lib.rs:4`) and CI runs clippy — omitting it fails `len_without_is_empty`.
> A generic `NonEmptyIds<Marker>` is fine, but ts-rs needs a distinct `export_to` path per
> concrete type; five newtypes is the lower-friction shape.

**Snapshot + diff primitives to REUSE, not re-derive** (`delete.rs:112-194`). `snapshot_pks`
already takes `(table, pk_col)` as `&str`, so the `InputField` rowid case needs **no new code**:

```rust
pub(crate) const TRACKED_TABLES: &[(&str, &str)] = &[
    ("Note", "NoteId"),
    ("UserMark", "UserMarkId"),
    ("BlockRange", "BlockRangeId"),
    ("TagMap", "TagMapId"),
    ("Tag", "TagId"),
    ("Location", "LocationId"),
    ("PlaylistItem", "PlaylistItemId"),
    ("PlaylistItemMarker", "PlaylistItemMarkerId"),
];

pub(crate) fn snapshot_pks(tx: &Transaction, table: &str, pk_col: &str)
    -> Result<HashSet<i64>, ArchiveError> {
    let sql = format!("SELECT {pk_col} FROM {table}");
    // ... query_map(|row| row.get::<_, i64>(0)) into a HashSet
}

pub(crate) fn snapshot_tables(tx: &Transaction, tables: &[(&str, &str)])
    -> Result<BTreeMap<String, HashSet<i64>>, ArchiveError> { /* delete.rs:146-155 */ }

pub(crate) fn diff_snapshots(before: &..., after: &...) -> DryRunReport { /* delete.rs:169-194 */ }
```

Phase 7 additions: `("InputField", "rowid")` and `("Bookmark", "BookmarkId")`. Precedent for a
per-op table set instead of the global one is already established — `downgrade.rs:515` calls
`snapshot_tables(&tx, DOWNGRADE_SNAPSHOT_TABLES)` with its own const. Do the same per op group.

**`DryRunReport`** (`delete.rs:94-101`) is already GENERAL and ts-rs-exported. Do not fork it:

```rust
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/DryRunReport.ts")]
pub struct DryRunReport {
    pub added: BTreeMap<String, usize>,
    pub overwritten: BTreeMap<String, usize>,
    pub deleted: BTreeMap<String, usize>,
    pub total_deleted: usize,
}
```

---

### `src/db/color.rs` (service, CRUD) — EDIT-02 recolor

**Analog:** `app/src-tauri/src/db/delete.rs` (envelope) + `src/db/labels.rs:73-79` (palette)

**Parameterized IN-clause — the SAFE-02 pattern** (`delete.rs:205-212`). This exact three-line
shape is what every new `apply_*` copies; only the placeholder COUNT is ever dynamic:

```rust
pub fn delete_notes(tx: &Transaction, ids: &NonEmptyNoteIds) -> Result<usize, ArchiveError> {
    let placeholders: String = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM Note WHERE NoteId IN ({placeholders})");
    tx.execute(&sql, rusqlite::params_from_iter(ids.iter()))
        .map_err(|e| map_sqlite_err(e, "delete_notes"))
}
```

For a recolor the bound value list is `once(&color).chain(ids.iter())` — the color param comes
FIRST because it appears first in the SQL text.

**Dry-run envelope — copy verbatim, change only the mutation line and the table set**
(`delete.rs:223-259`). Note the four comments; they encode non-obvious invariants
(`unchecked_transaction` vs `transaction`, why `drop(guard)` is separate from `drop(tx)`):

```rust
pub fn dry_run_delete_notes(conn: &mut Connection, ids: &NonEmptyNoteIds)
    -> Result<DryRunReport, ArchiveError> {
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_sqlite_err(e, "setting dry-run pragmas"))?;

    // `unchecked_transaction` (shared `&self`) because `guard` already holds
    // a shared borrow of `conn` for the duration of this function — see
    // `PragmaGuard`'s docs (same pattern as `trim_db`).
    let tx = conn.unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening dry-run transaction"))?;

    let before = snapshot_all(&tx)?;
    delete_notes(&tx, ids)?;
    trim_sweep(&tx)?;
    let after = snapshot_all(&tx)?;

    let report = diff_snapshots(&before, &after);

    // Deliberately DROPPED without `.commit()` — `Transaction::drop`'s
    // default `DropBehavior::Rollback` issues an automatic `ROLLBACK`, so
    // nothing above is ever persisted (SAFE-01).
    drop(tx);
    // Restores the snapshotted PRIOR pragma values.
    drop(guard);

    Ok(report)
}
```

**Color palette — already ported, do not re-derive** (`labels.rs:37`, `:73-79`):

```rust
const COLOR_NAMES: [&str; 7] = ["Grey", "Yellow", "Green", "Blue", "Red", "Orange", "Purple"];

/// Ports `process_color` (`JWLManager.py:598-599`).
pub(crate) fn process_color(color_index: i64) -> String {
    COLOR_NAMES
        .get(usize::try_from(color_index.max(0)).unwrap_or(0))
        .unwrap_or(&"Grey")
        .to_string()
}
```

`process_color` is `pub(crate)` — the ColorMenu needs the 7 names on the frontend, so either
widen visibility and add a ts-rs-exported palette const, or hard-code the same 7 names in the
component. Prefer widening: `Category` (`src/category.rs:9-18`) is the precedent for
single-sourcing an enum-shaped constant into TS via ts-rs.

**Note→UserMark synthesis constraints:** `UserMark.UserMarkGuid` is `TEXT NOT NULL UNIQUE` and
`UserMark.LocationId` is `NOT NULL` — so synthesis is only legal for a Note that HAS a
`LocationId` (exactly the Python's `WHERE LocationId IS NOT NULL AND UserMarkId IS NULL`
predicate). A Note with `LocationId IS NULL` (an independent note — `BrowseRow.independent`,
`notes.rs:52`) can never be recolored. Surface that as a no-op or typed error, never a
constraint failure.

---

### `src/db/highlights.rs` (service, geometric transform) — `merge_block_ranges`

**Analog:** partial — `app/src-tauri/src/archive/downgrade.rs:519-537` is the only in-repo
"compute groups, then delete/repoint on a predicate" code. The geometric overlap test itself has
**no analog** and is net-new.

What to copy from `downgrade.rs`: the shape of computing the full group/absorb set as a plain
`Vec`/`BTreeMap` in Rust FIRST, then issuing the DELETE/INSERT — never a SQL statement that
both selects and mutates on the predicate. That separation is what makes the operation
unit-testable without a DB round-trip and is why `downgrade.rs` survived Phase 4 review.

```rust
// Source: app/src-tauri/src/archive/downgrade.rs:519-528
let groups = compute_merge_groups(&tx)?;
let merged_old: Vec<i64> = groups.iter().flat_map(|(_, olds)| olds.iter().copied()).collect();

let mut repoint: BTreeMap<&str, i64> = BTreeMap::new();
for (table, col) in REMAP_TARGETS {
    *repoint.entry(table).or_insert(0) += count_in_ids(&tx, table, col, &merged_old)?;
}
```

Recommended signature, so the absorb decision is a pure function that tests can hit directly
without SQLite:

```rust
/// Pure geometry: given existing (id, start, end) ranges at one
/// (Identifier, LocationId) and a new [ns, ne], returns the absorbed ids and
/// the expanded union. NO SQL — exhaustively unit-testable.
fn plan_merge(existing: &[(i64, i64, i64)], ns: i64, ne: i64) -> (Vec<i64>, (i64, i64));
```

**BlockRange `CHECK (BlockType BETWEEN 1 AND 2)`** binds the merged INSERT — carry the absorbed
rows' `BlockType` through, never default it to 0.

This module is the phase's highest-risk code (D7-03). It is also blocked on the recolor/merge
criterion checkpoint — the planner should land the primitive + its tests independent of whether
recolor calls it.

---

### `src/db/tags.rs` (service, CRUD) — EDIT-03

**Analog:** `delete.rs` envelope + `src/db/trim.rs:171-205` for the staging-table technique

`get_available_ids` gap-fill (D7-04) is net-new logic, but the "compute the full plan in Rust,
then execute" discipline from `redensify_tag_positions` applies. Note that `trim.rs` uses an
**explicit column list** on every INSERT and says why:

```rust
/// Re-densifies `TagMap.Position` to be contiguous 0-based per `TagId`,
/// ordered by original `Position` then `TagMapId` — `JWLManager.py:3883-3886`.
/// Uses an EXPLICIT column list on the final `INSERT` (never `SELECT *`), so
/// the re-densify is immune to `TagMap`'s column order ever changing.
fn redensify_tag_positions(tx: &Transaction, counts: &mut BTreeMap<String, usize>)
    -> Result<(), ArchiveError> {
    tx.execute(
        "CREATE TEMP TABLE TagMapNew AS SELECT TagMapId, PlaylistItemId, LocationId, NoteId, \
         TagId, ROW_NUMBER() OVER (PARTITION BY TagId ORDER BY Position, TagMapId) - 1 AS Position \
         FROM TagMap",
        [],
    )
    .map_err(|e| map_sqlite_err(e, "tagmap_redensify_stage"))?;
    // ... DELETE FROM TagMap; INSERT INTO TagMap (<explicit cols>) SELECT <explicit cols> FROM TagMapNew;
    tx.execute("DROP TABLE TagMapNew", [])
        .map_err(|e| map_sqlite_err(e, "tagmap_redensify_cleanup"))?;
    Ok(())
}
```

**Constraint map for this op** (verified schema, above): `INSERT OR IGNORE` guards
`CONSTRAINT TagId_NoteId`; new-tag creation must respect `UNIQUE (Type, Name)` +
`CHECK (length(Name) > 0)` + `CHECK (Type IN (0,1,2))`; the `TagMap` `CHECK` requires exactly
one of `PlaylistItemId`/`NoteId`/`LocationId` non-NULL, so a note-tag row is
`(NULL, NULL, NoteId, TagId, Position)`.

---

### `src/db/reorder.rs` (service, batch) — EDIT-04 two-pass

**Analog:** `src/db/trim.rs:171-205` — same table, same constraint, already-proven technique

`trim.rs`'s re-densify dodges `UNIQUE(TagId, Position)` by staging to a TEMP table and doing a
full DELETE+re-INSERT. The Python's `sort_notes` instead uses the negative-position two-pass.
Both are collision-free; the planner picks one (D7-05 recommends the faithful two-pass). If the
two-pass is chosen, the loop shape is the one already in RESEARCH §Code Examples. Either way:

- Assert the post-condition the same way `trim_tests.rs` does — 0-based dense per `TagId`.
- Never `PRAGMA ignore_check_constraints` or drop the constraint to force the write.
- Reorder sets the ORDER; save's `trim_db` re-densifies (`trim.rs:177`) — a reorder followed by
  a save must be idempotent. Test that composition, not just reorder alone.

---

### `src/db/favorites.rs` (service, CRUD) — EDIT-05

**Analog (unmark):** `delete.rs:205-212` verbatim with `DELETE FROM TagMap WHERE TagMapId IN (...)`
and `NonEmptyTagMapIds`. The Favorites identity PK is already established at
`src/db/browse.rs:39-45`:

```rust
/// `get_favorites` — `JWLManager.py:669`. Identity = `TagMapId`. The
/// `WHERE tm.NoteId IS NULL ORDER BY tm.Position` is load-bearing: a Favorite
/// is a TagMap row with a NULL NoteId; dropping the predicate lists note-tag
/// mappings as favorites.
const FAVORITES_SQL: &str = "SELECT LocationId, l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, \
    TagMapId \
    FROM TagMap tm JOIN Location l USING (LocationId) WHERE tm.NoteId IS NULL ORDER BY tm.Position";
```

**Analog (edition list):** `src/db/resources.rs:40-113` `ResourceCatalog::load`. Add a sibling
loader for the `Favorites` VIEW, following the same prepare/query_map/collect shape and the same
`ArchiveError` propagation. The catalog's existing shape:

```rust
// Source: app/src-tauri/src/db/resources.rs:26-35
#[derive(Debug, Clone)]
pub struct ResourceCatalog {
    lang_name: HashMap<i64, String>,
    bible_books: HashMap<i64, String>,
    publications: HashMap<String, PublicationInfo>,
}
```

```rust
// Source: app/src-tauri/src/db/resources.rs:41-49 — the load shape to mirror
pub fn load(resources_db_path: &Path, ui_lang_code: &str) -> Result<Self, ArchiveError> {
    let conn = Connection::open(resources_db_path)?;
    let mut lang_name = HashMap::new();
    let mut ui_lang_id: Option<i64> = None;
    {
        let mut stmt = conn.prepare("SELECT Language, Name, Code FROM Languages")?;
        let rows = stmt.query_map([], |row| { /* ... */ })?;
        for row in rows { /* ... */ }
    }
```

New query: `SELECT Language, Symbol, Short, Lang FROM Favorites` (the VIEW). `Language` is the
integer `MepsLanguage` that goes into the `Location` INSERT; `Symbol` is the `KeySymbol`; `Short`
and `Lang` are the display strings the picker shows. This resolves the Python's polars
`favorites.filter((pl.col('Short') == pub) & (pl.col('Lang') == lng))` at `JWLManager.py:3451`.

**Kill the Python's f-string here.** `JWLManager.py:3455` is
`con.execute(f"SELECT TagMapId FROM TagMap WHERE LocationId = {location} AND TagId = (...)")` —
wart #20. Parameterize it. And per the verified schema, the dup case is also a hard
`CONSTRAINT TagId_LocationId UNIQUE (TagId, LocationId)` violation, so the pre-check is a
user-facing typed error, not just a nicety.

---

### `src/db/scrub.rs` (service, batch) — EDIT-06 clean + mask

**Analog (labeled bulk statements):** `src/db/trim.rs:155-166` — the per-label change-count
accumulator, used because `execute_batch` discards `changes()`:

```rust
/// Runs one ordered slice of (label, SQL) pairs via individual `tx.execute`
/// calls (never `execute_batch`) so each statement's `changes()` count can be
/// captured per label — a future dry-run/report consumer sums these.
fn run_labeled_sweep(tx: &Transaction, steps: &[(&str, &str)],
                     counts: &mut BTreeMap<String, usize>) -> Result<(), ArchiveError> {
    for (label, sql) in steps {
        let changed = tx.execute(sql, []).map_err(|e| map_sqlite_err(e, label))?;
        *counts.entry((*label).to_string()).or_insert(0) += changed;
    }
    Ok(())
}
```

**Analog (regex statics):** `src/db/labels.rs:20-40` — regexes are module-level `LazyLock` statics
with an `.expect("... must compile")` and a why-comment, not compiled per row. Note that the
crate-level `#![deny(clippy::expect_used)]` (`lib.rs:4`) is deliberately tolerated here because a
static regex compile is a build-time-constant invariant, not an archive-data path — follow the
same justification, and keep `.expect` OUT of any per-row code.

`regex = "1"` is declared (`Cargo.toml:38`) with a scoping comment worth matching in tone:

```toml
# process_code/process_yr regex ports (JWLManager.py:930-931) — used only
# against internally-sourced KeySymbol strings, never raw user input.
regex = "1"
```

Clean/mask operate on **user-authored text**, which is a wider input surface than
`process_code`'s — note that when the dep comment is updated.

**Mask RNG has no analog and no crate.** See Shared Pattern 6.

---

### `src/db/record_edit.rs` (service, CRUD) — EDIT-07

**Analog:** `delete.rs` envelope, plus `src/time.rs:29-44` for the `LastModified` stamp:

```rust
/// Returns the current UTC time formatted as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now_iso8601_utc() -> String { /* civil_from_days + format! */ }
```

Already threaded through commands as a parameter, not called inside the core fn — see
`lib.rs:132` (`save_archive(session, APP_NAME, APP_DEVICE_NAME, &time::now_iso8601_utc())`) and
`lib.rs:171`. **Copy that injection pattern**: `record_edit`'s core fn takes `now: &str` so tests
are deterministic; the command supplies `&time::now_iso8601_utc()`. `Note.LastModified` has a DB
default of `strftime('%Y-%m-%dT%H:%M:%SZ','now')` — the same shape, so the formats agree.

Annotations are keyed `(LocationId, TextTag)`. The two annotation delete paths differ and must
not be crossed (RESEARCH Pitfall 4): browse-list delete is by `LocationId` (over-deletes, by
design); the record-editor single delete is by `(LocationId, TextTag)`.

---

### `src/db/delete.rs` (modified) — per-category deletes (D7-10)

**Analog:** itself. Five new fns, each a clone of `delete_notes` (`:205-212`) with a different
table/column, each with its own `NonEmpty*Ids`:

| Category | SQL | Identity PK source |
|----------|-----|--------------------|
| Bookmarks | `DELETE FROM Bookmark WHERE BookmarkId IN (...)` | `browse.rs:33-37` |
| Favorites | `DELETE FROM TagMap WHERE TagMapId IN (...)` | `browse.rs:39-45` |
| Highlights | `DELETE FROM BlockRange WHERE BlockRangeId IN (...)` — **not** UserMark (rule #9) | `browse.rs:47-52` |
| Annotations | `DELETE FROM InputField WHERE LocationId IN (...)` — over-deletes by design (rule #10) | `browse.rs:28-31` |
| Playlists | **DEFERRED to Phase 8** (ref-counted media) | `browse.rs:54-58` |

Each `browse.rs` const carries a comment naming its identity PK — those comments ARE the
contract Phase 6 verified, e.g. `browse.rs:33-34`:

```rust
/// `get_bookmarks` — `JWLManager.py:656`. Identity = `BookmarkId` (col 5),
/// NOT the first-SELECTed `LocationId` — the load-bearing pitfall for Phase 7.
```

---

### `src/lib.rs` (modified) — the command pairs

**Analog:** `lib.rs:187-272` — `delete_notes_dry_run` / `delete_notes_apply`

**Dry-run command** (`:187-206`) — lock, `as_ref`, open conn, delegate, map every error through
`to_dto` with the operation name and the session's target path:

```rust
#[tauri::command]
fn delete_notes_dry_run(
    ids: NonEmptyNoteIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("delete_notes_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("delete_notes_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("delete_notes_dry_run", Some(session.target_path.as_path()))
    })?;

    db::delete::dry_run_delete_notes(&mut conn, &ids)
        .map_err(|err| err.to_dto("delete_notes_dry_run", Some(session.target_path.as_path())))
}
```

**Apply command** (`:215-272`) — `as_mut` (needs `session.dirty = true`), PragmaGuard, explicit
`tx.commit()`, `drop(guard_pragma)`, then the dirty flag:

```rust
#[tauri::command]
fn delete_notes_apply(
    ids: NonEmptyNoteIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state.lock().map_err(/* StatePoisoned */)?;
    let session = guard.as_mut().ok_or_else(/* MissingUserDataBackup */)?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(/* ... */)?;

    // Mirrors `JWLManager.py:3681`/`trim_db`: Note deletion must run with
    // `foreign_keys` OFF (TagMap.NoteId still references the row being
    // deleted until trim sweeps it on save), restored via `PragmaGuard`.
    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(/* ... */)?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';").map_err(/* ... */)?;

    // `unchecked_transaction` (shared `&self`) because `guard_pragma` already
    // holds a shared borrow of `conn` for the duration of this function —
    // same pattern as `trim_db`/`dry_run_delete_notes`.
    let tx = conn.unchecked_transaction().map_err(/* ... */)?;
    let deleted = db::delete::delete_notes(&tx, &ids).map_err(/* ... */)?;
    tx.commit().map_err(/* ... */)?;
    drop(guard_pragma);

    session.dirty = true;
    // ... build and return the DryRunReport
}
```

**Handler registration** (`lib.rs:386-400`) — a flat list; append the new pairs:

```rust
.invoke_handler(tauri::generate_handler![
    open_archive,
    jwlcore::loader::check_jwlcore,
    save_archive,
    save_as,
    new_archive,
    delete_notes_dry_run,
    delete_notes_apply,
    downgrade_dry_run,
    save_v14_copy,
    merge_dry_run,
    merge_commit,
    list_notes,
    list_category
])
```

Two ops are **archive-wide with no selection** (clean, mask) — their commands take no `ids`
param. `downgrade_dry_run` (`lib.rs:280-296`) is the existing no-argument dry-run analog for
exactly that shape.

---

### `src/error.rs` (modified) — new typed variants

**Analog:** the `DeleteFailed` pair. Variant (`:40-41`):

```rust
#[error("note delete failed: {reason}")]
DeleteFailed { reason: String },
```

`to_dto` arm (`:125-127`) — note the mandatory comment explaining why `reason` is dropped:

```rust
// `reason` is internal-only (module docs) — the DTO exposes only
// the stable code + message_key; the frontend copy is generic.
ArchiveError::DeleteFailed { .. } => ("delete_failed", "error.archive.delete_failed"),
```

Whether one `EditFailed { op, reason }` or seven per-op variants is discretion (D7 Discretion) —
but `to_dto` is an exhaustive `match` with no `_` arm (`:98-151`), so every new variant is a
compile-time-forced decision. That is the safety property; do not add a catch-all arm.

---

## Shared Patterns

### Shared Pattern 1 — PragmaGuard around every FK-off region

**Source:** `app/src-tauri/src/db/pragma_guard.rs:24-49`
**Apply to:** every `dry_run_*` and every `*_apply` command that touches FK-referenced rows

```rust
pub struct PragmaGuard<'c> {
    conn: &'c Connection,
    foreign_keys: i64,
    journal_mode: String,
    synchronous: i64,
    temp_store: i64,
}

impl<'c> PragmaGuard<'c> {
    /// Reads and stores the connection's current PRAGMA values. Does NOT
    /// change anything yet — callers set sweep-friendly PRAGMA values
    /// AFTER constructing the guard.
    pub fn new(conn: &'c Connection) -> Result<Self, rusqlite::Error> {
        let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
        // ... journal_mode, synchronous, temp_store
    }
}
```

Two non-obvious rules the doc comment (`pragma_guard.rs:16-23`) encodes:
1. The guard holds `&Connection`, **not** `&mut` — so callers must use
   `conn.unchecked_transaction()`, never `conn.transaction()`. Every existing call site does this
   and says so.
2. `PRAGMA foreign_keys` is a **no-op inside an active transaction** (`trim.rs:231-233`) — set
   pragmas BEFORE opening the tx, always.

### Shared Pattern 2 — reuse `trim_sweep` inside dry-runs, never `trim_db`

**Source:** `app/src-tauri/src/db/trim.rs:211-217`, and the rationale at `trim.rs:27-34`
**Apply to:** every `dry_run_*`

```rust
pub fn trim_sweep(tx: &Transaction) -> Result<BTreeMap<String, usize>, ArchiveError> {
    let mut counts = BTreeMap::new();
    run_labeled_sweep(tx, SWEEP_PRE_REDENSIFY, &mut counts)?;
    redensify_tag_positions(tx, &mut counts)?;
    run_labeled_sweep(tx, SWEEP_POST_REDENSIFY, &mut counts)?;
    Ok(counts)
}
```

`trim_db` (`:230`) VACUUMs and is **not** rollback-able — a dry-run that called it would corrupt
the working copy. `trim_sweep` is DML-only precisely so previews can run it. `downgrade.rs:510`
also shows the ordering choice matters: it runs `trim_sweep` BEFORE snapshotting so the report
reflects only the op's own effect, not pre-existing orphans. Each Phase 7 op must make that
before/after-trim snapshot-placement decision explicitly.

### Shared Pattern 3 — the frontend preview/confirm dialog

**Source:** `app/src/components/DeletePreviewDialog.tsx` (whole file, 121 lines)
**Apply to:** `EditPreviewDialog` (rename) and every new dialog

It is already parameterized for reuse — Phase 4 and 5 pass overrides. The prop surface
(`:4-21`) is the extension point; `MaskConfirmDialog` should be a superset (extra typed-confirm
gate), not a fork:

```tsx
interface DeletePreviewDialogProps {
  report: DryRunReport;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
  /** Dialog heading. Defaults to the Notes-delete copy. */
  title?: string;
  /** `aria-label` for the dialog role. Defaults to "Confirm delete". */
  ariaLabel?: string;
  /** Overrides the summary body entirely (caller-driven copy, e.g. the v14
   * "N Locations will be merged" framing). */
  summary?: ReactNode;
  confirmLabel?: string;
  confirmPendingLabel?: string;
}
```

**The double-click guard is mandatory** (`:48-63`) — a synchronous ref, not just React state,
because state updates are async and a fast second click would fire a duplicate `invoke`:

```tsx
const [pending, setPending] = useState(false);
const busyRef = useRef(false);

const handleConfirm = useCallback(async () => {
  if (busyRef.current) {
    return; // double-click guard: no-op, not a duplicate invoke
  }
  busyRef.current = true;
  setPending(true);
  try {
    await onConfirm();
  } finally {
    busyRef.current = false;
    setPending(false);
  }
}, [onConfirm]);
```

**Visual restraint is a documented constraint** (`:30-33`): "`--bg-secondary` card, hairline
border, `rounded-xl`; the destructive red accent is restrained to the Confirm button only, never
a full red-flooded modal." The mask dialog is the one place to add friction — do it with a typed
confirm, not with alarm styling.

Every interactive element carries a stable `data-testid` (`:85`, `:88`, `:103`, `:113`) — the
vitest suites select on those exclusively.

### Shared Pattern 4 — dry-run → preview → apply wiring in the list

**Source:** `app/src/components/CategoryList.tsx:129-160`
**Apply to:** every newly-LIVE operation

```tsx
const handleDeleteClick = useCallback(async () => {
  if (selected.size === 0 || dryRunPending) {
    return;
  }
  setDryRunPending(true);
  try {
    const ids = Array.from(selected);
    const dryRunReport = await invoke<DryRunReport>("delete_notes_dry_run", { ids });
    setReport(dryRunReport);
  } catch (err) {
    onError?.(err as ErrorDto);
  } finally {
    setDryRunPending(false);
  }
}, [selected, dryRunPending, onError]);

const handleConfirm = useCallback(async () => {
  const ids = Array.from(selected);
  try {
    await invoke("delete_notes_apply", { ids });
    onRowsChanged?.(rows.filter((row) => !selected.has(row.id)));
    setSelected(new Set());
  } catch (err) {
    onError?.(err as ErrorDto);
  } finally {
    setReport(null);
  }
}, [selected, rows, onRowsChanged, onError]);
```

Selection is `Set<bigint>` keyed on `row.id` (the category identity PK) and is reset on category
change (`:105-108`). `Array.from(selected)` yields `bigint[]`, which serializes to the Rust
`Vec<i64>` — that is the existing wire contract for `NonEmpty*Ids`; every new op reuses it.

**Op-bar dispatch** (`:183-213`) — the `if (live) {...} return <deferred/>` shape is what
Phase 7 progressively replaces. As each backend lands, one branch moves from the deferred
fallback into a live handler.

### Shared Pattern 5 — the capability flip

**Source:** `app/src/lib/operations.ts:50-54`
**Apply to:** each newly-live `(category, op)` pair

```ts
/**
 * The ONLY (category, op) pairs wired to a real backend mutation in Phase 6.
 * Keyed as `${Category}:${Op}`. Everything not in this set renders deferred.
 */
const LIVE: ReadonlySet<string> = new Set<string>(["Notes:delete"]);
```

The `Op` union (`:20`) is `"delete" | "export" | "view" | "color" | "tag" | "add" | "import"`.
Phase 7 makes `delete`/`view`/`color`/`tag`/`add` live per the `CAPABILITY` table (`:27-34`);
`export`/`import` stay deferred (Phase 8). Clean/mask are **archive-wide with no selection** and
have no `(category, op)` slot at all — they need either a new selection-independent `Op` member
(and a `NEEDS_SELECTION` exclusion, `:42-48`) or a separate app-level menu surface. Flag that as
a design decision; `operations.ts` as written cannot express an archive-wide op.

Keep the module doc-block updated — it states the phase scope explicitly (`:11-16`) and will read
as stale the moment the LIVE set grows.

### Shared Pattern 6 — no-new-dependency precedent (for UUID and the mask RNG)

**Source:** `app/src-tauri/src/time.rs:1-6`
**Apply to:** `UserMarkGuid` synthesis (D7-02) and the mask word chooser (D7-08)

Neither `uuid` nor `rand` is a declared dependency (verified: `Cargo.toml:15-38`; `uuid` appears
in `Cargo.lock:4073` transitively only; `rand` is absent). The repo already faced this exact
fork and chose to hand-roll with a cited algorithm plus shape tests:

```rust
//! Dependency-free `YYYY-MM-DDTHH:MM:SSZ` UTC timestamp formatting (matches
//! `JWLManager.py`'s `datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')`
//! shape used for `creationDate`/`lastModifiedDate`). No `chrono`/`time`
//! dependency is added for this single formatting need — civil-date
//! conversion from a Unix day count is a well-known, leap-second-free
//! algorithm (Howard Hinnant's `civil_from_days`).
```

with tests asserting shape and known values (`time.rs:47-66`) rather than exact output.

Either path is defensible; the planner must pick explicitly and, if adding a dep, run the
`checkpoint:human-verify` the RESEARCH legitimacy protocol requires. Two notes:
- `UserMarkGuid` is `TEXT NOT NULL UNIQUE` — any generator must produce a value not already
  present. A v4-shaped string from `getrandom` (already in the lock file, transitively) or a
  deterministic-per-session counter+timestamp both satisfy the schema.
- The mask RNG must be **seedable** for tests (RESEARCH Pitfall 6). A hand-rolled
  xorshift/PCG with an explicit seed parameter satisfies that directly; `rand` would need
  `StdRng::seed_from_u64`. Threading `seed: u64` through the core fn mirrors how `now: &str` is
  threaded for timestamps (`lib.rs:132`).

### Shared Pattern 7 — module registration

**Source:** `app/src-tauri/src/db/mod.rs` (entire file)

```rust
//! Read-side database access over the extracted `userData.db`.

pub mod browse;
pub mod delete;
pub mod labels;
pub mod notes;
pub mod pragma_guard;
pub mod resources;
pub mod trim;
```

Alphabetical, all `pub`. The doc line says "Read-side" — Phase 7 makes that false; update it.

---

## Test Patterns

### `tests/*_tests.rs` (per-op backend tests)

**Analog:** `app/src-tauri/tests/delete_tests.rs` (whole file, 161 lines)

Header block — every integration test file opens with the same three lines:

```rust
//! EDIT-01 / SAFE-02 / SAFE-03 / SAFE-04 coverage for the delete backend
//! (02-02-PLAN.md Task 2). ...

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::delete::{delete_notes, NonEmptyNoteIds};
use rusqlite::Connection;
```

Test body shape (`delete_tests.rs:18-29`) — fixture, extract, open, FK off, `unchecked_transaction`,
act, assert, `rollback`:

```rust
#[test]
fn test_delete_notes_removes_selected_rows() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let conn = Connection::open(&db_path).expect("open extracted db");
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let ids = NonEmptyNoteIds::try_from(vec![900_i64]).unwrap();

    let tx = conn.unchecked_transaction().expect("open tx");
    let deleted = delete_notes(&tx, &ids).expect("delete_notes must succeed");
    assert_eq!(deleted, 1, "exactly one Note row must be removed");
    // ... EXISTS assertions ...
    tx.rollback().unwrap();
}
```

Three test archetypes to instantiate per op group:
1. **Happy path + negative-space assertion** (`:18-60`) — assert what must NOT change, not just
   what must. Phase 7's equivalent: recolor must not touch unselected UserMarks; reorder must
   not touch `Type != 1` tags; clean must not touch publication-linked rows.
2. **Empty selection fails deserialization** (`:65-72`) — one per new `NonEmpty*Ids`:
   ```rust
   let empty: Result<NonEmptyNoteIds, _> = serde_json::from_str("[]");
   assert!(empty.is_err(), "empty selection must fail to deserialize");
   ```
3. **Adversarial ids bind harmlessly** (`:79-120`) — `vec![900, -1, i64::MAX, 123456789012345, -987654321]`,
   asserting only the real match is affected.
4. **Rollback on forced mid-transaction failure** (`:126-161`) — inject
   `tx.execute("SELECT ForcedFailureColumn FROM Note", [])` and assert
   `normalized_table_rows` before == after.

### `tests/edit_roundtrip_tests.rs`

**Analog:** `app/src-tauri/tests/delete_roundtrip_tests.rs`

```rust
//! QA-02 semantic round-trip: delete -> save (trim runs) -> reopen equals
//! the expected normalized post-state (02-02-PLAN.md Task 3). NEVER asserts
//! byte equality — save is not byte-preserving (VACUUM + tag re-densify),
//! only `common::normalized_table_rows`/targeted-existence assertions on the
//! reopened archive are used, per CLAUDE.md's Core Value.

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::archive::save::save_archive;
use jwlmanager_lib::db::resources::dev_resources_db_path;
```

The full cycle (`:39-56`): `open_and_validate` → apply the mutation on `session.db_path` →
`save_archive(&session, "JWL Manager", "JWL Manager_test", "2026-01-02T00:00:00Z")` →
`extract_to_tempdir(&session.target_path)` → assert on the reopened DB. Note the **fixed
timestamp literal** passed to `save_archive` — determinism by parameter injection, the same
technique `record_edit`'s `now: &str` should use.

### `tests/common/mod.rs` fixture extensions

**Analog (multi-category seeding):** `common/mod.rs:520-570` `insert_all_categories_rows` — each
INSERT carries an explicit id and a comment naming what it proves:

```rust
// Highlight: one UserMark (650) over Location 500 with TWO BlockRanges
// (633, 644) — proves one-row-per-BlockRange. ColorIndex 2 -> "Green".
conn.execute(
    "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
     VALUES (650, 2, 500, 0, 'fixture-highlight-usermark-0650', 1)",
    [],
)
.expect("insert highlight UserMark");
conn.execute(
    "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
     VALUES (633, 1, 1, 0, 5, 650)",
    [],
)
.expect("insert highlight BlockRange 633");
```

That existing highlight fixture is **already almost** the range-merge fixture: BlockRanges
`(Identifier 1, 0-5)` and `(Identifier 2, 6-10)` on the SAME UserMark. Extend with same-Identifier
overlapping/touching/containing/disjoint cases.

**Analog (composite-key fixtures):** `common/mod.rs:889-978` — four `generate_composite_*_db`
generators already exist for exactly the tables D7-11 flags, including:

```rust
pub fn generate_composite_inputfield_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
    insert_collision_group(&conn, &[20, 90]);
    // Survivor (LocationId 20, TextTag 'x') + merged-away (90, 'x').
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (20, 'x', 'survivor')",
        [],
    )
    .expect("insert survivor InputField");
    // ...
}
```

`generate_composite_tagmap_db` (`:889`) and `generate_composite_bookmark_slot_db` (`:916`) are the
TagMap and Bookmark equivalents. Reuse or clone these rather than authoring new composite
fixtures from scratch.

**Semantic comparison helper** (`common/mod.rs:58-63`, `normalized_table_rows`) — `BTreeMap` of
stringified-row → occurrence count, so row ORDER never matters and byte-diffing is impossible by
construction. This is the ONLY sanctioned way to compare table state.

### Frontend tests

**Analog (dialog):** `app/src/components/DeletePreviewDialog.test.tsx` — four cases: renders
report counts, double-click fires `onConfirm` once, pending disables the button, Cancel never
calls `onConfirm`. Every new dialog needs the same four.

**Analog (list + IPC):** `app/src/components/CategoryList.test.tsx:1-70` — the `invoke` mock and
the `ResizeObserver`/`clientHeight` stubs that the virtualizer requires:

```tsx
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

beforeAll(() => {
  class ResizeObserverMock {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  // @ts-expect-error test-only global stub
  global.ResizeObserver = ResizeObserverMock;
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, value: 600 });
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 600 });
});

beforeEach(() => { invokeMock.mockReset(); });
```

`makeRow(id, overrides)` (`:12-29`) is the row factory — note `id: BigInt(id)`, matching the
`Set<bigint>` selection contract.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `src/db/highlights.rs` — the overlap/absorb geometry itself | service | transform | No geometric-predicate code exists in the repo. `downgrade.rs:519-537` supplies the *structure* (compute plan in Rust, then mutate) but not the algorithm. Port `JWLManager.py:2160-2184` from scratch, as a pure fn over `&[(id, start, end)]` so it is testable without SQLite. |
| Mask word-cycling + RNG (`src/db/scrub.rs`) | service | transform | No randomness anywhere in the Rust crate; `rand` is not in `Cargo.lock`. Use the `src/time.rs` no-new-dep precedent (Shared Pattern 6) with an explicit `seed: u64` param, or declare a dep behind a checkpoint. |
| `src/components/RecordEditor.tsx` | component | CRUD | No form/text-input component exists in `app/src/components/` — every current component is read-only or confirm-only. `DeletePreviewDialog.tsx` supplies the modal shell, busy-ref, and testid conventions; the field-editing surface is net-new. UI work here must route through the `taste-skill` plugin per global rule 22b, and read `app/.../01-UI-SPEC` conventions the dialog cites. |

---

## Metadata

**Analog search scope:** `app/src-tauri/src/` (db, archive, error, category, session, time),
`app/src-tauri/tests/`, `app/src/components/`, `app/src/lib/`, `app/src/bindings/`;
plus schema introspection of `res/blank` (`userData.db`) and `res/resources.db`, and the Python
source of truth `JWLManager.py`.

**Files scanned:** 60 source files (line-counted), 18 read in full or in targeted spans, 2
SQLite databases introspected.

**Pattern extraction date:** 2026-07-24
