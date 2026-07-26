---
phase: 10
slug: n-way-merge-fold
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-26
---

# Phase 10 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Generated from `10-RESEARCH.md` `## Validation Architecture`, refined against the
> plan set (10-01..10-03).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` / `cargo test` (backend) + vitest 2.x (frontend) |
| **Config file** | none for Rust — `app/src-tauri/tests/*.rs` integration tests plus inline `#[cfg(test)] mod tests`; `app/vite.config.ts` for the frontend |
| **Quick run command** | `cd app/src-tauri && cargo test --jobs 2 --test fold_merge_tests` |
| **Full suite command** | `cd app/src-tauri && cargo test --jobs 2` then `cd app && npx vitest run` |
| **Estimated runtime** | ~180-240 s (Rust; the fold tests each run N-1 real native merges) + ~20 s (vitest) |

**Host constraint (load-bearing):** `--jobs 2` is MANDATORY on every `cargo test` and
`cargo build` invocation. Default parallelism OOMs the linker on this host (`os error
1455`, paging file too small). This is an environment limit, not a code defect — never
"fix" it by changing code, never drop the flag.

**Native-binary constraint (skip-as-pass):** every fold test drives the real
`jwlCore-amd64.dll` via `jwlcore::merge::host_dev_lib_path()`. On a host without the
vendored binary for its `(OS, ARCH)` — notably arm64-windows and any binary-less CI leg —
the test must return early and PASS, using the established skip-as-pass pattern from
`merge_orchestration.rs` / `jwlcore_status_real_load_current_host`. A skipped run is a
pass, but each SUMMARY must state explicitly whether its tests RAN against the real DLL
or skipped, so a green suite is never mistaken for verified behaviour.

Never use watch mode: `npx vitest run`, never bare `vitest`.

---

## Sampling Rate

- **After every task commit:** `cargo test --jobs 2 --test fold_merge_tests` (backend
  tasks) or `npx vitest run <file>` (frontend tasks)
- **After every wave:** full Rust suite + `npx vitest run` — catches any Phase 5
  regression from the `stage_and_merge` parameterization
- **Before `/gsd-verify-work`:** full suite green, zero failed, plus
  `cargo clippy --all-targets -- -D warnings`
- **Max feedback latency:** 240 seconds

---

## Per-Task Verification Map

| Req / Property | Behavior | Test Type | Automated Command | Status |
|----------------|----------|-----------|-------------------|--------|
| MERGE-03 c1 | 3+ archives fold in one operation; ALL sources' unique rows survive | integration | `cargo test --jobs 2 --test fold_merge_tests fold_merge_carries_all_sources` | ⬜ pending |
| MERGE-03 c1 | Fewer than 3 sources rejected with a typed error, never degraded | integration | `cargo test --jobs 2 --test fold_merge_tests fold_rejects_fewer_than_three_sources` | ⬜ pending |
| MERGE-03 c2 | ONE aggregate dry-run report; live DB unchanged by the preview | integration | `cargo test --jobs 2 --test fold_merge_tests fold_dry_run_aggregate` | ⬜ pending |
| **MERGE-03 c3** | **fold(A,B,C) == chained pairwise commits IN THE SAME ORDER**, normalized table state, never byte-diff | integration | `cargo test --jobs 2 --test fold_merge_tests fold_matches_chained_pairwise` | ⬜ pending |
| Core Value | Step-2 failure: live DB bytes identical, `dirty` unchanged, all N sources unchanged, fold root removed | integration | `cargo test --jobs 2 --test fold_merge_tests fold_step_failure_pristine` | ⬜ pending |
| D10-03 | Failing step named 1-indexed in the `MergeFailed` reason | integration | `cargo test --jobs 2 --test fold_merge_tests fold_step_failure_names_source` | ⬜ pending |
| D10-03 | Abort is immediate — step 3 never attempted after step 2 fails | integration | `cargo test --jobs 2 --test fold_merge_tests fold_step_failure_stops_immediately` | ⬜ pending |
| Pitfall 3 | Dry-run failure leaves no `fold_dryrun` root; no accumulation across repeated failures | integration | `cargo test --jobs 2 --test fold_merge_tests fold_dry_run_failure_cleans_up` | ⬜ pending |
| D10-04 / A3 | Media contributed at an INTERMEDIATE step survives the next step's re-seed | integration | `cargo test --jobs 2 --test fold_merge_tests fold_media_intermediate_step` | ⬜ pending |
| D10-06 / A1 | Full playlist graph through a 3-archive fold — carried, or blocked with the EXACT error recorded | integration | `cargo test --jobs 2 --test fold_merge_tests fold_playlist_graph_merge -- --nocapture` | ⬜ pending |
| No regression | Phase 5's own suite green with zero edits after the `stage_and_merge` refactor | integration | `cargo test --jobs 2 --test merge_orchestration` | ⬜ pending |
| D10-01 (UI) | Reorder changes the array sent to the backend; commit gets the identical array | component | `npx vitest run src/components/FoldMergeDialog.test.tsx` | ⬜ pending |
| MERGE-03 c1/c2 (UI) | Multi-pick, cancel-writes-nothing, one aggregate preview, error routing | component | `npx vitest run src/components/CommandBar.test.tsx` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

The **`fold_matches_chained_pairwise`** row is criterion 3 itself and the phase's single
most important test. It asserts equality with the SAME source order only. A test asserting
`fold(A,B,C) == fold(B,A,C)` is testing the WRONG thing (D10-01) and must not exist — order
divergence is correct behaviour, mirroring hand-chained Phase 5 merges.

Second-most important is **`fold_step_failure_pristine`**: an N-way operation has N-1
chances to fail partway, and the user must never see a "merged 2 of 3" archive.

---

## Wave 0 Requirements

- [ ] `app/src-tauri/tests/fold_merge_tests.rs` — new integration test file following
      `merge_orchestration.rs`'s host-DLL resolution, skip-as-pass early return, and
      synthetic-fixture conventions (created by plan 10-01 task 2)
- [ ] `common::fresh_v16_db()` — already exists (Phase 5/8/9 tests); reused as-is for each
      of the N fold-input fixture archives, no new helper needed
- [ ] Full playlist graph fixture — Phase 8's `build_container` ROW SET
      (`PlaylistItemAccuracy`, `PlaylistItem` with `ThumbnailFilePath`,
      `IndependentMedia`, `Location`, `PlaylistItemLocationMap`, plus the thumbnail file),
      inserted DIRECTLY into a source archive's `userData.db` with parameterized SQL —
      NOT routed through the `.jwlplaylist` export path. Test-local; do not refactor
      Phase 8's shipped test helpers.
- [ ] A deterministic step-2 failure source: reuse Phase 5's known-deterministic jwlCore
      abort (the minimal-`PlaylistItem` fixture it already used for the atomic-promote
      pristine leg) or an unreadable/non-zip file at position 2, whichever is deterministic
      on this host; record which was used
- [ ] `app/src/components/FoldMergeDialog.test.tsx` — new component test file
- [ ] Framework install: none — `cargo test` and vitest are already configured

---

## Manual-Only Verifications

| Behavior | Why Manual | Test Instructions |
|----------|-----------|-------------------|
| A folded archive still opens in real JW Library and in the Python JWL Manager | Third-party apps; the automated oracle covers normalized table state, not their loaders | Fold three synthetic archives, Save, open the result in JW Library and in `JWLManager.py`; confirm no error and the merged records are present |
| Perceived responsiveness during a long fold (D10-07 tradeoff) | Wall-clock feel with no percentage indicator is a judgement call, not an assertion | Fold several sizeable archives; confirm the window stays responsive and the busy state persists for the whole operation with no dead-looking gap |

*Everything else in this phase has automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have an `<automated>` verify or a Wave 0 dependency
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] `--jobs 2` present on every cargo invocation
- [ ] Skip-as-pass stated and honoured for binary-less hosts; each SUMMARY records RAN vs SKIPPED
- [ ] Feedback latency < 240 s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
