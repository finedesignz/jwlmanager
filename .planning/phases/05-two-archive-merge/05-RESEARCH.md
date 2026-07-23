# Phase 5: Two-Archive Merge - Research

**Researched:** 2026-07-22
**Domain:** Native FFI (jwlCore `mergeDatabase`) + throwaway-copy dry-run + differential parity
**Confidence:** HIGH (all findings verified in-repo against Phase 1 loader, Python source, and Phase 2/4 patterns)

## Summary

Phase 5 makes the FIRST real call to the vendored native `jwlCore` merge engine. Phase 1 already did the hard part: it loads the correct `(OS,ARCH)` binary, works around the Windows `sqlite3_64.dll` load quirk (PATH-prepend), resolves the `mergeDatabase` symbol to prove ABI compatibility, and models the arm64-windows "no binary" case as a non-loaded `JwlCoreStatus` (Ok, not Err). Phase 5 extends that load path with a `merge.rs` that actually INVOKES `mergeDatabase`.

The merge algorithm exists ONLY as a compiled binary — there is no Python or Rust reference to port (FUNCTIONALITY-SPEC §7). jwlCore takes two DIRECTORY paths (`path1`=destination, `path2`=source), opens `<dir>/userData.db` in each, and merges source records INTO the destination DB **in place on disk**, returning `0` on success. Because it mutates in place and has NO preview mode, the dry-run runs the REAL merge on a throwaway `fs::copy` of the destination DB, snapshot-diffs before/after with Phase 2's `DryRunReport`, and discards the copy — the exact analogue of Phase 4's `dry_run_downgrade`. This approach is CONFIRMED VIABLE against the existing throwaway-copy machinery (`save_v14_copy`, `downgrade.rs:610`).

**Primary recommendation:** Add `jwlcore/merge.rs` reusing Phase 1's `resolve_lib_name`/`resolve_lib_path`/`load_library`; expose two Tauri commands (`merge_dry_run`, `merge_commit`) sharing one internal FFI routine; add `ArchiveError::MergeUnavailable` + `MergeFailed{reason}`; extend `tests/differential.rs` with a Rust-FFI-vs-Python parity leg on synthetic fixtures. Pass `downgrade=false`. No progress callback for MVP — blocking call on the Tauri worker thread + a frontend busy state.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Load/resolve jwlCore binary | Rust core (jwlcore/loader.rs) | — | Already owned by Phase 1; reuse, never duplicate |
| Invoke `mergeDatabase` FFI | Rust core (jwlcore/merge.rs, NEW) | — | Unsafe FFI stays isolated in one module |
| Materialize two-dir layout + extract source | Rust core (archive/extract.rs) | — | Zip-slip-safe extraction already owned by Phase 1 |
| Dry-run snapshot-diff | Rust core (db/delete.rs diff) | archive/merge dry-run | Reuse `snapshot_tables`/`diff_snapshots` |
| Confirm/cancel + busy state | Frontend (WebView) | — | Reuse Phase 2/4 dry-run preview component |
| Error → DTO mapping | Rust core (error.rs) | Frontend (message_key) | Typed error, no leak across IPC |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `libloading` | (in-tree, Phase 1) | Bind + call jwlCore symbols | Already the loader's binding lib; ABI proven |
| `rusqlite` | (in-tree) | Snapshot PK sets for dry-run diff | Phase 2/4 dry-run uses it |
| jwlCore native | v0.32.1 (vendored binary) | The merge engine | Only impl of merge semantics; no source to port |
| `tempfile` | (in-tree) | Throwaway dirs under `session.temp_dir` | Session already uses `TempDir` |

No NEW external packages. Phase 5 uses only crates already in the tree. **No Package Legitimacy Audit needed** (no installs).

## Architecture Patterns

### System Architecture Diagram

```
User picks source .jwlibrary
        │
        ▼
[merge_dry_run command] ──────────────────────────────────┐
        │                                                   │
        ▼                                                   │
 fs::copy(session.db_path → <throwaway>/userData.db)        │  (arm64/no-lib?
 extract source → <throwaway>/merge/ (zip-slip-safe)        │   → MergeUnavailable)
        │                                                   │
        ▼                                                   │
 snapshot_tables(BEFORE)                                    │
        │                                                   │
        ▼                                                   │
 FFI: mergeDatabase(<throwaway>, <throwaway>/merge, false) ─┤ non-zero →
        │                                                   │ getLastResult()
        ▼                                                   │ → MergeFailed
 snapshot_tables(AFTER) → diff_snapshots → DryRunReport     │
        │                                                   │
 discard throwaway ────────────────────────────────────────┘
        │
        ▼
 Frontend shows added/overwritten/deleted  ──►  [Cancel] (nothing committed)
        │
     [Confirm]
        ▼
[merge_commit command]
 stage copy of session.db_path → <staging>/userData.db
 extract source → <staging>/merge/
 FFI: mergeDatabase(<staging>, <staging>/merge, false)
        │ success
        ▼
 copy <staging>/userData.db → session.db_path ; session.dirty = true
 (fold merged media into session.entries IF jwlCore wrote any — verify)
        │
        ▼
 refresh UI (regroup)   ──►  normal Save path trims+writes archive
```

