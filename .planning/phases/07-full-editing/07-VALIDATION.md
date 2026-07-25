---
phase: 7
slug: full-editing
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-24
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` (backend) + vitest 2.x (frontend) |
| **Config file** | `app/src-tauri/Cargo.toml`, `app/vite.config.ts` |
| **Quick run command** | `cd app/src-tauri && cargo test --jobs 2 <op>_tests` |
| **Full suite command** | `cd app/src-tauri && cargo test --jobs 2` then `cd app && npx vitest run` |
| **Estimated runtime** | ~180 seconds (Rust, cold ~7 min) + ~15 seconds (vitest) |

**Host constraint (load-bearing):** `--jobs 2` is MANDATORY. Default parallelism OOMs the
linker on this host (`os error 1455`, paging file too small). This is an environment limit,
not a code defect — never "fix" it by changing code.

Differential (oracle) tests are `#[ignore]`d and run explicitly:
`cargo test --jobs 2 --test differential -- --ignored` (needs Python 3.13 + PySide6 on PATH).

---

## Sampling Rate

- **After every task commit:** Run the op-group's Rust test target (quick command)
- **After every plan wave:** Run the full Rust suite + vitest
- **Before `/gsd-verify-work`:** Full suite green, zero failed
- **Max feedback latency:** 180 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| *(populated by gsd-planner from PLAN.md tasks)* | | | | | | | | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- Existing infrastructure covers all phase requirements — `app/src-tauri/tests/common/mod.rs`
  (multi-category v16 fixture seeding, `:540-581`) and the `delete_tests.rs` per-op template
  already exist from Phase 2/6. Each op group extends the fixture; no new framework install.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Mask irreversible-confirm friction | EDIT-06 | UI acknowledgement flow is human-judged (typed confirm must be non-bypassable by Enter) | Open a fixture archive, invoke Mask, confirm the dialog requires explicit typed input and shows dry-run row counts before enabling the destructive action |
| Real JW Library opens a Phase-7-edited archive | EDIT-02..07 | Third-party app, no automatable oracle | Save an archive after each op group, open in JW Library, confirm highlights/tags/favorites render |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags (`vitest run`, never `vitest --watch`)
- [ ] Feedback latency < 180s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
