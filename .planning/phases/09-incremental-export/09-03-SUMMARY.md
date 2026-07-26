---
phase: 09-incremental-export
plan: 03
subsystem: db/io (export diff engine, Annotations composite identity)
tags: [incremental-export, annotations, composite-identity, over-selection, sha256-diff, tauri-command]
dependency:
  requires: ["09-01", "09-02"]
  provides:
    - db::io::diff::{annotations_identity, split_prior_annotation_records}
    - db::io::diff::export_annotations_incremental
    - db::io::export::{format_annotation_record, read_annotation_id_rows}
    - export_annotations_incremental Tauri command
  affects:
    - app/src-tauri/src/db/io/export.rs (read_annotation_rows now a thin projection over read_annotation_id_rows; export_annotations' write loop now calls format_annotation_record)
tech-stack:
  added: []
  patterns:
    - "composite identity key: two wire-recoverable fields (DOC, LABEL) joined with the diff engine's KEY_UNIT_SEP, mirroring the flat-category pattern but built from bracket-tag text instead of pipe-split indices"
    - "disclosed over-selection: when the exporter's own selection granularity (LocationId) is coarser than the diff's identity granularity (DOC+LABEL), the summary's `exported` field carries the EXPORTER'S OWN written-record count rather than added.len()+modified.len(), so a caller can see the gap rather than being told a misleadingly small number"
key-files:
  created:
    - app/src-tauri/tests/fixtures/wire/annotations_prior.txt
  modified:
    - app/src-tauri/src/db/io/export.rs
    - app/src-tauri/src/db/io/diff.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/incremental_export_tests.rs
decisions:
  - "Identity is the wire-recoverable (DOC, LABEL) pair, never LocationId — LocationId is not on the Annotations wire at all (RESEARCH Pitfall 2), and D9-02's stated identity is refined in the direction its own rationale points, since (DocumentId, TextTag) and (LocationId, TextTag) are in one-to-one correspondence for any archive at rest."
  - "The exported SET is still decided purely by hash-set membership over every live record's own hash (the module's two-layer rule, unchanged) — but the SELECTION passed to export_annotations must be by LocationId (the exporter's only granularity), so the set of selected records is expanded from 'the (DOC,LABEL)-keyed records whose hash changed' to 'every InputField row at any LocationId that contains at least one changed record'. This is the disclosed over-selection, not a second decision layer — the diff_records summary counts are computed independently over every live record's own hash, so an unchanged sibling pulled in by the LocationId expansion is never mislabeled as added or modified."
  - "IncrementalExportSummary.exported for Annotations is the value export_annotations itself returns (its own written-row count) rather than selected_location_ids.len() or added+modified — matching the plan's explicit instruction to disclose the exporter's own count rather than infer it, since selected_location_ids.len() would undercount (it counts distinct LocationIds, not written InputField rows) and added+modified would undercount whenever over-selection pulled in an unchanged sibling."
  - "Test fixture Location uses a non-NULL DocumentId (1001) rather than NULL — SQL '=' never matches NULL, so find_or_insert_annotation_location's existing-Location lookup (used by apply_import_annotations on re-import) can only reuse a LocationId when DocumentId is a concrete value; with DocumentId NULL every re-import would create a brand-new Location (its INSERT path never sets Track, which then fails the schema's CHECK constraint entirely — an existing Phase 8 code path, out of this plan's scope to fix). Annotations_incremental_converges needed a real DocumentId to exercise re-import without tripping that unrelated pre-existing constraint gap."
metrics:
  duration: "~1 session"
  completed: "2026-07-26"
status: complete
---

# Phase 9 Plan 3: Annotations Incremental Export Summary

Brought Annotations — the one category where the browse-layer identity (`LocationId`) and the wire-recoverable identity (`(DOC, LABEL)`) genuinely differ — onto the incremental-export path, with the resulting `LocationId`-selection over-export disclosed in the returned summary rather than hidden.

## What was built

**`db/io/export.rs`** (Task 1): `format_annotation_record(row: &AnnotationExportRow) -> String` extracts the exact per-record bytes `export_annotations`' write loop wrote inline (the `\n===` opener, `{PUB=}`/optional `{ISSUE=}`/`{DOC=}{LABEL=}` header, and the `Value` body) into a single pure function; the write loop now calls it, with zero byte-output change (`export_wireformat_tests` stayed green against unedited golden fixtures, including the Annotations sentinel case). `read_annotation_id_rows` runs the SAME query `read_annotation_rows` used, with `LocationId` added as a leading SELECT column; `read_annotation_rows` is now a thin projection over it — one SQL column list exists for Annotations. `AnnotationExportRow` and its fields were widened from private to `pub(crate)` so `diff.rs` can read `doc`/`label` to build the identity key and pass the row to `format_annotation_record` for hashing.

**`db/io/diff.rs`** (Task 1 + Task 2): `annotations_identity(doc, label)` joins the two wire-recoverable fields with the module's `KEY_UNIT_SEP`. `split_prior_annotation_records` extracts `((doc, label), record_text)` pairs from a CRLF-normalized prior file, sharing `parse_annotations_file`'s exact `\n===`-forward-scan boundary discipline (the sentinel is consumed only as the final boundary, never emitted as a record). `export_annotations_incremental` mirrors the shipped `export_notes_incremental` two-layer shape: the exported set is decided purely by hash-set membership over `format_annotation_record`'s output (never the identity key), then the DISTINCT `LocationId`s of every selected record are collected into `NonEmptyLocationIds` for the actual `export_annotations` call — because that is the only selection granularity the exporter supports. `diff_records`' added/modified/deleted-candidate counts are computed completely independently over every live record's own `(key, hash)` pair, so an unchanged sibling pulled in by the `LocationId` expansion is never mislabeled. The returned `IncrementalExportSummary.exported` carries `export_annotations`' own written-row count (not `selected_location_ids.len()` and not `added+modified`), so a caller who sees more records written than were added/modified has the explanation (the LocationId selection type) visible in the summary rather than a silently misleading number.

**`export_annotations_incremental` Tauri command** (Task 2, `lib.rs`): thin session/path wrapper over the pure `db::io::diff::export_annotations_incremental`, same shape as the four prior categories' commands, registered in `generate_handler!`.

**`incremental_export_tests.rs`** (Task 2): 8 new tests — `annotations_no_change_reports_zero_and_writes_valid_empty_output`, `annotations_value_change_included`, `annotations_added_included` (asserting `exported: 2` since the unchanged sibling at the same `LocationId` rides along with the newly added record — over-selection applies to `added` too, not just `modified`), `annotations_deleted_candidate_not_exported`, `annotations_no_prior_file_exports_all`, `annotations_malformed_prior_file_aborts`, and the two plan-mandated tests: `annotations_composite_identity` (two annotations at ONE `LocationId` with DIFFERENT `TextTag`s; editing only one asserts `modified: 1` — proving the two are diffed independently, never collapsed into one identity — while `exported: 2` proves the unchanged sibling is written, disclosed, never hidden) and `annotations_incremental_converges` (export → re-import → export again against the same prior converges to `modified: 0`, explicitly exercising the `(LocationId, TextTag)` upsert conflict target `apply_import_annotations` uses on re-import).

## Deviations from Plan

**None functionally** — the plan's design was followed exactly (composite `(DOC, LABEL)` identity, `LocationId`-selection over-export disclosed via the exporter's own written count). One test-fixture adjustment was needed and is recorded as a decision above: the seeded Annotation `Location` uses `DocumentId = 1001` rather than `NULL`, to avoid an unrelated pre-existing gap in Phase 8's `find_or_insert_annotation_location` (its "insert new Location" path never sets `Track`, which trips the schema's CHECK constraint whenever `DocumentId` is `NULL` and no existing Location matches — SQL `=` never matches `NULL`, so re-import of a `DOC=None` annotation always takes that insert path). This is out of this plan's stated scope (Annotations incremental export, not Annotations import) and was worked around in the test fixture rather than touched.

