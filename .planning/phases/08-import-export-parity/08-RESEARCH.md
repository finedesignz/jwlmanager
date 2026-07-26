# Phase 8: Import / Export Parity - Research

**Researched:** 2026-07-26
**Domain:** Text/SQLite wire-format interchange (5 `.txt` categories + `.jwlplaylist` SQLite-in-zip), untrusted-input parsing, on-disk media side-effects (SHA-256 dedup + thumbnailing), archive-wide ID-gap recycling.
**Confidence:** HIGH (all format facts read directly from `JWLManager.py`; Rust infra read directly from the repo; one genuine new-dependency decision flagged, not resolved here).

<user_constraints>
## User Constraints (from 08-CONTEXT.md)

### Locked Decisions (D8-01..D8-10 — see 08-CONTEXT.md for full rationale; summarized)
- **D8-01**: `.txt` only. `.xlsx`/`.md` export and `.xlsx` import are explicitly OUT of Phase 8's interchange contract (one-directional Python conveniences; `.md` has no import path at all).
- **D8-02**: Playlist `.jwlplaylist` zip open/extract MUST reuse `archive/extract.rs::extract_zip_slip_safe` (already shipped, Phase 1) — do not re-derive a zip-slip fix.
- **D8-03**: Export = deterministic string-join, ported verbatim per category. The `{END}` sentinel is written by Annotations and Notes ONLY — Bookmarks/Favorites/Highlights do NOT write one. Do not unify into one generic writer that erases this asymmetry.
- **D8-04**: Import = per-category location dedup + `get_available_ids` recycling, ported verbatim. Fail-fast whole-transaction on any malformed record (Python's bare `except:` + `ROLLBACK` — NOT skip-and-continue). Each category's location dedup predicate is distinct SQL; do not collapse into one generic "find or insert Location" helper.
- **D8-05**: Highlight/Notes-RANGE import wires the Phase-7-built `merge_block_ranges`/`plan_merge` (`db/highlights.rs`) — reuse, do not re-derive. Notes' multi-range `RANGE` attribute (`;`-separated) calls the primitive once per sub-range, sequentially.
- **D8-06**: Media add = SHA-256 dedup + copy + Pillow-equivalent thumbnail. **Thumbnailing needs a genuine new Cargo dependency (likely `image`)** — MUST go through an explicit package-legitimacy checkpoint, not be silently added. SHA-256 does NOT need a new dependency — see Finding 1 below, this supersedes 08-CONTEXT's framing of it as an open choice.
- **D8-07**: Media delete reference-counting ports `delete_playlist_items`'s two-pass used-elsewhere check exactly (reference-count against the REMAINING set, not the deleted set); `os.remove()` failure is silently ignored (intentional, not a bug). File removal happens ONLY in `apply_*`, NEVER in `dry_run_*` — dry-run must stay 100% filesystem-inert (SQLite rollback does not undo a file write/delete).
- **D8-08**: `get_available_ids` generalized to one Rust helper covering the full 9-table set (`Location, Bookmark, UserMark, Note, BlockRange, TagMap, PlaylistItem, IndependentMedia, Tag`), computed ONCE per import operation and threaded through by mutable reference — never recomputed per record.
- **D8-09**: Import needs a NEW dry-run preview shape (rows landing as NEW PKs, not a delete diff) but reuses `EditPreviewDialog`'s `DryRunReport` scaffolding. Export has no dry-run (never mutates). Notes import's conditional `delete_notes(title_char)` bulk-delete must surface as a genuine "deleted" count, not silently.
- **D8-10**: Command surface = one `export_<category>` (plain, no dry-run) + `import_<category>_dry_run`/`import_<category>_apply` pair per category. Import dry-run/apply both re-parse the file (accept double-parse cost, no cached parse-result plumbing). File dialogs live in the frontend (Tauri `dialog` plugin); Rust commands take an already-resolved path.

### Claude's Discretion
Playlist wave placement (recommend: separate wave from the 5 txt categories); module layout (`db/export.rs`+`db/import.rs` vs. per-category files); whether `ImportSummary` is a new DTO or `DryRunReport` reused; exact `image` crate version/features if the checkpoint approves it; SHA-256 dependency choice (**resolved by this research: `sha2` is already a declared, in-use dependency — no choice needed, see Finding 1**); frontend menu placement for Export/Import.

### Deferred Ideas (OUT OF SCOPE)
`.xlsx`/`.md` export and `.xlsx` import (indefinite, D8-01). Incremental/changed-only export + content-hash note identity → Phase 9 (IO-04). N-way merge fold → Phase 10. Localized dialog strings/theme → Phase 11. HEIC thumbnail support — accepted parity gap (no mature pure-Rust decoder), revisit only if a future phase finds a HEIC crate worth the cost.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| IO-01 | Export any category to Python's exact wire format (`'None'` sentinel, `\|`→`¦` escaping, `{END}` sentinel, UTF-8 header) | §Wire Formats — every category's exact byte shape cited line-by-line below |
| IO-02 | Import any category from Python-produced files, data landing correctly | §Import Formats + §ID Recycling + §Range-Merge Wiring |
| IO-03 | Imported items recycle ID gaps like Python | §ID Recycling (`get_available_ids`, D8-08) |

(Playlist export/import + media add/delete are IO-01/02/03's Playlist-category and deferred-from-7 instances per 08-CONTEXT's scope statement — not separate requirement IDs.)
</phase_requirements>

## Summary

Phase 8 is a pure-Rust, no-FFI, file-I/O-plus-`rusqlite` phase whose entire risk surface is **fidelity to byte-exact string formats** (the five `.txt` categories) and **two on-disk side-effect operations** (media add/delete) that a SQLite transaction cannot roll back. There are no new *algorithmic* problems — `merge_block_ranges` and `get_available_ids`-style recycling already exist as tested Phase 7 primitives; Phase 8's job is porting exact SQL/string shapes and wiring the primitives to a new caller (import) for the first time. The one place this phase genuinely needs new capability is **image decoding for playlist-media thumbnails** — Pillow's `Image.thumbnail` has no free Rust equivalent, and this is the phase's mandatory package-legitimacy checkpoint. SHA-256 hashing, by contrast, needs no new dependency: `sha2 = "0.11"` is already declared and already used in `archive/manifest.rs` for the archive-hash field — reuse `Sha256::digest(&bytes)` directly, port Python's `sha256hash()` (whole-file digest) exactly.

Five wire-format warts are load-bearing and must survive verbatim: (1) the `'None'` string sentinel for NULL columns in pipe-joined rows; (2) `|`→`¦` escaping applied via SQL `REPLACE()` at export time on two Bookmark text columns (Title, Snippet) — NOT applied on any other category; (3) the `==={END}===` closing sentinel, present ONLY on Annotations and Notes (bracket-tag, multi-line-per-record formats), ABSENT on Bookmarks/Favorites/Highlights (flat pipe-row formats); (4) the `\n \n` invisible-character UTF-8-forcing header line, common to all five via `export_header()`; (5) the compact-JSON-separator `manifest.json` inside `.jwlplaylist` (`separators=(',', ':')`, `indent=None`) — the SAME contract `archive/manifest.rs::to_compact_string()` already implements for the main archive, reuse it, don't re-derive.

**Primary recommendation:** Build export as pure read+string-join+file-write commands (no dry-run needed — plain `Result<(), ErrorDto>`); build import as the standard `dry_run_*`/`apply_*` pair reusing `PragmaGuard`+`unchecked_transaction`+`DryRunReport`, but with parsing happening BEFORE the transaction opens (a malformed file must fail before any DB work starts, matching Python's `pre_import`/`read_text` running before `update_db`); wire `merge_block_ranges` into both Highlights import and Notes-RANGE import as two call sites of one primitive; sequence media add/delete so DB commit happens before any filesystem write for add (a post-commit file-write failure produces an orphan row — the LESSER of the two Core-Value violations vs. a file existing that the DB never learns about), and DB commit happens AFTER filesystem delete succeeds-or-silently-fails for delete-with-cleanup, matching Python's own ordering exactly (Python does the file `os.remove` INSIDE the same open transaction as the DB delete, so the safest Rust translation is: perform the file removal attempt during `apply_*` inside the same transaction scope, but never inside `dry_run_*`).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Export (5 txt categories) | Rust `db`/`io` (SQL read + string format) | Frontend (save-dialog invocation) | Pure read, no mutation; string format fidelity is the entire risk |
| Import (5 txt categories) | Rust `db`/`io` (parse + mutation) | Frontend (open-dialog + `EditPreviewDialog`) | Untrusted external content; must sit inside the dry-run/rollback envelope |
| Playlist export/import (`.jwlplaylist`) | Rust `archive`+`db` (nested mini-archive lifecycle) | Frontend | Structurally its own extract→mutate→re-zip pipeline, reusing `extract_zip_slip_safe`/manifest patterns |
| Playlist media add (thumbnailing, hashing, copy) | Rust `db`/`io` + new `image`-crate boundary | Frontend (file picker via Tauri dialog) | On-disk side effect; needs new decode capability behind a checkpoint |
| Playlist media delete (ref-counting) | Rust `db` (DB) + filesystem (Rust `std::fs`) | — | Reference-count logic is DB-only; file cleanup is a best-effort filesystem side-effect sequenced after commit |
| ID-gap recycling | Rust `db` (pure function over a transaction snapshot) | — | Archive-wide, computed once, threaded by mutable reference |
| Preview / confirm | Frontend (`EditPreviewDialog`) | Rust (`DryRunReport`/new `ImportPreview`) | Reuse Phase 2/7 dialog; new semantics (rows landing, not deleted) |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `rusqlite` | 0.40 (bundled) [VERIFIED: repo Cargo.toml] | All SQL read/write, transactions | Already the archive DB layer |
| `sha2` | 0.11 [VERIFIED: repo Cargo.toml, already used in `archive/manifest.rs:25,153`] | Whole-file SHA-256 for media dedup (`sha256hash` port) | Already declared AND already exercised for the archive-hash manifest field — zero new dependency surface |
| `zip` | =8.6.0 (pinned, CVE-2025-29787 closed) [VERIFIED: repo Cargo.toml] | `.jwlplaylist` container read/write | Same crate as the main archive; `extract_zip_slip_safe` already wraps it for extraction |
| `serde_json` | 1, `preserve_order` [VERIFIED: repo Cargo.toml] | `.jwlplaylist`'s `manifest.json`, compact separators | `archive/manifest.rs::to_compact_string()` already implements exactly this contract — reuse the `Manifest` struct/serializer, don't reimplement |
| `regex` | 1 [VERIFIED: repo Cargo.toml] | Import line/header parsing (`{KEY=value}` extraction, `^(\d+\|){6}` line-shape guard, `{ANNOTATIONS}`/`{BOOKMARKS}`/etc. tag-line checks) | Already a dependency; Rust `regex` (non-fancy) is sufficient for these fixed patterns — none require Python `regex`-module lookahead/lookbehind beyond what a manual two-pass parse can express (see Finding 3) |

### New dependency requiring a checkpoint
| Library | Purpose | Status |
|---------|---------|--------|
| `image` (or equivalent) | Decode+resize arbitrary raster formats (bmp/gif/jpg/jpeg/png; HEIC excluded, no mature pure-Rust decoder) for the 250×250 aspect-preserving thumbnail `add_images` generates | **NOT YET APPROVED.** Package-legitimacy checkpoint required before use (see Package Legitimacy Audit below). This is the phase's one genuinely new Cargo dependency. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tauri-plugin-dialog` | 2 [VERIFIED: repo Cargo.toml] | Frontend file open/save dialogs for export path / import path / add-images multi-select | Already declared; D8-10 requires dialogs live in the frontend, commands take resolved paths |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `image` crate for thumbnailing | `imageproc` / `resvg`-adjacent crates | `image` is the de facto standard, supports all 5 non-HEIC formats Python allows in one crate; alternatives are narrower or add more surface for less coverage |
| `image` crate | Skip thumbnail generation entirely, only copy the original | Rejected per 08-CONTEXT D8-06 rationale: JW Library reads `PlaylistItem.ThumbnailFilePath` — omitting it produces a playlist JW Library itself may not render correctly. Flag as fallback ONLY if the checkpoint rejects `image` |
| Hand-rolled SHA-256 (Phase 7's `time.rs`/`scrub.rs` hand-roll precedent) | `sha2` crate | **Moot — `sha2` is already in the dependency graph and already used for this exact purpose (archive hashing).** Hand-rolling now would be *regressing* to a worse position than what's already shipped. |

**Installation (pending checkpoint approval):**
```bash
cargo add image --no-default-features --features jpeg,png,gif,bmp
```

**Version verification:** run at implementation time, not assumed from training data:
```bash
cargo search image        # or: curl -s https://crates.io/api/v1/crates/image
```

## Package Legitimacy Audit

| Package | Registry | Verdict | Disposition |
|---------|----------|---------|-------------|
| `sha2` | crates.io | Already in `Cargo.lock`, already used in-repo | Approved — no new work |
| `zip` | crates.io | Already pinned `=8.6.0` in-repo | Approved — no new work |
| `serde_json` | crates.io | Already in-repo | Approved — no new work |
| `image` | crates.io | **NOT YET CHECKED** — `[ASSUMED]` widely-used, mature crate based on training knowledge only | **Flagged [SUS]-by-policy: run `gsd-tools query package-legitimacy check --ecosystem cargo image` AND `cargo view image` (or `cargo search image` + crates.io API) before use. Planner MUST insert a `checkpoint:human-verify` task before this dependency is added, per D8-06 and per-project "no new Cargo dependency without explicit legitimacy checkpoint."** |

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious pending verification:** `image` — not a rejection, a mandatory checkpoint (the crate is extremely well-known in the Rust ecosystem from training knowledge, but this claim is `[ASSUMED]` until verified against the registry this session per the package-name-provenance rule — verification could not be completed in this research pass, no live registry/npm-equivalent tool was invoked for crates.io; the planner must run it).

## Architecture Patterns

### System Architecture Diagram — Export
```
User selects category (+ optional row selection) → clicks Export
      │  invoke export_<category>(session, ids?, path)     [plain command, NO dry-run]
      ▼
Tauri command → open conn (read-only intent) → SQL query (verbatim per-category, D8-03)
      │
      ▼
String-join rows ('|'.join with 'None' sentinel; Bookmarks additionally REPLACE '|'→'¦')
      │
      ▼
Write UTF-8 file: export_header() preamble + per-record body + ({END} sentinel IFF Annotations/Notes)
      │
      ▼
Ok(()) → frontend shows "N items exported"
```
No dry-run: export never mutates the archive (D8-09).

### System Architecture Diagram — Import
```
User picks a file (Tauri dialog, frontend) → invoke import_<category>_dry_run(session, file_path)
      │
      ▼
Rust command: READ + PARSE the file FIRST, outside any transaction
      │   - verify tag line ({ANNOTATIONS}/{BOOKMARKS}/{FAVORITES}/{HIGHLIGHTS}/{NOTES=...})
      │   - regex-split / regex-finditer per record
      │   - ANY malformed record → return ImportError{category, line/record_index, reason}
      │     BEFORE opening a transaction (fail fast, matches Python failing inside read_text()
      │     before update_db() even starts for Annotations/Notes; for Bookmarks/Favorites/
      │     Highlights, Python parses+applies per-line inside one already-open transaction —
      │     Rust may parse ALL records into a Vec first, then apply, to get the same
      │     fail-whole-transaction guarantee without a partially-applied transaction ever existing)
      ▼
PragmaGuard + unchecked_transaction (never committed in dry-run)
      │   compute_available_ids(tx)   [D8-08, once]
      │   before = snapshot_tables(tx, AFFECTED_TABLES)
      │   apply_import_<category>(tx, parsed_records, &mut available_ids)   ← REAL mutation
      │   after  = snapshot_tables(tx, AFFECTED_TABLES)
      ◄── DryRunReport / ImportPreview {added, overwritten, deleted} ──────
      │
EditPreviewDialog shows preview → user confirms → invoke import_<category>_apply(session, file_path)
      │   RE-PARSE the file (D8-10: accept double-parse cost)
      ▼
PragmaGuard + unchecked_transaction → apply_import_<category>(tx, ...) → tx.commit()
      session.dirty = true
```

### System Architecture Diagram — Playlist (`.jwlplaylist`) export
```
User selects Playlist items → invoke export_playlist(session, ids, path)
      │
      ▼
mkdtemp (Rust: tempfile crate, already a dependency) seeded from res/blank_playlist template
      │  extract_zip_slip_safe(blank_playlist_path, playlist_tmp_dir)   [D8-02: REUSE, don't re-derive]
      ▼
Open a SECOND rusqlite connection to <playlist_tmp_dir>/userData.db
      │  copy rows verbatim, table by table, in Python's exact dependency order:
      │    Tag(1 row, hardcoded Type=2 tag) → android_metadata.locale (if present)
      │    → PlaylistItem → PlaylistItemLocationMap → PlaylistItemMarker
      │    → PlaylistItemMarkerBibleVerseMap/ParagraphMap (filtered by the Marker IDs just inserted)
      │    → TagMap (hardcoded TagId=1, Position renumbered 0..n by TagId,Position order)
      │    → PlaylistItemIndependentMediaMap → PlaylistItemAccuracy (ALL rows, unfiltered)
      │    → IndependentMedia (filtered by FilePath IN thumbnails OR IndependentMediaId IN media-map)
      │       + shutil.copy2-equivalent file copy per IndependentMedia row (best-effort; a copy
      │         failure in Python surfaces a warning but does NOT abort export — port as: continue,
      │         collect a warning list, return it in the command's Ok() payload)
      │    → Location (filtered by LocationId IN PlaylistItemLocationMap's LocationId set)
      ▼
UPDATE LastModified; VACUUM; commit; close playlist connection
      │
Build manifest.json via the SAME Manifest struct/`to_compact_string()` as the main archive
      (schemaVersion: 16, hash = sha256hash(userData.db) via `Sha256::digest`)
      ▼
Zip playlist_tmp_dir → fname (ZIP_DEFLATED); rm -rf playlist_tmp_dir
```

### Recommended Project Structure
```
app/src-tauri/src/db/
├── (existing: browse.rs, edit.rs, delete.rs, color.rs, highlights.rs, tags.rs,
│    reorder.rs, favorites.rs, scrub.rs, record_edit.rs, resources.rs, trim.rs, labels.rs)
├── export.rs (new)      # 5 txt-category export queries + export_header + write loop
├── import.rs (new)      # 5 txt-category import parse+apply, OR split per-category (discretion)
├── ids.rs (new)         # compute_available_ids(tx) -> HashMap<&'static str, Vec<i64>> (D8-08)
├── playlist_io.rs (new) # export_playlist / import_playlist — the SQLite-in-zip mini-archive lifecycle
└── media.rs (new)       # add_images (hash/copy/thumbnail) + delete_playlist_items ref-counting

app/src-tauri/src/archive/
└── (existing extract.rs/manifest.rs/save.rs — Playlist reuses these, does not fork them)

app/src/components/
├── ImportPreviewDialog.tsx (new, or generalize EditPreviewDialog further)
└── (Export/Import menu entries — placement is discretion per D8-10 note)
```

## Wire Formats — exact byte-for-byte specification

All five `.txt` files: **UTF-8 encoded** (`encoding='utf-8'`), opened with Python `open(fname, 'w', encoding='utf-8')` (no BOM — Python's `utf-8` codec never writes one; Rust must not write a UTF-8 BOM either). Import additionally uses `errors='namereplace'` for Annotations only (`JWLManager.py:1939`) — malformed/undecodable bytes in an Annotations import file become `\N{...}`-style backslash-replacement text rather than a hard decode error; the other four import paths use plain `encoding='utf-8'` (a hard decode failure there is a `[SLOP if wrong]`-class fact — verify: `regex.search` against a `readline()` and a `for line in file` both require the file to already be valid UTF-8, so Bookmarks/Favorites/Highlights/Notes import DOES fail hard on invalid UTF-8; only Annotations is lenient).

### Common preamble — `export_header(category)` (`JWLManager.py:1367-1369`)
```
{category}\n \n{Exported from} {current_archive}\n{by} {APP} ({VERSION}) {on} {YYYY-MM-DD @ HH:MM:SS}\n{'*'*76}
```
- Line 1: the literal category tag string, e.g. `{ANNOTATIONS}`, `{BOOKMARKS}`, `{FAVORITES}`, `{HIGHLIGHTS}`, `{NOTES=}` (Notes' tag carries an optional title-char suffix on IMPORT, always empty `{NOTES=}` on export).
- Line 2: a single space preceded by a literal `\n ` then `\n` — the comment marks this as "invisible char on first line to force UTF-8 encoding." Concretely the header string is `category + '\n \n' + ...` — i.e. after the category tag there is a newline, a single space character, then another newline. **This single space on its own line is load-bearing** — it is what the import-side `pre_import()` reads via `readline()` as line 1 when checking for the tag (the tag-check regex searches the FIRST line only, so the exact content of line 2 does not affect re-import correctness, but omitting it changes the file's byte shape and is a parity break for IO-01's "preserves the exact wire warts" criterion).
- Line 3: `{Exported from} {archive filename or "NEW ARCHIVE"}` then newline.
- Line 4: `{by} {APP} ({VERSION}) {on} {date}` then newline.
- Line 5: 76 literal `*` characters, NO trailing newline (the next content is prefixed with its own `\n`).
- `_()` gettext wrapping applies to `Exported from`/`by`/`on` — Rust need not localize in Phase 8 (localization deferred to Phase 11 per project rules) but MUST hardcode the same English literal text as the Python's `_()` default (untranslated) to keep the header byte-identical for English-locale users, which is the baseline interchange case.

### Annotations (`export_annotations`, `:1371-1436`; `import_annotations`, `:1871-1956`)
**Export SQL** (`:1378-1392`):
```sql
SELECT TextTag, Value, l.DocumentId doc, l.IssueTagNumber, l.KeySymbol,
       CAST(TRIM(TextTag, 'abcdefghijklmnopqrstuvwxyz') AS INT) i
FROM InputField LEFT JOIN Location l USING (LocationId)
WHERE Value <> '' AND Value IS NOT NULL [AND LocationId IN {selection}]
ORDER BY doc, i;
```
Per record, file body (`:1417-1419`):
```
\n==={PUB=<KeySymbol>}[{ISSUE=<IssueTagNumber>}]{DOC=<DocumentId>}{LABEL=<TextTag>}===\n<Value.strip()>
```
- `ISSUE` bracket is present ONLY if `IssueTagNumber > 10000000` (periodical-issue heuristic — a plain 8-digit-or-fewer publication code is NOT an issue); otherwise omitted entirely (not `{ISSUE=None}`).
- `LABEL` = the raw `InputField.TextTag` (e.g. `heading001`, `note003` — whatever tag JW Library assigned).
- File terminates with literal `\n==={END}===` (`:1420`), no trailing newline after it.
- **Import** (`:1871-1956`): line 1 must match `{ANNOTATIONS}` (`regex.search`, substring match — not anchored, so extra text on the line is tolerated). Body regex: `^===({.*?})===\n(.*?)(?=\n==={)` with `regex.S | regex.M` flags (DOTALL + MULTILINE) applied to the WHOLE remaining file content read via `.read()` — i.e. records are found by lookahead to the NEXT `\n===` boundary, meaning the LAST record's body extends to end-of-string only because `{END}` itself matches the `\n==={` lookahead pattern (the `{END}` block is a synthetic final "header" the finditer regex treats as a boundary, but its own body is never captured as a real record since there's no `\n===` after it — verify: the regex's lookahead `(?=\n==={)` requires a `\n===` sequence to follow; `{END}` line format is `\n==={END}===` which DOES contain `\n===` at its start, so `{END}` correctly terminates the preceding record's capture without itself being parsed as a data record). Header attributes extracted via `regex.findall('{(.*?)=(.*?)}', line)` → dict; `VALUE` = the captured body text (NOT stripped in Python at parse time — `.strip()` happens later on `Value` when inserting: `row['VALUE'].strip()`, `:1930`). Malformed record (regex/dict-access exception) → abort with `ROLLBACK` (D8-04, fail-fast).
- **Location dedup** (`add_location`, `:1909-1919`): `WHERE DocumentId = ? AND IssueTagNumber = ? AND KeySymbol = ? AND MepsLanguage IS NULL AND Type = 0` — note `ISSUE` is filled-null-to-0 before the query (`df.with_columns(pl.col('ISSUE').fill_null(0))`, `:1922`), so a record with no `{ISSUE=...}` bracket queries/inserts `IssueTagNumber = 0`, not NULL.
- **Upsert**: `INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?, ?, ?) ON CONFLICT(LocationId, TextTag) DO UPDATE SET Value = excluded.Value` (`:1930`) — re-importing the same annotation updates in place rather than erroring or duplicating.

### Bookmarks (`export_bookmarks`, `:1438-1452`; `import_bookmarks`, `:1958-2043`)
**Export SQL** (`:1444`):
```sql
SELECT l.BookNumber, l.ChapterNumber, l.DocumentId, l.IssueTagNumber, l.KeySymbol, l.MepsLanguage,
       l.Type, Slot, REPLACE(b.Title,"|","¦"), REPLACE(Snippet,"|","¦"), BlockType, BlockIdentifier
FROM Bookmark b LEFT JOIN Location l USING (LocationId) [WHERE BookmarkId IN {selection}];
```
Row format: `'|'.join(str(x) if x is not None else 'None' for x in row)` — 12 pipe-delimited fields, **`'None'` literal string for any NULL**, written one per line prefixed with `\n` (no `{END}` sentinel — flat format).
- The `¦` (U+00A6 BROKEN BAR) substitution is applied via SQL `REPLACE()` ONLY to `Title` and `Snippet` — the two free-text fields most likely to contain a literal `|` that would otherwise be misparsed as a field delimiter on import. No other category applies this substitution because no other category exports free-text fields that could contain `|` (Highlights/Favorites export are all-numeric; Annotations/Notes use the bracket-tag format where `|` inside `VALUE`/`NOTE` is safe because the delimiter there is `\n===` not `|`).
- **Import** (`:1958-2043`): line-by-line (`for line in import_file`), only lines containing `|` are processed; split via `regex.split(r'\|', line.rstrip())` — **this does NOT reverse the `¦` substitution** (Python never un-escapes `¦`→`|` on import; the Title/Snippet text is imported WITH the literal `¦` character still in place — this is a one-way lossy transform Python itself accepts, not a bug Rust needs to "fix"). Indices `[0,1,2,9,11]` (BookNumber, ChapterNumber, DocumentId, Snippet, BlockIdentifier) get `'None'`-string→`None` unwrapped (`:2021-2023`) — note index 9 is Snippet (a text field that could legitimately never be `'None'` unless the archive had a NULL, and index 11 is BlockIdentifier). Location dedup: scripture path keys on `KeySymbol+MepsLanguage+BookNumber+ChapterNumber` (`:1971`); publication path keys on `KeySymbol+MepsLanguage+IssueTagNumber+DocumentId+Type` (`:1983`); bookmark's own publication-Location (`Type=1`) keys on `KeySymbol+MepsLanguage+Type=1+Book/Chapter/DocIsNull` (`:1995`). Bookmark identity for upsert = `(PublicationLocationId, Slot)` (`:2004`) — a bookmark landing on the same publication+slot UPDATEs in place rather than duplicating.

### Favorites (`export_favorites`, `:1454-1468`; `import_favorites`, `:2044-2123`)
**Export SQL** (`:1460`):
```sql
SELECT DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type
FROM Location JOIN TagMap USING (LocationId)
WHERE TagId = (SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite') [AND TagMapId IN {selection}]
ORDER BY Position;
```
Row format: same `'|'.join(... 'None' ...)` pattern, 6 fields, no `{END}`, `ORDER BY Position` (export preserves the user's favorite ordering).
- **Import**: dup-check is STRING-level — `line.strip() not in favorite_list` where `favorite_list` is built by re-running the EXACT SAME export query/join against the live archive and re-formatting each row identically (`get_current`, `:2072-2077`) — an exact-formatted-line match is skipped silently (not an error, not a count). This is DIFFERENT from Phase 7's `(TagId, LocationId)` UNIQUE DB constraint (07-RESEARCH Finding 3, since confirmed in 07-PATTERNS as a HARD DB error) — a line that differs only in incidental formatting (impossible here since both sides use the identical join) would still collide at the DB constraint layer if the string check somehow missed it; surface the DB constraint violation as a genuine error if it ever fires, don't swallow it.
- Location find-or-create (`add_publication_location`, `:2079-2091`) is unusual: it does `INSERT OR IGNORE` first, THEN a dynamic `WHERE` built from non-None columns using `col IS NULL` for None fields and `col = ?` otherwise — this is the one spot where Python builds a dynamic (but NOT string-interpolated-with-user-values) WHERE clause; port as a fixed 6-column predicate with `IS NULL`/`= ?` chosen per-field at Rust compile-time-known column positions, still fully parameterized.
- Favorite tag_id resolution: `SELECT TagId FROM Tag WHERE Type = 0` (system Favorite tag; find-or-create via `tag_positions()`, `:2056-2070`) — `Name='Favorite'` literal is load-bearing (D7-06 precedent, same constant).
- TagMap Position: sequential starting from `max(Position)+1` for the tag, incrementing per imported record IN FILE ORDER (`position += 1` inside the loop, `:2108`) — NOT re-sorted.

### Highlights (`export_highlights`, `:1470-1484`; `import_highlights`, `:2124-2211`)
**Export SQL** (`:1476`):
```sql
SELECT b.BlockType, b.Identifier, b.StartToken, b.EndToken, u.ColorIndex, u.Version,
       l.BookNumber, l.ChapterNumber, l.DocumentId, l.IssueTagNumber, l.KeySymbol, l.MepsLanguage, l.Type
FROM UserMark u JOIN Location l USING (LocationId), BlockRange b USING (UserMarkId)
[WHERE BlockRangeId IN {selection}];
```
Row format: same `'|'.join(... 'None' ...)` pattern, **13 fields**, no `{END}`.
- **Import** line-shape guard: `regex.match(r'^(\d+\|){6}', line)` — only lines starting with at least 6 digit-groups each followed by `|` are treated as data (skips header/blank lines without needing to track a line-count offset). Then `attribs = regex.split(r'\|', line.rstrip().replace('None', ''))` — **note this REPLACES the literal substring `'None'` with EMPTY STRING before splitting**, not a per-field None-check like Bookmarks/Favorites use. This means any field whose actual numeric/text value happens to CONTAIN the substring "None" would be corrupted — verified as intentional-but-fragile Python behavior (KeySymbol values are always short alphanumeric codes like `nwt`/`w`, never containing "None", so this is safe in practice but Rust should port the identical blanket-replace, not "fix" it into a safer per-field check, to preserve exact parity, unless the planner/discuss-phase decides this qualifies as a bug-for-bug-parity case worth documenting as a deliberate deviation).
- Location dedup: scripture path keys on `KeySymbol+MepsLanguage+BookNumber+ChapterNumber` (`:2137`, fields `attribs[10,11,6,7]`); publication path keys on `KeySymbol+MepsLanguage+IssueTagNumber+DocumentId+Type` (`:2149`, fields `attribs[10,11,9,8,12]`).
- `add_usermark` (`:2160-2184`) is the RANGE-MERGE CALL SITE: grouping key is `Identifier + LocationId` (NOT filtered by color — an overlapping highlight of a DIFFERENT color still merges into one range under the LAST-imported color, matching D8-05's "not filtered by color" note), overlap test `ce >= ns and ne >= cs` (inclusive-token), absorbed BlockRanges DELETEd, new UserMark synthesized fresh EVERY TIME (Highlights import always creates a NEW UserMark row — it does NOT look up an existing one by ColorIndex/Version the way Bookmark/Favorite lookups reuse existing rows; each import of the same highlights file run twice would create N new UserMarks each time, though the merge logic on the BlockRange table still converges since BlockRange rows sharing `Identifier+LocationId` keep getting absorbed regardless of which UserMark inserted them — this IS the Python's actual behavior, a form of UserMark accumulation on repeated import that is NOT a round-trip-stable operation; document as a known non-idempotency, not a bug to silently fix).

### Notes (`export_notes`, `:1486-1723` — txt branch `:1636-1668`; `import_notes`, `:2212-2442`)
Bracket-tag format like Annotations but with MANY more optional tags. Per-record body (`:1646-1666`):
```
\n==={CREATED=<iso>}{MODIFIED=<iso>}{TAGS=<pipe-joined-no-spaces>}
   [Bible: {LANG=n}{PUB=sym}{BK=n}{CH=n}[{VS=n}][{BLOCK=n}][{Reference=ref}][{HEADING=text}]{COLOR=n}[{RANGE=...}][{DOC=0}]]
   [Publication: {LANG=n}{PUB=sym}[{ISSUE=n}][{DOC=n}][{BLOCK=n}][{HEADING=text}]{COLOR=n}[{RANGE=...}]]
   [Independent: (only CREATED/MODIFIED/TAGS)]
===\n<TITLE>\n<NOTE>
```
- `TAGS` in the export header is `item['TAGS'].replace(' | ', '|')` — the DB's `GROUP_CONCAT(t.Name, ' | ')` uses `" | "` as separator, export collapses it to bare `|` for the file; **import's tag-split is a bare `'|'`-split with `.strip()` per tag** (`process_tags`, `:2336`) — so a tag NAME containing a literal `|` would be mis-split; this is an accepted Python limitation (tag names in practice never contain `|`), port verbatim.
- `COLOR` defaults to `'0'` if falsy (`str(item['COLOR']) or '0'`, `:1641`) — note this is a Python truthiness quirk: `str(0)` is the string `'0'` which is truthy in Python (non-empty string), so this `or` NEVER actually triggers for COLOR=0 — the fallback is dead code in practice; port the simple `str(color)` without needing a special zero-case.
- `HEADING` bracket omitted only if the string is exactly `''` (not `None`-checked — `item['HEADING']` is always at least `''` per the SQL's `row[11] or ''`).
- Body: `TITLE + '\n' + NOTE` — note the body is NOT the record's raw content, it's literally `f"{title}\n{note}"`; on import this is re-split on the FIRST newline conceptually but actually via `note[0]` / `'\n'.join(note[1:])` after `.rstrip().split('\n')` — i.e. TITLE = first line, NOTE = everything after the first newline REJOINED with `\n` (so a multi-line note's internal newlines survive round-trip, but a note's FIRST line is always consumed as the title even for independent notes with no real title — Python stores empty-string TITLE and multi-line NOTE by writing `''` as the title line, i.e. body starts with a literal blank first line: `\nline1\nline2`).
- File terminates `\n==={END}===` (`:1668`), same as Annotations.
- **Import `pre_import`** (`:2214-2232`): tag line must match `{NOTES=(.?)}` — the `(.?)` capture group is a SINGLE optional character. If non-empty, it's a title-first-character bucket that triggers `delete_notes(title_char)`: a **conditional interactive bulk-delete** of all Notes whose `Title GLOB '{title_char}*'`, gated by a Yes/No dialog BEFORE the parse/import proceeds (D8-09: this must surface as an explicit "deleted" count in the Rust preview, and the decision to delete must be an explicit user choice in the frontend flow, not silently auto-applied — recommend: surface it as a SEPARATE checkbox/param on the import command, e.g. `import_notes_dry_run(session, file_path, delete_bucket: Option<char>)`, defaulting to `None`/no-delete unless the tag line's capture is non-empty AND the user explicitly opts in via the preview dialog).
- Notes import identity/upsert match (`:2352-2372`): match by `(LocationId, TRIM(Title)=?, BlockIdentifier, BlockType)` if titled, else `((Title='' OR Title IS NULL) AND TRIM(Content)=?, BlockType=0)` if untitled/independent — an EXISTING match UPDATEs `UserMarkId, Content, LastModified, Created`; no match INSERTs fresh with a new `uuid1()` GUID. Timestamps: `attribs['CREATED']`/`['MODIFIED']` fall back to "now" (UTC, `%Y-%m-%dT%H:%M:%SZ`) if absent, truncated to `[:19] + 'Z'`.
- `add_usermark` for Notes (`:2294-2330`) is the SECOND `merge_block_ranges` call site — same overlap/absorb/DELETE/INSERT logic as Highlights, but driven by the Notes' `RANGE` attribute (`;`-separated sub-ranges, each optionally `identifier:start-end`, defaulting to the record's own BLOCK/VS-derived identifier if no explicit identifier prefix) — **each sub-range is a SEPARATE sequential call** into the merge logic since a later sub-range's overlap test must see the BlockRanges the earlier sub-range just inserted/deleted (D8-05). Guard: `COLOR == 0` → return `None` for `usermark_id` WITHOUT creating any UserMark or BlockRange at all (an un-highlighted note skips UserMark synthesis entirely, unlike the Recolor op which DOES synthesize for a plain note — these are different code paths with different behavior, do not conflate).

### Playlist (`export_playlist`/`playlist_export`, `:1725-1818`)
See the architecture diagram above for the full table-copy order. Additional exact facts:
- The exported playlist's own hardcoded Tag row is `INSERT INTO Tag VALUES (1, 2, <filename stem>)` (`:1728`) — TagId is HARDCODED to 1 in the fresh mini-database (safe because `res/blank_playlist` starts empty), Type=2 (playlist-type tag), Name = the export filename's stem (without `.jwlplaylist` extension).
- `android_metadata` locale is copied from the source archive IF the source has that table (`:1730-1733`) — a legacy Android-export artifact table; check for its existence before querying, don't assume it's present (main `.jwlibrary` archives may or may not carry it depending on origin).
- The `TagMap` copy renumbers `Position` to a dense 0-based sequence ordered by `(TagId, Position)` from the SOURCE (`:1756-1760`), using `INSERT OR IGNORE` against the hardcoded TagId=1 — this is NOT a verbatim row copy like the other tables, it's a re-keyed copy.
- `IndependentMedia` selection is a UNION-by-two-predicates: `FilePath IN (thumbnails referenced by copied PlaylistItems)` OR `IndependentMediaId IN (media referenced by the copied PlaylistItemIndependentMediaMap)` (`:1768-1774`) — thumbnails are matched by FILENAME, full media by ID; both must be captured since a PlaylistItem's thumbnail and its full-media row are two SEPARATE IndependentMedia rows (per D8-06's media-add insight: original + thumbnail = TWO rows).
- Media file copy (`:1775-1779`) is best-effort: a `shutil.copy2` failure surfaces a UI warning but does NOT abort the export (`item_list` is still returned) — port as: collect failures into a `Vec<String>` warnings list returned alongside the success count, never `Err` the whole command for one missing source file.
- `Location` copy is filtered to exactly the LocationIds referenced by the just-copied `PlaylistItemLocationMap` (`:1781-1787`), explicit column list (12 named columns), preserving the SOURCE `LocationId` values (not renumbered) — since the mini-DB started empty, no collision is possible.
- manifest.json's `hash` field = `sha256hash()` of the FINISHED (post-VACUUM) `userData.db` file — compute the hash AFTER the SQLite connection closes and file is flushed to disk, matching the main-archive save pipeline's existing hash-last-after-flush pattern (already established Phase 1 convention — reuse the same ordering, don't hash mid-write).
- Compression: `ZIP_DEFLATED` (not stored/uncompressed) — verify `zip` crate's compression-method parameter matches (Rust `zip` crate: `CompressionMethod::Deflated`).

## Round-Trip Determinism

For export→import→export to be stable (byte-identical second export), the following must hold, verified against the Python's actual behavior (not assumed):
- **Annotations**: stable — upsert-by-`(LocationId,TextTag)` means re-importing the same file into the SAME archive is idempotent; a SECOND export of the same data reproduces the same bytes (field order and `VALUE.strip()` are deterministic).
- **Bookmarks**: stable for re-import into the SAME archive (upsert by `(PublicationLocationId, Slot)`); NOT round-trip-lossless for the `¦`↔`|` substitution — a Title/Snippet containing a literal `|` becomes permanently `¦` after one export, and stays `¦` forever after (the substitution is one-directional; there is no import-side un-escape). This is Python's own accepted lossy behavior — do not "fix" it in Rust.
- **Favorites**: stable for re-import (string-level dup-check prevents duplication); ORDER BY Position on export, sequential re-numbering on import — if items are exported then the archive's positions change before re-import, a re-import APPENDS at the new max position rather than restoring original order (an accepted Python limitation, not something to special-case).
- **Highlights**: **NOT idempotent** — see the Notes-and-Highlights UserMark-accumulation finding above. Re-importing the same Highlights file into the archive it came from creates additional UserMark rows every time (though BlockRange geometry still converges via the merge). Flag this explicitly for the round-trip test design: a Highlights round-trip test must assert BlockRange/geometric convergence, NOT UserMark row-count stability.
- **Notes**: stable for re-import into the SAME archive (upsert-by-title/content match); the RANGE-driven UserMark synthesis has the SAME accumulation caveat as Highlights when a note's underlying identity match fails to find the existing row (e.g., a title edit between export and re-import).
- **Playlist**: the exported `.jwlplaylist` is a NEW, self-contained mini-archive; importing it back is a SEPARATE code path from the 5 txt categories (whole-table copy in reverse, not row-by-row line parsing) — 08-CONTEXT flags the import-side line as approximately `:2570-2600`; this research could not independently re-verify that exact range within budget — **[ASSUMED, flag for planner: verify `:2570-2600` against source before wave-splitting Playlist import work; the export side (`:1725-1818`) was fully read and verified this session, the import side was not independently re-read**.

## Media (Playlist add/delete) — Detail

### `add_images` (`:3462-3600`) — UI dialog (`:3464-3526`) is NOT ported (Tauri file-picker replaces it); business logic (`:3528-3600`) IS ported:
1. Resolve/create playlist Tag (`Type=2`, `Name=<playlist name>`) — `INSERT` and catch constraint failure → `SELECT` existing (Python uses try/except on the INSERT itself as its existence check, `:3550-3556`; Rust should use an explicit `SELECT` first or `INSERT ... ON CONFLICT DO NOTHING RETURNING`, since "catch the DB error as control flow" is an anti-pattern this project's established style avoids — see `error.rs`'s typed-error-never-swallowed convention).
2. Load ALL existing `IndependentMedia` rows once (`current_files`, `current_hashes` — full-table scan, not indexed lookup) — for archive-wide dedup by content hash.
3. Per selected file: magic-byte sniff (Python: `puremagic.magic_file`) to get MIME type + validate extension against the allowlist `['bmp','gif','heic','jpg','jpeg','png']` — **Rust equivalent**: the `image` crate's `image::guess_format`/`ImageFormat::from_path` can classify by content OR extension; HEIC is excluded from Rust's allowlist per 08-CONTEXT's accepted gap (no mature pure-Rust HEIC decoder — flag any HEIC file as a typed rejection, not a silent skip, so the user knows why it wasn't added).
4. SHA-256 the source file (`sha256hash`, whole-file digest — Rust: `Sha256::digest(&std::fs::read(path)?)`, same pattern as `manifest.rs:153`). If hash already exists in `current_hashes` → reuse existing `(media_id, thumb_name)`, skip copy+thumbnail entirely (pure dedup, no new files, no new DB rows).
5. If new: `check_name` disambiguates the DISPLAY filename against `current_files` (append `_1`, `_2`, ... suffixes — collision on FILENAME, independent of the hash-dedup which is content-based) — copy original bytes into the archive's temp working dir under the disambiguated name, INSERT `IndependentMedia(OriginalFileName, FilePath, MimeType, Hash)`.
6. Thumbnail: generate a fresh `uuid1()`-named copy of the SAME source file (not the just-copied original — copies again from the SOURCE path, `:3578`), open via Pillow, `.thumbnail((250,250))` (aspect-ratio-preserving max-bound resize — Pillow's `thumbnail()` never upscales, only shrinks-to-fit within the box, preserving aspect ratio; Rust `image` crate equivalent: `DynamicImage::resize(w, h, FilterType)` with a max-fit calculation, or `resize_to_fill`/`thumbnail` helper if `image` exposes one — verify at implementation), save back over the same path (re-encodes in the SAME format as opened — Pillow round-trips format automatically from the file extension), hash the THUMBNAIL bytes separately (a DIFFERENT hash than the original), INSERT a SECOND `IndependentMedia` row for the thumbnail (same `OriginalFileName`/`MimeType` as the original, different `FilePath`/`Hash`).
7. `check_label` disambiguates the PlaylistItem's user-visible `Label` against existing labels for THIS playlist tag (parenthetical suffix `(1)`, `(2)`, ... — a DIFFERENT disambiguation SCHEME than `check_name`'s underscore suffix; port both distinctly, do not unify).
8. INSERT `PlaylistItem(Label, Accuracy=1, EndAction=1, ThumbnailFilePath=<thumb name>)`, `PlaylistItemIndependentMediaMap(PlaylistItemId, IndependentMediaId, DurationTicks=40000000)` (a HARDCODED 4-second-equivalent default duration — `40000000` ticks, verify unit against JW Library's tick convention if the planner wants to confirm this constant, but it is a direct literal port either way), then the playlist Tag's `TagMap` row via `add_tag()`.
- **Atomicity for D8-06's on-disk risk**: Python runs this entire loop inside ONE already-open transaction (`BEGIN` at `:3605`, commit at `:3611` — see the surrounding dispatcher) with file writes happening INTERLEAVED with DB inserts, no rollback-of-files-on-DB-failure and no rollback-of-DB-on-file-failure. The safe Rust translation per 08-CONTEXT's ordering recommendation: perform the DB transaction FIRST (staged inserts held in a real `tx` that is NOT yet committed), THEN perform the file writes/copies, THEN commit the transaction ONLY if every file write succeeded; on any file-write failure, roll back the transaction (leaving zero orphaned DB rows) rather than leaving a committed row pointing at a file that was never written. This is the OPPOSITE order from Python's (Python writes files interleaved with already-committing-adjacent DB statements) but is the correct, safer sequencing per the Core Value and per 08-CONTEXT's explicit "DB failure never leaves an orphaned file" framing — file-exists-but-DB-doesn't-know is a cleanable orphan (a future Clean pass or manual `rm` fixes it); DB-row-exists-but-file-is-missing is a corruption a user discovers only when JW Library fails to render their playlist. **Do the DB work in a held-open transaction, stage/copy files, THEN commit — never the reverse.**

### `delete_playlist_items`/`delete_items` (`:3622-3671`) — ref-counting two-pass:
1. Snapshot `used_thumbs` = `ThumbnailFilePath` values of PlaylistItems NOT IN the deletion selection (`:3628`).
2. For each selected item's `ThumbnailFilePath`: if it's in `used_thumbs`, skip (still referenced elsewhere); else `DELETE FROM IndependentMedia WHERE FilePath = ?` then `os.remove()` the file, silently ignoring a missing-file error (`:3630-3637`).
3. Repeat the identical pattern for full media (`FilePath` via `IndependentMedia JOIN PlaylistItemIndependentMediaMap`) — a SEPARATE `used_files` set, SEPARATE loop (`:3638-3647`) — a file that is a thumbnail for one surviving item but ALSO the full-media for another selected item is correctly NOT double-counted since the two loops check independent used-sets.
4. THEN delete the join/map tables (`PlaylistItemIndependentMediaMap`, `PlaylistItemLocationMap`, `TagMap` all by `PlaylistItemId`), THEN the Marker sub-tables (`PlaylistItemMarkerBibleVerseMap`/`ParagraphMap` filtered by Marker IDs belonging to selected items, THEN `PlaylistItemMarker` itself), THEN `PlaylistItem` last — this ORDER matters for FK integrity even with `foreign_keys` potentially off; port the exact sequence.
5. **Dry-run must show the DB-side diff only** (D8-07) — file removal happens ONLY in `apply_*`. The DryRunReport's `IndependentMedia`/`PlaylistItem`/etc. row diffs are computed inside a never-committed transaction exactly like every other dry-run in this project; the `os.remove()` calls must be OUTSIDE that transaction scope entirely (not merely "not yet reached" — structure the Rust function so dry-run literally cannot reach the file-removal code path, e.g. by having `dry_run_delete_playlist_items` call only the DB-mutation half of a shared helper, with a separate `apply_delete_playlist_items` that calls the DB-mutation helper AND THEN performs file removal after `tx.commit()`).

## ID Recycling — `get_available_ids` (`:1857-1869`)

```python
for table in {Location, Bookmark, UserMark, Note, BlockRange, TagMap, PlaylistItem, IndependentMedia, Tag}:
    expected = 1; available = []
    for id in (SELECT {table}Id FROM {table} ORDER BY {table}Id):
        while expected < id: available.append(expected); expected += 1
        expected = id + 1
    available_ids[table] = available[::-1]   # reversed: pop() takes the SMALLEST gap first
```
- This is a classic **gap-scan**: walks the sorted existing PKs once (O(n) per table, O(9n) total, not O(n²)), collecting every integer in `[1, max_existing)` that is NOT currently used. The `[::-1]` reversal + later `.pop()` (which removes from the END of a Python list) is equivalent to `Vec::pop()` after building the gaps in ASCENDING order and NOT reversing — Phase 7's `db/tags.rs` port already proved this equivalence (07-03-SUMMARY, cited in 08-CONTEXT) — **reuse that exact proof, do not re-derive it; the Rust helper should build gaps ascending and `Vec::pop()` directly, no reversal step needed.**
- Computed ONCE per import operation (`available_ids = get_available_ids()` called once at the top of `import_items`, then threaded via Python closure capture into every category's nested functions) — Rust: `compute_available_ids(tx: &Transaction) -> HashMap<&'static str, Vec<i64>>` called once per `apply_import_<category>`, passed as `&mut HashMap<...>` into every location/record-insert helper. Consuming a gap (`available_ids['Location'].pop()`) must be reflected immediately so a SECOND record needing a new Location ID in the SAME import doesn't reuse the same freed ID (D8-08's "stale gap-map risks double-allocating" warning) — this requires genuine mutable-reference threading, not a `Clone` of the map per call.
- Fallback when no gap is available for a table: plain `INSERT` without specifying the ID column, relying on SQLite's `rowid`/autoincrement (`lastrowid`) — Rust: omit the ID column from the parameterized `INSERT` and read back `tx.last_insert_rowid()`.
- Performance at import scale (thousands of records): the O(9n) gap-scan itself is cheap and happens ONCE regardless of import size; the per-record cost is O(1) `Vec::pop()`. The dominant cost at scale is the per-record location-dedup `SELECT` (Bookmarks/Favorites/Highlights/Annotations/Notes ALL do a `SELECT ... WHERE <dedup predicate>` before every insert) — this is O(n) SELECTs each potentially O(log n) via an index (verify: are `Location`'s dedup columns indexed in the v16 schema? If not, thousands-of-records imports become O(n²) table scans — **flag as an open question for the planner: check `PRAGMA index_list('Location')` on the working schema; if no covering index exists on the dedup predicate columns, consider whether Phase 8 should add one (a schema-neutral index addition, not a wire-format change, so it wouldn't break JW Library compatibility — but confirm this assumption before adding any index)**.

## Zip-Slip and Untrusted Input Validation

- `extract_zip_slip_safe(archive_path: &Path, dest: &Path) -> Result<Vec<ZipEntryMeta>, ArchiveError>` [VERIFIED: `app/src-tauri/src/archive/extract.rs:22-27`] is the exact, already-shipped function signature Phase 8's ONE new zip-open site (importing a `.jwlplaylist`) must call — do not open a raw `zip::ZipArchive` and iterate/extract manually anywhere in Phase 8.
- Beyond zip-slip, import of the 5 txt categories must additionally validate (none of these are Python bugs to "fix" — they are gaps the Python UI-layer implicitly tolerates because a human is watching, that Rust's headless command surface must handle explicitly as typed errors):
  - **Path traversal in manifest/file paths is N/A for the txt categories** (no embedded paths in Bookmarks/Favorites/Highlights/Annotations/Notes text) — it IS relevant for `.jwlplaylist`'s `manifest.json` and any embedded `IndependentMedia.FilePath` values, which flow through `extract_zip_slip_safe`'s existing guard already.
  - **Absurd sizes**: an import file with millions of lines/records has no size cap in Python (the UI would simply hang/be slow); Rust should NOT invent a new hard cap not requested by any decision (no D8-item asks for one) — but the planner should note this as a DoS-shaped edge case worth a comment, not a blocking requirement.
  - **Malformed UTF-8**: as noted above, only Annotations import uses `errors='namereplace'` leniency; the other four hard-fail on invalid UTF-8 at the OS/file-read layer — Rust's `String::from_utf8`/`std::fs::read_to_string` failing should map to a typed `ArchiveError::MalformedImportFile` for all five, with Annotations additionally attempting a lossy/replacement re-read ONLY if the strict read fails (to match Python's per-category leniency asymmetry) — OR, simpler and arguably safer: treat strict-UTF-8-required for all five as a DELIBERATE Rust-side strengthening beyond Python (flag as a discretionary parity deviation for the planner to decide, since `errors='namereplace'` literally corrupts data by design and a stricter Rust behavior is defensible, not a regression).
  - **Hostile field content breaking the `¦`/`|` round-trip**: since the substitution is one-directional (export only, never reversed on import) as established above, a HAND-CRAFTED import file (not Python-exported) containing a literal `|` inside what should be a Title/Snippet-equivalent field WOULD be mis-split into extra fields, corrupting the record — this is inherent to the wire format itself (a genuine Python design limitation being faithfully ported, not a new Rust vulnerability) and is exactly why D8-04 mandates fail-fast-whole-transaction: a malformed/adversarial line should abort the import with a typed error (wrong field count after split) rather than silently landing corrupted data. Rust's parse step MUST validate field COUNT per line (e.g. Bookmarks expects exactly 12 pipe-delimited fields; Highlights 13; Favorites 6) and reject with `ImportError` on any mismatch — Python's own `except:`-wrapped indexed access (`attribs[9]`) would raise `IndexError` on a short line and hit the SAME rollback path, so this is exact-parity behavior, just made explicit/typed in Rust rather than relying on an index-out-of-bounds panic (which Rust must NEVER do per project convention — never `unwrap`/panic on untrusted input).

## Runtime State Inventory

> Not a rename/refactor/migration phase. N/A. Phase 8 adds new commands/modules; it does not rename any identifier, table, file, or external reference. **None — verified: pure feature-addition phase, no renamed state anywhere.**

## Common Pitfalls

### Pitfall 1: Normalizing away the `{END}` sentinel asymmetry
**What goes wrong:** A generic `export_writer(header, records, footer)` helper defaults to always writing a footer, silently adding `{END}` to Bookmarks/Favorites/Highlights files that the Python never terminates that way.
**Why it happens:** Uniformity looks cleaner in code; the asymmetry looks like an oversight rather than a deliberate format difference.
**How to avoid:** Make the `{END}` sentinel an explicit per-category boolean/parameter, defaulting to absent; only Annotations/Notes pass `true`.
**Warning signs:** A round-trip test diffing byte-for-byte against a Python-exported fixture shows an extra trailing line for Bookmarks/Favorites/Highlights.

### Pitfall 2: Reversing the `¦`→`|` substitution on import
**What goes wrong:** Rust "fixes" what looks like a lossy one-way transform by un-escaping `¦` back to `|` on Bookmark import, diverging from Python's actual (lossy) behavior.
**Why it happens:** It looks like an obvious oracle bug; symmetric encode/decode feels more "correct."
**How to avoid:** Verified against source (`:2020` — Bookmarks import never touches `¦`) — port the asymmetry faithfully; flag it in a comment as intentional-Python-behavior, not fixed.
**Warning signs:** A Title with `¦` characters round-trips to a DIFFERENT string than Python produces for the same input.

### Pitfall 3: Computing `available_ids` per record instead of once
**What goes wrong:** Recomputing the gap-scan inside a per-record loop is both O(n²) at import scale AND semantically wrong once the import itself has inserted new rows mid-transaction (a stale gap-map could hand out the same ID twice).
**Why it happens:** It's the "obviously correct" naive translation if you don't notice Python computes it exactly once, outside all category loops.
**How to avoid:** `compute_available_ids(tx)` called once per `apply_import_<category>` invocation, threaded by `&mut` reference (D8-08).
**Warning signs:** A `UNIQUE constraint failed` on a primary key during a large import; degrading import performance as file size grows.

### Pitfall 4: File writes inside the transaction that DB rollback can't undo
**What goes wrong:** A media-add or media-delete apply path writes/removes a file BEFORE the transaction commits (or worse, inside `dry_run_*`), then the transaction rolls back — leaving a file on disk the DB no longer (or never did) reference, or a dry-run preview that already deleted a real file.
**Why it happens:** Mirroring Python's interleaved DB+file code too literally, without noticing Python never rolls back mid-loop in practice (its errors abort the whole app via `sys.exit()`, a luxury Rust's typed-error contract doesn't have).
**How to avoid:** Structure `apply_*` as: hold transaction open → stage all DB writes → perform file writes/removals only after the transaction is ready to commit (add: commit AFTER files land; delete: commit the DB delete, THEN best-effort remove files) → commit. `dry_run_*` NEVER calls the file-write/removal code path at all (D8-07).
**Warning signs:** A test that kills the process mid-apply leaves an orphaned file or a DB row pointing at a missing file; a dry-run test that finds a real file deleted afterward.

### Pitfall 5: Treating Highlights/Notes import as idempotent
**What goes wrong:** A round-trip test asserts "re-importing the same file changes nothing," which is FALSE for Highlights/RANGE-Notes because Python always synthesizes a fresh UserMark on every import pass (no UserMark-level dedup — only BlockRange geometry converges).
**Why it happens:** Every OTHER category in this phase IS idempotent (upsert-keyed), making it easy to assume uniform behavior.
**How to avoid:** Design the round-trip test to assert BlockRange geometric convergence (final set of ranges is stable) while explicitly tolerating UserMark row-count growth across repeated imports.
**Warning signs:** A "flaky" test that passes once and fails on a second run of the same import against the same archive.

### Pitfall 6: Assuming `image` crate parity with Pillow's `.thumbnail()`
**What goes wrong:** Rust code implements a fixed 250×250 resize (always producing exactly 250×250 output) instead of Pillow's aspect-preserving max-bound behavior (shrink-to-fit within 250×250, never upscale, never distort aspect ratio).
**Why it happens:** "Thumbnail" sounds like a fixed-size operation; Pillow's actual contract is a bounding-box fit.
**How to avoid:** Compute the scale factor as `min(250/width, 250/height)`, clamp to `<= 1.0` (never upscale), apply uniformly to both dimensions before calling the crate's resize function.
**Warning signs:** A portrait-oriented source image produces a stretched/cropped 250×250 thumbnail instead of a correctly-proportioned smaller image.

## Code Examples

### Reusing `extract_zip_slip_safe` for `.jwlplaylist` import
```rust
// Source: app/src-tauri/src/archive/extract.rs:22-27 (existing signature, call it, don't reimplement)
let entries = extract_zip_slip_safe(&jwlplaylist_path, &playlist_tmp_dir)?;
```

### Reusing `sha256hash` pattern for media dedup
```rust
// Source: app/src-tauri/src/archive/manifest.rs:25,146-153 (existing pattern for the manifest hash field)
use sha2::{Digest, Sha256};
let bytes = std::fs::read(&source_path)?;
let digest = Sha256::digest(&bytes);
let hex = format!("{digest:x}");
```

### Reusing the compact-JSON manifest contract for `.jwlplaylist`'s own manifest.json
```rust
// Source: app/src-tauri/src/archive/manifest.rs:97-100 — SAME struct/serializer, different schemaVersion/hash inputs
let manifest = Manifest::new(/* playlist-specific fields */);
let compact = manifest.to_compact_string()?; // no whitespace, matches Python's separators=(',', ':')
```

### Ascending-gap ID recycling (Phase 7-proven equivalence — no reversal needed)
```rust
// Pattern proven equivalent to Python's available[::-1] + .pop() in 07-03-SUMMARY (db/tags.rs)
fn compute_available_ids(tx: &Transaction) -> Result<HashMap<&'static str, Vec<i64>>, ArchiveError> {
    const TABLES: [&str; 9] = ["Location","Bookmark","UserMark","Note","BlockRange","TagMap","PlaylistItem","IndependentMedia","Tag"];
    let mut out = HashMap::new();
    for table in TABLES {
        let mut expected = 1i64;
        let mut available = Vec::new();
        let sql = format!("SELECT {table}Id FROM {table} ORDER BY {table}Id");
        let mut stmt = tx.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let current: i64 = row.get(0)?;
            while expected < current { available.push(expected); expected += 1; }
            expected = current + 1;
        }
        out.insert(table, available); // ascending; Vec::pop() takes smallest gap first — no reversal
    }
    Ok(out)
}
```

## State of the Art

| Old Approach (Python) | Current Approach (this rewrite) | When Changed | Impact |
|--------------|------------------|--------------|--------|
| UI-coupled `QFileDialog` calls inline in export/import business logic | Tauri `dialog` plugin invoked from frontend; Rust commands take resolved paths | Phase 8 (D8-10) | Backend testable without a UI harness; matches Phase 1-7's separation |
| Bare `except:` → `crash_box` → `sys.exit()` on malformed import | Typed `ImportError{category, line, reason}` → `ErrorDto`, rollback, no process exit | Phase 8 | App survives a bad import file; error is actionable |
| Interleaved DB-write + file-write with no atomicity story | DB transaction staged first, file writes sequenced around commit per D8-06/D8-07 | Phase 8 | No orphaned files from a failed DB write; no committed rows pointing at missing files |
| `str(list).replace('[','(')` inline `IN (...)` for playlist table filters | `params_from_iter` + generated placeholders | Phase 2 precedent, applied here to Playlist copy queries | No injection; consistent with the whole project |

**Deprecated/outdated:** `.xlsx`/`.md` export paths (Pillow/xlsxwriter/polars/xlsx2csv-dependent) — not ported (D8-01).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `image` crate is the correct, legitimate choice for thumbnailing (well-known, MIT/Apache-licensed, actively maintained) | Standard Stack, Package Legitimacy Audit | `[ASSUMED]` from training knowledge only — NOT verified via a live registry/legitimacy-check tool this session. **Must be verified by the planner via `gsd-tools query package-legitimacy check` and a live `cargo`/crates.io lookup before any `cargo add image` lands in a plan.** |
| A2 | `Location` table's dedup-predicate columns (KeySymbol/MepsLanguage/BookNumber/ChapterNumber/DocumentId/IssueTagNumber/Type combinations) are indexed in the working v16 schema, keeping per-record import dedup near-O(log n) | ID Recycling § Performance | If unindexed, large imports (thousands of records) degrade toward O(n²); verify via `PRAGMA index_list('Location')` on a fixture before committing to "accept Python's per-record SELECT pattern as-is" |
| A3 | The `.jwlplaylist` import-side code (approx. `JWLManager.py:2570-2600` per 08-CONTEXT) mirrors the export side closely enough that no additional novel logic exists there | Round-Trip Determinism §Playlist | This exact range was NOT independently re-read in this research pass (budget); the planner MUST read it before finalizing the Playlist import wave's task breakdown — could contain additional warts not captured here |
| A4 | Rust `image` crate's resize/thumbnail API can express Pillow's aspect-preserving max-bound-fit semantics without upscaling | Common Pitfalls §6 | If the exact API shape differs, the planner needs to hand-write the scale-factor math rather than relying on a single library call — low risk, just extra glue code either way |
| A5 | Highlights import's `.replace('None','')` blanket string-replace (rather than per-field None-check) is intentional-fragile-but-safe in practice because no real KeySymbol/field value contains the substring "None" | Wire Formats §Highlights | If a future publication code or user-entered value legitimately contains "None" as a substring, Highlights import would silently corrupt that field — extremely low real-world probability but worth a one-line code comment citing this research finding |
| A6 | Strict-UTF-8 (no `errors='namereplace'` leniency) is an ACCEPTABLE deliberate strengthening for all five categories in Rust, rather than a parity gap | Zip-Slip and Untrusted Input §Malformed UTF-8 | If a real-world Python-exported Annotations file somehow contains non-UTF-8 bytes needing the leniency, Rust's stricter behavior would reject a file Python accepts — flag as an explicit discuss-phase/planner decision point, not silently resolved here |

**If this table is empty:** N/A — six assumptions logged above, none blocking, all flagged for planner verification.

## Open Questions

1. **Should the Playlist import-side line range be independently verified before planning its wave?**
   - What we know: 08-CONTEXT cites `~:2570-2600`; export side (`:1725-1818`) is fully verified in this research.
   - What's unclear: whether the import side has additional warts (e.g., does it re-key IDs on import the way export does for TagMap, or does it trust the incoming mini-archive's IDs directly and risk collision with the target archive's existing IDs — THIS is a real open design question since the target archive already has its own ID space).
   - Recommendation: the planner/executor reads `:2570-2700`-ish directly before writing the Playlist-import task; likely needs its OWN ID-remapping pass distinct from the 5-category `get_available_ids` recycling, since importing a whole mini-archive's `PlaylistItem`/`IndependentMedia`/`Location` rows into a target archive risks direct PK collision, not just "find a gap" — this could be the phase's SECOND-highest-risk area after media atomicity, currently under-researched.

2. **`image` crate legitimacy — not yet run.**
   - What we know: no new Cargo dependency exists today for image decoding; `image` is the obvious/standard choice from training knowledge.
   - What's unclear: current maintenance status, exact license, and a live registry check (this research pass had no live crates.io/legitimacy-check tool invocation).
   - Recommendation: mandatory `checkpoint:human-verify` task in PLAN.md, per D8-06 and the project's package-legitimacy protocol; do not skip because the crate "sounds standard."

3. **Location dedup index coverage at import scale.**
   - What we know: Python does one `SELECT` per record before every insert across all 5 categories; this is fine at Python's typical hand-import scale (tens to low-hundreds of records) but Phase 8's IO-03 criterion implies a "round-trip test" workload that could exercise thousands of rows.
   - What's unclear: whether the v16 schema already indexes the relevant `Location` columns.
   - Recommendation: run `PRAGMA index_list('Location')` on the test fixture during Wave 0; if uncovered, decide whether to add a schema-neutral index (doesn't change wire format, only local query performance) as a small addendum task.

## Environment Availability

> Phase 8 is code + config only (Rust backend + frontend), same posture as Phase 7. The one new external dependency (`image` crate) is a Cargo-registry addition, not a system tool/service — standard `cargo build` picks it up once added. No system-level tools/services/runtimes beyond the existing Tauri/Cargo/npm toolchain. **Step effectively SKIPPED for system dependencies; the one registry dependency is tracked via the Package Legitimacy Audit instead.**

## Validation Architecture

> `workflow.nyquist_validation` enabled (default). Section included.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust: `cargo test` (`app/src-tauri/tests/`); Frontend: `vitest` |
| Config file | `app/src-tauri/Cargo.toml`; `app/vitest.config.*` |
| Quick run command | `cargo test --test <name>_tests` |
| Full suite command | `cd app/src-tauri && cargo test` ; `cd app && npm run test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| IO-01 | Each of 5 categories exports byte-identical to a Python-shaped golden fixture (incl. `{END}` asymmetry, `¦` escaping, `'None'` sentinel) | integration | `cargo test --test export_wireformat_tests` | ❌ Wave 0 |
| IO-01 | Playlist export produces a valid `.jwlplaylist` zip with compact-JSON manifest + correct table copy order | integration | `cargo test --test playlist_export_tests` | ❌ Wave 0 |
| IO-02 | Each of 5 categories imports a synthetic Python-shaped fixture, lands correct DB state (location dedup, upsert-vs-insert) | integration | `cargo test --test import_wireformat_tests` | ❌ Wave 0 |
| IO-02 | Malformed import file (wrong field count, missing tag line, bad `RANGE` syntax) aborts whole transaction, zero partial rows | integration | `cargo test --test import_failfast_tests` | ❌ Wave 0 |
| IO-02 | Highlights/Notes-RANGE import correctly calls `merge_block_ranges` (overlap absorb, chain-merge, cross-color grouping) | integration | `cargo test --test import_range_merge_tests` | ❌ Wave 0 (reuses `highlights.rs` primitive, new call-site tests) |
| IO-02 | Playlist media add: dedup by hash, thumbnail aspect-preserving, atomic DB-then-files sequencing (simulated file-write failure rolls back DB) | integration | `cargo test --test media_add_tests` | ❌ Wave 0 |
| IO-02 | Playlist media delete: two-pass ref-count (item shared across surviving items is NOT deleted); dry-run never touches filesystem | integration | `cargo test --test media_delete_tests` | ❌ Wave 0 |
| IO-03 | `compute_available_ids` matches Python's gap-scan across all 9 tables on a fixture with pre-seeded gaps | unit | `cargo test --test ids_tests` | ❌ Wave 0 |
| IO-03 | Import of N records into an archive with M pre-existing gaps recycles exactly `min(N,M)` gap IDs before falling back to autoincrement | integration | `cargo test --test import_wireformat_tests` (extend) | ❌ Wave 0 |
| all | Semantic round-trip: export this app's data → re-import → compare DB state (never byte-diff for DB; DO byte-diff the exported .txt bytes per the wire-format exception) | integration | `cargo test --test io_roundtrip_tests` | ❌ Wave 0 |
| all | Import/Export dialogs + preview reuse + operations.ts slot additions | unit (vitest) | `npm run test -- ImportPreviewDialog ExportDialog` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --test <area>_tests` for the area being built.
- **Per wave merge:** `cd app/src-tauri && cargo test` + `cd app && npm run test`.
- **Phase gate:** full suite green before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `tests/common/mod.rs` — extend fixture with: pre-seeded ID gaps across all 9 recycling tables; a Bookmark Title/Snippet containing literal `|`; Highlights/Notes with overlapping/adjacent ranges for merge testing; an `IndependentMedia` pair (original+thumbnail) with a known hash; multiple PlaylistItems sharing one media file (for delete ref-count testing); a synthetic Python-shaped `.txt` fixture PER category (golden files, hand-authored to match the exact wire format documented above, not generated by exporting from this app — needed to test IMPORT independent of this app's own EXPORT correctness).
- [ ] `tests/{export_wireformat,playlist_export,import_wireformat,import_failfast,import_range_merge,media_add,media_delete,ids,io_roundtrip}_tests.rs`.
- [ ] Synthetic image fixtures (tiny valid JPEG/PNG/GIF/BMP byte arrays, plus one HEIC to test the expected-rejection path) for media-add tests.
- [ ] Frontend vitest for import/export dialogs + `operations.ts` new `(category, Export|Import)` slots (note: Export is selection-OPTIONAL, a third state beyond `operations.ts`'s current binary `NEEDS_SELECTION` model per 07-PATTERNS' "Design gap" finding — Phase 8 inherits this same gap and must resolve it, likely reusing whatever resolution Phase 7's Clean/Mask work landed on for archive-wide ops).
- [ ] Package-legitimacy checkpoint task for `image` crate BEFORE any media-add implementation task.

## Security Domain

> `security_enforcement` enabled (absent = enabled). Included.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Local desktop app, no auth surface |
| V3 Session Management | no | No sessions |
| V4 Access Control | no | Single local user, local file |
| V5 Input Validation | yes | Typed `ImportError`; field-count/regex validation on every parsed line/record BEFORE any SQL executes; parameterized SQL only |
| V6 Cryptography | no | SHA-256 here is a content-identity hash (dedup), not a security control — no secret material involved |
| V12 File/Resource | yes | THE core new risk of this phase — `extract_zip_slip_safe` reuse for `.jwlplaylist`; on-disk media writes/deletes must stay inside the archive's session TMP working dir, never accept an absolute/traversal path from parsed import content |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Zip-slip via a hostile `.jwlplaylist` | Tampering / EoP (file write outside intended dir) | `extract_zip_slip_safe` reuse (D8-02) — already fixed, just call it |
| SQL injection via crafted import file field values | Tampering | `params_from_iter`; the ONLY dynamic SQL text is table names in the (internally-fixed) `get_available_ids`/ref-count queries, never user-controlled field content |
| Wire-format desync corrupting fields (`|` inside unescaped text) | Tampering (data integrity) | Fail-fast field-count validation before any insert (D8-04) |
| Orphaned files / phantom DB rows from a failed media add | Denial (data integrity) | Stage DB transaction, write files, commit only after file success (this research's Pitfall 4) |
| Resource exhaustion from an oversized/malicious import file | Denial (availability) | No hard cap requested; note as an accepted low-priority gap, not a blocking control |

## Sources

### Primary (HIGH confidence — read directly this session)
- `JWLManager.py:1307-1852` (export dispatch + all 5 txt exports + Playlist export, `export_header`, `create_xlsx`/`.md` branches noted-but-deferred).
- `JWLManager.py:1855-2442` (import dispatch, `get_available_ids`, all 5 txt imports incl. `add_usermark` range-merge call sites).
- `JWLManager.py:3462-3611` (`add_images` dialog + `update_db`).
- `JWLManager.py:3620-3671` (`delete_items`/`delete_playlist_items` ref-counting).
- `app/src-tauri/src/archive/extract.rs:22-27` (`extract_zip_slip_safe` signature).
- `app/src-tauri/src/archive/manifest.rs` (compact-JSON `Manifest`/`to_compact_string`, `Sha256::digest` pattern).
- `app/src-tauri/Cargo.toml` (confirmed `sha2`, `zip`, `serde_json`+`preserve_order`, `regex`, `rusqlite`, `tauri-plugin-dialog` already declared; confirmed `image` is ABSENT).
- `.planning/phases/08-import-export-parity/08-CONTEXT.md` (D8-01..D8-10, locked decisions).
- `.planning/phases/07-full-editing/07-RESEARCH.md` + its `## ⚠ Corrections` section (ID-recycling equivalence proof, `sha2`/`uuid`/`fancy-regex` dependency-graph facts, `merge_block_ranges` primitive location).
- `.planning/ROADMAP.md` Phase 8 section; `.planning/REQUIREMENTS.md:59-64` (IO-01..04).
- `app/src-tauri/tests/` directory listing (existing test file naming convention).

### Secondary (MEDIUM confidence)
- The exact `.jwlplaylist` import-side Python code (`~:2570-2600`, per 08-CONTEXT's citation) — NOT independently re-read this session (A3); the export side WAS fully read and verified.
- `image` crate's exact resize/thumbnail API shape for aspect-preserving max-bound-fit — inferred from general crate-ecosystem knowledge, not confirmed against current crate docs this session (A4).

### Tertiary (LOW confidence)
- `image` crate legitimacy (maintenance, license, current version) — `[ASSUMED]` from training knowledge only, no live registry check performed (A1) — MANDATORY planner follow-up.

## Metadata

**Confidence breakdown:**
- Wire formats (5 txt categories, export side): HIGH — every line cited, read directly from source this session.
- Wire formats (import side, 5 txt categories): HIGH — every line cited, read directly from source this session.
- Playlist export: HIGH — fully read (`:1725-1818`).
- Playlist import: MEDIUM — cited by CONTEXT but not independently re-verified this session (A3).
- Media add/delete: HIGH — fully read (`:3462-3611`, `:3620-3671`).
- ID recycling: HIGH — fully read, and the ascending-vs-reversed equivalence already proven in Phase 7.
- New dependency (`image` crate): MEDIUM — the NEED is HIGH confidence (no existing Rust image-decode capability, verified via Cargo.toml absence), but the SPECIFIC crate choice is LOW/`[ASSUMED]` pending the mandatory legitimacy checkpoint.

**Research date:** 2026-07-26
**Valid until:** stable (in-repo source of truth; ~30 days, but effectively until the Python source or Phase 7 infra changes) — EXCEPT the `image` crate legitimacy check, which must be re-verified at implementation time regardless of this document's age (registry state changes independently of the codebase).

---

## ⚠ Addendum — `image` crate legitimacy check BLOCKED this session (2026-07-26)

The research pass correctly refused to bless the `image` crate from training knowledge alone
(Assumption A1, Open Question 2) and required a live registry verification before any
`cargo add image` lands in a plan.

**That verification could not be performed.** Both available network paths are unusable in this
session: a hook redirects `curl` and `WebFetch` to the context-mode MCP tools, and that MCP
server is currently disconnected. No live crates.io lookup was possible. The crate's
maintenance status, current license, and provenance therefore remain **unverified** — exactly
the state the researcher flagged, not resolved.

**Do NOT treat `image` as approved.** The planner MUST take one of these two paths and record
which:

- **(a) Keep the dependency, gated.** Carry a genuine blocking `checkpoint:human-verify` task
  before the dependency is added, requiring a live `cargo add image --dry-run` / crates.io
  check by a human or a session with working network access. The project has added ZERO new
  Cargo dependencies across seven phases (`uuid`, `rand` and `fancy-regex` were all deliberately
  hand-rolled or designed around), so this would be a first — it deserves the friction.

- **(b) Ship without the dependency.** A thumbnail IS structurally required (`PlaylistItem
  .ThumbnailFilePath` plus a second `IndependentMedia` row — see the Media detail section), so
  the feature cannot simply be skipped. But the *resize* is not structurally required: copying
  the source image bytes unmodified to the thumbnail path yields a valid, JW-Library-readable
  archive with a correct hash and a correct second `IndependentMedia` row — merely a larger
  thumbnail than Pillow's 250×250 max-fit. This diverges from the Python on file SIZE, never on
  schema or wire format. If chosen, document it as a deliberate, reversible deviation and leave
  a TODO citing this addendum so a later phase can add real resizing once the dependency is
  verified.

Recommendation: **(b) for this phase**, since it keeps the zero-new-dependency streak intact and
removes an unverifiable blocker from the critical path, with (a) available as a follow-up once
the registry is reachable. The planner may overrule with reasoning — but must not silently
assume the crate is fine.
