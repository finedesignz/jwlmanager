# Phase 10: N-Way Merge Fold - Context

Gathered: 2026-07-26 (autonomous mode)
Status: Ready for planning

## Phase Boundary

A user with 3+ .jwlibrary archives to reconcile picks all of them plus a destination
session and merges them in ONE ordered operation instead of chaining Phase 5's
two-archive merge by hand N-1 times. Under the hood this IS that chain -- Phase 10 adds no
new native-merge semantics, only the orchestration, aggregate dry-run, and safety envelope
around calling Phase 5's already-shipped stage_and_merge / merge_commit_with_lib_path
repeatedly.

In scope (MERGE-03):
- Frontend: select 3+ source archives (existing archive-picker pattern, list not single-file).
- An ordered fold: dest = merge(merge(merge(dest, src1), src2), src3), each step reusing
  Phase 5's jwlcore::merge::run_merge_with_lib_path unmodified.
- Aggregate dry-run: ONE preview comparing original session state to the final folded
  state (not N-1 separate previews) -- added/overwritten/deleted per table, cumulative.
- User-controllable fold order (surfaced as the picked-list order; reordering is a normal
  list-reorder UI affordance, not new merge logic).
- Deterministic execution: same input list + same order always produces the same result
  (jwlCore itself is deterministic per Phase 5; Phase 10 adds no nondeterminism).
- Failure-at-step-k handling: the live session and all N source archives are untouched;
  no partially-folded archive is ever promoted or presented as complete.
- Typed error surface: reuse ArchiveError::MergeUnavailable / MergeFailed verbatim; a
  step-k failure reports which source (1-indexed) failed.
- Round-trip verification: N-way fold result matches performing the equivalent N-1
  pairwise merges via Phase 5's own commit path, on the same inputs, same order.
- Closing Phase 5's recorded playlist-coverage gap using Phase 8's now-real playlist
  fixtures (see D10-06).

Out of scope (own phases / deferred):
- Any change to Phase 5's mergeDatabase FFI call, stage_and_merge, snapshot-signature
  diff, or atomic-promote primitives -- Phase 10 is a caller, never a fork.
- Live percentage progress across the fold (D10-05) -- same MVP posture as Phase 5 D5-05.
- Automatic conflict resolution beyond what jwlCore already does per pairwise step --
  no new merge policy (e.g. "prefer newest," "prefer archive A") is introduced.
- Non-ordered / commutative fold guarantee -- order is user-visible and preserved, not
  hidden (D10-01).
- Downgrade-during-fold -- inherits Phase 5 D5-08's deferral (downgrade=false throughout).
- Localization, theme -> Phase 11.

Requirements: MERGE-03 (ROADMAP Phase 10; REQUIREMENTS.md line 35, 128).

Depends on: Phase 5 (jwlcore/merge.rs FFI wrapper, archive/merge.rs orchestration --
stage_and_merge, dry_run_merge(_with_lib_path), merge_commit(_with_lib_path),
content_diff, snapshot_signatures/diff_signatures, fold_back_media,
archive::save::atomic_replace, MERGE_SNAPSHOT_TABLES). Complete and unmodified by
this phase. Also draws on Phase 8 (real playlist import/export + fixtures, closing the
Phase 5 playlist gap -- D10-06) and Phase 9 precedent (content-hash-based, never
timestamp-based, change detection -- same posture reused for the aggregate diff).

## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit.

### Central question: fold semantics (criterion 3)

