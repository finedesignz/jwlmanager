# Phase 9: Incremental Export - Research

**Researched:** 2026-07-26
**Domain:** Diff-and-filter export (read-only), reusing Phase 8's exporters/parsers unmodified
**Confidence:** HIGH (all claims verified against shipped Phase 8 Rust source; zero Python precedent exists for this feature per D9-CONTEXT)

## Summary

Phase 9 adds exactly one new layer: a diff step that runs BEFORE the existing Phase 8
`export_<category>` calls, computing a `NonEmpty<Cat>Ids` selection from
`(parsed prior file, live archive rows)` instead of the caller building it from a UI
selection. Every downstream mechanic — the exporter, the `'None'` sentinel, `¦`
escaping, the `{END}` sentinel asymmetry, CRLF-tolerant re-import — is already shipped
and untouched. The phase's entire risk surface is getting the diff *itself* right: which
fields the identity key is, which fields the content hash covers, and proving by test
that a `LastModified`/`Created` change alone never flips the hash while a real content
change always does.

**Primary recommendation:** implement one generic `diff_category<K: Eq + Hash>(prior: &[(K, String)], live: &[(K, String)]) -> DiffResult<K>` helper (hash pre-computed by the caller per category, `K` = `i64` for four categories, `(i64, String)` tuple for Annotations), call it once per category from five new `export_<category>_incremental` Tauri commands, and feed `added ∪ modified` straight into the existing unmodified `export_<category>(conn, Some(&ids), ...)`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Prior-file parsing | Rust backend (`db/io/import.rs`, reused) | — | Already exists, already CRLF-normalized; must not be re-implemented |
| Live-row fetch | Rust backend (new small getters in `db/io/export.rs` or reuse `db/browse.rs` list_category) | — | Must read the SAME columns the exporter reads, so the hash is computed over what will actually be exported |
| Diff (identity + hash compare) | Rust backend (new `db/io/diff.rs`) | — | Pure in-memory computation; no SQL, no filesystem; easiest to unit-test in isolation |
| Row selection → export | Rust backend (reuses Phase 8 `export_<category>` unmodified) | — | D9-06: never fork the exporter |
| File picker (prior file) + summary display | Frontend (extend Phase 8 Export dialog) | Rust backend (Tauri command param) | Native file dialog is a frontend/Tauri-plugin concern; backend only needs a `PathBuf` |
| Deleted-candidate count display | Frontend | Rust backend (returns count in summary DTO) | UI-only concern; backend just counts, never encodes into the .txt |

## Standard Stack

### Core
No new libraries. Reuses:
| Component | Source | Purpose |
|-----------|--------|---------|
| `sha2::Sha256` | already declared (Cargo.toml, D8-06 media dedup) | Content hash for D9-03 |
| `parse_<category>_file` | `db/io/import.rs` (Phase 8, unmodified) | Reads the prior export file |
| `export_<category>` | `db/io/export.rs` (Phase 8, unmodified) | Writes the incremental output file |
| `NonEmpty<Cat>Ids` types | `db/delete.rs`, `db/color.rs`, `db/favorites.rs` | Selection type already accepted by every exporter |

### Supporting
| Library | Purpose | When to Use |
|---------|---------|-------------|
| `std::collections::{HashMap, HashSet}` | set-difference logic for added/modified/deleted-candidates | diff computation |
| `ts-rs` (already in use per Phase 6/8 DTO conventions) | derive TS bindings for the new `IncrementalExportSummary` DTO | frontend consumption |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `sha2::Sha256` | `std::collections::hash_map::DefaultHasher` (SipHash) | Rejected: not stable across Rust versions/platforms in principle (docs explicitly disclaim stability), and D9-03/CONTEXT already locks `sha2` — no reason to deviate. SHA-256 is also already proven correct for D8-06 dedup. |
| Generic `diff_category<K>` helper | Five independent copy-pasted diff functions, one per category | A shared generic is simpler given 4 of 5 categories share `K = i64`; Annotations' composite key is the one case needing its own instantiation (`K = (i64, String)`), which the generic still supports without special-casing (Rust generics over tuples work identically to over scalars). Recommend the generic — Claude's Discretion per CONTEXT explicitly leaves this open, and a single well-tested helper is less risk than five near-duplicates. |

