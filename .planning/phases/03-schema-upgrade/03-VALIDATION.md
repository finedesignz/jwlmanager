---
phase: 3
slug: schema-upgrade
status: approved
nyquist_compliant: true
wave_0_complete: false
created: 2026-07-20
approved: 2026-07-20
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` (established Phase 1) + `vitest` for any frontend copy changes |
| **Config file** | existing — `app/src-tauri/Cargo.toml`, `app/vitest.config.ts` |
| **Quick run command** | `cd app/src-tauri && cargo test` |
| **Full suite command** | `cd app/src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cd .. && npm test` |
| **Estimated runtime** | ~40s warm |

---

## Sampling Rate

- **After every task commit:** `cd app/src-tauri && cargo test`
- **After every plan wave:** full suite command
- **Before verify:** full suite green + the real-v14 acceptance run (local, env-gated)
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Req | Behavior | Test Type | Automated Command | Status |
|-----|----------|-----------|-------------------|--------|
| SCHEMA-01 | v12/v13/v14/v15/v16 fixtures all open successfully | integration | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-01 | v11 fixture rejected with a typed, actionable error (not a crash) | integration | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-01 | v17 (>16) rejected with a DISTINCT "newer version" error (D3-09) | integration | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-02 | After open, `PRAGMA user_version` == 16 and `session.manifest.schema_version` == 16 | integration | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-02 | Upgrade is transactional — an induced mid-upgrade failure leaves the DB at its ORIGINAL version, never half-migrated (D3-03) | unit | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-02 | Upgrade failure surfaces as a typed error — NEVER silently swallowed (D3-02, the Python `except: pass` defect) | unit | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-02 | Already-v16 upgrade is a no-op returning success (idempotent, D3-05) | unit | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-02 | Already-has-Specialty/Edition-but-v<16 upgrades correctly instead of aborting (D3-04) | unit | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-02 | Source archive on disk is byte-identical after an upgrade-on-open (D3-06) | integration | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-02 | An upgraded v14 archive saves with manifest `schemaVersion: 16` — never claiming 14 while holding a v16 DB (D3-07) | integration | `cargo test --test schema_upgrade_tests` | ⬜ pending |
| SCHEMA-01/02 | Upgraded archive still accepted by the Python app's `check_validity` (ARCH-02 oracle extended) | differential | `cargo test --test differential -- --ignored` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Extend the existing `res/blank`-seeded fixture generator to emit an arbitrary `user_version` **and** the corresponding older-schema shape (drop `Specialty`/`Edition`, drop `IX_Location_Media`) — blocks every version test
- [ ] v11 and v17 out-of-range fixtures for the reject paths

*Reuses Phase 1's harness; no new framework.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| A REAL v14 archive opens, upgrades, saves, and is accepted by the Python app | SCHEMA-01/02 (the reason this phase jumped the queue) | Real archives are never committed (GDPR Art. 9, D-06/D-07) | `cargo run --example roundtrip -- "<real v14 .jwlibrary>" "<out.jwlibrary>"` then `python -c "import sys;sys.path.insert(0,'.');import JWLManager;print(JWLManager.Window.check_validity(None, r'<out>'))"` |
| Upgraded archive opens in real JW Library | SCHEMA-02 | Requires the vendor app | Import the round-tripped v14→v16 output into JW Library (take a fresh backup first — restore replaces device data) |

---

## Known Coverage Limit (recorded, not hidden)

**v12 and v13 upgrade correctness is verified only against synthetic fixtures.** No v12/v13 archive exists in the owner's library (survey: 19×v14, 13×v16) or anywhere in this repo, and nothing in the Python source confirms v12/v13 share exactly the v14 delta. The implementation applies the same single transformation the Python app applies to any `user_version < 16` — so behavior matches the proven app — but this phase must NOT claim real-data-verified support for v12/v13. Success criteria language and any user-facing copy should reflect verified support for v14–v16 and best-effort, same-as-Python behavior for v12–v13.

---

## Validation Sign-Off

- [x] All tasks have automated verify or Wave 0 dependencies
- [x] Sampling continuity maintained
- [x] Wave 0 covers the fixture-generator extension
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true`

**Approval:** approved 2026-07-20 (strategy sign-off; `wave_0_complete` flips when the fixture generator extension lands).
</content>
