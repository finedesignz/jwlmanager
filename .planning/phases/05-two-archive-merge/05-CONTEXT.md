# Phase 5: Two-Archive Merge - Context

**Gathered:** 2026-07-22
**Status:** Ready for planning

<domain>
## Phase Boundary

A user with an open archive can merge a SECOND `.jwlibrary` archive INTO the current one, using the vendored native `jwlCore` library (already loaded by Phase 1). Before committing, the user sees a dry-run preview (rows added / overwritten / deleted) and can cancel. The merge result is semantically equivalent to what the Python app produces on the same inputs. A missing / wrong-arch / failed jwlCore binary yields a clear, actionable typed error — never a crash.

**In scope:**
- INVOKE `mergeDatabase` via FFI (Phase 1 already LOADS + resolves the symbol; Phase 5 extends the binding to actually call it — never re-loads).
- Materialize the two-directory on-disk layout jwlCore expects: destination root (contains `userData.db`) + `<root>/merge` subdir (contains the source archive's extracted `userData.db`).
- Dry-run preview: run the REAL merge on a THROWAWAY COPY of the destination DB, diff before/after with Phase 2's `DryRunReport` mechanism, discard the copy. jwlCore has NO native preview mode.
- Cancel path: dry-run report presented; user confirms or cancels; on cancel nothing is committed.
- Commit path: on confirm, run the merge against the live session's working-copy DB, mark dirty, refresh.
- Typed error surface: new `ArchiveError::MergeUnavailable` (lib absent / wrong-arch) and `MergeFailed { reason }` (non-zero return code + `getLastResult()` detail).
- Parity test: differential oracle extended — merge two synthetic fixtures via Rust FFI AND via the Python app's `merge_databases`, compare NORMALIZED table state.
- arm64: ships, but merge is UNAVAILABLE there (no aarch64-windows binary) with a clear message; reuse Phase 1 `JwlCoreStatus`.

**Out of scope (own phases / deferred):**
- Downgrade-on-merge (`mergeDatabase`'s 3rd `downgrade` arg) — MVP passes `false`; Phase 4 downgrade interplay deferred (D5-08).
- N-way / batch merge — one source at a time.
- Progress bar with live percentage — MVP uses a blocking call + busy/spinner state (D5-05).
- Media-file conflict UX beyond what jwlCore does internally.
- Browse/edit of merged categories (Phases 6/7), import/export (8), signing/localization (11).

**Requirements:** MERGE-01, MERGE-02, MERGE-04 (per ROADMAP Phase 5).

**Depends on:** Phase 1 (jwlCore loader `check_jwlcore`/`JwlCoreStatus`, `mergeDatabase` symbol resolved, arch-aware selection, PATH-prepend DLL load; `ArchiveSession`; `ErrorDto`), Phase 2 (`DryRunReport`, `diff_snapshots`, `snapshot_tables`, `PragmaGuard`), Phase 4 (`save_v14_copy` throwaway-copy pattern, `write_archive_from_db_source`). All complete.

</domain>

<decisions>
## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit.

### The FFI call — extend Phase 1, never re-load (MERGE-01)

- **D5-01 (extend the loader, don't duplicate):** Phase 1's `jwlcore/loader.rs` already loads the correct `(OS,ARCH)` binary, prepends the binary's dir to `PATH` on Windows so `sqlite3_64.dll` resolves, resolves `mergeDatabase`/`getLastResult`/`getCoreVersion`/`setProgressCallback`, and models arm64-windows as a non-loaded `JwlCoreStatus`. Phase 5 adds a `jwlcore/merge.rs` (sibling of `loader.rs`) that reuses the SAME load path (`resolve_lib_name` → `resolve_lib_path` → `load_library`) and then INVOKES `mergeDatabase`. Do NOT re-implement arch selection or the PATH-prepend — call into loader helpers (promote them to `pub(crate)` if needed).
  `[auto] binding placement — Q: "New standalone loader or extend Phase 1's?" → Selected: "Extend Phase 1 loader helpers; add merge.rs" (recommended default)`
  **Rationale:** The Windows `sqlite3_64.dll` PATH-prepend is load-bearing and hard-won (loader.rs:84-123). Any second load path that omits it will hard-terminate the process. One load path, reused.

- **D5-02 (exact ABI — verified):** `mergeDatabase` C signature, confirmed from `jwlcore.py:64-65` (ctypes decls) AND resolved-as-symbol in `loader.rs`:
  `extern "C" int mergeDatabase(const char* path1, const char* path2, bool downgrade)` — return `0` = success, non-zero = failure. Rust FFI type: `unsafe extern "C" fn(*const c_char, *const c_char, c_bool) -> c_int`. Paths are UTF-8, NUL-terminated (`CString`). `path1` = DESTINATION directory (mutated IN PLACE), `path2` = SOURCE directory. Supporting symbols: `getLastResult() -> *const c_char` (nullable UTF-8 detail string, read-only, never freed by us), `getCoreVersion() -> *const c_char`, `setProgressCallback(extern "C" fn(c_int))`.

- **D5-03 (path semantics — the two-directory layout):** Confirmed from `JWLManager.py:2670-2672`: `merge_databases(f'{TMP_PATH}', f'{TMP_PATH}/merge', False)`. jwlCore opens `<path1>/userData.db` and `<path2>/userData.db` and merges path2's records INTO path1's `userData.db`, **in place on disk**. Therefore:
  - `path1` (destination) = a directory containing `userData.db` — for the REAL commit this is a staging dir seeded with a copy of `session.db_path`; for the dry-run it is a throwaway copy dir.
  - `path2` (source) = a directory into which the incoming archive is extracted (its `userData.db` + media). Reuse the Phase 1 zip-slip-safe `extract` path (`archive/extract.rs`) — NEVER `ZipFile.extractall` semantics without path validation.
  - After the call, the merged result lives in `<path1>/userData.db`. Copy it back over `session.db_path` on commit; discard on dry-run/cancel.

### Dry-run on a throwaway copy — the parity criterion (MERGE-02)

- **D5-04 (dry-run = real merge on a throwaway copy, CONFIRMED VIABLE):** jwlCore has no preview mode; it mutates path1 in place. So the preview runs the ACTUAL merge against a disposable copy:
  1. `std::fs::copy(session.db_path, <throwaway_root>/userData.db)` (mirror `save_v14_copy` step 1, `downgrade.rs:610`).
  2. Extract the source archive into `<throwaway_root>/merge/` (zip-slip-safe).
  3. Snapshot BEFORE PK-sets on the throwaway copy (Phase 2 `snapshot_tables` over a merge-relevant table set).
  4. Call `mergeDatabase(<throwaway_root>, <throwaway_root>/merge, false)`.
  5. Snapshot AFTER; `diff_snapshots(before, after)` → `DryRunReport` (added / overwritten / deleted).
  6. Delete the throwaway root (best-effort, every path).
  The report drives the confirm/cancel UI. Because the SAME `mergeDatabase` runs in both preview and commit against a bit-identical starting DB, the preview is exact, not an estimate.
  `[auto] preview mechanism — Q: "Static estimate, or run the real merge on a copy?" → Selected: "Real merge on a throwaway copy, snapshot-diff" (recommended default)`
  **Rationale:** The merge algorithm exists ONLY as a compiled binary — there is no Rust/Python reference to re-derive a preview from. Running the real thing on a copy is the only way to get an accurate add/overwrite/delete count. This is the direct analogue of Phase 4's `dry_run_downgrade`, and the throwaway-copy machinery already exists.
  **Diff granularity:** per-table PK-set counts via `snapshot_tables` over the merge-affected single-PK tables (`Location`, `Note`, `UserMark`, `BlockRange`, `Bookmark`, `Tag`, `TagMap`, `InputField`, `PlaylistItem`, `PlaylistItemMarker`). Present as "N added / N overwritten" — merges rarely DELETE from the destination, but the `deleted` bucket is kept for completeness. This granularity satisfies the criterion "dry-run preview add/overwrite/delete + cancel."

### Progress + threading (MERGE-01)

- **D5-05 (blocking call off the UI thread; busy state, no live %):** For MVP, do NOT wire `setProgressCallback`. The Rust command runs `mergeDatabase` synchronously; Tauri invokes commands on a worker thread pool, so the WebView UI stays responsive — surface a simple busy/spinner state in the frontend while the command is in flight. The Python 0..15-step progress dialog is a nicety, not required for correctness.
  `[auto] progress — Q: "Wire the C progress callback, or blocking call + spinner?" → Selected: "Blocking call on Tauri worker thread + frontend busy state" (recommended default)`
  **Rationale:** `setProgressCallback` takes a C function pointer that would fire from jwlCore's thread into Rust — extra `unsafe` surface, callback-lifetime hazards, and cross-thread Tauri event emission, for a cosmetic gain. Merges of hobby-scale archives complete quickly. Callback-driven progress is a clean post-MVP add. Note: jwlCore's callback state is process-global (`setProgressCallback` sets a single fn ptr) — a reason to serialize merges anyway.

- **D5-06 (serialize merges; single in-flight):** jwlCore uses process-global state (`getLastResult`, the progress callback slot). Guard the merge command so only one merge runs at a time (the `SessionState` mutex already serializes session access; ensure the FFI call and its `getLastResult()` read happen under one critical section so a second concurrent merge can't clobber the result string).

### downgrade flag (MERGE-01)

- **D5-07 (pass `false`):** The 3rd `mergeDatabase` arg is hardcoded `false` for Phase 5 — no downgrade-on-merge. Matches `JWLManager.py:2672`.
- **D5-08 (Phase 4 interplay deferred):** Merging a v14 source into a v16 destination is handled by jwlCore internally at the schema it targets; explicit downgrade-during-merge and v14↔v16 merge-schema reconciliation are DEFERRED. If the source archive is a different schema version, rely on Phase 1's open-time upgrade normalization of the destination and jwlCore's own handling; document any observed mismatch as an open question, do not add downgrade logic here.

### arm64 / missing lib — never crash (MERGE-01, criterion 1 & 4)

- **D5-09 (reuse `JwlCoreStatus`; new typed errors):** Before attempting a merge, the command checks the same load path Phase 1 uses. If `resolve_lib_name` returns the arm64-windows / unsupported no-binary case, OR the library fails to load/resolve `mergeDatabase`, the command returns `ArchiveError::MergeUnavailable` (maps to a DTO code `merge_unavailable` / `message_key error.merge.unavailable` — "merge is not available on this platform / the merge engine could not be loaded"). A non-zero `mergeDatabase` return → `ArchiveError::MergeFailed { reason }` where `reason` = `getLastResult()` (INTERNAL only; the DTO exposes a generic `error.merge.failed` message per the D-14 no-leak rule). Never `unwrap`/`panic`/`sys.exit` — the Python `crash_box + sys.exit()` (`JWLManager.py:2682-2685`) is the defect NOT ported.
  **Rationale:** arm64-windows ships the app but has no jwlCore binary (Phase 1 D-13a). Merge must degrade to a clear "unavailable here" message, consistent with the rest of the app's typed-error posture.

### Verification — parity, never byte-diff (MERGE-03, criterion 3)

- **D5-10 (differential oracle extended):** Two synthetic v16 fixtures (dest + source) with overlapping and disjoint records across the merge-affected tables. Merge them (a) via the Rust FFI path and (b) via the Python app's `merge_databases(dest_dir, dest_dir/merge, False)` on the SAME inputs, then compare NORMALIZED table state (sorted rows per table, semantic key comparison) — NEVER byte-diff the resulting DBs (VACUUM + jwlCore's own ID-densifying makes bytes diverge legitimately). Follow the existing `tests/differential.rs` harness conventions: shell to `python3` from repo root, PATH-prepend for `sqlite3_64.dll`, root-staged gitignored DLLs, `#[ignore]` the real-lib/Python leg (CI is Rust-only, no PySide6). The Rust-only leg (FFI merge of two fixtures, assert internal invariants: no dup records, all source records present, referential integrity) runs by default and is env-gated only where the real DLL is required.
  **DLL availability for the Rust test:** the FFI test loads the SAME vendored `libs/jwlCore-amd64.dll` + co-located `libs/sqlite3_64.dll` that `loader.rs` resolves via `dev_libs_dir()` (repo `libs/`). No root-staging needed for the Rust FFI leg (loader's PATH-prepend covers `sqlite3_64.dll`); root-staging is only for the PYTHON leg (`jwlcore.py` resolves the win32 DLL next to itself = repo root). Gate the FFI test to skip-as-pass when `resolve_lib_name` has no binary for the host (mirror `jwlcore_status_real_load_current_host`).

- **D5-11 (source never mutated):** The incoming source archive FILE is only ever READ and extracted into a temp `merge/` subdir (throwaway or staging root under `session.temp_dir`); it is never written back. The DESTINATION mutated by jwlCore is always a copy (dry-run) or a staging copy promoted to `session.db_path` only on successful commit. Assert in tests: source file bytes unchanged after merge.

### Claude's Discretion
Module name (`jwlcore/merge.rs` recommended), exact staging-dir layout under `session.temp_dir`, whether commit merges into a staging copy then swaps vs. copies result over `session.db_path`, the precise merge-affected table list for `snapshot_tables`, whether to run `trim_db` after a committed merge (recommend: mark dirty, trim on next save via the existing save path — do NOT double-trim), test fixture construction, and the frontend confirm/cancel dialog shape (reuse Phase 2/4 dry-run preview component).

</decisions>

<canonical_refs>
## Canonical References — downstream agents MUST read

### The merge source of truth
- `jwlcore.py:59-83` — the FULL FFI contract: `CALLBACKTYPE`, `mergeDatabase` argtypes/restype (`:64-65`), `getLastResult` (`:67-68`), `getCoreVersion` (`:70-71`), wrappers (`:74-83`). `path1`=dest, `path2`=source (`:74-75` + `JWLManager.py:2672`).
- `JWLManager.py:2645-2694` — `merge_items`: validity gate (`:2661`), progress callback install (`:2668-2669`), extract source to `{TMP_PATH}/merge` (`:2670-2671`), `merge_databases(TMP_PATH, TMP_PATH/merge, False)` (`:2672`), non-zero → NOT merged (`:2673-2674, 2686-2688`), rmtree merge dir (`:2675`). The `crash_box + sys.exit()` (`:2682-2685`) is the defect NOT ported (D5-09).
- `JWLManager.py:1010-1014` — `merge_file`: file-dialog entry point.
- `.planning/research/FUNCTIONALITY-SPEC.md` §7 (Native jwlCore bridge / merge) + §1.6 — "the entire merge algorithm lives in the compiled jwlCore lib; there is no Python implementation to port."

### Foundations this builds on
- `app/src-tauri/src/jwlcore/loader.rs` — arch-aware load, PATH-prepend Windows DLL fix (`:84-123`), `mergeDatabase` already in `EXPECTED_SYMBOLS`, `JwlCoreStatus` non-loaded model, `resolve_lib_name`/`resolve_lib_path`/`load_library`/`dev_libs_dir` helpers to reuse (D5-01). `jwlcore_status_real_load_current_host` test = the skip-as-pass pattern for D5-10.
- `app/src-tauri/src/db/delete.rs:112-194` — `DryRunReport`, `snapshot_tables`/`snapshot_pks`, `diff_snapshots` (reuse verbatim for D5-04; never copy-paste the diff logic).
- `app/src-tauri/src/archive/downgrade.rs:495-581` (`dry_run_downgrade`) + `:583-627` (`save_v14_copy`) — the throwaway-`fs::copy` pattern (`:610`), best-effort cleanup, session-untouching output. The structural template for the merge dry-run + commit.
- `app/src-tauri/src/archive/save.rs:90-133` (`rebuild_zip`, `db_source` indirection), `:248` (`write_archive_from_db_source`) — how to promote a merged DB into a saved archive without touching the session (the merge marks dirty; save happens via the normal path).
- `app/src-tauri/src/archive/extract.rs` — zip-slip-safe extraction for the source `merge/` subdir (D5-03; never raw extractall).
- `app/src-tauri/src/session.rs:33-51` — `ArchiveSession` (`temp_dir`, `db_path`, `source_path`, `dirty`), `SessionState = Mutex<Option<ArchiveSession>>`.
- `app/src-tauri/src/error.rs` — add `MergeUnavailable` + `MergeFailed { reason }` to `ArchiveError`, wire `to_dto` (reason never leaks; codes `merge_unavailable` / `merge_failed`).
- `app/src-tauri/tests/differential.rs` — the Python-oracle harness to extend (D5-10); `#[ignore]` reason, root-staged DLL prerequisites, repo-root shell-out.
- `app/src-tauri/src/lib.rs:273` — `generate_handler![]` where the new `merge_*` commands register.

</canonical_refs>

<code_context>
## Existing Code Insights
- Phase 1 ALREADY resolves `mergeDatabase` (ABI proof) and handles the Windows `sqlite3_64.dll` load quirk — Phase 5 is "call the function you already proved loads," not "figure out how to load."
- The throwaway-copy + snapshot-diff dry-run is a solved pattern (Phase 2 delete, Phase 4 downgrade). Merge reuses it wholesale — the only new twist is the `<root>` + `<root>/merge` two-directory layout jwlCore wants and the FFI call itself.
- jwlCore mutates path1 IN PLACE — so the destination must ALWAYS be a copy (dry-run) or a staging copy (commit); the live `session.db_path` is only overwritten on a successful commit.

## Established Patterns
- Typed errors, no `unwrap`/`panic` on the archive-data path; the Python `sys.exit()`-on-error is a defect, not a spec.
- Source archives never mutated; all work on copies in `session.temp_dir`.
- Semantic (normalized-table) parity, NEVER byte-diff (VACUUM + jwlCore ID-densify legitimately diverge bytes).
- All SQL parameterized (snapshot queries use fixed table/col identifiers from a const list, never user input).
- Zip-slip-safe extraction (Phase 1 `extract.rs`) for any incoming archive.

## Integration Point / risk
- **The FFI boundary is the risk.** `mergeDatabase` takes raw `*const c_char` and mutates a SQLite DB on disk via a statically-linked `sqlite3_64.dll`. Getting the two-directory layout wrong (e.g. passing a FILE path instead of a DIR, or a dir without `userData.db`) yields undefined native behavior or a non-zero return. Verify the layout against `JWLManager.py:2670-2672` exactly: both args are DIRECTORIES, dest dir contains `userData.db`, source dir is the extract root.
- **Process-global jwlCore state** (`getLastResult`, progress-callback slot) → serialize merges (D5-06); read `getLastResult()` immediately after a non-zero return, under the same lock.
- **Media files**: jwlCore may copy playlist/media blobs from path2 into path1 (the dirs, not just the DBs). OPEN QUESTION — confirm during implementation whether merged media lands in `<path1>` and needs folding back into the session archive. For MVP, if merge only touches `userData.db`, copying the merged DB back suffices; if media appears, the commit must also fold new media entries into `session.entries`. Flag and verify empirically with a media-bearing fixture.
</code_context>

<specifics>
## Specific Ideas
- This is the phase that finally CALLS the native lib Phase 1 only loaded. Keep the FFI surface tiny: one `merge.rs`, one `unsafe extern "C"` type alias, one function that takes two dir paths + the downgrade bool and returns `Result<(), ArchiveError>` (mapping non-zero → `MergeFailed{getLastResult()}`).
- The dry-run and the commit share ONE internal merge routine (run against a given root dir); the difference is only whether the result is discarded or promoted. DRY the FFI call.
- jwlCore v0.32.1 is the version verified loading in Phase 1 (`differential.rs` doc comment); `getCoreVersion` already surfaces it.

## Constraints in force (project)
- Synthetic fixtures ONLY — NEVER a real `.jwlibrary` in tests.
- Parameterize all SQL; typed errors, never silent-swallow or crash.
- Semantic parity, NEVER byte-diff.
- Source archives never mutated (merge runs on copies).
- MIT — do NOT ingest jwlFusion (Infiniti Noncommercial) source; only the already-vendored jwlCore BINARY is used.

</specifics>

<deferred>
## Deferred Ideas
- Downgrade-on-merge (`mergeDatabase(..., true)`) + v14↔v16 merge-schema reconciliation → later phase (D5-08).
- Live progress bar via `setProgressCallback` → post-MVP (D5-05).
- N-way / batch merge of multiple sources → later.
- Media-conflict resolution UX → later (confirm media handling empirically first).
</deferred>

---

*Phase: 5-Two-Archive Merge*
*Context gathered: 2026-07-22*
</content>
</invoke>
