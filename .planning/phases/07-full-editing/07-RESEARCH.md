# Phase 7: Full Editing - Research

**Researched:** 2026-07-23
**Domain:** SQLite archive mutation (highlight color + geometric range merge, tag CRUD + constraint-safe reorder, favorites, text clean/mask, per-record editor) in Rust/rusqlite behind Tauri, reusing the Phase 2 dry-run/rollback safety spine.
**Confidence:** HIGH (all sources are in-repo: the Python source of truth, the FUNCTIONALITY-SPEC, and the already-built Rust safety infra — no external/registry dependencies introduced)

<user_constraints>
## User Constraints (from 07-CONTEXT.md)

### Locked Decisions
Copied from `07-CONTEXT.md ## Implementation Decisions` (D7-01..D7-13). The planner MUST honor these. Highest-attention items:
- **D7-01** Every edit op reuses the Phase 2 safety pattern: typed non-empty selection (`#[serde(try_from)]`), `apply_*(tx)` with `params_from_iter`, `dry_run_*` in a never-committed `unchecked_transaction` under `PragmaGuard`, semantic `DryRunReport` via `snapshot_tables`/`diff_snapshots`, and a `<op>_dry_run`/`<op>_apply` command pair.
- **D7-02/D7-03** Highlight recolor ports `set_color` exactly (UPDATE ColorIndex + Note→UserMark synthesis + Highlights-Grey no-op); the union-merge lives in `add_usermark` (import), NOT recolor — **resolve the ROADMAP-criterion mismatch via a checkpoint before coding**. Highest data-integrity risk.
- **D7-04/D7-05** Tag CRUD ports `tag_notes` tri-state + ID recycling; reorder ports the two-pass negative-position rewrite (mandatory for `TagMap UNIQUE(TagId,Position)`).
- **D7-06** Favorites mark/unmark only; playlist media add deferred to Phase 8.
- **D7-07/D7-08** Clean = separator scrub; Mask = irreversible archive-wide letter-replace → strengthen the guard beyond Python's one-click.
- **D7-09** Raw editor is FIELD-CONSTRAINED (Notes: Title/Content/Color; Annotations: Value) — never arbitrary SQL.
- **D7-10** Simple per-category deletes land here; Playlist media delete → Phase 8.
- **D7-11** Composite-key hazards (`TagMap`, `InputField`, `Bookmark`) need care in dry-run snapshotting.

### Claude's Discretion
Module layout, one-vs-per-op error variants, range-merge primitive location, UUID v1-vs-v4, edit-dialog component names/shape, mask acknowledgement strength, `EditPreviewDialog` naming, exact wave boundaries. (Verbatim list in `07-CONTEXT.md`.)

### Deferred Ideas (OUT OF SCOPE)
Playlist media add + Playlist delete; import/export + `.txt` formats; incremental export; N-way merge; duplicate-notes CTE / grouping / title modes / sort; localization / theme / geometry; crash telemetry (never ported).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| EDIT-02 | Change highlight colors, overlapping ranges union-merged as Python does | §Op 1 (color `set_color:3237-3278`; range-merge `add_usermark:2160-2184`) — mismatch flagged; `process_color`/`COLOR_NAMES` already ported |
| EDIT-03 | Add, remove, rename tags | §Op 2 (`tag_notes:3281-3386` tri-state + `get_available_ids` recycling) |
| EDIT-04 | Reorder items (two-pass negative-position, no TagMap uniqueness violation) | §Op 2b (`sort_notes:3825-3855` two-pass) |
| EDIT-05 | Mark items as favorites | §Op 3 (`add_favorite:3391-3460` mark; `TagMap` delete `:3662` unmark) |
| EDIT-06 | Clean/mask data | §Op 4 (`clean_items:3698-3748`; `obscure_items:3750-3823`) |
| EDIT-07 | View and edit underlying records directly | §Op 5 (`update_notes`/`update_annotations:2835-2855` — field-constrained) |
</phase_requirements>

## Summary

Phase 7 is a **mutation** phase built almost entirely on machinery Phase 2 already shipped. The `db/delete.rs` module is a reference implementation of the exact safety contract every edit op needs: an empty-selection-unrepresentable typed wrapper, a parameterized `apply` that runs in the caller's transaction, and a `dry_run` that executes the REAL mutation inside a never-committed transaction (auto-rollback on drop) under a `PragmaGuard`, returning a semantic before/after PK-diff (`DryRunReport`). Five of the six edit-op groups are "instantiate that pattern over a different SQL statement." Two carry genuine novelty/risk: the **highlight range union-merge** (a geometric predicate that DELETEs BlockRange rows) and the **two-pass negative-position reorder** (mandatory to dodge `TagMap`'s composite uniqueness constraint).

The single most important research finding is a **spec/behavior mismatch**: ROADMAP criterion 1 requires recolor to "union-merge overlapping ranges exactly as the Python app does," but the Python `set_color` does **not** merge on recolor — the union-merge exists only in the import path (`add_usermark`). This must be resolved (checkpoint) before implementation; do not silently invent a merge-on-recolor. The second finding **lowers** a feared risk: the "raw data editor" (EDIT-07) is not arbitrary SQL — it edits exactly Title/Content/Color (Notes) and Value (Annotations), a bounded, typed, parameterizable surface.

