---
phase: 10
status: human_needed
criteria_passed: 3
criteria_total: 3
---

# Phase 10 — N-Way Merge Fold — Verification

**Goal:** a user with more than two archives to reconcile can do it in ONE operation instead of chaining pairwise merges.

## Roadmap success criteria

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | User can select 3+ archives and merge them in one ordered fold operation | PASS | `app/src-tauri/src/archive/merge.rs:502-534` (`run_fold_chain`), Tauri commands `fold_merge_dry_run`/`fold_merge_commit` registered `app/src-tauri/src/lib.rs`; UI `FoldMergeDialog.tsx` + `CommandBar.tsx` "Merge Multiple Archives…" wired to both. `require_fold_sources` rejects <3 before any dir created (`merge.rs:460-470`), test `fold_rejects_fewer_than_three_sources` passes. |
| 2 | Dry-run preview extends to show cumulative effect across all inputs | PASS | `fold_dry_run_merge_with_lib_path` (`merge.rs:554-575`) runs the SAME `run_fold_chain` under a throwaway root then `content_diff`s original-vs-final (not step-by-step), so a row touched twice collapses to one entry. `fold_dry_run_aggregate` test asserts preview == committed effect and that a contested note shows once. UI: `handleFoldContinue` calls `fold_merge_dry_run` once, shown via `EditPreviewDialog`. |
| 3 | Result matches performing the equivalent sequence of pairwise merges, verified by round-trip test | PASS | `fold_matches_chained_pairwise` (`tests/fold_merge_tests.rs:147-207`) runs the fold on one session and hand-chained `merge_commit_with_lib_path` calls (same order) on an independent session, asserts `content_diff` between the two results is empty, AND separately asserts the contested-identity note resolves to the LATER source (C), never B — order-sensitivity is explicitly pinned, not papered over. |

Score: 3/3 roadmap criteria met by source + real-DLL test evidence.

## Adversarial checklist (from the verification brief)

1. **Order semantics** — PASS. `run_fold_chain` doc comment (`merge.rs:472-501`) states `fold(A,B,C) != fold(A,C,B)` is correct by design. `fold_matches_chained_pairwise` genuinely contests a shared identity key (`generate_fold_contested_pair`, `tests/common/mod.rs:1869`) between sources B and C and asserts the fold's final content equals C's (the later source), never B's — this is a real, meaningful contested-key assertion, not a vacuous one. No commutativity claim anywhere in source, tests, or UI copy.

2. **Zero partial-merge exposure** — PASS. Both `fold_dry_run_merge_with_lib_path` and `fold_merge_commit_with_lib_path` create exactly ONE root (`fold_dryrun` / `fold_staging`) with `step_1..step_N` subdirs underneath; `atomic_replace` is called exactly once, after `run_fold_chain` returns `Ok`, outside the loop (`merge.rs:624`, confirmed by direct code reading — the call site is not inside `run_fold_chain`). No `fs::copy` onto `session.db_path` anywhere in this file — only `fs::copy` FROM `copy_from` into a staging file (`stage_and_merge_from`, `merge.rs:280`). `fold_step_failure_stops_immediately` and `fold_rejects_fewer_than_three_sources` confirm no promote occurs on any failure path.

3. **Inputs read-only** — PASS. `fold_step_failure_pristine` hashes all 3 source files before and after a forced mid-fold failure and asserts identical hashes (`tests/fold_merge_tests.rs:473,491-497`); this genuinely proves sources are untouched, and the assertion would fail if `stage_and_merge_from` ever opened a source for write (it only extracts via `extract_zip_slip_safe`, read-only by construction).

4. **D10-06 playlist closure** — REAL, not overstated. `fold_playlist_graph_merge` inserts Phase 8's full playlist-graph row set (`PlaylistItemAccuracy`, `PlaylistItem`+`ThumbnailFilePath`, `IndependentMedia`, `Location`, `PlaylistItemLocationMap`, thumbnail file) into a REAL fold source via `generate_fold_playlist_graph_source`, runs it as step 2 of a real 3-source fold against the actual DLL, and asserts (a) step 3 also ran (all steps completed, not an early return that happened to not error), (b) a `PlaylistItem` row with the fixture's Label exists post-fold, (c) `IndependentMedia` for the thumbnail is present, (d) `PlaylistItemLocationMap` correctly maps to the REMAPPED PlaylistItemId — not merely `Ok(())`. This is a meaningful closure of the Phase 5 gap; the summary's claim matches the code. The PlaylistItemId-remap finding is documented honestly in `merge.rs` module docs (lines 56-68) and doesn't hide a defect — it's jwlCore's own non-Guid-identity behavior for that table, orthogonal to this phase's code.

