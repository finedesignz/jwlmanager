# Phase 7: Full Editing - Context

**Gathered:** 2026-07-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 6 shipped **browse + select + surface-valid-operations** across all six categories: the user picks a category, sees real archive data, selects rows keyed by that category's identity PK, and the contextual operation bar (`operations.ts`) shows which operations *would* apply — but every operation except Notes-delete (from Phase 2) renders **deferred** (surfaced, not executable). Phase 7 makes those operations **live**: it implements every *mutating* edit operation the Python app supports, each carrying the Phase 2 safety guarantees (semantic dry-run preview, `PragmaGuard` FK handling, typed errors, atomic commit, rollback on failure).

This is a **MUTATION** phase — the destructive counterpart to Phase 6's read-side. It ports six Python edit surfaces into Rust backends + edit dialogs, and flips the corresponding `LIVE` entries in the capability descriptor:

**In scope (the six edit-op groups, EDIT-02..07):**
- **Highlight color change** (EDIT-02): recolor selected highlights; for Notes, synthesize a UserMark where none exists; the geometric **overlapping-range union-merge** primitive. Port of `set_color` (`JWLManager.py:3237-3278`) + the range-merge core of `add_usermark` (`:2160-2184`).
- **Tag add / remove / rename** (EDIT-03): tri-state tag editing over a Note selection, with `get_available_ids` gap-filling ID recycling. Port of `tag_notes` (`:3281-3386`).
- **Tag reorder** (EDIT-04): the **two-pass negative-position** rewrite that dodges `TagMap`'s `UNIQUE(TagId, Position)` constraint. Port of `sort_notes` (`:3825-3855`).
- **Favorites mark / unmark** (EDIT-05): mark = `add_favorite` (`:3391-3460`, insert Location + TagMap position); unmark = delete the `TagMap` row (`:3662`).
- **Clean / Mask** (EDIT-06): `clean_items` Unicode-separator scrub (`:3698-3748`) and `obscure_items` privacy mask (`:3750-3823`) — both archive-wide, no selection.
- **Raw data viewer / editor** (EDIT-07): the field-constrained per-record editor for Notes (Title/Content/Color) and Annotations (Value) with single-item delete. Port of `data_viewer`'s write-back `update_notes`/`update_annotations` (`:2835-2855`).

**Out of scope (own phases / deferred):**
- Import / export of any category, and the `.txt` wire formats → **Phase 8**.
- Incremental / changed-only export → **Phase 9**; N-way merge → **Phase 10**.
- `add_images` / playlist media add (`:3462-3560`) — media-file ingestion, hashing, IndependentMedia — is an **import-flavored** operation with on-disk file side-effects; recommend deferring to Phase 8 (see D7-06). Phase 7 favorites = Bible-edition favorites only.
- Duplicate-notes CTE filter, grouping/tree hierarchy, title-view modes, sort-column persistence → polish/Phase 11.
- Localization of dialog strings, theme, persisted geometry → **Phase 11**.
- Crash-report telemetry (`ntfy.sh` POST) and the Python `sys.exit()`-on-error posture are **defects, not ported** (typed errors instead).

**Requirements:** EDIT-02, EDIT-03, EDIT-04, EDIT-05, EDIT-06, EDIT-07 (ROADMAP Phase 7).

**Depends on:** Phase 2 (`db/delete.rs` — `NonEmptyNoteIds`, `DryRunReport`, `snapshot_tables`/`diff_snapshots`, `dry_run_*` rolled-back-txn pattern; `db/pragma_guard.rs`; `db/trim.rs::trim_sweep`; `DeletePreviewDialog.tsx`), Phase 6 (`db/browse.rs` per-category getters + identity PKs, `db/labels.rs::process_color`, `category.rs` `Category` enum, `CategoryList.tsx`, `operations.ts` capability descriptor, `list_category` command), Phase 1 (`db/notes.rs`, `ErrorDto`/`ErrorBanner`, `ArchiveSession`/`SessionState`, save pipeline). All complete.

</domain>

<decisions>
## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit. These are **starting positions** for planning, not locked — flag anything the planner or a cross-AI review should re-litigate (esp. D7-03, D7-08).

