# Phase 4: Schema Downgrade - Context

**Gathered:** 2026-07-21
**Status:** Ready for planning

<domain>
## Phase Boundary

A user who needs v14 compatibility (older JW Library) can **explicitly** opt into a downgraded save. The save performs the 7-table LocationId remap closure, is previewed via Phase 2's dry-run, produces a semantically correct v14 archive, and leaves the app's working in-memory copy at v16.

**In scope:** an explicit "save as v14" user choice (never default/implicit); the v16→v14 Location DDL (drop `Specialty`/`Edition`, the v14 UNIQUE constraints incl. the second `UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)`, `PRAGMA user_version=14`); the **7-table LocationId remap closure** that merges Locations which collide under v14's stricter uniqueness; a dry-run preview (reuse Phase 2 `DryRunReport`); and working-copy-stays-v16 (backup/restore).

**Out of scope (own phases):** merge (Phase 5), the other categories' browse/edit (6/7), import/export (8), signing/localization (11). Upward schema handling is Phase 3 (done).

**Requirements:** SCHEMA-03, SCHEMA-04, SCHEMA-05

**Depends on:** Phase 2 (dry-run mechanism, `DryRunReport`, PragmaGuard, trim), Phase 3 (schema gate + upgrade + the v16 Location shape). Both complete.

</domain>

<decisions>
## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit.

### The remap closure — the data-integrity core (SCHEMA-04)

- **D4-01 (THE fix — deterministic ordering, load-bearing):** The Python `downgrade_schema` (`JWLManager.py:1172-1190`) groups Locations by the v14 uniqueness key and, for each group with >1 member, keeps `ids[0]` and remaps all other ids onto it across 7 tables. **`ids` comes from a SELECT with NO `ORDER BY`, so `ids[0]` is whichever row SQLite happens to return first — non-deterministic.** SCHEMA-04 demands "explicit, documented, tested ordering semantics." The Rust port MUST add a deterministic `ORDER BY LocationId` (keep the LOWEST LocationId as the survivor) to the grouping query, so the survivor is stable, reproducible, and testable. This is a deliberate, documented divergence from the Python latent bug — parity of BEHAVIOR (merge colliding Locations) without parity of its non-determinism.
  `[auto] ordering — Q: "Keep ids[0] with no ORDER BY (Python parity), or impose a deterministic ORDER BY?" → Selected: "Deterministic ORDER BY LocationId, keep lowest" (recommended default)`
  **Rationale:** Non-deterministic survivor selection means the same archive downgraded twice could produce different LocationId remappings and different resulting data — a Core-Value hazard and untestable. A documented `ORDER BY LocationId` makes the closure a pure function of the input. This is the single most important decision in the phase; it is exactly the risk flagged at project inception ("`keep ids[0]` first-row-order depends on SQLite iteration order").

- **D4-02:** The 7 remap targets are exactly (matching `JWLManager.py:1185-1191`): `Bookmark.LocationId`, `Bookmark.PublicationLocationId` (2 columns on Bookmark), `Note.LocationId`, `UserMark.LocationId`, `InputField.LocationId`, `TagMap.LocationId`, `PlaylistItemLocationMap.LocationId`. Then `DELETE FROM Location WHERE LocationId = old_id` for each merged-away id. Port these UPDATEs verbatim (parameterized), in this order, per merged group.

- **D4-03:** The grouping key is `KeySymbol|IssueTagNumber|MepsLanguage|DocumentId|Track|Type` over Locations `WHERE BookNumber IS NULL AND ChapterNumber IS NULL` (the exact Python predicate at `:1175`) — only these can collide under the v14 `UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)` that v16 lacks. Do NOT remap Locations with a BookNumber/ChapterNumber (scripture) — they're keyed differently and don't collide on this constraint.

- **D4-04 (defect NOT ported):** The Python `except: crash_box; sys.exit()` is NOT ported — a failed downgrade → typed `ArchiveError` + full rollback. Same posture as Phase 3 `upgrade_to_v16` and Phase 2 `trim_db`.

- **D4-05:** The remap + the Location DDL rebuild run inside ONE transaction; foreign_keys forced OFF for the duration (reuse Phase 2 `PragmaGuard`; FK is ON by default here — the Phase 3/2 finding); rollback leaves the working copy fully v16/untouched. The DDL `INSERT INTO Location_new SELECT ...` (10 columns, dropping Specialty/Edition) after the remap, then DROP/RENAME, then `PRAGMA user_version=14`.

### Working copy stays v16 (SCHEMA-05)

- **D4-06:** The downgrade operates on a **COPY** of the working-copy DB (or a snapshot/restore), never on the live session DB. The v14 bytes are written to the user's chosen output file via the Phase 1 atomic-save path; the in-memory session remains at v16 so the user can keep editing. Add a test: after a v14 save, `session.manifest.schema_version` and the working DB's `PRAGMA user_version` are still 16.
  `[auto] working-copy — Q: "Downgrade the live working copy then re-upgrade, or downgrade a throwaway copy?" → Selected: "Downgrade a throwaway copy; session stays v16" (recommended default)`
  **Rationale:** Downgrade is lossy (merges Locations). Applying it to the live session then re-upgrading would not restore the merged Locations — data would be permanently changed in the user's session. A throwaway copy keeps the session pristine.

### Explicit choice + preview (SCHEMA-03, and reuse Phase 2)

