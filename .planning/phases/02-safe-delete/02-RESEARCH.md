# Phase 2: Safe Delete (Dry-Run + Trim + Transactions) - Research

**Researched:** 2026-07-21
**Domain:** Transactional SQLite mutation (rusqlite) — orphan sweep, dry-run-via-rollback, typed empty-selection rejection
**Confidence:** HIGH

## Summary

Phase 2 has one real unknown (does the Python `trim_db` sweep, including a `ROW_NUMBER() OVER (PARTITION BY ...)` re-densify via a temp table, port cleanly to rusqlite 0.40 bundled SQLite) and the rest is composition of patterns this codebase already proved in Phase 1 (atomic hash-last save) and Phase 3 (`upgrade.rs` — transactional rebuild, FK forced off explicitly because this build's bundled SQLite does NOT default `foreign_keys` OFF, typed-error propagation on drop = automatic rollback, rollback-proof test style). `upgrade.rs` and `schema_upgrade_tests.rs` are the direct template to clone for `trim.rs`/`delete.rs` and their tests.

The one genuinely new mechanism is the dry-run: `rusqlite::Connection::transaction()` returns a `Transaction` that rolls back automatically on `Drop` unless `.commit()` is called — so a dry-run is simply "run the real delete + trim SQL inside a `Transaction`, read `changes()`/row counts, let the `Transaction` drop without committing." No scratch-copy, no diffing needed.

**Primary recommendation:** Port `trim_db` verbatim (order-preserving) as a single `const &str` executed via `Transaction::execute_batch` inside `db/trim.rs`, following `upgrade.rs`'s exact shape (explicit `PRAGMA foreign_keys = OFF` on the connection BEFORE opening the transaction — confirmed by Phase 3's empirical finding, not assumed synchronous/temp_store/journal_mode PRAGMAs are session-level and safe to set inside `execute_batch` alongside the DML). VACUUM runs as a separate `conn.execute_batch("VACUUM;")` call AFTER the transaction commits (SQLite hard-forbids VACUUM inside a transaction — this is a real constraint, not a Python quirk). Dry-run reuses the same SQL through a `Transaction` that is intentionally never committed.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Delete selected Notes (row removal) | API/Backend (Rust core, `db/delete.rs`) | — | All SQL mutation lives in Rust core per existing architecture; no business logic in frontend |
| Orphan sweep / tag re-densify / VACUUM (`trim_db`) | API/Backend (Rust core, `db/trim.rs`) | — | Same as Phase 3's `upgrade.rs` — pure DB-layer transformation, no UI involvement |
| Dry-run preview computation | API/Backend (Rust core) | — | Must run identical SQL to the real op inside a rolled-back `Transaction`; only the backend has DB access |
| Empty-selection rejection (SAFE-03) | API/Backend (Tauri command signature) + Frontend (UI gating) | Both | Backend is the enforcement boundary (typed rejection before touching DB); frontend is defense-in-depth (button only enabled with ≥1 selected) — SAFE-03 explicitly requires both, "not merely a disabled button" |
| Preview/confirm UI | Frontend (React) | — | Renders `DryRunReport`, gathers explicit confirm, reuses Phase 1 command-bar + ErrorDto surface |
| Save (hash-last, atomic rename) | API/Backend (`archive/save.rs`) | — | Existing Phase 1 module; trim slots in immediately before `update_manifest`/hash step |

## Standard Stack

### Core (already in the project — no new dependencies)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.40 (bundled feature) `[VERIFIED: app/src-tauri/Cargo.toml:22]` | SQLite driver, transactions, pragmas | Already used by `upgrade.rs`; `bundled` means SQLite is compiled from source into the binary, giving a modern, known SQLite version (window functions like `ROW_NUMBER() OVER (...)` have been supported since SQLite 3.25, 2018 — far below anything `bundled` in a 2026-era rusqlite 0.40 ships) `[CITED: sqlite.org/windowfunctions.html]` |
| ts-rs | already in use | Export `DryRunReport`/delete command types to TS bindings | Matches `NotesRow`/`ErrorDto` pattern already established |

**No installation needed.** This phase adds zero new crates. `[VERIFIED: grep of Cargo.toml — no new dependency required for transactions, window functions, or VACUUM; all are core SQLite/rusqlite features already available]`

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Rolled-back `Transaction` for dry-run (D2-07) | Copy DB to scratch file, run real op, diff, discard | Rejected by CONTEXT.md D2-07 — a scratch copy risks preview/apply divergence and doubles I/O for a large `userData.db`; the rolled-back transaction guarantees byte-identical logic between preview and apply |
| Verbatim single-batch `trim_db` port (D2-01) | Rewrite as discrete `rusqlite::Statement`s issued individually | Rejected by CONTEXT.md D2-01 — order is load-bearing (children before parents, re-densify after TagMap orphan removal); a single ordered batch is the safest literal port |