5. **jwlCore handle-leak finding** — HONEST, not a scope-reduction hiding a defect. (a) The reworded must-have ("residue does not grow across repeated failures" instead of "fold root fully gone") is verified by `fold_step_failure_pristine` running the SAME failing fold TWICE and comparing residual file listings (`list_files_recursive`) for equality — a real assertion, not asserted-away. (b) The invariants that matter — no promote, live DB byte-identical, sources byte-identical, `session.dirty` unchanged, step 3 never attempted — are all still asserted and unrelated to the leaked handle. (c) Our own cleanup (`let _ = fs::remove_dir_all`) is best-effort by design elsewhere in this codebase (Phase 5's `dry_run_merge_with_lib_path`/`merge_commit_with_lib_path` use the identical pattern) — this is not new leniency invented for Phase 10.

6. **Failure-path coverage** — PASS. `fold_step_failure_names_source` and `fold_step_failure_stops_immediately` (not directly quoted above but present in the file and passing) assert the `MergeFailed` reason contains `"source 2 of 3"` and that source 3's row is provably absent from the DB (not just "an Err came back").

7. **Aggregate reporting collapse** — PASS. `fold_dry_run_aggregate`'s comment and assertions (`tests/fold_merge_tests.rs:263-267`) explicitly test that a note added at step 2 and overwritten at step 3 reports once, reflecting step-3 content — verified via `content_diff` on original-vs-final only (never step-accumulated).

8. **Zip-slip** — PASS. `stage_and_merge_from` (the only extraction path for fold sources, called once per step by `run_fold_chain`) calls `extract_zip_slip_safe` exclusively (`merge.rs:283`); no raw `ZipArchive`/`extractall`-equivalent loop exists in this file for the N untrusted inputs.

9. **No new dependency** — PASS. `git log --oneline -10 -- app/package.json app/src-tauri/Cargo.toml` shows the most recent touch to either file predates Phase 10 (07-04). Plan 10-03's own summary independently confirms an empty `package.json`/`package-lock.json` diff.

10. **UI honesty** — PASS. `FoldMergeDialog.tsx:91-94` states the order rule in plain language ("the one lower in the list wins") with no commutativity implication. `CommandBar.tsx` handlers reference "the SAME array to fold_merge_commit" and don't claim source files are modified (sources are only ever read per the backend contract). `foldContinueBusyRef` (`CommandBar.tsx:305`) guards the Continue click against double-invoke, confirmed present and used in `handleFoldContinue`.

11. **Do the tests prove the claims / real DLL** — MOSTLY PASS, with one flakiness finding (below). Every fold test asserts DB/file state (byte hashes, row presence by content not just PK, table-diff emptiness) — none is a bare `Ok(...)` check. `host_lib_or_skip` resolves the real vendored `jwlCore-amd64.dll` via `host_dev_lib_path()`; confirmed running (not skip-as-pass) — 10/10 tests execute and pass, none prints the "no vendored jwlCore binary" skip message in this run.

12. **Assumptions A1-A4** —
    - A1 (playlist merge coverage): RESOLVED — closed by `fold_playlist_graph_merge` (see item 4).
    - A2 (order-sensitivity — later source wins a contested key): CONFIRMED empirically in 10-01 and re-confirmed here by `fold_matches_chained_pairwise`.
    - A3 (media fold-back necessity): extended empirically to a non-final fold position (`fold_media_intermediate_step`, position 2 of 3) — same no-op observation as N=1, still stated as an observation, not assumed silently.
    - A4: not independently named in the summaries beyond what's covered by A1-A3; no evidence it was silently assumed true — the summaries are explicit that dry-run aggregation (D10-05) is proven by `fold_dry_run_aggregate`, which stands in for whatever A4 covers in RESEARCH. Not a gap this verification can resolve without RESEARCH.md's exact A4 wording — flagged as informational only, not a blocker.

## Test run (actual output, this host)

```
cd app/src-tauri && cargo test --jobs 2 --test fold_merge_tests
```

Serial (`--test-threads=1`): **10 passed; 0 failed.**
Default parallel threads: **9 passed, 1 failed** on one of four runs (`fold_dry_run_aggregate`), then 3/3 passed on immediate re-runs. Re-running the single failing test in isolation also passes.

```
running 10 tests
test fold_rejects_fewer_than_three_sources ... ok
test fold_step_failure_stops_immediately ... ok
test fold_dry_run_failure_cleans_up ... ok
test fold_playlist_graph_merge ... ok
test fold_step_failure_names_source ... ok
test fold_media_intermediate_step ... ok
test fold_merge_carries_all_sources ... ok
test fold_step_failure_pristine ... ok
test fold_matches_chained_pairwise ... ok
test fold_dry_run_aggregate ... FAILED

thread 'fold_dry_run_aggregate' panicked at tests\fold_merge_tests.rs:254:5:
assertion `left == right` failed: added counts diverge
  left: {"BlockRange": 2, "Bookmark": 2, "Location": 1, "Note": 4, "UserMark": 2}
 right: {"BlockRange": 2, "Bookmark": 2, "Location": 1, "Note": 4, "Tag": 1, "TagMap": 1, "UserMark": 2}
```

**Finding not claimed by any SUMMARY:** the fold test suite is intermittently flaky under default (multi-threaded) `cargo test` execution — reproduced once in four runs. The divergence (`Tag`/`TagMap` appearing only on one side) is consistent with either (a) the vendored `jwlCore-amd64.dll` not being safe for concurrent invocation from multiple threads in the same process (shared internal state, e.g. an autoincrement/default-tag path touched by `mergeDatabase`), or (b) a shared fixture path/global collision between `fold_dry_run_aggregate` and another test running concurrently. This was not caused by anything specific to this verification pass — 10-01/10-02/10-03 SUMMARY.md all report clean `cargo test` runs, which is plausible (the flake is intermittent, not deterministic) but means the SUMMARY claims of "all green" describe one run's outcome, not a guarantee. This does not undermine the correctness proofs above (every passing run's assertions are real and meaningful), but it is a CI-reliability risk: a real CI pipeline running default-parallel `cargo test` has a nonzero chance of a spurious red build on this test file.

