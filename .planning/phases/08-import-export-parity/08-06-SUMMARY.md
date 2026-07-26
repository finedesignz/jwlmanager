---
phase: 08-import-export-parity
plan: 06
subsystem: playlist-media
tags: [playlist, media, filesystem, dedup, sha256, ref-counting, tauri-commands, checkpoint]
dependency-graph:
  requires:
    - db::ids (compute_available_ids, take_id) — 08-01
    - db::edit (DryRunReport, snapshot_tables, diff_snapshots, PragmaGuard-based dry-run shape) — 07-01/08-01
    - db::playlist_io::NonEmptyPlaylistItemIds — 08-05
    - guid::format_guid_v4 — 07-RESEARCH Shared Pattern 6
    - archive/manifest.rs's Sha256::digest hex-hashing pattern (reused verbatim) — Phase 1
  provides:
    - db::media module — sniff_format, media_precheck, apply_media_add,
      perform_staged_copies, delete_playlist_items_db, remove_media_files,
      dry_run_delete_playlist_items
    - media_add_precheck / media_add_apply / playlist_delete_dry_run /
      playlist_delete_apply commands
    - MediaPrecheckResult / MediaAddApplyReport / PlaylistDeleteReport DTOs
    - db::edit::MEDIA_DELETE_SNAPSHOT_TABLES
  affects:
    - app/src/components/CategoryList.tsx (Playlists add renamed "Add
      Media..." and wired; Playlists delete wired with the shared-media
      summary clause)
    - app/src/lib/operations.ts (LIVE set — Playlists:add, Playlists:delete)
tech-stack:
  added: []
  patterns:
    - "PD-3 staged-DB-then-files commit, the project's FIRST on-disk file
      writes: apply_media_add stages every DB row into the caller's
      transaction and every file copy into a Vec<PendingCopy>;
      perform_staged_copies runs the copies AFTER the DB half is staged, and
      on ANY copy failure deletes every file it had already written this
      call before returning Err. The caller (media_add_apply command) never
      commits the transaction on that Err — it is simply dropped, rolling
      back the whole batch atomically. Neither a phantom row nor a
      half-written batch can survive."
    - "D8-07 two-pass, INDEPENDENT used-set reference counting for playlist
      media delete: used_thumbs (ThumbnailFilePath of items NOT selected)
      and used_files (FilePath via IndependentMedia/PlaylistItemIndependent-
      MediaMap for items NOT selected) are computed and consulted in two
      SEPARATE loops, exactly porting Python's own two-loop shape
      (JWLManager.py:3627-3647) rather than a unified 'is this file used
      anywhere' check — a file protected by EITHER set is counted once
      (a local kept HashSet dedups) but never protected by a role it
      doesn't actually play in a given loop."
    - "Structural (not merely behavioral) SAFE-01: dry_run_delete_playlist_items
      calls ONLY delete_playlist_items_db and discards its returned file
      list — remove_media_files is a SEPARATE function nothing in the
      dry-run call graph can reach, so a future edit to the dry-run path
      cannot accidentally introduce a filesystem write without also adding a
      brand-new call site."
    - "Thumbnail is a byte-for-byte COPY of the source file, never a 250x250
      resize (PD-1) — the image crate could not be legitimacy-verified
      (08-RESEARCH.md addendum). The two IndependentMedia rows this produces
      per new file therefore share the SAME content hash; apply_media_add
      tracks a batch-local hash->(media_id, thumb_name) map so this known
      same-hash pair is never mistaken for an intra-batch duplicate."
key-files:
  created:
    - app/src-tauri/src/db/media.rs
    - app/src-tauri/tests/media_add_tests.rs
    - app/src-tauri/tests/media_delete_tests.rs
    - app/src/components/MediaAddDialog.tsx
    - app/src/components/MediaAddDialog.test.tsx
    - app/src/bindings/MediaPrecheckResult.ts
    - app/src/bindings/MediaAddApplyReport.ts
    - app/src/bindings/PlaylistDeleteReport.ts
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/db/edit.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/tests/common/mod.rs
    - app/src/components/CategoryList.tsx
    - app/src/components/CategoryList.test.tsx
    - app/src/lib/operations.ts
    - app/src/lib/operations.test.ts
    - app/src/lib/errors.ts
    - app/src/styles.css
