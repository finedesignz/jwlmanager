# Phase 3: Schema Upgrade - Context

**Gathered:** 2026-07-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Any archive a real user might hand the app (schema v12–16) opens correctly and is normalized to v16 in the working copy. Archives at v11 or earlier are rejected with a clear, actionable message rather than crashing or corrupting.

**In scope:** widening the open-time gate from v16-only to v12–16, the v12–15 → v16 upgrade applied to the working copy, rejection of ≤v11, multi-version fixtures, and round-trip verification that an upgraded archive is still valid.

**Out of scope (own phases):** the v16→v14 **downgrade** on save (Phase 4, SCHEMA-03/04/05 — including the 7-table LocationId remap closure), `trim_db`/VACUUM (Phase 2), delete/dry-run (Phase 2), merge (Phase 5), the other five categories (Phase 6).

**Requirements:** SCHEMA-01, SCHEMA-02

**Why this phase moved ahead of Phase 2:** a survey of the owner's 32 real archives found **19 at v14 and 13 at v16**. Every iPad backup is v14. The Phase 1 v16-only gate (D-13a-era decision, correct on safety grounds) therefore locks the owner out of 59% of their own library. Safe Delete has little value on files that cannot be opened. Phase 2 runs immediately after this one. Both depend only on Phase 1, so nothing in the dependency graph is disturbed.

</domain>

<decisions>
## Implementation Decisions

Auto-selected (`--auto`) using the recommended default for each gray area, with rationale for audit.

### The upgrade itself

- **D3-01:** Port the upgrade from `JWLManager.py:1016-1070` as a **single monolithic v<16 → v16 transformation**, not a chain of per-version steps (12→13→14→15→16). The Python app applies one transformation to any `user_version < 16`, and archives in the wild reach v16 through that same path.
  `[auto] Upgrade shape — Q: "Stepwise per-version chain or single transformation?" → Selected: "Single transformation" (recommended default)`
  **Rationale:** Matches the proven behavior in the app that produced the owner's real archives. A stepwise chain would be inventing intermediate states nothing has ever validated, and would need fixtures for transitions that never occur in practice.

- **D3-02 (defect NOT to port — load-bearing):** The Python `upgrade_schema` wraps the entire `executescript` in `try/except: pass`. **A failed upgrade is silently swallowed and the DB is left at its old version while the caller believes it succeeded.** The Rust port MUST surface failure as a typed error. This is the exact defect class in `.planning/codebase/CONCERNS.md` (29 bare excepts) and a direct SAFE-05 violation.
  `[auto] Error handling — Q: "Mirror Python's silent-failure semantics for compatibility, or fail loudly?" → Selected: "Fail loudly with a typed error" (recommended default)`
  **Rationale:** Silently continuing with a half-upgraded or un-upgraded DB and then *saving* it is precisely how an archive gets corrupted. Bug-for-bug compatibility is not owed here — the Python behavior is a defect, not a format wart.

- **D3-03:** The upgrade runs inside a **transaction** and rolls back cleanly on any failure. The working copy is either fully v16 or untouched at its original version — never half-migrated.
  `[auto] Atomicity — Q: "Transactional upgrade or best-effort script?" → Selected: "Transactional" (recommended default)`
  **Rationale:** `executescript` in Python implicitly commits and offers no rollback on partial failure. Core Value demands all-or-nothing.

- **D3-04:** Handle the **"columns already present but `user_version` < 16"** case explicitly instead of letting it fail. In the Python version, `ALTER TABLE Location ADD COLUMN Specialty` errors if the column exists, which aborts the whole script — and the bare `except` then hides it, leaving the archive unupgraded forever. Detect existing columns via `PRAGMA table_info(Location)` and skip the redundant `ALTER`.
  `[auto] Edge case — Q: "Guard the already-has-columns case?" → Selected: "Yes, detect and skip" (recommended default)`
  **Rationale:** This is a real, reachable state (an archive touched by a partially-completed upgrade). Under the Python app it is silently unfixable; here it must upgrade correctly.