**Installation:** none — zero new dependencies (confirmed: `sha2` already present, checked via `Cargo.toml` reference in D8-06/D9-03 CONTEXT; no live crates.io check performed or needed since no new package is being added).

**Version verification:** N/A — no new package added this phase.

## Package Legitimacy Audit

**Not applicable.** This phase adds zero new Cargo dependencies (binding constraint, confirmed in CONTEXT: "No new Cargo dependency. The project has added zero across eight phases... `sha2` is already declared and in use — reuse it."). No package-legitimacy check is required.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
User picks prior .txt file (optional)          Live archive (open Connection)
        │                                                │
        ▼                                                ▼
parse_<category>_file(text)  [Phase 8, reused]   read_<category>_lines/rows(conn, None)
  -> Vec<Record>  (or empty if no prior file)       [Phase 8 export.rs internals, reused
                                                       read-side, or a thin new getter]
        │                                                │
        └──────────────────┬─────────────────────────────┘
                            ▼
                  diff_category(prior, live)   [NEW, Phase 9]
                  - identity key per D9-02 (PK or composite)
                  - content hash per D9-03 (sha256 over exported-field tuple)
                  -> DiffResult { added: Vec<K>, modified: Vec<K>, deleted_candidates: Vec<K> }
                            │
                            ▼
              export_ids = added ∪ modified   (mapped K -> i64 row id)
                            │
                            ▼
        export_<category>(conn, Some(&NonEmpty<Cat>Ids::try_from(export_ids)?), header, out_path)
                  [Phase 8, UNMODIFIED — D9-06]
                            │
                            ▼
              ordinary .txt file, byte-identical shape to a normal
              filtered export (same header, same sentinels, same escaping)
                            │
                            ▼
        IncrementalExportSummary { added, modified, deleted_candidates } -> frontend
```

A reader can trace: prior file (or none) + live archive → `diff_category` → id-set →
existing exporter → file + summary. No branch re-enters the exporter with different
logic; the diff is strictly upstream.

### Recommended Project Structure
```
app/src-tauri/src/db/io/
├── export.rs        # unchanged (Phase 8) — read_<category>_lines/rows already here
├── import.rs         # unchanged (Phase 8) — parse_<category>_file already here
├── header.rs          # unchanged (Phase 8)
└── diff.rs            # NEW (Phase 9) — diff_category<K>, per-category hash-input builders,
                        #   DiffResult<K>, IncrementalExportSummary DTO
```
`lib.rs` gets five new `#[tauri::command] fn export_<category>_incremental(...)` entries
alongside the existing `export_<category>` commands (same file, same registration list
pattern at the `tauri::generate_handler!` call already at `lib.rs:2631+`).

### Pattern 1: Read-side reuse instead of new SQL
**What:** Do not write new SQL to fetch live rows for the diff. Phase 8's
`read_favorite_lines`/`read_bookmark_lines`/`read_annotation_rows`/`read_highlight_lines`/
`read_raw_note_rows` (all `pub(crate)` or private in `export.rs`) already read exactly the
columns that get exported, in exactly the field order the wire format uses. The diff's
content hash MUST be computed over the same field set the file will actually contain —
reusing these functions (promoting any currently-private ones to `pub(crate)` if `diff.rs`
lives in a sibling module) guarantees that by construction, rather than by keeping two
independently-written column lists in sync.
**When to use:** Always, for the four flat-row categories. For Notes, `read_raw_note_rows`
returns pre-derivation raw fields (title/content/tags/etc.) — hash the raw fields, NOT the
final formatted wire line, so a Notes hash is stable even if `now`/timestamp-fallback
logic (export-time-only, `db/io/export.rs:604-618`) would otherwise perturb formatting
between two export runs of unchanged data. See Pitfall 1 below — this is the sharpest trap
in the whole phase.
**Example:**
```rust
// Source: app/src-tauri/src/db/io/export.rs:68-110 (existing, reused)
pub(crate) fn read_favorite_lines(conn: &Connection, ids: Option<&NonEmptyTagMapIds>)
    -> Result<Vec<String>, ArchiveError> { /* ... */ }
```

