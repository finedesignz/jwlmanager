---
phase: 07-full-editing
plan: "05"
subsystem: editing
tags: [record-editor, sql-safety, per-category-delete, round-trip, react]
dependency-graph:
  requires: ["07-01 db/edit.rs safety spine", "07-01 EditPreviewDialog", "07-02 db/color.rs UserMark synthesis", "07-02 db/delete.rs delete_highlights", "07-03 db/tags.rs", "07-03 db/reorder.rs", "07-04 db/scrub.rs"]
  provides: ["Record Editor (EDIT-07)", "Bookmarks delete (D7-10)", "Annotations delete (D7-10)", "cross-op round-trip suite (ROADMAP criterion 5)"]
  affects: [app/src-tauri/src/db, app/src-tauri/src/error.rs, app/src-tauri/src/lib.rs, app/src/components, app/src/lib/operations.ts, app/src/styles.css]
tech-stack:
  added: []
  patterns:
    - "record_edit.rs reuses db::color::apply_color's Notes branch VERBATIM for UserMark synthesis rather than duplicating it — one implementation of the synthesis path, not two"
    - "a small record-scoped fetch command (record_fetch) added beyond the plan's named command list, because BrowseRow (the category-list row) never carries a Note's own Title/Content or an Annotation's own Value — those are publication-label metadata, not editable content, mirroring the Python Data Viewer's own separate get_notes()/get_annotations() fetch rather than reusing the browse-list row"
    - "BrowseRow gains text_tag: Option<String> (Annotations only) so the frontend can disambiguate rows sharing a LocationId with different TextTags — a pre-existing gap the record editor's (LocationId, TextTag) identity exposed"
key-files:
  created:
    - app/src-tauri/src/db/record_edit.rs
    - app/src-tauri/tests/record_edit_tests.rs
    - app/src-tauri/tests/edit_roundtrip_tests.rs
    - app/src/components/RecordEditor.tsx
    - app/src/components/RecordEditor.test.tsx
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/db/delete.rs
    - app/src-tauri/src/db/browse.rs
    - app/src-tauri/src/db/notes.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src/components/CategoryList.tsx
    - app/src/components/CategoryList.test.tsx
    - app/src/components/ColorMenu.tsx
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
    - app/src/styles.css
    - app/src/App.test.tsx
decisions:
  - "Added a record_fetch Tauri command (and db::record_edit::fetch_record_fields) not explicitly named in the plan's key_links, because the record editor needs the record's CURRENT Title/Content/ColorIndex or Value to prefill the form, and BrowseRow never carries those fields (it only carries publication-label metadata). This mirrors the Python Data Viewer's own separate get_notes()/get_annotations() query rather than reusing the browse-list row — a Rule 2 addition (missing critical functionality: without it the editor would open with empty/stale fields)."
  - "Added text_tag: Option<String> to BrowseRow (Annotations only, sourced from InputField.TextTag) so the record editor can identify exactly which (LocationId, TextTag) row is selected. Annotations' browse-list identity was previously LocationId alone, which is not unique across TextTags at one location — a real pre-existing gap the D7-09 record-editor identity requirement exposed. This is an additive, backward-compatible field (Rule 2)."
  - "RecordEditPayload/RecordIdentity/RecordEditFields are tagged enums keyed \"Notes\"/\"Annotations\" (matching the existing Category enum's plural naming), not singular \"Note\"/\"Annotation\" — consistent with db::color::ColorSelection's established \"Highlights\"/\"Notes\" tag convention."
  - "The record editor's own Annotation delete (apply_record_delete, keyed by (LocationId, TextTag)) never over-deletes, so RecordEditor.tsx's over-deletion summary override (report.deleted.InputField > 1) is a defensive backstop per 07-UI-SPEC.md's literal wording, not a path expected to actually fire from this component — the real over-deletion behavior lives in the browse-list's annotation_delete_* commands (delete_annotations, by LocationId), which the record editor's delete path never touches."
metrics:
  duration: "single session"
  completed: 2026-07-26
status: complete
---

