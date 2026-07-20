# Phase 1: Open, View, Save (Foundation Slice) - Context

**Gathered:** 2026-07-16
**Status:** Ready for planning

<domain>
## Phase Boundary

Prove the riskiest integration end-to-end before adding breadth: a user opens a real `.jwlibrary` archive, sees their real Notes in a responsive list, and saves it back to a file that JW Library and the existing Python app both still open.

**In scope:** zip envelope read/write, `manifest.json` byte-compatible serialization, `userData.db` open + Notes query, virtualized Notes list, new/save/save-as, zip-slip rejection, arch-aware `jwlCore` library loading (load + symbol resolution only), fixture archives, CI on every push, actionable error surfacing.

**Out of scope (own phases):** any destructive edit or delete (Phase 2), `trim_db` on save (Phase 2), schema upgrade/downgrade (Phases 3–4), calling `jwlCore` merge (Phase 5), the other five categories (Phase 6), import/export (Phase 8), signing/localization/theme (Phase 11).

**Requirements:** ARCH-01, ARCH-02, ARCH-03, ARCH-05, ARCH-06, ARCH-07, DATA-01, DATA-08, SAFE-05, QA-01, QA-03, PLAT-01

</domain>

<decisions>
## Implementation Decisions

All decisions below were auto-selected in `--auto` mode using the recommended default for each gray area. Each is logged with its rationale so it can be audited and overridden before planning.

### Repository Layout

- **D-01:** The Tauri app lives in a new `app/` subdirectory of this repo. The Python app stays untouched at the repo root until parity is reached, then is removed in a later milestone.
  `[auto] Repo layout — Q: "Where does the Tauri app live relative to the Python app?" → Selected: "New app/ subdir, Python untouched at root" (recommended default)`
  **Rationale:** This repo is a fork of `erykjj/jwlmanager` with `upstream` still tracked. Keeping the Python tree unmodified keeps upstream merges clean and — more importantly — keeps the working Python app available as the differential-testing oracle for every phase. ARCH-02 requires that the Python app opens what the Tauri app saves; that test is only cheap if both live in one checkout.

- **D-02:** The existing `libs/` native binaries are the single source of `jwlCore`. The Tauri build references them from `libs/`; it does not vendor a second copy.
  `[auto] Repo layout — Q: "Vendor jwlCore into app/, or reference libs/?" → Selected: "Reference libs/" (recommended default)`
  **Rationale:** Two copies of a closed-source binary drift. One copy, one hash.

### Working-Copy & Save Strategy

- **D-03:** Opening an archive extracts it to a per-session temp directory; `userData.db` is opened from there with `rusqlite` on disk. The user's original file is opened read-only and never mutated in place.
  `[auto] Working copy — Q: "Temp-dir extraction, or in-memory archive?" → Selected: "Temp-dir extraction, source read-only" (recommended default)`
  **Rationale:** Directly serves Core Value. The source archive cannot be corrupted by a crash mid-session if it is never the write target. It also matches the Python app's model, keeping behavior comparable. In-memory was rejected: media files in playlists make archive size unbounded.

- **D-04:** Save writes the rebuilt zip to a sibling temp file, then atomically renames over the target. A partial write must never replace a good archive.
  `[auto] Working copy — Q: "Write-in-place or write-temp-then-rename?" → Selected: "Write temp then atomic rename" (recommended default)`
  **Rationale:** Core Value again. Atomic rename is the only way a power loss during save leaves the old archive intact.

- **D-05:** Save-as (ARCH-07) writes to the chosen path and leaves the working copy's identity pointed at the *new* path, matching normal desktop save-as semantics. The original file on disk is left exactly as it was.
  `[auto] Working copy — Q: "After save-as, does the session follow the new file or stay on the old?" → Selected: "Follow the new file" (recommended default)`

### Fixture Archives (QA-01) — data-protection critical

- **D-06:** Fixture archives are **synthetic and generated programmatically** by a committed test helper that builds a valid archive from scratch. No real user archive is ever committed to this repository.
  `[auto] Fixtures — Q: "Commit a scrubbed real archive, or generate synthetic fixtures?" → Selected: "Generate synthetic fixtures" (recommended default)`
  **Rationale:** This is the GDPR Art. 9 bright line from PROJECT.md applied concretely. A `.jwlibrary` archive is evidence of religious practice — special-category personal data. Committing one, even the owner's own, publishes special-category data to a public fork permanently and irrevocably (git history). "Scrubbing" is not a defense: the note *structure and publication references* are themselves the religious-affiliation signal, not just the names. Generation also gives deterministic, diffable fixtures, which scrubbing does not.

