---
phase: 10-n-way-merge-fold
plan: 01
subsystem: database
tags: [rust, tauri, sqlite, jwlcore, merge, ffi]

requires:
  - phase: 05-two-archive-merge
    provides: stage_and_merge, dry_run_merge_with_lib_path, merge_commit_with_lib_path, content_diff, fold_back_media, atomic_replace
provides:
  - stage_and_merge_from (generalized copy-source primitive, session.db_path parameterized to any copy_from path)
  - run_fold_chain (shared N-way fold loop, step-indexed error wrapping, per-step callback)
  - fold_dry_run_merge / fold_dry_run_merge_with_lib_path (aggregate dry-run, session-unaffected)
  - fold_merge_commit / fold_merge_commit_with_lib_path (N-1 staged merges, single atomic promote)
  - fold_merge_dry_run / fold_merge_commit Tauri commands
affects: [10-02-failure-envelope, 10-03-ui]

tech-stack:
  added: []
  patterns:
    - "Shared fold-chain function (run_fold_chain) driven by both the dry-run and the commit, so aggregate preview cannot diverge from the committed effect (mirrors Phase 5's stage_and_merge preview==commit guarantee, generalized to N steps)."
    - "Step-indexed MergeFailed reason wrapping (`source {i} of {n}: {inner}`) instead of a new error variant, preserving the D-14 no-leak DTO mapping."

key-files:
  created:
    - app/src-tauri/tests/fold_merge_tests.rs
  modified:
    - app/src-tauri/src/archive/merge.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/common/mod.rs

key-decisions:
  - "stage_and_merge_from generalizes the Phase 5 copy-source primitive by parameterizing the hardcoded session.db_path into a copy_from argument; stage_and_merge becomes a one-line delegation so every Phase 5 call site and test is untouched and bit-identical."
  - "run_fold_chain tracks prev_step_db explicitly rather than recomputing a path formula, so step i>1 is provably seeded from step (i-1)'s own userData.db and never re-reads session.db_path (the highest-consequence bug this plan guards against)."
  - "Media fold-back runs after EVERY completed fold step (D10-04), not just the final one, so an intermediate step's media is never dropped by the next step's re-seed."
  - "Exactly one atomic_replace call, placed after run_fold_chain returns Ok, outside the loop — verified both by code inspection and by the fold_rejects_fewer_than_three_sources test asserting no staging directory exists after a rejected call."
  - "New test fixtures (generate_fold_standalone_source_archive, generate_fold_contested_pair) were added to tests/common/mod.rs even though the plan's files_modified list only named merge.rs/lib.rs/fold_merge_tests.rs — the plan's Task 2 behavior spec required a THREE-source set with a genuinely contested identity key across two sources, which no existing common/mod.rs fixture provided. Treated as a Rule 3 blocking-issue fix (test infra needed to write the criterion tests at all)."

requirements-completed: [MERGE-03]

coverage:
  - id: D1
    description: "N-way fold runs as one backend operation in caller order (dest = merge(merge(merge(dest,s1),s2),s3)); every source contributes (copy-source regression guard)"
    requirement: "MERGE-03"
    verification:
      - kind: integration
        ref: "app/src-tauri/tests/fold_merge_tests.rs#fold_merge_carries_all_sources"
        status: pass
    human_judgment: false
  - id: D2
    description: "fold(A,B,C) produces the same normalized table state as chained-pairwise merge_commit_with_lib_path calls in the SAME order; contested identity resolves to the later source (order-sensitivity proven, not papered over)"
    requirement: "MERGE-03"
    verification:
      - kind: integration
        ref: "app/src-tauri/tests/fold_merge_tests.rs#fold_matches_chained_pairwise"
        status: pass
    human_judgment: false
  - id: D3
    description: "Aggregate dry-run returns one report matching the committed effect; live session DB is unchanged by the dry-run"
    requirement: "MERGE-03"
    verification:
      - kind: integration
        ref: "app/src-tauri/tests/fold_merge_tests.rs#fold_dry_run_aggregate"
        status: pass
    human_judgment: false
  - id: D4
    description: "Fewer than 3 sources is rejected with MergeFailed before any staging/dry-run directory is created"
    requirement: "MERGE-03"
    verification:
      - kind: integration
        ref: "app/src-tauri/tests/fold_merge_tests.rs#fold_rejects_fewer_than_three_sources"
        status: pass
    human_judgment: false
  - id: D5
    description: "fold_merge_dry_run / fold_merge_commit Tauri commands registered, mirroring merge_dry_run/merge_commit's error-mapping wiring; shipped merge_dry_run/merge_commit unchanged"
    verification:
      - kind: unit
        ref: "cargo build --jobs 2 (generate_handler! includes fold_merge_dry_run, fold_merge_commit)"
        status: pass
    human_judgment: false

