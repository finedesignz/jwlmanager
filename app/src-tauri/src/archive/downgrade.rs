//! Transactional schema downgrade to v14 + the 7-column LocationId remap
//! closure (SCHEMA-04, 04-01-PLAN.md). Ports `JWLManager.py:1172-1236`'s
//! `downgrade_schema` — the near-exact structural inverse of Phase 3's
//! `upgrade_to_v16` — but NOT its failure handling.
//!
//! D4-04: Python's `except Exception: self.crash_box(ex); self.clean_up();
//! sys.exit()` (lines 1238-1241) is deliberately NOT ported. A failed
//! downgrade must never kill the process or leave a half-rebuilt DB. This
//! module wraps the entire transform in one `rusqlite` transaction: any
//! failure rolls back on drop (never committed), leaving the working-copy DB
//! at its ORIGINAL `PRAGMA user_version` (16) and Location row-set, and
//! returns a typed `ArchiveError::SchemaDowngradeFailed` — never a swallowed
//! error, never a partial write (T-04-03).
//!
//! D4-01 (the founding risk note): v14 adds a SECOND `Location` UNIQUE —
//! `(KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)` —
//! absent in v16, so a v16 archive may hold rows that collide under it. The
//! transform merges each colliding group of null-Book/Chapter Locations onto
//! ONE survivor. Python's `groups[key].append(row[0])` kept `ids[0]`, which
//! depends on SQLite iteration order (non-deterministic). This module adds
//! `ORDER BY LocationId` to the grouping query so the survivor is ALWAYS the
//! LOWEST LocationId — the closure is a pure function of input state, proven
//! by a run-twice test.
//!
//! MED-4 (correctness divergence from Python's stringify bug): Python builds
//! the group key as an f-string `f"{k}|{i}|{m}|{d}|{t}|{ty}"` (:1176), which
//! COLLAPSES a literal `KeySymbol='None'` and a SQL `NULL KeySymbol` to the
//! same string `"None"` and wrongly merges them. This module keys on a typed
//! `Option`-tuple `GroupKey`, so `Some("None")` and `None` are distinct — the
//! two Locations are never merged.
//!
//! Composite-key divergence: FOUR remap targets carry a UNIQUE/PK in which the
//! remapped column participates (`TagMap(TagId,LocationId)`,
//! `Bookmark(PublicationLocationId,Slot)`, `InputField(LocationId,TextTag)`,
//! `PlaylistItemLocationMap(PlaylistItemId,LocationId)`). PK/UNIQUE enforcement
//! is INDEPENDENT of `PRAGMA foreign_keys=OFF`, so Python's blind
//! `UPDATE ... SET <col>=survivor` would RAISE when the two merged Locations
//! share the secondary key. This module does dedup-then-repoint: DELETE the
//! merged-away row that would collide with the survivor, THEN plain-UPDATE the
//! rest — never `UPDATE OR IGNORE` (which would leave a dangling reference).
//!
//! HIGH-1: an archive can hold a residual v14-UNIQUE collision the merge cannot
//! fix (e.g. two non-null-BookNumber rows sharing all six U2 columns, legal in
//! v16 because they differed only by Specialty/Edition). This module runs an
//! explicit preflight over ALL Location rows for BOTH v14 uniques (with correct
//! SQLite NULL-distinctness semantics) and fails with a named
//! `SchemaDowngradeFailed` + full rollback rather than a partial write (T-04-13).

use crate::db::pragma_guard::PragmaGuard;
use crate::error::ArchiveError;
use rusqlite::{Connection, Transaction};
use std::collections::BTreeMap;

/// The literal v14 `Location_new` DDL from `JWLManager.py:1194-1229`,
/// byte-exact: this is the v16 `CREATE_LOCATION_NEW` in `upgrade.rs` MINUS the
/// `Specialty`/`Edition` columns, PLUS the second UNIQUE
/// `(KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)` — the
/// constraint that forces the remap. The three CHECK constraints are identical
/// to v16. No user data is interpolated (SAFE-02 not implicated).
const CREATE_LOCATION_V14: &str = "
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
        UNIQUE (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type),
        UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type),
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