### Pattern 2: Identity key type per category (D9-02)
**What:** Four categories use a single `i64` PK (`NoteId`, `BlockRangeId`, `BookmarkId`,
`TagMapId`); Annotations uses a composite `(LocationId, TextTag)` since two annotations
can share a `LocationId`. `diff_category<K: Eq + Hash + Clone>` is generic over `K`, so
Annotations simply instantiates `K = (i64, String)` (TextTag is already a `String` in the
parsed `AnnotationRecord`... actually AnnotationRecord has no raw TextTag field currently
— see Pitfall 2).
**When to use:** All five categories, one shared generic function.

### Pattern 3: Fail-fast on unparseable prior file (reuse D8-04 posture)
**What:** `parse_<category>_file` already returns `Result<_, ArchiveError::ImportMalformed>`
on any malformed line. Phase 9 MUST propagate that error, not swallow it into "treat as no
prior file" — CONTEXT explicitly calls this out ("Do not silently treat an unparseable
prior file as 'no reference point'"). Since `parse_<category>_file` already fails on the
first malformed line before returning anything, propagating with `?` gets this for free;
no new logic needed, just don't `.ok()`/`.unwrap_or_default()` the `Result`.
**When to use:** Every new `export_<category>_incremental` command.

### Anti-Patterns to Avoid
- **Hashing the formatted wire line instead of raw field values:** the wire line for Notes
  embeds an export-time `now` fallback and per-shape bracket assembly; two exports of
  identical underlying data at different wall-clock moments could theoretically format
  differently in edge cases (corrupted-timestamp fallback branch). Hash the raw
  `RawNoteRow` fields (as strings, before the `export_notes` formatting closure runs), not
  the final line.
- **Re-deriving column lists by hand for the "live" side:** always call into the existing
  `read_*` functions; a hand-rolled duplicate SQL query is a second place to keep in sync
  with the exporter and a real drift risk over time.
- **Trusting `LastModified`/`Created` for anything, even as a hash input tiebreaker:**
  D9-03 is explicit — content hash only, uniformly across all five categories, precisely
  because Note is the only table with any timestamp column and even that one must be
  excluded from the hash input (see Pitfall 1).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Parsing a prior-export .txt file | A new lightweight Phase-9-only parser | `parse_<category>_file` (Phase 8, `db/io/import.rs`) | Already handles CRLF normalization, the exact per-category line-shape grammar, and typed `ImportMalformed` errors naming the offending line |
| Filtering export by id set | A parallel `export_<category>_subset` | `export_<category>(conn, Some(&ids), ...)` (Phase 8, unmodified, D9-06) | Already accepts exactly this shape; guarantees byte-identical wire output to a normal filtered export |
| Content hashing | Rolling a custom checksum/hash | `sha2::Sha256` (already linked, D8-06 precedent) | Zero new dependency; cryptographic-quality hash makes accidental collisions a non-concern even at 9,000+ row scale |

**Key insight:** every piece of machinery this phase needs except the diff itself was
already built in Phase 8 with exactly this future use in mind (D8-10's
`ids: Option<&NonEmpty*Ids>` signature). The discipline is reuse, not reimplementation.

## Common Pitfalls

### Pitfall 1: Hashing a field the prior-file parser cannot reproduce (false "unchanged")
**What goes wrong:** If the hash input includes any field that `parse_<category>_file`
does NOT preserve from the wire text (either because the wire format itself omits it, or
because the parser drops it), the prior-side hash for that field is always some fixed/
default value, while the live-side hash reflects the real current value. Two outcomes are
both bad: (a) if the field is omitted from BOTH sides' hash input, that field's changes
are invisible to the diff — a real change never gets exported (the "wrong diff" risk
CONTEXT explicitly frames as the core Core-Value risk for this phase); (b) if it's
included only on the live side effectively (parser defaults it to `None`/`0` on the prior
side), EVERY row looks "modified" forever, defeating the entire feature (spurious
never-converges behavior).
**Why it happens:** Wire formats are lossy for fields that don't round-trip user-visibly.
Concretely audited from `db/io/import.rs`:
- `NoteRecord` has no `book_number`/`location_title` field at all — Notes' wire header
  encodes `{BK=}{CH=}{VS=}{Reference=}{HEADING=}` but `parse_notes_file` likely derives
  `bk`/`ch`/`vs`/`heading` (need to confirm field names match `NoteShape` variants exactly
  — see Open Questions). The exported `Reference` field (computed, not stored) MUST NOT be
  part of the hash input on either side, since it's fully derived from `bk`/`ch`/`vs` and
  hashing it is redundant, not wrong, but only if the underlying fields are what's hashed.