### Recommended Module Structure
```
src/jwlcore/
├── loader.rs      # Phase 1 — reuse resolve_lib_name/resolve_lib_path/load_library
├── merge.rs       # NEW — unsafe extern "C" type + invoke mergeDatabase, map result
└── mod.rs         # export merge commands
src/archive/
├── merge.rs       # NEW (or fold into jwlcore/merge.rs) — two-dir layout, dry-run, commit
└── extract.rs     # Phase 1 — reuse for source extraction
```

### Pattern 1: One FFI routine, two callers (DRY the unsafe)
**What:** A single `fn run_merge(app, dest_root: &Path, source_root: &Path) -> Result<(), ArchiveError>` does the load+call+result-map. `merge_dry_run` and `merge_commit` both call it; they differ only in whether the result DB is discarded or promoted.
**Verified ABI (from `jwlcore.py:64-65` + `loader.rs` symbol resolution):**
```rust
// mergeDatabase(const char* path1, const char* path2, bool downgrade) -> int  (0 = ok)
type MergeFn = unsafe extern "C" fn(*const c_char, *const c_char, bool) -> c_int;
type LastResultFn = unsafe extern "C" fn() -> *const c_char; // nullable, read-only
// call:
let dest = CString::new(dest_root.to_string_lossy().as_bytes())?;
let src  = CString::new(source_root.to_string_lossy().as_bytes())?;
let rc = unsafe { merge(dest.as_ptr(), src.as_ptr(), false) };
if rc != 0 {
    let reason = unsafe { last_result() }; // CStr::from_ptr if non-null, to_string_lossy
    return Err(ArchiveError::MergeFailed { reason });
}
```

### Anti-Patterns to Avoid
- **Second load path that omits the Windows PATH-prepend** — jwlCore statically imports `sqlite3_64.dll`; without loader.rs's PATH-prepend the OS loader HARD-TERMINATES the process (loader.rs:84-99). Reuse `load_library`.
- **Passing a FILE path instead of a DIRECTORY** to `mergeDatabase` — both args are DIRS containing `userData.db` (JWLManager.py:2670-2672).
- **Running the merge against `session.db_path` directly for the dry-run** — jwlCore mutates in place; the live session would be corrupted. Always a copy.
- **Porting the Python `crash_box + sys.exit()`** (JWLManager.py:2682-2685) — that's the defect; use typed errors + rollback.
- **Byte-diffing merged DBs in tests** — VACUUM + jwlCore ID-densifying diverge bytes legitimately.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Merge de-dup / conflict resolution | A Rust reimplementation | jwlCore `mergeDatabase` | Semantics exist only in the binary; reimplementing = new bug surface + parity loss |
| Arch-aware binary load + Windows DLL fix | New loader | Phase 1 `loader.rs` helpers | Hard-won PATH-prepend; single load path |
| Before/after row diff | New diff code | `db::delete::diff_snapshots`/`snapshot_tables` | Reused by Phase 2 + 4; never copy-paste |
| Throwaway-copy + best-effort cleanup | New machinery | `downgrade.rs::save_v14_copy` pattern | Proven; session-untouching |
| Zip-slip-safe extraction | Raw extractall | `archive/extract.rs` | Security constraint (zip-slip fixed in Phase 1) |

**Key insight:** Phase 5 is almost entirely composition of existing, tested pieces. The only genuinely new code is the ~30-line unsafe FFI call and the two-directory staging layout.

## Runtime State Inventory

Not a rename/refactor phase — N/A. (No stored data, service config, OS-registered state, secrets, or build artifacts embed a renamed string.)

## Common Pitfalls

### Pitfall 1: jwlCore process-global state clobbered by concurrent merges
**What goes wrong:** `getLastResult()` and the progress-callback slot are process-global. A second merge overwrites the first's result string.
**Why:** C library keeps a single static result buffer.
**How to avoid:** Serialize merges under the `SessionState` mutex; read `getLastResult()` immediately after a non-zero return, in the same critical section (D5-06).
**Warning signs:** Intermittent wrong/empty error messages under rapid re-merge.