## Package Legitimacy Audit

Not applicable — this phase introduces no new external packages (confirmed via `Cargo.toml` read). No `slopcheck`/registry check needed.

## Architecture Patterns

### System Architecture Diagram

```
Frontend (React)                    Tauri IPC                    Rust Core
─────────────────                   ──────────                   ─────────
Notes table, checkbox select
        │
        ▼
[Delete button] ──(only enabled            "delete_notes_dry_run"
  when selection.len() ≥ 1)──────────────────────►  cmd(ids: NonEmptyNoteIds)
                                                          │
                                                          ▼
                                          BEGIN Transaction (rusqlite)
                                            DELETE FROM Note WHERE NoteId IN (...)
                                            DELETE FROM UserMark/BlockRange (Note-owned)
                                            trim_db sweep SQL (same batch)
                                            read changes()/counts per table
                                          ROLLBACK (never committed) ◄── dry-run
                                                          │
                                                          ▼
◄── DryRunReport { added: 0, overwritten: 0, deleted: {...} } ──┘
        │
[Preview UI: "N notes + M orphaned rows will be removed"]
        │
   user confirms ──────────────────────────►  cmd "delete_notes_apply"
                                                          │
                                                          ▼
                                          BEGIN Transaction
                                            same DELETE SQL
                                          COMMIT  (session.dirty = true)
                                                          │
                                                          ▼
                                          (later) save_archive()
                                                          │
                                                          ▼
                                          trim_db() — separate call, own
                                          transaction + FK-off + VACUUM,
                                          runs on EVERY save (D2-04)
                                                          │
                                                          ▼
                                          update_manifest (hash-last) → zip rebuild → atomic rename
```

### Recommended Project Structure
```
app/src-tauri/src/
├── db/
│   ├── notes.rs        # existing — Notes query
│   ├── delete.rs        # NEW — delete_notes(ids), dry_run_delete_notes(ids)
│   └── trim.rs           # NEW — trim_db(conn): the verbatim sweep + VACUUM
├── archive/
│   └── save.rs           # MODIFY — call trim::trim_db(&session.db_path) before update_manifest
├── error.rs               # MODIFY — add ArchiveError::TrimFailed, DeleteFailed, EmptySelection
└── commands (lib.rs / commands.rs) # NEW — delete_notes_dry_run, delete_notes_apply Tauri commands
```