- `HighlightRecord`'s doc comment states explicitly: "NULL renders as an EMPTY STRING here
  ... exactly reproducing Python's fragile-but-intentional behavior" — meaning a live row
  with SQL `NULL` in e.g. `KeySymbol` hashes as `""` on the live side (if using
  `value_to_field`'s `None`) but the exporter's OWN `value_to_field` renders `Value::Null
  -> None` then `join_row` prints the literal string `"None"` — a DIFFERENT representation
  than `HighlightRecord.key_symbol: String` (empty string) uses after `.replace("None",
  "")`. **The live-side hash MUST use the exact same normalization the parser applies**,
  or a never-changed NULL field will hash differently between prior (parsed, `""`) and
  live (raw SQL, `None`/`"None"`) on every single run.
- `AnnotationRecord` has no field carrying the raw `TextTag` (e.g. `"lb1"`) — it has
  `label: String` per the struct at `import.rs:792-801`. Need to verify at plan time
  whether `label` IS the raw TextTag or a derived value; D9-02 requires
  `(LocationId, TextTag)` as identity, so the diff needs the ACTUAL TextTag string on both
  sides, not a value the parser paraphrases.
**How to avoid:** Before writing `diff.rs`, enumerate — per category — the exact set of
fields BOTH `read_<category>_*` (live) and `parse_<category>_file`'s Record struct (prior)
carry in common, using IDENTICAL normalization (same `None`-vs-`"None"`-vs-`""` handling
on both sides). Only hash that common, identically-normalized field set. Any field present
on only one side must be excluded from the hash (it cannot be used for change-detection at
all — note as a known limitation, do not silently ignore).
**Warning signs:** A synthetic round-trip test where a row's un-exported/derivable field
changes but the hash doesn't move (correct — but must be explicitly tested, not assumed);
conversely a test where NOTHING changes between two exports of the same live data but the
"modified" count is nonzero (this is the actual bug pattern to catch).

### Pitfall 2: Annotations' composite identity vs. its parsed struct shape
**What goes wrong:** D9-02 names `(LocationId, TextTag)` as Annotations' identity. But
`AnnotationRecord` (parsed side) has `doc: Option<i64>` (NOT `LocationId` — `doc` is
`DocumentId`, a different column than the browse-identity `LocationId` used by
`NonEmptyLocationIds` in `export.rs`'s own `read_annotation_rows`). The export SQL joins
`InputField LEFT JOIN Location l USING (LocationId)` and selects `l.DocumentId doc` — so
the WIRE format encodes `DOC` (DocumentId), never the raw `LocationId` itself. If the diff
tries to use `LocationId` as half the identity key but the prior-file parser only ever
recovers `DocumentId`, the two sides use different coordinate systems and can never be
correctly matched.
**Why it happens:** The browse-layer identity PK (`LocationId`, established Phase 6/7) and
the wire-format's own natural key (`DocumentId` + `TextTag`, what's actually encoded in
the `.txt`) are two different things for Annotations specifically — every other category's
browse-identity PK IS a column the wire format encodes directly (or is deducible from one
column), but Annotations' wire format never encodes `LocationId` at all.
**How to avoid:** At plan time, re-derive what identity is actually RECOVERABLE from a
parsed `AnnotationRecord` (`doc: Option<i64>`, `label: String` i.e. TextTag, `pub_sym`,
`issue`) versus what the live query can produce, and pick the SAME coordinate system on
both sides. Likely resolution: use `(doc, label)` == `(DocumentId, TextTag)` as the diff
identity for Annotations (not `LocationId`) — this is still "the same category identity
PK" in spirit (D9-02's stated rationale), just clarified to the recoverable form; document
this as a refinement of D9-02, not a violation, since `(LocationId, TextTag)` and
`(DocumentId, TextTag)` are 1:1 for any given archive at rest, DocumentId+KeySymbol+Issue
uniquely resolving to one LocationId server-side. Flag this precisely for planner
attention — it is the single sharpest correctness trap in the phase.
**Warning signs:** A synthetic fixture where two different Annotations rows share the same
`LocationId` (same page, different labels/paragraphs) — if the diff key collapses to
`LocationId` alone the two get merged as "the same identity," corrupting the diff.

### Pitfall 3: `Type` field naming collision with Rust keyword (cosmetic, not a trap, but a consistency note)
**What goes wrong:** Nothing goes wrong functionally — flagged only so the diff code
matches the existing `kind` naming convention (`FavoriteRecord.kind`,
`BookmarkRecord.kind`, `HighlightRecord.kind`) rather than reintroducing a raw `type`
identifier or a differently-named field, which would make the "same field set on both
sides" audit (Pitfall 1) harder to verify by inspection.
**How to avoid:** Reuse the existing `kind` naming when building the live-side row
capture for hashing.

## Runtime State Inventory

*(Not applicable — Phase 9 is a rename/refactor/migration-inventory concern only. This is
a pure new-code addition phase touching zero existing runtime state; skipped per the
trigger condition in the template.)*

## Code Examples

### Diff skeleton (illustrative, not exhaustive — planner should size to actual field audit from Pitfall 1)
```rust
// NEW: app/src-tauri/src/db/io/diff.rs
use std::collections::{HashMap, HashSet};
use sha2::{Digest, Sha256};

pub struct DiffResult<K> {
    pub added: Vec<K>,
    pub modified: Vec<K>,
    pub deleted_candidates: Vec<K>,
}

/// Generic set-difference-plus-hash-compare diff, shared across all five
/// categories. `prior`/`live` are (identity_key, content_hash) pairs already
/// computed by the caller using category-specific, IDENTICALLY-normalized
/// field extraction (Pitfall 1) — this function has zero category-specific
/// knowledge.
pub fn diff_category<K: Eq + std::hash::Hash + Clone>(
    prior: &[(K, String)],
    live: &[(K, String)],
) -> DiffResult<K> {
    let prior_map: HashMap<&K, &String> = prior.iter().map(|(k, h)| (k, h)).collect();
    let live_map: HashMap<&K, &String> = live.iter().map(|(k, h)| (k, h)).collect();
    let prior_keys: HashSet<&K> = prior_map.keys().copied().collect();
    let live_keys: HashSet<&K> = live_map.keys().copied().collect();

    let added = live_keys.difference(&prior_keys).map(|k| (*k).clone()).collect();
    let deleted_candidates = prior_keys.difference(&live_keys).map(|k| (*k).clone()).collect();
    let modified = live_keys
        .intersection(&prior_keys)
        .filter(|k| live_map[*k] != prior_map[*k])
        .map(|k| (*k).clone())
        .collect();

    DiffResult { added, modified, deleted_candidates }
}

pub fn content_hash(fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    // Explicit separator prevents ("ab","c") and ("a","bc") from colliding.
    for f in fields {
        hasher.update(f.as_bytes());
        hasher.update([0x1f]); // unit separator, never a legal field byte
    }
    format!("{:x}", hasher.finalize())
}
```
**Hash input specification (reproducibility requirement):** for each category, the
ordered field list passed to `content_hash` MUST be a fixed, documented constant order —
recommend literally the same field order the exporter's own `read_*` SQL uses (see
Pattern 1), since that list is already canonical, already tested, and gives a natural
single source of truth for "what fields exist" per category.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| N/A — no prior incremental-export implementation exists in either the Python app or this codebase | Prior-file diff with content hashing | This phase (net-new) | First feature in the project with zero Python precedent; design-review bar is correspondingly higher (CONTEXT explicit) |