/// Typed grouping key for the merge (MED-4). Each element is the raw
/// `Option` value read from the row — a real `KeySymbol='None'` is
/// `Some("None")`, never conflated with a SQL `NULL` (`None`). Ordering:
/// `(KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)`.
type GroupKey = (
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    i64,
);

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::SchemaDowngradeFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Downgrades `conn`'s working-copy database from schema v16 to v14,
/// transactionally.
///
/// - `PRAGMA user_version == 14`: no-op, `Ok(())` (idempotent).
/// - `PRAGMA user_version != 16` (and != 14): `Err(SchemaDowngradeFailed)`.
/// - `== 16`: runs the merge + rebuild inside a single transaction; ANY
///   failure rolls back on drop, leaving `conn` at user_version 16 and its
///   original Location row-set, and returns `Err(SchemaDowngradeFailed)`.
pub fn downgrade_to_v14(conn: &mut Connection) -> Result<(), ArchiveError> {
    let current_version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| map_sqlite_err(e, "reading user_version"))?;

    if current_version == 14 {
        return Ok(());
    }
    if current_version != 16 {
        return Err(ArchiveError::SchemaDowngradeFailed {
            reason: format!("expected user_version 16, found {current_version}"),
        });
    }

    // `DROP TABLE Location` would cascade to every FK reference; the rebuild
    // needs foreign_keys OFF. `PRAGMA foreign_keys` is a no-op inside an active
    // transaction, so it MUST be set on the connection BEFORE the tx opens.
    // A `PragmaGuard` snapshots the prior PRAGMA state and restores it on drop
    // (PRAGMAs are not rolled back by a transaction rollback).
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;
    conn.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| map_sqlite_err(e, "disabling foreign_keys before rebuild"))?;

    // `unchecked_transaction` (shared `&self`) because `guard` already holds a
    // shared borrow of `conn` — same pattern as `db::delete::dry_run_delete_notes`.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening downgrade transaction"))?;

    run_downgrade_ddl(&tx)?;

    tx.commit()
        .map_err(|e| map_sqlite_err(e, "committing downgrade transaction"))?;

    drop(guard);
    Ok(())
}

/// Runs the merge + preflight + rebuild inside an already-open transaction.
/// Any error propagates up and rolls the transaction back on drop.
fn run_downgrade_ddl(tx: &Transaction) -> Result<(), ArchiveError> {
    // a. Grouping — typed tuple key (MED-4), ordered by LocationId (D4-01).
    let mut order: Vec<GroupKey> = Vec::new();
    let mut groups: BTreeMap<GroupKey, Vec<i64>> = BTreeMap::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT LocationId, KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type \
                 FROM Location WHERE BookNumber IS NULL AND ChapterNumber IS NULL ORDER BY LocationId",
            )
            .map_err(|e| map_sqlite_err(e, "preparing grouping query"))?;
        let mut rows = stmt
            .query([])
            .map_err(|e| map_sqlite_err(e, "running grouping query"))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| map_sqlite_err(e, "reading grouping row"))?
        {
            let location_id: i64 = row
                .get(0)
                .map_err(|e| map_sqlite_err(e, "reading LocationId"))?;
            let key: GroupKey = (
                row.get::<_, Option<String>>(1)
                    .map_err(|e| map_sqlite_err(e, "reading KeySymbol"))?,
                row.get::<_, Option<i64>>(2)
                    .map_err(|e| map_sqlite_err(e, "reading IssueTagNumber"))?,
                row.get::<_, Option<i64>>(3)
                    .map_err(|e| map_sqlite_err(e, "reading MepsLanguage"))?,
                row.get::<_, Option<i64>>(4)
                    .map_err(|e| map_sqlite_err(e, "reading DocumentId"))?,
                row.get::<_, Option<i64>>(5)
                    .map_err(|e| map_sqlite_err(e, "reading Track"))?,
                row.get::<_, i64>(6)
                    .map_err(|e| map_sqlite_err(e, "reading Type"))?,
            );
            if !groups.contains_key(&key) {
                order.push(key.clone());
            }
            // Rows arrive ascending by LocationId, so ids[0] is the LOWEST —
            // the deterministic survivor (D4-01).
            groups.entry(key).or_default().push(location_id);
        }
    }

    // b/c. Remap each colliding group onto its lowest-id survivor.
    for key in &order {
        let ids = &groups[key];
        if ids.len() < 2 {
            continue;
        }
        let keep_id = ids[0];
        for &old_id in &ids[1..] {
            remap_location(tx, keep_id, old_id)?;
            tx.execute("DELETE FROM Location WHERE LocationId = ?1", [old_id])
                .map_err(|e| map_sqlite_err(e, "deleting merged-away Location"))?;
        }
    }

    // d. Un-downgradeable preflight (HIGH-1) — over ALL remaining Location rows.
    check_downgradeable(tx)?;

    // e. DDL rebuild — drop Specialty/Edition, stamp v14.
    tx.execute_batch(CREATE_LOCATION_V14)
        .map_err(|e| map_sqlite_err(e, "creating Location_new (v14)"))?;
    tx.execute(
        "INSERT INTO Location_new SELECT LocationId, BookNumber, ChapterNumber, DocumentId, \
         Track, IssueTagNumber, KeySymbol, MepsLanguage, Type, Title FROM Location",
        [],
    )
    .map_err(map_downgrade_insert_err)?;
    tx.execute_batch("DROP TABLE Location; ALTER TABLE Location_new RENAME TO Location;")
        .map_err(|e| map_sqlite_err(e, "swapping Location table"))?;
    tx.pragma_update(None, "user_version", 14_i64)
        .map_err(|e| map_sqlite_err(e, "setting user_version"))?;

    Ok(())
}