- **D3-05:** Upgrading an archive already at v16 is an explicit **no-op** that returns success (matching the Python early-return at `:1019-1021`). Idempotent.

### Where the upgrade applies

- **D3-06:** The upgrade is applied to the **extracted working copy only** (inside `ArchiveSession`'s temp dir). The user's source file on disk is never modified by opening it — D-03 from Phase 1 stands unchanged.
  `[auto] Scope — Q: "Upgrade the working copy or write back to source?" → Selected: "Working copy only" (recommended default)`
  **Rationale:** Opening a file must never mutate it. The upgrade becomes durable only if the user explicitly saves, and then it goes through the Phase 1 atomic-save path.

- **D3-07:** After upgrade, `ArchiveSession.manifest.schema_version` and the DB's `PRAGMA user_version` both read 16, and the **manifest written on save reflects 16** (the manifest's `schemaVersion` comes from `PRAGMA user_version` per ARCH-03, so this follows automatically — but it must be asserted in a test, because a v14 archive that saves claiming v14 while containing a v16 DB is a corruption vector).

### The gate

- **D3-08:** Replace the Phase 1 v16-only gate with: **accept 12–16, reject ≤11** with a typed, actionable error naming the version found. Keep the same `ErrorDto` surface built in Phase 1 — the error copy changes, the mechanism does not.
  `[auto] Gate — Q: "Widen the existing gate or add a parallel path?" → Selected: "Widen the existing gate" (recommended default)`

- **D3-09:** Reject **>16** as well (an archive from a *newer* JW Library than this app knows) with its own distinct message — "created by a newer version" is a genuinely different user situation from "too old," and silently trying to open it risks misreading an unknown schema. The Python app's `>= 16` early-return would happily accept a v17 and treat it as v16; that is a latent hazard, not a behavior to copy.
  `[auto] Forward compat — Q: "Accept >16 as the Python app does, or reject?" → Selected: "Reject with a distinct message" (recommended default)`

### Fixtures and verification

- **D3-10:** Generate **synthetic v12, v13, v14, v15 fixtures** (plus the existing v16) and a **v11 reject fixture**, all from the committed generator seeded from `res/blank` — same GDPR Art. 9 bright line as Phase 1 (D-06). No real archive is ever committed.
  `[auto] Fixtures — Q: "Synthesize the older versions or capture real ones?" → Selected: "Synthesize" (recommended default)`

- **D3-11:** The owner's **real v14 archives are the acceptance test**, run locally through the existing env-gated path (`JWLM_REAL_ARCHIVE`, D-07) and the `examples/roundtrip` tool — never in CI, never committed. Phase 3 is not done until a real v14 archive opens, upgrades, saves, and is **accepted by the Python app's `check_validity`** (the ARCH-02 oracle, now verified working as of 2026-07-20).
  `[auto] Acceptance — Q: "Fixtures only, or also the owner's real v14 data?" → Selected: "Also real v14, locally" (recommended default)`
  **Rationale:** Fixtures prove the code path; the owner's actual iPad backups prove it against data JW Library really produced. The whole reason this phase jumped the queue is those 19 files.

### Claude's Discretion

Module placement of the upgrade (likely `db/schema.rs` or `archive/upgrade.rs`), how the fixture generator parameterizes version, whether the upgrade SQL is one embedded string or composed statements, and test organization.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### The upgrade source of truth
- `JWLManager.py:1016-1070` — `upgrade_schema`. The exact DDL to port: two `ALTER TABLE Location ADD COLUMN` (Specialty, Edition), a full `Location_new` rebuild with its UNIQUE + three CHECK constraints, `INSERT ... SELECT` copying NULL into the two new columns, `DROP TABLE Location`, rename, three index creations (including the `IX_Location_Media` UNIQUE index over `COALESCE(Specialty,'')`/`COALESCE(Edition,'')`), then `PRAGMA user_version = 16`. **Lines 1071-1074 are the `except: pass` defect — port the DDL, not the error handling.**
- `JWLManager.py:1100`, `:1252`, `:2582` — the three call sites, showing when the Python app upgrades (on load, on save-path, and for playlists).

