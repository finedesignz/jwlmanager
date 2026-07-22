# Phase 4: Schema Downgrade - Research

**Researched:** 2026-07-22
**Domain:** SQLite schema transform (v16 -> v14), LocationId FK-remap closure, Rust/rusqlite
**Confidence:** HIGH (all claims verified against live v16 blank schema + Phase 3 source in-repo)

## Summary

Phase 4 ports `JWLManager.py:1172-1243` (`downgrade_schema`) to Rust as the near-exact structural inverse of Phase 3's `upgrade_to_v16` (`archive/upgrade.rs`), plus a 7-column LocationId **remap closure** that merges Locations colliding under v14's stricter second UNIQUE constraint. Every structural pattern already exists in-repo and is verified: the transactional rebuild, `PragmaGuard` FK-off, typed `ArchiveError`, rollback-on-drop, `DryRunReport` rolled-back-txn preview, and the atomic save path. This is a faithful port of a well-understood transform with **one deliberate, documented divergence**: D4-01's deterministic `ORDER BY LocationId` survivor selection replacing Python's non-deterministic `ids[0]`.

The single load-bearing risk (the reason this phase exists) is survivor determinism. Everything else is mechanical. The v16<->v14 delta is exactly: drop `Specialty`/`Edition` columns, drop the `IX_Location_Media` UNIQUE index (which encoded those two columns), add the second `UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)` constraint, set `user_version = 14`.

**Primary recommendation:** Create `archive/downgrade.rs` mirroring `upgrade.rs`. Run remap closure (with `ORDER BY LocationId`) + DDL rebuild in ONE transaction with `PragmaGuard` forcing `foreign_keys = OFF`. Operate on a **file-copy** throwaway DB (`std::fs::copy`), never the live session. Reuse `DryRunReport` for the preview.

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D4-01 (THE fix):** Add deterministic `ORDER BY LocationId` to the grouping query; keep the LOWEST LocationId as survivor. Documented divergence from Python's non-deterministic `ids[0]`.
- **D4-02:** 7 remap column-targets, verbatim order per merged group: `Bookmark.LocationId`, `Bookmark.PublicationLocationId`, `Note.LocationId`, `UserMark.LocationId`, `InputField.LocationId`, `TagMap.LocationId`, `PlaylistItemLocationMap.LocationId`; then `DELETE FROM Location WHERE LocationId = old_id`.
- **D4-03:** Grouping key = `KeySymbol|IssueTagNumber|MepsLanguage|DocumentId|Track|Type` over `WHERE BookNumber IS NULL AND ChapterNumber IS NULL`. Do NOT remap scripture Locations.
- **D4-04:** Python `except: crash_box; sys.exit()` NOT ported -> typed `ArchiveError` + full rollback.
- **D4-05:** Remap + DDL rebuild in ONE transaction; `foreign_keys` forced OFF via `PragmaGuard`; rollback leaves working copy untouched.
- **D4-06:** Downgrade a throwaway COPY; session stays v16 (assert `user_version`/manifest still 16 after v14 save).
- **D4-07:** v14 save is an explicit separate action ("Save v14-compatible copy..."), own confirmation, never default.
- **D4-08:** Dry-run preview reusing Phase 2 `DryRunReport`: N Locations merged surfaced as `deleted` (merged-away Location rows) + `overwritten` (remapped FK rows) + trim effect.
- **D4-09:** Round-trip semantic-equivalence test (colliding Locations + dependents across all 7 targets). NEVER byte equality.
- **D4-10:** Python differential oracle extended: downgraded archive accepted by `check_validity`; Python's own `downgrade_schema` produces semantically equivalent v14 (modulo ordering fix). Env-gated real-archive path.

### Claude's Discretion
Module placement (`archive/downgrade.rs`), throwaway-copy materialization (file copy vs `VACUUM INTO` vs in-memory), dry-run count categorization, test organization.

### Deferred Ideas (OUT OF SCOPE)
- Downgrade to versions other than v14 (only v14 supported).
- N-way merge / merge-downgrade interplay (Phases 5/10).

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SCHEMA-03 | Explicit opt-in v14 save | D4-07 UI action + confirmation; separate command path |
| SCHEMA-04 | Documented/tested remap closure with ordering semantics | D4-01 `ORDER BY LocationId`; 7-column remap verified complete (below) |
| SCHEMA-05 | Working copy stays v16 | D4-06 throwaway file-copy; session pristine |

## LocationId FK Completeness Audit (verified)

`[VERIFIED: PRAGMA foreign_key_list over res/blank userData.db, user_version=16]`