# Phase 7 Plan 5: Record Editor (EDIT-07) + Remaining Deletes + Round-Trip Suite Summary

Shipped the field-constrained record editor for Notes and Annotations, the two remaining per-category deletes deferred from Phase 6 (Bookmarks, Annotations), and the cross-op semantic round-trip suite that closes out ROADMAP criterion 5 — the final plan of Phase 7.

## What Was Built

**Task 1 (commit `fa280dda`) — `db/record_edit.rs` + `db/delete.rs` additions.** `record_edit.rs` implements the field-constrained editor backend (EDIT-07, D7-09): `RecordEditPayload` is a tagged enum — `Notes { note_id, title, content, color_index: Option<i64> }` and `Annotations { location_id, text_tag, value }` — with no table name, column name, or SQL fragment ever crossing the IPC boundary. `apply_record_edit(tx, payload, now, guid_seed)` reuses `db::color::apply_color`'s `Notes` branch VERBATIM when `color_index` is `Some`, synthesizing a `UserMark` exactly as the Color Menu path does, then updates `Title`/`Content`/`LastModified`; Annotations update only `InputField.Value` for `(LocationId, TextTag)`. `apply_record_delete` deletes exactly one record — `NoteId` for Notes, `(LocationId, TextTag)` for Annotations — kept structurally distinct from the browse-list's over-deleting `delete_annotations` (by `LocationId` alone). A `fetch_record_fields` command (`record_fetch`) was added beyond the plan's named commands (see Decisions) so the editor can prefill current values. `db/delete.rs` gains `NonEmptyBookmarkIds`/`delete_bookmarks`/`dry_run_delete_bookmarks` and `NonEmptyLocationIds`/`delete_annotations`/`dry_run_delete_annotations` (the browse-list's intentional by-`LocationId` over-delete, rule #10), both reusing the existing `TRACKED_TABLES` default (already extended in 07-01-PLAN.md with `Bookmark`/`InputField` entries). `error.rs` gains `ArchiveError::RecordEditFailed` → `record_edit_failed`. `lib.rs` registers `record_fetch`, `record_edit_dry_run`/`_apply`, `record_delete_dry_run`/`_apply`, `bookmark_delete_dry_run`/`_apply`, `annotation_delete_dry_run`/`_apply`. `BrowseRow` gains `text_tag: Option<String>` (Annotations only) to disambiguate rows sharing a `LocationId`. 10 tests in `tests/record_edit_tests.rs` cover every behavior bullet: Note field save + `LastModified` stamping, color synthesis for an unmarked note, existing-`UserMark` color update with no new synthesis, Annotation Value update leaving a sibling `TextTag` at the same `LocationId` untouched, single-record delete for both categories (with the sibling-survives assertion for Annotations), deterministic `LastModified` across two calls with the same injected `now`, and `fetch_record_fields` for both categories including the no-`UserMark` case.

**Task 2 (commit `f7d80b05`) — `RecordEditor.tsx` + `CategoryList.tsx` wiring + capability flips.** `RecordEditor.tsx` is a modal that fetches current field values on open (`record_fetch`), renders Title (single-line-styled text input)/Content (textarea)/the 7-swatch Color row (imported verbatim from `ColorMenu`'s now-exported `PALETTE`, including its "No color" muted state when `UserMarkId IS NULL`) for Notes, or just Value (textarea) for Annotations. "Save Changes" fires `record_edit_dry_run` → `EditPreviewDialog` ("Save these changes?") → `record_edit_apply`; "Delete" fires `record_delete_dry_run` → `EditPreviewDialog` ("Delete this Note?"/"Delete this Annotation?") → `record_delete_apply`, carrying the annotation over-deletion summary override per 07-UI-SPEC.md (see Decisions on why it's a defensive backstop here). The Content/Value `<textarea>` wraps and scrolls internally (`.record-editor-textarea`, `white-space: pre-wrap; overflow-wrap: break-word; overflow-y: auto`) — the first real multi-line textarea in the app, a deliberate, commented deviation from the list-row ellipsis convention. `CategoryList.tsx` renames `OP_LABEL.view` → "Edit", wires `Bookmarks`/`Annotations` deletes into `DELETE_COMMANDS`, and adds the `selectionSize === 1` precondition for the `view`/"Edit" op — present-but-`disabled` with `title="Select exactly one row to edit"` at 2+ selected, using the single matching `BrowseRow` (found via `rows.find(r => selected.has(r.id))`) to open `RecordEditor`. `operations.ts`'s `LIVE` set gains `Notes:view`, `Annotations:view`, `Bookmarks:delete`, `Annotations:delete`; `Playlists:add`/`Playlists:delete` remain deferred. Pre-existing tests in `operations.test.ts` and `CategoryList.test.tsx` that asserted the OLD `LIVE` state (Bookmarks fully deferred) were updated to the new truth rather than deleted, per the autonomy note. 15 new/updated frontend tests cover: Notes render three fields vs. Annotations render one, the "No color" state, a linked-`UserMark` selected-swatch state, Cancel firing zero non-fetch invocations, Save firing exactly one `record_edit_dry_run`, the over-deletion summary at `report.deleted.InputField === 2`, and the Edit button's disabled/title states at selection sizes 0/1/2.