### Phase 1 foundations this builds on
- `.planning/phases/01-open-view-save-foundation-slice/01-CONTEXT.md` — D-03 (read-only source), D-04 (atomic save), the v16-only gate this phase widens.
- `.planning/phases/01-open-view-save-foundation-slice/01-REVIEWS.md` — finding 2 is exactly why the gate was v16-only; this phase is the sanctioned widening.
- `.planning/phases/01-open-view-save-foundation-slice/VERIFICATION.md` — Phase 1's verified baseline.
- `app/src-tauri/src/session.rs` — `ArchiveSession` (`manifest.schema_version`, `db_path`, `entries`).
- `app/src-tauri/src/archive/mod.rs` — `open_and_validate`, the current v16-only gate.
- `app/src-tauri/src/error.rs` — `ArchiveError` + `ErrorDto`; add the new schema variants here.

### Format contract
- `.planning/research/FUNCTIONALITY-SPEC.md` — the v16↔v14 delta (`Location.Specialty`/`Edition` + `IX_Location_Media`) and the manifest's `schemaVersion` = `PRAGMA user_version` rule.
- `.planning/codebase/CONCERNS.md` — the bare-except inventory D3-02 refuses to reproduce.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `open_and_validate` + `ArchiveSession` (Phase 1) — the upgrade slots in after extraction and before the Notes query, mutating only the temp-dir DB.
- The fixture generator + harness from `01-01` — already seeds from `res/blank`; extend it to emit arbitrary `user_version` rather than writing a second generator.
- `ErrorDto`/`ArchiveError` (Phase 1) — the reject paths reuse this; no new error mechanism.
- `examples/roundtrip.rs` (added 2026-07-20) — already round-trips a real archive; becomes the manual acceptance tool for real v14 files.
- The verified ARCH-02 Python oracle (`tests/differential.rs`) — extend it to assert an upgraded v14 archive is accepted by `check_validity`.

### Established Patterns
- Working copy in a temp dir; source never mutated (D-03).
- Typed errors only; no `unwrap`/`expect` on archive-data paths (clippy `-D warnings`).
- All SQL parameterized. Note the upgrade DDL is static SQL with no user values interpolated — that is fine and is not an exception to SAFE-02.

### Integration Points
- `res/blank` is v16, so v12–15 fixtures are produced by generating from the seed then **reverse-adjusting** the schema to the older shape (drop `Specialty`/`Edition`, drop `IX_Location_Media`, set `user_version`). The planner should confirm what else, if anything, actually differs below v16 — the FUNCTIONALITY-SPEC delta is documented for v16↔v14 specifically; v12/v13 differences need checking rather than assuming.

</code_context>

<specifics>
## Specific Ideas

- Real-data survey that triggered this phase (2026-07-20, 32 archives under `C:\Users\artic\OneDrive\_JW`): **v14 × 19, v16 × 13**. Every iPad backup is v14; the two `TITANIUMLABSLT7` laptop backups split across both.
- A real v16 archive already round-trips successfully: 5.94 MB in, 4,312 notes read, all zip entries preserved, source byte-identical, Python `check_validity` **ACCEPTED**. Phase 3's bar is to reach that same result starting from a v14 file.
- The saved archive is currently *larger* than the source (6.29 MB vs 5.94 MB) because `trim_db` + VACUUM do not exist yet — that is Phase 2 (ARCH-04), expected, not a defect.

</specifics>

<deferred>
## Deferred Ideas

- **v16→v14 downgrade on save + the 7-table LocationId remap closure** — Phase 4 (SCHEMA-03/04/05). This phase only goes *up*.
- **`trim_db`/VACUUM** — Phase 2 (ARCH-04); explains the current size growth on save.
- **Playlist DB upgrade** (`JWLManager.py:2582` upgrades playlist DBs too) — playlists are Phase 6; note the call site but do not implement it here.
- **Accepting >v16** — deliberately rejected (D3-09); revisit only when a real newer-schema archive exists to test against.

</deferred>

---

*Phase: 3-Schema Upgrade*
*Context gathered: 2026-07-20*
</content>
