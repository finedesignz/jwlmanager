//! Full schema-downgrade (v16 -> v14) test matrix (SCHEMA-04, 04-01-PLAN.md):
//! deterministic lowest-id survivor (D4-01), 7-column remap with
//! dedup-then-repoint on the four composite-key targets, typed-key non-merge
//! (MED-4), un-downgradeable-with-rollback (HIGH-1), and the version gate.
//!
//! Semantic assertions only — never byte-diff (save is not byte-preserving).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::downgrade::downgrade_to_v14;
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;

fn user_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn location_ids(conn: &Connection) -> Vec<i64> {
    let mut stmt = conn
        .prepare("SELECT LocationId FROM Location ORDER BY LocationId")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn location_has_column(conn: &Connection, column: &str) -> bool {
    let mut stmt = conn.prepare("PRAGMA table_info(Location)").unwrap();
    let found = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .any(|c| c.unwrap() == column);
    found
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

// ---------------------------------------------------------------------------
// 1. Deterministic survivor (D4-01)
// ---------------------------------------------------------------------------

#[test]
fn test_deterministic_lowest_id_survivor() {
    let (_dir, db_path) = common::generate_v16_collision_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("downgrade must succeed");

    assert_eq!(user_version(&conn), 14);
    // Group 50/20/90 collapses to 20; scripture 100 untouched.
    assert_eq!(location_ids(&conn), vec![20, 100]);
}

#[test]
fn test_survivor_is_pure_function_run_twice() {
    // Two fresh copies -> identical resulting Location row-set.
    let (_d1, p1) = common::generate_v16_collision_db();
    let (_d2, p2) = common::generate_v16_collision_db();
    let mut c1 = Connection::open(&p1).unwrap();
    let mut c2 = Connection::open(&p2).unwrap();
    downgrade_to_v14(&mut c1).unwrap();
    downgrade_to_v14(&mut c2).unwrap();
    assert_eq!(
        common::normalized_table_rows(&c1, "Location"),
        common::normalized_table_rows(&c2, "Location"),
    );
}

// ---------------------------------------------------------------------------
// 2. Round-trip semantic equivalence (D4-09)
// ---------------------------------------------------------------------------

#[test]
fn test_round_trip_all_dependents_repoint_to_survivor() {
    let (_dir, db_path) = common::generate_v16_collision_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("downgrade must succeed");

    // (a) version.
    assert_eq!(user_version(&conn), 14);
    // (b) merged to lowest id.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Location WHERE LocationId IN (50,90)"
        ),
        0
    );
    // (c) every dependent points at survivor 20, none at 50/90.
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM Bookmark WHERE LocationId = 20"),
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Bookmark WHERE PublicationLocationId = 20"
        ),
        1
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM Note WHERE LocationId = 20"),
        1
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM UserMark WHERE LocationId = 20"),
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 20"
        ),
        1
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM TagMap WHERE LocationId = 20"),
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM PlaylistItemLocationMap WHERE LocationId = 20"
        ),
        1
    );
    for table in [
        "Bookmark",
        "Note",
        "UserMark",
        "InputField",
        "TagMap",
        "PlaylistItemLocationMap",
    ] {
        assert_eq!(
            count(
                &conn,
                &format!("SELECT COUNT(*) FROM {table} WHERE LocationId IN (50,90)")
            ),
            0,
            "{table} must have no dangling reference to a merged-away Location"
        );
    }
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Bookmark WHERE PublicationLocationId IN (50,90)"
        ),
        0
    );
    // (d) integrity + no v14 UNIQUE violation.
    let ic: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ic, "ok");
    // (e) Specialty/Edition gone.
    assert!(!location_has_column(&conn, "Specialty"));
    assert!(!location_has_column(&conn, "Edition"));
}

// ---------------------------------------------------------------------------
// 3. No-collision passthrough
// ---------------------------------------------------------------------------

#[test]
fn test_no_collision_passthrough() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        // Two DISTINCT publication Locations (different DocumentId) -> no merge.
        conn.execute(
            "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
             IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
             VALUES (10, NULL, NULL, 111, NULL, 0, NULL, 0, 0, 'A', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
             IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
             VALUES (11, NULL, NULL, 222, NULL, 0, NULL, 0, 0, 'B', NULL, NULL)",
            [],
        )
        .unwrap();
    }
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("downgrade must succeed");
    assert_eq!(user_version(&conn), 14);
    assert_eq!(location_ids(&conn), vec![10, 11]);
    assert!(!location_has_column(&conn, "Specialty"));
}

// ---------------------------------------------------------------------------
// 4. Scripture not remapped
// ---------------------------------------------------------------------------

#[test]
fn test_scripture_location_untouched() {
    let (_dir, db_path) = common::generate_v16_collision_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).unwrap();
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM Location WHERE LocationId = 100 AND BookNumber = 1 AND ChapterNumber = 1"),
        1,
        "the non-null Book/Chapter scripture Location must survive unchanged"
    );
}

// ---------------------------------------------------------------------------
// 5-8. Composite-collision (dedup-then-repoint)
// ---------------------------------------------------------------------------

