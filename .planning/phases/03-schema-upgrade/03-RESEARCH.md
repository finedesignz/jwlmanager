# Phase 3: Schema Upgrade - Research

**Researched:** 2026-07-20
**Domain:** SQLite DDL migration (Rust/rusqlite), synthetic fixture generation, Tauri archive-open pipeline
**Confidence:** MEDIUM (HIGH on rusqlite/SQLite mechanics via inspected `res/blank`; MEDIUM-LOW on v12/v13 correctness — see Finding 1, the phase's central open question)

## Summary

Porting `JWLManager.py:1016-1075`'s `upgrade_schema` to Rust is mechanically straightforward: one `ALTER`/`CREATE`/`INSERT...SELECT`/`DROP`/`RENAME`/index sequence, run inside a `rusqlite` transaction, replacing the Python `except: pass` with a typed error (D3-02). The two real risks are not in the Rust port itself but in what the port *implicitly claims*: (1) whether a single transformation is actually correct for v12/v13 archives that may be missing more than the v16↔v14 delta, and (2) whether synthesizing v12-v15 fixtures by reverse-mutating the v16 `res/blank` seed produces database shapes JW Library ever actually produced, or a false oracle.

Direct inspection of `res/blank` (extracted and queried in this session) confirms: `PRAGMA foreign_keys` defaults to **0** (off) in this codebase, `PRAGMA user_version` is a page-header write that participates in the surrounding transaction (rollback-safe), and 6+ tables (`InputField`, `Bookmark`, `Note`, `UserMark`, `TagMap`, `PlaylistItemLocationMap`) declare `FOREIGN KEY ... REFERENCES Location(LocationId)` in their DDL — but since `foreign_keys` is off, `DROP TABLE Location` will not be blocked or cascade-checked against them. This must stay off (or be explicitly scoped) during the upgrade, and the plan must decide explicitly rather than leave it to rusqlite's default (which happens to already match).

**Primary recommendation:** Port the DDL as one static, embedded SQL string executed via `execute_batch` inside an explicit `BEGIN`/`COMMIT`/`ROLLBACK` (not relying on `execute_batch`'s implicit behavior — see Finding 3), add an idempotent `ADD COLUMN` guard via `PRAGMA table_info`, and — critically — do NOT claim verified v12/v13 support in REQUIREMENTS/ROADMAP language beyond "accepted and upgraded via the same code path as v14/v15"; flag the v12/v13 semantic-correctness gap explicitly in the plan and its verification section, matching D3-11's real-archive acceptance bar for v14 specifically (the owner has no v12/v13 samples to verify against).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Schema version gate widening (12-16 accept, ≤11/≥17 reject) | API/Backend (Rust core, `archive/mod.rs`) | — | Gate already lives in `open_and_validate`; this phase widens the existing constant/match, no new tier |
| DDL upgrade execution | API/Backend (Rust core, new `db/schema.rs` or `archive/upgrade.rs`) | Database/Storage (SQLite DDL semantics) | Runs against the extracted working-copy DB via rusqlite; owns transaction boundary |
| Fixture synthesis (v11-v15 + v16) | Test harness (`tests/common/mod.rs`) | — | Extends existing `res/blank`-seeded generator; not shipped in the app binary |
| Error surface for reject/upgrade-fail | API/Backend (`error.rs` typed variants) → Frontend (existing `ErrorDto` + i18n key) | — | Reuses Phase 1's two-layer error mechanism (D3-08); no new IPC surface |
| Real-archive acceptance (D3-11) | Local dev tooling (`examples/roundtrip.rs`, `JWLM_REAL_ARCHIVE` env gate) | — | Never CI, never committed; manual verification step only |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `rusqlite` | 0.40 (already pinned, `bundled` feature) [VERIFIED: app/src-tauri/Cargo.toml] | SQLite driver, `execute_batch`, `query_row`, `Transaction` | Already the project's sole DB layer since Phase 1; `bundled` feature vendors a modern SQLite (post-3.26, so `legacy_alter_table` defaults OFF — see Finding 4) |

No new crates needed for this phase — it is pure DDL execution against the existing connection.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `execute_batch` for the whole DDL string | Individual `execute()` calls per statement | `execute_batch` is simpler and matches the Python `executescript` shape 1:1, but does NOT let you interleave a `PRAGMA table_info` check mid-script (D3-04 needs this) — so the recommended shape is: explicit txn + `table_info` check + conditional `ALTER` via `execute()` + a single `execute_batch` for the rest, all inside one transaction. See Code Examples. |

**Installation:** none — no new dependencies.

## Package Legitimacy Audit

Not applicable — this phase introduces zero new external packages/crates.

## Architecture Patterns

### System Architecture Diagram

```
open_and_validate(path)
  │
  ├─ extract_zip_slip_safe → temp_dir
  ├─ read manifest.json → schema_version (manifest-declared)
  │     │
  │     ├─ version <= 11 ──────────────► ArchiveError::SchemaTooOld { version }
  │     ├─ version >= 17 ──────────────► ArchiveError::SchemaTooNew { version }
  │     └─ 12 <= version <= 16 ─────────┐
  │                                      ▼
  ├─ open rusqlite::Connection(db_path) │
  ├─ read PRAGMA user_version ──────────┤ (cross-check vs manifest; mismatch = own error path, unchanged from Phase 1's dual-check pattern)
  │                                      ▼
  ├─ IF pragma_version < 16:  upgrade::upgrade_to_v16(&mut conn)
  │     │
  │     ├─ BEGIN IMMEDIATE
  │     ├─ PRAGMA table_info(Location) → has_specialty/has_edition already?
  │     ├─ conditionally ALTER TABLE Location ADD COLUMN Specialty/Edition
  │     ├─ execute_batch(CREATE Location_new ... DROP ... RENAME ... 3x CREATE INDEX)
  │     ├─ execute("PRAGMA user_version = 16")  ⚠ inside same txn (Finding 3)
  │     ├─ on any Err → ROLLBACK, return ArchiveError::SchemaUpgradeFailed
  │     └─ COMMIT
  │                                      ▼
  ├─ re-read PRAGMA user_version (must now be 16) → assert, feed into ManifestMeta.schema_version
  ├─ query_notes(&conn, &catalog)        (existing Phase 1 step, unchanged)
  └─ ArchiveSession { manifest.schema_version: 16, ... }
```

### Recommended Project Structure
```
app/src-tauri/src/
├── archive/
│   ├── mod.rs           # open_and_validate — widen SUPPORTED_SCHEMA_VERSION gate to a range, call upgrade after PRAGMA check
│   └── upgrade.rs        # NEW — upgrade_to_v16(&Connection) -> Result<(), ArchiveError>; the ported DDL
├── error.rs               # add SchemaTooOld { version }, SchemaTooNew { version }, SchemaUpgradeFailed { reason } variants
└── db/
    └── ...                # unchanged
app/src-tauri/tests/
├── common/mod.rs           # extend generate_v16_fixture → generate_versioned_fixture(version: i64) or per-version fns
├── fixtures.rs             # extend with v12/v13/v14/v15/v11-reject fixture existence tests
└── schema_upgrade_tests.rs # NEW — upgrade round-trip tests per version, idempotency test, D3-04 already-has-columns test
```

### Pattern 1: Explicit transaction wrapping `execute_batch` + a pre-check
**What:** rusqlite's `Connection::execute_batch` does NOT begin its own transaction — statements run in autocommit mode unless you wrap them. Use `conn.transaction()` (rusqlite's `Transaction` guard, which rolls back on `Drop` unless `.commit()` is called) around the whole sequence, and issue the batch DDL via `tx.execute_batch(...)` (available on `Transaction` too, since it derefs to `Connection`).

**When to use:** Any multi-statement DDL that must be all-or-nothing (D3-03).

**Example:**
```rust
// Source: rusqlite docs (Connection::transaction, Connection::execute_batch) — [CITED: docs.rs/rusqlite]
pub fn upgrade_to_v16(conn: &mut rusqlite::Connection) -> Result<(), ArchiveError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current >= 16 {
        return Ok(()); // D3-05: idempotent no-op
    }

    let tx = conn.transaction()?; // BEGIN DEFERRED by default

    // D3-04: detect already-present columns before ALTER (SQLite errors on
    // ADD COLUMN of an existing name — "duplicate column name: Specialty").
    let has_specialty = column_exists(&tx, "Location", "Specialty")?;
    let has_edition = column_exists(&tx, "Location", "Edition")?;
    if !has_specialty {
        tx.execute("ALTER TABLE Location ADD COLUMN Specialty TEXT", [])?;
    }
    if !has_edition {
        tx.execute("ALTER TABLE Location ADD COLUMN Edition TEXT", [])?;
    }

    tx.execute_batch(
        "CREATE TABLE Location_new ( ... same DDL as JWLManager.py:1026-1062 ... );
         INSERT INTO Location_new SELECT LocationId, BookNumber, ChapterNumber, DocumentId,
             Track, IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, NULL, NULL
         FROM Location;
         DROP TABLE Location;
         ALTER TABLE Location_new RENAME TO Location;
         CREATE INDEX IF NOT EXISTS IX_Location_KeySymbol_MepsLanguage_BookNumber_ChapterNumber
             ON Location(KeySymbol, MepsLanguage, BookNumber, ChapterNumber);
         CREATE INDEX IF NOT EXISTS IX_Location_MepsLanguage_DocumentId
             ON Location(MepsLanguage, DocumentId);
         CREATE UNIQUE INDEX IF NOT EXISTS IX_Location_Media
             ON Location(KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type,
                 COALESCE(Specialty, ''), COALESCE(Edition, ''));"
    )?;

    // PRAGMA user_version IS transactional in SQLite (it's a header-page
    // write like any other page mutation) — safe inside the same txn.
    tx.pragma_update(None, "user_version", 16)?;

    tx.commit()?; // explicit; Drop alone would ROLLBACK
    Ok(())
}

fn column_exists(conn: &rusqlite::Connection, table: &str, col: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?; // table_info column 1 = name
        if name.eq_ignore_ascii_case(col) {
            return Ok(true);
        }
    }
    Ok(false)
}
```
Note: `PRAGMA` statements cannot be parameterized/interpolated safely with table names from user input via `?` bind params — but `table` here is always a hardcoded literal (`"Location"`), never user data, so the `format!` is safe and is not a SAFE-02 violation (matches the CONTEXT.md D3-11 note that the upgrade DDL has no user-value interpolation).

### Anti-Patterns to Avoid
- **Relying on `execute_batch`'s autocommit + hoping for atomicity:** Without an explicit `Transaction`, a mid-script failure leaves whatever DDL already executed committed (SQLite's `execute_batch` runs each statement in its own implicit transaction unless one is already open). This exactly reproduces the D3-03 hazard the Python `except: pass` was hiding.
- **Trusting the manifest's `schemaVersion` and the DB's `PRAGMA user_version` to always agree pre-upgrade:** Phase 1's gate already checks both independently (`archive/mod.rs:70` and `:79`). Keep both checks; widen both from `== 16` to a range, and validate they agree with each other before deciding whether to upgrade (a manifest saying 14 with a DB actually at 16 — or vice versa — is a corruption signal, not a normal case, and should be its own typed error rather than silently trusting one source).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Multi-statement DDL execution | Manual per-statement string splitting on `;` | `rusqlite::Connection::execute_batch` | It already handles this correctly (delegates to `sqlite3_exec`); splitting on `;` yourself breaks on the `CHECK` constraints' semicolon-free-but-comma-heavy bodies and is needless surface area |
| Column-existence detection | Regex/string-matching `sqlite_master.sql` | `PRAGMA table_info(table)` | Structured, versioned SQLite API; matching DDL text is fragile against formatting differences (the CHECK constraints alone are ~40 lines of free-form whitespace) |