D10-01 (N-way merge is a SEQUENTIAL ORDERED FOLD over Phase 5's pairwise merge, NOT a new
associative/commutative merge algorithm; order is preserved and user-visible, not
normalized away): verified against the shipped code, not assumed. jwlCore.mergeDatabase
does full-column content-signature overwrites in place (archive/merge.rs:17-24 module
docs: "jwlCore does not only ADD rows -- it also UPDATES matched rows IN PLACE"). This means
fold(A,B,C) and fold(A,C,B) can genuinely diverge whenever B and C both touch the SAME
identity key (e.g. both archives have a Note with the same Guid, edited differently, or a
UserMark on the same Location) -- the later step in the fold order wins that row's content,
exactly like the last merge in a hand-chained pairwise sequence would. This is NOT a defect
to engineer around: it is the same behavior a user chaining Phase 5 merges by hand already
gets, and the Python app has no N-way merge to compare against (jwlCore's algorithm is a
compiled binary with no documented associativity guarantee -- verified there is no
source in this repo per CLAUDE.md, and jwlcore.py/JWLManager.py never call
merge_databases more than once per user action). Therefore: the fold is order-sensitive
BY DESIGN, order is exactly the user's picked-list order (top to bottom = fold sequence),
and the UI must make order visibly controllable (reorder = normal list drag/reorder, not a
new decision surface) rather than silently sorting or deduplicating.
[auto] fold semantics -> Selected: sequential ordered fold, order = user list order,
divergence under reordering is expected and disclosed (recommended default)
Rationale: matches ROADMAP criterion 3 literally ("verified by round-trip test" against
"the equivalent sequence of pairwise merges" -- the ROADMAP text itself assumes order-
sensitivity, it does not ask for order-independence). Inventing an order-independent merge
algorithm would be new, unverified merge semantics layered on top of a black-box binary --
directly against the "no new merge algorithm" boundary and against the project's oracle-
verification posture (nothing to verify a novel algorithm against).

D10-02 (round-trip test proves fold == chained pairwise commits, same order, not fold ==
fold under any permutation): criterion 3 says "matches performing the equivalent sequence
of pairwise merges" -- read literally this is fold(A,B,C) via Phase 10's own orchestration
must equal calling Phase 5's merge_commit_with_lib_path three times by hand in the SAME
order (A-then-B, then result-then-C), compared via normalized table state (never
byte-diff, inherited constraint). This is a strictly weaker and more honest claim than
"any order gives the same answer," and is exactly what D10-01 predicts should hold (the
fold literally calls the same primitive the same number of times in the same order).
Rationale: directly falsifiable against Phase 5's own shipped commit function; does not
require inventing a new oracle.

### Intermediate state and failure isolation (criterion 1, Core Value)

