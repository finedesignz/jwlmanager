---
phase: 04-schema-downgrade
plan: 03
subsystem: frontend/v14-export + differential-oracle
tags: [schema-downgrade, v14-export, preview-dialog, differential-oracle, react, vitest]
requires:
  - "04-02: downgrade_dry_run + save_v14_copy Tauri commands + DryRunReport"
  - "02-08: DeletePreviewDialog reusable destructive-confirm surface"
  - "01-06: CommandBar (Open/New/Save/Save As) pending+double-click discipline"
provides:
  - "Save v14-compatible copy… CommandBar action (explicit opt-in, preview-then-write)"
  - "DeletePreviewDialog caller-driven copy (title/summary/confirm props)"
  - "generate_v16_collision_fixture: full .jwlibrary collision archive for oracles"
  - "downgrade differential oracle (Python check_validity + A2 normalized equivalence)"
affects:
  - "Closes Phase 4 — user-facing v14 export + cross-implementation verification"
tech-stack:
  added: []
  patterns:
    - "preview-then-write: dry-run first, reused dialog is the confirmation, write only on Confirm"
    - "normalized-equivalence oracle: compare surviving v14-key + dependent fan-in, never literal ids, never byte-diff"
    - "replicate Python closure SQL verbatim when it is not headlessly callable"
key-files:
  created: []
  modified:
    - app/src/components/DeletePreviewDialog.tsx
    - app/src/components/CommandBar.tsx
    - app/src/components/CommandBar.test.tsx
    - app/src-tauri/tests/differential.rs
    - app/src-tauri/tests/common/mod.rs
decisions:
  - "Hosted the v14 preview state inside CommandBar (not App.tsx), mirroring how NotesList hosts the delete preview — App.tsx needed no change"
  - "Normalized-equivalence leg replicates downgrade_schema's SQL verbatim (the closure is not headlessly callable) and runs on stdlib sqlite3 only, so it executes in CI/this env without PySide6"
  - "check_validity downgrade oracle kept #[ignore]d as a RECORDED MANUAL GATE (PySide6 stack required)"
metrics:
  duration: ~35m
  completed: 2026-07-22
---

# Phase 4 Plan 03: v14-Compatible Export UI + Downgrade Differential Oracle Summary

Explicit "Save v14-compatible copy…" action with a merge preview (reusing Phase 2's `DeletePreviewDialog`), plus a v16→v14 downgrade differential oracle proving the Rust downgrade is Python-`check_validity`-accepted and normalized-equivalent to Python's own `downgrade_schema`.

## What shipped

**Task 1 — DeletePreviewDialog generalized (caller-driven copy).** Added optional `title` / `ariaLabel` / `summary` (ReactNode) / `confirmLabel` / `confirmPendingLabel` props defaulting to the existing Notes-delete strings. Pure copy-generalization: no behavior change, existing delete tests pass unchanged. No component fork.

**Task 2 — "Save v14-compatible copy…" CommandBar action.** New secondary toolbar button, separate from Save/Save As, disabled when no archive is open. Preview-then-write flow: `save()` target → `invoke("downgrade_dry_run")` → reused `DeletePreviewDialog` framing `report.deleted["Location"]` as "N Locations will be merged for v14 compatibility". Only Confirm calls `invoke("save_v14_copy", { path })` then `onSaved()`; Cancel/dismiss write nothing. Reuses the existing `busyRef` double-click guard and dismissed-dialog-is-clean-cancel pattern. Preview state hosted in CommandBar (App.tsx unchanged — mirrors NotesList's delete-preview hosting).

**Task 3 — downgrade differential oracle.**
- `python_app_opens_downgraded_v14_archive` (`#[ignore]`d): `save_v14_copy` on the collision fixture → asserts extracted `userData.db` PRAGMA `user_version == 14` → `run_python_check_validity` must accept it. RECORDED MANUAL GATE.
- `rust_downgrade_matches_python_downgrade_normalized` (runs by default, python3-gated): A2 normalized equivalence — Rust `downgrade_to_v14` vs the app's `downgrade_schema` merge (replicated verbatim, stdlib sqlite3) on the same collision fixture. Compares `NORMALIZED_STATE_SQL` (surviving v14-key + total dependent fan-in per key), never literal survivor ids (Rust keeps lowest id, Python keeps `ids[0]`), never byte-diff.
- `generate_v16_collision_fixture`: full `.jwlibrary` archive variant sharing the collision graph with `generate_v16_collision_db` (extracted `populate_collision_graph`).

## Verification results

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean (only pre-existing ts-rs serde-parse notes, not clippy warnings).
- `cargo test` (full workspace): all binaries green, 0 failed. 5 ignored total = the 3 PySide6 differential oracles (incl. the new downgrade one) + real-archive env-gated leg's ignored member. The new normalized-equivalence oracle **ran and passed**.
- `cargo test --test differential -- --list`: shows all 5 tests incl. `python_app_opens_downgraded_v14_archive` + `rust_downgrade_matches_python_downgrade_normalized`.
- `npm run build`: clean (tsc + vite).
- `npm test` (vitest): 5 files, 37 tests passed — incl. 4 DeletePreviewDialog + 14 CommandBar (6 new: dry-run+preview, confirm, cancel, dismiss, disabled).

## Manual-gate status (honest)

- **Rust/Python normalized equivalence (A2):** VERIFIED PASSING 2026-07-22 in this environment (Python 3.13.3, stdlib sqlite3). Real cross-implementation run, not asserted.
- **Python `check_validity` accepts Rust-downgraded v14:** NOT-YET-VERIFIED in this environment. PySide6 is not installed here (`python3 -c "import PySide6"` → ModuleNotFoundError), so `JWLManager.check_validity` cannot be exercised. Test compiles, is discoverable, and is `#[ignore]`d with a NOT-YET-VERIFIED reason string. Flip to VERIFIED PASSING by running `cargo test --test differential -- --ignored` locally with `res/requirements.txt` installed + the root-staged jwlCore/sqlite3 DLLs.

## Deviations from Plan

- **App.tsx listed in files_modified but not changed.** The plan allowed hosting the preview in CommandBar OR App.tsx ("inspect the current host and follow that pattern"). The existing delete preview is hosted inside its owning component (NotesList), so the v14 preview lives in CommandBar and App.tsx needed no edit. Not a functional deviation.
- **Normalized-equivalence via verbatim SQL replication (anticipated by the plan).** Python's `downgrade_schema` is a nested closure inside a Qt method operating on global `TMP_PATH` and is not headlessly callable; per the plan's explicit fallback, its merge SQL is replicated verbatim (stdlib sqlite3, no PySide6/jwlCore) — which made the equivalence leg runnable in CI/this env rather than a silent skip.

## Self-Check: PASSED
- `app/src/components/DeletePreviewDialog.tsx` — FOUND (props generalized)
- `app/src/components/CommandBar.tsx` — FOUND (saveV14 + save_v14_copy wiring)
- `app/src-tauri/tests/differential.rs` — FOUND (downgrade oracle + normalized equivalence)
- Commits: 9d5fb7bc (Task 1), d6203138 (Task 2), bdf84195 (Task 3) — all present.