- **D4-07:** v14 save is an explicit, separate user action ("Save v14-compatible copy…"), never the default Save. It carries its own confirmation.
- **D4-08:** Before the downgrade save, show a **dry-run preview reusing Phase 2's `DryRunReport`** — specifically surfacing how many Locations will be MERGED (the remap closure's effect) and any consequent row changes, plus the trim effect. The general `added/overwritten/deleted` shape covers "N Locations merged" as `deleted` (the merged-away Location rows) + `overwritten` (the remapped foreign-key rows). Reuse the rolled-back-transaction dry-run exactly.

### Verification (SCHEMA-04 round-trip)

- **D4-09:** A round-trip semantic-equivalence test: a v16 fixture containing **Locations that collide under v14 uniqueness** (same KeySymbol/IssueTagNumber/MepsLanguage/DocumentId/Track/Type, BookNumber+ChapterNumber NULL, distinct LocationIds) with dependent rows across all 7 remap targets → downgrade → assert (a) `user_version=14`, (b) the colliding Locations merged to the lowest LocationId, (c) every dependent row (Bookmark×2 cols, Note, UserMark, InputField, TagMap, PlaylistItemLocationMap) now points at the survivor, (d) no v14 UNIQUE violation, (e) Specialty/Edition columns gone. NEVER byte equality.
- **D4-10:** The Python differential oracle extended: a downgraded archive is accepted by `check_validity` (Python), AND — stronger — the Python app's OWN `downgrade_schema` on the same v16 fixture produces a semantically equivalent v14 result (modulo the deterministic-ordering fix). The env-gated real-archive path: a real v16 archive downgraded and Python-accepted.

### Claude's Discretion
Module placement (likely `archive/downgrade.rs`), how the throwaway-copy is materialized (file copy vs `VACUUM INTO` vs in-memory), the exact dry-run count categorization, and test organization.

</decisions>

<canonical_refs>
## Canonical References — downstream agents MUST read

### The downgrade source of truth
- `JWLManager.py:1172-1245` — `downgrade_schema`: the grouping query (`:1175`, note NO ORDER BY — D4-01 fixes this), the `keep_id = ids[0]` merge (`:1183`), the 7 remap UPDATEs + Location DELETE (`:1185-1191`), the v14 Location DDL with BOTH v14 UNIQUE constraints (`:1192-1237`), `PRAGMA user_version=14`. The `except:crash_box:sys.exit` at `:1239-1242` is the defect NOT ported (D4-04).
- `JWLManager.py:1016-1070` — the v16 upgrade DDL (Phase 3) — the exact inverse; the v14↔v16 delta is Specialty/Edition + IX_Location_Media + the second Location UNIQUE.

### Foundations this builds on
- `.planning/phases/03-schema-upgrade/03-02-SUMMARY.md` — `upgrade_to_v16` transactional/PragmaGuard/typed-error pattern to mirror; the v16 Location shape; FK-not-default-off finding.
- `.planning/phases/02-safe-delete/02-02-SUMMARY.md` — `DryRunReport` + `dry_run_*` rolled-back-txn mechanism to reuse (D4-08); `PragmaGuard`.
- `.planning/phases/02-safe-delete/02-01-SUMMARY.md` — trim runs on save; the SQLite gotchas (FK-on-default, temp_store=MEMORY drops temp triggers).
- `app/src-tauri/src/archive/upgrade.rs`, `src/db/trim.rs`, `src/db/pragma_guard.rs`, `src/db/delete.rs` (DryRunReport), `src/archive/save.rs`, `src/session.rs`, `src/error.rs`.
- `.planning/research/FUNCTIONALITY-SPEC.md` — the v16↔v14 delta + the remap closure documentation.

</canonical_refs>

<code_context>
## Existing Code Insights
- `upgrade.rs` is the near-exact structural template (transaction, PragmaGuard, typed error, rollback test) — downgrade is its inverse plus the remap closure.
- `DryRunReport` + the rolled-back-txn dry-run (Phase 2) reuse directly for D4-08.
- `PragmaGuard` (Phase 2) reuse for FK-off-during / restore-after.
- The Phase 3 fixture generator (versioned) + Phase 2 trim fixture — extend to seed colliding Locations with dependents across all 7 tables.
- The verified Python differential oracle (`tests/differential.rs`) — extend for downgrade acceptance.

## Established Patterns
- Working copy in temp dir; source never mutated; save = atomic temp+rename.
- Typed errors, no unwrap/expect on archive-data paths, all SQL parameterized (the remap UPDATEs bind keep_id/old_id).
- FK ON by default here → force OFF during the closure; PRAGMAs restored via guard.
- Semantic (normalized-table) parity, never byte-diff.

## Integration Point / risk
- **The `ids[0]` ordering (D4-01) is THE risk this phase exists to close correctly.** Any plan/executor must add `ORDER BY LocationId` and TEST that the survivor is deterministically the lowest id, with all 7 dependent tables repointed. A cross-AI review should specifically probe: does any remap target get missed? does a merged Location leave a dangling FK? is the survivor stable across repeated runs?
</code_context>

<specifics>
## Specific Ideas
- This is the phase the project's founding risk note called out by name. Get the ordering deterministic and tested; everything else is a faithful port of a well-understood transform.
- Real v14 files exist in the owner's library (19 of them) — but those are ALREADY v14; the downgrade path is v16→v14, so the acceptance test needs a v16 fixture/archive with colliding Locations, downgraded and Python-accepted.
</specifics>

<deferred>
## Deferred Ideas
- Downgrade to versions other than v14 — out of scope; only v14 is the supported downgrade target (SCHEMA-03).
- N-way merge / merge downgrade interplay → Phases 5/10.
</deferred>

---

*Phase: 4-Schema Downgrade*
*Context gathered: 2026-07-21*
</content>