- **D-07:** The owner's real archives stay outside the repo. An env var (e.g. `JWLM_REAL_ARCHIVE`) points local-only manual smoke tests at one. CI never sees a real archive; those tests skip when the var is unset.
  `[auto] Fixtures — Q: "How do we test against a real archive without committing one?" → Selected: "Env-var-gated local-only test, skipped in CI" (recommended default)`
  **Rationale:** Preserves the highest-value test (the owner's actual v12.1.0-era archives) without the archive entering version control. Known real path for local use: `C:\Users\artic\OneDrive\_JW\JW Library\JWL Manager\`.

- **D-08:** Phase 1 commits one v16 synthetic fixture plus one crafted zip-slip fixture (for ARCH-05). The v12–v15 fixture set is generated in Phase 3, where the schema-version requirements land.
  `[auto] Fixtures — Q: "Generate all schema versions now, or just v16?" → Selected: "v16 + zip-slip now; v12–15 in Phase 3" (recommended default)`

### Frontend Stack

- **D-09:** Vite + React + TypeScript for the frontend.
  `[auto] Frontend stack — Q: "Which frontend framework?" → Selected: "React + TypeScript + Vite" (recommended default)`
  **Rationale:** Widest maturity for the one thing this phase actually needs — virtualized lists at 9,000+ rows. Not a strong conviction; the planner may substitute Svelte or Solid if it argues the virtualization story is equivalent.

- **D-10:** The Notes list is virtualized with TanStack Virtual. Rendering 9,000 DOM rows is forbidden, not merely discouraged.
  `[auto] Frontend stack — Q: "Virtualize the list, or paginate?" → Selected: "Virtualize (TanStack Virtual)" (recommended default)`
  **Rationale:** PROJECT.md constraint: Linux WebKitGTK has documented performance collapse on DOM-heavy grids. This is the phase's one hard perf criterion (success criterion 1) and it is platform-specific — it must be verified on Linux in CI or manually, not assumed from Windows behavior.

- **D-11:** Category identity (DATA-08) is a Rust enum, single-sourced, with TypeScript types generated from it via `ts-rs` at test time. Translated display strings never participate in control flow.
  `[auto] Frontend stack — Q: "How are stable category enums shared with the frontend?" → Selected: "Rust enum + ts-rs generated TS types" (recommended default)`
  **Rationale:** Fixes the existing app's latent i18n bug (`if category == _('Notes')`) at the root rather than reproducing it. Generation means the two sides cannot silently drift.

### jwlCore Loading Depth

- **D-12:** Phase 1 loads the `jwlCore` library, resolves its symbols, and reports success/failure — it does **not** call merge. Merge is Phase 5.
  `[auto] jwlCore depth — Q: "How far do we exercise jwlCore in Phase 1?" → Selected: "Load + resolve symbols only" (recommended default)`
  **Rationale:** The phase goal names jwlCore loading as one of the three riskiest integrations to prove early. Loading is the part that fails on a platform/arch mismatch; the merge call is a separate risk that belongs with the merge phase.

- **D-13:** Library selection is by OS **and CPU architecture**, fixing the arch-blind bug found in research. Selection logic is unit-tested against the binaries that exist by name, and returns a typed "no binary for this OS+arch" error for combinations that have none (see D-13a).
  `[auto] jwlCore depth — Q: "Fix the arch-blind loading bug now or in Phase 5?" → Selected: "Now, in Phase 1" (recommended default)`
  **Rationale:** `jwlcore.py:_platform_lib_name` selects on `sys.platform` alone, so `sys.platform` returns "linux" on aarch64 and `libjwlCore-arm64.so` never loads. The fix is real and valuable for the Linux x86_64-vs-aarch64 split that genuinely ships two binaries. On targets with no binary it converts a silent failure into a clear, tested error — strictly better, even where it cannot make a missing binary appear.

- **D-13a (AMENDED 2026-07-19, owner decision — "ship both arm and x"):** **Windows ships BOTH a native x64 build and a native arm64 build.** Facts established during research: there is no native Windows arm64 `jwlCore` binary — `libs/` contains only `jwlCore-amd64.dll` (x64), `.github/workflows/jwlCore.config`'s `win32` rule has no arm64 entry, and the source is not in this repo, so one cannot be built here. A native arm64 process cannot load an x64 DLL (hard OS boundary). The owner's own `JWLManager_v12.1.0-arm64` build is, by PE-header inspection, **entirely x64** (`JWLManager.exe`, `jwlCore-amd64.dll`, `python313.dll` all AMD64), running on arm64 hardware under Windows 11 x64 emulation (Prism).

  **Consequence, accepted knowingly:** the native arm64 build delivers the full open/view/save/browse surface, but `jwlCore` **cannot load on it**, so merge (Phase 5) and native schema-upgrade-via-jwlCore are **unavailable on the native arm64 build**. For those operations the owner uses the x64 build (natively on x64 machines, or under emulation on arm64). The x64 build remains the full-capability build; the native arm64 build is a faster/smaller open-view-save build that trades away jwlCore-dependent features until an upstream Windows arm64 jwlCore binary exists.

  **Phase 1 impact:** D-12 already scopes jwlCore to load+resolve only (no merge call), so this is testable now. The `windows-11-arm` CI job's jwlCore acceptance criterion is **"`check_jwlcore()` returns a first-class, typed `no binary for aarch64-windows` error"** — NOT "jwlCore loads." Every other Phase 1 criterion (open, view, save, save-as, new, zip-slip reject, virtualized Notes) must pass on the arm64 job identically to x64. This makes D-13 (arch-aware selection) genuinely load-bearing on Windows, not just Linux. Supersedes the prior same-numbered "x64 only" draft of this decision.

### Error Surfacing (SAFE-05)

- **D-14:** Rust uses typed errors (`thiserror`) that serialize across the Tauri IPC boundary to a frontend error surface. Every error a user can trigger states what failed and what they can do about it.
  `[auto] Error surfacing — Q: "Typed errors or stringly-typed?" → Selected: "Typed errors via thiserror, serialized to frontend" (recommended default)`
  **Rationale:** The existing app has 29 bare `except:` clauses (CONCERNS.md) — the exact failure mode SAFE-05 exists to prevent. Typed errors make silent swallowing a compile-time impossibility rather than a code-review question.

- **D-15:** No `unwrap()` / `expect()` on any path that touches user archive data. Enforced by a clippy lint in CI, not by convention.
  `[auto] Error surfacing — Q: "Ban unwrap by convention or by lint?" → Selected: "By clippy lint in CI" (recommended default)`

### CI Scope (QA-03)

- **D-16:** GitHub Actions matrix from day one: `windows-latest` (x64), `windows-11-arm` (arm64), `ubuntu-latest`, `macos-latest`. Build + test on every push. Per D-13a, the `windows-11-arm` job asserts the typed "no jwlCore binary for aarch64-windows" error rather than a successful jwlCore load; all non-jwlCore criteria pass identically to x64.
  `[auto] CI scope — Q: "Full platform matrix now, or Windows-only and expand later?" → Selected: "Full matrix now" (recommended default)`
  **Rationale:** PLAT-01 is a Phase 1 requirement, and the arch-aware loading fix (D-13) is untestable without arm64 in the matrix. Since the owner ships a native arm64 build (D-13a), the `windows-11-arm` runner is mandatory, not optional. Research confirmed `windows-11-arm` is GA for public repos (GitHub, 2025-08-07) — verify this repo's visibility supports the free-tier arm64 runner.

- **D-17:** Phase 1 CI runs build + test + clippy + fmt. Code signing is **not** wired up here — that is PLAT-02 in Phase 11.
  `[auto] CI scope — Q: "Add signing to CI now?" → Selected: "No — Phase 11" (recommended default)`
  **Rationale:** Signing must run *during* Tauri bundling (`bundle.windows.signCommand`), not as a post-build pass, so it is a real design task rather than a config line — it earns its own phase slot.

### Claude's Discretion

The planner has latitude on: Rust crate selection beyond the named ones (`rusqlite`, `zip`, `thiserror`, `libloading`, `ts-rs`); module decomposition inside `app/src-tauri`; the specific Tauri command surface; component structure in the frontend; test-harness organization. The frontend framework choice (D-09) is explicitly re-openable if the planner has a stronger case.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### The parity contract (highest priority)
- `.planning/research/FUNCTIONALITY-SPEC.md` — 700 lines, line-cited against the working Python app. The archive format contract lives here: manifest shape and field order, compact `separators=(',',':')` serialization, `hash` = sha256 of the FINAL DB bytes, `schemaVersion` from `PRAGMA user_version`. ARCH-01/02/03 are unimplementable without it.
  **Caveat, stated once so it is not forgotten:** this spec was derived *from* the code it replaces. It is documentation, not an oracle — it encodes the Python app's bugs as requirements. Where it and a real archive disagree, the real archive wins.

### Project-level constraints
- `.planning/PROJECT.md` — Core Value, the three legal bright lines, the Key Decisions table (including the fact that the Tauri rewrite proceeded against a RESHAPE verdict).
- `.planning/REQUIREMENTS.md` — the 47 v1 requirements; §Out of Scope holds the bright lines in requirement form.
- `.planning/ROADMAP.md` §"Phase 1" — success criteria this phase is verified against.

### Existing-code map
- `.planning/codebase/CONCERNS.md` — the defect inventory this rewrite must not reproduce: 29 bare `except:`, f-string SQL in IN-clauses, unvalidated `ZipFile.extractall()` (the zip-slip ARCH-05 fixes), zero tests.
- `.planning/codebase/STACK.md` — the four `jwlCore` binaries in `libs/` and how the Python app loads them.
- `.planning/codebase/ARCHITECTURE.md` — structure of the 4077-line monolith being replaced.

### The reference implementation (read, don't copy)
- `jwlcore.py` (83 lines) — the ctypes bridge the Rust `libloading` binding mirrors. `_platform_lib_name` is where the arch-blind bug (D-13) lives. MIT — safe to learn from.
- `JWLManager.py` — the Python app. Load-bearing sections for this phase: archive open/save, manifest write, Notes query. MIT — safe to learn from.

### Research context (standing risk, not action items)
- `.planning/research/JWLFUSION-COMPARE.md` — hash-verification proving jwlFusion wraps byte-identical `jwlCore` binaries; source is Infiniti Noncommercial — **do not ingest**.
- `.planning/research/FEATURE-IDEAS.md` — upstream demand evidence from 277 issues.
- `.planning/research/ERYKJJ-REPOS.md` — sibling-repo license survey.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `libs/jwlCore-amd64.dll`, `libs/libjwlCore-x86_64.so`, `libs/libjwlCore-arm64.so`, `libs/libjwlCore.dylib` — the prebuilt merge engine, referenced in place (D-02). The single largest piece of this project's hard logic, already built and hash-verified against jwlFusion's copies.
- `jwlcore.py` — a working, minimal FFI reference. The Rust binding is a direct translation, with the arch bug fixed.
- `res/blank`, `res/blank_playlist` — the Python app's empty-archive seeds. Directly relevant to ARCH-06 (create new empty archive); inspect before generating one from scratch.
- `.github/workflows/build_*.yml` — existing per-platform CI. Structure and runner selection are worth borrowing; the Python/PyInstaller steps are not.

### Established Patterns
- Archive = plain zip: `manifest.json` + `userData.db` + loose media files. No custom container format.
- Working schema is v16; `PRAGMA user_version` carries it.
- Manifest `hash` is sha256 over the **final** DB bytes — so it must be computed after every mutation, as the last step before zipping.

### Integration Points
- The Python app is the differential-test oracle: ARCH-02's real proof is that `JWLManager.py` opens what the Tauri app wrote. That check is runnable from this same checkout (D-01).
- `res/resources.db` (335 KB, bundled) backs publication-reference lookups. Phase 1 shows Notes; whether the Tauri app needs this DB to render a Note's location label is a **question for the planner** — if a Note's displayed title requires resolving a `LocationId` through `resources.db`, that dependency lands in this phase.

</code_context>

<specifics>
## Specific Ideas

- The owner uses the Windows arm64 build (`JWLManager_v12.1.0-arm64`) as their daily driver for merges. arm64 is the reference platform, not an afterthought — which is exactly why D-13 (arch-aware loading) and the arm64 CI runner (D-16) are in Phase 1.
- "Test as we go" is the owner's explicit replacement for a retrofitted characterization harness over the Python app. Phase 1 is where that stops being a plan and becomes a habit: if it ships without fixtures and green CI, the strategy has already failed.
- Parity is verified **semantically** — normalized table state — never by byte-diffing. `trim_db` + VACUUM make byte comparison meaningless. `trim_db` itself arrives in Phase 2, but the test *style* is set here.

</specifics>

<deferred>
## Deferred Ideas

- **`trim_db` on save (ARCH-04)** — Phase 2. Phase 1 saves without trimming; a Phase 1 round-trip will therefore not be byte-identical to a Python-app save, and that is expected, not a bug.
- **Calling jwlCore merge (MERGE-02)** — Phase 5. Phase 1 only proves the library loads.
- **Schema version acceptance and upgrade (SCHEMA-01/02)** — Phase 3. Phase 1 assumes v16 fixtures.
- **The other five categories (DATA-02..06)** — Phase 6.
- **Dry-run preview (SAFE-01)** — Phase 2, with the first destructive operation.
- **Code signing (PLAT-02)** — Phase 11.
- **Retiring the Python app from the repo root** — a post-parity milestone, not a phase in this roadmap.

</deferred>

---

*Phase: 1-Open, View, Save (Foundation Slice)*
*Context gathered: 2026-07-16*
</content>
</invoke>
