# Phase 10: N-Way Merge Fold - Research

**Researched:** 2026-07-26
**Domain:** Orchestration layer over an existing FFI merge primitive (Rust/Tauri); no new external library, no new native call shape.
**Confidence:** HIGH

## Summary

Phase 10 adds no new merge semantics and no new FFI surface. Every primitive it needs
(`jwlcore::merge::run_merge_with_lib_path`, `archive::merge::stage_and_merge`,
`content_diff`, `dry_run_merge_with_lib_path`/`merge_commit_with_lib_path`,
`fold_back_media`, `archive::save::atomic_replace`) already exists in Phase 5's shipped
code, verified by reading `app/src-tauri/src/archive/merge.rs` and
`app/src-tauri/src/jwlcore/merge.rs` directly. The work is: generalize one call site
(`stage_and_merge`'s DB-copy source) so a chain of N-1 steps can run instead of 1, add a
single final atomic promote, and reuse `content_diff` unmodified for the aggregate
dry-run. The playlist-coverage gap Phase 5 left (D10-06) has a concrete, already-proven
closure path: Phase 8's `build_container` fixture helper in
`app/src-tauri/tests/playlist_import_tests.rs` constructs the exact row set
(`PlaylistItemAccuracy`, `PlaylistItem`, `IndependentMedia`, `Location`,
`PlaylistItemLocationMap`) that a minimal synthetic `PlaylistItem` was missing.

**Primary recommendation:** Add one new function `fold_stage_and_merge` (or generalize
`stage_and_merge`'s destination-copy source parameter) that loops
`run_merge_with_lib_path` N-1 times over a chain of per-step staging directories, each
seeded by copying the PREVIOUS step's `userData.db` (not always `session.db_path`), call
`fold_back_media` once per completed step, and perform exactly one
`archive::save::atomic_replace` at the very end. Build the round-trip test as two bodies
sharing one 3-fixture setup: (a) the new fold commands, (b) two sequential
`merge_commit_with_lib_path` calls, compared via `content_diff`'s underlying
`snapshot_signatures`/normalized-table comparison — never byte-diff.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| N-way fold orchestration (loop, staging chain, single promote) | API / Backend (Rust `archive::merge`) | — | Pure sequencing logic over Phase 5's existing Tauri commands layer; no UI state needed to decide fold order beyond what the user already picked. |
| FFI merge invocation (`mergeDatabase`) | API / Backend (`jwlcore::merge`) | — | Unchanged from Phase 5 — reused verbatim, called N-1 times instead of once. |
| Aggregate dry-run diff | API / Backend (`archive::merge::content_diff`) | — | Same content-signature diff Phase 5 built; operates on two DB files (original session vs. final fold state), agnostic to how many steps produced the "after" state. |
| Atomic promote | API / Backend (`archive::save::atomic_replace`) | — | Single `fs::rename`, same-filesystem, same primitive Phase 5 already proved atomic — called exactly once at the end of the fold, never per-step. |
| Multi-select + reorder UI | Browser / Client (WebView frontend) | — | Extends Phase 5's single-archive picker to a list with a drag-reorder affordance; no new backend contract beyond `Vec<PathBuf>` in fold order. |
| Busy/spinner state during fold | Browser / Client | — | Tauri commands run off the WebView UI thread already; frontend only needs a single busy indicator spanning the whole N-1-step operation (D10-07), not per-step progress. |

## Standard Stack

### Core

No new library is needed. Phase 10 is pure Rust orchestration over Phase 5's existing
modules.

| Component | Version | Purpose | Why Standard |
|-----------|---------|---------|---------------|
| `rusqlite` | (already vendored, unchanged) | Reading `userData.db` for signature snapshots in `content_diff` | Already the project's sole SQLite binding; `content_diff` calls it unmodified. |
| `libloading` | (already vendored, unchanged) | Loading `jwlCore-*` shared lib per fold step | Already the project's sole FFI loader; `run_merge_with_lib_path` is called N-1 times, loading the lib fresh each call exactly as Phase 5 does per merge. |

### Supporting

| Component | Purpose | When to Use |
|-----------|---------|-------------|
| `tempfile` (already vendored, dev-dependency) | Test fixture DBs (`common::fresh_v16_db()`) for the 3+-archive round-trip test | Already used by every Phase 5/8/9 integration test — reuse the same `common` module. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Sequential fold (D10-01, locked) | A batch/associative merge invented in Rust | Explicitly rejected by CONTEXT.md D10-01 — jwlCore's binary has no documented associativity guarantee and is closed-source; inventing new merge semantics would be unverifiable and out of scope. Not researched further; this is a locked decision, not a discretion area. |

**Installation:** None. No `Cargo.toml` change — zero new dependencies, matching CONTEXT.md's constraint ("No new Cargo dependency without an explicit legitimacy checkpoint").

**Version verification:** N/A — no package versions to verify; every crate Phase 10 touches (`rusqlite`, `libloading`, `tempfile`) is already pinned and vendored from Phase 1/5/8. `[VERIFIED: Cargo.toml / shipped code]` — confirmed by reading `app/src-tauri/src/archive/merge.rs` and `app/src-tauri/src/jwlcore/merge.rs` directly; no registry lookup needed since nothing new is added.

## Package Legitimacy Audit

Not applicable — this phase adds zero new packages. Per CONTEXT.md: "No new Cargo
dependency without an explicit legitimacy checkpoint — none anticipated; every primitive
Phase 10 needs already exists in Phase 5's shipped code." Verified true by direct source
inspection of `archive/merge.rs`, `jwlcore/merge.rs`, and `archive/save.rs` above — no
`Cargo.toml` diff is required for this phase's scope.

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.

## Architecture Patterns

### System Architecture Diagram

```
User picks N archive files (list, reorderable)  [Browser/Client]
        |
        v
Frontend invokes fold_dry_run_merge(paths: Vec<PathBuf>)   [Tauri command boundary]
        |
        v
+-------------------------------------------------------------+
| archive::merge::fold_dry_run_merge (NEW, throwaway root)    |
|                                                               |
|  step_0 <- fs::copy(session.db_path)         [seed = live DB]|
|  for i in 1..=N:                                             |
|     step_i_dir <- copy step_(i-1)'s userData.db              |
|     extract source[i] into step_i_dir/merge/  (zip-slip-safe)|
|     run_merge_with_lib_path(step_i_dir, step_i_dir/merge)    |
|       |-- on failure: abort, remove_dir_all(throwaway root), |
|       |               return MergeFailed{step: i, reason}    |
|  content_diff(session.db_path, step_N/userData.db)           |
|  remove_dir_all(throwaway root)  [always, every path]        |
+-------------------------------------------------------------+
        |
        v
DryRunReport (aggregate added/overwritten/deleted) -> confirm/cancel UI
        |  (user confirms)
        v
Frontend invokes fold_merge_commit(paths: Vec<PathBuf>)
        |
        v
+-------------------------------------------------------------+
| archive::merge::fold_merge_commit (NEW, staging root)        |
|                                                               |
|  SAME step_0..step_N chain as dry-run, but under              |
|  session.temp_dir/fold_staging/ (not throwaway)               |
|  fold_back_media(session, step_i_dir) after EVERY step (D10-04)|
|    |-- on ANY step failure: abort, remove_dir_all(staging),   |
|    |                        session UNCHANGED, typed error    |
|  ONLY after step_N succeeds:                                  |
|     atomic_replace(step_N/userData.db, session.db_path)       |
|       [single fs::rename, same filesystem, all-or-nothing]    |
|  session.dirty = true                                         |
+-------------------------------------------------------------+
        |
        v
Session now reflects the folded result; N source archives
untouched throughout (read-only extraction only).
```

A reader can trace: pick archives -> aggregate preview -> confirm -> chained staging
merges -> single atomic promote -> dirty session, matching Phase 5's own dry-run/commit
split generalized to N-1 steps.

### Recommended Project Structure

No new files needed beyond additive functions in the existing module — matches the
project's established one-module-per-domain layout:

```
app/src-tauri/src/
├── archive/
│   └── merge.rs         # ADD: fold_dry_run_merge, fold_merge_commit,
│                         #      fold_stage_and_merge (generalizes stage_and_merge's
│                         #      copy-source), no changes to existing Phase 5 fns
├── jwlcore/
│   └── merge.rs          # UNCHANGED — run_merge_with_lib_path called in a loop
├── error.rs               # ADD: step index field on MergeFailed, or fold into reason
│                          #      string (Claude's Discretion per CONTEXT.md)
└── lib.rs                 # ADD: two new command registrations in generate_handler![]
app/src-tauri/tests/
├── merge_orchestration.rs # ADD: fold round-trip test (fold vs. chained pairwise)
└── merge_fold_failure.rs  # ADD (or same file): step-k failure leaves session pristine
```

### Pattern 1: Chained staging directories, single final promote

**What:** Generalize Phase 5's `stage_and_merge(lib_path, session, source_archive, root)`
— which always does `fs::copy(&session.db_path, root.join("userData.db"))` as its FIRST
line — into a variant whose copy source is parameterized: step 1 copies from
`session.db_path` (unchanged), steps 2..N copy from the PREVIOUS step's
`root.join("userData.db")`.

**When to use:** Every fold step after the first. Getting this wrong (always copying
from `session.db_path`) silently degrades the fold to "only the last source's merge
survives" — CONTEXT.md flags this as the phase's highest-consequence subtle bug.

**Example (generalizing the existing function, verified against shipped code):**
```rust
// Source: app/src-tauri/src/archive/merge.rs:151-166 (existing stage_and_merge)
fn stage_and_merge(
    lib_path: &Path,
    session: &ArchiveSession,
    source_archive: &Path,
    root: &Path,
) -> Result<(), ArchiveError> {
    fs::copy(&session.db_path, root.join("userData.db"))?;
    let merge_dir = root.join("merge");
    extract_zip_slip_safe(source_archive, &merge_dir)?;
    crate::jwlcore::merge::run_merge_with_lib_path(lib_path, root, &merge_dir, false)
}

// NEW: parameterize the copy source instead of hardcoding session.db_path,
// so Phase 10 can seed step i from step (i-1)'s result. `stage_and_merge`
// itself can become a thin wrapper: stage_and_merge(..) calls this with
// copy_from = &session.db_path (unchanged call sites, unchanged behavior).
fn stage_and_merge_from(
    lib_path: &Path,
    copy_from: &Path,          // session.db_path (step 1) or prev step's userData.db (step i>1)
    source_archive: &Path,
    root: &Path,
) -> Result<(), ArchiveError> {
    fs::copy(copy_from, root.join("userData.db"))?;
    let merge_dir = root.join("merge");
    extract_zip_slip_safe(source_archive, &merge_dir)?;
    crate::jwlcore::merge::run_merge_with_lib_path(lib_path, root, &merge_dir, false)
}
```

### Pattern 2: Aggregate dry-run reusing `content_diff` unmodified

**What:** `content_diff(before_db, after_db)` already snapshots content signatures over
`MERGE_SNAPSHOT_TABLES` and diffs them — it takes two DB FILE PATHS and is completely
agnostic to how many operations produced `after_db`. For the fold, `before_db =
session.db_path` (untouched, read-only) and `after_db` = the LAST step's throwaway
`userData.db`.

**When to use:** For D10-05's single aggregate preview. No new diff code, no per-step
report — confirmed sound: a row overwritten at step 1 and again at step 3 has ONE final
signature in `after_db`, so `diff_signatures` reports it once, against the true original
baseline (`session.db_path`), exactly matching the phase brief's "cumulative effect"
framing.

**Example:**
```rust
// Source: app/src-tauri/src/archive/merge.rs:224-234 (existing content_diff, called unmodified)
pub fn content_diff(before_db: &Path, after_db: &Path) -> Result<DryRunReport, ArchiveError> {
    let before = { let conn = Connection::open(before_db)?; snapshot_signatures(&conn, MERGE_SNAPSHOT_TABLES)? };
    let after  = { let conn = Connection::open(after_db)?;  snapshot_signatures(&conn, MERGE_SNAPSHOT_TABLES)? };
    Ok(diff_signatures(&before, &after))
}
// Fold dry-run: content_diff(&session.db_path, &fold_chain_last_step_db)
```

### Anti-Patterns to Avoid

- **Re-copying from `session.db_path` at every fold step:** silently drops every
  earlier step's effect — the fold degrades to "merge only the last source." Cover with
  an explicit 3-archive test asserting ALL THREE sources' unique rows survive in the
  final result (CONTEXT.md's own stated risk).
- **Per-step promote:** never call `atomic_replace` inside the loop. Only ONE promote,
  after the LAST step succeeds — an intermediate promote would expose a partially-folded
  archive as the live session mid-fold, violating the Core Value.
- **Skipping `fold_back_media` on intermediate steps:** if a step's staging DB is seeded
  fresh from the previous step's `userData.db` only (not its sibling media files), any
  media jwlCore wrote at an earlier step is silently lost by the next step's re-copy.
  D10-04 requires running `fold_back_media` after EVERY completed step, not just the
  last.
- **Inventing a new merge algorithm to "fix" order-sensitivity:** D10-01 is a locked
  decision — order-sensitivity is correct, expected behavior mirroring hand-chained
  Phase 5 merges, not a defect.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Merge algorithm itself | Any new record-matching/conflict logic | `jwlcore::merge::run_merge_with_lib_path` (unmodified), called N-1 times | The algorithm is closed-source, compiled, and already proven correct for the pairwise case by Phase 5's differential oracle against the Python app. |
| Content-diff / dry-run | A per-step or novel diff format | `archive::merge::content_diff` (unmodified) | Already handles the "overwritten-then-overwritten-again counts once" case correctly by construction (final-state signature comparison), no new code needed. |
| Atomic promote | A custom multi-file transaction or `fs::copy`-then-verify | `archive::save::atomic_replace` (single `fs::rename`) | Already proven same-filesystem-atomic under test (`merge_commit_promote_atomic`); a second implementation would duplicate an already-hardened primitive and risk a new correctness gap. |
| Zip-slip-safe extraction of each of the N sources | Any new extraction routine | `archive::extract::extract_zip_slip_safe` (unmodified) | Already the project's sole path-validated extraction primitive, used identically N times (once per source), no per-call variation needed. |

**Key insight:** Phase 10's entire job is to LOOP existing, already-hardened primitives
correctly — the risk is in orchestration and cleanup-on-failure discipline, not in any new
algorithm. Every "Don't Hand-Roll" item above is proven, tested code from Phase 5.

## Runtime State Inventory

Not applicable — Phase 10 is a greenfield capability (new orchestration functions and
commands), not a rename/refactor/migration. No existing stored data, service config, OS
registrations, secrets, or build artifacts reference anything Phase 10 renames or moves.

## Common Pitfalls

### Pitfall 1: Fold-order divergence mistaken for a bug

**What goes wrong:** A developer sees `fold(A,B,C) != fold(A,C,B)` when B and C both edit
the same record (e.g. same `Note.Guid`) and "fixes" it by trying to make the fold order-
independent.

**Why it happens:** jwlCore does in-place content UPDATEs at matched PKs (verified,
`archive/merge.rs:17-24` module docs), so the later step in any fold order wins that
row's content — exactly like a user chaining Phase 5 merges by hand would experience.

**How to avoid:** This is D10-01, a LOCKED decision. The round-trip test (D10-02) must
compare fold-vs-chained-pairwise IN THE SAME ORDER only, never assert order-independence.
The UI's job is to make fold order visible and reorderable, not to hide or normalize it.

**Warning signs:** A test asserting `fold(A,B,C) == fold(B,A,C)` — this is testing the
WRONG thing per CONTEXT.md and will either fail correctly (proving the point) or pass by
fixture-coincidence (masking a divergent-order bug elsewhere).

### Pitfall 2: Copy-source regression collapses the fold to "last source wins"

**What goes wrong:** `stage_and_merge_from` (or equivalent) is called with `copy_from =
session.db_path` at every step instead of the previous step's result. Every merge
overwrites the SAME copy of the original session DB; sources 1..N-1's effects are
silently discarded, and only source N's merge survives.

**Why it happens:** Phase 5's `stage_and_merge` hardcodes `session.db_path` as its copy
source because Phase 5 only ever has one step — that assumption is exactly what Phase 10
must break, and it is easy to forget when generalizing the loop body.

**How to avoid:** Track `prev_step_db: &Path` explicitly through the loop, starting at
`session.db_path` for step 1 and updated to `step_i.join("userData.db")` after each
successful step. Write the 3-archive assertion test FIRST (each source contributes a
uniquely-identifiable row; assert all three appear in the final result), before any other
fold test — this is the single highest-value regression guard for this phase.

**Warning signs:** A round-trip test that only checks TOTAL row counts rather than
per-source content — a "collapsed to last source" bug can still produce a plausible total
row count if sources overlap partially.

### Pitfall 3: Partial cleanup on mid-fold failure leaves stray staging directories

**What goes wrong:** A step-k failure removes only that step's directory, leaving
`step_1`..`step_(k-1)` directories orphaned under `session.temp_dir`, or an earlier
`remove_dir_all` call races with an in-progress extraction.

**Why it happens:** Phase 5's pattern is "best-effort cleanup on EVERY path" for a SINGLE
throwaway/staging root; Phase 10 needs the SAME discipline but for a directory that
contains N-1 sub-steps, not one.

**How to avoid:** Use ONE parent directory (e.g. `session.temp_dir/fold_staging/` or
`fold_dryrun/`) containing `step_0/`, `step_1/`, ... `step_(N-1)/` as children, and do a
SINGLE `remove_dir_all` on the parent in the outer closure's cleanup — mirroring Phase
5's `let _ = fs::remove_dir_all(&root` `/staging)` pattern exactly, just at one directory
level higher. Never clean up per-sub-step; clean up the whole fold root once, on every
exit path (success or failure), matching the existing `(|| { ... })()` + trailing
best-effort-cleanup idiom already used in `dry_run_merge_with_lib_path` and
`merge_commit_with_lib_path`.

**Warning signs:** Disk usage growing across repeated failed-fold test runs in CI; a test
that doesn't assert the fold root is gone after a forced step-k failure.

### Pitfall 4: Assuming `fold_back_media`'s Phase 5 no-op generalizes without testing

**What goes wrong:** Skipping the per-step `fold_back_media` call (only running it on the
final step) on the assumption that Phase 5's "jwlCore wrote only `userData.db`, no loose
media" observation holds for every step of an N-way fold too.

**Why it happens:** Phase 5 only ever observed ONE merge step on ONE host with the
fixtures tested. That is evidence for N=1, not for N>1, and CONTEXT.md explicitly flags
this as unverified territory for Phase 10 — especially once the round-trip test exercises
playlist tables (D10-06), which are exactly the category most likely to involve media
(`ThumbnailFilePath`, `IndependentMedia`).

**How to avoid:** D10-04 (locked default): run `fold_back_media(session, step_i_dir)`
after EVERY completed step, not only the last, until the implementation empirically
proves (across a real 3-archive fixture that includes playlist/media content) that
intermediate-step media never appears — and even then, CONTEXT.md's Claude's Discretion
section requires that simplification be justified by disproof, not assumed. Default stays
conservative (every step) unless disproven during implementation.

**Warning signs:** A media-bearing fixture (playlist thumbnail) that passes the fold test
only because the LAST step happens to already contain all needed media — this would mask
a genuine intermediate-step data-loss bug that a differently-ordered fixture would expose.

## Code Examples

### jwlCore mergeDatabase ABI (verified, unchanged for Phase 10)

```rust
// Source: app/src-tauri/src/jwlcore/merge.rs:24-26, verified against jwlcore.py:64-68
/// `int mergeDatabase(const char* path1_dest_dir, const char* path2_src_dir, bool downgrade);`
/// 0 = success. Called N-1 times by the fold, once per (dest_root, source_root) pair,
/// downgrade always false (D10 inherits D5-07 unchanged).
type MergeFn = unsafe extern "C" fn(*const c_char, *const c_char, bool) -> c_int;
```

### Atomic promote primitive (verified, called exactly once by the fold)

```rust
// Source: app/src-tauri/src/archive/save.rs:169-172
pub(crate) fn atomic_replace(temp: &Path, target: &Path) -> Result<(), ArchiveError> {
    fs::rename(temp, target)?;
    Ok(())
}
// Fold commit: atomic_replace(&step_last.join("userData.db"), &session.db_path)
// -- the ONLY call site in the whole fold, after step N-1 succeeds.
```

### Typed error surface (extend, don't replace)

```rust
// Source: app/src-tauri/src/error.rs:79-81 (existing variants, reused verbatim)
MergeUnavailable,
MergeFailed { reason: String },
// Claude's Discretion (CONTEXT.md): fold the 1-indexed failing source into
// `reason` (e.g. format!("source {i} of {n}: {inner_reason}")) rather than
// adding a new enum variant -- keeps the DTO mapping (to_dto, D-14 no-leak)
// unchanged, since `reason` is already never leaked over IPC.
```

### Playlist fixture row set proven to build a full valid graph (D10-06 closure path)

```rust
// Source: app/src-tauri/tests/playlist_import_tests.rs:28-75 (build_container,
// Phase 8's fixture helper -- reuse the SAME row set, not the .jwlplaylist export
// step, since a fold operates on full archive userData.db rows directly)
// PlaylistItemAccuracy(1, 'Exact')
// PlaylistItem(pi_id, Label, ..., Accuracy=1, EndAction=1, ThumbnailFilePath='thumb.jpg')
// IndependentMedia(10, 'thumb-original.jpg', 'thumb.jpg', 'image/jpeg', 'hash-thumb-fixed')
// Location(500, BookNumber=1, ChapterNumber=1, ..., Title='Genesis 1:1')
// PlaylistItemLocationMap(pi_id, 500, MajorMultimediaType=1, BaseDurationTicks=12345)
// + fs::write(dir/"thumb.jpg", b"fake-thumb-bytes")
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| Phase 5: chain Phase 5's `merge_commit` by hand, N-1 times, from the frontend | Phase 10: one backend fold operation, N-1 internal steps, single promote | This phase | Removes N-1 manual dry-run/confirm round-trips for the user; the safety envelope (atomic promote, cleanup-on-failure) moves from "N-1 independent Phase-5 invocations" to "one fold operation with one promote," which is STRICTLY safer (fewer windows where a partial merge could be presented as complete) than manual chaining. |
| Phase 5's playlist coverage: deferred, minimal synthetic fixture aborts jwlCore | Phase 8's `build_container` fixture row set is available and known to produce a real `.jwlplaylist` export successfully | Phase 8 shipped (prior to this phase) | Gives Phase 10 a concrete, already-proven row set to attempt for D10-06's playlist-merge closure — not a new fixture-design problem. |

**Deprecated/outdated:** None — Phase 10 does not deprecate any Phase 5 API; every
Phase 5 function (`dry_run_merge`, `merge_commit`, and their `_with_lib_path` cores)
remains the two-archive entry point, unmodified and still used directly for N=2.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `build_container`'s row set (PlaylistItemAccuracy + PlaylistItem + IndependentMedia + Location + PlaylistItemLocationMap), inserted DIRECTLY into a source archive's `userData.db` (not exported through `.jwlplaylist`), will satisfy whatever jwlCore's merge step needs to avoid the "key not found: 0" abort. This is `[ASSUMED]` — verified only that this row set produces a valid `.jwlplaylist` EXPORT via a different code path (`export_playlist_from_seed`); NOT verified that the SAME row set, present in a full archive being merged via `mergeDatabase`, avoids the abort jwlCore raised in Phase 5. | Priority 4 / D10-06 | If wrong, D10-06's closure attempt still fails and must be honestly re-documented as deferred (per CONTEXT.md's own instruction) rather than claimed closed — no functional regression, but rework of the fixture or an accepted continued gap. |
| A2 | jwlCore's `mergeDatabase` is re-entrant / safe to invoke N-1 times sequentially within one process without needing to reload the library between calls, OR that reloading per call (as `run_merge_with_lib_path` already does, loading fresh via `load_library(lib_path)` every invocation) has no cumulative side effect across steps. `[ASSUMED]` — Phase 5 only ever called it once per process in production code (tests call it multiple times across different processes/fixtures, not chained within one merge operation on accumulating state). No source for jwlCore exists in this repo to confirm. | Priority 3 | If wrong (e.g. hidden process-global state accumulates across calls), a fold step later than the first could behave differently than an equivalent isolated Phase 5 merge — this is exactly what the round-trip test (fold vs. chained `merge_commit_with_lib_path`) is designed to catch, since BOTH legs call the same function the same number of times; a divergence would surface as a round-trip test failure, not silently. |
| A3 | jwlCore never writes loose media files during intermediate fold steps (only possibly at some step, unverified which). `[ASSUMED]` per D10-04's own conservative framing — Phase 5 empirically observed no media writes for N=1 on synthetic fixtures on one host; N>1 behavior is genuinely untested until this phase's own round-trip fixture runs. | Priority 5, Pitfall 4 | Mitigated by design: D10-04 already defaults to running `fold_back_media` after every step regardless of this assumption, so the conservative default absorbs the risk even if the assumption is wrong. |
| A4 | A step-k failure's cleanup (`remove_dir_all` on the fold root) can safely run even if extraction (`extract_zip_slip_safe`) partially wrote files into a `merge/` subdirectory at the moment of failure. `[ASSUMED]` — inherited from Phase 5's identical single-step pattern, not independently re-verified for the multi-step case, though the mechanism (`fs::remove_dir_all` on a whole directory tree) is identical regardless of how many sub-steps exist under it. | Pitfall 3 | Low risk — `remove_dir_all` is directory-tree-agnostic; if Phase 5's pattern is safe for one step's `merge/` subdir, it is safe for N-1 steps' subdirs under the same parent, since the parent is what gets removed. |

## Open Questions

1. **Does the playlist fixture (A1) actually avoid jwlCore's abort inside a merge call, not just an export call?**
   - What we know: the row set is proven sufficient for a `.jwlplaylist` export (a
     different jwlCore-independent code path).
   - What's unclear: whether `mergeDatabase` itself needs additional graph elements
     (markers, maps beyond `PlaylistItemLocationMap`) that `export_playlist_from_seed`
     tolerates being absent but `mergeDatabase` does not.
   - Recommendation: implementation should attempt D10-06's closure EARLY (as the plan's
     first playlist-related task, per CONTEXT.md's own framing of it as "a natural
     byproduct of building the round-trip harness this phase needs anyway"), and if it
     still aborts, capture the EXACT `getLastResult()` string this time (Phase 5's
     VERIFICATION.md only recorded `"key not found: 0"` for a MINIMAL fixture — a
     different error string with the fuller `build_container` fixture would itself be
     diagnostic information worth recording even in failure).

2. **Should the fold's `MergeFailed` error carry a structured step index, or fold it into the `reason` string?**
   - What we know: `ArchiveError::MergeFailed { reason: String }` already exists and its
     DTO mapping never leaks `reason` (generic `error.merge.failed` message_key).
   - What's unclear: whether the frontend needs the 1-indexed failing source
     PROGRAMMATICALLY (e.g. to highlight which list item failed) versus only in a log/
     debug string.
   - Recommendation: CONTEXT.md marks this as Claude's Discretion — default to folding
     `step: usize` into the `reason` string (simplest, no DTO shape change) unless the UI
     design genuinely needs to highlight a specific list item, in which case add a
     `source_index: Option<usize>` field to the DTO (additive, non-breaking).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `jwlCore-amd64.dll` (+ `sqlite3_64.dll`) | Real-DLL leg of the fold round-trip test, `cargo test --jobs 2` | Confirmed available this session — Phase 5's `05-VERIFICATION.md` recorded these tests RAN (not skipped) against the real DLL on this exact host/repo. | v0.32.1 (per Phase 5's `differential.rs` doc comment, `getCoreVersion`) | Skip-as-pass pattern already established (`jwlcore_status_real_load_current_host`) for hosts without the binary (e.g. arm64-windows in CI) — reuse for any new fold FFI tests. |
| Python 3.13.3 / PySide6 | Differential-oracle leg, if Phase 10 extends `tests/differential.rs` to a 3-way chained-pairwise Python comparison | Confirmed available this session (Phase 5 VERIFICATION.md: "Python 3.13.3/PySide6"). | 3.13.3 | Same `#[ignore]`-gated pattern as Phase 5's `rust_ffi_merge_matches_python_merge` — CI remains Rust-only by design; this leg is optional for Phase 10 since the phase brief's oracle is "fold == chained pairwise via Phase 5's OWN commit function" (Rust-internal), not a fresh Python comparison. |
| `cargo test --jobs 2` | All new tests | Available (mandatory flag per CONTEXT.md — default parallelism OOMs the linker) | — | None — this is a hard requirement, not a fallback situation. |

**Missing dependencies with no fallback:** none identified.

**Missing dependencies with fallback:** none beyond the already-established skip-as-pass
pattern for hosts without the native binary.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust: built-in `cargo test` (no external test framework); Frontend: Vitest |
| Config file | `app/src-tauri/Cargo.toml` (workspace test config, none custom); `app/vitest.config.ts` (unchanged) |
| Quick run command | `cargo test --jobs 2 --test merge_orchestration` (fold-specific tests once added) |
| Full suite command | `cargo test --jobs 2` (workspace) and `npx vitest run` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|---------------------|--------------|
| MERGE-03 (criterion 1) | Select 3+ archives, fold in one operation | integration | `cargo test --jobs 2 --test merge_orchestration fold_merge_carries_all_sources -x` | Wave 0 |
| MERGE-03 (criterion 1, Core Value) | Step-k failure leaves session/all sources untouched | integration | `cargo test --jobs 2 --test merge_orchestration fold_step_failure_pristine -x` | Wave 0 |
| MERGE-03 (criterion 2) | Aggregate dry-run shows cumulative effect, one report | integration | `cargo test --jobs 2 --test merge_orchestration fold_dry_run_aggregate -x` | Wave 0 |
| MERGE-03 (criterion 3) | Fold == chained pairwise commits, same order | integration | `cargo test --jobs 2 --test merge_orchestration fold_matches_chained_pairwise -x` | Wave 0 |
| D10-06 | Full playlist graph fixture merges without abort (or honestly documented as still blocked) | integration | `cargo test --jobs 2 --test merge_orchestration fold_playlist_graph_merge -x` | Wave 0 |
| D10-04 | Media fold-back fires per intermediate step when media present | integration | `cargo test --jobs 2 --test merge_orchestration fold_media_intermediate_step -x` | Wave 0 |
| Frontend reorder + fold trigger | Reorder list, invoke fold commands, cancel path | component | `npx vitest run CommandBar.test.tsx` (extend existing file) | Wave 0 (extend existing) |

### Sampling Rate
- **Per task commit:** `cargo test --jobs 2 --test merge_orchestration`
- **Per wave merge:** `cargo test --jobs 2` (full workspace) + `npx vitest run`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `app/src-tauri/tests/merge_orchestration.rs` — extend with fold test bodies (file
      exists from Phase 5; add fold-specific tests, don't create a new file unless the
      existing one grows unwieldy)
- [ ] `common::fresh_v16_db()` fixture helper — already exists (used by Phase 5/8 tests),
      reused as-is for each of the N fold-input fixture DBs
- [ ] Playlist fixture helper — reuse `build_container`'s ROW SET (not the whole
      `.jwlplaylist`-export function) by extracting a shared row-insertion helper if it
      doesn't already exist as a standalone function, OR duplicate the row-insertion SQL
      inline in the new fold test (small enough to not warrant a shared-helper refactor
      across Phase 8/10 test files unless the planner judges otherwise)

*(No framework install needed — `cargo test` and `vitest` are already fully configured.)*

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|-------------------|
| V2 Authentication | no | Desktop app, no auth boundary. |
| V3 Session Management | no | N/A — `ArchiveSession` is an in-memory app-state object, not a web session. |
| V4 Access Control | no | Single-user desktop app. |
| V5 Input Validation | yes | Same zip-slip-safe extraction (`extract_zip_slip_safe`) applied to EACH of the N source archives, unchanged from Phase 5 — never trust archive-internal paths. |
| V6 Cryptography | no | No new crypto surface; `DefaultHasher` content-signature hashing (SipHash, fixed-seed) is explicitly NOT a security primitive — it is a within-process diff key only, per Phase 5's own doc comment, unchanged for Phase 10. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| Zip-slip via a malicious source archive (any of the N inputs) | Tampering | `extract_zip_slip_safe`, applied per-source, unchanged from Phase 5 — Phase 10 does not add a new extraction call site, only calls the existing one N times. |
| Path/interior-NUL injection via a crafted staging directory name | Tampering | `dir_cstring`'s existing interior-NUL guard (`jwlcore/merge.rs`) degrades to a typed `MergeFailed`, never UB — unchanged, applies per fold step identically. |
| Partial-merge exposure (Core Value's sharpest edge in this phase) | Tampering / Denial of intended integrity | Single-promote-at-the-end design (D10-03): no fold step ever writes to `session.db_path` until the final `atomic_replace`; a step-k failure leaves the live session and all N sources byte-identical to their pre-fold state, proven by an explicit test (Pitfall 3/A4). |

## Sources

### Primary (HIGH confidence)
- `app/src-tauri/src/archive/merge.rs` (full file read) — `stage_and_merge`,
  `content_diff`, `dry_run_merge_with_lib_path`, `merge_commit_with_lib_path`,
  `fold_back_media`, `MERGE_SNAPSHOT_TABLES`, module docs.
- `app/src-tauri/src/jwlcore/merge.rs` (full file read) — `run_merge_with_lib_path`,
  `MergeFn`/`LastResultFn` ABI, `merge_availability`, `host_dev_lib_path`.
- `app/src-tauri/src/archive/save.rs:169-172` — `atomic_replace`.
- `app/src-tauri/src/error.rs:79-86, 247-251` — `MergeUnavailable`/`MergeFailed` variants
  and `to_dto` no-leak mapping.
- `app/src-tauri/tests/playlist_import_tests.rs:1-80` — `build_container`, the proven
  full-playlist-graph fixture row set (D10-06 closure candidate).
- `.planning/phases/05-two-archive-merge/VERIFICATION.md` — the recorded playlist gap,
  the confirmed real-DLL/real-Python test evidence, jwlCore v0.32.1.
- `.planning/phases/10-n-way-merge-fold/10-CONTEXT.md` — all locked decisions D10-01
  through D10-07, verified against the above shipped code (not re-litigated here).
- `.planning/ROADMAP.md:214-226`, `.planning/REQUIREMENTS.md:35,128` — MERGE-03 goal and
  success criteria, source of the "ordered" language underpinning D10-01.

### Secondary (MEDIUM confidence)
- None used — every claim above was verifiable directly against shipped source in this
  repo; no external web/docs lookup was needed since Phase 10 introduces no new
  third-party technology.

### Tertiary (LOW confidence)
- See Assumptions Log (A1-A4) — all `[ASSUMED]` claims are logged there rather than
  presented as verified.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new dependencies; every primitive read directly from shipped source.
- Architecture: HIGH — the fold's structure is a direct, mechanical generalization of Phase 5's own dry-run/commit split, verified line-by-line against `archive/merge.rs`.
- Pitfalls: HIGH for Pitfalls 1-3 (directly derived from CONTEXT.md's own stated risks, cross-verified against source); MEDIUM for Pitfall 4 (media fold-back generalization is explicitly unverified territory per Phase 5's own module docs, hence the conservative default).

**Research date:** 2026-07-26
**Valid until:** No external dependency drift risk (zero new packages); re-verify only if Phase 5's `archive/merge.rs` changes before Phase 10 executes.