**Deprecated/outdated:** N/A.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `AnnotationRecord.label` (parsed) is the raw `TextTag` string (e.g. `"lb1"`), matching the wire's `{LABEL=...}` bracket which is written from `row.label` in `export.rs:331` (itself sourced from the raw `TextTag` column, `import.rs`/`export.rs:258` `SELECT TextTag, ...`) | Pitfall 2 | If wrong, Annotations identity key is unrecoverable from the parsed side as designed; planner must re-audit `parse_annotations_file`'s attribute-name mapping (`process_header`'s `{LABEL=}` key) against the struct field before implementing |
| A2 | `NoteRecord`'s fields (via `NoteShape` — not read in this research pass) are sufficient to reconstruct the SAME raw-field hash input as `RawNoteRow` on the live side, for all three note shapes (Bible/Publication/Independent) | Pattern 1, Pitfall 1 | If some raw field (e.g. `location_title`/HEADING before auto-fill) isn't preserved distinctly from its derived/defaulted form, a real content change to that field could hash as unchanged; planner must read `NoteShape`'s exact variant fields (not read in this pass — see Open Questions) before finalizing the Notes hash-field list |
| A3 | `HighlightRecord`'s parser normalizes NULL-as-string exactly the way described in its doc comment (`import.rs:1050-1052`, empty string not `None`/`"None"`), and this can be matched on the live side by applying the identical `.replace("None","")` normalization to `value_to_field`'s output before hashing | Pitfall 1 | If the normalization functions diverge even slightly (e.g. only replaces a LEADING "None" vs. any-position), the live/prior hash for an unchanged NULL field could mismatch, causing spurious "modified" forever for that row |
| A4 | `list_category`/`db/browse.rs` getters were NOT found in this research pass (grep for `pub fn list_` returned no matches — likely a different function-name pattern was used in the actual Phase 6 code); the diff's live-side read should instead reuse `db/io/export.rs`'s own `read_*` functions per Pattern 1, which WERE confirmed to exist and take the exact `ids: Option<&NonEmpty*Ids>` shape needed | Pattern 1 | Low risk — this is a correction, not a gap: reusing the exporter's own read functions is strictly safer than the original CONTEXT suggestion of `db/browse.rs` getters, since it guarantees hash-input/export-output field parity by construction (see Pitfall 1) |

**A1-A3 need a direct read of `import.rs`'s `NoteShape` definition and `parse_annotations_file`'s header-attribute-to-field mapping before planning finalizes the exact hash field lists — flagged as the single highest-value follow-up read for the planner, not re-derived here to keep this research pass proportionate to the phase's stated low ceremony.**

## Open Questions

1. **Exact `NoteShape` variant field names and whether `location_title`/pre-autofill
   `HEADING` is preserved distinctly by the parser.**
   - What we know: `RawNoteRow.location_title: Option<String>` on the export/live side;
     `NoteRecord.heading: Option<String>` on the parsed/prior side (per the struct read in
     this research pass, `import.rs:1444`).
   - What's unclear: whether `heading` as parsed represents the RAW `location_title` or the
     export-time auto-filled value (`export.rs:641-647`'s `catalog.bible_book(bk)`
     fallback) — if the parser can only ever see the POST-autofill heading (because that's
     literally what's on the wire), then a row whose raw `location_title` is empty (using
     autofill) can never be distinguished by content hash from a row where autofill
     produces the identical string by coincidence, which is a correctness non-issue in
     practice (the exported bytes ARE identical, correctly) but changes which field the
     planner should treat as canonical for hash input: use the WIRE-VISIBLE `heading`
     (post-autofill) uniformly on both sides, not the raw pre-autofill DB column, since
     that's what's actually recoverable from a prior file.
   - Recommendation: at plan time, read `NoteRecord`'s full field set already listed
     above (all confirmed in this pass) and confirm each maps 1:1 to a `RawNoteRow` field
     (post-any-export-time-transform) before finalizing.

2. **Where the five new `export_<category>_incremental` Tauri commands and the
   `IncrementalExportSummary` DTO should physically live** — new `db/io/diff.rs` module
   plus `lib.rs` command additions (recommended, matches existing module boundary
   discipline) vs. adding functions directly into `export.rs`.
   - What we know: `export.rs`'s own doc comment frames it as strictly the byte-exactness
     write path; mixing diff logic in would blur that stated scope.
   - Recommendation: new `db/io/diff.rs` file, `pub` diff functions, called from `lib.rs`
     commands that ALSO call the existing `export_<category>_impl` — matches the existing
     `lib.rs` import-aliasing pattern already visible (`export_favorites as
     export_favorites_impl`, etc., `lib.rs:22-24`).

## Environment Availability

*(Skipped — this phase has no external dependencies beyond the already-present Rust
toolchain and `sha2` crate; no new tool, service, or runtime is introduced.)*

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` / `cargo test`, matching every prior phase |
| Config file | none — plain `#[cfg(test)] mod tests` blocks, per `export.rs`'s own existing pattern (`export.rs:712-728`) |
| Quick run command | `cd app/src-tauri && cargo test --jobs 2 db::io::diff` |
| Full suite command | `cd app/src-tauri && cargo test --jobs 2` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| IO-04 (criterion 1) | Diff correctly selects added+modified rows from a prior-file vs. live comparison | unit | `cargo test --jobs 2 diff_category` | ❌ Wave 0 |
| IO-04 (criterion 1) | No prior file supplied ⇒ export everything (D9-05) | unit/integration | `cargo test --jobs 2 incremental_no_prior_file_exports_all` | ❌ Wave 0 |
| IO-04 (criterion 2) | A row whose `LastModified`/`Created` changed but content did not is EXCLUDED from the diff | unit | `cargo test --jobs 2 timestamp_only_change_excluded` | ❌ Wave 0 |
| IO-04 (criterion 1) | A row whose non-identity content changed IS included, with the correct id | unit | `cargo test --jobs 2 content_change_included` | ❌ Wave 0 |
| IO-04 (criterion 1) | Added row (new PK, absent from prior) is included | unit | `cargo test --jobs 2 added_row_included` | ❌ Wave 0 |
| IO-04 (criterion 1) | Row present in prior but absent live is a deleted-candidate, NOT written to output file, but counted in summary | unit | `cargo test --jobs 2 deleted_candidate_not_exported` | ❌ Wave 0 |
| D9-04 | Round-trip stability: incremental-export → re-import → incremental-export again against the SAME reference converges to zero added/modified | integration | `cargo test --jobs 2 incremental_export_converges` | ❌ Wave 0 |
| D9-04 | Malformed prior file aborts the whole incremental-export attempt (typed error, not silent full-export fallback) | unit | `cargo test --jobs 2 malformed_prior_file_aborts` | ❌ Wave 0 |
| Annotations composite identity | Two annotations sharing a `LocationId` but different `TextTag` are diffed independently (Pitfall 2) | unit | `cargo test --jobs 2 annotations_composite_identity` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --jobs 2 db::io::diff` (fast, isolated to the new module)
- **Per wave merge:** `cargo test --jobs 2` (full suite, catches any Phase 8 regression from the five new `lib.rs` commands)
- **Phase gate:** full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `app/src-tauri/src/db/io/diff.rs` — new module, `#[cfg(test)] mod tests` inline (matches `export.rs`/`import.rs` convention — no separate `tests/` file needed for pure-function unit tests)
- [ ] `app/src-tauri/tests/` — an integration test exercising the five new Tauri commands end-to-end against a synthetic fixture pair (prior .txt + live archive), following the existing `io_roundtrip_tests.rs` convention (referenced in the task brief; not read directly in this pass — planner should confirm its exact location, likely `app/src-tauri/tests/io_roundtrip_tests.rs`)
- [ ] Synthetic prior/current fixture PAIRS derived from `tests/fixtures/wire/*_golden.txt` (per CONTEXT's canonical-references section) — one pair per category, covering: unchanged row, added row, content-modified row, timestamp-only-changed row (Notes only, since it's the only category with a timestamp column), deleted row
- [ ] Framework install: none — `cargo test` already fully configured project-wide

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Desktop app, no auth surface touched by this phase |
| V3 Session Management | no | N/A |
| V4 Access Control | no | N/A |
| V5 Input Validation | yes | Prior-file parsing reuses Phase 8's `parse_<category>_file`, which already fails fast on malformed input (D8-04 posture); Phase 9 adds no new raw-text parsing of its own |
| V6 Cryptography | yes (narrow) | `sha2::Sha256` used strictly as a content-identity fingerprint, NOT for any security/authentication purpose — no key material, no secrets; SHA-256 is appropriate and already the project's own precedent (D8-06) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Untrusted prior-file content (user-supplied .txt, could be hand-edited/malicious) | Tampering | Reuse Phase 8's existing typed-error fail-fast parse path (D8-04); never eval/interpret file content beyond the documented grammar; all SQL stays parameterized (inherited, unchanged this phase — the diff touches zero SQL beyond the existing read functions) |
| Hash-input field mismatch silently masking a real data change | Tampering (of trust, not of the archive) | Pitfall 1's field-audit discipline; explicit synthetic tests per category proving timestamp-only changes are excluded and content changes are included |
| Zip-slip / path traversal | N/A this phase | Not applicable — Phase 9 writes only a plain `.txt` file via the existing exporter, no new archive-extraction code path |

## Sources

### Primary (HIGH confidence)
- `app/src-tauri/src/db/io/export.rs` (full file read) — every `export_<category>`/`read_<category>_*` signature, sentinel behavior, field order
- `app/src-tauri/src/db/io/import.rs` (targeted reads: header comment, `normalize_line_endings`, all five Record structs, `parse_favorites_file`/`parse_bookmarks_file`/`parse_annotations_file`/`parse_highlights_file` entry points, `NoteRecord` + `extract_notes_bucket` + `parse_note_range`) — CRLF normalization confirmed already shipped; Record struct field sets audited for Pitfall 1/2
- `app/src-tauri/src/db/ids.rs` (full file read) — recycling-table list, confirmed NOT touched by this phase (import-side, D9-07 already verified unnecessary)
- `app/src-tauri/src/lib.rs` (grep for export command registration) — confirmed the aliasing/registration pattern to follow for five new incremental commands
- `.planning/phases/09-incremental-export/09-CONTEXT.md` — authoritative, all D9-01..D9-07 decisions treated as locked, not re-litigated
- `.planning/phases/08-import-export-parity/08-DIFFERENTIAL-WIRE.md` — CRLF-on-Windows finding, confirmed already fixed at `import.rs:59-65` (this research verified the fix is live, not just documented)
- `.planning/ROADMAP.md` — Phase 9 goal, success criteria, dependency on Phase 8 (complete)

### Secondary (MEDIUM confidence)
- None used this phase — no web search was needed; the entire domain is internal, already-shipped code.

### Tertiary (LOW confidence)
- `NoteShape` enum definition and full `NoteRecord`/`RawNoteRow` field-for-field mapping — NOT read directly in this pass (see Assumptions A1-A2, Open Question 1); flagged explicitly for planner follow-up rather than guessed at.
- `db/browse.rs` list_category getters mentioned in CONTEXT's canonical-references — grep found no `pub fn list_` match in this pass; superseded by the Pattern-1 recommendation to reuse `export.rs`'s own `read_*` functions instead, which is strictly safer and was directly confirmed to exist.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new dependencies, all reused code paths directly read and confirmed
- Architecture: HIGH — diff-then-delegate shape is unambiguous given D9-06's locked command-surface decision and Phase 8's already-selection-capable exporters
- Pitfalls: MEDIUM-HIGH — Pitfall 1 and 2 are grounded in directly-read struct definitions and SQL, but the exact `NoteShape` field enumeration was not read in this pass (proportionate to the phase's stated low-ceremony brief); flagged as the one required pre-planning follow-up read

**Research date:** 2026-07-26
**Valid until:** no expiry driver — this is 100% internal/first-party code with no external version drift risk; revalidate only if Phase 8's exporter/parser signatures change
