# Phase 9: Incremental Export - Context

Gathered: 2026-07-26 (autonomous mode)
Status: Ready for planning

## Phase Boundary

Phase 8 shipped the export/import spine. Phase 9 adds a selection-computation step in
front of the existing exporters: diff the archive current state against a prior export
file, resolve which rows changed, feed those row-ids into the existing Phase 8 export
functions.

In scope (IO-04):
- A prior export point supplied as a previously-exported .txt file (D9-01).
- A diff step using Phase 8 parse_<category>_file, computing added/modified rows (D9-02, D9-03).
- Content-hash-based change detection, never comparing LastModified/Created timestamps.
- Wiring the computed id-set into the existing export_<category>(conn, Some(&ids), ...) call.
- A frontend flow: pick a prior file, get the same save-dialog output as normal export,
  plus a summary of added/modified/deleted-candidate counts (D9-04).

Out of scope:
- N-way merge fold -> Phase 10.
- Deletion representation in the .txt format -> not possible without a new format (D9-04).
- Playlist incremental export -> deferred, no phase claims it (D9-06).
- .xlsx/.md incremental export -> inherits Phase 8 D8-01 deferral.
- Any change to Phase 8 import path -> verified unnecessary (D9-07).
- Localization, theme -> Phase 11.

Requirements: IO-04 (ROADMAP Phase 9; REQUIREMENTS.md line 64).

Depends on: Phase 8 (db/io/export.rs selection-taking exporters, db/io/import.rs
parse_<category>_file parsers and typed Record structs, db/io/header.rs, golden .txt
fixtures), Phase 6 (identity PKs per category), Phase 7 (grouping-key discipline
precedent). All complete.

## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit.

### The reference point (criterion 1)

D9-01 (reference point = a prior exported .txt file the user supplies, NOT an app-stored
watermark, NOT a timestamp): Three designs weighed. (a) Timestamp/since-date -- rejected,
IO-04 criterion 2 forbids trusting vendor timestamps; a wall-clock cutoff is fooled by
clock skew and by edits that restore prior content (no real change but LastModified
moves). (b) A stored watermark/manifest inside the app (JSON sidecar recording the last
export row-hashes) -- rejected as primary mechanism: lib.rs has no existing
app_data_dir/path-resolver usage anywhere in the codebase (grep confirmed zero hits), so
this would be genuinely new persistent app state, and it breaks the moment the user
exports from a second machine or reinstalls. (c) Diff against a previously-exported .txt
file the user points at -- SELECTED. Stateless, portable, reuses Phase 8
parse_<category>_file verbatim.
[auto] reference-point design -> Selected: user-supplied prior export .txt file, diffed
via Phase 8 existing parser (recommended default)
Rationale: zero new persistent state, zero new dependency, reuses shipped Phase 8 code
unmodified, directly serves the sync-to-external-tool workflow issue #188 describes.

D9-02 (identity for the diff = the same category identity PK Phase 6/7 already
established): Notes->NoteId, Highlights->BlockRangeId, Bookmarks->BookmarkId,
Favorites->TagMapId, Annotations->(LocationId, TextTag). Since the diff compares two
exports of the SAME archive at two points in time (not two different archives -- that is
Phase 10 concern), these PKs are valid stable identity within one archive lifetime. A row
present live but absent from the prior file = added; present in both but content differs
(D9-03) = modified; present in prior file but absent live = a deletion candidate
(surfaced in the summary only, not exported -- D9-04).
Rationale: reuses an already-tested identity scheme; keeps the diff a pure lookup-and-compare.

### Content hashing (criterion 2)