Enumerated every table in the live v16 blank schema. Tables carrying a LocationId FK to `Location.LocationId`:

| Table | FK column(s) | In Python remap list? |
|-------|-------------|----------------------|
| Bookmark | `LocationId`, `PublicationLocationId` | YES (both) |
| Note | `LocationId` | YES |
| UserMark | `LocationId` | YES |
| InputField | `LocationId` | YES |
| TagMap | `LocationId` | YES |
| PlaylistItemLocationMap | `LocationId` | YES |
| Location | `LocationId` (self, PK) | N/A (target, not remapped) |

**Result: the Python 7-column remap list is COMPLETE. No LocationId FK is omitted.** 6 dependent tables, 7 FK columns, all covered. No hidden LocationId FK exists in v16.

## v16 -> v14 DDL Delta (exact)

`[VERIFIED: JWLManager.py:1193-1236 vs archive/upgrade.rs CREATE_LOCATION_NEW + CREATE_INDEXES]`

The v14 `Location` shape is the v16 shape MINUS `Specialty`/`Edition`, PLUS the second UNIQUE:

| Element | v16 (Phase 3) | v14 (Phase 4 target) |
|---------|---------------|----------------------|
| `Specialty TEXT` column | present | **DROPPED** |
| `Edition TEXT` column | present | **DROPPED** |
| `UNIQUE (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)` | present | present (unchanged) |
| `UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)` | ABSENT | **ADDED** (the constraint that forces the remap) |
| 3 CHECK constraints (Type 0/1/2-3) | present | present (byte-identical) |
| `IX_Location_Media` UNIQUE index | present (encodes Specialty/Edition via COALESCE) | **NOT created** (v14 has no such index; the added table UNIQUE replaces its role) |
| `IX_Location_KeySymbol_...` / `IX_Location_MepsLanguage_DocumentId` | present | Python downgrade does NOT recreate these. Match Python: omit. |
| `PRAGMA user_version` | 16 | **14** |

