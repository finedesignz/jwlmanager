# Phase 1: Open, View, Save (Foundation Slice) - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-16
**Phase:** 1-Open, View, Save (Foundation Slice)
**Areas discussed:** Repository layout, Working-copy & save strategy, Fixture archives, Frontend stack, jwlCore loading depth, Error surfacing, CI scope
**Mode:** `--auto` — all gray areas auto-selected, recommended default chosen for each question without user prompting. Every choice below is reviewable and overridable before planning.

---

## Repository Layout

| Option | Description | Selected |
|--------|-------------|----------|
| New `app/` subdir, Python untouched at root | Tauri app coexists; Python stays as the differential oracle | ✓ |
| Replace Python at root now | Clean tree, but destroys the comparison oracle mid-rewrite | |
| Separate repo | Clean separation, but cross-repo differential testing is expensive | |

**Choice:** New `app/` subdir (D-01).
**Notes:** Decided by a fact found during scouting — this repo is a fork of `erykjj/jwlmanager` with `upstream` still tracked. Keeping the Python tree unmodified keeps upstream merges clean *and* keeps the ARCH-02 oracle one checkout away.

| Option | Description | Selected |
|--------|-------------|----------|
| Reference `libs/` in place | One copy, one hash | ✓ |
| Vendor a copy into `app/` | Independent build tree, but two binaries drift | |

**Choice:** Reference `libs/` (D-02).

---

## Working-Copy & Save Strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Temp-dir extraction, source read-only | Source archive is never the write target | ✓ |
| In-memory archive | Fast, but playlist media makes size unbounded | |
| Mutate in place | Simplest; a crash corrupts the user's data | |

**Choice:** Temp-dir extraction (D-03).
**Notes:** Direct application of Core Value. Also matches the Python app's model, which keeps behavior comparable.

| Option | Description | Selected |
|--------|-------------|----------|
| Write temp then atomic rename | Power loss leaves the old archive intact | ✓ |
| Write in place | A partial write destroys a good archive | |

**Choice:** Atomic rename (D-04).

| Option | Description | Selected |
|--------|-------------|----------|
| Session follows the new file | Standard desktop save-as semantics | ✓ |
| Session stays on the original | Surprising; diverges from every other app | |

**Choice:** Follow the new file (D-05).

---

## Fixture Archives

| Option | Description | Selected |
|--------|-------------|----------|
| Generate synthetic fixtures | Deterministic, diffable, no personal data | ✓ |
| Commit a scrubbed real archive | Realistic, but publishes special-category data permanently | |
| No committed fixtures; local only | CI can't test anything | |

**Choice:** Generate synthetic (D-06).
**Notes:** This is the GDPR Art. 9 bright line made concrete. Worth recording *why* scrubbing was rejected rather than merely not chosen: the note structure and publication references are themselves the religious-affiliation signal — removing names does not de-identify the archive. And git history makes the mistake permanent.

| Option | Description | Selected |
|--------|-------------|----------|
| Env-var-gated local test, skipped in CI | Keeps the highest-value test without committing the archive | ✓ |
| No real-archive testing at all | Loses the owner's actual v12.1.0 archives as evidence | |

**Choice:** Env-var-gated (D-07).

| Option | Description | Selected |
|--------|-------------|----------|
| v16 + zip-slip now; v12–15 in Phase 3 | Matches where the schema requirements actually land | ✓ |
| All versions now | Front-loads work belonging to Phase 3 | |

**Choice:** v16 + zip-slip (D-08).

---

## Frontend Stack

| Option | Description | Selected |
|--------|-------------|----------|
| React + TypeScript + Vite | Most mature virtualization ecosystem | ✓ |
| Svelte | Smaller/faster output, thinner virtualization story | |
| SolidJS | Excellent perf, smallest ecosystem | |

**Choice:** React + TS + Vite (D-09).
**Notes:** Weakest-conviction decision in the phase. Chosen on virtualization maturity alone, and explicitly flagged re-openable if the planner argues an equivalent story elsewhere.

