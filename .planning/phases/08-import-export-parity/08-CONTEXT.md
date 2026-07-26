# Phase 8: Import / Export Parity - Context

**Gathered:** 2026-07-26 (autonomous mode — no user gating; all gray-area calls marked `[auto]` with rationale)
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 7 shipped every live *mutation* operation (recolor, tags, favorites, clean/mask, raw
editor) inside the Phase 2 safety spine, all against records already resident in the archive.
Phase 8 is the interchange boundary: bytes written by the Python app (or another JWL Manager
user) must import cleanly into this app's schema, and this app's exports must import cleanly
back into the Python app. This is a **MUTATION phase with two extra risk dimensions** beyond
Phase 7: (1) it parses **untrusted external input** (a `.txt`/`.jwlplaylist` file, not data
already inside the archive) and (2) two of its requirements have **on-disk file side-effects**
(playlist media add/delete), a class of hazard none of Phases 1-7 touched.

**In scope (IO-01, IO-02, IO-03):**
- **Export** for all 5 non-Playlist txt categories — Annotations, Bookmarks, Favorites,
  Highlights, Notes — preserving every wire wart bit-for-bit (`export_items`,
  `JWLManager.py:1307-1727`).
- **Import** for the same 5 categories, including per-category location/dedup logic and
  `get_available_ids` ID-gap recycling (`import_items`, `:1855-2438`).
- **Playlist export/import** as a self-contained `.jwlplaylist` (SQLite-in-zip) file
  (`export_playlist` `:1783-1854`; import side around `:2570-2600`) — included here because it
  shares the wire-format work of this phase even though its risk profile (whole-DB copy, not
  row-format parsing) differs from the other five.
- **Playlist media add** (`add_images`, `:3462-3600`) — deferred from Phase 7 (D7-06). Ingests
  files from disk, hashes (SHA-256) for dedup, writes `IndependentMedia` rows + copies files +
  generates a 250x250 thumbnail via Pillow.
- **Playlist delete's media reference-counting** (`delete_playlist_items`, `:3627-3660`) —
  deferred from Phase 7 (D7-10). Reference-counts shared media (by `FilePath`, separately for
  thumbnails and full media) across the REMAINING playlist items before deleting orphaned
  `IndependentMedia` rows and their on-disk files.
- **Highlight range union-merge wired into the import path** (IO-02/IO-03 for Highlights,
  Notes-with-RANGE): `merge_block_ranges`/`plan_merge` already ships as a tested, standalone
  primitive from Phase 7 Plan 02 (`app/src-tauri/src/db/highlights.rs`) *precisely because*
  `add_usermark` (`:2160-2184` Highlights import, `:2288-2323` Notes-with-RANGE import) needs
  it. Phase 8 is where the primitive finally gets a caller.
- **ID-gap recycling** (`get_available_ids`, `:1857-1869`) generalized as a shared Rust helper
  reused by every category's import path — one algorithm, ported once, extending Phase 7's
  category-scoped Tags version (D7-04) to the archive-wide 9-table version: `Location,
  Bookmark, UserMark, Note, BlockRange, TagMap, PlaylistItem, IndependentMedia, Tag`.

**Out of scope (own phases / genuinely deferred):**
- Incremental / changed-only export, content-hash note identity → **Phase 9** (IO-04).
- N-way merge fold → **Phase 10** (uses the *native jwlCore* merge, unrelated to this phase's
  hand-rolled per-row import).
- `.xlsx` and `.md` export/import formats (`create_xlsx`, `pl.read_excel(engine='xlsx2csv')`,
  the `# 'md'` branch in every `export_*`) — **deferred indefinitely, see D8-01**. ROADMAP
  Phase 8 success criteria only reference the `.txt` wire format; `.xlsx`/`.md` are optional
  Python conveniences, not interchange formats (a `.md` export is one-way and human-facing —
  the Python has no `.md` import path at all).
