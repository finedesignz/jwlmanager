---
phase: 2
slug: safe-delete
status: approved
nyquist_compliant: true
wave_0_complete: false
created: 2026-07-21
approved: 2026-07-21
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` + `vitest` (established). |
| **Quick run** | `cd app/src-tauri && cargo test` |
| **Full suite** | `cd app/src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cd .. && npm test` |
| **Runtime** | ~40s warm |

## Sampling Rate
- After every task commit: `cargo test`
- Before verify: full suite green + differential oracle (delete-then-save accepted by Python)
- Max latency: 120s

## Per-Task Verification Map

| Req | Behavior | Test | Command | Status |
|-----|----------|------|---------|--------|
| EDIT-01 | Deleting selected Notes removes exactly those rows (parameterized) | integration | `cargo test --test delete_tests` | ⬜ |
| ARCH-04 | Save runs trim_db: empty Notes/InputField swept, orphan TagMap/UserMark/BlockRange/Playlist*/Location removed, tag positions re-densified via ROW_NUMBER, Location.Title="" where NULL, VACUUM | integration | `cargo test --test trim_tests` | ⬜ |
| ARCH-04 | TagMap re-densify preserves all non-orphan mappings with contiguous 0-based positions per TagId | unit | `cargo test --test trim_tests` | ⬜ |
| SAFE-01 | Dry-run returns a DryRunReport (per-table deleted counts incl. orphan cascade) WITHOUT mutating the working copy (DB byte-identical after dry-run) | integration | `cargo test --test dryrun_tests` | ⬜ |
| SAFE-02 | Every delete/trim statement is parameterized or static DDL; no format!-built value SQL (grep assert) | source | `cargo test --test delete_tests` + grep | ⬜ |
| SAFE-03 | An empty selection cannot reach the delete path — NonEmptyNoteIds rejects `[]` at IPC deserialization (typed), before any DB access | unit | `cargo test --test delete_tests` | ⬜ |
| SAFE-04 | A failure induced mid-delete/mid-trim leaves the working-copy DB byte-identical to pre-operation (transaction rolled back) | integration | `cargo test --test delete_tests` / `trim_tests` | ⬜ |
| QA-02 | Round-trip semantic equivalence: fixture with multi-table orphans → delete → save(trim) → reopen → normalized-table state equals expected; NEVER byte equality | integration | `cargo test --test delete_roundtrip_tests` | ⬜ |
| ARCH-04/EDIT-01 | A deleted-then-saved archive is accepted by the Python app check_validity | differential | `cargo test --test differential -- --ignored` | ⬜ |

*⬜ pending · ✅ green · ❌ red*

## Wave 0 Requirements
- [ ] **VERIFY TagMap column order** (RESEARCH flagged): the `INSERT INTO TagMap SELECT * FROM TagMapNew` re-densify depends on TagMapNew's column list exactly matching TagMap's. Add a test/assert that reads `PRAGMA table_info(TagMap)` and confirms the re-densify column list matches — OR make the INSERT explicit-column instead of `SELECT *` (safer; recommended). Blocks the trim implementation.
- [ ] **VERIFY window-function support** in the bundled SQLite (ROW_NUMBER OVER PARTITION) — a one-line test.
- [ ] Fixture: Notes with owned UserMark+BlockRange, a TagMap entry, and a Location referenced ONLY by the deleted Note — so the sweep is exercised across tables.

## Manual-Only Verifications
| Behavior | Why | Instructions |
|----------|-----|--------------|
| Real v14 archive: delete a Note, save, confirm trimmed output still accepted by Python + smaller/cleaner than an untrimmed save | Real data; not committed | `examples/roundtrip` variant + Python check_validity |
| Deleted archive imports into real JW Library | Vendor app | owner import (fresh backup first) |

## Validation Sign-Off
- [x] All tasks have automated verify or Wave 0 deps
- [x] Sampling continuity maintained
- [x] Wave 0 covers the TagMap-column-order + window-fn verification
- [x] Feedback latency < 120s
- [x] nyquist_compliant: true

**Approval:** approved 2026-07-21 (strategy sign-off; wave_0_complete flips when the fixture + column-order verification land).
</content>