**Task 3 (commit `4517963c`) — `edit_roundtrip_tests.rs`.** One full seed → apply → `save_archive` (real trim + VACUUM) → reopen → assert-normalized-state test per op group, using `common::generate_v16_all_categories_fixture` as the shared base (extended per-test with targeted synthetic rows where an op needed data the base fixture doesn't carry — a fresh Tag/Notes trio for reorder, a sibling `InputField` TextTag for the record-edit test): **color** (recoloring a plain Note synthesizes a `UserMark` and the color survives save), **highlights merge** (`merge_block_ranges` coalesces an overlapping range at one `Identifier` while a different `Identifier`'s range on the same `UserMark` survives untouched), **tags** (adding an existing tag and a brand-new tag to a Note both land and survive save's `TagMap` re-densify), **reorder** (a gapped `Type=1` tag's positions end up 0-based dense, ordered by `NoteId`, and stay that way after save's own trim-path re-densify runs on top — idempotent composition), **favorites** (unmarking an existing favorite and marking a new edition both survive save), **clean** (an NBSP in an Annotation's Value normalizes to an ASCII space and survives save), **mask** (letters replaced, non-letter positions preserved, CHARACTER count — not byte length, since the `børk` mask word has a non-ASCII character — preserved, and the masked text survives save), and **record edit** (a Note's Title/Content/Color save and a scoped Annotation `(LocationId, TextTag)` delete both survive save, with the sibling `TextTag` at the same location intact). Every assertion is a normalized SQL query against the reopened archive — never a byte/hash comparison, per CLAUDE.md's Core Value (save is not byte-preserving: VACUUM, mask's RNG, fresh timestamps).

## Deviations from Plan

### Auto-fixed Issues (Rule 2 — missing critical functionality)

**1. Added `record_fetch` command + `db::record_edit::fetch_record_fields`, not named in the plan's key_links.**
- **Found during:** Task 1, while designing `RecordEditor.tsx`'s data flow.
- **Issue:** `BrowseRow` (the category-list row shape) never carries a Note's own `Title`/`Content` or an Annotation's own `Value` — it only carries publication-label metadata (`symbol`/`short`/`full`/`detail1`/`detail2`). Without a fetch, the editor would have no way to prefill the form with the record's actual current content.
- **Fix:** Added `RecordIdentity` (a small tagged enum distinct from `db::delete`'s per-category selection types — carries exactly one id, never a selection), `RecordEditFields`, and `fetch_record_fields(conn, &identity)`, exposed as the `record_fetch` command. Mirrors the Python Data Viewer's own separate `get_notes()`/`get_annotations()` fetch (`JWLManager.py:3041`, `:3125`) rather than reusing the browse-list row.
- **Files modified:** `app/src-tauri/src/db/record_edit.rs`, `app/src-tauri/src/lib.rs`.
- **Commit:** `fa280dda`.

**2. Added `text_tag: Option<String>` to `BrowseRow`, not named in the plan.**
- **Found during:** Task 1, while resolving how the record editor identifies which `(LocationId, TextTag)` Annotation row is selected.
- **Issue:** The Annotations browse-list identity (`BrowseRow.id`) is `LocationId` alone (`browse.rs:28-31`), which is NOT unique when a location carries more than one `InputField`/`TextTag` — a real pre-existing gap (Phase 6 never needed per-TextTag identity since it had no per-record edit surface). The record editor's D7-09 `(LocationId, TextTag)` identity requirement exposed this.
- **Fix:** Added `text_tag: Option<String>` to `BrowseRow` (populated only by the Annotations query from `InputField.TextTag`; every other category sets `None`), and had `CategoryList.tsx` pass the full selected `BrowseRow` (not just its `id`) into `RecordEditor`, which reads `row.text_tag` to build the identity.
- **Files modified:** `app/src-tauri/src/db/notes.rs`, `app/src-tauri/src/db/browse.rs`, `app/src/components/CategoryList.tsx`, `app/src/components/RecordEditor.tsx`.
- **Commit:** `fa280dda`.

None of the seven prohibitions or must_haves required a deviation from the plan's literal text beyond these two additive, backward-compatible extensions.

## Known Stubs

None. The record editor is fully wired end-to-end; Bookmarks and Annotations deletes are live; the round-trip suite covers every op group.

## Self-Check: PASSED

- `app/src-tauri/src/db/record_edit.rs` — FOUND
- `app/src-tauri/tests/record_edit_tests.rs` — FOUND
- `app/src-tauri/tests/edit_roundtrip_tests.rs` — FOUND
- `app/src/components/RecordEditor.tsx` — FOUND
- Commit `fa280dda` (Task 1) — FOUND in `git log`
- Commit `f7d80b05` (Task 2) — FOUND in `git log`
- Commit `4517963c` (Task 3) — FOUND in `git log`

## Verification

- `cd app/src-tauri && cargo test --jobs 2` — full Rust suite: **all green, 0 failed** (includes 10/10 in `record_edit_tests`, 8/8 in `edit_roundtrip_tests`, and every pre-existing binary unaffected).
- `cargo clippy --all-targets -- -D warnings` — clean (only the pre-existing, unrelated `ts-rs`/`try_from` proc-macro parse notices, not clippy lints, and not new to this plan).
- `cd app && npx vitest run` — **120/120 passed**, including 6 new `RecordEditor.test.tsx` tests and 4 new/updated `CategoryList.test.tsx` Edit-precondition tests.
- `npx tsc --noEmit` — clean.
- Acceptance-criteria greps: `fn record_edit`/`fn record_fetch`/`fn record_delete` signatures in `lib.rs` take only the typed `RecordEditPayload`/`RecordIdentity` — no `table`/`column`/`sql` parameter; `grep -n "\"Edit\""` matches in `CategoryList.tsx` and `grep -rn "\"View\""` over `app/src` returns nothing; `grep -n "Playlists" app/src/lib/operations.ts` shows neither `add` nor `delete` in the `LIVE` set; `edit_roundtrip_tests.rs` contains no hash/byte-comparison assertion.
- One acceptance-criterion grep (`grep -rn "DELETE FROM UserMark" app/src-tauri/src` returns no matches) is NOT satisfied as literally written: `db/trim.rs:79` has always contained a `DELETE FROM UserMark` for orphan-sweep cleanup (Phase 1/2, pre-existing, unrelated to this plan's record-edit/delete additions). No new `record_edit.rs`/`delete.rs` code introduces such a statement — `record_edit`'s Notes save path never deletes a `UserMark`, only `apply_color`'s existing INSERT/UPDATE path — so the intent of the check (this plan doesn't add a new `UserMark`-deleting statement) is satisfied; the grep's literal scope (the whole `src/` tree) was always going to hit `trim.rs`.
- Manual save-then-reopen-in-JW-Library verification (per 07-VALIDATION.md) was not run this session — deferred, same as prior Phase 7 plans; the round-trip suite's own real `save_archive` + reopen + normalized-query coverage is the automated proxy.
