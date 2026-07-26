---
phase: 9
status: passed
criteria_passed: 2
criteria_total: 2
---

# Phase 9: Incremental Export — Verification Report

**Phase Goal:** A user doing repeated exports only has to review what actually changed (upstream ask #188).
**Verified:** 2026-07-26 (live re-run against shipped source, not SUMMARY claims)

## ROADMAP Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | User can choose a prior export point and export only items changed since then | PASS | `INCREMENTAL_EXPORT_COMMANDS` in `app/src/components/CategoryList.tsx:79-84` maps all 5 categories (Notes/Favorites/Bookmarks/Annotations/Highlights) to `export_<category>_incremental` Tauri commands, all registered in `lib.rs` invoke_handler (`lib.rs:2896-2912`). Backend `export_<category>_incremental_impl` in `db/io/diff.rs` computes and writes only the added+modified set. |
| 2 | Note identity for diff resolved via content hashing, not vendor timestamps | PASS | `record_hash()` (`diff.rs:61-70`) SHA-256s the wire text; `notes_hash_input()` (`diff.rs:140-151`) strips the leading `{CREATED=}{MODIFIED=}` bracket pair before hashing so a timestamp-only change is excluded — asserted live by `timestamp_only_change_excluded` (passing). |

**Score:** 2/2 ROADMAP criteria verified.

## Adversarial checks (per verify_specifically list)

1. **Over-export invariant.** `diff_records` (`diff.rs:98-129`) computes `added`/`modified` (the exported set) purely from `prior_hashes.contains(hash)` membership; the identity key `K` is consulted only *after* an entry already passed the hash test, solely to choose the added/modified label. No code path lets an identity match suppress a record — confirmed by reading every `export_*_incremental` function (`diff.rs:316-776`), each of which builds `selected_ids`/`selected_location_ids` from the same hash-only filter, never gated by identity. VERIFIED.

2. **Hash-input symmetry.** `export_notes` (`export.rs:657+`) and `export_annotations` (`export.rs:386+`) both call the SAME `format_note_record`/`format_annotation_record` functions that `read_note_id_records`/`read_annotation_id_rows` (consumed by `diff.rs`) use to build the live-side hash input — confirmed by direct read of `export.rs:657-720` and `:386-403`, plus the doc comment on `format_note_record` explicitly stating this dual-use design. Favorites/Bookmarks/Highlights hash the raw wire line itself (`split_prior_lines`/`read_*_id_lines`), which is the same text the flat exporters write via `join_row`. VERIFIED.

3. **Timestamp exclusion.** Only Notes carries a wire timestamp (`{CREATED=}{MODIFIED=}` bracket pair); Favorites/Bookmarks/Highlights/Annotations wire formats have no timestamp fields (confirmed via the format functions and identity-key field lists in `diff.rs:258-284`, `:503-512`) — nothing else to strip. VERIFIED.

4. **Proportionality (inverted check).** Grepped `diff.rs` and `export.rs` for `dry_run_*`, `apply_*`, `PragmaGuard`, `DryRunReport` — zero matches inside the incremental-export module; the one reference (`diff.rs:42`) is a doc comment explicitly explaining why `IncrementalExportSummary` is a NEW type rather than reusing `DryRunReport`, i.e. active avoidance of the anti-pattern, not adoption of it. No read-only-path over-engineering found. VERIFIED.

5. **No new format.** Every `export_<category>_incremental` routes through the shipped `export_favorites`/`export_bookmarks`/`export_highlights`/`export_notes`/`export_annotations` (`diff.rs:355,419,485,658,753,766` etc.) with a computed id/location selection — no bespoke writer exists. VERIFIED.

6. **CRLF.** `normalize_line_endings` (`diff.rs:158-164`) is applied inside every `split_prior_*` helper before parsing; a live re-run of the test suite confirms `notes_crlf_prior_diffs_identically_to_lf`, `highlights_crlf_prior_diffs_identically_to_lf`, and equivalents for all 5 categories PASS, asserting identical summaries AND identical record sets (per 09-04-SUMMARY's stated CRLF-suite shape, confirmed by actually running it — see Test Execution below). VERIFIED.

7. **Deletions informational-only.** `deleted_candidates` is a `usize` count only (`IncrementalExportSummary` struct, `diff.rs:47-54`); no code path writes it to the output file. `docs/incremental-export.md:52-57` explicitly states "Removals are never written into the file." UI doc comment in `CategoryList.tsx` and the D9-04 doc comment on `IncrementalExportSummary` both flag this as informational-only, requiring an explicit caveat. VERIFIED.

8. **Disclosed limitations tested + documented.** Favorites' structural zero-modified: asserted by `favorites_never_reports_modified` (passing) and documented (`docs/incremental-export.md:66-69`). Annotations' sibling over-selection: `export_annotations_incremental`'s summary uses the exporter's own written-record count (not `added+modified`) specifically to surface the over-selection (`diff.rs:598-601`), and `annotations_invariant_edit_add_and_untouched_sibling_all_correct` test passes live. Playlists exclusion documented (`docs/incremental-export.md:70-74`) with the D9-06 rationale (whole-archive zip copy, nothing to diff) also recorded as a code comment in `CategoryList.tsx`. VERIFIED.

9. **Adversarial suite has teeth.** Live-ran the full 50-test suite (below). Identity-collision tests (`notes_invariant_identity_collision_and_new_record_all_exported`, `highlights_invariant_identity_collision_and_new_record_all_exported`, `bookmarks_invariant_...`, `annotations_invariant_edit_add_and_untouched_sibling_all_correct`) construct genuine collisions (two live records at the same LocationId / same `{CREATED=}` value) and assert via `HashSet<String>` record-CONTENT comparison (not counts) that both the edited and newly-added records remain in the exported set. VERIFIED — and per 09-04-SUMMARY, this suite caught a real mislabeling finding (Bookmarks/Highlights collision labels both records `modified`, never dropping either), which is exactly the kind of finding that indicates genuine adversarial pressure rather than happy-path-only coverage.

10. **Convergence.** `incremental_export_converges` and `highlights_incremental_converges` PASS live; the latter's doc comment (`diff.rs` tests / `incremental_export_tests.rs:890-897,947-948`) explicitly explains why Highlights' accepted `UserMark`-growth from Phase 8 does not cause perpetual churn (identity is keyed on wire-visible Location/token fields, not the DB-internal `UserMarkId`). VERIFIED.

11. **No new Cargo dependency.** `grep sha2/image/rand/uuid/fancy-regex` in `Cargo.toml` shows only `sha2 = "0.11"` (pre-existing from earlier phase use, reused not added) — `image`/`rand`/`uuid`/`fancy-regex` absent. VERIFIED.

12. **Tests prove the claims.** Spot-checked several tests beyond the collision suite (`deleted_candidate_not_exported`, `timestamp_only_change_excluded`, `malformed_prior_file_aborts`) — each asserts on the resulting `IncrementalExportSummary` fields or the actual output record set, not merely `.is_ok()`. No hollow assertions found in the sampled set.

13. **Regression — Phase 8 defects.** `normalize_line_endings` confirmed present at 5 call sites in `import.rs` (lines 116, 425, 840, 1114, 1541) — one per parser entry point (Notes/Bookmarks/Favorites/Highlights/Annotations). `find_or_insert_annotation_location` (`import.rs:937`) exists and is called from the annotation import path (`import.rs:1011`) with typed `map_sqlite_err` error wrapping rather than a raw constraint violation. VERIFIED.

## Test Execution (live, this verification run)

```
cd app/src-tauri && cargo test --test incremental_export_tests
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
```

All 50 tests — including every collision, CRLF, wrong-category, empty-prior-body, and convergence test named in the SUMMARY — pass against the actual shipped source, run independently by this verifier (not copy-pasted from SUMMARY.md).

## Minor doc-sync gap (non-blocking)

`.planning/ROADMAP.md`'s Phase 9 plan checklist still shows `09-02`, `09-03`, `09-04` as unchecked (`- [ ]`) even though all four plans are implemented, tested, and merged (confirmed by source + passing tests + `09-04-SUMMARY.md` commit hashes `9a8c7dc0`/`9a2105ca`/`27c35031`). This is a checklist-formatting drift, not a functional gap — does not affect the ship verdict but should be swept during the phase-closeout docs pass.

## Anti-Patterns Found

None. No TBD/FIXME/XXX markers, no placeholder returns, no dry-run/apply scaffolding on this read-only path.

## Gaps Summary

None. Both ROADMAP success criteria verified against live source and a live test run; all 13 adversarial checks pass; the one finding (ROADMAP checklist not ticked) is cosmetic.

## Ship Verdict

**PASS.** Phase 9 goal is achieved: the over-export-safe, content-hash-only invariant is correctly implemented and enforced by shared code paths (never a code fork), symmetric hashing guarantees the diff can't drift from what's actually exported, CRLF and identity-collision adversarial tests genuinely exercise the invariant and pass live, disclosed limitations are both tested and documented, and no scope creep (no new format, no new dependency, no mutation-safety over-engineering) occurred on this read-only feature.

---
_Verified: 2026-07-26_
_Verifier: Claude (gsd-verifier)_