D10-03 (fold operates entirely on a chain of STAGING copies under session.temp_dir;
originals and the live session are read-only until the FINAL atomic promote; a step-k
failure aborts the whole operation with zero mutation to anything the user can see):
mirrors Phase 5 merge_commit_with_lib_path exactly but chains N-1 stage_and_merge calls
before the single promote. Layout: session.temp_dir/fold_staging/step_0/userData.db
(seeded via fs::copy(session.db_path, ...), same as Phase 5's staging seed), then for each
source i in order: stage_and_merge(lib_path, session, source[i], step_i_dir) where
step_i_dir's userData.db is first copied from step_(i-1)'s result (Phase 5's
stage_and_merge always copies FROM session.db_path -- Phase 10 generalizes this one call
site to copy FROM the previous step's staged DB instead, everything else in
stage_and_merge reused verbatim). Every source[i] archive is only ever READ (extracted
into step_i_dir/merge/, never written) -- same MERGE-02 guarantee Phase 5 already proves.
On ANY step's MergeFailed/MergeUnavailable, abort immediately: do not attempt remaining
steps, do not promote, best-effort fs::remove_dir_all(fold_staging) (mirrors Phase 5's
"cleanup on every path"), and surface which 1-indexed source failed plus jwlCore's
getLastResult() reason (internal-only, generic DTO message per D-14). ONLY after step N-1
succeeds does a SINGLE atomic promote run: archive::save::atomic_replace(final_step_db,
session.db_path) -- one rename, same all-or-nothing guarantee Phase 5's merge_commit
already relies on (same filesystem, same-kernel-call atomicity). session.dirty = true only
after that promote succeeds.
[auto] intermediate state -> Selected: chained staging copies under session.temp_dir,
single final atomic promote, abort-with-cleanup on any step failure (recommended default)
Rationale: this is the direct, minimal generalization of Phase 5's own commit function
(N-1 sequential stage_and_merge calls instead of 1) -- reuses the exact atomic-promote
primitive Phase 5 already proved correct under test, adds no new promote mechanism, and
keeps the Core Value guarantee (never a partially-merged archive presented as complete)
by never touching session.db_path until the very last step succeeds.

D10-04 (fold_back_media generalizes the same way -- run once per completed step, on that
step's staging dir, not only on the final step): Phase 5's fold_back_media walks a staging
dir for non-userData.db, non-merge/ files and reconciles them into session.entries.
Default: run fold_back_media after EVERY step against that step's own staging dir (not
just the last), each time using the now-standard already-present-content-compare rule, so
media written at step 2 is not silently lost if step 3's staging dir is seeded fresh from
step 2's DB only (not its sibling files). This is the conservative reading given Phase 5's
own "empirically a no-op on this host's fixtures" caveat -- do not assume no future/host
jwlCore writes media at every merge, only that it did not on the fixtures tested so far.
[auto] media fold-back -> Selected: run fold_back_media after every fold step, not only
the last (recommended default, conservative)
Rationale: Phase 5's own module docs flag this as an empirical (not guaranteed) observation
on one host/fixture combination; a fold that seeds each step's DB from the previous step
but drops sibling media between steps would introduce a genuine data-loss regression Phase
5 never had (Phase 5 has only one step, so this failure mode literally cannot occur there).

### Aggregate dry-run (criterion 2)

D10-05 (dry-run shows ONE aggregate cumulative report -- final folded state vs. original
session state -- not N-1 per-step previews): reuses Phase 5's content_diff(before_db,
after_db) UNMODIFIED -- before_db = the live session DB (read-only snapshot, same as
Phase 5), after_db = the FINAL throwaway fold chain's last staged DB. The fold runs the
SAME staging-chain logic as commit (D10-03) but under a throwaway root that is discarded
after diffing, exactly mirroring Phase 5's dry_run_merge vs merge_commit split (same
stage_and_merge-derived chain, throwaway vs. staging root is the only difference,
inherited verbatim from Phase 5's existing pattern). overwritten therefore reports the
FINAL content-signature state per PK, so a row overwritten at step 2 then overwritten AGAIN
at step 3 counts once (as one overwrite, showing the step-3 content) -- matching what the
user actually cares about ("what will my archive look like"), per the phase brief's own
steer. Per-step breakdown is NOT surfaced in MVP (Claude's Discretion: could be an
expandable detail row later, not required by criterion 2's "cumulative effect" wording).
[auto] dry-run granularity -> Selected: single aggregate before/after diff over the whole
fold, reusing content_diff unmodified (recommended default)
Rationale: directly answers the phase brief's question 3 with "aggregate, because Phase
5's content_diff already computes exactly disjoint added/overwritten/deleted between any
two DB states regardless of how many operations produced the after-state" -- zero new diff
code needed, same reasoning Phase 9 used to reuse Phase 8 exporters unmodified.

### Playlist coverage gap (criterion 3, quality bar)

D10-06 (build a full valid playlist graph fixture for the round-trip test, closing Phase
5's recorded deferral, using Phase 8's now-real playlist infrastructure): Phase 5
VERIFICATION.md recorded that a MINIMAL synthetic PlaylistItem aborts jwlCore's merge
("key not found: 0") because it needs a fuller graph (thumbnail/IndependentMedia/markers/
maps) than a minimal fixture reproduces. Phase 8 has since shipped real playlist import/
export (db/playlist_io.rs, tests/playlist_import_tests.rs, tests/
playlist_export_tests.rs) with fixture-construction helpers for exactly this graph shape
(res_blank_playlist_path and siblings). Phase 10's round-trip test (D10-02) is the first
place in the codebase that NEEDS a multi-step merge exercised against playlist tables at
all (Phase 5 only had one merge step to test), so building a full playlist fixture here --
reusing Phase 8's helpers rather than reinventing fixture construction -- both serves this
phase's own test needs AND closes the Phase 5 gap in the same commit. If jwlCore still
aborts on a full graph fixture (i.e. the gap is deeper than "needs more fields"), document
that finding transparently in this phase's VERIFICATION.md exactly as Phase 5 did -- do NOT
claim closed if it empirically still fails.
[auto] playlist gap -> Selected: attempt closure using Phase 8 fixtures as part of this
phase's own round-trip test; document honestly if still blocked (recommended default)
Rationale: the phase brief explicitly asks whether Phase 8 changes what's testable, and it
does -- this is a natural byproduct of building the round-trip harness this phase needs
anyway, not extra scope; explicitly bounded so a still-failing jwlCore playlist merge is
reported, not silently reattempted with unlimited effort.

### Scale and progress (criterion 1)

D10-07 (no hard-coded archive-count ceiling in code; UI/UX guidance notes practical scale;
no live progress bar, same MVP posture as Phase 5 D5-05): N-1 sequential native merge
calls, each a full DB copy + extraction + mergeDatabase invocation, run synchronously on
the Tauri command worker thread (same as Phase 5 -- the WebView stays responsive because
Tauri commands run off the UI thread, not because of any new async work here). For hobby-
scale archives (the project's stated bandwidth) and a "handful" of devices (the documented
multi-device pain point), N is realistically single digits -- no enforced maximum, but the
frontend surfaces a simple busy/spinner state for the whole fold duration (extending Phase
5's existing busy-state pattern to cover N-1 steps as one operation) rather than a
step-by-step progress bar. setProgressCallback remains unwired, per Phase 5 D5-05's
rationale (process-global callback state, extra unsafe surface, cosmetic gain) -- this
carries forward unchanged rather than being reconsidered, since Phase 10 adds N calls to
the SAME already-declined mechanism, not a new one.
[auto] scale/progress -> Selected: no enforced count ceiling, single busy-state spinner
for the whole fold, setProgressCallback still unwired (recommended default)
Rationale: consistent with Phase 5's already-accepted MVP tradeoff; introducing live
progress now would be new scope unrelated to MERGE-03's actual criteria, none of which
mention progress reporting.

### Claude's Discretion
Exact command surface (recommend dry_run_fold_merge(app, session, source_paths:
Vec<PathBuf>) -> Result<DryRunReport, ErrorDto> and fold_merge_commit(app, session,
source_paths: Vec<PathBuf>) -> Result<(), ErrorDto>, mirroring Phase 5's dry_run_merge/
merge_commit naming); whether the per-step fold_back_media walk (D10-04) is provably
unnecessary for intermediate steps once implementation reveals jwlCore's actual write
pattern across a real 3-archive fixture (if genuinely never observed at intermediate
steps across a manual test run, may simplify to last-step-only with the same honest-
disclosure posture Phase 5 used -- but default conservative unless disproven); exact
staging directory naming under session.temp_dir/fold_staging/; minimum archive count
enforced at the command boundary (recommend 3 or more, since 1-2 archives already have
Phase 5's own two-archive merge and single-open flows -- reject 0-2 with a typed
validation error rather than silently degrading to a Phase-5-equivalent call); frontend
reorder-list UI shape (reuse Phase 5's picker + add a standard list-reorder affordance);
whether the round-trip test's chained-pairwise-commit comparison (D10-02) needs its own
throwaway session copies to avoid mutating the fold's own test fixtures.

## Canonical References -- downstream agents MUST read

### The primitives this phase folds (reuse verbatim, do not fork)
- app/src-tauri/src/jwlcore/merge.rs:119-159 -- run_merge_with_lib_path, the single FFI
  call site; called once per fold step, unmodified.
- app/src-tauri/src/archive/merge.rs:203-216 -- stage_and_merge (copy dest DB, extract
  source zip-slip-safely, invoke merge); the one call site to generalize so its DB-copy
  source can be the PREVIOUS fold step's output rather than always session.db_path
  (D10-03).
- app/src-tauri/src/archive/merge.rs:224-234 -- content_diff (before/after content-
  signature diff into DryRunReport); reused unmodified for the aggregate dry-run (D10-05).
- app/src-tauri/src/archive/merge.rs:253-305 -- dry_run_merge/dry_run_merge_with_lib_path
  (throwaway-copy preview pattern) -- Phase 10's dry-run generalizes this loop structure to
  N-1 steps.
- app/src-tauri/src/archive/merge.rs:326-371 -- merge_commit/merge_commit_with_lib_path
  (staging-copy + fold_back_media + atomic promote) -- Phase 10's commit generalizes this
  loop structure to N-1 steps, single final promote.
- app/src-tauri/src/archive/merge.rs:389-413 -- fold_back_media -- reused per-step per
  D10-04.
- app/src-tauri/src/archive/save.rs -- atomic_replace (rename-with-replace); the SAME
  single promote call at the end of the fold, never per-step.
- app/src-tauri/src/archive/extract.rs -- extract_zip_slip_safe; used once per source
  archive in the fold, unchanged.
- app/src-tauri/src/error.rs -- ArchiveError::MergeUnavailable / MergeFailed { reason };
  reused verbatim; Phase 10 adds no new error variant beyond identifying which 1-indexed
  source failed (Claude's Discretion whether that needs a new field or is folded into the
  existing reason string).
- app/src-tauri/src/session.rs -- ArchiveSession (temp_dir, db_path, entries, dirty),
  ZipEntryMeta.

### Verified deferral this phase closes or re-documents
- .planning/phases/05-two-archive-merge/VERIFICATION.md:10,75 -- the recorded playlist-
  table merge coverage gap (D10-06). Read before writing the round-trip fixture.
- .planning/phases/08-import-export-parity (SUMMARY/CONTEXT) and
  app/src-tauri/src/db/playlist_io.rs, app/src-tauri/tests/playlist_import_tests.rs,
  app/src-tauri/tests/playlist_export_tests.rs -- the real playlist fixture-construction
  helpers to reuse for D10-06, rather than hand-building a new minimal fixture.

### Source of truth for the underlying ask
- ROADMAP.md Phase 10 section (.planning/ROADMAP.md:214-226) -- goal and all 3 success
  criteria, quoted in Phase Boundary above.
- REQUIREMENTS.md line 35 (MERGE-03) -- "User can merge N archives in one operation
  (ordered fold)"; the word "ordered" is itself the strongest textual evidence for D10-01.

## Existing Code Insights
- jwlCore.mergeDatabase is confirmed (not assumed) to perform in-place content UPDATEs
  at matched PKs, not pure additive merging -- this is the entire basis for D10-01's
  order-sensitivity conclusion, documented in Phase 5's own module docs
  (archive/merge.rs:17-24), independently re-confirmed here for Phase 10's central
  question.
- Every Phase 5 primitive that Phase 10 needs already exists in a form parameterized
  enough to generalize by LOOPING the call site, not by rewriting the primitive -- the
  lib_path core split (_with_lib_path functions) that Phase 5 built for its own
  integration tests is exactly the shape Phase 10's fold loop calls repeatedly.
- No Python code exists to port for N-way merge (same posture as Phase 9) -- JWLManager.py
  calls merge_databases at most once per user action (JWLManager.py:2672); N-way fold
  is a genuinely new orchestration layer over an existing native primitive, not a port.

## Established Patterns
- Typed errors (ArchiveError/ErrorDto), never unwrap/panic on the archive-data path.
- Source archives never mutated -- every fold input is read-only throughout (extends the
  Phase 5 single-source guarantee to N sources).
- Semantic (normalized-table) parity, NEVER byte-diff, for the round-trip verification.
- Atomic promote via fs::rename-with-replace, never fs::copy, onto the live DB -- the
  ONE promote at the end of the fold, never an intermediate promote.
- All SQL parameterized; MERGE_SNAPSHOT_TABLES stays a fixed compile-time identifier list.
- Synthetic fixtures ONLY, including the new playlist graph fixture (D10-06).

## Integration Point / risk
- Order-sensitivity is the phase's central risk, not a defect to eliminate (D10-01). The
  planner and implementer must not "fix" divergence under reordering -- it is correct
  behavior matching hand-chained Phase 5 merges. The UI must make fold order visible and
  controllable, and the round-trip test must compare fold-vs-chained-pairwise IN THE SAME
  ORDER, never assert order-independence.
- Chain generalization of stage_and_merge's DB-copy source is the one real code change
  inside Phase 5's primitives (D10-03) -- every fold step after the first must seed from the
  PREVIOUS step's merged DB, not from session.db_path again (that would silently drop
  every earlier fold step's effect). Get this wrong and the fold degrades to "merge only the
  last source," a subtle, high-consequence bug -- cover it explicitly with a 3-archive test
  asserting ALL THREE sources' unique rows are present in the final result, not just the
  last one.
- Media fold-back across steps (D10-04) is unverified territory -- Phase 5 only ever
  observed one step, so "no media written" was proven for N=1, not for N>1. Do not assume
  the Phase 5 empirical no-op generalizes without testing a fold that touches
  media-bearing categories (playlists again -- ties to D10-06).
- Failure-at-step-k must leave zero visible trace -- this is the Core Value's sharpest
  edge in this phase: an N-way operation has N-1 chances to fail partway, and the user must
  never see (or have promoted) a "merged 2 of 3" archive. Test explicitly: force a step-2
  failure (e.g. a corrupt/invalid source at position 2) and assert session.db_path is
  byte-identical to its pre-fold state and session.dirty is unchanged.

## Specific Ideas
- Command surface: two Tauri commands mirroring Phase 5's merge_* pair but taking
  Vec<PathBuf> instead of a single PathBuf; frontend reuses the Phase 5 picker/dry-run
  dialog components extended for a list.
- The round-trip test (D10-02) is naturally two test bodies sharing one fixture setup: (a)
  fold A,B,C via the new N-way commands; (b) commit A-then-B via Phase 5's
  merge_commit_with_lib_path, then commit that result-then-C via the same function again;
  compare normalized table state between (a) and (b) -- this IS the criterion 3 test, not
  a new oracle.
- Consider a small helper fold_stage_and_merge(lib_path, prev_step_db, source_archive,
  step_dir) that wraps stage_and_merge with the generalized copy-source (D10-03) so the
  N-1 loop body is one call, keeping archive/merge.rs's existing functions untouched and
  additive-only (new function, not a rewrite) -- reduces risk of regressing Phase 5's own
  tests.

## Constraints in force (project)
- Never lose or corrupt a user's archive (Core Value) -- sharpest here: N-1 chances to fail,
  zero tolerance for a partially-folded archive being promoted or presented as complete.
- Semantic parity verified on normalized table state, NEVER byte-diff.
- Atomic promote via fs::rename, never fs::copy, for the single final promote.
- All SQL parameterized. Typed errors, never unwrap/panic.
- Synthetic fixtures ONLY -- including the new playlist graph fixture.
- No new Cargo dependency without an explicit legitimacy checkpoint -- none anticipated;
  every primitive Phase 10 needs already exists in Phase 5's shipped code.
- MIT licence -- jwlCore binary only, no jwlFusion/NOASSERTION source ingestion.

## Deferred Ideas
- Live percentage progress across fold steps (setProgressCallback wiring) -> post-MVP,
  same deferral as Phase 5 D5-05, now compounded across N steps but not reconsidered.
- Automatic conflict-resolution policy beyond jwlCore's own per-step behavior (e.g.
  "newest wins" across the whole fold regardless of order) -> not requested by MERGE-03,
  would be new merge semantics needing its own design and oracle.
- Order-independent / commutative fold guarantee -> explicitly rejected as a goal (D10-01),
  not merely deferred -- would require inventing new merge semantics on top of a black-box
  binary with no source in this repo.
- Downgrade-during-fold -> inherits Phase 5 D5-08's open deferral, unchanged.
- Localized dialog strings, theme -> Phase 11.

---

Phase: 10-N-Way-Merge-Fold
Context gathered: 2026-07-26