**Primary recommendation:** Generalize the `delete.rs` primitives (`NonEmpty*Ids`, `snapshot_tables`, `diff_snapshots`, the `dry_run_*` envelope) into a shared edit-safety module; implement one op group per wave; extract the range-merge as a single exhaustively-tested `merge_block_ranges` primitive (shared with Phase 8 import); strengthen the mask guard; resolve the recolor/merge criterion mismatch first.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Highlight recolor + UserMark synth | Rust `db` (SQL mutation) | Frontend (color menu + preview) | ColorIndex/UserMark/BlockRange writes are archive-integrity-critical; belong in the audited backend |
| Range union-merge | Rust `db` (geometric primitive) | — | Pure DB geometry; DELETE-on-predicate must be inside dry-run/rollback |
| Tag add/remove/rename | Rust `db` | Frontend (tri-state tag dialog) | ID recycling + TagMap writes are backend; tri-state UI is presentation |
| Tag reorder | Rust `db` (two-pass txn) | Frontend (confirm) | Constraint-safe rewrite is a transaction concern |
| Favorites mark/unmark | Rust `db` | Frontend (edition/lang picker) | Location/TagMap writes backend; picker fed by resources.db `favorites` table |
| Clean / Mask | Rust `db` (whole-archive UPDATE) | Frontend (strong confirm + preview) | Bulk text mutation backend; irreversibility guard is UX |
| Raw record editor | Rust `db` (typed per-record UPDATE) | Frontend (record editor view) | Bounded field writes backend; editor/navigation is presentation |
| Preview / confirm | Frontend (`EditPreviewDialog`) | Rust (`DryRunReport`) | Reuse Phase 2 dialog rendering the semantic report |

## Standard Stack

No new external dependencies. Everything is already in `app/src-tauri/Cargo.toml` from Phases 1-6.

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `rusqlite` | (as in repo, bundled SQLite) | All SQL mutation, transactions, `params_from_iter` | Already the archive DB layer; `unchecked_transaction` supports the PragmaGuard pattern |
| `uuid` | (in repo) | Synthesize `UserMarkGuid` for new UserMarks (`set_color`/raw-editor Note path) | Python uses `uuid.uuid1()`; use `uuid` crate's v1 (byte-parity) or v4 (semantically valid) |
| `serde` + `ts-rs` | (in repo) | Typed selection wrappers + `DryRunReport` bindings across IPC | Already how `NonEmptyNoteIds`/`DryRunReport` cross the boundary |
| `regex` | (in repo, used by browse labels) | Clean/mask Unicode-class scrub | Already a dependency for label synthesis |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `fancy-regex` | verify if present; add only if needed | `regex.V1` set-subtraction `[\p{Zs}--\x20]` has no direct `regex`-crate equivalent | ONLY if the char-class-construction workaround (build `\p{Zs}` + special-case ASCII space) proves insufficient — prefer the no-new-dep workaround |