**Key insight:** Every piece of this phase already has a direct, non-hand-rolled primitive in rusqlite/SQLite (`execute_batch`, `PRAGMA table_info`, `Connection::transaction`, `pragma_update`). The only genuinely hard-and-not-already-solved problem is Finding 1 (below) — and that is a *data* question (what did old JW Library actually write), not a code problem a library can solve.

## Common Pitfalls

### Pitfall 1 (CRITICAL, phase-defining): Single transformation may be wrong for v12/v13
**What goes wrong:** The plan assumes (per D3-01, matching the Python app) that ANY archive below v16 gets exactly the same `Location` rebuild. This is provably correct for **v14→v16** (FUNCTIONALITY-SPEC.md documents the exact delta: `Specialty`/`Edition` columns + `IX_Location_Media` index). It is **NOT independently verified for v12 or v13** by anything in this repo, the Python source, or this research pass.

**Why it happens:** The Python `upgrade_schema` was written once, empirically, against whatever archives its author had — almost certainly v14 or v15 samples, given how JW Library versioned schemas historically. It applies the same DDL unconditionally to `user_version < 16` without ever branching on the *actual* value. Two possibilities, and the codebase gives no way to distinguish them:
  1. v12/v13 have the exact same table shape as v14 (only differing in some column the Location rebuild doesn't touch) — in which case one transformation really is correct for all of them.
  2. v12/v13 are missing something ELSE (e.g., a different column, table, or index added between v12→v14) that the Python script never touches — in which case archives at v12/v13 that go through `upgrade_schema` end up at `PRAGMA user_version = 16` while still missing whatever v13→v14 or v12→v13 actually changed. This would be a **silent partial upgrade masquerading as success** — arguably worse than the `except: pass` defect D3-02 already flags, because it wouldn't even throw.

**How to avoid:** The research pass found no schema changelog, no v12/v13 sample archives (owner's real-archive survey found only v14×19 and v16×13 — zero v12/v13 samples exist to check against), and no upstream JW Library schema documentation. **Recommendation for the plan:**
- Implement the port as specified (D3-01, single transformation) — it is still the best available evidence-based default and matches D3-11's philosophy of trusting what real archives prove.
- BUT do not let REQUIREMENTS/ROADMAP success-criteria language ("Opening a v12...v13...archive succeeds and data displays correctly") be verified ONLY via synthetic fixtures that were *constructed by assuming* the same delta applies (see Pitfall 2 — this would be circular). Fixture-based tests for v12/v13 prove the CODE PATH executes without SQL errors on a fixture shaped like v14; they cannot prove semantic correctness against real v12/v13 data because none exists to check against.
- The plan's verification section should explicitly state this limitation rather than imply v12/v13 are proven to the same bar as v14 (D3-11's real-archive gate). Suggested language: "v12/v13 support is implemented via the same code path as v14/v15 per D3-01, verified against synthetic fixtures only — semantic correctness against real v12/v13 archives is unverified due to no sample data being available (open item, not a blocker: D3-01 explicitly accepts this tradeoff)."
- If the owner (or any user) later surfaces a real v12/v13 archive, it should go through the exact same `JWLM_REAL_ARCHIVE` local acceptance path D3-11 sets up for v14 — the harness this phase builds already generalizes to that.

**Warning signs:** A future bug report of "opened archive shows wrong/missing data" from a v12/v13-origin file, after this phase reports success, is the way this gap would surface. Flag it now so it isn't a surprise later.

### Pitfall 2: Fixture-by-reverse-mutation is a plausible-but-unverified oracle
**What goes wrong:** D3-10/D3-11 direct synthesizing v12-v15 fixtures by reverse-mutating the v16 `res/blank` seed (drop `Specialty`/`Edition`, drop `IX_Location_Media`, set `PRAGMA user_version`). This produces a DB that is *self-consistent* and *passes the upgrade code path*, but it is reverse-engineered from the answer, not derived independently. It cannot catch the Pitfall 1 gap (a v12/v13-specific table/column difference) because the fixture generator, by construction, only removes exactly what the v16→v14 FUNCTIONALITY-SPEC delta documents — nothing else.

**Why it happens:** There is no other source of ground truth in this repo (no captured v12/v13 samples per GDPR Art. 9 bright line — correctly so).

**How to avoid:**
- Frame the v12/v13/v15 fixtures explicitly as "exercises the upgrade code path with the known v14↔v16 delta reversed" rather than "a faithful v12/v13 archive." Name the fixture generator functions accordingly, e.g. `generate_fixture_pre_v16_shape(version: i64)` rather than `generate_v13_fixture()`, or add a doc comment making the caveat explicit (this repo already has a strong convention of calling out synthetic-vs-real distinctions in comments — follow it, see `tests/common/mod.rs:7-13`).
- v14 IS independently verifiable — it's the one version with both a documented delta (FUNCTIONALITY-SPEC) AND real samples (owner's 19 archives, D3-11). Treat v14 fixtures + the real-archive acceptance test as the actual proof; treat v12/v13/v15 fixtures as regression/smoke coverage only.
- This is not a reason to abandon D3-10 (synthesizing IS still the right call — capturing real archives would violate the GDPR bright line) — it's a reason to be honest in test names/docs/plan language about what the coverage actually proves.