## Test output (actual)

```
cargo test --jobs 2 db::io::diff --lib
  test result: ok. 19 passed; 0 failed
  (includes annotations_identity_varies_by_label_at_the_same_doc,
   split_prior_annotation_records_retains_both_siblings_at_one_location,
   split_prior_annotation_records_never_emits_the_end_sentinel)

cargo test --jobs 2 --test export_wireformat_tests
  test result: ok. 23 passed; 0 failed   (golden fixtures unchanged, including
  exported_annotations_match_golden_fixture_exactly and
  exported_annotations_end_with_end_sentinel_and_no_trailing_newline)

cargo test --jobs 2 --test incremental_export_tests
  test result: ok. 30 passed; 0 failed
  (22 pre-existing Notes/Favorites/Bookmarks/Highlights tests + 8 new:
   annotations_no_change_reports_zero_and_writes_valid_empty_output,
   annotations_value_change_included, annotations_added_included,
   annotations_deleted_candidate_not_exported, annotations_no_prior_file_exports_all,
   annotations_malformed_prior_file_aborts, annotations_composite_identity,
   annotations_incremental_converges)

cargo test --jobs 2   (full suite)
  all binaries: 0 failed

cargo clippy --all-targets -- -D warnings
  clean

cd app && npx tsc --noEmit
  clean (no output)

cd app && npx vitest run
  Test Files  13 passed (13) | Tests  139 passed (139)
```

## Self-Check: PASSED

- `app/src-tauri/src/db/io/export.rs` — FOUND (modified, +`format_annotation_record`, +`read_annotation_id_rows`)
- `app/src-tauri/src/db/io/diff.rs` — FOUND (modified, +`annotations_identity`, +`split_prior_annotation_records`, +`export_annotations_incremental`)
- `app/src-tauri/src/lib.rs` — FOUND (modified, +`export_annotations_incremental` command, registered)
- `app/src-tauri/tests/incremental_export_tests.rs` — FOUND (modified, +8 tests)
- `app/src-tauri/tests/fixtures/wire/annotations_prior.txt` — FOUND
- Commits `acd00d6f` (Task 1), `2be0e05e` (Task 2) — FOUND in `git log --oneline`

## Known Stubs

None. Same as 09-02: the frontend `CategoryList.tsx` "Export changed…" affordance wiring for Annotations is out of this plan's stated file scope (`files_modified` lists only backend files + fixtures) — a follow-on `INCREMENTAL_EXPORT_COMMANDS` map-entry change, not tracked as a stub since the plan never scoped frontend wiring here.

## Threat Flags

None — every new surface (the prior-file parse, the composite identity key, the over-selection summary count, and the malformed-header fallback) was already named in the plan's own `<threat_model>` (T-09-10 through T-09-13, T-09-SC) and mitigated as designed: `parse_annotations_file` runs as a fail-fast gate before any write; identity is the wire-recoverable `(DOC, LABEL)` pair (never `LocationId`), asserted collision-free by `annotations_composite_identity`; the exporter's own record count is carried into the summary as a distinct `exported` figure with a doc comment naming the `LocationId` selection type as the cause; and the diff's own `extract_bracket` header splitter falls back to an empty string per key rather than indexing out of bounds on a malformed header (the shipped `parse_annotations_file` gate already rejects a header lacking `PUB`/`DOC`/`LABEL` before this path is ever reached in practice).
