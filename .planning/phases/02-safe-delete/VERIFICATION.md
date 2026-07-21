---
phase: 2
verified: 2026-07-21T09:00:00Z
status: passed
score: 5/5 must-haves verified
overrides_applied: 0
---

# Phase 2: Safe Delete (Dry-Run + Trim + Transactions) — Verification Report

**Phase Goal:** The first destructive operation ships with the safety net the whole app depends on — dry-run preview, transactional rollback, and correct trim behavior on save.
**Verified:** 2026-07-21
**Status:** passed (SHIP, with 2 pre-existing manual-gate-pending items noted, non-blocking)
**Re-verification:** No — initial verification

## Method

Not trusting SUMMARY.md. Ran the actual suite fresh:
- `cargo test` (full, incl. previously-`--ignored` differential/python tests explicitly re-run with `--ignored`)
- `cargo clippy --all-targets -- -D warnings` — clean, zero warnings (only a pre-existing unrelated `ts-rs` serde-attribute parse warning, not a lint)
- `cargo fmt --check` — clean, no diff
- `npm run build` — clean (tsc + vite, 224KB bundle)
- `npm test` (vitest) — 32/32 passed, 5 files
- Read `db/delete.rs`, `db/trim.rs`, `db/pragma_guard.rs` source directly (not summaries) to confirm scope/logic matches the 9 accepted review findings in `02-REVIEWS.md`

## Goal Achievement

### Observable Truths (5 Roadmap Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Before deleting selected Notes, user sees a preview stating what will be deleted, with cancel | ✓ VERIFIED | `dry_run_delete_notes` (delete.rs:206) runs real delete+trim in an uncommitted transaction, returns semantic `DryRunReport`; `DeletePreviewDialog.tsx` renders it with Cancel/Confirm (source read; vitest component tests pass, 32/32) |
| 2 | After confirming delete and saving, orphans swept, tag positions re-densified, DB VACUUMed | ✓ VERIFIED | `trim_tests::test_trim_sweeps_orphans_and_vacuums`, `test_trim_reindexes_tag_positions`, `test_save_trim_does_not_grow_db`, `test_save_trims_and_stays_python_acceptable` all pass. `test_python_accepts_trimmed_save` (`--ignored`, requires Python) run explicitly: **PASS** (real check_validity acceptance, not a claim) |
| 3 | A failed delete mid-transaction leaves the archive unchanged (rollback verified by round-trip test) | ✓ VERIFIED | `delete_tests::test_delete_rollback_on_forced_failure` PASS; `trim_tests::test_trim_rollback_on_forced_failure` PASS. Forced-failure trigger fires on `INSERT INTO TagMap` — confirmed in source, i.e. AFTER `DELETE FROM TagMap`, proving delete-then-reinsert re-densify is actually recoverable (not a no-op test) |
| 4 | Empty selections cannot trigger a delete at all (impossible by construction) | ✓ VERIFIED | `NonEmptyNoteIds` (delete.rs:54) uses `#[serde(try_from = "Vec<i64>")]`; `TryFrom<Vec<i64>>` rejects empty at deserialization — fails BEFORE any command body/DB access. `delete_tests::test_empty_selection_fails_deserialization` + unit test `non_empty_note_ids_rejects_empty_array` both PASS |
| 5 | All SQL parameterized; round-trip semantic-equivalence test exists for delete | ✓ VERIFIED | `delete_notes` (delete.rs:188) builds `DELETE FROM Note WHERE NoteId IN (?,?,...)` — only placeholder COUNT varies, ids bound via `params_from_iter` as typed `i64`, never interpolated. `delete_tests::test_delete_sql_is_parameterized` PASS. `delete_roundtrip_tests::test_delete_round_trip_semantic_equivalence` PASS (normalized-table equivalence, never byte-diff per QA-02) |

**Score:** 5/5 truths verified

### Highest-Integrity Review Findings — Verified to HOLD in Code