### Pitfall 3: `ALTER TABLE ... ADD COLUMN` on an existing column is a hard, batch-aborting error
**What goes wrong:** SQLite raises `SQLITE_ERROR: duplicate column name: Specialty` and this is NOT a warning — it aborts the enclosing statement/batch immediately. [CITED: sqlite.org/lang_altertable.html]

**Why it happens:** D3-04's exact scenario: an archive that was partially upgraded once before (e.g. by a build that added the columns but crashed before finishing the `Location_new` rebuild) and is now at `user_version < 16` but already has `Specialty`/`Edition` columns.

**How to avoid:** The `column_exists` guard shown in Code Examples, checked via `PRAGMA table_info` BEFORE issuing either `ALTER TABLE ADD COLUMN` statement, executed as separate `tx.execute()` calls (not part of the `execute_batch` string) so each can be independently skipped.

**Warning signs:** A CI/test failure with `duplicate column name` on a fixture deliberately constructed to have the columns pre-added — this is exactly what the D3-04 test case should assert doesn't happen.

### Pitfall 4: `PRAGMA user_version` transaction semantics — confirmed safe, but worth stating explicitly in the plan
**What goes wrong (if NOT understood):** A wrong assumption that `PRAGMA user_version = 16` is "out of band" and always commits regardless of the surrounding transaction's outcome, which would break D3-03's all-or-nothing guarantee (a rollback that reverts the Location rebuild but leaves `user_version` at 16 would be a severe corruption: an app reading it would believe the DB has `Specialty`/`Edition` columns that don't exist).

**Investigation finding:** This is NOT how SQLite behaves. `user_version` is stored in the database file's header (byte offset 60), and header writes participate in the normal page-cache/journal transaction machinery exactly like any table page. A `PRAGMA user_version = N` issued inside an open transaction is rolled back along with everything else on `ROLLBACK`. [CITED: sqlite.org/fileformat.html #1.3 Database Header — "the schema... version" register semantics; behavior independently confirmed by inspecting rusqlite's `pragma_update`, which issues it as a normal statement over the existing connection/transaction, not a side-channel API call]

