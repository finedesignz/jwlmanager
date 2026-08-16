---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: verifying
last_updated: "2026-08-16T17:06:22.844Z"
progress:
  total_phases: 11
  completed_phases: 6
  total_plans: 49
  completed_plans: 44
  percent: 55
---

# Project State — JWL Manager (Tauri)

## Project Reference

**Core value:** Never lose or corrupt a user's archive.
**Current focus:** Phase 11 — Platform Polish (Signing, Localization, Theme)

## Current Position

Phase: 11 (Platform Polish) — IN PROGRESS
Plans: 11-01-PLAN.md (settings/theme tracer) EXECUTED — PLAT-04 satisfied. 11-02-PLAN.md (Windows signing wiring) EXECUTED — PLAT-02 wired, fail-closed, and gated (real signature verification remains a documented manual step, blocked on Azure credentials this environment cannot provision). 11-03-PLAN.md (i18n architecture: dependency-free catalog + I18nContext, App.tsx + SettingsDialog.tsx retrofit) EXECUTED — PLAT-03's architecture half satisfied. 11-04-PLAN.md (i18n retrofit of the remaining 13 components + lib/errors.ts) drafted, still in review/blocked per its own re-review note as of this update.
**Phase:** 11 of 11 — Platform Polish (Signing, Localization, Theme)
**Plan:** 3 of 4 drafted plans executed (11-01, 11-02, 11-03 done; 11-04 drafted, in review)
**Status:** Phase in progress — 11-04 remaining
**Progress:** [██████████] 98%

## Performance Metrics

- Phases complete: 10/11
- Requirements delivered: 46/47 (PLAT-03 delivered this plan; Phase 11 verification pending overall phase close)

