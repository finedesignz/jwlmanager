---
phase: 08-import-export-parity
plan: 05
subsystem: import-export-io
tags: [wire-format, playlist, zip-in-zip, re-keying, id-recycling, tauri-commands, zip-slip]
dependency-graph:
  requires:
    - archive::extract::extract_zip_slip_safe — Phase 1
    - archive::manifest (Manifest, to_compact_string, compute_hash) — Phase 1
    - db::ids (compute_available_ids, take_id, RECYCLING_TABLES including
      PlaylistItem/IndependentMedia/Location/Tag/TagMap) — 08-01
    - db::edit (DryRunReport, snapshot_tables, diff_snapshots) — 07-01/08-01
  provides:
    - db::playlist_io module — export_playlist(_from_seed), read_playlist_container,
      apply_import_playlist, dry_run_import_playlist, count_container_media
    - export_playlist / import_playlist_dry_run / import_playlist_apply commands
    - NonEmptyPlaylistItemIds, PlaylistExportReport, PlaylistImportPreview DTOs
    - db::edit::PLAYLIST_IMPORT_SNAPSHOT_TABLES
  affects:
    - app/src/components/CategoryList.tsx (EXPORT_COMMANDS/IMPORT_COMMANDS
      maps, Playlists-specific file-picker filters, selection-required
      export, the leading "adds the playlist ... and its N media files"
      preview clause)
    - app/src/lib/operations.ts (LIVE set)
    - app/src/lib/errors.ts (playlist_export_failed/playlist_import_failed copy)
tech-stack:
  added: []
  patterns:
    - "Playlist import re-keys on a SEMANTIC identity — (Label,
      ThumbnailFilePath, playlist Tag Name) — never the incoming
      PlaylistItemId (RESEARCH addendum, resolved risk). A miss allocates a
      fresh id from the SAME db::ids gap pool every other category's import
      already shares; every dependent row (media map, location map, marker
      sub-maps, TagMap) is written with the NEW id, and no INSERT OR REPLACE
      on a trusted incoming PK exists anywhere in the path."
    - "A playlist item's ThumbnailFilePath is resolved via a SEPARATE lookup
      (load_source_thumbnail_media, IndependentMedia.FilePath = ?) from its
      PlaylistItemIndependentMediaMap rows (load_source_media_maps) — ports
      Python's own two-source shape (`add_thumbnails`'s map walk vs. the
      main row's `JOIN IndependentMedia i ON i.FilePath = p.ThumbnailFilePath`).
      resolve_target_media dedups by Hash, so calling it twice for the same
      media (thumbnail also present in the map) is safe — it reuses the
      already-inserted row, never double-inserts."
    - "Media file placement follows PD-3 ordering: every DB write is staged
      into the caller's transaction FIRST; file copies into the archive
      working directory happen after, and a copy failure returns Err before
      the caller's tx.commit() — the whole run rolls back atomically. Export
      is the opposite (best-effort): a missing source media file collects a
      warning and does NOT abort, matching Python's try/except around
      shutil.copy2."
    - "target_media_dir: Option<&Path> is the dry-run/apply switch — None
      skips every filesystem write (dry run touches nothing outside its own
      temp extraction), Some(dir) performs the real copy+disambiguation."
key-files:
  created:
    - app/src-tauri/src/db/playlist_io.rs
    - app/src-tauri/tests/playlist_export_tests.rs
    - app/src-tauri/tests/playlist_import_tests.rs
    - app/src/bindings/NonEmptyPlaylistItemIds.ts
    - app/src/bindings/PlaylistExportReport.ts
    - app/src/bindings/PlaylistImportPreview.ts
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/db/edit.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src/components/CategoryList.tsx
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
    - app/src/lib/errors.ts