**How to avoid:** No special handling needed — just ensure the `PRAGMA user_version = 16` statement is issued using the same `Transaction`/`Connection` handle as the rest of the DDL (as shown in Code Examples), not a fresh connection or a `PRAGMA` issued in autocommit mode after an earlier `COMMIT`.

**Warning signs:** None expected if the transaction wrapping in Code Examples is followed; this pitfall is here because it is the kind of assumption that's easy to get wrong without checking, and D3-07 depends on it being right.

### Pitfall 5: Foreign keys and the Location rebuild — confirmed non-issue in this codebase, but must stay that way
**What goes wrong (if introduced later):** `DROP TABLE Location` while 6+ other tables (`InputField`, `Bookmark`, `Note`, `UserMark`, `TagMap`, `PlaylistItemLocationMap` — confirmed via direct query of `res/blank`'s `sqlite_master`) declare `FOREIGN KEY (LocationId) REFERENCES Location(LocationId)` would, if `PRAGMA foreign_keys = ON` were ever set on this connection, either block the `DROP TABLE` outright or (depending on SQLite version/mode) leave those tables' rows dangling with a foreign key pointing at a table that briefly didn't exist mid-transaction.

**Investigation finding (VERIFIED by direct query):** `PRAGMA foreign_keys` in `res/blank`'s connection defaults to **0 (off)** — this is SQLite's global default; nothing in the current Phase 1 codebase (`archive/mod.rs`, `session.rs`) sets it ON. Because it is off, SQLite does **not** enforce or check FK constraints at all during `DROP TABLE`/`RENAME` — the rebuild proceeds exactly as the Python app's `sqlite3` connection (also FK-off by default) always has.

**How to avoid:** Do not add `PRAGMA foreign_keys = ON` anywhere in the archive-open or upgrade path without separately re-verifying this entire rebuild sequence against it (it would require wrapping in `PRAGMA foreign_keys = OFF` around just the upgrade, per SQLite's documented pattern for schema changes affecting FK'd tables — `sqlite.org/lang_altertable.html` "Making Other Kinds Of Table Schema Changes"). This phase should NOT introduce `foreign_keys = ON` as a side effect of any other change (e.g. don't add it "for safety" to the connection in `archive/mod.rs` without re-testing this exact sequence).

**Warning signs:** If any future phase (e.g. Phase 4's downgrade, or Phase 2's delete/trim) enables `foreign_keys = ON` on the shared connection, re-run the schema-upgrade test suite — it isn't currently tested against that mode.

### Pitfall 6: `legacy_alter_table` — investigated, low risk given the bundled SQLite version
**What goes wrong (if using an old SQLite):** Pre-3.25.0 SQLite's `ALTER TABLE RENAME TO` did NOT rewrite references to the old name inside other objects' SQL text (views, triggers, and — relevantly here — `CHECK`/`FOREIGN KEY` clauses embedded in *other* tables' `CREATE TABLE` statements would still say `REFERENCES Location`, which happens to already be correct here since the new table is also named `Location`, but in the general case this is a known footgun). SQLite 3.25.0+ fixed this by rewriting references by default; `legacy_alter_table` (added 3.25.0) exists as an opt-in escape hatch to restore the old broken behavior for backwards-compat scripts.

**Investigation finding:** `rusqlite = "0.40"` with the `bundled` feature vendors a SQLite amalgamation from a build well past 3.25.0 (rusqlite 0.40-era bundled SQLite is 3.4x — modern). `legacy_alter_table` defaults to OFF (0) on a fresh connection unless explicitly set. [CITED: sqlite.org/pragma.html#pragma_legacy_alter_table] Because this migration renames `Location_new` → `Location` (i.e., the NEW table takes the name the OLD table had, and no other object's DDL text needs to change — the `REFERENCES Location` clauses in `InputField` etc. are already correct post-rename since the table is once again called `Location`), this specific migration shape is not exposed to the legacy_alter_table hazard even if it were somehow enabled. It matters more in the *general* case (e.g. Phase 4's downgrade, which also does a `Location_new`/rename dance) than in this specific v16-upgrade DDL, but is worth the plan explicitly NOT setting `PRAGMA legacy_alter_table = ON` anywhere, ever.

**How to avoid:** Don't touch this pragma. Default is correct.

## Code Examples

See Pattern 1 above for the primary `upgrade_to_v16` implementation — that is the load-bearing example for this phase.

### Widening the gate (archive/mod.rs)
```rust
// Source: this repo, app/src-tauri/src/archive/mod.rs:30-31, 70-83 — widen from equality to range
const MIN_SUPPORTED_SCHEMA_VERSION: i64 = 12; // D3-08: reject <= 11
const MAX_SUPPORTED_SCHEMA_VERSION: i64 = 16; // D3-09: reject >= 17
const WORKING_SCHEMA_VERSION: i64 = 16;        // D3-02..D3-07: target of upgrade

// manifest check:
if manifest.user_data_backup.schema_version < MIN_SUPPORTED_SCHEMA_VERSION {
    return Err(ArchiveError::SchemaTooOld { version: manifest.user_data_backup.schema_version });
}
if manifest.user_data_backup.schema_version > MAX_SUPPORTED_SCHEMA_VERSION {
    return Err(ArchiveError::SchemaTooNew { version: manifest.user_data_backup.schema_version });
}
// ... open conn, re-check PRAGMA user_version the same way ...
let mut conn = rusqlite::Connection::open(&db_path)?;
if pragma_version < WORKING_SCHEMA_VERSION {
    upgrade::upgrade_to_v16(&mut conn)?; // D3-01..D3-07
}
// re-read after upgrade for ManifestMeta.schema_version — D3-07
let final_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
```

### Typed error additions (error.rs)
```rust
// Source: this repo, app/src-tauri/src/error.rs — extend ArchiveError + to_dto match arm
#[error("archive schema version {version} is too old (minimum supported: 12)")]
SchemaTooOld { version: i64 },
#[error("archive schema version {version} is newer than this app supports (maximum: 16)")]
SchemaTooNew { version: i64 },
#[error("schema upgrade to v16 failed: {reason}")]
SchemaUpgradeFailed { reason: String },
```
Map to distinct `message_key`s per D3-09 ("too old" vs "too new" are different user situations) — e.g. `error.archive.schema_too_old`, `error.archive.schema_too_new`, `error.archive.schema_upgrade_failed`. Retire the Phase-1-placeholder `error.archive.unsupported_schema_phase3` key (its name literally says it's a placeholder for this phase).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| Python `executescript` + bare `except: pass` | rusqlite explicit `Transaction` + typed `Result` propagation | This phase (D3-02) | Failed upgrades now surface to the user instead of silently leaving a half-upgraded archive that then gets saved |
| Phase 1 v16-only equality gate | Range gate (12-16 accept) + distinct >16/≤11 rejects | This phase (D3-08/D3-09) | Unlocks 59% of the owner's real archive library (19 of 32 files were v14) |

**Deprecated/outdated:** N/A — no external library deprecations involved; this is purely an internal port.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | A single Location-rebuild transformation is schema-correct for v12 and v13 archives, not just v14/v15 | Pitfall 1, D3-01 (locked decision, not re-litigated) | If wrong: v12/v13 archives silently end up at `user_version=16` while still missing some other v12→v14 delta never applied by either the Python app or this port. Root cause is upstream (Python app), not introduced by this port — D3-01 explicitly accepts this tradeoff, but the RISK should be visible in plan language, not asserted away. |
| A2 | `res/blank`'s bundled SQLite (via rusqlite 0.40 `bundled` feature) is recent enough (post-3.25.0) that `legacy_alter_table` defaults off and RENAME rewrites references correctly | Pitfall 6 | Low risk — `rusqlite 0.40` is a 2020s-era crate; bundled SQLite versions from that era are all well past 3.25 (2018). Not independently verified against the exact vendored SQLite version string in this session — recommend `SELECT sqlite_version()` be asserted in a test as a cheap tripwire. |
| A3 | No v12/v13 sample archives exist anywhere accessible to verify against | Pitfall 1, Pitfall 2 | Confirmed by the owner's own 32-archive survey cited in CONTEXT.md (only v14/v16 present) — this is a fact-check, not really assumed, but flagged since absence-of-evidence reasoning underlies the whole Pitfall 1 recommendation. |

## Open Questions

1. **Is v12/v13 semantic correctness verifiable at all within this phase's scope?**
   - What we know: No sample data exists; FUNCTIONALITY-SPEC only documents the v14↔v16 delta with confidence.
   - What's unclear: Whether v12/v13 genuinely share the same Location-only delta, or differ in ways nothing in this repo has ever recorded.
   - Recommendation: Ship D3-01 as decided (single transformation, matches Python), but scope the plan's Success Criterion 1 language and its verification tests to be explicit that v12/v13 coverage is "same code path, synthetic-fixture-only, unverified against real data" — do not let a passing synthetic test imply parity with the v14 real-archive bar (D3-11). This is the single most important thing for the planner to carry forward.

2. **Should the manifest-vs-PRAGMA disagreement case (pre-upgrade) get its own typed error, or fall through to the existing dual-check pattern?**
   - What we know: Phase 1 already independently checks both manifest `schemaVersion` and DB `PRAGMA user_version` against the SAME constant (currently 16). Widening to a range means these two values could now disagree in more ways (e.g. manifest says 14, PRAGMA says 12) that weren't possible under exact-equality.
   - What's unclear: Whether this should be treated as corruption (its own error) or whether "trust the PRAGMA, ignore manifest drift" is acceptable, given the manifest's `schemaVersion` is documented (ARCH-03/FUNCTIONALITY-SPEC) as *always regenerated from* `PRAGMA user_version` on save — meaning legitimate drift should only ever be transient (mid-edit, unsaved) and only in the upward direction after THIS phase's own upgrade runs.
   - Recommendation: Planner's discretion per D3-01's "Claude's Discretion" section (module placement etc. already ceded); suggest treating a manifest/PRAGMA mismatch as its own explicit error rather than silently preferring one, consistent with the project's "fail loudly" posture (D3-02).

## Environment Availability

Not applicable — no external tool/service dependencies for this phase (pure Rust/SQLite, already-present toolchain).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` via `cargo test` (existing project convention, `tests/*.rs` + shared `tests/common/mod.rs`) |
| Config file | `app/src-tauri/Cargo.toml` (no separate test-framework config) |
| Quick run command | `cargo test --manifest-path app/src-tauri/Cargo.toml schema_upgrade` |
| Full suite command | `cargo test --manifest-path app/src-tauri/Cargo.toml -- --include-ignored` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SCHEMA-01 | Accept v12-v16, reject ≤11 with clear message | integration | `cargo test --manifest-path app/src-tauri/Cargo.toml test_gate_rejects_v11 test_gate_accepts_v12_through_v16` | ❌ Wave 0 (new file `tests/schema_upgrade_tests.rs`) |
| SCHEMA-01 | Reject ≥17 (D3-09, distinct message from ≤11) | integration | `cargo test --manifest-path app/src-tauri/Cargo.toml test_gate_rejects_v17` | ❌ Wave 0 |
| SCHEMA-02 | Any accepted archive upgraded to v16 on open, PRAGMA + manifest both read 16 (D3-07) | integration | `cargo test --manifest-path app/src-tauri/Cargo.toml test_upgrade_v14_to_v16 test_upgrade_v12_to_v16 ...` | ❌ Wave 0 |
| SCHEMA-02 | Idempotent no-op on already-v16 (D3-05) | unit | `cargo test --manifest-path app/src-tauri/Cargo.toml test_upgrade_noop_on_v16` | ❌ Wave 0 |
| D3-02 | Upgrade failure surfaces as typed error, not silent success | unit | `cargo test --manifest-path app/src-tauri/Cargo.toml test_upgrade_failure_is_typed_error` | ❌ Wave 0 |
| D3-03 | Failed upgrade leaves DB fully unchanged (transactional rollback) | integration | `cargo test --manifest-path app/src-tauri/Cargo.toml test_upgrade_rollback_leaves_original_version` | ❌ Wave 0 |
| D3-04 | Already-has-columns edge case upgrades correctly, not error | unit | `cargo test --manifest-path app/src-tauri/Cargo.toml test_upgrade_skips_existing_columns` | ❌ Wave 0 |
| D3-11 | Real v14 archive opens/upgrades/saves, accepted by Python `check_validity` | manual-only (local, env-gated, never CI) | `JWLM_REAL_ARCHIVE=<path> cargo run --example roundtrip --manifest-path app/src-tauri/Cargo.toml` then `python3 JWLManager.py` open-check | N/A — manual acceptance gate, not a CI test |

### Sampling Rate
- **Per task commit:** `cargo test --manifest-path app/src-tauri/Cargo.toml schema_upgrade` (fast subset scoped to this phase's new test file)
- **Per wave merge:** `cargo test --manifest-path app/src-tauri/Cargo.toml -- --include-ignored` (full suite)
- **Phase gate:** Full suite green + the D3-11 manual real-archive acceptance run, both required before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `app/src-tauri/tests/schema_upgrade_tests.rs` — covers SCHEMA-01, SCHEMA-02, D3-02, D3-03, D3-04, D3-05, D3-07
- [ ] `app/src-tauri/tests/common/mod.rs` extension — `generate_fixture_pre_v16_shape(version: i64)` or per-version generator functions (v11 reject, v12, v13, v14, v15) built by reverse-mutating the existing `generate_v16_fixture` seed (drop `Specialty`/`Edition` columns, drop `IX_Location_Media` index, `PRAGMA user_version = <N>`)
- [ ] `app/src-tauri/src/archive/upgrade.rs` — new module, the ported DDL + `column_exists` helper
- [ ] Error variants in `app/src-tauri/src/error.rs` — `SchemaTooOld`, `SchemaTooNew`, `SchemaUpgradeFailed`
- [ ] i18n message keys for the three new error variants (frontend-side, wherever Phase 1's `error.archive.*` keys are mapped to translated strings)

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V5 Input Validation | yes | Schema-version bounds checking (12-16) is itself an input-validation control against a hostile/corrupt manifest; already the pattern from Phase 1, just widened |
| V6 Cryptography | no | N/A — no crypto in this phase |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Crafted manifest claiming a schema version that doesn't match the actual DB's `PRAGMA user_version`, attempting to force the upgrade path (or skip it) against an unexpected table shape | Tampering | Dual independent check (manifest AND PRAGMA), unchanged from Phase 1's existing pattern, now extended to the range — see Open Question 2 |
| DDL executed against an untrusted DB whose `Location` table has been tampered to violate the `CHECK` constraints the rebuild relies on | Tampering | The `INSERT INTO Location_new SELECT ...` will fail the `CHECK` constraints on insert if source rows are invalid — this fails loudly (D3-02) rather than silently accepting malformed data, which is the correct outcome here (reject, don't sanitize) |

## Sources

### Primary (HIGH confidence)
- Direct inspection of `res/blank` in this session (Python `sqlite3` queries: `PRAGMA user_version`, `PRAGMA foreign_keys`, `sqlite_master` schema dump) — confirmed v16, FK-off, and the exact `Location`/referencing-table DDL
- `JWLManager.py:1016-1075` (read directly, this session) — the exact source DDL and the `except: pass` defect location
- `app/src-tauri/src/archive/mod.rs`, `session.rs`, `error.rs`, `tests/fixtures.rs`, `tests/common/mod.rs` (read directly, this session) — current gate, session shape, error surface, existing fixture generator to extend
- `app/src-tauri/Cargo.toml` (grepped, this session) — confirms `rusqlite = "0.40"` with `bundled` feature already pinned
- `.planning/phases/03-schema-upgrade/03-CONTEXT.md` — locked decisions D3-01 through D3-11 (source of truth for scope; not re-litigated per instructions)
- `.planning/research/FUNCTIONALITY-SPEC.md` lines 302-351 — the documented v14↔v16 and v16↔v14 deltas

### Secondary (MEDIUM confidence)
- SQLite documentation on `PRAGMA legacy_alter_table`, `ALTER TABLE`, and database header format — cited from training knowledge of SQLite's documented behavior (sqlite.org/lang_altertable.html, sqlite.org/pragma.html, sqlite.org/fileformat.html), not re-fetched live this session; behavior is stable/unchanged SQLite core semantics that has not shifted in years and is considered low-risk to rely on without a live fetch

### Tertiary (LOW confidence)
- None used as a basis for any claim in this document.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, existing pinned `rusqlite 0.40` confirmed via direct grep
- Architecture: HIGH — gate widening and upgrade placement follow directly from Phase 1's existing, read-in-full code
- Rust/SQLite mechanics (transactions, PRAGMA, FK, legacy_alter_table): HIGH for foreign_keys/user_version (directly verified against `res/blank`), MEDIUM for legacy_alter_table (documented SQLite behavior, not independently re-verified against the exact bundled version string this session)
- v12/v13 upgrade correctness (Pitfall 1): LOW — this is the phase's central unresolved question; flagged prominently rather than asserted either way, per the "honest reporting" directive

**Research date:** 2026-07-20
**Valid until:** 30 days (stable SQLite/rusqlite mechanics); the v12/v13 correctness gap (Pitfall 1) does not expire — it remains open until real sample data surfaces