duration: ~55min
completed: 2026-07-26
status: complete
---

# Phase 10 Plan 01: N-Way Merge Fold (Tracer) Summary

**A shared `run_fold_chain` loop generalizes Phase 5's pairwise merge into an N-source fold — `dest = merge(merge(merge(dest,s1),s2),s3)` in caller order, one atomic promote, proven equal to hand-chained Phase 5 commits in the same order on the real jwlCore DLL.**

## Performance

- **Duration:** ~55 min
- **Tasks:** 3 completed
- **Files modified:** 3 (archive/merge.rs, lib.rs, tests/common/mod.rs), 1 created (tests/fold_merge_tests.rs)

## Accomplishments

- Generalized `stage_and_merge` into `stage_and_merge_from(lib_path, copy_from, source_archive, root)` — the copy-source is now a parameter instead of a hardcoded `session.db_path`, with `stage_and_merge` reduced to a one-line delegation so the shipped Phase 5 `merge_orchestration.rs` suite passes unedited.
- Added the shared `run_fold_chain` primitive: creates `root/step_1..step_N`, seeds step 1 from `session.db_path` and every later step from the PREVIOUS step's own `userData.db` (tracked explicitly as `prev_step_db`, never recomputed via a formula that could accidentally re-read `session.db_path`), calls a per-step callback, wraps `MergeFailed` reasons with the 1-indexed source position on error, and returns the final step's DB path on success.
- Built `fold_dry_run_merge_with_lib_path` and `fold_merge_commit_with_lib_path` on top of the SAME `run_fold_chain` call, so the aggregate preview and the committed result cannot diverge (generalizes Phase 5's preview==commit guarantee to N steps). The commit performs media fold-back after every step (D10-04) and exactly one `atomic_replace` after the final step, setting `session.dirty = true` only after that succeeds.
- Added `require_fold_sources` rejecting fewer than 3 sources with a typed `MergeFailed` before any directory is created; both fold entry points call it first.
- Registered `fold_merge_dry_run` / `fold_merge_commit` Tauri commands mirroring the shipped `merge_dry_run`/`merge_commit` wiring (state lock, `StatePoisoned`, `MissingUserDataBackup`, `to_dto` mapping) — source order passed through untransformed.
- Wrote four criterion tests in `tests/fold_merge_tests.rs`, all RAN (not skipped) against the real `jwlCore-amd64.dll` on this host: `fold_merge_carries_all_sources`, `fold_matches_chained_pairwise` (with an explicit order-sensitivity assertion — the contested note's final content is the LATER source's, never the earlier one's, with a comment noting a reverse/order-independence assertion would test the wrong property), `fold_dry_run_aggregate`, `fold_rejects_fewer_than_three_sources`.

## Task Commits

1. **Task 1: The fold chain — generalized copy source, shared loop, aggregate dry-run, single promote** - `5dda970a` (feat)
2. **Task 2: The two criterion tests — all sources carried, and fold == chained pairwise in order** - `bb285ce5` (test)
3. **Task 3: Tauri command surface for the fold** - `ebf40eb2` (feat)

_Note: this plan mixed feat/test task types per its own task-type annotations (Task 1 = tracer/feat, Task 2 = auto/tdd test, Task 3 = auto/feat); no separate red/green/refactor TDD gate applies since Task 2 is a batch of integration tests against already-passing production code from Task 1, not a red-first unit cycle._

## Files Created/Modified

- `app/src-tauri/src/archive/merge.rs` - `stage_and_merge_from`, `run_fold_chain`, `require_fold_sources`, `fold_dry_run_merge[_with_lib_path]`, `fold_merge_commit[_with_lib_path]`; `stage_and_merge` reduced to a delegation; +1 unit test (`require_fold_sources_rejects_fewer_than_three`)
- `app/src-tauri/src/lib.rs` - `fold_merge_dry_run`, `fold_merge_commit` Tauri commands, registered in `generate_handler!`
- `app/src-tauri/tests/common/mod.rs` - `generate_fold_standalone_source_archive`, `generate_fold_contested_pair` + supporting const identities and row-seeders (fold-specific test fixtures)
- `app/src-tauri/tests/fold_merge_tests.rs` - the four criterion tests (new file)

## Decisions Made

- **`stage_and_merge_from` parameterization is purely additive** — no behavior change to any Phase 5 call site; verified by running `merge_orchestration.rs` unedited before writing any fold-specific code (all 5 tests passed).
- **`prev_step_db` is tracked as an explicit local, not recomputed** from a path formula on each iteration — this is the concrete mechanism that prevents the "step i>1 re-seeds from `session.db_path`" bug the plan calls out as the phase's highest-consequence subtle failure mode.
- **Error wrapping applies uniformly to both the merge-staging step and the per-step callback** (`wrap_step_reason` used at both call sites in `run_fold_chain`) rather than only the merge call, so a media-fold-back failure at step 2 also correctly reports `"source 2 of 3: ..."`.
- **New fold-specific test fixtures were added to `tests/common/mod.rs`** rather than duplicated inline in `fold_merge_tests.rs`, following the existing repo convention (all merge fixtures for `merge_orchestration.rs` live in `common/mod.rs`, not the test file itself) — even though `common/mod.rs` was not in the plan's `files_modified` list. See Deviations below.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking issue] Added fold-specific fixtures to `tests/common/mod.rs`**
- **Found during:** Task 2 (writing the criterion tests)
- **Issue:** The plan's `files_modified` list for this plan named only `app/src-tauri/src/archive/merge.rs`, `app/src-tauri/src/lib.rs`, and `app/src-tauri/tests/fold_merge_tests.rs`. But Task 2's `<behavior>` spec requires (a) three MUTUALLY INDEPENDENT sources each carrying a uniquely identifiable row (for the copy-source regression guard) and (b) two sources sharing a contested identity key with different content (to pin order-sensitivity in `fold_matches_chained_pairwise`). No existing fixture in `common/mod.rs` provided a genuinely contested-key SOURCE-vs-SOURCE pair (the existing `generate_merge_overwrite_pair_archives` is a DEST-vs-SOURCE overwrite pair, not two sources); the private fixture-building helpers (`seed_from_res_blank`, `build_fixture_archive`, `synthetic_manifest_json`) are not `pub`, so they cannot be called from a separate test binary's `mod common;` even if reimplemented ad hoc without risking divergent fixture shape.
- **Fix:** Added `generate_fold_standalone_source_archive(note_guid, content)` (one independent Note per call, no shared identity) and `generate_fold_contested_pair()` (two sources sharing a Note/UserMark Guid with different content and different `LastModified`, each also carrying its own unique-only Note) to `common/mod.rs`, following the exact same `seed_from_res_blank` + row-insert + `build_fixture_archive` shape as every other fixture in that file.
- **Files modified:** `app/src-tauri/tests/common/mod.rs`
- **Verification:** All four `fold_merge_tests.rs` tests pass against the real DLL; the contested-pair assertion (`final_content == MERGE_FOLD_C_CONTENT`) specifically exercises the new fixture's shared-identity design.
- **Committed in:** `bb285ce5` (part of Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 3).
**Impact on plan:** Necessary for correctness of the plan's own Task 2 test spec — no scope creep beyond what writing the named tests required. `Cargo.toml`/`Cargo.lock` diff remains empty; no other files outside the plan's stated set were touched.

## Issues Encountered

None. All three tasks compiled and passed on the first implementation, including against the real `jwlCore-amd64.dll` on this host (Windows x64) — no test was skipped-as-pass.

## Assumption Status (RESEARCH A1-A4)

This plan exercised the real jwlCore binary through 3-source folds with a genuinely contested identity key. The order-sensitivity assumption underlying D10-01 (`jwlCore.mergeDatabase` does in-place UPDATEs at matched identity keys, so a later source wins a contested key) is CONFIRMED empirically: `fold_matches_chained_pairwise` asserts the contested note's final content equals the LATER source's content, and this assertion passed against the real DLL — not merely inferred from Phase 5's module docs. No RESEARCH assumption was found false during this plan; a full A1-A4 audit against every documented assumption is out of scope for this tracer wave (it did not need to touch schema-heterogeneity, blocking/progress, or the failing-step-identification paths beyond what's asserted above) and remains the concern of later plans in this phase if those specific assumptions are exercised.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

The fold chain, aggregate dry-run, single atomic promote, and Tauri command surface are all proven end-to-end on N=3 against the real jwlCore DLL. Plan 10-02 (failure envelope) and 10-03 (UI) can build directly on `fold_dry_run_merge` / `fold_merge_commit` and the `run_fold_chain` internals without re-deriving the sequencing invariants — the step-indexed `MergeFailed` reason format (`"source {i} of {n}: {inner}"`) is already in place for 10-02 to surface. No blockers identified.

---
*Phase: 10-n-way-merge-fold*
*Completed: 2026-07-26*

## Self-Check: PASSED

All created/modified files confirmed present on disk; all three task commit hashes (`5dda970a`, `bb285ce5`, `ebf40eb2`) confirmed in `git log`.
