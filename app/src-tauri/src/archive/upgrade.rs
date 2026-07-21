//! Transactional schema upgrade to v16 + post-upgrade v16 contract validator
//! (SCHEMA-01/02, 03-02-PLAN.md). Ports `JWLManager.py:1016-1070`'s DDL —
//! NOT its `except: pass` (lines 1071-1074), which is the exact
//! silent-corruption defect this module must never reproduce (D3-02, T-03-03).
//!
//! `upgrade_to_v16` wraps the entire rebuild in a single `rusqlite`
//! `Transaction`: on any failure the transaction rolls back on drop (never
//! committed), leaving the working-copy DB at its ORIGINAL `PRAGMA
//! user_version` and Location row-set — the caller always gets a typed
//! `ArchiveError::SchemaUpgradeFailed`, never a silently-swallowed error and
//! never a half-migrated DB (D3-03).
//!
//! Finding 1 (03-REVIEWS.md, HIGH): the Python DDL's `INSERT ... SELECT`
//! copies literal `NULL, NULL` into the new `Specialty`/`Edition` columns.
//! That is safe ONLY when those columns were just `ADD COLUMN`ed (necessarily
//! empty). On the D3-04 path — where a pre-v16 DB already carries
//! `Specialty`/`Edition` with real data but `user_version < 16` — that
//! `NULL, NULL` DESTROYS existing data. This module instead builds the
//! `INSERT ... SELECT` source column list CONDITIONALLY: real column when it
//! already exists, literal `NULL` only when it is being added fresh.
//!
//! Finding 2 (03-REVIEWS.md, HIGH): `validate_v16_contract` runs AFTER the
//! upgrade and BEFORE the session is accepted, so an unknown v12/v13 shape
//! gap (some undocumented pre-v14 delta this codebase hasn't verified) fails
//! LOUD instead of being silently stamped v16 and trusted.
//!
//! Finding 6: the rebuild needs `PRAGMA foreign_keys` OFF so
//! `DROP TABLE Location` doesn't trip FK enforcement from other tables that
//! reference it. 03-RESEARCH.md assumed this defaults OFF in this codebase;
//! empirically (03-01-SUMMARY.md Deviations, reconfirmed here) this build's
//! bundled SQLite does NOT default to OFF, so this module explicitly
//! disables it on the connection before opening the transaction (a pragma
//! change is a no-op inside an active transaction). This module never
//! ENABLES `foreign_keys` or sets `legacy_alter_table` — only disables, and
//! only for the duration of the rebuild.

use crate::error::ArchiveError;
use rusqlite::{Connection, Transaction};

/// The literal `Location_new` DDL from `JWLManager.py:1026-1062`, byte-exact
/// down to the UNIQUE + three CHECK constraints. The `{specialty_src}` /
/// `{edition_src}` placeholders in the `INSERT ... SELECT` are the ONLY
/// dynamic part (finding 1) — and they are always one of the two static
/// literals `"Specialty"` / `"NULL"` / `"Edition"` / `"NULL"`, never
/// user-controlled data (SAFE-02 is not implicated).
const CREATE_LOCATION_NEW: &str = "
    CREATE TABLE Location_new (
        LocationId     INTEGER NOT NULL PRIMARY KEY,
        BookNumber     INTEGER,
        ChapterNumber  INTEGER,
        DocumentId     INTEGER,
        Track          INTEGER,
        IssueTagNumber INTEGER NOT NULL DEFAULT 0,
        KeySymbol      TEXT,
        MepsLanguage   INTEGER,
        Type           INTEGER NOT NULL,
        Title          TEXT,
        Specialty      TEXT,
        Edition        TEXT,
        UNIQUE (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type),
        CHECK ((Type = 0 AND
            ((DocumentId IS NOT NULL AND DocumentId != 0) OR
            (Track IS NOT NULL AND
            ((KeySymbol IS NOT NULL AND length(KeySymbol) > 0) OR
            (DocumentId IS NOT NULL AND DocumentId != 0))) OR
            (BookNumber IS NOT NULL AND BookNumber != 0 AND
            KeySymbol IS NOT NULL AND length(KeySymbol) > 0 AND
            (ChapterNumber IS NULL OR ChapterNumber = 0)) OR
            (ChapterNumber IS NOT NULL AND ChapterNumber != 0 AND
            BookNumber IS NOT NULL AND BookNumber != 0 AND
            KeySymbol IS NOT NULL AND length(KeySymbol) > 0))) OR
            Type != 0),
        CHECK ((Type = 1 AND
            (BookNumber IS NULL OR BookNumber = 0) AND
            (ChapterNumber IS NULL OR ChapterNumber = 0) AND
            (DocumentId IS NULL OR DocumentId = 0) AND
            KeySymbol IS NOT NULL AND length(KeySymbol) > 0 AND
            Track IS NULL) OR
            Type != 1),
        CHECK ((Type IN (2, 3) AND
            (BookNumber IS NULL OR BookNumber = 0) AND
            (ChapterNumber IS NULL OR ChapterNumber = 0)) OR
            Type NOT IN (2, 3)));
