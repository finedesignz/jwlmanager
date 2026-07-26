---
phase: 9
verdict: issues_found
blockers: 1
warnings: 3
---

## ISSUES FOUND

**Phase:** 09-incremental-export
**Plans checked:** 4 (09-01 through 09-04)
**Issues:** 1 blocker, 3 warnings

### Blockers (must fix)

**1. [nyquist_compliance / Check 8e] No 09-VALIDATION.md exists for this phase**
- Plan: phase-level (all 4)
- config.json has workflow.nyquist_validation: true, and 09-RESEARCH.md has a
  Validation Architecture section with a filled-in Test Framework table,
  Phase Requirements -> Test Map, Sampling Rate, and Wave 0 Gaps -- so Dimension 8
  applies to this phase and is not skippable.
- ls .planning/phases/09-incremental-export/*-VALIDATION.md returns nothing.
  Compare phases 1, 2, 3 and 7, which each carry an NN-VALIDATION.md sibling to
  their RESEARCH.md; phase 9 (and phase 8) do not.
- Per the gate rule this is a BLOCKING FAIL and checks 8a-8d are skipped entirely
  until it is fixed.
- Fix: regenerate the missing artifact -- re-run /gsd-plan-phase 9 --research (or
  the equivalent VALIDATION.md-generation step) so 09-VALIDATION.md exists
  before execution, then re-run checks 8a-8d (automated-verify presence,
  feedback-latency, sampling continuity, Wave 0 completeness) against it. Spot
  check on the plan content itself: this is likely to pass once generated -- every
  task across all 4 plans already carries a cargo test --jobs 2 with a specific
  test name inside its automated verify block, no --watchAll or watch-mode flags
  appear anywhere, npx vitest run (never vitest alone) is used consistently, and
  Wave 0 test files (incremental_export_tests.rs, diff.rs inline tests) are
  created in the SAME task that consumes them rather than a separate prerequisite
  wave -- so the missing artifact is a process gap, not evidence of a real
  test-coverage defect.

### Warnings (should fix)

**1. [numeric/factual claim authority] Read-first line-number citations have drifted from the current file state**
- Plans: 09-01, 09-02, 09-03 (multiple read_first blocks)
- Plans cite exact line ranges against export.rs/import.rs. Live measurement
  against the current tree: parse_notes_file is at import.rs:1516 (plan
  citation exact), read_favorite_lines is at export.rs:68 (exact),
  AnnotationRecord struct in 09-01's design_resolutions is cited at
  import.rs:792-801 and is in fact at 792 (exact) -- however CONTEXT.md's
  own citations for normalize_line_endings (import.rs:43-65) are off by
  roughly +16 lines against the live file (actual import.rs:59-65). Most
  function/struct-name anchors resolve correctly by grep; a handful of
  CONTEXT.md-inherited ranges are stale.
- This is drift in supporting citations, not in the plans' own executable
  content (tasks reference function/struct NAMES, which are greppable and
  correct); it will not block execution but could cost an implementer a
  moment of confusion.
- Fix: no action required before execution; implementer should grep for the
  named function/struct rather than trusting the line number literally.

**2. [claude_md_compliance] Shell examples in verify blocks use bash chaining, not the project's stated PowerShell default**
- Plans: all 4 (every automated block, e.g. "cd app/src-tauri && cargo test --jobs 2 ...")
- Global CLAUDE.md rule 29: Shell: ALWAYS PowerShell, never bash; use PowerShell
  syntax (semicolon, not && for chaining). Every automated verify command in
  all 4 plans uses bash && chaining.
- This matches the GSD execute-plan.md workflow's own established convention
  (Phase 7/8 plans use the identical &&-chained shape) and the phase's own
  verification blocks are consistent internally, so it is not a phase-specific
  defect -- flagging as WARNING since it is a real (if pre-existing, project-wide)
  deviation from rule 29 that a strict reading would also apply to plans 1-8.
- Fix: no phase-specific action needed; if the project wants to fix this it's a
  GSD-template-level change (execute-plan.md), not a re-plan of phase 9.

**3. [task_completeness / verify command format sanity] A handful of automated commands assert full-suite green as the acceptance bar without isolating the new-test signal first**
- Plans: 09-01 Task 2, 09-02 Task 2, 09-03 Task 2 (each chains the specific new
  test name with a bare full cargo test --jobs 2 in the same automated line)
- This is intentional per the plans' own stated purpose (proving no Phase 8
  regression) and is not the swallowed-error/anchor anti-pattern flagged as
  a BLOCKER elsewhere -- no "2>/dev/null || echo" pattern and no caret-anchored
  grep over tree-formatted package-manager output appears anywhere in any of the
  4 plans. Downgraded to WARNING only because a full-suite run failing for an
  unrelated, pre-existing reason would fail the whole task's verify step rather
  than isolating which assertion broke.
- Fix: none required; acceptable as written.

### Dimension-by-dimension results

1. Proportionality (inverted check): PASS. No dry-run/apply pairs,
   PragmaGuard, DryRunReport reuse, rollback envelopes, or transactions
   anywhere in any of the 4 plans. All 4 plans' must_haves.prohibitions
   explicitly forbid introducing them. IncrementalExportSummary is
   correctly specified as a NEW DTO, not a DryRunReport reuse (09-01 Task 1).

2. Over-export invariant: PASS. 09-01's design_resolutions states the
   invariant explicitly: the exported id set is defined purely as the set of
   live records whose hash is absent from the prior hash set, and does not
   consult the identity key at all. diff_records's spec (Task 1) takes the
   exported-set decision from the hash set, not from a per-key lookup, so a
   key collision cannot suppress an export. This is carried through
   consistently in 09-02 (Highlights/Bookmarks/Favorites key-vs-hash split)
   and 09-03 (Annotations DOC+LABEL identity used only for
   added/modified labelling and LocationId grouping on the export side, never
   to gate inclusion). No plan lets an identity match suppress a record.

3. Hash-input symmetry: PASS. Notes and Annotations each extract a single
   format_category_record function used by both the exporter's write loop
   and the diff's live-side hash input (09-01 Task 1, 09-03 Task 1) -- an
   explicit key_link in both plans' frontmatter. The three flat categories
   (09-02) go further: read_cat_lines becomes a thin projection over the
   new read_cat_id_lines, so there is exactly ONE SQL column list and the
   live-side hash input is literally the same string the exporter writes,
   never a second independently-maintained field list. The prior side never
   reconstructs from a parsed struct (explicitly prohibited in 09-02's
   must_haves.prohibitions, citing the Highlights lossy-transform pitfall).

4. Timestamp exclusion: PASS. Notes strips the leading CREATED/MODIFIED
   bracket pair identically on both sides (notes_hash_input, 09-01 Task 1).
   CONTEXT.md's own investigation (D9-03) states only the Note table has
   any LastModified/Created column at all, and this claim is structurally
   consistent with 09-02/09-03's category specs, none of which mention or
   exclude any timestamp field -- correctly, since none exists on those
   wire formats.

5. CRLF: PASS. Every prior-side splitter (split_prior_note_records,
   split_prior_lines, split_prior_annotation_records) is specified to
   operate on a CRLF-normalized prior file. Plan 09-04 Task 2 adds a
   dedicated cross-category CRLF-equivalence suite explicitly tied to the
   08-DIFFERENTIAL-WIRE.md finding and normalize_line_endings, and no
   plan reads raw bytes directly into the hash without going through this
   normalization step or the parse_*_file validation gate (which itself
   normalizes).

6. No new format: PASS. All 4 plans' prohibitions forbid any deletion
   marker, tombstone, or new bracket tag; the incremental output is always
   produced by delegating to the unmodified, shipped export_category_impl
   with a computed id selection -- never a bespoke writer.

7. Deletions informational-only: PASS. deleted_candidates is counted in
   the summary DTO but never fed into the exporter's selection; 09-01's
   must_haves.truths and every subsequent plan's behavior list state a
   deleted candidate is NOT written into the output file. UI-facing
   disclosure (09-01 Task 3, generalized in 09-04 Task 1) renders an explicit
   caveat sentence, not a bare number.

8. Convergence: PASS. Each plan owns its own convergence test as a named,
   first-class task item (not an afterthought): incremental_export_converges
   (09-01), highlights_incremental_converges (09-02, with an explicit note
   that Highlights' UserMark-row-growth property from Phase 8 does NOT
   translate into spurious modified counts here since UserMarkId is not on
   the wire), annotations_incremental_converges (09-03, tied explicitly to
   the Phase 8 upsert conflict target making convergence hold).

9. Disclosed limitations: PASS. Favorites' structurally-always-zero
   modified count is asserted by a dedicated test
   (favorites_never_reports_modified, 09-02) and documented in code, not
   quietly assumed. Annotations' LocationId over-selection is disclosed via
   a distinct written-count field in the summary DTO plus a doc comment
   (09-03 Task 2) and asserted by annotations_composite_identity. Plan
   09-04 Task 3 commits all four limitations (no-deletion-representation,
   Annotations over-selection, Favorites no-modified-state, Playlists
   exclusion) to docs/incremental-export.md plus a README pointer.

10. No new Cargo dependency: PASS. Every plan's prohibitions state this
    explicitly; sha2 reuse is cited by name in CONTEXT.md, RESEARCH.md,
    and all 4 plans consistently.

11. Test commands: PASS. Every automated block uses cargo test --jobs 2
    (never bare cargo test), and the frontend suites use npx vitest run
    (never watch mode / --watchAll). Consistent across all 4 plans and
    matching 09-RESEARCH.md's Validation Architecture section.

12. Requirement coverage: PASS. IO-04 is the phase's sole requirement
    (ROADMAP.md:199, REQUIREMENTS.md:64,148) and appears in the
    requirements frontmatter of all 4 plans.

13. Task completeness: PASS. Every task across all 4 plans carries
    read_first, action, verify/automated, and acceptance_criteria/done.
    No fenced code blocks appear in any action element (verified by direct
    grep). Every plan carries its own threat_model with a populated STRIDE
    register, and every plan's frontmatter carries a separate
    must_haves.prohibitions block.

14. D9-02 refinement documented: PASS. 09-01's design_resolutions
    explicitly documents the deviation from CONTEXT's stated D9-02 (DB
    primary keys as identity) with rationale: no wire format encodes its
    category's DB primary key, so D9-02 as literally written is
    unimplementable on the prior side for every category, and identity
    must be a wire-recoverable natural key -- and states the invariant
    that makes this safe (over-export, never under-export). 09-02 and
    09-03 each carry their own explicit per-category identity-key
    specification building on this resolution, not a silent
    re-application of the original D9-02 wording.

15. Context Compliance (CONTEXT.md decisions D9-01 through D9-07): PASS.
    No plan introduces app-stored state (D9-01 respected -- prohibitions
    explicitly forbid any watermark, manifest, or recently-used-path
    state). Hash scope is uniform across all 5 categories (D9-03). No
    deletion representation is invented (D9-04). Absent prior file exports
    everything (D9-05, tested as incremental_no_prior_file_exports_all in
    every relevant plan). New commands wrap and delegate to unmodified
    Phase 8 exporters (D9-06). No import-side changes are made anywhere
    (D9-07). Deferred Ideas (N-way merge, Playlist incremental export,
    localization) do not appear in any plan's scope.

16. Scope reduction detection: PASS (none found). No "v1", "static for
    now", "future enhancement", "stub", or similar reduction language
    appears in any task action across all 4 plans. Every plan delivers its
    stated slice fully within its own scope (Notes / three flat categories
    / Annotations / UI generalization + cross-category proof + docs).

17. Dependency correctness: PASS. depends_on chain is linear and acyclic:
    09-01 (none) -> 09-02 (09-01) -> 09-03 (09-01, 09-02) ->
    09-04 (09-01, 09-02, 09-03). Wave numbers (1,2,3,4) match max(deps)+1
    in each case. No forward references.

18. Scope sanity: PASS. Each plan has 2-3 tasks (09-01: 3, 09-02: 2,
    09-03: 2, 09-04: 3), all within the 2-3 target / 4 warning threshold.
    File-modification counts per plan are all under 10.

19. Key links planned: PASS. Each plan's frontmatter key_links names the
    specific wiring point (shared formatter, id-carrying read path, UI
    handler-to-command wiring) and the corresponding task explicitly
    implements it, not merely creating the artifacts in isolation.

20. must_haves derivation: PASS. Truths are user-observable (a user gets
    a .txt containing only the notes that changed; the user sees
    added/modified/deleted-candidate counts), not implementation-detail
    phrased. Artifacts and key_links map cleanly onto the stated truths.

21. Architectural tier compliance (7c): SKIPPED -- no Architectural
    Responsibility Map section in 09-RESEARCH.md (this phase's research is
    Rust-backend/React-frontend within the app's existing two-tier shape
    already established by Phases 6-8; no new tier is introduced).

22. Cross-plan data contracts: PASS. The four plans share diff.rs and
    export.rs incrementally (each plan extends, never reverts, the prior
    plan's additions) and no plan strips/normalizes data another plan needs
    in raw form -- the flat-category plan (09-02) explicitly prohibits
    struct round-tripping for exactly this class of risk.

23. CLAUDE.md compliance: See Warning 2 above (bash chaining verify syntax
    vs. rule 29) -- pre-existing project-wide GSD template convention, not
    phase-specific; not blocking.

24. Research resolution (Dimension 11): WARNING-adjacent, not raised
    separately -- 09-RESEARCH.md's Open Questions section lacks a
    (RESOLVED) suffix and its two listed questions lack inline RESOLVED
    markers within RESEARCH.md itself. However, 09-01's design_resolutions
    block demonstrably resolves both (A1-A3 verified by direct source read
    with line citations, Open Question 1 "RESOLVED, and made moot") with
    actual evidence, and 09-02/09-03 build on those resolutions
    consistently. This is a documentation-location gap (the resolution
    belongs in RESEARCH.md per the dimension's letter) rather than an
    actual unresolved question reaching execution. Folded into Warning 1's
    fix (regenerating phase artifacts) rather than raised as a fourth
    separate warning.

## Recommendation

One BLOCKER: 09-VALIDATION.md is missing and nyquist_validation is enabled
with an applicable RESEARCH.md Validation Architecture section, which makes
this a hard gate per Dimension 8 Check 8e regardless of how strong the
plans' own verify blocks already are. Every other dimension -- including
the two hardest phase-specific correctness properties (the over-export
invariant and Annotations' composite identity) -- passes cleanly with no
scope reduction and full CONTEXT.md decision coverage.

Fix path: regenerate 09-VALIDATION.md (re-run the research/validation-
generation step for phase 9), then re-run Dimension 8 checks 8a-8d against
it. No plan content changes are expected to be needed based on the
automated commands already present in all 4 plans.