### The safety spine — every mutation reuses the Phase 2 pattern (all EDIT-0x, criterion 5)

- **D7-01 (one safety pattern, applied to every edit op — no exceptions):** Every edit backend follows `db/delete.rs` verbatim in shape: (1) a typed non-empty selection wrapper (`NonEmpty<Cat>Ids` via `#[serde(try_from)]`) so an empty selection fails at IPC deserialization, generalizing `NonEmptyNoteIds` (`delete.rs:54-85`); (2) an `apply_*(tx, ...)` fn that runs inside the caller's transaction with only-placeholder-count-dynamic parameterized SQL (`params_from_iter`); (3) a `dry_run_*(conn, ...)` that runs the REAL `apply_*` (+ `trim_sweep` where relevant) inside an `unchecked_transaction` that is **never committed** (auto-`ROLLBACK` on drop), wrapped in `PragmaGuard`, returning a semantic `DryRunReport` computed from BEFORE/AFTER PK-set snapshots (`snapshot_tables`/`diff_snapshots`, `delete.rs:146-194`); (4) two Tauri commands per op group (`<op>_dry_run` / `<op>_apply`) mirroring `delete_notes_dry_run`/`delete_notes_apply` (`lib.rs:187-272`) — dry-run previews, apply commits + sets `session.dirty = true`.
  `[auto] safety reuse — Q: "New per-op safety scaffolding, or generalize delete.rs?" → Selected: "Generalize the delete.rs primitives; every op is an instance of the same pattern" (recommended default)`
  **Rationale:** The Core Value is "never lose or corrupt an archive." Phase 2 already built and tested the exact machinery (rolled-back preview, semantic diff, PragmaGuard, empty-selection-unrepresentable). Six ad-hoc mutation paths would each need their own audit; one generalized pattern is audited once. `DryRunReport`/`snapshot_tables`/`diff_snapshots` are already documented as GENERAL and reused by Phase 4/5 — extend `TRACKED_TABLES` (`delete.rs:112-121`) to cover `InputField`/`Bookmark` (composite-key tables need a synthetic rowid diff — see D7-11).

### Highlight color + range union-merge (EDIT-02, criterion 1) — HIGHEST-RISK