### Pattern 1: Verbatim ordered sweep as a single batch, executed inside a `Transaction`
**What:** Port `JWLManager.py:3858-3935` as one `const TRIM_SQL: &str` (the `BEGIN...COMMIT` body only — PRAGMAs and VACUUM handled separately in Rust control flow, not inside the SQL string, because `Transaction` is already an explicit BEGIN/COMMIT and VACUUM cannot appear inside it).
**When to use:** Every `save_archive` call (D2-04), and inside the dry-run transaction for delete's preview.
**Example:**
```rust
// Source: JWLManager.py:3858-3935, ported per D2-01/D2-03
const TRIM_SQL: &str = "
    DELETE FROM InputField WHERE COALESCE(Value, '') = '';
    DELETE FROM Note WHERE COALESCE(Title, '') = '' AND COALESCE(Content, '') = ''
        AND NOT EXISTS (SELECT 1 FROM TagMap WHERE TagMap.NoteId = Note.NoteId);
    DELETE FROM TagMap WHERE
        (NoteId IS NOT NULL AND NoteId NOT IN (SELECT NoteId FROM Note))
        OR (PlaylistItemId IS NOT NULL AND PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem));
    DELETE FROM Tag WHERE TagId NOT IN (SELECT DISTINCT TagId FROM TagMap) AND Type > 0;
    CREATE TEMP TABLE TagMapNew AS SELECT TagMapId, PlaylistItemId, LocationId, NoteId, TagId,
        ROW_NUMBER() OVER (PARTITION BY TagId ORDER BY Position, TagMapId) - 1 AS Position FROM TagMap;
    DELETE FROM TagMap;
    INSERT INTO TagMap SELECT * FROM TagMapNew;
    DROP TABLE TagMapNew;
    DELETE FROM UserMark WHERE
        (UserMarkId NOT IN (SELECT UserMarkId FROM BlockRange WHERE UserMarkId IS NOT NULL)
        AND UserMarkId NOT IN (SELECT UserMarkId FROM Note WHERE UserMarkId IS NOT NULL))
        OR LocationId NOT IN (SELECT LocationId FROM Location WHERE LocationId IS NOT NULL);
    DELETE FROM BlockRange WHERE
        UserMarkId NOT IN (SELECT UserMarkId FROM UserMark WHERE UserMarkId IS NOT NULL);
    DELETE FROM PlaylistItem WHERE PlaylistItemId NOT IN (SELECT PlaylistItemId FROM TagMap);
    DELETE FROM PlaylistItemMarker WHERE PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem);
    DELETE FROM PlaylistItemLocationMap WHERE PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem);
    DELETE FROM PlaylistItemIndependentMediaMap WHERE PlaylistItemId NOT IN (SELECT PlaylistItemId FROM PlaylistItem);
    DELETE FROM PlaylistItemIndependentMediaMap WHERE IndependentMediaId NOT IN (SELECT IndependentMediaId FROM IndependentMedia);
    DELETE FROM PlaylistItemMarkerBibleVerseMap WHERE PlaylistItemMarkerId NOT IN (SELECT PlaylistItemMarkerId FROM PlaylistItemMarker);
    DELETE FROM PlaylistItemMarkerParagraphMap WHERE PlaylistItemMarkerId NOT IN (SELECT PlaylistItemMarkerId FROM PlaylistItemMarker);
    DELETE FROM Location WHERE
        LocationId NOT IN (SELECT LocationId FROM UserMark)
        AND LocationId NOT IN (SELECT LocationId FROM Note)
        AND LocationId NOT IN (SELECT LocationId FROM TagMap)
        AND LocationId NOT IN (SELECT LocationId FROM Bookmark)
        AND LocationId NOT IN (SELECT PublicationLocationId FROM Bookmark)
        AND LocationId NOT IN (SELECT LocationId FROM InputField)
        AND LocationId NOT IN (SELECT LocationId FROM PlaylistItemLocationMap);
    UPDATE Location SET Title = '' WHERE Title IS NULL;
";

pub fn trim_db(conn: &mut rusqlite::Connection) -> Result<(), ArchiveError> {
    // Session-level pragmas — safe to set outside a transaction, mirrors Python.
    conn.execute_batch("PRAGMA temp_store = 'MEMORY'; PRAGMA synchronous = 'OFF';
        PRAGMA journal_mode = 'MEMORY'; PRAGMA foreign_keys = 'OFF';")?;

    let tx = conn.transaction()?;
    tx.execute_batch(TRIM_SQL)?;   // CREATE TEMP TABLE + window fn all valid inside execute_batch
    tx.commit()?;                   // only after this does state persist

    conn.execute_batch("PRAGMA foreign_keys = 'ON'; PRAGMA synchronous = 'FULL';
        PRAGMA journal_mode = 'DELETE'; PRAGMA temp_store = 'DEFAULT';")?;
    conn.execute_batch("VACUUM;")?; // MUST be outside any transaction — SQLite hard error otherwise
    Ok(())
}
```

### Pattern 2: Dry-run via a `Transaction` that is never committed
**What:** Run the exact delete + trim SQL, capture row-count deltas, drop the `Transaction` without `.commit()` — `rusqlite::Transaction::drop` issues `ROLLBACK` automatically (`drop_behavior()` defaults to `DropBehavior::Rollback`) `[CITED: docs.rs/rusqlite/latest/rusqlite/struct.Transaction.html]`.
**When to use:** `delete_notes_dry_run` command; reused pattern for Phase 4 (downgrade preview) and Phase 5 (merge preview) per D2-07.
**Example:**
```rust
pub fn dry_run_delete_notes(conn: &mut Connection, ids: &NonEmptyNoteIds) -> Result<DryRunReport, ArchiveError> {
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")?;
    let tx = conn.transaction()?;
    let placeholders = std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM Note WHERE NoteId IN ({placeholders})");
    let mut stmt = tx.prepare(&sql)?;
    let deleted_notes = stmt.execute(rusqlite::params_from_iter(ids.iter()))?;
    tx.execute_batch(TRIM_SQL)?; // reuse same sweep; changes() per statement can be summed if per-table counts needed
    let total_changes = tx.changes(); // or query per-table counts before drop
    // tx dropped here WITHOUT .commit() -> automatic ROLLBACK, working copy untouched
    Ok(DryRunReport { deleted: total_changes, added: 0, overwritten: 0 })
}
```
**Note on per-table counts:** `Transaction::changes()` (via the underlying `Connection`) returns cumulative rows changed by the *last* statement, not a running total across the whole batch. For a `DryRunReport` that breaks down deleted counts per table (Notes vs. orphan sweep), either (a) run each `DELETE` as its own `tx.execute(...)` call (not inside one `execute_batch`) and read `tx.changes()`/`stmt.execute()`'s own return value after each, or (b) `SELECT COUNT(*)` per table before and after within the same transaction. Recommend (a): breaking `TRIM_SQL`'s DELETE statements into individually-executed `tx.execute()` calls (not `execute_batch`) inside `trim.rs` gives per-table counts "for free" and doesn't change behavior — `execute_batch` vs. sequential `execute()` calls inside the same open `Transaction` are semantically identical for DML (both run inside the one BEGIN/COMMIT), just individually addressable. **Recommend restructuring `trim.rs`'s internals as a `Vec<(&str, &str)>` of (label, sql) pairs run via a loop of `tx.execute(sql, [])?`, summed per label** — this ALSO gives dry-run a free per-table breakdown by reusing the same function with a "count only" flag, and gives SAFE-04's rollback test a table-by-table assertion surface.