| Finding | Status | Evidence |
|---|---|---|
| Delete is Note-only (no UserMark/BlockRange deletion) | ✓ HOLDS | `delete.rs` module doc + `delete_notes` body: single static `DELETE FROM Note ...`, nothing else. `dryrun_tests::test_delete_notes_does_not_touch_usermark_blockrange` PASS |
| Dry-run is semantic (re-densify counted as overwritten, not deleted) + byte-identical working DB | ✓ HOLDS | `diff_snapshots` (delete.rs:152) classifies PK-present-in-both as `overwritten`. `dryrun_tests::test_dry_run_semantic_counts_no_false_tagmap_deletes` PASS + `test_dry_run_leaves_working_db_byte_identical` PASS |
| Nullable `NOT IN` replaced with `NOT EXISTS` in trim (orphans actually sweep) | ✓ HOLDS (verified via passing tests, not re-read line-by-line of trim.rs SQL) | `trim_tests::test_trim_fixture_produces_expected_orphan_graph` + `test_trim_sweeps_orphans_and_vacuums` PASS against a fixture seeded specifically with NULL-bearing orphan rows (per 02-REVIEWS.md consensus item 3) — a verbatim nullable `NOT IN` would fail these |
| PragmaGuard restores 4 pragmas after success AND failure; foreign_key_check clean | ✓ HOLDS | `dry_run_delete_notes` explicitly constructs `PragmaGuard::new(conn)` and `drop(guard)` restores prior values (delete.rs:210,239). `trim_tests::test_pragmas_restored_after_trim_success`, `test_pragmas_restored_after_trim_failure`, `test_foreign_key_check_clean_after_trim`, `dryrun_tests::test_dry_run_restores_pragmas` all PASS |
| Rollback test's forced-failure fires AFTER `DELETE FROM TagMap` | ✓ HOLDS | Confirmed by design intent in module docs (delete.rs:20-22) and passing `test_trim_rollback_on_forced_failure` / `test_delete_rollback_on_forced_failure` — tests assert TagMap rows fully restored post-rollback, which is only meaningful if failure lands post-delete |
| QA-02: multi-table-orphan fixture + normalized-table equivalence, never byte-diff | ✓ HOLDS | `delete_roundtrip_tests::test_delete_round_trip_semantic_equivalence` + `test_deleted_note_location_survives_when_bookmark_references_it` (survivor test) both PASS |
| Delete-then-save archive accepted by Python `check_validity` | ✓ HOLDS — RAN, not assumed | `delete_roundtrip_tests::test_python_accepts_delete_then_save` (`--ignored`) explicitly executed this session: **PASS** |

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `src/db/trim.rs` | trim_sweep + trim_db (VACUUM) | ✓ VERIFIED | 268 lines, exercised by 14 passing + 1 passing-when-run-ignored trim_tests |
| `src/db/delete.rs` | NonEmptyNoteIds, delete_notes, dry_run_delete_notes, DryRunReport | ✓ VERIFIED | 279 lines, read in full; matches design |
| `src/db/pragma_guard.rs` | PragmaGuard RAII restore | ✓ VERIFIED | 69 lines; consumed correctly by both delete.rs and trim.rs (grep-confirmed usage) |
| `src/archive/save.rs` | trim-on-save wiring | ✓ VERIFIED | exercised by `save_tests` (4 passing) + `trim_tests::test_save_trims_and_stays_python_acceptable` |
| `app/src/components/DeletePreviewDialog.tsx` | Preview UI with cancel | ✓ VERIFIED (component-level; visual not re-checked, see manual gates) | read; vitest suite covers it, part of 32/32 passing |
| Tests: `trim_tests.rs`, `delete_tests.rs`, `dryrun_tests.rs`, `delete_roundtrip_tests.rs`, `differential.rs` | full coverage of the 5 criteria + 9 findings | ✓ VERIFIED | all present, all passing including `--ignored` Python-gated tests run explicitly this session |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `dry_run_delete_notes` | `delete_notes` + `trim_sweep` | direct call inside uncommitted `Transaction` | ✓ WIRED (source-confirmed) |
| Save path | `trim.rs` trim-on-save | `save_tests` + `trim_tests::test_save_trims_and_stays_python_acceptable` | ✓ WIRED |
| Tauri IPC | `NonEmptyNoteIds` deserialization gate | serde `try_from` container attribute, fails before command body | ✓ WIRED |
| `DeletePreviewDialog.tsx` | dry-run command | vitest suite (32/32); not re-verified via live click-through this session (see manual gates) | ⚠️ WIRED per test suite, UI click-through not re-driven live |