- **D7-02 (color change = port `set_color` exactly, including its two side-effects):** `set_color` (`:3237-3278`) does three distinct things: (a) **Highlights** — resolve `UserMarkId` from the selected `BlockRangeId`s, then `UPDATE UserMark SET ColorIndex = ?` (`:3241`, `:3251`); (b) **Notes** — for any selected Note with a `LocationId` but no `UserMarkId`, **synthesize a new UserMark** (`StyleIndex 0`, fresh `uuid.uuid1()` GUID, `Version 1`, the note's `LocationId`, the chosen `ColorIndex`) and link it via `Note.UserMarkId` (`:3243-3246`) — *this turns a plain note into a highlighted one* (business rule #12); (c) **Highlights + color 0 (Grey) is a silent no-op** — early return, no message (`:3255-3256`, rule #11). Port all three faithfully. GUID generation = `uuid` crate (v1 time-based to match `uuid1()`; a random v4 is semantically valid but not byte-identical — discretion, see D7-03 note).
  **Rationale:** These warts are load-bearing for parity. The Note→UserMark synthesis is a genuine schema mutation (INSERT + UPDATE), not a simple color swap, and must be inside the dry-run/rollback envelope.

- **D7-03 (range union-merge — port `add_usermark`'s geometric merge as a reusable primitive; RESOLVE the criterion mismatch first):** ⚠️ **Critical finding:** the Python `set_color` does **NOT** union-merge ranges on recolor — it only `UPDATE`s `ColorIndex`. The overlapping-range union-merge lives exclusively in `add_usermark` (`:2160-2184`, the **import** path, FUNCTIONALITY-SPEC §3.8): before inserting a highlight, fetch all existing `BlockRange`s at the same `(Identifier, LocationId)`, and for each whose range overlaps (`ce >= ns and ne >= cs`, half-open-token overlap test), expand `ns = min(cs, ns)` / `ne = max(ce, ne)`, mark it absorbed, then `DELETE` the absorbed ranges and insert one merged range. ROADMAP criterion 1 says recolor must union-merge "exactly as the Python app does" — but the Python app does **not** merge on recolor. **The planner must resolve this before implementing** (recommend a `checkpoint:human-verify`): most-faithful reading = port the `add_usermark` merge as a standalone, tested `merge_block_ranges` primitive (it is needed anyway for Phase 8 import), and have recolor invoke it ONLY if we deliberately extend beyond Python parity; strict parity = recolor does NOT merge and the criterion is satisfied by the primitive existing + being round-trip tested. Do NOT silently implement a merge-on-recolor that the Python never did without an explicit decision.
  **Rationale:** This is the phase's single most dangerous operation: it **DELETEs BlockRange rows based on a geometric predicate**. A wrong overlap test, an off-by-one on token boundaries, or merging across the wrong grouping key silently destroys highlight geometry — undetectable without a semantic round-trip. The overlap test operates on `(Identifier, LocationId)` regardless of color (the Python does not filter by ColorIndex when merging), which is itself subtle. Flag every branch for cross-AI review.

### Tag ops (EDIT-03, EDIT-04, criterion 2)

- **D7-04 (tag add/remove/rename = port `tag_notes` tri-state + ID recycling):** `tag_notes` (`:3281-3386`) computes, per `Tag WHERE Type = 1`, how many *selected* notes carry it (`:3287-3298`) → tri-state. On confirm: `delete_tags` removes `TagMap` rows for tags the user unchecked (count → 0, `:3317-3331`); `add_tags` inserts for checked tags (`:3333-3361`), creating the `Tag` row if new (rename = create-new + the old tag GC'd on save when unused), computing `Position = ifnull(max(Position), -1) + 1` per tag (`:3351`), and recycling free IDs via `get_available_ids` gap-fill over `{TagMap, Tag}` (`:3303-3315`, FUNCTIONALITY-SPEC §3.6). `INSERT OR IGNORE` guards the `UNIQUE(TagId, NoteId)`-style duplicate (`:3354`). Preserve ID recycling for byte-comparable output (rule #22).
  **Rationale:** Tri-state + ID recycling is the exact behavior; a naive autoincrement rewrite produces valid-but-different archives. The `Tag Type` taxonomy (0=Favorite, 1=note tag, 2=playlist) is load-bearing (§3.4).

- **D7-05 (reorder = the two-pass negative-position technique, preserved or via temp namespace):** `sort_notes` (`:3825-3855`) rewrites `TagMap.Position` for every `Tag WHERE Type = 1`, ordered by `NoteId`. **Two-pass:** pass 1 writes `Position = -pos` (`pos` = 1, 2, 3…, so positions become -1, -2, -3…); pass 2 writes `Position = abs(Position) - 1` (→ 0, 1, 2…, dense 0-based). The negatives are mandatory because `TagMap` has a **uniqueness constraint on (TagId, Position)** — a single-pass rewrite collides with a not-yet-updated row mid-loop (rule #13). **This is the load-bearing correctness item of the phase** (analogous to Phase 4's determinism fix). Port the two-pass exactly, OR replace with an equivalent collision-free rewrite (e.g. a temp table / offset into a disjoint namespace) — but the negative-sentinel two-pass is the proven, minimal-diff choice.
  **Rationale:** Same class of composite-key hazard Phase 4 hit. A rewrite that ignores the two-pass gets a `UNIQUE` violation (loud) at best, or — if constraints are off — silently non-deterministic positions. Assert final positions are 0-based dense per tag in the round-trip test.

### Favorites (EDIT-05, criterion 3)

- **D7-06 (favorites mark = `add_favorite`; unmark = TagMap delete; DEFER media/playlist add):** Mark (`add_favorite`, `:3391-3460`): ensure the system `Tag (Type=0, Name='Favorite')` exists (`INSERT … WHERE NOT EXISTS`, `:3435`), find-or-insert the `Location` (`KeySymbol/MepsLanguage/IssueTagNumber=0/Type=1`, `:3444`), reject a duplicate favorite for that (edition, language) (`:3455-3457`), then `INSERT INTO TagMap (LocationId, TagId, Position)` with `Position = max(Position)+1` for the Favorite tag (`:3437-3441`, `:3459`). Unmark = `DELETE FROM TagMap WHERE TagMapId IN (...)` (`:3662`) — the Favorite's identity is `TagMapId` (§3.3); the system `Favorite` tag (Type 0) is never GC'd (rule #16). The Bible-edition list comes from a bundled `favorites` table (resources.db) filtered by language. **`add_images`/playlist media add is DEFERRED to Phase 8** — it ingests files from disk, dedups by hash/path, and writes `IndependentMedia` + on-disk files (`:3462-3560`), which is import-shaped and has file-side-effects beyond this phase's DB-only scope.
  `[auto] favorites+media scope — Q: "Include playlist media add in Phase 7?" → Selected: "Favorites (Bible-edition) yes; playlist media add deferred to Phase 8" (recommended default)`
  **Rationale:** Favorites mark/unmark is pure DB mutation and squarely EDIT-05. Playlist media add touches the filesystem and the reference-counted `IndependentMedia` model (rule #18) — a different risk surface that belongs with import.

### Clean / Mask (EDIT-06, criterion 3)

- **D7-07 (clean = port `clean_items` separator scrub, archive-wide):** `clean_items` (`:3698-3748`) strips Unicode separator junk from `InputField.Value` (keyed by `TextTag`) and `Note.Title`/`Content` (keyed by `NoteId`): `spaces = [\p{Zs}--\x20] → ' '`, `joiners = [\p{Zl}\p{Zp}] → ''`, `\r → \n` (uses `regex` crate for Unicode property + set-subtraction; Rust `regex` supports `\p{Zs}` but **not** `regex.V1` set-subtraction `--` — use `fancy-regex` or explicit char-class construction, see RESEARCH). Only rows matching the `combined` pattern are touched; count is of rows, not replacements. No selection — operates on the whole archive.
  **Rationale:** Deterministic, reversible-in-spirit (whitespace normalization), low risk. The only subtlety is faithful Unicode-class replication in Rust's `regex` (no `--` operator).

- **D7-08 (mask = port `obscure_items`; treat as IRREVERSIBLE + archive-wide → strongest guard):** `obscure_items` (`:3750-3823`) replaces **every** Unicode letter (`\p{L}`) with letters cycled from a randomly-chosen word (`['obscured','yada','bla','gibberish','børk']`), preserving case/non-letters/length, across `InputField.Value`, `Bookmark.Title`+`Snippet`, `Note.Title`+`Content`, and `Location.Title` (`:3810-3813`). **No selection; irreversible; destroys ALL user text.** This is the highest *irreversibility* risk in the phase (D7-03 is the highest *corruption* risk). Reuse the dry-run/`DryRunReport` preview so the user sees the row counts about to be masked, require an explicit typed confirm (stronger than the Python's single Yes/No), and — recommended — gate behind an extra `checkpoint`-style acknowledgement in the UI. Randomness makes it non-round-trippable byte-wise; the round-trip test asserts *shape* invariants (length preserved, letter-positions masked, non-letters untouched, case preserved) not exact output. Seed the RNG in tests for determinism.
  `[auto] mask guard — Q: "Match Python's single confirm, or strengthen?" → Selected: "Strengthen — preview counts + explicit typed confirm (irreversible, archive-wide)" (recommended default)`
  **Rationale:** "Never lose or corrupt a user's archive" makes an archive-wide irreversible text-destroyer the operation most deserving of friction. The Python's one-click Yes/No under-guards it.

### Raw data editor (EDIT-07, criterion 4)

- **D7-09 (raw editor is FIELD-CONSTRAINED, not arbitrary SQL — a per-record structured editor):** ⚠️ **Finding that lowers the feared danger surface:** despite "raw data viewer/editor," the Python editor is **not** a free-form table/SQL grid. The write-back (`update_notes`/`update_annotations`, `:2835-2855`) edits only: **Notes** → `Title`, `Content`, `ColorIndex` (via UserMark, synthesizing one if absent exactly like `set_color`, `:2840-2845`), plus `LastModified` auto-stamped; **Annotations** → `Value` only, keyed by `(LocationId, TextTag)` (`:2853`). Plus single-item delete (`:2848-2849`, `:2854-2855`). Enabled for **Notes and Annotations only** (§1.14). Implement it as a typed per-record edit command over those exact fields — never accept arbitrary column/table names or SQL from the frontend. All writes parameterized; the edit is one bounded `UPDATE` per record inside the dry-run/rollback envelope.
  **Rationale:** Modeling it as arbitrary SQL would invent a corruption vector the Python never had. The real surface is 3 fields (Notes) + 1 field (Annotations) + delete — safe when typed and parameterized. `LastModified`/`Note.LastModified` timestamp overwrite mirrors the Python (breaks byte-parity but matches behavior; round-trip test uses semantic, not byte, equivalence per project constraint).

### Cross-cutting scope + hazards

- **D7-10 (per-category delete backends — deferred here from Phase 6 — land in Phase 7):** Phase 6's D6-08 explicitly deferred Bookmark/Favorite/Highlight/Annotation/Playlist deletes to Phase 7 (only Notes-delete was live). These are simple instances of the `delete.rs` pattern: Bookmarks `DELETE FROM Bookmark WHERE BookmarkId IN`, Favorites `DELETE FROM TagMap WHERE TagMapId IN`, Highlights `DELETE FROM BlockRange WHERE BlockRangeId IN` (**not** UserMark — rule #9), Annotations `DELETE FROM InputField WHERE LocationId IN` (**by LocationId — deletes ALL InputFields at that location, not just the selected TextTag**, rule #10). **Playlists delete is the exception** — `delete_playlist_items` (`:3628-3647`) reference-counts shared media files by FilePath before deleting `IndependentMedia` rows and on-disk files (rule #18); recommend **deferring Playlist delete to Phase 8** with media handling. Note: EDIT-01 (delete) is formally a Phase 2 requirement (Notes-only); including the other categories' deletes here is a scope decision the planner should confirm — they fit the phase goal ("every edit op … across all categories") and the Phase 6 deferral note.
  `[auto] per-cat delete — Q: "In Phase 7 or later?" → Selected: "Simple deletes in Phase 7 (they were deferred from Phase 6); Playlist media delete → Phase 8" (recommended default)`

- **D7-11 (composite-key hazards — the recurring corruption class; document + guard each):** Several affected tables lack a single-column identity or carry composite uniqueness the naive rewrite trips:
  - `TagMap`: `UNIQUE(TagId, Position)` (drives D7-05 two-pass) AND effectively `UNIQUE(TagId, NoteId)` for note tags (drives `INSERT OR IGNORE`, D7-04). A Favorite is a `TagMap` row with `NoteId IS NULL`.
  - `InputField`: natural key `(LocationId, TextTag)` — no integer PK. Annotation upsert is `ON CONFLICT(LocationId, TextTag) DO UPDATE` (§3.9). For `DryRunReport` diffing, `InputField` has no single-column PK — snapshot a synthetic key (e.g. `LocationId || '\x1f' || TextTag` or `rowid`) rather than forcing it into `snapshot_pks` (which assumes an `i64` PK).
  - `Bookmark`: `UNIQUE(PublicationLocationId, Slot)` — relevant if masking/editing ever touches slots (it does not here, but the diff must not assume a bare `BookmarkId` capture is enough).
  These are the same hazard Phase 4 documented. Extend `TRACKED_TABLES` carefully; for composite/rowid tables add a parallel snapshot helper rather than misusing `snapshot_pks`.
  **Rationale:** Every silent-corruption vector in this phase routes through a composite constraint. Enumerate them so verification checks each.

- **D7-12 (semantic round-trip test per op group — criterion 5, synthetic fixtures only):** Each of the six op groups gets a round-trip semantic-equivalence test: seed a synthetic v16 fixture (extend `tests/common/mod.rs`), apply the edit, assert the resulting normalized table state matches the expected transform — **never byte-diff** (save is not byte-preserving; mask is random). Color: recolor updates ColorIndex + synthesizes UserMark for plain notes; range-merge: overlapping ranges coalesce to one, non-overlapping untouched. Tags: add/remove/rename land correct TagMap rows with recycled IDs; reorder yields 0-based dense positions with no UNIQUE violation. Favorites: mark inserts one TagMap+Location, dup rejected, unmark removes it. Clean: separators normalized, count = rows. Mask: shape invariants (length/case/non-letter preserved). Raw editor: Title/Content/Color (Notes) and Value (Annotations) updated, single delete removes the record. **Synthetic fixtures ONLY** (the `test_no_real_archive_is_tracked_in_git` bright-line guards this). Frontend vitest: each edit dialog + preview reuse + capability `LIVE` flip.

- **D7-13 (command surface — one dry_run/apply pair per op group, flip `operations.ts` LIVE):** Add `<op>_dry_run`/`<op>_apply` command pairs mirroring `delete_notes_*`; register in `generate_handler![]` (`lib.rs:386`). Frontend: flip the corresponding `(category, op)` entries from deferred → `LIVE` in `operations.ts` (`:54`), reuse `DeletePreviewDialog` (rename to a generic `EditPreviewDialog` showing `DryRunReport` added/overwritten/deleted) for the confirm-with-preview flow. New typed `ArchiveError` variants as needed (`ColorFailed`/`TagFailed`/`ReorderFailed`/`FavoriteFailed`/`CleanFailed`/`MaskFailed`/`RecordEditFailed`, or one `EditFailed{op, reason}`), each mapped through `to_dto` — never `unwrap`/`panic`/`sys.exit()`.

### Claude's Discretion
Module layout (`db/edit/` submodule per op group vs a flat `db/color.rs`/`db/tags.rs`/`db/favorites.rs`/`db/scrub.rs`/`db/record_edit.rs`; recommend one file per op group under `db/`), one `EditFailed{op,reason}` variant vs per-op variants, whether the range-merge primitive lives in its own `db/highlights.rs` (shared with Phase 8 import), UUID v1-vs-v4 for synthesized UserMarks (v1 for byte-parity, v4 acceptable since save isn't byte-preserving), exact edit-dialog component names and whether color is a swatch menu vs list, how strong the mask acknowledgement UI is (typed-confirm vs checkbox), whether `EditPreviewDialog` is a rename of `DeletePreviewDialog` or a superset, and the precise wave boundaries (see recommended split in RESEARCH).

</decisions>

<canonical_refs>
## Canonical References — downstream agents MUST read

### Python source of truth per edit-op group (port faithfully; cite in code)
- **Highlight color** — `JWLManager.py:3217-3278` (`select_color` palette + `set_color`: `colorize` closure `:3239-3252`, Highlights+Grey no-op `:3255-3256`, Note→UserMark synthesis `:3243-3246`, `UPDATE ColorIndex` `:3251`).
- **Highlight range union-merge** — `JWLManager.py:2160-2184` (`add_usermark`: overlap test `:2174`, expand `:2175-2176`, delete absorbed + insert merged `:2177-2184`). FUNCTIONALITY-SPEC §3.8.
- **Tag add/remove/rename** — `JWLManager.py:3281-3386` (`tag_notes`: `get_notes` tri-state `:3283-3301`, `get_available_ids` `:3303-3315`, `delete_tags` `:3317-3331`, `add_tags` `:3333-3361`, position `:3351`).
- **Tag reorder (two-pass)** — `JWLManager.py:3825-3855` (`sort_notes`/`reorder`: pass 1 negatives `:3829-3832`, pass 2 flip `:3833-3834`). FUNCTIONALITY-SPEC §1.13, rule #13.
- **Favorites mark/unmark** — `JWLManager.py:3391-3460` (`add_favorite`: `tag_positions` `:3434-3441`, `add_location` `:3443-3446`, dup-check `:3455-3457`, insert `:3459`); unmark `:3662` (`delete('TagMap','TagMapId')`).
- **Clean** — `JWLManager.py:3698-3748` (`clean` `:3700-3703`, `clean_annotations` `:3705-3711`, `clean_notes` `:3713-3723`, regex classes `:3730-3732`).
- **Mask** — `JWLManager.py:3750-3823` (`obscure_text` `:3752-3768`, `obscure_locations/annotations/bookmarks/notes` `:3770-3798`, words `:3805`).
- **Raw editor write-back** — `JWLManager.py:2833-2876` (`update_notes` `:2835-2849` incl. UserMark synth `:2840-2845`; `update_annotations` `:2851-2855`). Editor UI: `res/ui_extras.py` `DataViewer`. FUNCTIONALITY-SPEC §1.14.
- **Per-category delete dispatch** — `JWLManager.py:3658-3671` (identity keys per category; Annotations delete-by-LocationId `:3669`, Playlist ref-counted `:3628-3647`).
- **ID recycling** — `JWLManager.py:1857-1869` (`get_available_ids` gap-fill). FUNCTIONALITY-SPEC §3.6.

### Established Rust safety infra to REUSE (do not reinvent)
- `app/src-tauri/src/db/delete.rs` — `NonEmptyNoteIds` (`:54-85`), `DryRunReport` (`:94-101`), `TRACKED_TABLES` (`:112-121`), `snapshot_pks`/`snapshot_tables`/`snapshot_all`/`diff_snapshots` (`:123-194`), `delete_notes` parameterized (`:205-212`), `dry_run_delete_notes` rolled-back-txn (`:223-259`).
- `app/src-tauri/src/db/pragma_guard.rs` — `PragmaGuard` RAII snapshot/restore (PRAGMAs are NOT transactional; FK-off must be guarded).
- `app/src-tauri/src/db/trim.rs` — `trim_sweep` (VACUUM-free orphan sweep, reuse in dry-runs), and `trim_db` (save path) for context on the re-densify/GC rules (#14-#19).
- `app/src-tauri/src/lib.rs:187-272` — `delete_notes_dry_run`/`delete_notes_apply` command shape (session lock, conn open, PragmaGuard, unchecked_transaction, commit, `dirty=true`, `to_dto`); `:386` `generate_handler![]`.
- `app/src-tauri/src/db/browse.rs` — per-category getters + the identity PK per category (the edit dispatch key); `db/labels.rs::process_color` (`:73-79`) + `COLOR_NAMES` (`:37`).
- `app/src-tauri/src/category.rs` — `Category` enum (dispatch key, ts-rs exported).
- `app/src-tauri/src/error.rs` — `ArchiveError` variants + `to_dto`; add edit variants here.
- `app/src/lib/operations.ts` — capability descriptor; flip `LIVE` (`:54`) per newly-live (category, op).
- `app/src/components/DeletePreviewDialog.tsx` (`DryRunReport` render + confirm/cancel) + `CategoryList.tsx` (selection + toolbar) — reuse/generalize for edit previews.

### Test scaffolding
- `app/src-tauri/tests/common/mod.rs` (esp. `:540-581` multi-category v16 fixture seeding) — extend for per-op fixtures.
- `app/src-tauri/tests/delete_tests.rs`, `notes_query_tests.rs` — the per-op backend + round-trip test template.
- `app/src/components/NotesList.test.tsx` / `CommandBar.test.tsx` — frontend dialog/preview vitest patterns.

</canonical_refs>

<code_context>
## Existing Code Insights
- The entire safety spine (rolled-back semantic dry-run, `DryRunReport`, `PragmaGuard`, empty-selection-unrepresentable, `params_from_iter`, dry_run/apply command pair) is BUILT and TESTED. Phase 7 is "instantiate the pattern six more times over six SQL mutations," plus one genuinely novel algorithm (the geometric range merge) and one correctness-critical rewrite (the two-pass reorder).
- `process_color`/`COLOR_NAMES` already ported (`labels.rs`); the color palette index→name mapping is done. Only the *write* side (UPDATE ColorIndex + UserMark synthesis + range merge) is new.
- The capability descriptor (`operations.ts`) already models every (category, op) pair as deferred-vs-live — Phase 7 is largely flipping `LIVE` entries as each backend lands.
- Per-category identity PKs are already correct in `browse.rs` selection (Phase 6, D6-05) — they ARE the edit dispatch keys, verified there. Getting these right was the load-bearing Phase 6 risk precisely so Phase 7 mutations target correctly.

## Established Patterns
- Typed errors (`ErrorDto`), never `unwrap`/`panic`/`sys.exit()` (the Python's `crash_box`+`sys.exit()` on edit errors is a defect, not ported).
- All SQL parameterized via `params_from_iter`; only the placeholder COUNT is dynamic, never interpolated values (kills the Python `str(list).replace('[','(')` `IN (...)` string-mangling, wart #20).
- Semantic parity, never byte-diff (save trims+VACUUMs; mask is random; timestamps overwritten).
- Synthetic fixtures only; a git-tracked real archive fails the build.
- `PragmaGuard` around any FK-off region (PRAGMAs survive rollback).

## Integration Point / risk
- **Highest data-integrity risk: highlight recolor + range union-merge (D7-02/D7-03).** It DELETEs BlockRange rows on a geometric predicate and can synthesize UserMarks — silent, geometry-destroying, and carries an unresolved ROADMAP-criterion-vs-Python-behavior mismatch (Python does NOT merge on recolor). Resolve the mismatch (checkpoint) before coding; keep the merge inside dry-run/rollback; round-trip test every overlap case.
- **Highest irreversibility risk: mask (D7-08).** Archive-wide, random, destroys all text, no selection. Strengthen the guard beyond the Python's one-click.
- **Load-bearing correctness: two-pass reorder (D7-05).** `TagMap UNIQUE(TagId,Position)` — the negative-sentinel two-pass is mandatory; a single-pass rewrite collides.
- **Composite-key diffing:** `InputField` (`(LocationId,TextTag)`, no int PK) and `TagMap`/`Bookmark` composite constraints need care in `DryRunReport` snapshotting (D7-11) — `snapshot_pks` assumes a single `i64` PK.
- **Annotation delete scope wart:** deleting an Annotation deletes ALL InputFields at that `LocationId` (rule #10), not just the selected TextTag — preview must show this.

</code_context>

<specifics>
## Specific Ideas
- Keep the native jwlCore lib entirely OUT of this phase (as in Phase 6) — every edit op is pure Rust `rusqlite` SQL. No FFI, no merge.
- Extract a `merge_block_ranges(tx, identifier, location_id, ns, ne, ...)` primitive shared by the recolor path (if we choose to merge) AND Phase 8 import — write it once, test it exhaustively, since it is the single most dangerous piece of code in the milestone.
- Reuse `trim_sweep` inside dry-runs so previews reflect the post-save orphan cleanup, exactly as `dry_run_delete_notes` does — a recolor that leaves an orphaned UserMark, or an annotation-delete that orphans a Location, should preview truthfully.
- For clean/mask's Unicode classes: Rust `regex` supports `\p{Zs}`/`\p{Zl}`/`\p{Zp}`/`\p{L}` but NOT the `regex.V1` set-subtraction `[\p{Zs}--\x20]`; build the class as `\p{Zs}` then special-case the ASCII space, or use `fancy-regex`. Verify against the exact Python semantics with a Unicode-separator fixture.

## Constraints in force (project)
- Parameterize ALL SQL (no f-string/format-string interpolation of values).
- Typed errors, never crash/swallow/`sys.exit()`.
- Semantic parity, never byte-diff.
- NO publication body text — clean/mask/edit touch only user-authored fields (Note Title/Content, InputField Value, Bookmark Title/Snippet, Location Title) and metadata; never publication content.
- Every mutation runs the Phase 2 safety pattern (dry-run preview, PragmaGuard FK handling, atomic, rollback).
- Synthetic fixtures ONLY.
- MIT — jwlCore binary only; no jwlFusion/`NOASSERTION` source ingested.
- This phase is DESTRUCTIVE — flag every corruption vector for the eventual cross-AI review (`/qc` / `gsd-review`).

</specifics>

<deferred>
## Deferred Ideas
- Playlist media add (`add_images`) + Playlist delete (ref-counted `IndependentMedia`/on-disk files) → Phase 8.
- Import/export of any category, `.txt` wire formats → Phase 8; incremental export → Phase 9; N-way merge → Phase 10.
- Duplicate-notes CTE filter, grouping/tree hierarchy, title-view modes, sort persistence → polish/Phase 11.
- Localized dialog strings, theme, persisted geometry, last-category → Phase 11.
- Crash-report telemetry — intentionally NOT ported (privacy + typed-error posture).
</deferred>

---

*Phase: 7-Full Editing*
*Context gathered: 2026-07-23*