Column list for the v14 `INSERT INTO Location_new SELECT` is exactly 10 columns (drops the two): `LocationId, BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type, Title`. This is a static list (no conditional column logic like the upgrade's finding-1 — downgrade always drops, never preserves, Specialty/Edition, matching Python).

**DDL ordering (from Python):** run remap closure first (UPDATEs + DELETEs), THEN `CREATE TABLE Location_new` -> `INSERT ... SELECT` -> `DROP TABLE Location` -> `ALTER TABLE Location_new RENAME TO Location` -> `PRAGMA user_version = 14`. The remap MUST precede the table rebuild so the new second-UNIQUE constraint is satisfied by the already-de-duplicated data.

Port the `Location_new` DDL as a `const` byte-exact from `JWLManager.py:1194-1229` (mirror `upgrade.rs`'s `CREATE_LOCATION_NEW`).

## The Remap Closure (deterministic — D4-01)

### Grouping query (WITH the fix)

```sql
SELECT LocationId, KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type
FROM Location
WHERE BookNumber IS NULL AND ChapterNumber IS NULL
ORDER BY LocationId
```

`ORDER BY LocationId` is the ONLY divergence from `JWLManager.py:1175`. Because rows arrive in ascending LocationId order, for each collision group `ids[0]` is now deterministically the LOWEST id -> survivor is a pure function of input. (Equivalent: `ORDER BY <key cols>, LocationId` if you also want groups themselves ordered, but grouping is done in a HashMap so only intra-group order matters — a plain `ORDER BY LocationId` suffices.)

Build groups keyed by the composite string `KeySymbol|IssueTagNumber|MepsLanguage|DocumentId|Track|Type` (match Python's `f"{...}|..."`; be deliberate about NULL rendering — Python renders `None`; in Rust normalize each key part to a canonical token, e.g. an `Option`-aware formatter, so two NULLs collide the same way Python's `None` does). Keep the first id per group as `keep_id`.

### Per-merged-group SQL (parameterized, verbatim order)

For each `old_id` in `ids[1..]`:

```rust
// all bind (keep_id, old_id)
tx.execute("UPDATE Bookmark SET LocationId = ?1 WHERE LocationId = ?2", (keep_id, old_id))?;
tx.execute("UPDATE Bookmark SET PublicationLocationId = ?1 WHERE PublicationLocationId = ?2", (keep_id, old_id))?;
tx.execute("UPDATE Note SET LocationId = ?1 WHERE LocationId = ?2", (keep_id, old_id))?;
tx.execute("UPDATE UserMark SET LocationId = ?1 WHERE LocationId = ?2", (keep_id, old_id))?;
tx.execute("UPDATE InputField SET LocationId = ?1 WHERE LocationId = ?2", (keep_id, old_id))?;
tx.execute("UPDATE TagMap SET LocationId = ?1 WHERE LocationId = ?2", (keep_id, old_id))?;
tx.execute("UPDATE PlaylistItemLocationMap SET LocationId = ?1 WHERE LocationId = ?2", (keep_id, old_id))?;
tx.execute("DELETE FROM Location WHERE LocationId = ?1", [old_id])?;
```

**Pitfall — TagMap uniqueness collision:** merging two Locations can produce duplicate `TagMap(TagId, LocationId)` rows if both merged Locations were tagged with the same Tag. Python's blind UPDATE can hit a TagMap UNIQUE constraint. Verify the v14 TagMap constraints during planning; if a UNIQUE exists on `(TagId, LocationId)`, the remap needs an `UPDATE OR IGNORE` (or pre-delete of would-be duplicates) — flag as a test case (two Locations sharing a tag). This is a latent edge Python may not handle; the Rust port should surface it via a typed error, not a panic. `[ASSUMED — verify TagMap constraints against v14 schema during planning]`

## Throwaway-Copy Materialization (D4-06)

**Recommendation: `std::fs::copy` of the working-copy DB file** to a temp path, run trim + downgrade on the copy, atomic-save the copy bytes to the user's target, discard the copy. Rationale:

| Option | Verdict |
|--------|---------|
| **File copy (`std::fs::copy`)** | RECOMMENDED. Matches Python (`shutil.copy2` at `:1249`), simplest, whole-DB snapshot including WAL-flushed state, no open-connection coupling. Ensure the working copy has no pending WAL (checkpoint or copy after a clean close). |
| `VACUUM INTO 'path'` | Viable, produces a defragmented copy in one statement, but overlaps trim's VACUUM and adds a second full write. Use only if you want trim folded in. Not worth the added path. |
| In-memory (`:memory:` + backup API) | Rejected: large archives blow memory; the atomic-save path already expects a file. |

Note Python calls `trim_db()` BEFORE the copy/downgrade (`:1245`). Preserve that order: trim the working copy (or the fresh copy), then downgrade the copy. Confirm whether trim should run on the live session or the copy — for session-stays-v16, run trim on the COPY only so the live session isn't mutated.

## Dry-Run Reuse (D4-08)

`DryRunReport { added, overwritten, deleted, total_deleted }` (each a `BTreeMap<String, usize>` keyed by table) maps cleanly:

- **`deleted["Location"]`** = count of merged-away Location rows (`sum(len(ids)-1)` over collision groups).
- **`overwritten[table]`** = count of FK rows repointed per table (Bookmark, Note, UserMark, InputField, TagMap, PlaylistItemLocationMap).
- Trim effect folds in via the existing trim sweep counts.

Implement `dry_run_downgrade(conn) -> DryRunReport` mirroring `dry_run_delete_notes` (`delete.rs:206`): open `PragmaGuard`, run the full remap+DDL inside a transaction, diff tracked tables, then **roll back** (drop tx without commit) so the preview mutates nothing. Extend `TRACKED_TABLES` to include `Location`, `Bookmark`, `InputField`, `PlaylistItemLocationMap` (each with its single-column PK) for accurate diffing. A "N Locations merged" UI count = `deleted["Location"]`.

## Module Placement & Structure

`archive/downgrade.rs`, mirroring `upgrade.rs`:

```
const CREATE_LOCATION_V14: &str        // byte-exact v14 Location DDL (2 UNIQUEs, 3 CHECKs)
fn run_downgrade_ddl(tx: &Transaction) // remap closure + table rebuild + user_version=14
pub fn downgrade_to_v14(conn: &mut Connection) -> Result<(), ArchiveError>
pub fn dry_run_downgrade(conn: &mut Connection) -> Result<DryRunReport, ArchiveError>
```

- Reuse `PragmaGuard` (Phase 2) to force `foreign_keys = OFF` before opening the transaction (PRAGMA is a no-op inside an active tx — set on connection first, exactly as `upgrade.rs:229`). Guard restores prior pragma state on drop.
- Add error variant `ArchiveError::SchemaDowngradeFailed { reason: String }` (mirror `SchemaUpgradeFailed`, `error.rs:34`) with its own stable code/message_key in the DTO mapping (`reason` never leaks to DTO).
- No FK ENABLE, no `legacy_alter_table` — only disable FK for the rebuild duration.
- Guard version preconditions: only downgrade a `user_version == 16` DB (or a documented supported range). If already 14, no-op `Ok(())`. If not 16, typed error.

## Test Strategy

Mirror `schema_upgrade_tests.rs` + `delete_roundtrip_tests.rs`. All fixtures SYNTHETIC (never a real `.jwlibrary`).

1. **Deterministic survivor (D4-01, THE test):** seed 3 colliding Locations (same key, BookNumber/ChapterNumber NULL, ids e.g. 50/20/90) -> downgrade -> assert survivor is `20` (lowest). Run twice, assert identical result (stability).
2. **Round-trip semantic equivalence (D4-09):** colliding Locations with dependents across ALL 7 targets (Bookmark w/ both LocationId + PublicationLocationId, Note, UserMark, InputField, TagMap, PlaylistItemLocationMap) -> downgrade -> assert (a) `user_version=14`, (b) merged to lowest id, (c) every dependent repointed to survivor, (d) no v14 UNIQUE violation (`PRAGMA integrity_check` + explicit uniqueness query), (e) `Specialty`/`Edition` columns gone (`PRAGMA table_info`).
3. **No-collision passthrough:** distinct Locations -> downgrade -> zero merges, all rows intact, columns dropped, version 14.
4. **TagMap shared-tag edge:** two merged Locations both tagged with the same Tag -> assert no crash, dedup handled (see remap pitfall above).
5. **Rollback on failure (D4-04):** inject a failing statement -> assert transaction rolls back, working copy still v16, original row-set intact, typed `SchemaDowngradeFailed`.
6. **Session stays v16 (D4-06):** after a v14 save, assert live session `PRAGMA user_version == 16` and `manifest.schema_version == 16`.
7. **Dry-run non-mutation (D4-08):** run `dry_run_downgrade`, assert report counts correct AND DB unchanged (version still 16, rows intact).
8. **Differential oracle (D4-10):** extend `tests/differential.rs` — Rust-downgraded archive accepted by Python `check_validity`; Python's own `downgrade_schema` on same fixture yields semantically equivalent v14 (modulo survivor id — Python may pick a different survivor, so compare NORMALIZED dependent-count-per-surviving-key, not literal ids). Env-gated real v16 archive path.

## Common Pitfalls

- **Non-deterministic survivor** — the whole reason for D4-01. Never omit `ORDER BY LocationId`.
- **FK enforcement ON by default** — this build's bundled SQLite does NOT default `foreign_keys=OFF` (Phase 3 finding, `upgrade.rs:229`). `DROP TABLE Location` and the DELETEs trip FK without explicit OFF. Set via `PragmaGuard`/connection before the tx.
- **NULL key rendering** — Python builds the group key from `None` values as literal string parts. Rust must render `NULL` key components identically so collision grouping matches (two NULL DocumentIds must land in the same group).
- **TagMap unique collision on merge** (see closure section).
- **Trim ordering** — trim before downgrade (Python `:1245`), on the copy for session-stays-v16.
- **temp_store=MEMORY drops temp triggers** (Phase 2 finding) — relevant if trim runs in the same connection.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | v14 TagMap may have a UNIQUE(TagId,LocationId) that the blind remap UPDATE can violate | Remap Closure | Downgrade errors on archives with shared tags across colliding Locations; needs `UPDATE OR IGNORE`/pre-dedup. Verify v14 TagMap DDL in planning. |
| A2 | Python may pick a different survivor id than Rust; differential oracle must compare normalized state not literal ids | Test Strategy | A naive id-equality differential test fails spuriously. |

## Sources

### Primary (HIGH confidence)
- `res/blank` userData.db (`PRAGMA foreign_key_list`, `PRAGMA table_info`, `user_version`) — LocationId FK completeness audit, v16 shape.
- `JWLManager.py:1172-1259` — `downgrade_schema` source of truth.
- `app/src-tauri/src/archive/upgrade.rs` — inverse structural template.
- `app/src-tauri/src/db/delete.rs` (`DryRunReport`, `dry_run_delete_notes`, `TRACKED_TABLES`), `src/error.rs` (`ArchiveError`).
- `.planning/phases/04-schema-downgrade/04-CONTEXT.md` — locked decisions.

## Metadata

**Confidence breakdown:**
- FK completeness / DDL delta: HIGH — verified against live v16 DB + Phase 3 source.
- Remap closure / ordering: HIGH — direct port with one verified divergence.
- TagMap edge / differential normalization: MEDIUM — flagged as assumptions to confirm in planning.

**Research date:** 2026-07-22
**Valid until:** stable (in-repo schema + source, not external deps)
