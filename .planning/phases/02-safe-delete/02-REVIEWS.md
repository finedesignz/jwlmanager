---
phase: 2
reviewers: [codex]
reviewed_at: 2026-07-21
plans_reviewed: [02-01-PLAN.md, 02-02-PLAN.md, 02-03-PLAN.md]
attempted_but_failed: [gemini]
overall_external_risk: HIGH
---

# Cross-AI Plan Review — Phase 2 (Safe Delete)

## Codex Review (summary of full text; full text archived in scratchpad)

Right safety architecture (DML/VACUUM split, dry-run-by-rollback, explicit-column re-densify, hash-last, typed errors, NonEmptyNoteIds). Would NOT ship unchanged — the risks are semantic, in the delete/trim model.

**Concerns:**
- HIGH — Over-deletes UserMark/BlockRange. Python Notes-delete does only `DELETE FROM Note` (JWLManager.py:3666); FUNCTIONALITY-SPEC:140 confirms Notes-delete targets only Note. `Note.UserMarkId` is not unique and a UserMark can carry highlight BlockRange data — deleting "the Note's own UserMark/BlockRange" can destroy highlight/mark data the user did not select.
- HIGH — Dry-run counts must be SEMANTIC not physical `changes()`. The re-densify does `DELETE FROM TagMap` + full reinsert; counting statement `changes()` would report every tag mapping as "deleted". `UPDATE Location SET Title=''` is an overwrite/normalization, not a deletion. The preview would lie.
- HIGH — Verbatim `NOT IN` NULL-poisons. Nullable subqueries (`PlaylistItemId NOT IN (SELECT PlaylistItemId FROM TagMap)`, `LocationId NOT IN (SELECT LocationId FROM Note/TagMap)`) — one NULL makes NOT IN match nothing, so orphan Location/PlaylistItem are never swept. Fixtures already seed an independent Note with LocationId NULL and a TagMap with PlaylistItemId NULL, so the "orphan swept" tests would FAIL under a verbatim port.
- HIGH — PRAGMA state is not rollback-protected. `PRAGMA foreign_keys=OFF` is not undone by ROLLBACK; the core fn takes `&mut Connection` and can leave callers in a different FK mode after a dry-run. Plan 01 restores to hardcoded ON while Phase 3 expects FK left off — inconsistent.
- MEDIUM — Rollback test must fail AFTER `DELETE FROM TagMap` (via an INSERT-aborting trigger) to actually prove the delete-then-reinsert re-densify is recoverable; failing before it proves nothing.
- MEDIUM — Fixture's duplicate Tag positions (5,5,9 same TagId) violate `UNIQUE(TagId,Position)` and can't be inserted into a valid v16 DB. Use gaps (5,9,20).
- MEDIUM — Save becomes destructive without preview (trim on every save silently removes empty InputFields, empty untagged Notes, unused Tags). Matches the locked decision but weakens "first destructive op always previewed."
- LOW — UI needs a refresh story after delete (local removal or a query_notes command); save-time trim may remove rows beyond the preview.

**Suggestions:** delete only `Note` rows; replace nullable `NOT IN` with `NOT EXISTS` (documented safety fix); semantic dry-run diff not statement counts; `PRAGMA foreign_key_check` after trim/save; PRAGMA restoration guard + tests (success + failure); rollback test aborting after TagMap delete; survivor test (Location kept when referenced by Bookmark.LocationId or PublicationLocationId).

**Overall Risk: HIGH** — block until UserMark/BlockRange scope, nullable NOT IN, and semantic dry-run accounting are corrected.

## Gemini — not run (auth failure, environment).

## Consensus / Orchestrator triage — ALL accepted, folded into a --reviews revision

1. **Delete only Note rows (HIGH, over-deletion)** — ACCEPT. `delete_notes` = `DELETE FROM Note WHERE NoteId IN (bound ids)` ONLY. Do NOT delete UserMark/BlockRange in the delete op — trim (with the NOT EXISTS fix) sweeps genuine orphans. Update D2-05. Add a test: deleting a Note whose UserMark still has BlockRange data does NOT remove that UserMark/BlockRange if still referenced.
2. **Semantic dry-run accounting (HIGH, preview lies)** — ACCEPT. DryRunReport is built from before/after row-IDENTITY snapshots per affected table (or per-statement classification): the TagMap re-densify counts as `overwritten` (net-zero rows for preserved PKs), `UPDATE Location SET Title=''` counts as `overwritten`, only genuine row removals count as `deleted`. Add a test: a dry-run on a fixture with tag mappings reports 0 TagMap deletions for preserved mappings (not "all deleted").
3. **NOT EXISTS instead of nullable NOT IN (HIGH, under-sweep)** — ACCEPT. Replace every nullable-subquery `NOT IN` in the sweep (Location, PlaylistItem, and any TagMap/UserMark predicate over a nullable column) with `NOT EXISTS (SELECT 1 ... WHERE ...)`. Document as a DELIBERATE safety fix over Python's latent NULL-poisoning bug (Python under-sweeps here; we sweep correctly). The orphan-swept tests must PASS (they'd fail under verbatim NOT IN). Keep the non-nullable predicates as-is for order fidelity.
4. **PRAGMA restoration guard (HIGH, leak)** — ACCEPT. A small RAII/guard that snapshots foreign_keys/journal_mode/synchronous/temp_store before the sweep and restores them on drop (so both commit and rollback/dry-run paths restore). Tests assert all four PRAGMAs are back to their prior values after: dry-run success, dry-run failure, trim success, trim failure. Add `PRAGMA foreign_key_check` after a real trim/save to prove no dangling refs.
5. **Rollback test aborts AFTER DELETE FROM TagMap (MEDIUM)** — ACCEPT. Use a temporary trigger that RAISEs on `INSERT INTO TagMap`, so the failure lands after the destructive delete; assert the original TagMap rows are fully restored by rollback.
6. **Fix fixture Tag positions (MEDIUM)** — ACCEPT. Use gapped positions within a TagId (5,9,20) — never duplicates within one TagId (violates UNIQUE(TagId,Position)). Duplicates only across different TagId partitions if needed.
7. **Document save-time trim is destructive (MEDIUM)** — ACCEPT as recorded scope. trim-on-save silently removes empty untagged Notes / empty InputFields / unused Tags, matching the Python app. Note it in the SUMMARY + a test documenting exactly what a bare save removes; a save-time trim PREVIEW is deferred (add to deferred ideas), not built now.
8. **UI refresh after delete (LOW)** — ACCEPT. After apply, remove the deleted Notes from the list locally; note that save-time trim may remove additional rows (the preview already showed them).
9. **Survivor test (from suggestions)** — ACCEPT. A deleted Note's Location SURVIVES trim when still referenced by Bookmark.LocationId or Bookmark.PublicationLocationId (the sweep's Location predicate checks Bookmark) — add this to the multi-table fixture + assert.
</content>
