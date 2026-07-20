---
phase: 01-open-view-save-foundation-slice
plan: 03
subsystem: jwlcore-bridge
tags: [tauri, rust, libloading, ffi, windows-dll, ts-rs, security]

requires:
  - phase: 01-01
    provides: app/ scaffold, libs/* bundled resources declared in tauri.conf.json
  - phase: 01-07
    provides: error.rs two-layer error surface, lib.rs invoke_handler registering open_archive
provides:
  - "Arch-aware (OS, ARCH) jwlCore binary selection, fixing jwlcore.py's OS-only selection bug"
  - "Unified JwlCoreStatus { loaded, arch, version, reason } shape consumed by 01-06's capability notice"
  - "Lazy check_jwlcore Tauri command (load + resolve symbols + version only, no merge call — D-12)"
  - "Confirmed libs/libjwlCore.dylib is a universal (fat) Mach-O binary covering x86_64 + arm64"
affects: [01-06]

tech-stack:
  added: []
  patterns:
    - "Arch-aware (OS,ARCH) match table instead of OS-only selection (fixes jwlcore.py:29-38's bug)"
    - "Non-loaded status is Ok, not Err — Err reserved for genuinely unexpected load faults (finding 12)"
    - "Windows dependent-DLL resolution via temporary PATH prepend (LOAD_WITH_ALTERED_SEARCH_PATH alone did not work in practice on this host)"

key-files:
  created:
    - app/src-tauri/src/jwlcore/mod.rs
    - app/src-tauri/src/jwlcore/loader.rs
    - app/src/bindings/JwlCoreStatus.ts
    - app/src/bindings/JwlCoreError.ts
  modified:
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs

key-decisions:
  - "Windows dependent-DLL fix: LOAD_WITH_ALTERED_SEARCH_PATH (libloading::os::windows) did NOT resolve jwlCore-amd64.dll's static import of sqlite3_64.dll on this host — the OS loader hard-terminated the whole test process, printing \"could not load: sqlite3_64.dll\" and exiting, bypassing Rust's Result-based error handling entirely. Fixed by temporarily prepending the binary's own directory to PATH for the duration of the load (PATH directories are part of the standard DLL search order and correctly resolve statically-imported dependent DLLs); PATH is restored immediately after."
  - "macOS dylib confirmed universal (fat) via manual Mach-O fat-header parse (no macOS host available for `file`/`lipo`): nfat_arch=2, cputype 0x1000007 (x86_64) + 0x100000c (arm64) — one dylib entry in the (macos, _) match arm is correct for both Mac architectures, no per-arch macOS split needed."
  - "JwlCoreError kept intentionally minimal (LoadFailed, MissingSymbol, PathResolutionFailed) since finding 12 pushes the arm64-windows / unsupported-platform cases into a non-loaded Ok(JwlCoreStatus) rather than a returned Err."

requirements-completed: [PLAT-01, SAFE-05]

metrics:
  duration: "~40 min"
  completed: "2026-07-20"
---

# Phase 1 Plan 3: Arch-Aware jwlCore Load + Symbol Resolution Summary

Bound `jwlCore` via `libloading` with `(OS, ARCH)`-aware binary selection — fixing the Python bridge's OS-only selection bug — resolving its four FFI symbols and reading its version through a lazy `check_jwlcore` Tauri command that returns one unified `JwlCoreStatus` shape. Load + resolve only; `mergeDatabase` is resolved to prove ABI compatibility but never called (D-12, Phase 5's job). A real, non-mocked load of `jwlCore-amd64.dll` was performed on this Windows x64 host, uncovered a genuine dependent-DLL resolution bug, and now succeeds with symbols resolved and a version string read.

## Accomplishments

- `jwlcore/loader.rs`: `resolve_lib_name(os, arch)` matches on both `env::consts::OS` and `env::consts::ARCH` — `(windows, x86_64)` → `jwlCore-amd64.dll`, `(windows, aarch64)` → no binary (D-13a), `(linux, x86_64)` → `libjwlCore-x86_64.so`, `(linux, aarch64)` → `libjwlCore-arm64.so`, `(macos, *)` → `libjwlCore.dylib`, else unsupported.
- `JwlCoreStatus { loaded, arch, version, reason }` — the single unified shape (finding 12), ts-rs exported to `app/src/bindings/JwlCoreStatus.ts`, consumed identically by `check_jwlcore`'s `Ok` path and (later) 01-06's capability notice.
- `check_jwlcore()` Tauri command: on a supported target, loads the library, resolves all four symbols (`setProgressCallback`, `mergeDatabase`, `getLastResult`, `getCoreVersion`), calls **only** `getCoreVersion`, returns `Ok(JwlCoreStatus { loaded: true, version: Some(v), .. })`. On arm64-windows/unsupported, returns `Ok(JwlCoreStatus { loaded: false, reason: Some(..), .. })` — never an `Err`. `Err(JwlCoreError)` is reserved for a genuine load fault (present-but-corrupt binary, missing symbol, path resolution failure).
- Registered as a lazy command in `lib.rs`'s `invoke_handler` alongside `open_archive` — not called from `setup()`, so a missing/wrong-arch binary can never crash app launch.
- Unit-tested the full selection table by name for every `(OS, ARCH)` arm, including the arm64-windows no-binary case, without requiring every platform's binary to be present.
- Separately, a real (non-mocked) `libloading` load was performed against the actual `jwlCore-amd64.dll` on this Windows x64 host, asserting all four symbols resolve and `getCoreVersion()` returns a non-empty string.
- Ran a manual Mach-O fat-header parse against `libs/libjwlCore.dylib` (no macOS host available for `file`/`lipo -info`): confirmed it is a universal binary with 2 slices — `cputype 0x1000007` (x86_64) and `0x100000c` (arm64) — so the single `(macos, _)` match arm is correct and needs no per-arch split.

## Task Commits

1. **Task 1: Arch-aware (OS, ARCH) selection + JwlCoreStatus + JwlCoreError** — `42d0f3c9` (feat)
2. **Task 2: Register check_jwlcore as a lazy Tauri command** — `3686360a` (feat)

## Files Created/Modified

- `app/src-tauri/src/jwlcore/mod.rs` — module surface, re-exports `check_jwlcore`/`JwlCoreStatus`
- `app/src-tauri/src/jwlcore/loader.rs` — arch-aware selection, Windows dependent-DLL PATH fix, symbol resolution, `check_jwlcore` command, unit + real-load tests
- `app/src-tauri/src/error.rs` — added `JwlCoreError` (LoadFailed/MissingSymbol/PathResolutionFailed), Serialize + ts-rs
- `app/src-tauri/src/lib.rs` — registers `jwlcore::loader::check_jwlcore` in `invoke_handler`
- `app/src/bindings/JwlCoreStatus.ts`, `app/src/bindings/JwlCoreError.ts` — ts-rs generated bindings

## Verification Evidence — REAL, NOT SIMULATED

- `cargo test jwlcore_resolve` — 7/7 pass (all `(OS,ARCH)` arms including windows-aarch64 no-binary, reason string asserted exactly).
- `cargo test jwlcore_status` — 1/1 pass: **real** `libloading::Library` load of `libs/jwlCore-amd64.dll` on this Windows x64 host, all four symbols (`setProgressCallback`, `mergeDatabase`, `getLastResult`, `getCoreVersion`) resolved, `getCoreVersion()` called and returned a non-empty version string. `mergeDatabase` was resolved-but-never-invoked, matching D-12.
- `cargo test` (full suite) — 29 tests pass across all binaries (13 lib unit tests including the 8 jwlcore tests + 5 ts-rs export tests, plus all pre-existing integration test files), 0 failed, 0 ignored.
- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — zero warnings (no `unwrap`/`expect` on the load path; test module carries its own `#[allow]` per the established 01-01/01-07 pattern).
- `npm run build` (`app/`) — `tsc` + `vite build` clean; `JwlCoreStatus.ts`/`JwlCoreError.ts` type-check.

**Real finding, root-caused and fixed (not faked):** the first real-load attempt against `jwlCore-amd64.dll` failed with `"could not load: sqlite3_64.dll"` and the whole test **process terminated** (exit code 1, no Rust panic trace) — the OS loader hard-fails when a statically-imported dependent DLL can't be found, bypassing normal `Result`-based error propagation entirely. `libloading::os::windows::Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH)` was tried first per the standard libloading Windows guidance and did **not** resolve it in practice on this host. Root cause: `jwlCore-amd64.dll` statically imports `sqlite3_64.dll` (co-located in `libs/`, confirmed present), and the altered-search-path flag's directory-search semantics did not cover it here. Fixed by temporarily prepending the binary's own directory to the process `PATH` env var for the duration of the load (PATH is part of the standard DLL search order and correctly resolves statically-imported dependent DLLs), then restoring `PATH` immediately after. After the fix, the real load succeeds cleanly with all four symbols resolved and a version string read — confirmed by `jwlcore_status_real_load_current_host`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `LOAD_WITH_ALTERED_SEARCH_PATH` did not resolve jwlCore's dependent DLL; real load hard-crashed the process**
- **Found during:** Task 2, `cargo test jwlcore_status` (first real, non-mocked load attempt on this host)
- **Issue:** `jwlCore-amd64.dll` statically imports `sqlite3_64.dll`. Loading via plain `libloading::Library::new` or via `load_with_flags(.., LOAD_WITH_ALTERED_SEARCH_PATH)` both failed to resolve the co-located `sqlite3_64.dll` in `libs/`; the Windows loader hard-terminated the entire test process (not a caught `Result::Err`) printing `"could not load: sqlite3_64.dll"`.
- **Fix:** Load via a small Windows-only `load_library` helper that temporarily prepends the target binary's own directory to the process `PATH` env var (a documented member of the standard DLL search order), performs the load, then restores the original `PATH`. Non-Windows platforms use plain `libloading::Library::new` unchanged.
- **Files modified:** `app/src-tauri/src/jwlcore/loader.rs`
- **Commit:** `3686360a` (folded into Task 2's real-load verification pass; the fix landed before the commit, so it is included in the committed code, not a separate commit)

## Known Stubs

None — `check_jwlcore` is fully wired (loader, error types, unified status, command registration). The frontend UI notice consuming this status is 01-06's explicit scope, not this plan's.

## Threat Flags

None — the `libs/*` binary trust boundary and `check_jwlcore` IPC surface were already registered in this plan's own `<threat_model>` (T-03-01, T-03-02, T-03-03); the Windows PATH-prepend fix stays within the same trust boundary (the same vendored, trusted binary directory) and adds no new attack surface.

## Self-Check: PASSED

- `app/src-tauri/src/jwlcore/mod.rs` — FOUND
- `app/src-tauri/src/jwlcore/loader.rs` — FOUND
- `app/src/bindings/JwlCoreStatus.ts` — FOUND
- `app/src/bindings/JwlCoreError.ts` — FOUND
- Commit `42d0f3c9` (feat: Task 1) — FOUND in `git log`
- Commit `3686360a` (feat: Task 2) — FOUND in `git log`