### Pitfall 2: Merged media not folded back into the session archive
**What goes wrong:** jwlCore operates on DIRECTORIES, not just DBs. If it copies playlist/media blobs from `path2` into `path1`, the merged files live in the staging dir but aren't added to `session.entries`, so Save drops them.
**Why:** Save rebuilds the zip from `session.entries` (save.rs:104); new files unknown to it are excluded.
**How to avoid:** After a committed merge, diff the staging dir against `session.temp_dir`; fold any new media files into `session.entries`. VERIFY empirically with a media-bearing fixture — if jwlCore only touches `userData.db`, this is a no-op.
**Warning signs:** Merged playlists reference media that's missing after save.

### Pitfall 3: Wrong-arch / missing lib crashes instead of degrading
**What goes wrong:** Calling into a lib that didn't load panics.
**Why:** Skipping the Phase 1 load-status check.
**How to avoid:** Check `resolve_lib_name`/load result FIRST; return `MergeUnavailable` (D5-09). arm64-windows must show "merge unavailable here," not crash.
**Warning signs:** Process exit on merge attempt on arm64.

### Pitfall 4: Dry-run and commit produce different results
**What goes wrong:** Preview counts don't match the committed merge.
**Why:** Preview and commit started from different DB states, or extra ops (trim/VACUUM) ran in one but not the other.
**How to avoid:** BOTH run the identical `mergeDatabase` against a bit-identical `fs::copy` of `session.db_path`; do NOT trim between merge and snapshot in the dry-run. Trim happens later on Save, equally for both.
**Warning signs:** "N added" in preview ≠ actual added after commit.

## Code Examples

### The two-directory layout (from Python, verbatim semantics)
```python
# JWLManager.py:2670-2672 — the authoritative call
with ZipFile(file, 'r') as zipped:
    zipped.extractall(f'{TMP_PATH}/merge')          # source → <root>/merge
res = merge_databases(f'{TMP_PATH}', f'{TMP_PATH}/merge', False)
#                     ^dest dir      ^source dir        ^downgrade
# jwlcore.py:74 — path1=dest (has userData.db), path2=source
```

### Skip-as-pass gate for the FFI test (mirror loader.rs)
```rust
// tests: skip when the host has no binary (e.g. aarch64-windows CI)
let name = match resolve_lib_name(std::env::consts::OS, std::env::consts::ARCH) {
    Ok(n) => n,
    Err(_) => return, // no binary for this host — nothing to test
};
```

## State of the Art

| Old Approach (Python) | Current Approach (Tauri) | Impact |
|--------------|------------------|--------|
| `crash_box + sys.exit()` on merge error | Typed `MergeFailed{reason}` + rollback, no exit | App survives a bad merge |
| Arch-blind load (`sys.platform` only) | Arch-aware `(OS,ARCH)` (Phase 1) | arm64 handled, not silently x86-assumed |
| In-place merge on the one `TMP_PATH` | Copy-first (dry-run) / staging-copy (commit) | Live session never corrupted; real preview |
| Progress dialog via C callback | Blocking call on worker thread + busy UI | Simpler; no cross-thread `unsafe` callback |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | jwlCore writes ONLY `<path1>/userData.db` (not media blobs into path1) | Pitfall 2 | If it writes media, commit must fold new files into `session.entries` or saved archive loses media. VERIFY with media fixture. |
| A2 | `mergeDatabase` return `0`=success, non-zero=fail is total (no partial-success codes) | D5-02/ABI | A partial-success code treated as failure would falsely abort; verify against jwlCore behavior on a known-good merge. |
| A3 | Merging differing source schema versions is handled internally by jwlCore | D5-08 | A schema-mismatch merge could fail or corrupt; document observed behavior before shipping cross-version merge. |

## Open Questions

1. **Does jwlCore copy media files between the two directories, or only merge the DBs?**
   - Known: it takes DIR paths and mutates `<path1>/userData.db`.
   - Unclear: whether new media blobs appear under `<path1>`.
   - Recommendation: build a media-bearing source fixture, run the FFI merge, list `<staging>` for new files; if present, fold into `session.entries` on commit.

2. **What non-zero return codes does `mergeDatabase` emit, and what does `getLastResult()` say for each?**
   - Recommendation: capture returns for (a) good merge, (b) corrupt source, (c) schema mismatch during the FFI test; map them to the `MergeFailed` message. Env-gated (needs the real DLL).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `libs/jwlCore-amd64.dll` | FFI merge (Windows x64) | ✓ | v0.32.1 | — |
| `libs/sqlite3_64.dll` | jwlCore static import | ✓ (co-located) | — | loader PATH-prepend |
| `libs/libjwlCore-x86_64.so` / `.dylib` / `-arm64.so` | Linux/macOS merge | ✓ | v0.32.x | — |
| aarch64-windows jwlCore | Windows arm64 merge | ✗ | — | `MergeUnavailable` (ships, merge disabled) |
| python3 + PySide6 + root-staged DLLs | Python parity leg of differential test | ✗ in CI | — | `#[ignore]`; RECORDED MANUAL GATE (matches Phase 1) |

