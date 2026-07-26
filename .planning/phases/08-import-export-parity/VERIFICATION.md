---
phase: 8
status: passed
criteria_passed: 3
criteria_total: 3
---

# Phase 8: Import / Export Parity — Verification Report

**Phase Goal:** the user's existing export files (produced by the Python app, or shared between its users) remain interchangeable with this app in BOTH directions.

## Success Criteria

### 1. Export preserves exact wire warts — PASS

- All five categories (Favorites, Bookmarks, Annotations, Highlights, Notes) have committed golden fixtures on disk: `app/src-tauri/tests/fixtures/wire/{favorites,bookmarks,annotations,highlights,notes}_golden.txt`.
- `app/src-tauri/tests/export_wireformat_tests.rs` byte-compares actual export output against each golden file (`assert_eq!(actual, golden, ...)`) — not a round-trip-through-own-reader test. Doc comment explicitly states the fixtures are "hand-authored to the documented wire format, never produced by running this app's own exporter (would prove only self-consistency, not Python compatibility)" — correctly avoids the tautology trap flagged in the verify brief.
- Header format verified byte-for-byte against `JWLManager.py:1367-1369`, including the load-bearing single-space second line (`db/io/header.rs:37-58`, tested).
- Ran the full wire-format suite locally: `cargo test --test export_wireformat_tests --test import_wireformat_tests` → 23 + 24 = 47 tests, all pass.
- Caveat (not a blocker): fixtures are hand-authored to the documented spec rather than captured from a live Python-app export run. This is an honest, disclosed limitation, not a hidden gap — the doc comment flags it. Recommend a future spot-check against a real Python-produced file if one becomes available, but the current approach is the correct engineering choice absent one.

### 2. Import lands data correctly for any category — PASS

- `db/io/import.rs` provides `dry_run_import_*` / `apply_import_*` pairs for all five categories plus `db/playlist_io.rs` for `.jwlplaylist`, each following the shared `unchecked_transaction` + `PragmaGuard` shape (confirmed via grep: every `dry_run_import_*` opens a `PragmaGuard::new(conn)` before mutating).
- `lib.rs` wires matching Tauri command pairs (`import_favorites_dry_run/apply`, `import_bookmarks_dry_run/apply`, `import_annotations_dry_run/apply`, `import_highlights_dry_run/apply`, `import_notes_dry_run/apply`, `import_playlist_dry_run/apply`) — no bare/unpaired mutation entry point found.
- Zip-slip: `extract_zip_slip_safe` (`archive/extract.rs:22`) is the only zip-open path for untrusted input (playlist container import at `playlist_io.rs:135` and `:560`). The one other raw `zip::ZipArchive::new` call (`archive/new.rs:67`) opens the app's own bundled `res/blank` seed template for New Archive, not untrusted user input — correctly out of scope for zip-slip.
- Parameterization: `format!`-built SQL only interpolates identifier lists/placeholders (`IN ({ph})`, table/column names) — all row *values* go through `params![...]`/`params_from_iter`. No `format!` call carries a user-controlled value into SQL.

### 3. Playlist import re-keys via ID-gap recycling — PASS

- `apply_import_playlist` (`db/playlist_io.rs:975-1070`) matches on the documented semantic triple `(Label, ThumbnailFilePath, Tag Name)` via a joined `SELECT`; on a miss it calls `take_id(available, "PlaylistItem")` (the shared gap-recycler) for a fresh id — the incoming `PlaylistItemId` is never read into the target INSERT. Every dependent row (media map, location map, marker sub-maps, TagMap) is written against the new/reused id, not the source id.
- PD-3 media ordering verified in source, not just claimed: `media_add_apply` (`lib.rs:2412-2482`) stages every DB write into `tx` via `apply_media_add`, then calls `perform_staged_copies` BEFORE `tx.commit()` (`lib.rs:2471` precedes `:2474`). `perform_staged_copies` (`media.rs:567`) returns `Err` on first copy failure and deletes files already written by that call; because the `?`-propagated error returns from `media_add_apply` before reaching `tx.commit()`, `tx` is dropped un-committed — no phantom row can survive a copy failure. This is genuine ordering, not just a code comment.
- `dry_run_delete_playlist_items` (`media.rs:790`) calls only `delete_playlist_items_db` and discards its returned file list; it never calls `remove_media_files`, which is the sole filesystem-touching function in the module (doc comment + call graph both confirm — structurally incapable of reaching file removal, not just "doesn't in practice").
- Ref-counted delete (`delete_playlist_items_db`, `media.rs:618`) computes `used_thumbs`/`used_files` against `PlaylistItemId NOT IN (ids)` — i.e. counts against REMAINING items — with separate thumbnail and full-media passes (D8-07), matching the spec.
- PD-1: `grep` of `Cargo.toml` confirms no `image`, `rand`, `uuid`, or `fancy-regex` dependency was added; thumbnail handling is a byte copy.
- D7-03 invariant: `color.rs` contains no reference to `merge_block_ranges`; `merge_block_ranges` (`highlights.rs:111`) is called only from the import path (`db/io/usermark.rs:90`). Its new `recycled_id: Option<i64>` parameter is purely an INSERT-target branch (`if let Some(id) = recycled_id { INSERT ... id } else { INSERT ... autoincrement }`) — the `plan_merge` geometry call above it is unchanged. The one `format!` in `color.rs:128` builds only a placeholder list for `UserMark.UserMarkId IN (...)`; the `ColorIndex` value is bound via `?`, not interpolated.

## Additional checks

- No real `.jwlibrary` archives found in `tests/fixtures/` — synthetic fixtures only.
- Cross-AI review gate was unavailable for this run (per team-lead note); this verification serves as the compensating control and found no blocker-class issues.

## Verdict

**PASS — 3/3 success criteria met.** No blockers found. One disclosed, non-blocking caveat: golden fixtures are hand-authored to spec rather than captured from a live Python export (correct engineering choice, but worth a future spot-check against a real Python-produced file). Ready to ship.

---
_Verified: 2026-07-26_
_Verifier: Claude (independent goal-backward verification, compensating for unavailable cross-AI review gate)_