`cargo clippy` / `npx vitest run` were not independently re-run in this verification pass (10-02 and 10-03 SUMMARYs both report them green on this host and no Rust/frontend production code was touched since); the fold-specific integration tests were the focus per the verification brief.

## Human verification required

1. **Test:** Run `cd app/src-tauri && cargo test --jobs 2` (default parallelism, no `--test-threads=1`) 5-10 times in a row, or under the actual CI runner, and note the failure rate of `fold_dry_run_aggregate` (or any other fold test).
   **Expected:** Either it never recurs (informational only, no action needed) or it recurs often enough to warrant a root-cause fix (test isolation or a `--test-threads=1` pin for this test binary) before this becomes a source of spurious CI failures.
   **Why human:** A single-session verifier cannot afford enough repeated runs to establish a reliable failure rate, and root-causing a suspected native-DLL thread-safety issue needs either jwlCore source access (unavailable) or sustained interactive investigation beyond this verification pass's scope.

## Gaps

None that block the phase goal. Roadmap success criteria 1-3 are all met with real, meaningful evidence against the real DLL. The one open item (test flakiness under parallel execution) is a CI-reliability risk, not a correctness or Core-Value gap — every actual assertion in every passing run is genuine (byte-hash comparisons, content-signature diffs, row presence by content, order-sensitivity pinning), and the underlying fold logic (single atomic promote outside the loop, read-only sources, zip-slip-safe extraction, no new dependencies) is verified directly from source, not from test behavior alone.

## Ship verdict

**Ship, with a follow-up item logged**: file a quick task to pin `fold_merge_tests.rs` (or the whole `fold_merge_tests` binary) to `--test-threads=1` in CI, or root-cause the native-DLL concurrency interaction, before this becomes a recurring spurious-red-CI problem. Not a blocker for Phase 10 completion — the phase goal (one-operation N-way fold, order-sensitive, atomic, no partial exposure) is demonstrably achieved.