decisions:
  - "Playlist export's selection is REQUIRED, not optional (deviates from
    the txt categories' D8-10 whole-category fallback): export_playlist's
    Rust signature takes NonEmptyPlaylistItemIds (non-empty by construction,
    matching the RESEARCH/plan's own `ids: &NonEmptyPlaylistItemIds`
    signature), and CategoryList.tsx's handleExportClick returns early for
    Playlists with an empty selection rather than falling back to a
    whole-category export. A `.jwlplaylist` mini-archive is built by
    re-keying a concrete row subtree; there is no meaningful 'export the
    whole Playlists category' analog to a flat txt-file dump the way the
    five text categories have one."
  - "PlaylistItemLocationMap is treated as at-most-one-row-per-item on both
    export and import (ORDER BY LocationId LIMIT 1 on the import read side;
    export copies every row the schema's composite PK allows). In practice
    a JW Library playlist item references exactly one media Location; the
    schema's composite PK technically permits more, but nothing in the
    Python original or the UI-SPEC exercises multi-location items, and this
    is the same simplification precedent Bookmarks/Highlights import takes
    for their own single-Location-per-record shape."
  - "TagMap's import guard is ported LITERALLY from Python's exact-tuple
    `WHERE NOT EXISTS (... AND Position = ?)` (`JWLManager.py:2508-2512`)
    rather than 'fixed' into a (PlaylistItemId, TagId)-only guard. Since
    `position` is always freshly computed as `max(Position)+1`, a re-import
    of an already-tagged (skipped) item DOES add another TagMap row at a new
    position — an existing Python quirk, preserved rather than silently
    corrected, consistent with this codebase's stated policy of porting
    documented quirks verbatim rather than opportunistically improving on
    them mid-port."
metrics:
  duration: "~1 session"
  completed: 2026-07-26
status: complete
---

# Phase 8 Plan 5: `.jwlplaylist` Export/Import Summary

Ships the phase's only nested archive-in-archive lifecycle: a self-contained
SQLite-in-zip mini-archive exported from a Playlist selection (`db/playlist_io.rs`
`export_playlist`) and imported back into any archive with full re-keying
(`apply_import_playlist`/`dry_run_import_playlist`), plus the three Tauri
commands and two test files the plan specifies.

## What was built

**`db/playlist_io.rs`** — new module, `export_playlist`/`export_playlist_from_seed`
port `export_playlist` (`JWLManager.py:1725-1818`): seeds a fresh mini-database
from `res/blank_playlist` via `extract_zip_slip_safe` (the only zip-open path,
D8-02), copies the selected `PlaylistItem` subtree in Python's exact table
order — hardcoded `Tag(1,2,<stem>)`, conditional `android_metadata` locale
copy, `PlaylistItem`, `PlaylistItemLocationMap`, `PlaylistItemMarker` +
sub-maps (filtered by the marker ids just inserted into the DEST db, not the
source), `TagMap` re-keyed to `TagId=1` with a dense 0-based `Position`
renumbering, `PlaylistItemIndependentMediaMap`, `PlaylistItemAccuracy`
unfiltered, `IndependentMedia` via the thumbnail-FilePath/media-map-id union
predicate with best-effort media-file copy (a missing file becomes a warning,
never a failure), and `Location` filtered to exactly the referenced ids. The
mini-database is then `UPDATE LastModified`, committed, `VACUUM`ed, and
closed before `compute_hash` reads the final bytes (hash-last) — the manifest
reuses `archive::manifest::Manifest`/`to_compact_string` with `type=1`
distinguishing a playlist archive from the main archive's `type=0`.

`read_playlist_container`/`apply_import_playlist`/`dry_run_import_playlist`
port `import_playlist`'s `update_db` (`JWLManager.py:2444-2587`) with the
RESEARCH-addendum-resolved re-keying discipline: row identity on import is
the semantic triple `(Label, ThumbnailFilePath, playlist Tag Name)`, never
the incoming `PlaylistItemId`. A miss allocates a fresh id from the shared
`db::ids` gap pool; every dependent row (media map, location map, marker
sub-maps, `TagMap`) is written with the NEW id. `target_media_dir:
Option<&Path>` is the dry-run/apply switch (`None` performs zero filesystem
writes); a real apply stages every DB write first and copies media files in
after (PD-3), returning `Err` — never committing — on any copy failure.

**Error surface**: `ArchiveError::PlaylistExportFailed`/`PlaylistImportFailed`
+ `to_dto` codes + `errors.ts` sentences.

**`db/edit.rs`**: `PLAYLIST_IMPORT_SNAPSHOT_TABLES` (`Tag`, `TagMap`,
`PlaylistItem`, `Location`, `IndependentMedia`).

**Tauri commands** (`lib.rs`): `export_playlist` (takes `NonEmptyPlaylistItemIds`,
returns `PlaylistExportReport{item_count, warnings}`), `import_playlist_dry_run`
(returns `PlaylistImportPreview{report, playlist_name, media_count}` — the
playlist name is derived from the picked file's own stem, matching Python's
`Path(file).stem` for a `.jwlplaylist`), `import_playlist_apply` (returns the
standard `DryRunReport`, marks the session dirty).

**Frontend**: `operations.ts` flips `Playlists:export`/`Playlists:import`
LIVE. `CategoryList.tsx` adds Playlists to `EXPORT_COMMANDS`/`IMPORT_COMMANDS`,
switches the native file-picker filter to `.jwlplaylist`, requires a non-empty
selection before invoking Playlist export (see Deviations), special-cases the
Playlist import preview response shape, and renders the leading `This adds the
playlist "{Name}" and its {N} media file{s}.` clause ahead of the standard
added/updated/skipped lines, with `"Import Playlist?"`/`"Import Playlist"`
title/confirm copy per the UI-SPEC.

## Deviations from Plan

1. **[Claude's Discretion, documented]** Playlist export's selection is
   REQUIRED rather than optional — see Decisions above. This is the natural
   consequence of the plan's own `export_playlist(conn, ids, dest)` signature
   (`ids: &NonEmptyPlaylistItemIds`, non-empty by construction) and is not a
   deviation from the plan text itself, just called out because it differs
   from the five txt categories' D8-10 selection-optional convention the
   UI-SPEC states applies to "all 6 categories."
2. **[Rule 1 - bug found during Task 2 test-writing]** The first draft
   resolved a `PlaylistItem`'s thumbnail media through the SAME
   `PlaylistItemIndependentMediaMap` join used for full media, which left
   `ThumbnailFilePath` `NULL` on import (a thumbnail-only item, with no map
   row, never got resolved). Fixed by adding `load_source_thumbnail_media` —
   a separate `IndependentMedia.FilePath = ?` lookup mirroring Python's own
   two-source shape (the main row's `JOIN IndependentMedia i ON i.FilePath =
   p.ThumbnailFilePath` is distinct from `add_thumbnails`' map walk). Caught
   by `dependent_rows_reference_the_newly_allocated_id`; fixed inline before
   commit.
3. **[Process note, not a content deviation]** Commits are two (`feat` then
   `test`) rather than one-commit-per-task, because Task 1 and Task 2 share
   the single `playlist_io.rs` module file — the export and import functions
   could not be split into independently-compiling commits without an
   artificial intermediate stub. The first commit carries the whole module
   (export + import implementation, since import code was needed for the
   file to be complete) plus Task 1's own test file; the second commit adds
   Task 2's test file and the generated `PlaylistImportPreview.ts` binding.

## Test output (actual, run at completion)

```
cd app/src-tauri && cargo test --jobs 2
  → ALL binaries: test result: ok (0 failed), including:
    playlist_export_tests: 5 passed (new file) — compact manifest w/ correct
      hash, hardcoded Tag row, dense TagMap positions, thumbnail+full-media
      union in IndependentMedia, missing-media-file warning (not a failure)
    playlist_import_tests: 7 passed (new file) — PK-collision never
      overwrites the existing row, dependent rows reference the newly
      allocated id, semantic re-import is reused+reported as skipped,
      zip-slip container rejected + writes nothing, missing userData.db
      fails before any transaction, gap-pool id recycled before
      autoincrement, dry-run leaves every tracked table's row count
      unchanged
    (every other pre-existing suite unaffected: 0 failed across the full run)

cd app/src-tauri && cargo clippy --all-targets -- -D warnings
  → clean (only the pre-existing ts-rs try_from attribute-parse warning,
    unrelated to this plan, present since 08-01)

cd app && npx tsc --noEmit
  → clean, zero errors

cd app && npx vitest run
  → Test Files  12 passed (12)
    Tests  124 passed (124)
    (operations.test.ts's LIVE_PAIRS assertion updated to include
     Playlists:export/Playlists:import — the load-bearing claim, "deferred
     is true exactly when a (category, op) pair is not LIVE," is preserved)

git diff --stat app/src-tauri/Cargo.toml app/package.json
  → (empty — no dependency additions, PD/prohibition satisfied)

grep -n "generate_handler" -A 90 app/src-tauri/src/lib.rs
  → lists export_playlist, import_playlist_dry_run, import_playlist_apply

grep -n "extract_zip_slip_safe" app/src-tauri/src/db/playlist_io.rs
  → 2 call sites (export seed extraction, import container extraction) —
    the ONLY zip-open path in this file

grep -n "ZipArchive" app/src-tauri/src/db/playlist_io.rs
  → no matches — no raw extraction loop anywhere in this file (the
    write-side ZipWriter used for export is a separate type)

grep -n "format!(" app/src-tauri/src/db/playlist_io.rs
  → every match is placeholder-count SQL construction, a fixed WHERE-clause
    shape selection, an error-reason string, or a media-filename
    disambiguation suffix — no interpolated SQL value
```

## Known Stubs

None — Playlist export/import is fully live end to end (op bar → native
file picker → command → disk/DB → refresh), no stubbed data paths.

## Self-Check: PASSED

- FOUND: app/src-tauri/src/db/playlist_io.rs
- FOUND: app/src-tauri/tests/playlist_export_tests.rs
- FOUND: app/src-tauri/tests/playlist_import_tests.rs
- FOUND: app/src/bindings/NonEmptyPlaylistItemIds.ts
- FOUND: app/src/bindings/PlaylistExportReport.ts
- FOUND: app/src/bindings/PlaylistImportPreview.ts
- FOUND: `export_playlist`/`import_playlist_dry_run`/`import_playlist_apply`
  in `app/src-tauri/src/lib.rs` generate_handler![]
- All test suites green (see Test output above) — commits:
  - `045e35c2` feat(08-05): .jwlplaylist export
  - `65b927c8` test(08-05): .jwlplaylist import