/// Repoints all dependent rows of `old_id` onto `keep_id` across the 7 FK
/// columns, in verbatim `JWLManager.py:1185-1191` order. Three plain repoints;
/// four dedup-then-repoint (composite-key targets — see module docs).
fn remap_location(tx: &Transaction, keep_id: i64, old_id: i64) -> Result<(), ArchiveError> {
    // 1. Bookmark.LocationId — plain repoint (no LocationId-only uniqueness).
    tx.execute(
        "UPDATE Bookmark SET LocationId = ?1 WHERE LocationId = ?2",
        [keep_id, old_id],
    )
    .map_err(|e| map_sqlite_err(e, "repointing Bookmark.LocationId"))?;

    // 2. Bookmark.PublicationLocationId — dedup on UNIQUE (PublicationLocationId, Slot).
    // Deliberate, tested divergence from Python's blind UPDATE: delete the
    // merged-away row that collides with the survivor on Slot, then repoint the
    // rest — avoids both the UNIQUE violation and a dangling reference.
    tx.execute(
        "DELETE FROM Bookmark WHERE PublicationLocationId = ?1 AND Slot IN \
         (SELECT Slot FROM Bookmark WHERE PublicationLocationId = ?2)",
        [old_id, keep_id],
    )
    .map_err(|e| map_sqlite_err(e, "deduping Bookmark.PublicationLocationId"))?;
    tx.execute(
        "UPDATE Bookmark SET PublicationLocationId = ?1 WHERE PublicationLocationId = ?2",
        [keep_id, old_id],
    )
    .map_err(|e| map_sqlite_err(e, "repointing Bookmark.PublicationLocationId"))?;

    // 3. Note.LocationId — plain repoint.
    tx.execute(
        "UPDATE Note SET LocationId = ?1 WHERE LocationId = ?2",
        [keep_id, old_id],
    )
    .map_err(|e| map_sqlite_err(e, "repointing Note.LocationId"))?;

    // 4. UserMark.LocationId — plain repoint.
    tx.execute(
        "UPDATE UserMark SET LocationId = ?1 WHERE LocationId = ?2",
        [keep_id, old_id],
    )
    .map_err(|e| map_sqlite_err(e, "repointing UserMark.LocationId"))?;

    // 5. InputField.LocationId — dedup on PRIMARY KEY (LocationId, TextTag).
    tx.execute(
        "DELETE FROM InputField WHERE LocationId = ?1 AND TextTag IN \
         (SELECT TextTag FROM InputField WHERE LocationId = ?2)",
        [old_id, keep_id],
    )
    .map_err(|e| map_sqlite_err(e, "deduping InputField.LocationId"))?;
    tx.execute(
        "UPDATE InputField SET LocationId = ?1 WHERE LocationId = ?2",
        [keep_id, old_id],
    )
    .map_err(|e| map_sqlite_err(e, "repointing InputField.LocationId"))?;

    // 6. TagMap.LocationId — dedup on UNIQUE (TagId, LocationId).
    tx.execute(
        "DELETE FROM TagMap WHERE LocationId = ?1 AND TagId IN \
         (SELECT TagId FROM TagMap WHERE LocationId = ?2)",
        [old_id, keep_id],
    )
    .map_err(|e| map_sqlite_err(e, "deduping TagMap.LocationId"))?;
    tx.execute(
        "UPDATE TagMap SET LocationId = ?1 WHERE LocationId = ?2",
        [keep_id, old_id],
    )
    .map_err(|e| map_sqlite_err(e, "repointing TagMap.LocationId"))?;

    // 7. PlaylistItemLocationMap.LocationId — dedup on PK (PlaylistItemId, LocationId).
    tx.execute(
        "DELETE FROM PlaylistItemLocationMap WHERE LocationId = ?1 AND PlaylistItemId IN \
         (SELECT PlaylistItemId FROM PlaylistItemLocationMap WHERE LocationId = ?2)",
        [old_id, keep_id],
    )
    .map_err(|e| map_sqlite_err(e, "deduping PlaylistItemLocationMap.LocationId"))?;
    tx.execute(
        "UPDATE PlaylistItemLocationMap SET LocationId = ?1 WHERE LocationId = ?2",
        [keep_id, old_id],
    )
    .map_err(|e| map_sqlite_err(e, "repointing PlaylistItemLocationMap.LocationId"))?;

    Ok(())
}