#[test]
fn test_composite_tagmap_shared_tag() {
    let (_dir, db_path) = common::generate_composite_tagmap_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("no error on shared-tag merge");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM TagMap WHERE LocationId = 20 AND TagId = 700"
        ),
        1
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM TagMap WHERE LocationId = 90"),
        0
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM (SELECT 1 FROM TagMap GROUP BY TagId, LocationId HAVING COUNT(*) > 1)"),
        0,
        "no UNIQUE(TagId,LocationId) violation"
    );
}

#[test]
fn test_composite_bookmark_shared_slot() {
    let (_dir, db_path) = common::generate_composite_bookmark_slot_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("no error on shared-slot merge");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM Bookmark"), 1);
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Bookmark WHERE PublicationLocationId = 20 AND Slot = 0"
        ),
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Bookmark WHERE PublicationLocationId = 90"
        ),
        0
    );
}

#[test]
fn test_composite_inputfield_shared_texttag() {
    let (_dir, db_path) = common::generate_composite_inputfield_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("no error on shared-texttag merge");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 20 AND TextTag = 'x'"
        ),
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 90"
        ),
        0
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM (SELECT 1 FROM InputField GROUP BY LocationId, TextTag HAVING COUNT(*) > 1)"),
        0,
        "no PK(LocationId,TextTag) violation"
    );
}

#[test]
fn test_composite_playlist_shared_item() {
    let (_dir, db_path) = common::generate_composite_playlist_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("no error on shared-playlist merge");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM PlaylistItemLocationMap WHERE LocationId = 20 AND PlaylistItemId = 1"), 1);
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM PlaylistItemLocationMap WHERE LocationId = 90"
        ),
        0
    );
}

// ---------------------------------------------------------------------------
// 9. MED-4 typed key: 'None' string is not SQL NULL
// ---------------------------------------------------------------------------

#[test]
fn test_med4_none_string_not_merged_with_null() {
    let (_dir, db_path) = common::generate_med4_none_key_db();
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("downgrade must succeed");
    assert_eq!(
        location_ids(&conn),
        vec![30, 40],
        "KeySymbol='None' and KeySymbol IS NULL must NOT be merged"
    );
}

// ---------------------------------------------------------------------------
// 10. HIGH-1 un-downgradeable + rollback
// ---------------------------------------------------------------------------

#[test]
fn test_high1_undowngradeable_rolls_back() {
    let (_dir, db_path) = common::generate_high1_undowngradeable_db();
    let mut conn = Connection::open(&db_path).unwrap();
    let before = common::normalized_table_rows(&conn, "Location");

    let result = downgrade_to_v14(&mut conn);
    match result {
        Err(ArchiveError::SchemaDowngradeFailed { .. }) => {}
        other => panic!("expected SchemaDowngradeFailed, got {other:?}"),
    }
    // DB unchanged: still v16, Specialty still present, original rows intact.
    assert_eq!(user_version(&conn), 16, "must roll back to v16");
    assert!(
        location_has_column(&conn, "Specialty"),
        "no half-rebuild: Specialty column must still exist"
    );
    assert_eq!(
        before,
        common::normalized_table_rows(&conn, "Location"),
        "Location row-set must be unchanged after rollback"
    );
}

// ---------------------------------------------------------------------------
// 11. Rollback on injected failure (D4-04)
// ---------------------------------------------------------------------------

#[test]
fn test_rollback_on_poisoned_state() {
    let (_dir, db_path) = common::generate_v16_collision_db();
    {
        // Rename a required remap target away so remap_location fails mid-run.
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("ALTER TABLE Note RENAME TO Note_poisoned;")
            .unwrap();
    }
    let mut conn = Connection::open(&db_path).unwrap();
    let before = common::normalized_table_rows(&conn, "Location");
    let result = downgrade_to_v14(&mut conn);
    assert!(matches!(
        result,
        Err(ArchiveError::SchemaDowngradeFailed { .. })
    ));
    assert_eq!(user_version(&conn), 16, "must remain v16 after rollback");
    assert_eq!(
        before,
        common::normalized_table_rows(&conn, "Location"),
        "Location rows unchanged after rollback"
    );
}

// ---------------------------------------------------------------------------
// 12. Version gate
// ---------------------------------------------------------------------------

#[test]
fn test_version_gate_noop_on_v14() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "user_version", 14_i64).unwrap();
    }
    let mut conn = Connection::open(&db_path).unwrap();
    downgrade_to_v14(&mut conn).expect("v14 must be an idempotent no-op Ok");
    assert_eq!(user_version(&conn), 14);
}

#[test]
fn test_version_gate_rejects_non_16() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "user_version", 15_i64).unwrap();
    }
    let mut conn = Connection::open(&db_path).unwrap();
    match downgrade_to_v14(&mut conn) {
        Err(ArchiveError::SchemaDowngradeFailed { .. }) => {}
        other => panic!("expected SchemaDowngradeFailed for a non-16 DB, got {other:?}"),
    }
}
