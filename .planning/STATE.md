---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: verifying
stopped_at: Completed 01-06-PLAN.md (command bar + typed error surface + jwlCore capability notice) — Phase 1 all 7 plans complete
last_updated: "2026-07-21T04:38:56.523Z"
progress:
  total_phases: 11
  completed_phases: 1
  total_plans: 10
  completed_plans: 9
  percent: 9
---

# Project State — JWL Manager (Tauri)

## Project Reference

**Core value:** Never lose or corrupt a user's archive.
**Current focus:** Phase 3 — Schema Upgrade

## Current Position

Phase: 3 (Schema Upgrade) — EXECUTING
Plan: 2 of 3 complete (03-02 done; 03-03 remaining — Python differential test)
**Phase:** 1 of 11 — Open, View, Save (Foundation Slice)
**Plan:** 7 of 7
**Status:** Phase 1 complete — ready for verification; Phase 3 in progress
**Progress:** [████████░░] 80%

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
- [Phase 01]: 01-06: double-click guard implemented via a synchronous ref (not React state) so a second click dispatched before React re-renders the disabled button is still caught
- [Phase 01]: 01-06: shadcn deferred; CommandBar/ErrorBanner/JwlCoreNotice use plain HTML + the existing hand-authored CSS-token stylesheet (01-01's substitute pattern), not a new component registry
- [Phase 01]: 01-06: cancel affordance for Open/New/Save-As is the native dialog dismissal (open()/save() resolving null), not a separate abort button, per the plan's own action text
- [Phase 01]: 01-06: lib/errors.ts keys off ArchiveError::to_dto's real snake_case code strings (not_a_zip, zip_slip_rejected, ...) read directly from error.rs, not the plan's illustrative PascalCase variant names
- [Phase ?]: v12/v13 fixtures apply only the documented v16<->v14 delta; not independently-verified
- [Phase 03]: 03-02: upgrade_to_v16 ports JWLManager.py:1016-1070's DDL transactionally (rusqlite Transaction, rollback on any failure) — never the Python original's silent except:pass; conditional Specialty/Edition INSERT source columns preserve pre-existing data instead of the original's data-destroying NULL,NULL
- [Phase 03]: 03-02: post-upgrade v16 contract validator (validate_v16_contract) runs before session acceptance so an unknown/incomplete v12/v13 shape gap fails loud instead of being silently stamped v16
- [Phase 03]: 03-02: archive/mod.rs and archive/manifest.rs schema gates widened to 12-16 sharing one MIN/MAX/WORKING const module so they cannot drift; in-range manifest/PRAGMA mismatch normalizes to the final PRAGMA value rather than rejecting
- [Phase 03]: 03-02: foreign_keys does NOT default OFF in this build's bundled SQLite (contra 03-RESEARCH.md's assumption) — upgrade_to_v16 explicitly disables it on the connection before opening the transaction (pragma changes are a no-op inside an active transaction), never re-enables it
- [Phase 03]: 03-02: ArchiveError::UnsupportedSchema removed entirely (zero remaining producers after gate widen) along with its unsupported_schema_phase3 message_key and errors.ts case

### Todos

- Manual gate: `npm run tauri dev` visual boot + interaction check (01-06) — exercise all four command-bar actions, double-click Open, cancel a dialog, confirm jwlCore notice on arm64.
- Manual gate: Linux WebKitGTK 9,000-row scroll smoothness check (01-04).
- Manual gate: ARCH-02 Python differential oracle with PySide6 installed + real JW Library open (01-05).

### Blockers

- None.

## Session Continuity

**Last session:** 2026-07-20T00:00:00.000Z
**Stopped at:** Completed 03-02-PLAN.md (transactional schema upgrade + contract validator + dual-gate widening) — Phase 3 plan 2 of 3 complete
**Next action:** Execute 03-03-PLAN.md (Python differential test against real v14 owner archives), then Phase 3 verification.