decisions:
  - "media_add_apply is designed as an ATOMIC all-or-nothing batch, not a
    per-file-failure-tolerant apply: it re-runs media_precheck fresh on the
    given paths (never trusting a stale client-side classification),
    filters to New, and either the WHOLE batch lands or NONE of it does
    (PD-3). This matches the UI-SPEC's own framing ('a whole-operation
    failure renders the media_add_failed sentence rather than a per-row
    message') and simplifies the frontend: apply never needs its own
    per-file result DTO, because a copy failure can never partially land."
  - "[Claude's Discretion, documented deviation from 08-UI-SPEC.md] The
    UI-SPEC's Add Media copy never specifies how the destination playlist
    (its Tag Name) is chosen -- Python's own dialog has a 'select existing
    playlist or type a new name' combo box with no UI-SPEC equivalent
    described for this port. Rule 2 (auto-add missing critical
    functionality): without SOME way to name the destination playlist the
    command cannot function, so MediaAddDialog adds a single text input
    ahead of the file-result list (Confirm additionally requires a
    non-empty trimmed name). No new color/spacing token -- reuses the
    app's existing 44px/--bg-tertiary text-input box metrics."
  - "The 'Copying files... {i} of {N}' counter renders 'i' fixed at 0
    throughout the apply call, never animating to N mid-request -- the
    Tauri command is a single synchronous invoke with no progress-event
    channel wired up, so there is no true intermediate 'i' to report. This
    is an honest simplification, not a fabricated increment: the label
    still shows a real, correct N and never claims false progress.
    08-UI-SPEC.md's own backstop framing for this loading state ('whether
    it's perceptible depends on real timing... acceptable, not a defect')
    covers this; a follow-up phase adding a Tauri progress-event emitter
    from media_add_apply is the natural way to make 'i' live."
  - "kept_count in PlaylistMediaDeleteOutcome/PlaylistDeleteReport is a
    SINGLE HashSet<String> insertion point shared by both the thumbnail and
    full-media loops -- a file protected by BOTH used-sets simultaneously
    (proven by the thumbnail_and_full_media_used_sets_are_evaluated_independently
    test) is counted exactly ONCE in the delete-preview's 'kept' clause,
    never twice, even though the two loops evaluate it independently."
metrics:
  duration: "~1 session"
  completed: 2026-07-26
status: complete
---

# Phase 8 Plan 6: Playlist Media Add + Ref-Counted Delete Summary

Closes the phase's two deferred-from-Phase-7 operations -- playlist media
add (content-hash dedup, magic-byte gate, staged-DB-then-files commit) and
playlist item delete (two-pass, independent-used-set media reference
counting) -- the project's first on-disk file writes and first irreversible
on-disk removals. `Playlists:add` and `Playlists:delete` are now the last
two `(category, op)` slots in the app to go LIVE.

## What was built

**`db/media.rs`** (new module) --