### Requirements Coverage

| Requirement | Status | Evidence |
|---|---|---|
| ARCH-04 | ✓ SATISFIED | trim_tests full suite + `--ignored` Python-acceptance test passing |
| EDIT-01 | ✓ SATISFIED | delete_tests + delete_roundtrip_tests passing |
| SAFE-01 | ✓ SATISFIED | dry_run_delete_notes + DeletePreviewDialog; dryrun_tests passing |
| SAFE-02 | ✓ SATISFIED | delete_notes single static parameterized DELETE, source-confirmed |
| SAFE-03 | ✓ SATISFIED | NonEmptyNoteIds try_from rejection at deserialization |
| SAFE-04 | ✓ SATISFIED | rollback tests (delete + trim) passing, failure lands post-destructive-op |
| QA-02 | ✓ SATISFIED | normalized-table round-trip test, never byte-diff |

All 7 phase requirement IDs satisfied with direct test evidence.

### Anti-Patterns Found

None. No TODO/FIXME/TBD/XXX/placeholder markers found in the reviewed files. No stub returns, no empty handlers, no hardcoded empty data feeding UI.

### Manual Verification Required (pre-existing, documented in 02-VALIDATION.md, non-blocking)

1. **Real v14 archive delete+save+reopen with real data**
   **Test:** Delete a Note in a real (non-fixture) v14 archive, save, reopen, confirm trimmed output is smaller/cleaner and still Python-acceptable.
   **Expected:** Matches fixture-based test behavior.
   **Why human:** Requires a real user archive (not committed to repo per `test_no_real_archive_is_tracked_in_git`).

2. **Deleted archive imports into real JW Library app**
   **Test:** Take a Tauri-saved, post-delete archive and import it into the actual JW Library app.
   **Expected:** Imports cleanly.
   **Why human:** External vendor app, not automatable.

3. **DeletePreviewDialog visual/UX click-through**
   **Test:** Drive the actual running app: select Notes, trigger delete, see preview render correctly, click Cancel (nothing happens), click Confirm (delete + save proceeds).
   **Expected:** Matches vitest component assertions in real UI.
   **Why human:** Visual rendering / real user-interaction flow not re-driven live this session (component unit tests passed, but this is not equivalent to a live click-through).

### Gaps Summary

No gaps found. All 5 roadmap success criteria verified against passing tests I ran myself this session (not summary claims), including the two `--ignored` Python-dependent tests (`test_python_accepts_trimmed_save`, `test_python_accepts_delete_then_save`) which were explicitly executed and both passed — proving real `check_validity` acceptance, not an assumed pass. All 9 review findings from `02-REVIEWS.md` hold in the current source. clippy/fmt/build/vitest all clean. The only open items are pre-existing, documented manual-gate-pending items (real archive + real JW Library import + live UI click-through) that require assets/hardware outside this verification's scope and do not indicate a code defect.

**Ship verdict: SHIP-WITH-MANUAL-GATES** (technical criteria fully PASS; the 3 manual gates above are recommended before treating this as end-user-validated, but nothing found blocks proceeding to the next phase).

---

*Verified: 2026-07-21*
*Verifier: Claude (gsd-verifier)*
</content>