**Version verification:** No package additions are recommended. If `fancy-regex` becomes necessary for clean/mask, verify and gate it behind a `checkpoint:human-verify` per the legitimacy protocol before adding:
```bash
cargo tree -p app 2>/dev/null | grep -i regex   # confirm what's already available
```

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `regex` char-class workaround for `[\p{Zs}--\x20]` | `fancy-regex` | Extra dep vs. a 3-line explicit-class construction; prefer no dep |
| `uuid` v1 (time-based, matches `uuid1()`) | `uuid` v4 (random) | v1 = byte-closer to Python; v4 = simpler, still valid (save isn't byte-preserving anyway) — discretion |
| Reuse `DeletePreviewDialog` | New `EditPreviewDialog` | Rename/generalize the existing one; it already renders `DryRunReport` — no reason to fork |

**Installation:** None. `cargo build` uses existing manifest.

## Package Legitimacy Audit

> No external packages are introduced. Every crate/module used already ships in the repo's `Cargo.toml`/`package.json` from Phases 1-6.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none added) | — | — | — | — | — | No new dependencies |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none
**If `fancy-regex` is later required:** treat as a new dependency — run `cargo add fancy-regex` only after a `checkpoint:human-verify` (crates.io legitimacy + it is a well-known, widely-used crate). Prefer the no-dep workaround.

## Architecture Patterns

### System Architecture Diagram

```
  User selects rows (CategoryList, PK = category identity key from Phase 6)
        │  op chosen from contextual bar (operations.ts — LIVE flag now true)
        ▼
  Frontend edit dialog  ──(1) invoke <op>_dry_run(selection, params)──►  Tauri command (lib.rs)
        │                                                                     │  lock SessionState, open conn
        │                                                                     ▼
        │                                          PragmaGuard::new(conn)  →  FK-off (where needed)
        │                                                                     │
        │                                          unchecked_transaction (NEVER committed)
        │                                                     │
        │                                          before = snapshot_tables(tx, affected)
        │                                          apply_<op>(tx, selection, params)   ← REAL mutation
        │                                          trim_sweep(tx)  (reflect on-save orphan cleanup)
        │                                          after  = snapshot_tables(tx, affected)
        │                                          drop(tx) ⇒ ROLLBACK ;  drop(guard) ⇒ pragmas restored
        │                                                     │
        ◄──────────── DryRunReport {added, overwritten, deleted} ────────────┘
        │
  EditPreviewDialog renders the semantic diff  →  user Confirms / Cancels
        │  (2) invoke <op>_apply(selection, params)
        ▼
  Tauri command: PragmaGuard + unchecked_transaction → apply_<op>(tx) → tx.commit()
        │  session.dirty = true
        ▼
  Working copy (userData.db in TMP) mutated in place  →  refresh CategoryList
        │  (save pipeline unchanged: trim_db + VACUUM + zip on user Save — Phase 1)
        ▼
  On corruption predicate (range-merge / composite-key): every branch inside the rolled-back
  preview, so the user sees the exact row deltas BEFORE any commit.
```

The diagram traces the primary use case (select → preview → confirm → mutate) for any of the six ops; only `apply_<op>` and the affected-table set differ per op.

### Recommended Project Structure
```
app/src-tauri/src/db/
├── delete.rs          # EXISTING — generalize its primitives (NonEmpty*, snapshot_*, diff_*, dry_run envelope)
├── edit.rs (new)      # shared: NonEmptyIds<T> generalization, affected-table snapshot helpers incl. composite-key
├── color.rs (new)     # set_color port: recolor + Note→UserMark synth + Grey no-op
├── highlights.rs (new)# merge_block_ranges primitive (shared w/ Phase 8 import)
├── tags.rs (new)      # tag_notes port (add/remove/rename) + get_available_ids recycling
├── reorder.rs (new)   # sort_notes two-pass negative-position
├── favorites.rs (new) # add_favorite mark + TagMap unmark
├── scrub.rs (new)     # clean_items + obscure_items
├── record_edit.rs(new)# update_notes/update_annotations field-constrained editor
└── (browse.rs, labels.rs, trim.rs, pragma_guard.rs, notes.rs — existing)

app/src/components/
├── EditPreviewDialog.tsx   # rename/generalize DeletePreviewDialog (renders DryRunReport)
├── ColorMenu.tsx, TagDialog.tsx, FavoriteAddDialog.tsx, RecordEditor.tsx (new)
└── (CategoryList.tsx, operations.ts — flip LIVE entries)
```
(One-file-per-op vs a `db/edit/` submodule is discretion — D7 Claude's Discretion.)

### Pattern 1: The generalized dry_run/apply envelope
**What:** Every op is a pair `apply_<op>(tx, sel, params) -> Result<_, ArchiveError>` (runs in caller's tx, parameterized) and `dry_run_<op>(conn, sel, params) -> Result<DryRunReport, ArchiveError>` (PragmaGuard + never-committed tx + snapshot/diff).
**When to use:** All six op groups + the per-category deletes.
**Example:**
```rust
// Source: app/src-tauri/src/db/delete.rs:223-259 (the template to clone per op)
pub fn dry_run_<op>(conn: &mut Connection, sel: &NonEmpty<Cat>Ids, params: <P>)
    -> Result<DryRunReport, ArchiveError> {
    let guard = PragmaGuard::new(conn)?;              // pragmas restored on drop
    conn.execute_batch("PRAGMA foreign_keys = 'OFF'; ...")?; // where FK-off needed
    let tx = conn.unchecked_transaction()?;           // &self — guard holds shared borrow
    let before = snapshot_tables(&tx, AFFECTED_TABLES)?;
    apply_<op>(&tx, sel, params)?;                     // REAL mutation
    trim_sweep(&tx)?;                                  // reflect on-save orphan sweep
    let after  = snapshot_tables(&tx, AFFECTED_TABLES)?;
    let report = diff_snapshots(&before, &after);
    drop(tx);    // auto ROLLBACK — nothing persisted
    drop(guard); // pragmas restored (rollback does NOT restore pragmas)
    Ok(report)
}
```

### Pattern 2: Empty-selection-unrepresentable per category
**What:** Generalize `NonEmptyNoteIds` so each category's selection is a typed non-empty wrapper failing at IPC deserialization.
**Example:**
```rust
// Source: app/src-tauri/src/db/delete.rs:54-69 (generalize to NonEmptyIds<Marker>)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
pub struct NonEmptyBlockRangeIds(Vec<i64>); // + Favorites: TagMapId, Bookmarks: BookmarkId, etc.
// TryFrom rejects [] — an empty selection cannot reach a command body.
```

### Anti-Patterns to Avoid
- **String-mangled `IN (...)`** (`str(list).replace('[','(')`, Python wart #20) — use `params_from_iter` + generated placeholders (`delete.rs:206-210`). Never interpolate values.
- **Merge-on-recolor without a decision** — the Python does not do it (D7-03). Do not add it silently.
- **Treating the raw editor as free-form SQL** — it is 3 fields (Notes) + 1 (Annotations); model it as typed per-record UPDATE (D7-09).
- **`snapshot_pks` on a composite-key table** — it assumes one `i64` PK; `InputField` has none. Use a synthetic-key snapshot (D7-11).
- **`sys.exit()`/`crash_box` on error** — port to typed `ArchiveError` → `ErrorDto`.
- **Byte-diffing to verify** — save trims+VACUUMs; mask is random; timestamps overwritten. Semantic-only.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Preview of a mutation | Custom row-count logic | `dry_run_*` + `snapshot_tables`/`diff_snapshots` (`delete.rs`) | Semantic before/after already handles re-densify/overwrite vs. delete correctly |
| Empty-selection guard | Runtime `if sel.is_empty()` checks | `#[serde(try_from)]` wrapper (`NonEmptyNoteIds`) | Unrepresentable at IPC, not merely checked |
| FK-off safety | Manual pragma save/restore | `PragmaGuard` | PRAGMAs survive rollback; RAII restore covers every exit path |
| Orphan cleanup after edit | New GC pass | `trim_sweep` (VACUUM-free) | Already implements rules #14-#19 (re-densify, GC Type>0 tags, Location.Title="") |
| Confirm/preview UI | New dialog | `DeletePreviewDialog` → `EditPreviewDialog` | Already renders `DryRunReport` |
| Color index→name | Lookup table | `labels.rs::process_color` + `COLOR_NAMES` | Already ported/tested |
| ID gap-filling | Autoincrement | Port `get_available_ids` (§3.6) | Byte-comparable output; naive autoincrement drifts |

**Key insight:** The corruption-proofing (rolled-back preview, semantic diff, PragmaGuard, empty-selection type) is the hard, already-solved part. Phase 7's real engineering is the two genuinely-new algorithms — the geometric range merge and the constraint-safe two-pass reorder — plus faithful Unicode scrubbing. Everything else is wiring the existing spine to new SQL.

## The Six Edit Operations (implementation-ready detail)

### Op 1 — Highlight color change + range union-merge (EDIT-02) — HIGHEST RISK

**1a. Recolor (`set_color`, `JWLManager.py:3237-3278`):**
```
Highlights branch (:3241):  UserMarkIds ← SELECT UserMarkId FROM BlockRange WHERE BlockRangeId IN {selection}
Notes branch    (:3243-46): for each Note w/ LocationId AND UserMarkId IS NULL in selection:
                              INSERT UserMark(ColorIndex=<color>, LocationId, StyleIndex=0,
                                              UserMarkGuid=<uuid1>, Version=1) ; Note.UserMarkId ← new id
                            then UserMarkIds ← SELECT UserMarkId FROM Note WHERE UserMarkId NOT NULL AND NoteId IN {sel}
Both (:3251):               UPDATE UserMark SET ColorIndex=<color> WHERE UserMarkId IN {UserMarkIds}
Guard (:3255-56):           if category==Highlights and color==0 (Grey): return  (silent no-op, rule #11)
```
- Ported dependency: `process_color`/`COLOR_NAMES` (`labels.rs:37,73`) give the 7-color palette (0 Grey … 6 Purple).
- Affected tables for `DryRunReport`: `UserMark` (overwritten = recolored; added = synthesized), `Note` (overwritten = UserMarkId set), + `trim_sweep` fallout.
- Corruption vectors: (i) Note→UserMark synthesis silently upgrades a plain note to highlighted (rule #12) — preview must surface `UserMark` added; (ii) resolving UserMarkId from BlockRangeId must key on the exact selected BlockRanges only.

**1b. Range union-merge (`add_usermark`, `JWLManager.py:2160-2184`):**
```
rows ← SELECT * FROM BlockRange JOIN UserMark USING(UserMarkId)
        WHERE Identifier=? AND LocationId=?          -- grouping key; NOTE: not filtered by color
ns,ne ← new range StartToken,EndToken
for row in rows:  cs,ce = row.StartToken,row.EndToken
    if ce >= ns and ne >= cs:                        -- OVERLAP TEST (inclusive-token)
        ns = min(cs, ns); ne = max(ce, ne)           -- expand union
        absorb row.BlockRangeId
DELETE FROM BlockRange WHERE BlockRangeId IN {absorbed}
INSERT BlockRange(BlockType, Identifier, StartToken=ns, EndToken=ne, UserMarkId=<new>)
```
- ⚠️ **This is the single most dangerous operation in the milestone**: it DELETEs BlockRange rows on a geometric predicate. Extract as a standalone `merge_block_ranges(tx, identifier, location_id, ns, ne) -> merged range` primitive, exhaustively unit-tested (no overlap, touching endpoints `ce==ns`, containment, chain-merge of 3+ ranges, cross-UserMark grouping).
- ⚠️ **Criterion mismatch (must resolve before coding — recommend `checkpoint:human-verify`):** ROADMAP criterion 1 says recolor union-merges "as the Python does," but `set_color` does NOT call `add_usermark` — the Python only merges on IMPORT. Two defensible resolutions: (A) strict parity — recolor does not merge; the criterion is met by the tested `merge_block_ranges` primitive existing (and used by Phase 8). (B) deliberate extension — recolor invokes `merge_block_ranges` when a recolor produces overlaps. Do not pick silently.
- Affected tables: `BlockRange` (deleted = absorbed; added = merged), `UserMark` (trim fallout for now-orphaned marks).

### Op 2 — Tags: add/remove/rename + reorder (EDIT-03, EDIT-04)

**2a. Tag add/remove/rename (`tag_notes`, `JWLManager.py:3281-3386`):**
```
Tri-state (:3287-98): per Tag WHERE Type=1, count how many SELECTED notes carry it (SUM CASE WHEN tm.NoteId IN {sel})
delete_tags (:3317-31): for tags user set to count 0 → DELETE FROM TagMap WHERE NoteId=? AND TagId=? (per note)
add_tags   (:3333-61):  for tags user set to count != 0:
    if new tag: reuse Tag WHERE Type=1 AND Name=? else INSERT Tag(Type=1, Name=?) (recycle TagId via get_available_ids)
    pos ← ifnull(max(Position),-1)+1  FROM TagMap WHERE TagId=?
    INSERT OR IGNORE INTO TagMap(TagMapId?, NoteId, TagId, Position)   -- recycle TagMapId; OR IGNORE guards dup
```
- Rename = create-new-tag + old tag GC'd on save when unused (rule #16, Type>0 only; the Type=0 Favorite tag is never GC'd).
- ID recycling: port `get_available_ids` gap-fill over `{TagMap, Tag}` (§3.6) for byte-comparable output.
- `Tag Type` taxonomy: 0=Favorite(system), 1=note tag, 2=playlist (§3.4) — filter `Type=1` exactly.
- Composite hazard: `TagMap` effectively `UNIQUE(TagId, NoteId)` (the `INSERT OR IGNORE` target) AND `UNIQUE(TagId, Position)`.

**2b. Reorder (`sort_notes`, `JWLManager.py:3825-3855`) — THE load-bearing correctness item:**
```
for tag_id in Tag WHERE Type=1:
    pos = 1
    for tm in TagMap WHERE TagId=? ORDER BY NoteId:      -- PASS 1: write negatives
        UPDATE TagMap SET Position = -pos WHERE TagMapId=tm ;  pos += 1     -- → -1,-2,-3,...
    for tm,p in TagMap WHERE TagId=?:                    -- PASS 2: flip to dense 0-based
        UPDATE TagMap SET Position = abs(p)-1 WHERE TagMapId=tm             -- -1→0, -2→1, ...
```
- **Why two passes / why negatives:** `TagMap` has `UNIQUE(TagId, Position)`. A single-pass rewrite (`Position := new`) collides with a not-yet-rewritten row that still holds `new`. Writing to the disjoint negative namespace first guarantees no collision with any existing non-negative position; pass 2 maps the now-unique negatives to the final unique 0-based values. This is exactly the class of composite-key hazard Phase 4 hit.
- Faithful port keeps the negative-sentinel two-pass (minimal diff). An equivalent temp-table/offset rewrite is acceptable but must be provably collision-free. Assert final = 0-based dense per tag, no UNIQUE error.
- Note: `trim_db` on save re-densifies positions via `ROW_NUMBER() OVER (PARTITION BY TagId ORDER BY Position, TagMapId) - 1` (rule #14) — reorder sets the ORDER (by NoteId), save enforces density.

### Op 3 — Favorites mark/unmark (EDIT-05)

**Mark (`add_favorite`, `JWLManager.py:3391-3460`):**
```
tag_positions (:3435-41): INSERT Tag(Type=0,Name='Favorite') WHERE NOT EXISTS(... Type=0 ... 'Favorite')
                          tag_id ← Tag WHERE Type=0 ;  pos ← ifnull(max(Position)+1, 0) FROM TagMap WHERE TagId=?
add_location  (:3444):    INSERT Location(IssueTagNumber=0,KeySymbol,MepsLanguage,Type=1) WHERE NOT EXISTS(...); return LocationId
dup-check     (:3455-57): if TagMap WHERE LocationId=? AND TagId=(Favorite): reject "already exists"
insert        (:3459):    INSERT TagMap(LocationId, TagId, Position)
```
- Edition/language list ← bundled `favorites` table (resources.db) filtered by language (`:3395-3399`). Only metadata (edition symbol/language) — no publication body.
- **Unmark** = `DELETE FROM TagMap WHERE TagMapId IN {sel}` (`:3662`). Favorite identity = `TagMapId` (§3.3). System Favorite tag (Type 0) never GC'd (rule #16).
- The literal `Name='Favorite'` + `Type=0` is load-bearing (§3.4).

### Op 4 — Clean / Mask (EDIT-06)

**Clean (`clean_items`, `JWLManager.py:3698-3748`):**
```
spaces   = [\p{Zs}--\x20] → ' '     joiners = [\p{Zl}\p{Zp}] → ''     '\r' → '\n'
clean_annotations (:3705-11): UPDATE InputField SET Value=clean(Value) WHERE TextTag=?  (rows matching `combined`)
clean_notes       (:3713-23): UPDATE Note SET Title=clean(Title),Content=clean(Content) WHERE NoteId=?
combined = [[\p{Zl}\p{Zp}\p{Zs}]--[\x20]]   -- only rows matching are touched; count = rows, not replacements
```
- ⚠️ Rust `regex` supports `\p{Zs}`/`\p{Zl}`/`\p{Zp}` but NOT `regex.V1` set-subtraction `--`. Workaround: match `\p{Zs}` then special-case ASCII `\x20` in code (a `\p{Zs}` char that is not `' '` → replace), OR `fancy-regex` (last resort). Verify against a Unicode-separator fixture (NBSP U+00A0, line-sep U+2028, para-sep U+2029, thin-space U+2009).
- Deterministic, whitespace-normalizing → low risk. Whole-archive (no selection).

**Mask (`obscure_items`, `JWLManager.py:3750-3823`) — HIGHEST IRREVERSIBILITY:**
```
words = ['obscured','yada','bla','gibberish','børk'] ; m = \p{L}
obscure_text (:3752-68): every \p{L} char → next letter cycled from a randomly-chosen word,
                         preserving case (:3759-62), non-letters, and total length
Applied to (:3810-13): InputField.Value ; Bookmark.Title+Snippet ; Note.Title+Content ; Location.Title
Always marks modified (:3823)
```
- Whole-archive, no selection, RANDOM, IRREVERSIBLE. Reuse `DryRunReport` to preview row counts about to be masked; require an explicit typed confirm stronger than Python's one Yes/No (D7-08). Seed RNG in tests. Round-trip test asserts SHAPE invariants (length/case/non-letter positions preserved, letters masked), never exact bytes.
- Touches only user-authored + Location.Title metadata — never publication body (constraint satisfied).

### Op 5 — Raw record editor (EDIT-07) — field-constrained, NOT arbitrary SQL

**Write-back (`update_notes`/`update_annotations`, `JWLManager.py:2833-2876`):**
```
Notes (:2835-49):  per modified item:
   if not independent: resolve/ synthesize UserMark (same as set_color, :2840-45), then
     UPDATE Note SET Title=?, Content=?, LastModified=<now UTC>, UserMarkId=? WHERE NoteId=?
   if independent:    UPDATE Note SET Title=?, Content=?, LastModified=<now> WHERE NoteId=?
   deleted items:     DELETE FROM Note WHERE NoteId=?
Annotations (:2851-55): UPDATE InputField SET Value=? WHERE LocationId=? AND TextTag=?
   deleted items:       DELETE FROM InputField WHERE LocationId=? AND TextTag=?
Enabled for Notes + Annotations ONLY (§1.14)
```
- Editable surface is exactly: Notes = {Title, Content, ColorIndex(→UserMark, synth if absent)}; Annotations = {Value}. Plus single-item delete. Model as a typed per-record command; never accept table/column names or SQL from the frontend. `LastModified` auto-stamped (matches Python; breaks byte-parity, fine under semantic parity).
- Composite key: Annotation record keyed by `(LocationId, TextTag)` — no integer PK (D7-11).

## Runtime State Inventory

> Not a rename/refactor/migration phase. This section is N/A. Phase 7 mutates archive DB content only; there is no stored external state, live service config, OS registration, secret, or build artifact that embeds a renamed identifier. **None — verified: Phase 7 is in-repo Rust + frontend feature work with no deployment/OS/service surface.**

## Common Pitfalls

### Pitfall 1: Implementing merge-on-recolor because the criterion says so
**What goes wrong:** You read ROADMAP criterion 1 literally, wire `merge_block_ranges` into recolor, and now recolor destroys/merges BlockRanges the Python never touched — a parity break that silently loses highlight geometry.
**Why it happens:** The criterion conflates the import-path merge with recolor.
**How to avoid:** Resolve via checkpoint first (D7-03). Default to strict parity (recolor = UPDATE ColorIndex only) unless a deliberate extension is chosen.
**Warning signs:** A recolor test where two adjacent highlights collapse into one.

### Pitfall 2: Single-pass reorder → UNIQUE(TagId,Position) violation
**What goes wrong:** Rewriting `Position` in one pass collides mid-loop.
**Why it happens:** `TagMap` has composite position uniqueness; the row you're about to overwrite still holds the target value.
**How to avoid:** The two-pass negative-sentinel rewrite (Op 2b). Never disable the constraint to "make it work" — that hides the bug.
**Warning signs:** Intermittent `UNIQUE constraint failed: TagMap` under FK/constraint enforcement.

### Pitfall 3: `snapshot_pks` on `InputField` (no integer PK)
**What goes wrong:** The dry-run for clean/annotation-edit/annotation-delete tries to snapshot `InputField` by a single `i64` PK that doesn't exist.
**Why it happens:** `snapshot_pks` assumes `(table, pk_col)` with an `i64` column (`delete.rs:123`).
**How to avoid:** Add a synthetic-key snapshot helper (`LocationId || '\x1f' || TextTag`, or `rowid`) for composite-key tables (D7-11).
**Warning signs:** Diff shows `InputField` all-deleted-all-added, or a type error on `row.get::<_, i64>(0)`.

### Pitfall 4: Annotation delete over-deletes
**What goes wrong:** Deleting one annotation removes ALL InputFields at that Location.
**Why it happens:** The Python deletes by `LocationId` (rule #10, `:3669`), not by `(LocationId, TextTag)`.
**How to avoid:** Match Python behavior BUT surface it truthfully in the preview (the `DryRunReport` will show every InputField at that Location as deleted). For the raw-editor single-delete, the Python deletes by `(LocationId, TextTag)` (`:2855`) — the two delete paths differ; don't cross them.
**Warning signs:** User deletes one annotation, several vanish.

### Pitfall 5: Unicode class `--` set-subtraction assumed to work in `regex`
**What goes wrong:** `Regex::new("[\\p{Zs}--\\x20]")` fails or misbehaves; clean silently no-ops or errors.
**Why it happens:** `regex.V1` set-subtraction is a Python-`regex`-module feature, not in Rust `regex`.
**How to avoid:** Build the class explicitly (§Op 4 workaround); test against real separator codepoints.
**Warning signs:** Clean leaves NBSP/line-sep intact, or a regex-compile error at startup.

### Pitfall 6: Mask verified by exact output
**What goes wrong:** Tests flake because the mask word is chosen randomly.
**Why it happens:** `randint`-driven word choice.
**How to avoid:** Seed the RNG in tests; assert shape invariants only.

## Code Examples

### Generalized non-empty selection (per category)
```rust
// Source: app/src-tauri/src/db/delete.rs:54-85 (pattern to instantiate)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(try_from = "Vec<i64>")]
pub struct NonEmptyTagMapIds(Vec<i64>); // Favorites unmark; likewise BlockRangeId, BookmarkId, LocationId
impl TryFrom<Vec<i64>> for NonEmptyTagMapIds {
    type Error = String;
    fn try_from(v: Vec<i64>) -> Result<Self, String> {
        if v.is_empty() { Err("selection must not be empty".into()) } else { Ok(Self(v)) }
    }
}
```

### Parameterized IN-clause (never string-mangled)
```rust
// Source: app/src-tauri/src/db/delete.rs:205-212
let placeholders: String = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(",");
let sql = format!("UPDATE UserMark SET ColorIndex = ? WHERE UserMarkId IN ({placeholders})");
tx.execute(&sql, rusqlite::params_from_iter(std::iter::once(&color).chain(ids.iter())))?;
// Only the placeholder COUNT is dynamic; color + ids bound as typed params.
```

### Two-pass reorder core
```rust
// Source port of JWLManager.py:3829-3834
for tag_id in note_tag_ids(tx)? {                       // Tag WHERE Type=1
    let mut pos = 1i64;
    for tmid in tagmap_ids_ordered_by_note(tx, tag_id)? {   // ORDER BY NoteId
        tx.execute("UPDATE TagMap SET Position = ? WHERE TagMapId = ?", (-pos, tmid))?; pos += 1;
    }
    for (tmid, p) in tagmap_id_pos(tx, tag_id)? {
        tx.execute("UPDATE TagMap SET Position = ? WHERE TagMapId = ?", (p.abs() - 1, tmid))?;
    }
}
```

## State of the Art

| Old Approach (Python) | Current Approach (this rewrite) | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `str(list).replace('[','(')` inline `IN (...)` | `params_from_iter` + generated placeholders | Phase 2 | No injection, empty-`IN ()` impossible |
| Bare `except:` → `crash_box` → `sys.exit()` on edit error | Typed `ArchiveError` → `ErrorDto` → banner | Phase 1/2 | App never hard-exits; error surfaced |
| Mutate then hope | Rolled-back semantic dry-run preview | Phase 2 | User sees exact deltas before commit |
| Manual pragma restore to literals | `PragmaGuard` RAII | Phase 2 | Restores caller's actual prior pragmas |
| One Yes/No before irreversible mask | Preview + strengthened typed confirm | Phase 7 (D7-08) | Friction proportional to irreversibility |

**Deprecated/outdated:** crash-report telemetry (`ntfy.sh`) — not ported (privacy). `regex.V1` set-subtraction — not available in Rust `regex`; workaround required.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Rust `regex` lacks `--` set-subtraction; explicit-class workaround suffices for clean/mask | Op 4, Pitfall 5 | If workaround can't replicate `combined` semantics, need `fancy-regex` (new dep, checkpoint) |
| A2 | `TagMap` enforces `UNIQUE(TagId, Position)` and effectively `UNIQUE(TagId, NoteId)` in the v16 schema | D7-05, Op 2 | If the working-copy schema doesn't enforce these, the two-pass is unnecessary (but harmless); verify via `PRAGMA index_list(TagMap)` on a fixture |
| A3 | `InputField` has no single-column integer PK (composite `(LocationId,TextTag)`) | D7-11, Pitfall 3 | If it has a rowid/PK, snapshotting simplifies; verify schema |
| A4 | The bundled `favorites` table (resources.db) exists and is the edition/language source for `add_favorite` | Op 3 | If absent/different, favorites-mark UI needs another source; verify `resources.rs`/resources.db |
| A5 | UUID v1 vs v4 for synthesized UserMarks is cosmetic under semantic parity | D7-02 | Only matters if a downstream consumer requires v1 time-ordering (unlikely) |
| A6 | Per-category simple deletes belong in Phase 7 (deferred from Phase 6) despite EDIT-01 being formally Phase 2 | D7-10 | Planner may scope them out; low risk (pattern identical to Notes delete) |

**Verification commands for the planner (run on a synthetic v16 fixture, not a real archive):**
```bash
# A2/A3: confirm the actual constraints in the working schema
sqlite3 fixture.db "PRAGMA index_list('TagMap');"
sqlite3 fixture.db "PRAGMA table_info('InputField');"   # look for a pk column
sqlite3 fixture.db "PRAGMA index_list('InputField');"
```

## Open Questions

1. **Recolor union-merge (criterion 1 vs Python behavior).**
   - What we know: `set_color` does NOT merge; `add_usermark` (import) does.
   - What's unclear: whether Phase 7 recolor should merge, or the criterion is satisfied by the primitive existing + being tested.
   - Recommendation: `checkpoint:human-verify` before implementing; default to strict parity (no merge-on-recolor).

2. **Per-category delete scope.**
   - What we know: Phase 6 deferred non-Notes deletes here; EDIT-01 is formally Phase 2.
   - What's unclear: whether all five (minus Playlist media) land in Phase 7.
   - Recommendation: include Bookmark/Favorite/Highlight/Annotation deletes (trivial pattern reuse); defer Playlist delete to Phase 8.

3. **Rename semantics.**
   - What we know: Python "rename" = create-new-tag + old GC'd on save.
   - What's unclear: whether users expect an in-place `UPDATE Tag SET Name` (which would preserve TagId/positions) vs the Python's create-new.
   - Recommendation: port Python behavior (create-new) for parity; note the alternative for discuss-phase.

## Environment Availability

> Phase 7 is code + config only (Rust backend + frontend). No external tools/services/runtimes beyond the existing Tauri/Cargo/npm toolchain already required by Phases 1-6. **Step SKIPPED — no new external dependencies.**

## Validation Architecture

> `workflow.nyquist_validation` is enabled (default). Section included.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust: `cargo test` (integration tests in `app/src-tauri/tests/`); Frontend: `vitest` |
| Config file | `app/src-tauri/Cargo.toml`; `app/vitest.config.*` |
| Quick run command | `cargo test --test <op>_tests` (per op) |
| Full suite command | `cd app/src-tauri && cargo test` ; `cd app && npm run test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| EDIT-02 | Recolor updates ColorIndex + synth UserMark for plain note; Grey-on-Highlight no-op | integration | `cargo test --test color_tests` | ❌ Wave 0 (`tests/color_tests.rs`) |
| EDIT-02 | `merge_block_ranges` coalesces overlaps, leaves disjoint ranges | integration | `cargo test --test highlight_merge_tests` | ❌ Wave 0 (`tests/highlight_merge_tests.rs`) |
| EDIT-03 | Add/remove/rename tags land correct TagMap rows, recycle IDs | integration | `cargo test --test tag_tests` | ❌ Wave 0 (`tests/tag_tests.rs`) |
| EDIT-04 | Two-pass reorder → 0-based dense per tag, no UNIQUE violation | integration | `cargo test --test reorder_tests` | ❌ Wave 0 (`tests/reorder_tests.rs`) |
| EDIT-05 | Mark inserts one Location+TagMap, dup rejected; unmark removes it | integration | `cargo test --test favorites_tests` | ❌ Wave 0 (`tests/favorites_tests.rs`) |
| EDIT-06 | Clean normalizes separators (count=rows); mask preserves length/case | integration | `cargo test --test scrub_tests` | ❌ Wave 0 (`tests/scrub_tests.rs`) |
| EDIT-07 | Notes editor updates Title/Content/Color; annotation editor updates Value; single delete | integration | `cargo test --test record_edit_tests` | ❌ Wave 0 (`tests/record_edit_tests.rs`) |
| all | Semantic round-trip equivalence (never byte-diff) | integration | `cargo test --test edit_roundtrip_tests` | ❌ Wave 0 (`tests/edit_roundtrip_tests.rs`) |
| all | Edit dialogs + preview reuse + `LIVE` flip | unit (vitest) | `npm run test -- EditPreviewDialog TagDialog ColorMenu` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --test <op>_tests` for the op being built.
- **Per wave merge:** `cd app/src-tauri && cargo test` + `cd app && npm run test`.
- **Phase gate:** full suite green before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `tests/common/mod.rs` — extend the synthetic v16 fixture: overlapping BlockRanges, tagged notes across multiple Type=1 tags, a Favorite (TagMap NoteId IS NULL), InputField rows, separator/Unicode text.
- [ ] `tests/{color,highlight_merge,tag,reorder,favorites,scrub,record_edit,edit_roundtrip}_tests.rs` — per-op + round-trip.
- [ ] Frontend vitest for each edit dialog + `EditPreviewDialog` + `operations.ts` LIVE flips.
- [ ] No framework install needed (cargo + vitest already present).

## Security Domain

> `security_enforcement` enabled (absent = enabled). Included.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Local desktop app, no auth surface |
| V3 Session Management | no | No sessions |
| V4 Access Control | no | Single local user, local file |
| V5 Input Validation | yes | Typed selection wrappers (`NonEmpty*Ids`); field-constrained editor (no arbitrary SQL/columns from frontend); parameterized SQL only |
| V6 Cryptography | no | No crypto; UUID is an identifier, not a secret |
| V12 File/Resource | yes | Edits stay within the session TMP working copy; no path input from user (unlike zip-slip in open/save — not in this phase) |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| SQL injection via value interpolation | Tampering | `params_from_iter`; only placeholder COUNT dynamic (delete.rs pattern) — the Python `str(list)` mangling is the anti-pattern being killed |
| Arbitrary table/column edit via "raw editor" | Tampering / EoP | Field-constrained typed commands (D7-09); frontend can only send Title/Content/Color/Value + record identity, never SQL |
| Silent data destruction (mask/range-merge/annotation-delete) | Denial (data) | Rolled-back dry-run preview before commit; strengthened confirm for irreversible mask; every DELETE-on-predicate inside the preview envelope |
| Constraint bypass corrupting positions | Tampering | Two-pass reorder respects `UNIQUE(TagId,Position)`; never disable constraints to force a write |
| Data-loss on partial failure | Denial (data) | Single atomic transaction per apply; rollback-on-error; typed error never `sys.exit()` |

## Recommended Wave Split

| Wave | Scope | Rationale |
|------|-------|-----------|
| **W0** | Fixtures + shared `db/edit.rs` generalization (NonEmptyIds<T>, composite-key snapshot helper, `EditPreviewDialog` rename) + resolve recolor/merge checkpoint | Everything depends on the safety spine generalization + the criterion decision |
| **W1** | Highlight color + `merge_block_ranges` (color/ranges) | Highest-risk, isolate it; the merge primitive is shared with Phase 8 |
| **W2** | Tags add/remove/rename + two-pass reorder | Second correctness-critical group (composite constraints) |
| **W3** | Favorites mark/unmark + Clean + Mask | DB-only bulk/text ops; group the two scrubs with favorites |
| **W4** | Raw record editor (Notes/Annotations) + simple per-category deletes | Field-constrained; reuses color-synth + delete pattern |
| **W5** | Frontend: edit dialogs, preview wiring, flip all `operations.ts` LIVE entries, capability presentation | Integrates every backend; last so LIVE flips only after backends land |

(W1-W4 backends are largely parallelizable behind W0; W5 depends on all. Matches the task's suggested "1 wave per op-group" shape.)

## Sources

### Primary (HIGH confidence)
- `JWLManager.py` — the edit-op source of truth (exact line ranges cited per op above): `select_color`/`set_color` :3217-3278; `add_usermark` :2160-2184; `tag_notes` :3281-3386; `sort_notes` :3825-3855; `add_favorite` :3391-3460; `delete_items` :3658-3671; `clean_items` :3698-3748; `obscure_items` :3750-3823; `update_notes`/`update_annotations` :2833-2876; `get_available_ids` :1857-1869.
- `.planning/research/FUNCTIONALITY-SPEC.md` — §1.8-1.14, §3.3-3.10, §4 (business rules #9-#22).
- `app/src-tauri/src/db/delete.rs`, `pragma_guard.rs`, `trim.rs`, `browse.rs`, `labels.rs`; `lib.rs:187-272,386`; `operations.ts`; `category.rs`; `error.rs` — the reusable Rust safety infra (read in-session).
- `.planning/ROADMAP.md` Phase 7 (criteria + EDIT-02..07); `.planning/REQUIREMENTS.md`; `.planning/phases/06-full-data-browsing/06-CONTEXT.md` (deferral notes, identity PKs).

### Secondary (MEDIUM confidence)
- Rust `regex` crate Unicode-class support (no `--` set-subtraction) — assumption A1, verify at implementation.

### Tertiary (LOW confidence)
- None — no web/registry sources; this phase introduces no external dependencies.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; all infra in-repo and read this session.
- Architecture (dry_run/apply envelope): HIGH — directly generalizes shipped, tested `delete.rs`.
- Edit-op algorithms: HIGH — ported line-by-line from the Python with exact citations.
- Recolor/merge criterion: MEDIUM — a genuine spec ambiguity flagged for checkpoint.
- Unicode scrub in Rust `regex`: MEDIUM — workaround assumed, verify (A1).

**Research date:** 2026-07-23
**Valid until:** stable (in-repo source of truth; ~30 days, but effectively until the Python source or Phase 2 infra changes)