**Missing with fallback:** arm64-windows merge → clear "unavailable" message (criterion 4). Python parity leg → `#[ignore]`d, run manually with `res/requirements.txt` installed.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `tests/differential.rs` integration harness |
| Config file | none (cargo) |
| Quick run command | `cargo test -p jwlmanager-lib merge` |
| Full suite command | `cargo test` (FFI leg skips-as-pass off-host; Python leg `--ignored`) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MERGE-01 | FFI merge succeeds on two synthetic fixtures; all source records present, no dups, referential integrity | integration (FFI) | `cargo test --test merge_ffi` | ❌ Wave 0 |
| MERGE-01 | arm64/no-lib → `MergeUnavailable`, never panic | unit | `cargo test merge_unavailable` | ❌ Wave 0 |
| MERGE-02 | dry-run report add/overwrite/delete matches committed merge; cancel commits nothing | integration | `cargo test merge_dry_run_matches_commit` | ❌ Wave 0 |
| MERGE-02 | source archive bytes unchanged after merge | unit | `cargo test merge_source_immutable` | ❌ Wave 0 |
| MERGE-03 | Rust-FFI merge vs Python `merge_databases` on same fixtures → normalized table state equal | differential (ignored) | `cargo test --test differential -- --ignored` | ⚠️ extend |

### Sampling Rate
- **Per task commit:** `cargo test -p jwlmanager-lib merge`
- **Per wave merge:** `cargo test`
- **Phase gate:** full suite green + manual run of the Python parity leg (RECORDED MANUAL GATE)

### Wave 0 Gaps
- [ ] `tests/merge_ffi.rs` — FFI merge of two synthetic v16 fixtures; skip-as-pass off-host (MERGE-01)
- [ ] Synthetic dest+source fixture builder with overlapping + disjoint records across merge-affected tables (extend Phase 3/4 fixture gen)
- [ ] Extend `tests/differential.rs` — Rust-FFI-vs-Python normalized-state comparison leg (MERGE-03)
- [ ] Media-bearing source fixture to resolve Open Question 1

## Security Domain

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | Zip-slip-safe extraction (Phase 1 `extract.rs`) for the incoming source archive; validate it's a real archive before merge (mirror `check_validity` gate, JWLManager.py:2661) |
| V6 Cryptography | no | — |
| V12 File handling | yes | Source read-only; dest always a copy; atomic-replace on eventual save (Phase 1) |

### Known Threat Patterns
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious zip path traversal in source archive | Tampering | Zip-slip validation on extraction (Phase 1) |
| Corrupt/hostile source DB crashes native lib | DoS | Non-zero return → typed `MergeFailed`; the dry-run runs on a copy so a crash can't corrupt the session; consider catching/limiting FFI blast radius |
| Path/SQL injection via table names | Tampering | Snapshot queries use fixed const identifiers, never user input; all params bound |

## Sources

### Primary (HIGH confidence)
- `jwlcore.py:59-83` — FFI ABI (argtypes/restype), path1=dest/path2=source
- `JWLManager.py:1010-1014, 2645-2694` — merge_file / merge_items call site, two-dir layout, error path
- `app/src-tauri/src/jwlcore/loader.rs` — arch-aware load, Windows DLL fix, `mergeDatabase` symbol resolution, `JwlCoreStatus`
- `app/src-tauri/src/db/delete.rs:112-259` — `DryRunReport`, `snapshot_tables`, `diff_snapshots`
- `app/src-tauri/src/archive/downgrade.rs:495-627` — `dry_run_downgrade` + `save_v14_copy` throwaway-copy pattern
- `app/src-tauri/src/archive/save.rs:90-260` — `rebuild_zip` / `write_archive_from_db_source` (session-untouching output)
- `app/src-tauri/src/error.rs` — `ArchiveError` / `ErrorDto` no-leak mapping
- `app/src-tauri/tests/differential.rs` — Python-oracle harness conventions
- `.planning/research/FUNCTIONALITY-SPEC.md` §1.6, §7 — "merge algorithm lives only in the compiled binary"

### Secondary (MEDIUM confidence)
- Phase 4 `04-CONTEXT.md` — throwaway-copy + dry-run precedent, structural template

## Metadata

**Confidence breakdown:**
- ABI / call semantics: HIGH — verified in ctypes decls + resolved symbol + live Python call site
- Dry-run-on-copy viability: HIGH — direct analogue of shipped `dry_run_downgrade`
- Media handling: LOW — Open Question 1, verify empirically
- arm64 / error path: HIGH — reuses Phase 1 `JwlCoreStatus` model

**Research date:** 2026-07-22
**Valid until:** stable (in-repo, no external moving parts) — re-verify only if jwlCore binary version changes
</content>