D9-03 (hash the record full exported-field tuple, uniformly across all five categories,
not just Notes): ROADMAP criterion 2 wording reads Notes-specific, but the underlying
problem -- vendor timestamps unreliable -- is not Notes-specific. Only the Note table has
any LastModified/Created column at all (tests/common/mod.rs:188-189 schema confirmed; no
other category table -- Bookmark, TagMap, BlockRange, Location -- carries any timestamp
column). Since the other four categories have no timestamp signal to even mistakenly
trust, content comparison is the ONLY available mechanism for them too -- so one uniform
hash rule (over the same Record struct Phase 8 already parses) is simpler than a
Notes-only special case.
[auto] hash scope -> Selected: all five categories, uniformly (recommended default)
Rationale: a Notes-only implementation would leave the other four categories with no
change-detection mechanism at all, contradicting IO-04's plain "items changed," not "notes
changed."
Hash algorithm: reuse sha2, already a declared in-use dependency (08-RESEARCH Finding 1,
D8-06 SHA-256 media-dedup). Zero new dependency needed; no legitimacy checkpoint required.

### Wire-format compatibility (criterion 1, project constraint)

D9-04 (an incremental export is an ORDINARY .txt file containing a row subset; NOT a new
format; deletions are NOT represented): confirmed against Phase 8 exporter signatures
(db/io/export.rs:118,208,312,409,573 -- every export_<category> already takes
ids: Option<&NonEmpty<Cat>Ids>). Feeding the diff added+modified id-set into the
unmodified exporter produces a file indistinguishable in format from a normal filtered
export -- same header, same None sentinel, same pipe escaping, same asymmetric END
sentinel behavior. Python (and this app own import) reads it with zero new code. Can a
.txt file represent a deletion -- resolves to no: none of the five formats has deletion
syntax. Rather than inventing new syntax Python could never read, deletions are surfaced
to the user in the pre-export summary as an informational count but never written into
the output file. This is a disclosed limitation, not silently dropped behavior.
[auto] deletion representation -> Selected: omit from file, surface as informational
summary count (recommended default)
Rationale: matches the phase brief steer that a new format needs explicit justification;
none exists, and Python has zero mechanism to interpret any deletion marker we could
invent.

