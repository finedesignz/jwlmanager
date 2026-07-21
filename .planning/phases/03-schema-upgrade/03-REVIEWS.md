---
phase: 3
reviewers: [codex]
reviewed_at: 2026-07-20
plans_reviewed: [03-01-PLAN.md, 03-02-PLAN.md, 03-03-PLAN.md]
attempted_but_failed: [gemini]
overall_external_risk: HIGH
---

# Cross-AI Plan Review — Phase 3 (Schema Upgrade)

## Codex Review (full text)

**Summary:** Directionally strong safety posture (working-copy-only, fail-loudly, typed errors, transactional). Main remaining risk is overclaiming v12/v13 and two concrete correctness gaps. Ship as "v14/v16 proven; v12/v13 same code path with explicit residual risk," not "v12–v16 proven," unless a post-upgrade schema contract is added.

**Concerns:**
- HIGH — v12/v13 safety not actually proven; synthetic reverse-mutated fixtures are circular (prove the code path, not real old-schema correctness). A real v12 DB missing another required table/column/index could be stamped user_version=16 and later behave like a valid archive.
- HIGH — existing Specialty/Edition data can be LOST in the D3-04 edge case. The migration guards ADD COLUMN, but the copy still does `NULL, NULL`. An archive with user_version<16 that already has non-null Specialty/Edition upgrades "successfully" and discards those values. The D3-04 test as written checks success/version, not row preservation.
- MEDIUM — in-range manifest/PRAGMA mismatch not specified (manifest 14, DB 16). Save normalizes to 16 (reads PRAGMA), so the worst vector is mostly closed, but open-path behavior should be an explicit choice with tests both directions.
- MEDIUM — missed file owners: repo has a separate v16-only gate + v14-rejection test in `manifest.rs`/`manifest_tests.rs`/`archive_validity_tests.rs`/`error_tests.rs`. Plans focus on `archive/mod.rs`; the retirement of the v16-only gate is incomplete without updating these.
- MEDIUM — Python `check_validity` is a shallow oracle (zip + manifest schemaVersion>11); keep it but don't treat it as proof the upgraded DB is semantically valid.
- MEDIUM — unusual real v14 Location rows could fail the rebuild INSERT..SELECT on CHECK/UNIQUE. Safer than corruption, but test representative Location types (scripture, publication, media/document, media/track, Type 1, Type 2/3, NULL-heavy, duplicate media-key).
- LOW/MEDIUM — FK-off acceptable but brittle; assert/enforce foreign_keys OFF around the transaction so a future FK-on caller can't change DROP TABLE behavior.
- LOW — stale `Location_new` from a prior failed Python upgrade: failing loudly is safe; note handling.

**Suggestions:** post-upgrade schema validator (required tables/columns/indexes + user_version==16) as the best v12/v13 mitigation; preserve existing columns in the copy (`SELECT ..., Specialty, Edition` after the conditional ADD); tests for in-range mismatch; update the manifest.rs gate + tests; make upgrade_to_v16 no-op exactly at v16 (reject >16 on direct call); strengthen round-trip (reopen after save, assert columns/index/notes preserved); honest v12/v13 language.

**Overall Risk: HIGH** — approach sound, evidence model too weak for "any real v12–v16." Highest-value fix is small: preserve preexisting Specialty/Edition and add a post-upgrade schema contract so unknown gaps fail loudly instead of being mislabeled v16.

## Gemini Review

Not run — gemini-cli auth failure (same as Phase 1). Reviewer-environment issue, not a plan defect.

## Consensus / Orchestrator triage — findings folded into a --reviews revision

All accepted; none rejected. The two HIGH items are genuine corruption/data-loss vectors on a data-integrity tool.

1. **Preserve existing Specialty/Edition (HIGH, silent data loss)** — ACCEPT. The INSERT..SELECT must copy existing Specialty/Edition values when the columns pre-exist (the D3-04 path), not `NULL, NULL`. Build the SELECT column list conditionally on the `column_exists` guard result. Add a test: a v<16 DB with NON-NULL Specialty/Edition upgrades and those exact values survive.
2. **Post-upgrade schema validator (HIGH mitigation)** — ACCEPT. After upgrade + before accepting the session, validate the working DB against a v16 contract: required tables present, Location has all v16 columns, IX_Location_Media exists, PRAGMA user_version == 16. On any gap → typed `SchemaUpgradeFailed`/`SchemaContractViolation`, not silent acceptance. This is what makes an unknown v12/v13 gap fail loudly.
3. **Update manifest.rs gate + its tests (MEDIUM, completeness/build-correctness)** — ACCEPT. 03-02 must also widen/retire the v16-only gate in `manifest.rs` and update `manifest_tests.rs` / `archive_validity_tests.rs` / `error_tests.rs` — specifically the existing test asserting a v14 archive is REJECTED must become "v14 accepted + upgraded" (or move to the reject-suite for ≤11/>16). Add these files to 03-02's files_modified.
4. **In-range manifest/PRAGMA mismatch (MEDIUM)** — ACCEPT. Make it an explicit decision: normalize (upgrade the DB, write 16 to manifest on save) and add tests for manifest14/DB16 and manifest16/DB14. Document the chosen behavior in the plan.
5. **Representative Location-type rebuild test (MEDIUM)** — ACCEPT. The fixture used for the upgrade test must contain scripture, publication, media/document, media/track, Type 1, Type 2/3, and NULL-heavy Location rows so the INSERT..SELECT + CHECK/UNIQUE constraints are actually exercised.
6. **Assert foreign_keys OFF around the transaction (LOW/MEDIUM)** — ACCEPT. Explicitly assert `PRAGMA foreign_keys` is 0 before the rebuild (or disable+restore), with a test, so a future FK-on caller can't silently change DROP TABLE semantics.
7. **no-op exactly at v16 (LOW)** — ACCEPT. `upgrade_to_v16` returns Ok only when user_version == 16; > 16 is `SchemaTooNew` even on a direct call (belt-and-suspenders with the gate).
8. **Strengthen round-trip (LOW)** — ACCEPT. After saving an upgraded v14 fixture, reopen and assert user_version==16, Location columns + IX_Location_Media exist, note count/content unchanged, manifest schemaVersion==16.
9. **Honest v12/v13 language (LOW)** — ALREADY DONE in CONTEXT/VALIDATION; keep it in any user-facing copy.
</content>
