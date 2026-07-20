---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: Completed 01-05-PLAN.md (atomic save, save-as, new_archive, ARCH-02 differential oracle)
last_updated: "2026-07-20T05:33:18.849Z"
progress:
  total_phases: 11
  completed_phases: 0
  total_plans: 7
  completed_plans: 6
  percent: 0
---

# Project State — JWL Manager (Tauri)

## Project Reference

**Core value:** Never lose or corrupt a user's archive.
**Current focus:** Phase 1 — Open, View, Save (Foundation Slice)

## Current Position

**Phase:** 1 of 11 — Open, View, Save (Foundation Slice)
**Plan:** 7 of 7
**Status:** Ready to execute
**Progress:** [█████████░] 86%

## Performance Metrics

- Phases complete: 0/11
- Requirements delivered: 0/47

## Accumulated Context

### Decisions

- Roadmap derived goal-backward from 47 v1 requirements across 11 vertical-slice phases (fine granularity).
- QA-01/02/03 front-loaded into Phase 1/2 rather than deferred — per-phase testing is the parity oracle in the absence of a retrofitted characterization harness.
- SAFE-01 (dry-run preview) placed in Phase 2, alongside the first destructive operation (delete), so every later destructive capability (downgrade, merge) reuses the same mechanism.
- Merge (jwlCore FFI) is never reimplemented — Phase 5/10 bind the existing native lib via libloading.
- MERGE-03 (N-way fold) and IO-04 (incremental export) split into their own phases (10, 9) as new-value enhancements, not bundled into parity phases.
- [Phase ?]: 01-01: zip crate pinned exact =8.6.0; vite/vitest bumped to current registry majors; shadcn init deferred to a future UI plan
- [Phase ?]: tempfile moved dev-dependency to regular dependency (ArchiveSession owns TempDir in production code, 01-07)
- [Phase ?]: Schema gate checks manifest schemaVersion before opening extracted userData.db as SQLite (v16-only gate, 01-07)
- [Phase 01]: 01-02: serde_json preserve_order added for manifest flatten catch-all unknown-key ordering; zip-slip test corrected to match zip 8.6.0's real enclosed_name() containment behavior (only traversal variants literally error; absolute/duplicate/symlink variants are safely contained, asserted via extraction-root escape check)
- [Phase 01]: 01-03: jwlCore selection made arch-aware via (OS,ARCH) match, fixing jwlcore.py's OS-only selection bug; arm64-windows (no shipped binary) returns a non-loaded JwlCoreStatus (Ok), never an Err; libs/libjwlCore.dylib confirmed universal (fat) Mach-O covering x86_64+arm64; Windows dependent-DLL load required a temporary PATH prepend (LOAD_WITH_ALTERED_SEARCH_PATH alone hard-crashed the process on sqlite3_64.dll resolution)
- [Phase 01]: 01-04: UI language hardcoded to 'en' for resources.db label synthesis; Phase 1 has no locale switcher (deferred to Phase 11)
- [Phase 01]: 01-04: open_and_validate gained resources_db_path param, open_archive gained AppHandle param to resolve bundled resources.db (dev/prod fallback mirrors jwlcore loader)
- [Phase 01]: 01-05: raw Win32 ReplaceFileW/MoveFileExW rejected (verified failing in this environment); std::fs::rename used on both platforms for atomic save-replace
- [Phase 01]: 01-05: sync_all must be called on the write-capable ZipWriter::finish() handle, never a fresh read-only File::open (ERROR_ACCESS_DENIED on Windows)
- [Phase 01]: 01-05: new_archive seeds from the real res/blank v16 archive, never a hand-built schema, keeping the ARCH-02 oracle non-circular
- [Phase 01]: 01-05: ARCH-02 Python oracle is #ignore'd with an explicit reason (PySide6 not installed); recorded manual gate required before Phase 1 completion

### Todos

- None yet — Phase 1 planning not started.

### Blockers

- None.

## Session Continuity

**Last session:** 2026-07-20T05:33:18.839Z
**Stopped at:** Completed 01-05-PLAN.md (atomic save, save-as, new_archive, ARCH-02 differential oracle)
**Next action:** Execute 01-04-PLAN.md.