| Option | Description | Selected |
|--------|-------------|----------|
| Virtualize with TanStack Virtual | Only approach that survives 9k rows on WebKitGTK | ✓ |
| Paginate | Sidesteps perf, but the Python app doesn't paginate — a parity regression | |
| Render all rows | Fails success criterion 1 on Linux | |

**Choice:** Virtualize (D-10).
**Notes:** The one hard perf criterion in the phase, and platform-specific — must be verified on Linux, not inferred from Windows.

| Option | Description | Selected |
|--------|-------------|----------|
| Rust enum + `ts-rs` generated types | Single source; drift impossible | ✓ |
| Hand-mirrored TS union | Cheap, drifts silently | |
| Strings | Reproduces the existing i18n bug | |

**Choice:** Rust enum + ts-rs (D-11).

---

## jwlCore Loading Depth

| Option | Description | Selected |
|--------|-------------|----------|
| Load + resolve symbols only | Proves the platform/arch risk; leaves merge to Phase 5 | ✓ |
| Full merge call | Pulls Phase 5 forward into the foundation slice | |
| Defer loading to Phase 5 | Leaves the phase goal's named risk unproven | |

**Choice:** Load + resolve (D-12).

| Option | Description | Selected |
|--------|-------------|----------|
| Fix arch-blind loading now | PLAT-01 covers Windows arm64 in this phase | ✓ |
| Fix in Phase 5 with merge | arm64 is the owner's daily platform — too late | |

**Choice:** Fix now (D-13).
**Notes:** The bug is real and located: `jwlcore.py:_platform_lib_name` selects on `sys.platform` alone, which returns `"linux"` on aarch64, so `libjwlCore-arm64.so` never loads.

---

## Error Surfacing

| Option | Description | Selected |
|--------|-------------|----------|
| Typed errors (`thiserror`) → frontend surface | Silent swallowing becomes a compile error | ✓ |
| Stringly-typed errors | Easy, and exactly how the Python app got 29 bare excepts | |

**Choice:** Typed errors (D-14).

| Option | Description | Selected |
|--------|-------------|----------|
| Ban `unwrap` by clippy lint in CI | Enforced, not aspirational | ✓ |
| Ban by convention | Convention does not survive a deadline | |

**Choice:** Clippy lint (D-15).

---

## CI Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Full matrix now (win x64 + win arm64 + linux + macos) | PLAT-01 is a Phase 1 requirement; arm64 needed for D-13 | ✓ |
| Windows-only, expand later | Cross-platform breakage found in Phase 11 is a re-architecture | |

**Choice:** Full matrix (D-16).

| Option | Description | Selected |
|--------|-------------|----------|
| No signing in Phase 1 | Signing is PLAT-02 / Phase 11 | ✓ |
| Wire signing now | Must run during bundling, not post-build — a real design task | |

**Choice:** No signing (D-17).

---

## Claude's Discretion

- Rust crate selection beyond the named ones (`rusqlite`, `zip`, `thiserror`, `libloading`, `ts-rs`)
- Module decomposition inside `app/src-tauri`
- The specific Tauri command surface
- Frontend component structure
- Test-harness organization
- **Re-openable:** the frontend framework choice (D-09)

## Deferred Ideas

- `trim_db` on save (ARCH-04) → Phase 2
- Calling jwlCore merge (MERGE-02) → Phase 5
- Schema acceptance + upgrade (SCHEMA-01/02) → Phase 3
- Remaining five categories (DATA-02..06) → Phase 6
- Dry-run preview (SAFE-01) → Phase 2
- Code signing (PLAT-02) → Phase 11
- Retiring the Python app from the repo root → post-parity milestone

## Open Question Raised for the Planner

Does rendering a Note's location label require resolving a `LocationId` through the bundled `res/resources.db` (335 KB)? If so, that dependency lands in Phase 1 rather than later. Flagged in CONTEXT.md `<code_context>` rather than decided here — it is a factual question about the Python app, answerable by reading it, not a preference.
</content>