### Anti-Patterns to Avoid
- **VACUUM inside a transaction:** SQLite raises `SQLITE_ERROR: cannot VACUUM from within a transaction` — this is a hard SQLite constraint (documented behavior, not a Python-app quirk) `[CITED: sqlite.org/lang_vacuum.html — "the VACUUM command ... cannot be used within a transaction"]`. Never call `VACUUM` on a `Transaction` object or inside `execute_batch` alongside `BEGIN`.
- **Relying on `foreign_keys` defaulting OFF:** Phase 3's `03-02-SUMMARY.md` empirically found this build's bundled SQLite does NOT default `foreign_keys` to OFF — always set it explicitly before opening the transaction (pragma changes are a documented no-op inside an active transaction, per `upgrade.rs`'s own comment).
- **Cascading delete logic duplicated in `delete.rs`:** D2-05 explicitly assigns orphan cleanup to `trim_db` on save, not to the delete command. Do not hand-write UserMark/BlockRange cascade beyond the Note's OWN direct links inside `delete_notes`.
- **Disabled-button-only empty-selection guard:** SAFE-03/D2-06 requires the Tauri command itself reject an empty id list via a typed error/newtype BEFORE any DB access — a disabled button alone is explicitly rejected as insufficient.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Preview-without-mutating | Custom scratch-copy-and-diff harness | `rusqlite::Connection::transaction()` + never `.commit()` | Zero extra I/O, and guarantees preview/apply run byte-identical SQL — Rust's own drop-rolls-back semantics do the work |
| Non-empty-collection type safety | Runtime `if ids.is_empty() { return Err(...) }` scattered at every call site | A `NonEmptyNoteIds` newtype constructed via `TryFrom<Vec<i64>>` that fails at the IPC boundary before the command body runs | SAFE-03 requires "impossible by construction" — a newtype that cannot be built empty is the idiomatic Rust way to make invalid states unrepresentable, matching the phase's own explicit language |
| Row-count diffing for dry-run | Manual before/after `SELECT COUNT(*)` snapshots per table stored in temp vars | `tx.changes()` (or per-statement `execute()` return value) inside the transaction, then drop | rusqlite already tracks this via SQLite's own `sqlite3_changes()` — no need to hand-roll counting |

**Key insight:** This phase's complexity is almost entirely "port this exact SQL and don't get the order or the FK/VACUUM sequencing wrong" — there is very little novel engineering. Resist the urge to redesign `trim_db`'s logic; D2-01 and prior Phase 3 experience both say verbatim-order porting is the safe path.

## Common Pitfalls

### Pitfall 1: VACUUM issued while a transaction (or the dry-run's rolled-back transaction) is still open
**What goes wrong:** `SQLITE_ERROR` at runtime, or (worse, if silently swallowed) a trim that appears to succeed but never reclaims space.
**Why it happens:** `trim_db`'s Python source runs VACUUM as later statements in the SAME `executescript` call, but AFTER the `COMMIT;` line — the script implicitly ends the transaction first. A naive Rust port that wraps the WHOLE thing (PRAGMAs + DML + VACUUM) inside one `rusqlite::Transaction` will fail on VACUUM.
**How to avoid:** Structure as: (1) session pragmas, (2) `conn.transaction()` → DML → `tx.commit()`, (3) restore pragmas, (4) separate `conn.execute_batch("VACUUM;")` call with NO open transaction.
**Warning signs:** `rusqlite::Error::SqliteFailure` with message containing "cannot VACUUM".

### Pitfall 2: Dry-run's VACUUM
**What goes wrong:** If the dry-run path is implemented as "run trim_db but roll back," and `trim_db` includes a VACUUM call, VACUUM is NOT transactional in SQLite — it CANNOT be rolled back, meaning a "dry run" would permanently rewrite the database file even though the logical DELETEs were rolled back.
**Why it happens:** Naive reuse of the full `trim_db` function (DML + VACUUM) for both the real save-time trim and the delete dry-run.
**How to avoid:** The dry-run must ONLY run the DML portion (DELETE/orphan sweep) inside the rolled-back `Transaction`, and must NEVER call VACUUM. Split `trim_db` into `trim_sweep(tx)` (DML only, callable from both dry-run and real trim) and `trim_db(conn)` (sweep + VACUUM, save-path only per D2-04).
**Warning signs:** A "cancelled" dry-run still shrinks the file size on disk.

### Pitfall 3: `ROW_NUMBER() OVER (...)` and `CREATE TEMP TABLE ... AS SELECT` inside `execute_batch`
**What goes wrong:** None expected — rusqlite's bundled SQLite (0.40, compiled from a recent SQLite source tree) supports window functions (added SQLite 3.25.0, 2018) and `CREATE TABLE ... AS SELECT` (core SQLite since inception) with no special handling needed `[CITED: sqlite.org/windowfunctions.html; sqlite.org/lang_createtable.html]`. This is flagged LOW-effort-to-verify but HIGH-consequence if wrong, so: **verify empirically in Wave 0** by running the exact `TagMapNew` block against a fixture DB and asserting `TagMap` positions are 0-based and gap-free per `TagId` partition afterward — do not trust this research claim alone for a data-integrity-critical statement.
**Why it happens:** Not a real risk given `bundled`, but the codebase has already been burned once by an "assumed default" (`foreign_keys`) turning out wrong — treat every ported PRAGMA/feature-support claim with the same skepticism until a test proves it against THIS build's bundled SQLite.
**How to avoid:** A dedicated `trim_reindexes_tag_positions` test (see Validation Architecture below) that seeds TagMap rows with gapped/duplicate `Position` values per `TagId`, runs the sweep, and asserts contiguous 0-based positions per partition.
**Warning signs:** Any `rusqlite::Error::SqliteFailure` mentioning `near "OVER"` or `no such function: ROW_NUMBER` — would indicate an unexpectedly old bundled SQLite, contradicting the `bundled` feature's guarantee.

### Pitfall 4: TagMap re-densify's `INSERT INTO TagMap SELECT * FROM TagMapNew` column order
**What goes wrong:** `SELECT *` from `TagMapNew` returns columns in the order they were declared in the `CREATE TEMP TABLE ... AS SELECT` statement (`TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position`), which must exactly match `TagMap`'s real column order for a bare `INSERT INTO TagMap SELECT *` (no explicit column list) to land values in the right columns.
**Why it happens:** This is exactly how the Python original does it too (also relies on matching order) — so it's a pre-existing sharp edge being ported, not a new one, but it deserves an explicit test because a wrong port order here would silently scramble every user's tag associations.
**How to avoid:** Confirm `TagMap`'s actual schema column order (via `PRAGMA table_info(TagMap)` against a real v16 fixture) matches `TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position` before trusting the bare `SELECT *`. If planner wants extra safety, port it as an explicit column list instead of `SELECT *` — behaviorally identical, more assertion-visible in a test.
**Warning signs:** A test asserting `TagMap.NoteId`/`TagMap.TagId` for a specific known row are swapped or `NULL` post-trim.

### Pitfall 5: Empty-selection "impossible by construction" implemented only in Rust, not enforced at the Tauri boundary
**What goes wrong:** If the frontend can still invoke the raw `invoke("delete_notes_apply", { ids: [] })` and the Rust command signature takes `Vec<i64>` (deserializing an empty array successfully) with the non-empty check as the FIRST line of the function body, that is a runtime check, not a construction-time guarantee — SAFE-03 explicitly wants the type itself to reject it.
**Why it happens:** Serde will happily deserialize `[]` into any `Vec<T>` parameter; Tauri commands receive plain deserialized types, so a `Vec<i64>` parameter type provides no compile-time or IPC-boundary protection.
**How to avoid:** Use a custom `Deserialize` impl (or a wrapper with `#[serde(try_from = "Vec<i64>")]`) for `NonEmptyNoteIds` so an empty array fails to deserialize AT the IPC boundary (surfaces as a Tauri invoke rejection before the command body runs), not inside it.
**Warning signs:** A test that calls the command with `ids: []` and finds the command function body executed (even if it then returns an error) rather than the deserialization itself failing.

## Code Examples

### `NonEmptyNoteIds` — impossible-by-construction empty guard (SAFE-03/D2-06)
```rust
// Source: pattern derived from serde's documented try_from container attribute
// (serde.rs/container-attrs.html#try_from) + this codebase's existing
// ArchiveError/ErrorDto typed-boundary convention.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[serde(try_from = "Vec<i64>")]
#[ts(export, export_to = "../../src/bindings/NonEmptyNoteIds.ts")]
pub struct NonEmptyNoteIds(Vec<i64>);

impl TryFrom<Vec<i64>> for NonEmptyNoteIds {
    type Error = String; // maps to a deserialization error at the IPC boundary
    fn try_from(ids: Vec<i64>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            Err("selection must not be empty".to_string())
        } else {
            Ok(NonEmptyNoteIds(ids))
        }
    }
}

impl NonEmptyNoteIds {
    pub fn iter(&self) -> impl Iterator<Item = &i64> { self.0.iter() }
    pub fn len(&self) -> usize { self.0.len() }
}
```
Frontend still ALSO gates the delete button on `selection.length >= 1` (defense-in-depth per D2-06's "AND the UI only enables the flow with ≥1 selected") — both layers required, neither alone satisfies SAFE-03.

### Parameterized IN-clause (SAFE-02)
```rust
// rusqlite has no native array-bind for IN(...); the standard, safe pattern
// is to build `?,?,?` placeholders and bind via params_from_iter — NEVER
// interpolate the ids themselves into the SQL string.
let placeholders: String = std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
let sql = format!("DELETE FROM Note WHERE NoteId IN ({placeholders})");
tx.execute(&sql, rusqlite::params_from_iter(ids.iter()))?;
```
Source pattern: rusqlite docs `params_from_iter` — the only part of the SQL string that is dynamic is the placeholder COUNT (a safe integer, never user data), identical in spirit to `upgrade.rs`'s `column_exists`'s dynamic-but-safe table-name interpolation.

## State of the Art

| Old Approach (Python) | Current Approach (Rust port) | When Changed | Impact |
|--------------------|------------------|---------------|--------|
| `trim_db` wrapped in bare `try/except Exception: crash_box(); sys.exit()` | Typed `ArchiveError` propagated, transaction rolled back on any `Err`, no process exit | This phase (D2-02) | Matches SAFE-05 posture already established in `upgrade.rs` — a trim failure no longer kills the whole app or (worse) commits a partial sweep |
| No dry-run in Python (`trim_db` and delete both mutate directly) | Dry-run via rolled-back `Transaction` before every destructive op | This phase (D2-07, new capability) | New safety net Python never had; designed for reuse in Phase 4/5 |

**Deprecated/outdated:** N/A — this is a from-scratch Rust port, no prior Rust implementation to deprecate.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `TagMap`'s real column order in a v16 fixture is exactly `TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position` (needed for the bare `SELECT *` re-densify insert to land correctly) | Pitfall 4 | If wrong, the re-densify silently scrambles TagMap associations (tag-to-note/location/playlist links) — the exact kind of "silently deletes/corrupts user data" the Core Value forbids. MUST be verified against a real fixture in Wave 0, not assumed from the Python source's implicit trust in `SELECT *` |
| A2 | rusqlite 0.40's `bundled` SQLite supports `ROW_NUMBER() OVER (PARTITION BY ... ORDER BY ...)` and `CREATE TEMP TABLE ... AS SELECT` with no extra rusqlite feature flags | Pattern 1, Pitfall 3 | If SQLite version bundled is old enough to lack window functions (would be surprising for a `bundled`-feature crate at 0.40, but not independently verified via `cargo tree` or a smoke query in this research pass), the whole re-densify statement fails outright — LOW likelihood, MEDIUM consequence, cheap to verify (one test) |
| A3 | `Transaction::changes()` / statement-level `execute()` return values, run per-statement rather than via one `execute_batch`, give an equivalent semantic result to the Python original's single `executescript` — no ordering or intermediate-state difference | Pattern 2 | If wrong, a per-statement-executed trim could theoretically see different transaction-visibility semantics than a single `executescript` batch; SQLite's transaction model makes this extremely unlikely (both are the same BEGIN...COMMIT), but not empirically tested in this research pass |

## Open Questions

1. **Exact `DryRunReport` shape for "overwritten"**
   - What we know: CONTEXT.md's D2-07 defines the shape as `{ added, overwritten, deleted }`, generalized for reuse by Phase 4 (downgrade) and Phase 5 (merge). For Phase 2's delete-only use, `added` and `overwritten` will always be `0`.
   - What's unclear: Whether `deleted` should be a single count or a per-table breakdown (`{ Note: 3, TagMap: 5, Location: 1, ... }`). CONTEXT.md leaves the exact shape to Claude's Discretion.
   - Recommendation: Per-table breakdown (a `HashMap<String, i64>` or a small fixed struct with one field per swept table) — richer for the confirm UI ("3 Notes, 5 tag links, 1 unused Bible location will be removed") and trivially satisfies the single-count case too (sum the map). Costs nothing extra given Pattern 2's per-statement `execute()` restructuring already produces per-table counts.

2. **Should `trim_sweep` (DML-only, no VACUUM) be a shared function called by BOTH `trim_db` (save path) and the dry-run, or should dry-run only simulate the DELETE-from-Note step and estimate orphans separately?**
   - What we know: D2-05 says the delete command itself only deletes the Note (+ its direct UserMark/BlockRange), and D2-07 says dry-run must account for "the selected Notes + the orphans trim_db would then remove" — implying the dry-run DOES need to run the full sweep to get an accurate orphan count.
   - What's unclear: Whether running the FULL multi-table sweep (Tag, PlaylistItem cascade, Location, etc.) inside dry-run is wanted for EVERY delete preview, even though most of those tables are unrelated to a Notes-only delete in this phase.
   - Recommendation: Run the full sweep in dry-run (it's cheap — it's exactly what save would do anyway) so the preview number is always exactly correct, never an approximation the planner has to keep in sync with the real sweep.

## Environment Availability

No external dependencies beyond what Phase 1/3 already established (rusqlite bundled, no OS service, no new CLI). Skipped — code/config-only phase, per the section's own skip condition.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` via `cargo test`, integration tests in `app/src-tauri/tests/*.rs` `[VERIFIED: existing schema_upgrade_tests.rs]` |
| Config file | none — plain `cargo test`, no custom harness |
| Quick run command | `cargo test --package jwlmanager --test delete_tests -- --nocapture` (per-file, once created) |
| Full suite command | `cargo test --workspace` (mirrors CI's existing four-leg matrix) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| EDIT-01 | Delete selected Notes removes exactly the targeted rows | integration | `cargo test test_delete_notes_removes_selected_rows` | ❌ Wave 0 (`tests/delete_tests.rs`) |
| SAFE-01 | Dry-run returns an accurate preview and mutates nothing | integration | `cargo test test_dry_run_delete_does_not_mutate` | ❌ Wave 0 |
| SAFE-02 | All delete/trim SQL is parameterized (no string-built IN with raw ids) | static/manual review + integration (SQL-injection-shaped id smoke test) | `cargo test test_delete_rejects_sql_injection_shaped_input` | ❌ Wave 0 |
| SAFE-03 | Empty selection cannot reach the delete path | integration | `cargo test test_empty_selection_fails_deserialization` | ❌ Wave 0 |
| SAFE-04 | Mid-operation failure leaves the archive unchanged (rollback) | integration | `cargo test test_delete_rollback_on_forced_failure` (mirrors `test_upgrade_rollback_leaves_original_version` at `schema_upgrade_tests.rs:289`) | ❌ Wave 0 |
| ARCH-04 | trim_db sweeps orphans, re-densifies tags, VACUUMs, on every save | integration | `cargo test test_trim_sweeps_orphans_and_vacuums` + `cargo test test_trim_reindexes_tag_positions` | ❌ Wave 0 |
| QA-02 | Round-trip semantic equivalence after delete+save+reopen | integration | `cargo test test_delete_round_trip_semantic_equivalence` (uses existing `normalized_table_rows` harness from Phase 1) | ❌ Wave 0 (harness exists per Phase 1; new fixture + test needed) |

### Sampling Rate
- **Per task commit:** targeted `cargo test <specific_test_name>`
- **Per wave merge:** `cargo test --workspace` + `cargo clippy --workspace -- -D warnings` (existing unwrap-ban gate per CI)
- **Phase gate:** Full suite green before `/gsd-verify-work`; additionally re-run the real v14 round-trip fixture noted in CONTEXT.md's Specific Ideas (post-trim archive should be same-size-or-smaller and still Python-app-acceptable)

### Wave 0 Gaps
- [ ] `app/src-tauri/tests/trim_tests.rs` — covers ARCH-04 (sweep correctness, tag re-densify, VACUUM-outside-transaction, FK-off-forced)
- [ ] `app/src-tauri/tests/delete_tests.rs` — covers EDIT-01, SAFE-01, SAFE-02, SAFE-03, SAFE-04
- [ ] Fixture: a v16 test DB seeded with a Note that owns a UserMark+BlockRange, a TagMap entry, and a Location referenced ONLY by that Note (so deleting it exercises every branch of the sweep) — extend Phase 1/3's existing fixture generator
- [ ] `changes()`-per-table counting refactor in `trim.rs` internals (Pattern 2) — needed before `DryRunReport`'s per-table breakdown can be tested

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | desktop app, no auth surface |
| V3 Session Management | no | N/A |
| V4 Access Control | no | single-user local file, no access control boundary |
| V5 Input Validation | yes | Parameterized SQL via `rusqlite::params_from_iter` for all id-list operations (SAFE-02); `NonEmptyNoteIds` typed rejection at IPC boundary (SAFE-03) |
| V6 Cryptography | no | not applicable to this phase |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| SQL injection via unparameterized IN-clause id list | Tampering | `params_from_iter` with `?` placeholders (Code Examples above) — NEVER `format!` the raw ids into the SQL string |
| Partial/corrupted mutation on crash mid-delete | Tampering / Repudiation of data integrity | `rusqlite::Transaction` (rollback-on-drop) ensures no partial state persists; this is the SAFE-04 guarantee itself |

## Project Constraints (from CLAUDE.md)

- No f-string/format-string SQL interpolation — parameterize (matches SAFE-02, enforced via `params_from_iter`, not string formatting of ids)
- Save is not byte-preserving; parity verified semantically only, never byte-diffing (QA-02's normalized-table-equivalence approach already satisfies this)
- MIT licensing constraint — not implicated by this phase (no new dependencies, no code lineage concerns)
- Rust project style already established (typed errors via `thiserror`, `ArchiveError`→`ErrorDto` boundary mapping, `unwrap`/`expect` banned on archive-data paths per existing clippy gate) — Phase 2 code must follow the same conventions as `upgrade.rs`/`notes.rs`

## Sources

### Primary (HIGH confidence)
- `JWLManager.py:3858-3935` (`trim_db`) and `:1245` (call site) — read directly, exact statements and order transcribed above
- `app/src-tauri/src/archive/upgrade.rs` — direct template for transactional FK-off-forced typed-error rollback pattern
- `app/src-tauri/src/archive/save.rs` — hash-last save sequence, confirms where trim hooks in (before `update_manifest`)
- `app/src-tauri/src/error.rs`, `src/session.rs`, `src/db/notes.rs` — existing conventions (ArchiveError/ErrorDto, ArchiveSession fields, query-module shape)
- `app/src-tauri/tests/schema_upgrade_tests.rs` (line numbers for `test_upgrade_rollback_leaves_original_version` etc.) — rollback test style to mirror
- `app/src-tauri/Cargo.toml:22` — `rusqlite = { version = "0.40", features = ["bundled"] }` confirmed via direct read
- `.planning/phases/02-safe-delete/02-CONTEXT.md` — all D2-01..D2-10 locked decisions read in full

### Secondary (MEDIUM confidence)
- SQLite official docs on VACUUM-cannot-run-in-transaction and window function support since 3.25 — well-established, stable SQLite behavior, cited from training knowledge of the (unchanging) SQLite language spec, consistent with the `bundled` feature guaranteeing a modern SQLite
- rusqlite `Transaction`/`Connection::transaction()` rollback-on-drop semantics — standard, long-documented rusqlite behavior (`DropBehavior::Rollback` default), consistent with `upgrade.rs`'s own working use of the identical pattern in this exact codebase

### Tertiary (LOW confidence)
- None — no unverified WebSearch-only claims were needed for this phase; all critical claims were resolvable by reading the actual Python source, the actual Rust codebase, and stable/unchanging SQLite core behavior

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new dependencies, all APIs already proven working in this exact codebase (`upgrade.rs`)
- Architecture: HIGH — direct template exists (`upgrade.rs` + `save.rs`), CONTEXT.md's D2-01..D2-10 are prescriptive
- Pitfalls: HIGH for VACUUM/transaction-boundary issues (stable SQLite behavior); MEDIUM for the TagMap column-order and window-function-support claims (flagged as A1/A2, cheap to verify via Wave 0 tests, not independently smoke-tested in this research pass)

**Research date:** 2026-07-21
**Valid until:** 2026-08-20 (30 days — stable domain, no external API surface, no fast-moving dependency)
</content>