D9-05 (no prior file supplied = the incremental export IS the normal full export, not an
error): first-ever incremental export has nothing to diff against, so "changed since" is
trivially everything. Treat "no reference file" as ids = None (Phase 8 existing "no
selection = export everything" convention). Makes incremental export a strict superset of
the Phase 8 full export.
Rationale: avoids a confusing first-run dead end; falls out for free from Phase 8 existing
Option<&NonEmpty*Ids> convention.

### Command surface (criteria 1-2)

D9-06 (new commands wrap the diff and delegate to Phase 8 exporters; no changes to Phase 8
export functions themselves): recommend export_<category>_incremental(conn,
prior_file_path: Option<PathBuf>, out_path: PathBuf) -> Result<IncrementalExportSummary,
ErrorDto> per category, five commands mirroring Phase 8 per-category shape: (1) parse
prior_file_path if present via existing parse_<category>_file; (2) fetch current live rows
via Phase 6 list_category/db/browse.rs getters; (3) diff by PK + content hash into
added/modified/deleted-candidate id-sets; (4) call export_<category>(conn,
Some(&NonEmpty*Ids::try_from(added union modified)?), ...) -- the exact existing Phase 8
function, unmodified; (5) return a summary DTO with counts. Playlist is explicitly NOT
given an incremental variant this phase -- its export is a whole-database SQLite-in-zip
copy with no natural per-row diff mechanism, and no ROADMAP criterion asks for it.
Rationale: keeps "port Phase 8 exporter unmodified" literal -- new code is entirely
upstream of the existing call, never a fork of it.

D9-07 (import needs zero new code -- verified, not assumed): checked against shipped
Phase 8 import: Annotations import already upserts via ON CONFLICT (LocationId,TextTag)
DO UPDATE (08-02-PLAN.md:138 citing JWLManager.py:1941); Bookmarks/Favorites/Highlights/
Notes import via per-category location-dedup find-or-insert (D8-04) plus ID-gap recycling
(D8-08) for genuinely new rows. A file containing only added+modified rows imports
identically to how Phase 8 already handles re-importing an overlapping file. No
import-side work exists for this phase.
Rationale: directly answers the phase brief question 4; confirmed via shipped code, not
inference.

### Claude Discretion
Exact IncrementalExportSummary DTO shape (recommend added/modified/deleted_candidates
counts per category); whether the diff runs entirely in Rust (recommended) or the
frontend does file-picking only; whether IncrementalExportSummary is a new DTO or reuses
DryRunReport shape (recommend new DTO -- this is a read-only export-scope summary, not a
mutation preview); frontend placement (recommend extending the Export dialog Phase 8
already built with an incremental toggle plus file picker); hashing library choice
(recommend sha2, already linked, matching D8-06 rationale for correctness over raw speed).

## Canonical References -- downstream agents MUST read

### Existing Rust infra to REUSE (do not reinvent)
- db/io/export.rs:118-576 -- export_favorites/export_bookmarks/export_annotations/
  export_highlights/export_notes, EVERY ONE already accepting
  ids: Option<&NonEmpty<Cat>Ids> -- Phase 9 diff output plugs directly into this
  parameter, unmodified function signature.
- db/io/import.rs -- parse_favorites_file/parse_bookmarks_file/parse_annotations_file/
  parse_highlights_file/parse_notes_file and their Record structs (FavoriteRecord:51,
  BookmarkRecord:375, AnnotationRecord:766, HighlightRecord:1026, NoteRecord:1394) --
  Phase 9 reads the prior export file via a direct, unmodified call to these.
- db/io/header.rs -- export_header/ExportHeaderCtx, reused unchanged.
- db/browse.rs -- Phase 6 per-category list_category getters, the source of current live
  archive state to diff against the parsed prior file.
- sha2 crate -- already declared and in use (08-RESEARCH Finding 1, D8-06 SHA-256
  media-dedup) -- zero new dependency for D9-03 content hashing.
- Category identity PKs (Phase 6/7 established, reused verbatim per D9-02):
  Notes->NoteId, Highlights->BlockRangeId, Bookmarks->BookmarkId,
  Favorites->TagMapId, Annotations->(LocationId, TextTag).
- tests/fixtures/wire/*_golden.txt -- Phase 8 golden fixtures; Phase 9 tests should
  derive synthetic prior/current fixture PAIRS from these (never real user data).

### Source of truth for the underlying ask (not Python code -- an upstream issue)
- research/FEATURE-IDEAS.md:83 -- issue #188 (29 comments): a named user with 9,000+
  notes describes a real Obsidian-vault sync workflow and states re-exporting everything
  each time became impractical at that scale. No Python implementation of incremental
  export exists to port from -- this is a net-new feature, not a Python port, unlike every
  requirement in Phases 1-8.
- research/FEATURE-IDEAS.md:282 -- "the local content-hash manifest design sidesteps
  trusting vendor timestamps" -- origin of ROADMAP criterion 2 wording; D9-01 reads
  "manifest" here as the prior export file itself acting as the manifest, not a separate
  app-stored artifact.
- ROADMAP.md Phase 9 section -- goal and both success criteria, quoted in Phase Boundary
  above.

## Existing Code Insights
- No Python code to port for this phase. Every prior phase (1-8) cited JWLManager.py line
  ranges as the port source; Phase 9 has none -- a search for "incremental" in
  JWLManager.py returns nothing (verified during 08-CONTEXT/RESEARCH review; Python has no
  such feature). Treat all design decisions here as genuinely new, not
  ported-and-verified-against-an-oracle -- this raises the design-review bar relative to
  Phases 1-8, even though implementation risk itself is low (read-only).
- Phase 8 exporters were deliberately built id-selection-capable -- D8-10 command-surface
  decision already anticipated a selection-shaped caller beyond "export everything," which
  is exactly what Phase 9 now is. No exporter signature change needed.
- Note is the only table with any timestamp column at all (LastModified, Created,
  tests/common/mod.rs:188-189); this single fact is why D9-03 generalizes
  content-hashing to all five categories rather than special-casing Notes.

## Established Patterns
- Typed errors (ErrorDto), never unwrap/panic.
- All SQL parameterized (inherited from Phase 6 list_category, unchanged by this phase).
- Semantic parity / wire-format byte-exactness split still applies: DB reads stay
  semantic-only; exported .txt bytes remain the one place byte-exactness is the contract
  (IO-01, still true for the subset Phase 9 selects).
- Synthetic fixtures ONLY -- extends to synthetic prior/current fixture PAIRS this phase.
- No new Cargo dependency without an explicit legitimacy checkpoint -- not triggered this
  phase, sha2 already declared.

## Integration Point / risk
- Lowest risk of any phase since Phase 6. No DryRunReport/rollback envelope needed --
  export never mutates the archive: no PragmaGuard, no transaction, no filesystem
  side-effect on the archive itself. The only genuinely new risk surface is correctness
  of the diff (false negatives -- a real content change hashed as unchanged and silently
  omitted would be a real, if non-destructive, data gap for the user external sync
  workflow) -- must be tested with synthetic before/after fixture pairs covering: unchanged
  row (excluded), added row (included), a row whose non-identity fields changed but PK
  stayed the same (included, hash differs), and a row whose LastModified changed but
  content did not (must be EXCLUDED, proving the hash -- not the timestamp -- governs
  inclusion).
- Reading a user-supplied prior .txt file is untrusted external input, same class Phase 8
  already established a fail-fast posture for (D8-04) -- reuse that exact posture:
  malformed prior file = typed error, abort the whole incremental-export attempt. Do not
  silently treat an unparseable prior file as "no reference point / export everything" --
  that would silently produce a full export when the user expected an incremental one, a
  confusing silent behavior change distinct from D9-05 explicit absence of a file.

## Specific Ideas
- The diff is symmetric-set-style: added = live_pks minus prior_pks, deleted_candidates =
  prior_pks minus live_pks, modified = pk in both sets where hash(live[pk]) differs from
  hash(prior[pk]), export_ids = added union modified.
- Because Annotations identity is a composite key (LocationId, TextTag) rather than a
  single PK column, the diff key type must be generic enough to carry either a single i64
  or a tuple -- do not force a single-column-PK assumption into a shared diff helper if one
  is written (or write the composite case as its own small function; Claude Discretion).
- Consider surfacing the deleted-candidate count with an explicit caveat string in the UI
  (N items removed since your prior export are not represented in this file) rather than a
  bare number, so the D9-04 disclosed limitation is visible in the moment.

## Constraints in force (project)
- Wire formats stay byte-compatible with Python -- an incremental file is an ordinary
  filtered export, not a new format (D9-04).
- Semantic parity for DB state, byte-exactness for .txt file contents (inherited from
  Phase 8, unchanged).
- All SQL parameterized. Typed errors, never unwrap/panic.
- Synthetic fixtures ONLY -- including synthetic prior-export-file fixtures.
- No new Cargo dependency without an explicit legitimacy checkpoint -- not expected this
  phase (sha2 already declared); flag immediately if the planner finds a reason to deviate.
- Any state this phase persists (if the planner deviates from D9-01 and still wants e.g. a
  recently-used prior-export path convenience) must never be written into or read from the
  .jwlibrary archive itself -- app-local convenience state at most, never archive data.

## Deferred Ideas
- Deletion representation in the .txt wire format -> not deferred to a future phase, ruled
  genuinely out of scope for the format as it exists (D9-04); would require a new format
  decision if ever pursued, which no current requirement asks for.
- Playlist incremental export (.jwlplaylist per-item diffing) -> no phase claims this;
  would need its own design if a future requirement asks for it (D9-06).
- Any app-stored watermark/manifest design (the rejected alternative to D9-01) -> not
  revisited unless a future requirement explicitly demands cross-machine incremental
  export without a file in hand, which nothing today asks for.
- N-way merge fold (jwlCore native merge) -> Phase 10, unrelated mechanism.
- Localized dialog strings, theme -> Phase 11.

---

Phase: 9-Incremental-Export
Context gathered: 2026-07-26