**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 06 P01 | 35m | 2 tasks | 13 files |
| Phase 6 P02 | 30m | 3 tasks | 5 files |
| Phase 06 P04 | ~8m | 2 tasks | 3 files |
| Phase 07 P01 | resumed | 3 tasks | 13 files |
| Phase 07 P02 | single session | 3 tasks | 16 files |
| Phase 07 P03 | single session | 3 tasks | 21 files |
| Phase 07 P04 | single session | 3 tasks | 12 files |
| Phase 07 P05 | single session | 3 tasks | 26 files |
| Phase 08 P01 | 1 session | 3 tasks | 30 files |
| Phase 08 P03 | 1 session | 2 tasks | 18 files |
| Phase 10 P01 | 55min | 3 tasks | 4 files |
| Phase 11 P01 | 40min | 3 tasks | 18 files |
| Phase 11 P02 | 50min | 3 tasks | 7 files |
| Phase 11 P03 | ~25min | 2 tasks | 18 files |

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
- [Phase 02]: 02-02: D2-05 corrected — delete_notes removes Note rows ONLY; UserMark/BlockRange highlights are durable and survive a Note's deletion (only genuinely orphaned rows are swept by trim on save), matching JWLManager.py:3666 exactly
- [Phase 02]: 02-02: dry_run_delete_notes computes SEMANTIC per-table added/overwritten/deleted from before/after primary-key-set snapshots (never raw changes()), run inside a never-committed rusqlite::Transaction reusing Plan 01's VACUUM-free trim_sweep; overwritten is a PK-set-intersection simplification, sufficient for the TagMap re-densify's 0-false-deletion requirement
- [Phase 02]: 02-02: NonEmptyNoteIds (serde try_from newtype) makes an empty delete selection unrepresentable at IPC deserialization, before either Tauri command body runs
- [Phase ?]: 05-01: jwlCore mergeDatabase FFI wrapper (merge.rs) reuses Phase 1 load path; MergeUnavailable/MergeFailed typed errors, no crash; real DLL merged synthetic pair
- [Phase ?]: D5-02: merge dry-run uses a CONTENT-signature diff (not PK-set) so in-place UPDATEs count as overwritten; commit promotes via atomic rename-with-replace, never fs::copy
- [Phase ?]: jwlCore dir-pair merge wrote only userData.db (no loose media relocated); media fold-back is an empirically-verified no-op (branch a)
- [Phase ?]: 06-02: five category getters (db/browse.rs) surface the correct identity PK as row.id (Bookmark=BookmarkId, Favorite=TagMapId, Highlight=BlockRangeId, Annotation=LocationId, Playlist=PlaylistItemId), never the join's LocationId
- [Phase ?]: 06-02: one generic list_category(Category) command dispatches all six getters keyed by the ts-rs enum, not six commands nor a translated display string
- [Phase ?]: D7-03 resolved strict Python parity: merge_block_ranges ships standalone, recolor never invokes it
- [Phase ?]: D7-05: reorder reuses redensify_tag_positions staging technique (not Python's two-pass) — identical observable contract, verified via adversarial fixture + idempotent composition test
- [Phase ?]: record_edit.rs reuses db::color::apply_color's Notes branch verbatim for UserMark synthesis; RecordEditor added record_fetch + BrowseRow.text_tag beyond the plan's named surface since BrowseRow never carried editable Note/Annotation content or per-TextTag identity
- [Phase ?]: Fold order is real (D10-01): fold(A,B,C) != fold(A,C,B) by design, proven with a contested-identity fixture
- [Phase ?]: run_fold_chain shared by dry-run and commit; single atomic promote after last step, media folded back every step (D10-04)
- [Phase ?]: 11-01: SettingsProvider lifted above App in main.tsx (own ErrorBanner for save failures); updateSettings fires save_settings from inside the setState functional updater to prove concurrent theme+language writes never drop a field; settings.rs errors get their own SettingsError->ErrorDto mapping, never reusing ArchiveError
- [Phase ?]: 11-02: Windows signing wired via bundle.windows.signCommand, gated on ENABLE_MSI_SIGNING == 'true' in a new release-app.yml (app-v*.*.* tag prefix, distinct from the Python app's bare v*); public GitHub Release publish gated on the identical condition so an unsigned build can never be published; sign.ps1 fails closed (proven by verify-fail-closed.ps1); guard test signing_wiring.rs proven red/green against a live demonstration; actual signature verification remains manual, blocked on Azure credentials not provisioned in this repo
- [Phase ?]: PLAT-03: dependency-free TypeScript catalog + React context (I18nProvider/useI18n), catalogs[locale]?.[key] ?? en[key] fallback; I18nProvider nests inside ThemeProvider, controlled by SettingsProvider's existing language/setLanguage

### Todos

- [DONE] Execute 11-01-PLAN.md (settings/theme tracer: `load_settings`/`save_settings`/`app_version` commands, SettingsProvider, ThemeContext, light theme, SettingsDialog). See 11-01-SUMMARY.md.
- Manually verify the live Tauri app (open settings, flip to light, confirm instant repaint with no reload; restart, confirm persistence; corrupt the settings file, confirm silent degradation) — not exercised in this headless execution environment; automated tests cover the same behaviors at the Rust/React layers.
- [DONE] Execute 11-02-PLAN.md (Windows Authenticode signing wiring via Azure Trusted Signing — inert/fail-closed until credentials are provisioned). See 11-02-SUMMARY.md.
- Provision Azure Trusted Signing service-principal credentials (three secrets + one enable variable) for this repo as an operational step before the signed-release path can go live (blocks the signed/published path only, not CI greenness, which already stays green today with the gate off). Then run the deliberate fail-closed check in docs/signing.md once before trusting any signed build.
- Plan and execute a Phase 11 plan for PLAT-03 (i18n/localization — English complete, other locales deferred per 11-CONTEXT.md). No 11-03-PLAN.md exists yet; this is the one remaining Phase 11 requirement with no plan file.

### Blockers

- None.

## Session Continuity

**Resume file:** None

**Last session:** 2026-08-16T17:06:22.834Z
**Stopped at:** Completed 11-03-PLAN.md
**Next action:** Draft and execute a Phase 11 plan for PLAT-03 (i18n/localization); separately, provision Azure Trusted Signing credentials and run the deliberate fail-closed check per docs/signing.md when ready to publish signed releases.
