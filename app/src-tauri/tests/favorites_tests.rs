//! EDIT-05 favorites coverage (07-01-PLAN.md). Task 1 covers unmark — the
//! TRACER slice proving the whole edit-op safety envelope (typed non-empty
//! selection, `apply_*` inside the caller's transaction, rolled-back
//! `dry_run_*` under `PragmaGuard`, semantic `DryRunReport`) end to end on
//! one op group before Plans 02-05 build four more on the same `db::edit`
//! spine. Task 2 (this same file) adds mark coverage.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::favorites::{apply_favorite_remove, dry_run_favorite_remove, NonEmptyTagMapIds};
use rusqlite::Connection;

/// The system `Favorite` tag's `TagId`. `res/blank` (the real v16
/// blank-archive seed every fixture is built from) ships this row
/// PRE-SEEDED (`Type=0, Name='Favorite'`, `TagId 1`) — `Tag` carries
/// `UNIQUE(Type, Name)`, so a test must never insert a second one.
fn favorite_tag_id(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite'",
        [],
        |r| r.get(0),
    )
    .expect("res/blank must ship the pre-seeded system Favorite tag")
}

/// Inserts one favorite instance: a scripture `Location` + a `TagMap` row
/// (`PlaylistItemId`/`NoteId` both `NULL`, per the TagMap one-of CHECK)
/// linking it to the pre-seeded system Favorite tag — the exact shape
/// `apply_favorite_add` (Task 2) also produces. `id` seeds
/// `LocationId`/`TagMapId` together so each call can pick a fresh,
/// non-colliding id.
fn seed_favorite(conn: &Connection, id: i64) {
    let tag_id = favorite_tag_id(conn);
    // Mirrors the `Position = ifnull(max(Position), -1) + 1` rule Task 2's
    // `apply_favorite_add` uses — `TagMap` carries `UNIQUE(TagId, Position)`,
    // so a second favorite sharing the system tag can't reuse `Position 0`.
    let position: i64 = conn
        .query_row(
            "SELECT IFNULL(MAX(Position), -1) + 1 FROM TagMap WHERE TagId = ?1",
            rusqlite::params![tag_id],
            |r| r.get(0),
        )
        .expect("compute next favorite Position");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (?1, NULL, NULL, NULL, NULL, 0, 'nwt', 0, 1, NULL, NULL, NULL)",
        rusqlite::params![id],
    )
    .expect("insert favorite Location");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (?1, NULL, ?1, NULL, ?2, ?3)",
        rusqlite::params![id, tag_id, position],
    )
    .expect("insert favorite TagMap");
}

/// A test asserting `dry_run_favorite_remove` leaves the TagMap row count
/// unchanged (SAFE-01) while still reporting the favorite as `deleted`.
#[test]
fn test_dry_run_favorite_remove_leaves_tagmap_count_unchanged() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open seeded db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        seed_favorite(&conn, 900);
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();

    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let report = dry_run_favorite_remove(&mut conn, &ids).expect("dry run must succeed");

    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        before, after,
        "dry-run must leave the working copy's TagMap row count unchanged"
    );
    assert_eq!(
        report.deleted.get("TagMap").copied().unwrap_or(0),
        1,
        "report must show the favorite's TagMap row as deleted: {:?}",
        report.deleted
    );
}

/// A test asserting `apply_favorite_remove` removes exactly the selected
/// TagMapIds — a sibling favorite must survive untouched.
#[test]
fn test_apply_favorite_remove_removes_exactly_selected_tagmapids() {
    let (_dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed_favorite(&conn, 900);
    seed_favorite(&conn, 901); // untouched sibling favorite

    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let tx = conn.unchecked_transaction().expect("open tx");
    let removed = apply_favorite_remove(&tx, &ids).expect("apply must succeed");
    assert_eq!(removed, 1, "exactly one TagMap row must be removed");

    let tagmap_900: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE TagMapId = 900",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tagmap_900, 0, "TagMap 900 must be gone");

    let tagmap_901: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE TagMapId = 901",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        tagmap_901, 1,
        "sibling favorite TagMap 901 must survive untouched"
    );

    tx.rollback().unwrap();
}

/// Rule #16: the system `Tag (Type=0, Name='Favorite')` row is never GC'd,
/// even when an unmark leaves it with zero remaining TagMap rows —
/// `trim_sweep`'s `unused_tag` predicate is scoped to `Type > 0`.
#[test]
fn test_favorite_tag_never_gc_d_after_unmark() {
    let (_dir, db_path) = common::fresh_v16_db();
    let tag_id = {
        let conn = Connection::open(&db_path).expect("open seeded db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        seed_favorite(&conn, 900);
        favorite_tag_id(&conn)
    };

    let mut conn = Connection::open(&db_path).expect("reopen db");
    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let report = dry_run_favorite_remove(&mut conn, &ids).expect("dry run must succeed");

    // The dry-run's internal `trim_sweep` already ran (inside its rolled-back
    // transaction) — if the system tag were wrongly GC'd, it would show up
    // here as `deleted["Tag"] == 1`.
    assert_eq!(
        report.deleted.get("Tag").copied().unwrap_or(0),
        0,
        "system Favorite tag (Type=0) must survive trim_sweep even though this \
         was its only TagMap reference — rule #16: {:?}",
        report.deleted
    );

    // Confirm against a real apply too (apply alone never calls trim_sweep —
    // that only happens on save — so this just reconfirms the tag is never
    // touched by the mutation itself).
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_favorite_remove(&tx, &ids).expect("apply must succeed");
    let tag_survives: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM Tag WHERE TagId = ?1",
            rusqlite::params![tag_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        tag_survives, 1,
        "the system Favorite tag must survive the real apply"
    );
    tx.rollback().unwrap();
}

/// A test asserts a `Vec<i64>` of length 0 fails `TryFrom` for
/// `NonEmptyTagMapIds` — an empty selection is unrepresentable at IPC
/// deserialization, before any command body or DB access runs.
#[test]
fn test_empty_selection_fails_deserialization() {
    let empty: Result<NonEmptyTagMapIds, _> = serde_json::from_str("[]");
    assert!(empty.is_err(), "empty selection must fail to deserialize");

    let non_empty: Result<NonEmptyTagMapIds, _> = serde_json::from_str("[42]");
    assert!(non_empty.is_ok(), "non-empty selection must deserialize");
}

/// Dry-run must leave the connection's PRAGMA state restored, matching the
/// `PragmaGuard` contract already proven for `dry_run_delete_notes`.
#[test]
fn test_dry_run_favorite_remove_restores_pragmas() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open seeded db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        seed_favorite(&conn, 900);
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    let fk_before: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();

    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let _report = dry_run_favorite_remove(&mut conn, &ids).expect("dry run must succeed");

    let fk_after: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        fk_before, fk_after,
        "dry-run must restore the connection's prior foreign_keys pragma"
    );
}