- Localization of dialog strings, theme → Phase 11 (per Phase 7's precedent).

**Requirements:** IO-01, IO-02, IO-03 (ROADMAP Phase 8; `.planning/REQUIREMENTS.md:61-63`).

**Depends on:** Phase 2 (`db/delete.rs` safety primitives — `DryRunReport`, `PragmaGuard`,
rolled-back-txn dry-run pattern), Phase 6 (`db/browse.rs` per-category getters, identity PKs,
`category.rs::Category`), Phase 7 (`db/edit.rs` shared spine, `db/highlights.rs::merge_block_ranges`
/`plan_merge`, `db/color.rs` UserMark synthesis, `guid.rs` dependency-free GUID gen, `db/tags.rs`
ID-recycling pattern, `EditPreviewDialog.tsx`), Phase 1 (`archive/extract.rs::extract_zip_slip_safe`
— zip-slip fix ALREADY SHIPPED, see D8-02; `archive/save.rs` manifest round-trip). All complete.

</domain>

<decisions>
## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit. Starting positions for
planning, not locked — flag anything the planner or cross-AI review should re-litigate
(esp. D8-03, D8-06, D8-07).

### Scope guard — what "parity" means here (criterion 1-2)

- **D8-01 (`.txt` only; `.xlsx`/`.md` explicitly deferred, not silently dropped):**
  ROADMAP Phase 8 criteria 1-2 name only the `.txt` wire format (`'None'` sentinel, `|`→`¦`
  escaping, `==={END}===`, UTF-8 header). The Python's `.xlsx` export (`create_xlsx`,
  `:1343-1358`) and `.md` export (one branch per category, e.g. `:1416-1432` Annotations,
  `:1712-1727` Notes) are one-directional human-consumption formats — the Python itself has NO
  `.md` *import* path at all, and `.xlsx` import exists only for Annotations/Notes via
  `xlsx2csv`/`polars`. Since IO-01/02/03 are framed as *interchange* requirements (files must
  flow BOTH ways between users of either app), and `.md` fundamentally cannot flow back, treat
  `.xlsx`/`.md` as out of Phase 8's interchange contract.
  `[auto] format scope — Q: "Port every Python export format, or only the round-trippable one?" -> Selected: ".txt only for Phase 8; .xlsx/.md deferred, revisit only if a future requirement explicitly asks for spreadsheet/markdown export" (recommended default)`
  **Rationale:** `.xlsx` needs a new dependency (Python uses `polars`+`XlsxWriter`/`xlsx2csv` —
  none declared in Cargo.lock per 07-RESEARCH's dependency audit pattern) for a format the
  ROADMAP criteria never mention. Building it now is scope creep against an unstated
  requirement; the `.txt` format alone satisfies "interchangeable... in both directions."

### Zip-slip — already fixed, not new work here

- **D8-02 (Playlist `.jwlplaylist` extraction reuses `extract_zip_slip_safe`, does NOT re-fix
  zip-slip):** The project brief flags zip-slip as an "explicit project security constraint
  Phase 8 touches" — true in the sense that Playlist import/export both extract a zip
  (`export_playlist`'s `ZipFile(PROJECT_PATH/'res/blank_playlist').extractall(playlist_path)`
  at `:1792`, and the mirrored import-side extraction around `:2580`), but the actual zip-slip
  FIX was already shipped in Phase 1 (`app/src-tauri/src/archive/extract.rs::extract_zip_slip_safe`,
  cited there as fixing `JWLManager.py:977-978, 1097-1099`). Phase 8 must **call the existing
  Phase 1 primitive** for both the read side (importing a `.jwlplaylist` someone sent you) and
  the write side (the blank-playlist template extraction that seeds a new export) — it does
  NOT need to design a new zip-slip guard. Any newly-authored zip open in this phase (the
  `.jwlplaylist` container) routes through `extract_zip_slip_safe`, never a raw `ZipArchive`
  loop.
  **Rationale:** Re-deriving the fix would duplicate tested code and risk a second,
  slightly-different implementation of the same security control. One implementation, reused.

### The five `.txt` categories — wart-for-wart parity (IO-01, criterion 1)

- **D8-03 (export = deterministic string-join, port verbatim per category; HIGH interchange
  risk, LOW algorithmic risk):** Each of Bookmarks/Favorites/Highlights export is a single SQL
  query whose rows are joined `'|'.join(str(x) if x is not None else 'None' for x in row)`
  (`:1445, :1461, :1477`) — the `'None'` sentinel wart. Notes/Annotations export builds a
  `{KEY=value}` bracket-tag header per record instead of a flat pipe row (Notes:
  `:1637-1667`, Annotations: `:1618`). All five write `export_header()` (`:1362-1364`, the
  `\n \n` invisible-char UTF-8-forcing line + a 76-`*` divider) as the file preamble, but only
  Annotations (`:1420`) and Notes (`:1668`) write `f.write('\n==={END}===')` —
  **Bookmarks/Favorites/Highlights do NOT write an `{END}` sentinel at all** (a flat file with
  no closing marker; only the bracket-tagged, multi-line-per-record categories need one to
  delimit the last record). Port each category's write loop exactly; do not unify the five into
  one generic writer that loses this end-sentinel asymmetry.
  **Rationale:** These are pure formatting functions over already-known SQL results — the
  Rust risk here is 100% about NOT normalizing away a wart (stray whitespace trim, wrong
  sentinel placement, `Some(x)`->`x.to_string()` where Python's `str(None)` would have produced
  literal `'None'`), not about algorithmic complexity.

- **D8-04 (import = per-category location dedup + `get_available_ids` recycling, port
  verbatim; fail-fast-whole-transaction on malformed content):**
  Every `import_*` does `pre_import()` (verify the `{CATEGORY}` tag line via `regex.search`,
  `:1873-1879` pattern repeated per category) then a per-line/per-record parse wrapped in a
  bare `except:` that shows an error dialog and does `con.execute('ROLLBACK;')` — i.e. **one
  bad record aborts the entire import transaction** (not skip-and-continue). Rust must
  replicate this fail-fast-whole-transaction behavior (a typed `ImportError{category, line,
  reason}` that causes the whole `apply_*` to return `Err`, rolling back via the existing
  `unchecked_transaction`/dry-run envelope) — NOT a per-row try/skip that silently drops
  malformed records (that would be a *worse* parity story than the Python, and would risk
  partial-import corruption). Each category's location dedup is a distinct SQL `WHERE` shape
  (scripture: `KeySymbol+MepsLanguage+BookNumber+ChapterNumber`, `:1978`/`:2138`; publication:
  `+IssueTagNumber+DocumentId`, `:1984`/`:2144`; bookmark-publication:
  `KeySymbol+MepsLanguage+Type=1+Book/Chapter/DocIsNull`, `:1990`) — port each verbatim, do not
  collapse into one generic "find or insert Location" helper that loses a category-specific
  predicate.
  `[auto] malformed-input policy — Q: "Fail the whole import on first bad record (Python parity), or skip-and-report per-record?" -> Selected: "Fail-fast whole-transaction, exactly like Python's bare except+ROLLBACK" (recommended default)`
  **Rationale:** The Core Value forbids "corrupt" as much as "lose" — a half-applied import
  (some records landed, then abort) is exactly the corrupt-partial-state the dry-run/rollback
  envelope exists to prevent. Python's own posture (aggressive rollback on any parse error) is
  already the safer choice; weakening it to skip-bad-rows would be new, unrequested leniency
  toward untrusted external file content.

- **D8-05 (highlight/note-range import wires `merge_block_ranges` — reuse, do not re-derive):**
  `add_usermark` for Highlights import (`:2160-2184`) and the Notes-with-`RANGE`-attribute
  import path (`:2288-2323`) BOTH perform the identical overlap-and-merge-then-delete-absorbed
  pattern already extracted and tested as `merge_block_ranges`/`plan_merge` in Phase 7
  (`db/highlights.rs`). Two call sites, same primitive, same `(Identifier, LocationId)` grouping
  key, same half-open-token overlap test. The Notes path additionally supports a **multi-range
  `RANGE` attribute** (`;`-separated, each optionally prefixed `identifier:`, `:2314-2317`) —
  each sub-range is a separate call into the merge primitive, sequentially, since each can
  affect what the next one's DELETE-and-insert sees.
  **Rationale:** This is the same "single most dangerous piece of code in the milestone" flagged
  in 07-CONTEXT (D7-03) — Phase 8 is where it goes live for the first time. Reusing the
  Phase-7-tested primitive rather than writing import-specific merge logic keeps the one
  DELETE-on-geometric-predicate code path singular and already-verified.

### Playlist media (deferred-from-7 scope, on-disk side effects — HIGHEST NEW RISK CLASS)

- **D8-06 (media add = SHA-256 dedup + copy + Pillow-equivalent thumbnail; new dependency
  decision required, do NOT silently add image crates):** `add_images` (`:3462-3600`) computes
  `sha256hash(f)` per selected file, skips the copy+INSERT if the hash already exists in
  `IndependentMedia` (dedup by content, not filename — `check_name`/`check_label` only
  disambiguate *display* names on a genuine new file, `:3530-3544`), copies the original into
  the archive temp dir, generates a 250x250 `Pillow`-shrunk thumbnail via `Image.thumbnail`
  (`:3579-3581`, **aspect-ratio-preserving max-bound resize**, not a crop), and inserts TWO
  `IndependentMedia` rows (original + thumbnail) plus a `PlaylistItem`+
  `PlaylistItemIndependentMediaMap`+`TagMap` row set. Rust has **no image-decoding/resizing
  dependency today** (Cargo.lock audit pattern from 07-RESEARCH finding #4 — `uuid`, `rand`,
  `fancy-regex` were all absent and hand-rolled instead). Thumbnailing is NOT hand-rollable the
  way a GUID or PRNG was (it needs real JPEG/PNG/GIF/BMP/HEIC decode) — this is a genuine new
  dependency need (likely the `image` crate) and MUST be flagged as an explicit
  package-legitimacy checkpoint in PLAN.md per project constraint, not silently added.
  SHA-256 hashing itself has no such gap — `sha2` is a reasonable minimal add, or a
  dependency-free hand-rolled SHA-256 (more code, zero supply-chain surface) is the
  hand-rolled-precedent-consistent alternative; flag both options.
  `[auto] recommend, not lock — Q: "New image dep for thumbnailing, or defer thumbnail generation?" -> Recommended: "Add the image crate behind an explicit checkpoint; thumbnailing is core to Playlist media add's contract (JW Library reads ThumbnailFilePath), skipping it produces a playlist JW Library itself won't render correctly" — planner must still run the checkpoint, not treat this as pre-approved.`
  **Rationale:** File-format decode is real complexity (magic-byte sniffing via `puremagic` in
  Python, `:3518`) that a hand-rolled Rust implementation would under-serve; unlike `uuid`/`rand`
  this is not "one function, easily hand-rolled" territory.

- **D8-07 (media delete reference-counting — port `delete_playlist_items`'s two-pass
  used-elsewhere check EXACTLY; silent-failure-on-file-missing is intentional, not a bug):**
  Deleting playlist items must NOT delete media still referenced by items outside the
  selection. The Python computes `used_thumbs`/`used_files` as the set of `ThumbnailFilePath`/
  `FilePath` values belonging to items **NOT IN** the selection (`:3628, :3638`), then only
  deletes `IndependentMedia` rows + `os.remove()`s the on-disk file for items whose media is
  NOT in that used-set (checked separately for thumbnails vs. full media, since a file can be
  one but not the other) — and Python's `os.remove()` failure is caught and silently `pass`ed
  (`:3634-3637, :3644-3647`), because a thumbnail file legitimately may not exist on disk in
  some archive states. Port this as-is: reference-count against the *remaining* set (not the
  deleted set), and treat a missing on-disk file as a non-error during delete (it's cleanup,
  not the source of truth — the DB row deletion is authoritative). This op must go through
  the standard dry-run/rollback envelope for the DB side; the file-removal side effect happens
  only in the `apply_*` (never inside `dry_run_*`, which must never touch the filesystem).
  **Rationale:** A DB-only preview that ALSO deletes files during dry-run would corrupt state
  irrecoverably if the transaction later rolls back — files aren't covered by SQLite's
  transaction/rollback guarantee. This is the phase's second major "why on-disk side effects are
  a different risk class from Phase 1-7" lesson: **dry-run must stay 100% filesystem-inert.**

### ID recycling — generalize the Phase 7 pattern, archive-wide

- **D8-08 (`get_available_ids` = one Rust helper covering all 9 tables, computed once per
  import operation, threaded through — not re-queried per record):** Python computes the full
  gap-map once per `import_items()` call (`:1857-1869`) across `{Location, Bookmark, UserMark,
  Note, BlockRange, TagMap, PlaylistItem, IndependentMedia, Tag}`, then every category's
  `add_*`/`update_db` pops from the pre-computed per-table `Vec` (largest gap first — reverse
  order, `available[::-1]`, D7-03's summary already proved-by-trace this equals ascending
  `Vec::pop()`). Port as a single `fn compute_available_ids(tx) -> HashMap<&str, Vec<i64>>`
  called once at the top of each import command, threaded by mutable reference through the
  category-specific insert helpers exactly as Python threads the shared `available_ids` dict
  via closure capture.
  **Rationale:** Recomputing the gap map per-record would be both slow (defeats the whole point
  of gap-filling being O(rows) not O(rows^2)) and semantically wrong once import starts
  consuming its own freshly-INSERTed IDs mid-transaction — a stale gap-map would risk
  double-allocating the same ID to two records within one import.

### Command surface + dry-run shape (criteria 1-3)

- **D8-09 (import gets a NEW preview shape — dry-run must show what WILL land, not a delete
  diff; reuse `EditPreviewDialog`'s scaffolding, not its default add/overwrite/delete framing
  verbatim):** Unlike every Phase 2/7 operation (which mutates existing rows the user selected),
  import creates NEW rows from external content the user has not yet seen inside the app. The
  existing `DryRunReport`/`EditPreviewDialog` (added/overwritten/deleted PK-set diff) already
  fits this shape reasonably well (new rows = "added", location/tag reuse = implicit, no
  deletes on the happy path — except Notes import's `delete_notes(title_char)` prompt,
  `:1946-1955`, which DOES delete-then-import for a chosen title-character bucket and must
  surface as a genuine "deleted" count in the preview, not silently). Export needs NO dry-run
  (it never mutates the archive) — only Import (and the Playlist-media add/delete deferred ops)
  go through the safety envelope; Export commands are plain read + file-write commands outside
  the mutation spine.
  **Rationale:** Keeps one preview component; the semantics (before/after PK-set diff) already
  generalize to "external rows landing as new PKs" without new UI machinery.

- **D8-10 (command surface — one `export_<category>`/`import_<category>_dry_run`/
  `import_<category>_apply` triple per category, per file-format; file I/O happens in Rust via
  Tauri's file dialog plugin, not the frontend):** Export: `export_<category>(session, ids?,
  path) -> Result<(), ErrorDto>` — a plain command (no dry-run) that writes the `.txt` file
  directly, taking an optional selection (Python's `items` — omitted means "export all",
  `:1327` `if not fname: return get_annotations(True)` pattern retained as "no selection =
  export everything in category"). Import: `import_<category>_dry_run(session, file_path) ->
  DryRunReport` (parses the file, computes the would-be mutation, rolls back) /
  `import_<category>_apply(session, file_path) -> Result<ImportSummary, ErrorDto>` (re-parses
  and commits — accept the double-parse cost for correctness, matching the delete/edit dry-run
  pattern's own re-run-for-real approach rather than caching parsed state between dry-run and
  apply calls, which would require session-scoped parse-result storage). File save/open dialogs
  use Tauri's `dialog` plugin from the frontend; the Rust command receives an already-resolved
  path, not a dialog callback — a structural improvement over Python's UI-coupled
  `QFileDialog` calls inline in the business logic, consistent with this project's UI/backend
  separation already established in Phases 1-7.
  **Rationale:** Mirrors `delete_notes_dry_run`/`delete_notes_apply` shape exactly (D7-01's
  "generalize the pattern" now extended to a THIRD input shape — external file — beyond
  "selection of existing rows").

### Claude's Discretion
Whether Playlist export/import lands in the same wave as the 5 txt categories or its own wave
(recommend: separate wave — different SQLite-in-zip mechanics vs. line-based parsing); exact
module layout (`db/export.rs`+`db/import.rs` vs. per-category files under `db/io/`); whether
`ImportSummary` is a new DTO or `DryRunReport` reused for the apply-return value too; the exact
`image` crate version/feature-flags if D8-06's checkpoint approves it (recommend
`image = { version = "*", default-features = false, features = ["jpeg","png","gif","bmp"] }`
to match Python's `['bmp','gif','heic','jpg','jpeg','png']` allowlist minus HEIC, which has no
mature pure-Rust decoder — flag HEIC as a possible gap vs. Python parity, likely acceptable
since HEIC playlist media is an edge case); whether SHA-256 uses a `sha2` dependency or a
hand-rolled implementation (recommend `sha2` — cryptographic hash correctness is not a good
hand-roll candidate, unlike GUID/PRNG); precise frontend dialog/menu placement for
Export/Import (Utilities menu vs. per-category toolbar, consistent with Phase 7's
`UtilitiesMenu.tsx` precedent for archive-wide ops, but Export/Import ARE selection-shaped for
the 5 txt categories so may belong back on `CategoryList.tsx`'s operation bar).

</decisions>

<canonical_refs>
## Canonical References — downstream agents MUST read

### Python source of truth per IO surface (port faithfully; cite in code)
- **Export dispatch + shared helpers** — `JWLManager.py:1307-1364` (`export_items`, `process_issue`,
  `export_file` file-dialog dispatch, `create_xlsx` [deferred, D8-01], `export_header` — the
  `\n \n` invisible-char UTF-8-forcing line + `export_header`'s divider).
- **Export: Annotations** — `:1366-1432` (`export_annotations`; txt branch `:1417-1421`, `{END}`
  sentinel `:1420`).
- **Export: Bookmarks** — `:1434-1445` (`export_bookmarks`; `|`->`¦` escaping via SQL `REPLACE`,
  `'None'` sentinel `:1445`; no `{END}` sentinel).
- **Export: Favorites** — `:1447-1461` (`export_favorites`; `'None'` sentinel `:1461`; no `{END}`).
- **Export: Highlights** — `:1463-1477` (`export_highlights`; `'None'` sentinel `:1477`; no `{END}`).
- **Export: Notes** — `:1479-1727` (`export_notes`; bracket-tag header build `:1637-1667`, `{END}`
  sentinel `:1668`; `.md`/`.xlsx` branches deferred D8-01).
- **Export: Playlists** — `:1783-1854` (`export_playlist`/`playlist_export`; whole-table copy
  into a fresh SQLite-in-zip container seeded from `res/blank_playlist`, `:1792`;
  `IndependentMedia` file copy `shutil.copy2` `:1814-1817`).
- **Import dispatch + ID recycling** — `:1855-1869` (`import_items`, `get_available_ids`
  gap-fill, D8-08).
- **Import: Annotations** — `:1871-1943` (`import_annotations`; `pre_import` tag check
  `:1873-1879`; header regex `'{(.*?)=(.*?)}'` `:1888`; record regex
  `'^===({.*?})===\n(.*?)(?=\n==={)'` `:1897`; `add_location` dedup `:1930-1939`; upsert
  `ON CONFLICT(LocationId,TextTag) DO UPDATE` `:1941`).
- **Import: Bookmarks** — `:1946-2030` (`import_bookmarks`; scripture/publication/bookmark
  location dedup `:1978, :1984, :1990`; `'None'`-string-to-`None` unwrap `:2022-2024`).
- **Import: Favorites** — `:2036-2109` (`import_favorites`; `tag_positions` system-tag
  find-or-create `:2079-2091`; dup-line-string skip `:2104` — a STRING-level dup check against
  already-exported lines, distinct from Phase 7's `(TagId,LocationId)` DB constraint).
- **Import: Highlights** — `:2124-2205` (`import_highlights`; `add_usermark` w/ range-merge
  `:2160-2184`, D8-05; line-shape guard `regex.match(r'^(\d+\|){6}', line)` `:2196`).
- **Import: Notes** — `:2209-2438` (`import_notes`; `pre_import`'s conditional bulk-delete-by-
  title-char-bucket `:2211-2229`, D8-09; header/body regex parse mirrors Annotations; `add_usermark`
  w/ multi-range `RANGE` `:2288-2323`; `update_note`/tag processing beyond `:2340`).
- **Playlist media add** — `:3462-3600` (`add_images`; dialog `:3464-3526` — UI only, not
  ported; `update_db` `:3528-3600`: SHA-256 dedup `:3568-3574`, thumbnail gen `:3576-3583`,
  D8-06).
- **Playlist item delete + media ref-counting** — `:3622-3660+` (`delete_items`/
  `delete_playlist_items`; two-pass used-elsewhere check `:3628-3647`, D8-07).
- **Zip-slip anti-pattern (ALREADY FIXED, Phase 1)** — `:977-978, :1097-1099` (original
  `extractall` sites); Playlist-specific extraction sites needing the SAME fix applied in
  Phase 8: `:1792` (export template), `~:2580` (import-side).

### Established Rust infra to REUSE (do not reinvent)
- `app/src-tauri/src/archive/extract.rs::extract_zip_slip_safe` — the zip-slip fix; Phase 8's
  ONLY new zip-open site (`.jwlplaylist`) must call this, not `ZipArchive` directly (D8-02).
- `app/src-tauri/src/db/delete.rs` — `DryRunReport`, `PragmaGuard`-wrapped `unchecked_transaction`
  dry-run pattern, `snapshot_tables`/`diff_snapshots`, `TRACKED_TABLES`.
- `app/src-tauri/src/db/edit.rs` — the Phase 7 shared safety spine (typed non-empty selection
  wrappers, `apply_*(tx)`/`dry_run_*(conn)` pairing) — Import commands follow the SAME
  dry_run/apply command-pair shape, adapted for a file-path input instead of a selection (D8-10).
- `app/src-tauri/src/db/highlights.rs` — `merge_block_ranges`/`plan_merge`, the range
  union-merge primitive Phase 7 built specifically for this phase to consume (D8-05).
- `app/src-tauri/src/db/color.rs` — UserMark synthesis pattern (Note->UserMark), same shape
  needed for Highlights/Notes import's `add_usermark`.
- `app/src-tauri/src/db/tags.rs` — Phase 7's category-scoped ID-recycling port; generalize to
  the archive-wide 9-table version for D8-08.
- `app/src-tauri/src/guid.rs` — hand-rolled dependency-free GUID generator (07-RESEARCH finding
  #4 precedent) — reuse for UserMark GUID synthesis on Highlights/Notes import.
- `app/src-tauri/src/db/scrub.rs` — hand-rolled SplitMix64 PRNG precedent, cited only as the
  "hand-roll vs. dependency" decision-pattern reference for D8-06's SHA-256/thumbnailing calls.
- `app/src-tauri/src/error.rs` — `ArchiveError` variants + `to_dto`; add `ImportError`/
  `ExportError`/`ZipSlipRejected`(exists)/`MalformedImportFile` variants here.
- `app/src/components/EditPreviewDialog.tsx` — reuse for import preview (D8-09); the
  `busyRef` double-click guard (07-RESEARCH "Mandatory frontend detail") is MANDATORY here too.
- `app/src/lib/operations.ts` — capability descriptor; Export/Import need NEW `(category, op)`
  slots, likely `Op::Export`/`Op::Import` — Export has no selection requirement (archive-wide
  default) but CAN take one, unlike Clean/Mask which are purely archive-wide (07-RESEARCH's
  "Design gap" finding about `NEEDS_SELECTION` exclusions applies here in reverse: Export is
  selection-OPTIONAL, a third state beyond the binary the descriptor currently models).

### Test scaffolding
- `app/src-tauri/tests/common/mod.rs` — extend the v16 fixture with known-gap-ID sequences per
  table so ID-recycling (D8-08) has something real to recycle into.
- `app/src-tauri/tests/edit_roundtrip_tests.rs` (Phase 7) — the semantic round-trip test
  template; Phase 8 round-trip tests must go BOTH directions (export this app's data, re-import
  it; AND parse a synthetic Python-shaped fixture file, import it, verify DB state) — never a
  real Python-produced archive (the bright-line synthetic-fixtures-only rule extends to the
  TEXT FILE fixtures too, not just `.jwlibrary` archives).
- `app/src-tauri/tests/favorites_tests.rs`, `tag_tests.rs`, `scrub_tests.rs` — per-op backend
  test template to follow per import category.

</canonical_refs>

<code_context>
## Existing Code Insights
- Phase 7 deliberately built `merge_block_ranges` as a standalone, DB-independent-testable
  primitive (`plan_merge` pure fn + thin SQL executor) SPECIFICALLY because Phase 8 needs it —
  read `07-02-SUMMARY.md`'s "provides" line before re-deriving the range-merge logic.
- The Phase 7 ID-recycling port (`db/tags.rs`) already proved Python's `available[::-1]` +
  `.pop()` reversal is equivalent to an ascending `Vec` + `Vec::pop()` (07-03-SUMMARY) — Phase
  8's archive-wide version can reuse that proof, not re-derive it.
- `07-RESEARCH.md` finding #4 (Cargo.lock audit: `uuid`/`rand`/`fancy-regex` all absent,
  hand-rolled instead) is the governing precedent for D8-06's dependency checkpoint — Phase 8
  is the first phase where a hand-roll is genuinely NOT the right call (image decoding), so the
  planner must make that case explicitly rather than defaulting to "hand-roll like Phase 7 did."
- Zip-slip is a DONE ITEM from Phase 1, not new work — the project brief's framing ("Phase 8
  touches archive extraction") is about REUSE, not a new fix (D8-02).

## Established Patterns
- Typed errors (`ErrorDto`), never `unwrap`/`panic`/`sys.exit()`.
- All SQL parameterized via `params_from_iter`; only placeholder COUNT is dynamic.
- Semantic parity, never byte-diff — for DB state.
- Synthetic fixtures ONLY — including synthetic TEXT FILE fixtures for import tests.
- `PragmaGuard` around any FK-off region.
- Dry-run must be 100% side-effect-free, including filesystem (D8-07's new addendum to this
  established pattern — Phase 1-7 dry-runs only needed to be DB-transaction-inert; Phase 8 adds
  "and never touches disk" for the media-delete path).

## Integration Point / risk
- **Highest new risk class: on-disk side effects escaping the transaction boundary
  (D8-06/D8-07).** SQLite rollback does not undo a `shutil.copy2`/`os.remove`. Any apply-path
  that both mutates the DB and touches the filesystem must sequence so a DB failure never
  leaves an orphaned file, and a file-op failure never leaves an inconsistent DB (recommend:
  perform file writes/deletes AFTER the DB transaction commits successfully, accepting that a
  post-commit file-op failure produces an orphan that a future Clean/trim pass can catch, rather
  than the reverse ordering which could leave a committed row pointing at a file that was never
  written).
- **Second-highest: fail-fast-whole-transaction on malformed import content (D8-04).** Untrusted
  external `.txt` files are new attack/corruption surface class 1-7 never faced (files inside
  the archive are, by definition, produced by an app that already validated schema on write).
- **Continuing highest: the range union-merge going live for the first time (D8-05)** — same
  DELETE-on-geometric-predicate danger flagged throughout Phase 7, now exercised by
  attacker/error-prone-controlled input (an import file) rather than only in-app-selected data.
- **`.jwlplaylist` is a full nested archive-in-archive** — Playlist export creates a NEW zip
  (seeded from `res/blank_playlist`) containing its own `userData.db`, its own `manifest.json`
  (compact-separator JSON, same wart as the main archive), and copied media files — treat it as
  structurally its own mini-archive lifecycle (extract -> mutate -> re-zip), analogous to but
  independent from the main `.jwlibrary` open/save pipeline (`archive/new.rs`/`archive/save.rs`).

</code_context>

<specifics>
## Specific Ideas
- Keep the native jwlCore lib entirely OUT of this phase (consistent with Phases 6-7) — every
  import/export op is pure Rust file I/O + `rusqlite` SQL. No FFI.
- The `'None'` sentinel, `|`->`¦` escaping, `==={END}===` sentinel, and the UTF-8-forcing header
  are the FOUR load-bearing wire warts named in the project brief; add a fifth found during this
  pass: **the ASYMMETRIC end-sentinel** (Annotations/Notes get `{END}`; Bookmarks/Favorites/
  Highlights do not) — document this explicitly so it isn't "corrected" into uniformity.
- Compact manifest JSON separators (`separators=(',', ':')`, `indent=None`) appear THREE places
  in the Python: main archive save (`:991, :1170`) and Playlist export (`:1812`) — Phase 8 only
  needs the Playlist-export instance; the main-archive instance is already Phase 1's concern
  (`archive/save.rs`) and presumably already ported — verify, don't re-port.
- Favorites import's duplicate check is STRING-level (exact `'|'.join(...)`-formatted line
  match against already-existing favorites, `:2104`) — NOT the same as Phase 7's DB-level
  `(TagId, LocationId)` UNIQUE constraint (07-RESEARCH finding #3). Both can fire in different
  circumstances; the import path should surface the Python's softer string-level pre-check
  first (silently skip an exact-duplicate LINE) but let a genuine constraint violation (e.g.
  same location, different formatting/whitespace) surface as the Phase 7-discovered hard DB
  error, not be swallowed.

## Constraints in force (project)
- Parameterize ALL SQL.
- Typed errors, never crash/swallow/`sys.exit()`.
- Semantic parity, never byte-diff for DB state — EXCEPT the wire-format bytes themselves, which
  must be byte-exact per IO-01 (this is the one place in the project where byte-for-byte
  comparison is the correct test, not the wrong one — distinguish "archive save is semantic-only"
  from "exported .txt file bytes are the contract").
- Zip-slip: reuse the existing fix, apply it to the one new zip-open site (D8-02).
- No new Cargo dependencies without an explicit legitimacy checkpoint — this phase likely NEEDS
  one (image crate, D8-06) for the first time in the milestone; do not skip the checkpoint just
  because Phase 7 always found a hand-roll.
- Media files = on-disk side effects; dry-run must remain 100% filesystem-inert (D8-07).
- Synthetic fixtures ONLY, including synthetic import-file fixtures.
- MIT — jwlCore binary only; no jwlFusion/`NOASSERTION` source ingested.
- Fail-fast whole-transaction on malformed import input (D8-04) — do not silently soften
  Python's aggressive rollback-on-error posture.

</specifics>

<deferred>
## Deferred Ideas
- `.xlsx`/`.md` export and `.xlsx` import -> deferred indefinitely (D8-01); revisit only on an
  explicit future requirement.
- Incremental/changed-only export, content-hash note identity -> Phase 9 (IO-04).
- N-way merge fold (native jwlCore multi-archive merge) -> Phase 10.
- Localized dialog strings, theme -> Phase 11.
- HEIC thumbnail support may be a genuine gap vs. Python (no mature pure-Rust decoder) —
  flag as accepted parity gap unless a future phase finds a HEIC crate worth the dependency
  cost.
</deferred>

---

*Phase: 8-Import-Export-Parity*
*Context gathered: 2026-07-26*
