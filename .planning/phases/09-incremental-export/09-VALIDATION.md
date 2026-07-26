---
phase: 9
slug: incremental-export
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-26
---

# Phase 9 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Generated from `09-RESEARCH.md` `## Validation Architecture`, refined against the
> committed plan set (09-01..09-04) after the plan-checker flagged this artifact missing.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` / `cargo test` (backend) + vitest 2.x (frontend) |
| **Config file** | none for Rust — inline `#[cfg(test)] mod tests`, matching `export.rs:712-728`; `app/vite.config.ts` for the frontend |
| **Quick run command** | `cd app/src-tauri && cargo test --jobs 2 db::io::diff` |
| **Full suite command** | `cd app/src-tauri && cargo test --jobs 2` then `cd app && npx vitest run` |
| **Estimated runtime** | ~180 s (Rust) + ~15 s (vitest) |

**Host constraint (load-bearing):** `--jobs 2` is MANDATORY. Default parallelism OOMs the
linker on this host (`os error 1455`, paging file too small). This is an environment limit,
not a code defect — never "fix" it by changing code, never drop the flag.

Never use watch mode: `npx vitest run`, never bare `vitest`.

---

## Sampling Rate

- **After every task commit:** `cargo test --jobs 2 db::io::diff` — fast, isolated to the new module
- **After every wave:** full Rust suite + `npx vitest run` — catches any Phase 8 regression from the
  new `lib.rs` commands
- **Before `/gsd-verify-work`:** full suite green, zero failed
- **Max feedback latency:** 180 seconds

---

## Per-Task Verification Map

| Req / Property | Behavior | Test Type | Automated Command | Status |
|----------------|----------|-----------|-------------------|--------|
| IO-04 c1 | Diff selects added + modified rows from prior-file vs. live | unit | `cargo test --jobs 2 diff_category` | ⬜ pending |
| IO-04 c1 | No prior file ⇒ export everything (D9-05) | integration | `cargo test --jobs 2 incremental_no_prior_file_exports_all` | ⬜ pending |
| IO-04 c2 | Timestamp-only change is EXCLUDED | unit | `cargo test --jobs 2 timestamp_only_change_excluded` | ⬜ pending |
| IO-04 c1 | Non-identity content change IS included | unit | `cargo test --jobs 2 content_change_included` | ⬜ pending |
| IO-04 c1 | Added row (absent from prior) is included | unit | `cargo test --jobs 2 added_row_included` | ⬜ pending |
| IO-04 c1 | Deleted candidate counted in summary, NEVER written to file | unit | `cargo test --jobs 2 deleted_candidate_not_exported` | ⬜ pending |
| D9-04 | Convergence: export → re-import → export settles to zero added/modified | integration | `cargo test --jobs 2 incremental_export_converges` | ⬜ pending |
| D9-04 | Malformed prior file aborts with a typed error — never a silent full-export fallback | unit | `cargo test --jobs 2 malformed_prior_file_aborts` | ⬜ pending |
| Pitfall 2 | Annotations sharing a LocationId but differing TextTag diff independently | unit | `cargo test --jobs 2 annotations_composite_identity` | ⬜ pending |
| **Over-export invariant** | The exported set is `{live hash ∉ prior hash set}` and NEVER consults identity — an identity failure can only over-export, never under-export | integration | `cargo test --jobs 2 identity_failure_biases_to_over_export` | ⬜ pending |
| CRLF | A CRLF prior file (real Windows Python export) diffs identically to its LF twin | unit | `cargo test --jobs 2 crlf_prior_file_diffs_identically` | ⬜ pending |
| Disclosed limit | Favorites `modified` is structurally always 0 (no mutable wire field) | unit | `cargo test --jobs 2 favorites_modified_always_zero` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

The **over-export invariant** row is the phase's single most important test. Every other
failure mode degrades the user's convenience; under-export silently breaks the user's belief
that they exported a change they made.

---

## Wave 0 Requirements

- [ ] `app/src-tauri/src/db/io/diff.rs` — new module with inline `#[cfg(test)] mod tests`
      (matches the `export.rs`/`import.rs` convention; no separate `tests/` file needed for
      pure-function unit tests)
- [ ] Integration coverage for the new Tauri commands against a synthetic fixture pair
      (prior `.txt` + live archive), following the shipped `app/src-tauri/tests/io_roundtrip_tests.rs`
      convention
- [ ] Synthetic prior/current fixture PAIRS derived from `tests/fixtures/wire/*_golden.txt` —
      one per category, covering unchanged / added / content-modified / timestamp-only-changed
      (Notes only) / deleted rows
- [ ] Framework install: none — `cargo test` and vitest are already configured project-wide

---

## Manual-Only Verifications

| Behavior | Why Manual | Test Instructions |
|----------|-----------|-------------------|
| An incremental `.txt` still imports into the real Python app | Third-party app; the automated oracle covers export bytes, not the Python's own import path | Produce an incremental export, import it with `JWLManager.py`, confirm the records land and no parse error occurs |

*Everything else in this phase has automated verification — it is read-only on the archive.*

---

## Validation Sign-Off

- [ ] All tasks have an `<automated>` verify or a Wave 0 dependency
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 180 s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