/// HIGH-1 preflight: counts residual collisions the merge could not fix, for
/// BOTH v14 uniques, over ALL remaining Location rows. Returns a named
/// `SchemaDowngradeFailed` if any remain (so the transaction rolls back and the
/// DB stays v16). SQLite UNIQUE treats a NULL indexed column as DISTINCT, but
/// `GROUP BY` treats NULLs as equal — so a raw GROUP BY over-reports. To match
/// the constraint exactly, each preflight excludes rows where ANY indexed
/// column is NULL (a UNIQUE cannot fire on such a row), then groups the rest.
fn check_downgradeable(tx: &Transaction) -> Result<(), ArchiveError> {
    // U1: UNIQUE (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)
    let u1_violations: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM (SELECT 1 FROM Location \
             WHERE BookNumber IS NOT NULL AND ChapterNumber IS NOT NULL AND KeySymbol IS NOT NULL \
             AND MepsLanguage IS NOT NULL AND Type IS NOT NULL \
             GROUP BY BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type HAVING COUNT(*) > 1)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "preflighting v14 U1 uniqueness"))?;

    // U2: UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type)
    let u2_violations: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM (SELECT 1 FROM Location \
             WHERE KeySymbol IS NOT NULL AND IssueTagNumber IS NOT NULL AND MepsLanguage IS NOT NULL \
             AND DocumentId IS NOT NULL AND Track IS NOT NULL AND Type IS NOT NULL \
             GROUP BY KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type HAVING COUNT(*) > 1)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| map_sqlite_err(e, "preflighting v14 U2 uniqueness"))?;

    if u1_violations > 0 || u2_violations > 0 {
        return Err(ArchiveError::SchemaDowngradeFailed {
            reason: "archive cannot be downgraded to v14: two Locations violate a v14 uniqueness \
                     constraint that v16 does not enforce"
                .to_string(),
        });
    }
    Ok(())
}

/// Maps the `Location_new` INSERT error. A UNIQUE constraint failure here is
/// the HIGH-1 un-downgradeable case (belt-and-suspenders behind the preflight);
/// anything else is a generic rebuild failure.
fn map_downgrade_insert_err(err: rusqlite::Error) -> ArchiveError {
    let msg = err.to_string().to_lowercase();
    if msg.contains("unique") {
        return ArchiveError::SchemaDowngradeFailed {
            reason: "archive cannot be downgraded to v14: two Locations violate a v14 uniqueness \
                     constraint that v16 does not enforce"
                .to_string(),
        };
    }
    map_sqlite_err(err, "rebuilding Location rows (v14)")
}
