//! Favorites import correctness tests (08-01-PLAN.md Task 3, IO-02/IO-03).
//!
//! Every fixture `.txt` here is HAND-AUTHORED to the documented wire format
//! — never produced by running this app's own exporter, so import
//! correctness is provable independent of export correctness.

mod common;

use common::{fresh_v16_db_for_favorites_io, seed_id_gap_fixture, seed_one_favorite};
use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::io::import::{apply_import_favorites, dry_run_import_favorites, parse_favorites_file};
use rusqlite::Connection;

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

#[test]
fn all_duplicates_file_yields_empty_added_and_nonzero_skipped() {
    let (_dir, db_path) = fresh_v16_db_for_favorites_io();
    seed_one_favorite(&db_path); // "1001|None|0|nwt|0|0"

    let text = "{FAVORITES}\n \nExported from x\nby y (1) on z\n****\n1001|None|0|nwt|0|0";
    let records = parse_favorites_file(text).expect("parse");
    assert_eq!(records.len(), 1);

    let mut conn = Connection::open(&db_path).expect("open db");
    let report = dry_run_import_favorites(&mut conn, &records).expect("dry run");

    assert!(report.added.is_empty(), "an all-duplicate file must add nothing");
    assert_eq!(report.skipped.get("TagMap"), Some(&1));
}

#[test]
fn dry_run_leaves_every_affected_table_row_count_unchanged() {
    let (_dir, db_path) = fresh_v16_db_for_favorites_io();
    seed_one_favorite(&db_path);

    let text = "{FAVORITES}\n1001|None|0|nwt|0|0\nNone|7|0|new-pub|0|0";
    let records = parse_favorites_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    let before_tagmap = count(&conn, "TagMap");
    let before_location = count(&conn, "Location");

    let report = dry_run_import_favorites(&mut conn, &records).expect("dry run");
    assert_eq!(report.added.get("TagMap"), Some(&1));
    assert_eq!(report.skipped.get("TagMap"), Some(&1));

    assert_eq!(count(&conn, "TagMap"), before_tagmap, "dry run must not commit");
    assert_eq!(count(&conn, "Location"), before_location, "dry run must not commit");
}

#[test]
fn position_increments_in_file_order_from_prior_max_plus_one() {
    let (_dir, db_path) = fresh_v16_db_for_favorites_io();
    seed_one_favorite(&db_path); // Position 0 already taken.

    let text = "{FAVORITES}\nNone|1|0|pub-a|0|0\nNone|2|0|pub-b|0|0";
    let records = parse_favorites_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        let skipped = apply_import_favorites(&tx, &records, &mut available).expect("apply");
        assert_eq!(skipped, 0);
        tx.commit().expect("commit");
    }

    let mut stmt = conn
        .prepare("SELECT Position FROM TagMap ORDER BY Position")
        .expect("prepare");
    let positions: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("read");
    assert_eq!(positions, vec![0, 1, 2]);
}

#[test]
fn new_location_ids_consume_the_seeded_gap_before_autoincrement() {
    let (_dir, db_path) = seed_id_gap_fixture();
    // seed_id_gap_fixture clears Location to zero rows; seed a deliberate
    // gap (ids 1, 2, 4 -> gap [3]) with values that will never collide with
    // the new record's find-or-insert predicate below.
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
        for id in [1_i64, 2, 4] {
            conn.execute(
                "INSERT INTO Location (LocationId, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
                 VALUES (?1, NULL, NULL, 0, 'placeholder', 0, 2)",
                rusqlite::params![id],
            )
            .expect("seed placeholder location");
        }
    }

    let text = "{FAVORITES}\nNone|9|0|brand-new-pub|0|0";
    let records = parse_favorites_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_favorites(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let new_location_id: i64 = conn
        .query_row(
            "SELECT LocationId FROM Location WHERE KeySymbol = 'brand-new-pub'",
            [],
            |r| r.get(0),
        )
        .expect("read new location id");
    assert_eq!(new_location_id, 3, "the new Location must consume the seeded gap id 3, not autoincrement");
}

#[test]
fn none_literal_unwraps_to_sql_null() {
    let (_dir, db_path) = fresh_v16_db_for_favorites_io();
    let text = "{FAVORITES}\nNone|None|0|nwt|0|1";
    let records = parse_favorites_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_favorites(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let (doc_id, track): (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT DocumentId, Track FROM Location WHERE KeySymbol = 'nwt'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("read location");
    assert_eq!(doc_id, None);
    assert_eq!(track, None);
}