- `sniff_format(bytes) -> Option<MediaFormat>` -- a hand-written magic-byte
  prefix table (BMP `BM`, GIF `GIF87a`/`GIF89a`, JPEG `FF D8 FF`, PNG's
  8-byte signature, HEIC's ISOBMFF `ftyp` box with a 9-brand allowlist).
  No dependency, no decoder -- the `puremagic` equivalent (PD-1).
- `media_precheck(conn, paths) -> Vec<MediaPrecheck>` -- classifies each
  selected file as `New`/`Duplicate{existing_media_id}`/
  `Unsupported{reason}` against a single preload of `IndependentMedia`'s
  hashes. Performs NO writes; this is the dialog's confirm surface.
- `apply_media_add(tx, playlist_name, prechecked_new, staged, available,
  guid_seed)` -- resolves-or-creates the playlist `Tag`, then for each
  `New` entry inserts the original `IndependentMedia` row, a SECOND
  `IndependentMedia` row for the thumbnail (a fresh-GUID-named
  byte-for-byte COPY of the source -- PD-1, never a resize, source TODO
  cites the RESEARCH addendum), the `PlaylistItem` (`Accuracy = 1,
  EndAction = 1`), its `PlaylistItemIndependentMediaMap` row
  (`DurationTicks = 40000000` literal), and the playlist Tag's `TagMap`
  row. Two selected files sharing identical content within one batch reuse
  the SAME media/thumbnail pair (`batch_hashes`) rather than double-inserting.
  Two distinct disambiguation helpers -- `disambiguate_filename` (underscore
  scheme, storage `FilePath`) and `disambiguate_label` (parenthetical
  scheme, `PlaylistItem.Label`) -- are kept as two separate functions, never
  unified (D8-06).
- `perform_staged_copies(staged, media_dir)` -- copies every staged file;
  on the FIRST failure, deletes every file already written by THIS call and
  returns `Err`, so the caller (never committing its transaction on that
  `Err`) rolls back the whole batch atomically (PD-3, T-08-31).
- `delete_playlist_items_db(tx, ids) -> PlaylistMediaDeleteOutcome` --
  ports `delete_playlist_items` (`JWLManager.py:3627-3656`) exactly,
  including its table order: computes `used_thumbs`/`used_files` as two
  INDEPENDENT sets from items NOT selected, deletes orphaned
  `IndependentMedia` rows in two independent loops, then the join/map
  tables, marker sub-tables, `PlaylistItemMarker`, `PlaylistItem` last.
  Performs NO filesystem operation -- returns the removed `FilePath` list
  and a `kept_count` for the preview summary.
- `remove_media_files(media_dir, files)` -- the ONLY filesystem-removal
  function in this module; a missing file is silently ignored (Python's
  bare `except: pass`). Called ONLY from the apply command, AFTER commit.
- `dry_run_delete_playlist_items` -- calls ONLY `delete_playlist_items_db`
  inside a never-committed transaction and discards the file list, so it is
  STRUCTURALLY incapable of reaching `remove_media_files` (D8-07).

**Error surface**: `ArchiveError::MediaAddFailed`/`MediaUnsupportedFormat`/
`MediaDeleteFailed` + `to_dto` codes (`media_add_failed`,
`media_unsupported_format`, `media_delete_failed`) + `errors.ts` sentences.

**`db/edit.rs`**: `MEDIA_DELETE_SNAPSHOT_TABLES` (`PlaylistItem`,
`PlaylistItemMarker`, `TagMap`, `IndependentMedia`).

**Tauri commands** (`lib.rs`): `media_add_precheck` (returns
`Vec<MediaPrecheckResult>`), `media_add_apply` (re-runs precheck fresh,
stages+copies+commits atomically, returns `MediaAddApplyReport{added}`),
`playlist_delete_dry_run`/`playlist_delete_apply` (both return
`PlaylistDeleteReport{report, media_removed, media_kept}`; apply commits the
DB delete FIRST, then calls `remove_media_files`, never the reverse).

**Frontend**: `MediaAddDialog.tsx` -- native multi-file picker ->
`media_add_precheck` -> per-file 44px result rows with `✓`/`–`/`✕` glyphs
(`--text-primary`/`--text-muted`/`--destructive`, no new color token) ->
Confirm ("Add Media (N)") -> single atomic `media_add_apply` (busyRef
double-click guard) -> post-completion summary + Done. `CategoryList.tsx`
renames Playlists' `add` to "Add Media..." and wires it; wires Playlists
delete to the ref-counted commands with a custom `EditPreviewDialog`
summary rendering the "{N} media file(s) removed / {K} kept because still
used" clause (kept clause dropped entirely at zero). `operations.ts` flips
`Playlists:add`/`Playlists:delete` LIVE -- the last two deferred capability
slots in the app.

## Deviations from Plan

1. **[Claude's Discretion, documented]** `MediaAddDialog` adds a playlist-name
   text input the UI-SPEC never specifies (see Decisions above) -- required
   for the command to function at all.
2. **[Claude's Discretion, documented]** The "Copying files… {i} of {N}"
   counter's `i` stays fixed at 0 during the single synchronous
   `media_add_apply` call rather than animating (see Decisions above) --
   covered by the UI-SPEC's own backstop framing for this exact loading
   state.
3. **[Process note]** Rust commits are split 2 ways (`feat` then `test`)
   rather than one-commit-per-task, because Tasks 1 and 3 share
   `db/media.rs`/`edit.rs`/`error.rs`/`lib.rs` (same precedent 08-05 set) --
   the first commit carries the whole module (add + delete) plus both
   error variants and all four commands; the second adds both test files.
   Frontend commits are similarly split 2 ways (`feat` then `test`).

## Checkpoint

The blocking `checkpoint:decision` gating irreversible on-disk media
deletion was answered **approve-parity** (the Python-parity design: two
independent used-sets, removal in a function `dry_run_*` cannot reach,
committed DB delete before any file removal) -- implemented exactly as
specified, with no rows-only fallback taken.

## Test output (actual, run at completion)

```
cd app/src-tauri && cargo test --jobs 2 --test media_add_tests
  → 6 passed: sniff+precheck (New/HEIC-Unsupported), duplicate-hash adds
    zero rows/copies, one-new-file -> two IndependentMedia rows +
    byte-identical thumbnail, underscore/parenthetical disambiguation,
    real-copy-failure full rollback + empty media dir after cleanup

cd app/src-tauri && cargo test --jobs 2 --test media_delete_tests
  → 6 passed: shared-thumbnail survival, orphan-only media removed from
    DB+disk, independent used-sets (dual-role file protected once, not
    twice), missing-on-disk-file tolerated, dry-run leaves disk+row-counts
    unchanged, dry-run never touches remove_file

cd app/src-tauri && cargo test --jobs 2
  → ALL binaries: test result: ok (0 failed) across every pre-existing
    suite plus the two new ones above — zero regressions

cd app/src-tauri && cargo clippy --all-targets -- -D warnings
  → clean (only the pre-existing ts-rs try_from attribute-parse warning,
    unrelated to this plan, present since 08-01)

cd app && npx tsc --noEmit
  → clean, zero errors

cd app && npx vitest run
  → Test Files  13 passed (13)
    Tests  133 passed (133)
    (MediaAddDialog.test.tsx new: 8 passed; CategoryList.test.tsx and
     operations.test.ts updated for Playlists' newly-live add/delete)

git diff --stat app/src-tauri/Cargo.toml app/package.json
  → (empty — no dependency additions; the `image` crate was NOT added)

grep -n 'image' app/src-tauri/Cargo.toml
  → no matches

grep -n "generate_handler" -A 100 app/src-tauri/src/lib.rs
  → lists media_add_precheck, media_add_apply, playlist_delete_dry_run,
    playlist_delete_apply

grep -n 'remove_file' app/src-tauri/src/db/media.rs
  → confined to perform_staged_copies' own-batch rollback cleanup (media
    ADD path) and remove_media_files (media DELETE apply path) — never
    inside dry_run_delete_playlist_items or any dry-run function
```

## Known Stubs

None -- playlist media add and ref-counted delete are fully live end to
end (op bar -> native file picker / selection -> command -> disk+DB ->
refresh), no stubbed data paths.

## Threat Flags

None beyond the phase's own pre-registered threat register (T-08-30
through T-08-36, T-08-SC), all of which this plan's design directly
addresses per the `<threat_model>` mitigation plans already on file.

## Self-Check: PASSED

- FOUND: app/src-tauri/src/db/media.rs
- FOUND: app/src-tauri/tests/media_add_tests.rs
- FOUND: app/src-tauri/tests/media_delete_tests.rs
- FOUND: app/src/components/MediaAddDialog.tsx
- FOUND: app/src/components/MediaAddDialog.test.tsx
- FOUND: app/src/bindings/MediaPrecheckResult.ts
- FOUND: app/src/bindings/MediaAddApplyReport.ts
- FOUND: app/src/bindings/PlaylistDeleteReport.ts
- FOUND: `media_add_precheck`/`media_add_apply`/`playlist_delete_dry_run`/
  `playlist_delete_apply` in `app/src-tauri/src/lib.rs` generate_handler![]
- All test suites green (see Test output above) — commits:
  - `9f49fa38` feat(08-06): playlist media add + ref-counted delete backend
  - `650d1f8c` test(08-06): media add + delete backend test coverage
  - `1eb96f7c` feat(08-06): MediaAddDialog + Add Media/Delete wiring for Playlists
  - `23796f2e` test(08-06): MediaAddDialog tests + update Playlists capability assertions