";

const CREATE_INDEXES: &str = "
    CREATE INDEX IF NOT EXISTS IX_Location_KeySymbol_MepsLanguage_BookNumber_ChapterNumber ON Location(KeySymbol, MepsLanguage, BookNumber, ChapterNumber);
    CREATE INDEX IF NOT EXISTS IX_Location_MepsLanguage_DocumentId ON Location(MepsLanguage, DocumentId);
    CREATE UNIQUE INDEX IF NOT EXISTS IX_Location_Media ON Location(KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type, COALESCE(Specialty, ''), COALESCE(Edition, ''));
";

/// True iff `table` has a column named `column`, via `PRAGMA table_info`.
/// The only dynamic SQL fragment here is the static literal table name
/// (`"Location"`, never user data) interpolated into the pragma statement —
/// `PRAGMA table_info` does not accept bound parameters for its argument.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True iff `table` has an index named `index_name`, via `sqlite_master`.
fn index_exists(conn: &Connection, index_name: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [index_name],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// True iff `table` exists, via `sqlite_master`.
fn table_exists(conn: &Connection, table: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::SchemaUpgradeFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Runs the rebuild DDL inside an already-open transaction. Any error here
/// propagates up and causes the transaction to roll back on drop (never
/// committed) — see module docs.
fn run_upgrade_ddl(tx: &Transaction) -> Result<(), ArchiveError> {
    // ADD COLUMN, guarded (D3-04): skip a column that already exists rather
    // than erroring on a duplicate-column DDL failure.
    if !column_exists(tx, "Location", "Specialty")
        .map_err(|e| map_sqlite_err(e, "checking Specialty column"))?
    {
        tx.execute("ALTER TABLE Location ADD COLUMN Specialty TEXT", [])
            .map_err(|e| map_sqlite_err(e, "adding Specialty column"))?;
    }
    if !column_exists(tx, "Location", "Edition")
        .map_err(|e| map_sqlite_err(e, "checking Edition column"))?
    {
        tx.execute("ALTER TABLE Location ADD COLUMN Edition TEXT", [])
            .map_err(|e| map_sqlite_err(e, "adding Edition column"))?;
    }

    // Finding 1: conditional source columns. Both ADD COLUMNs above are now
    // guaranteed present (either pre-existing or just added), so this check
    // reflects whether real data could be present to preserve.
    let specialty_src = if column_exists(tx, "Location", "Specialty")
        .map_err(|e| map_sqlite_err(e, "checking Specialty column"))?
    {
        "Specialty"
    } else {
        "NULL"
    };
    let edition_src = if column_exists(tx, "Location", "Edition")
        .map_err(|e| map_sqlite_err(e, "checking Edition column"))?
    {
        "Edition"
    } else {
        "NULL"
    };

    tx.execute_batch(CREATE_LOCATION_NEW)
        .map_err(|e| map_sqlite_err(e, "creating Location_new"))?;

    let insert_sql = format!(
        "INSERT INTO Location_new SELECT LocationId, BookNumber, ChapterNumber, DocumentId, \
         Track, IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, {specialty_src}, {edition_src} \
         FROM Location"
    );
    tx.execute(&insert_sql, [])
        .map_err(|e| map_sqlite_err(e, "rebuilding Location rows"))?;

    tx.execute_batch("DROP TABLE Location; ALTER TABLE Location_new RENAME TO Location;")
        .map_err(|e| map_sqlite_err(e, "swapping Location table"))?;

    tx.execute_batch(CREATE_INDEXES)
        .map_err(|e| map_sqlite_err(e, "creating Location indexes"))?;

    tx.pragma_update(None, "user_version", 16_i64)
        .map_err(|e| map_sqlite_err(e, "setting user_version"))?;

    Ok(())
}

/// Upgrades `conn`'s working-copy database to schema v16, transactionally.
///
/// - `PRAGMA user_version == 16`: no-op, `Ok(())` (D3-05, idempotent).
/// - `PRAGMA user_version > 16`: `Err(SchemaTooNew)` (finding 7,
///   belt-and-suspenders — the range gate should already have caught this).
/// - otherwise: runs the rebuild DDL inside a single `rusqlite::Transaction`.
///   ANY failure rolls the transaction back on drop, leaving `conn` at its
///   ORIGINAL `user_version` and Location row-set, and returns
///   `Err(SchemaUpgradeFailed)` — never `Ok`, never a bare propagated
///   `rusqlite::Error` without upgrade context (D3-02/D3-03).
pub fn upgrade_to_v16(conn: &mut Connection) -> Result<(), ArchiveError> {
    let current_version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| map_sqlite_err(e, "reading user_version"))?;

    if current_version == 16 {
        return Ok(());
    }
    if current_version > 16 {
        return Err(ArchiveError::SchemaTooNew {
            version: current_version,
        });
    }

    // `DROP TABLE Location` cascades to every other table's FK reference to
    // it, so foreign_keys must be OFF for the rebuild. `PRAGMA foreign_keys`
    // is a documented SQLite no-op inside an active transaction, so this
    // MUST run on the connection BEFORE `conn.transaction()` opens one.
    // Empirically (03-01-SUMMARY.md Deviations) this build's bundled SQLite
    // does NOT default to foreign_keys=OFF as 03-RESEARCH.md assumed — the
    // rebuild fails with a FK constraint violation without this explicit
    // disable. This never ENABLES foreign_keys (finding 6's actual
    // constraint); it only makes the OFF state the rebuild requires
    // explicit and local rather than relying on an unverified default.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| map_sqlite_err(e, "disabling foreign_keys before rebuild"))?;

    let tx = conn
        .transaction()
        .map_err(|e| map_sqlite_err(e, "opening upgrade transaction"))?;

    run_upgrade_ddl(&tx)?;

    tx.commit()
        .map_err(|e| map_sqlite_err(e, "committing upgrade transaction"))?;

    Ok(())
}

/// Post-upgrade v16 contract validator (finding 2, HIGH). Verifies the
/// working DB actually satisfies the v16 shape this app depends on, so an
/// unknown/undocumented pre-v14 gap fails LOUD rather than being silently
/// trusted as v16. Checked, in order:
///   - `PRAGMA user_version == 16`
///   - required tables present (`Location`, `Note`, `UserMark`, `Tag`,
///     `TagMap`, `LastModified` — the tables Phase 1's `query_notes` +
///     manifest-update path already read/write)
///   - `Location` has all v16 columns, including `Specialty`/`Edition`
///   - the UNIQUE index `IX_Location_Media` exists
pub fn validate_v16_contract(conn: &Connection) -> Result<(), ArchiveError> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| map_sqlite_err(e, "reading user_version for contract check"))?;
    if version != 16 {
        return Err(ArchiveError::SchemaUpgradeFailed {
            reason: format!("post-upgrade contract: expected user_version 16, found {version}"),
        });
    }

    const REQUIRED_TABLES: &[&str] = &[
        "Location",
        "Note",
        "UserMark",
        "Tag",
        "TagMap",
        "LastModified",
    ];
    for table in REQUIRED_TABLES {
        if !table_exists(conn, table).map_err(|e| map_sqlite_err(e, "checking required table"))? {
            return Err(ArchiveError::SchemaUpgradeFailed {
                reason: format!("post-upgrade contract: missing required table {table}"),
            });
        }
    }

    const REQUIRED_LOCATION_COLUMNS: &[&str] = &[
        "LocationId",
        "BookNumber",
        "ChapterNumber",
        "DocumentId",
        "Track",
        "IssueTagNumber",
        "KeySymbol",
        "MepsLanguage",
        "Type",
        "Title",
        "Specialty",
        "Edition",
    ];
    for column in REQUIRED_LOCATION_COLUMNS {
        if !column_exists(conn, "Location", column)
            .map_err(|e| map_sqlite_err(e, "checking Location column for contract"))?
        {
            return Err(ArchiveError::SchemaUpgradeFailed {
                reason: format!("post-upgrade contract: Location missing column {column}"),
            });
        }
    }

    if !index_exists(conn, "IX_Location_Media")
        .map_err(|e| map_sqlite_err(e, "checking IX_Location_Media for contract"))?
    {
        return Err(ArchiveError::SchemaUpgradeFailed {
            reason: "post-upgrade contract: missing unique index IX_Location_Media".to_string(),
        });
    }

    Ok(())
}
