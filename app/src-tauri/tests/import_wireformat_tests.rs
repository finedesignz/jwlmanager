//! Favorites import correctness tests (08-01-PLAN.md Task 3, IO-02/IO-03).
//!
//! Every fixture `.txt` here is HAND-AUTHORED to the documented wire format
//! — never produced by running this app's own exporter, so import
//! correctness is provable independent of export correctness.

mod common;

use common::{fresh_v16_db, fresh_v16_db_for_favorites_io, seed_id_gap_fixture, seed_one_favorite};
use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::io::import::{
    apply_import_annotations, apply_import_bookmarks, apply_import_favorites,
    dry_run_import_annotations, dry_run_import_bookmarks, dry_run_import_favorites,
    parse_annotations_file, parse_bookmarks_file, parse_favorites_file,
};
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

// ---------------------------------------------------------------------------
// Bookmarks (08-02-PLAN.md Task 1, IO-02/IO-03)
// ---------------------------------------------------------------------------

#[test]
fn bookmark_reimport_updates_existing_slot_and_reports_overwritten() {
    let (_dir, db_path) = fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
        conn.execute(
            "INSERT INTO Location (KeySymbol, MepsLanguage, Type) VALUES ('nwt', 0, 1)",
            [],
        )
        .expect("insert container location");
        let container_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (1, 1, NULL, NULL, 0, 'nwt', 0, 0)",
            [],
        )
        .expect("insert scripture location");
        let scripture_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title, Snippet, BlockType, BlockIdentifier) \
             VALUES (?1, ?2, 0, 'Old Title', NULL, 0, NULL)",
            rusqlite::params![scripture_id, container_id],
        )
        .expect("insert existing bookmark");
    }

    let text = "{BOOKMARKS}\n1|1|None|0|nwt|0|0|0|New Title|None|0|None";
    let records = parse_bookmarks_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("reopen");
    let before = count(&conn, "Bookmark");
    let report = dry_run_import_bookmarks(&mut conn, &records).expect("dry run");
    assert_eq!(report.overwritten.get("Bookmark"), Some(&1));
    assert_eq!(count(&conn, "Bookmark"), before, "dry run must not commit");
}

#[test]
fn scripture_import_reuses_existing_location_not_a_duplicate() {
    let (_dir, db_path) = fresh_v16_db();
    let existing_id = {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
        conn.execute(
            "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (1, 1, NULL, NULL, 0, 'nwt', 0, 0)",
            [],
        )
        .expect("insert scripture location");
        conn.last_insert_rowid()
    };

    let text = "{BOOKMARKS}\n1|1|None|0|nwt|0|0|0|Title|None|0|None";
    let records = parse_bookmarks_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("reopen");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_bookmarks(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let location_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE KeySymbol = 'nwt' AND BookNumber = 1 AND ChapterNumber = 1",
            [],
            |r| r.get(0),
        )
        .expect("count matching locations");
    assert_eq!(location_count, 1, "must reuse the existing scripture Location, not duplicate");

    let bookmark_location: i64 = conn
        .query_row("SELECT LocationId FROM Bookmark", [], |r| r.get(0))
        .expect("read bookmark location");
    assert_eq!(bookmark_location, existing_id);
}

#[test]
fn bookmark_title_broken_bar_is_not_reversed_on_import() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{BOOKMARKS}\nNone|None|1001|0|pub-x|0|0|0|Genesis \u{A6} Note|None|0|None";
    let records = parse_bookmarks_file(text).expect("parse");
    assert_eq!(records[0].title.as_deref(), Some("Genesis \u{A6} Note"));

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_bookmarks(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let title: String = conn
        .query_row("SELECT Title FROM Bookmark", [], |r| r.get(0))
        .expect("read title");
    assert_eq!(title, "Genesis \u{A6} Note", "the \u{A6} must not be reversed back to |");
}

// ---------------------------------------------------------------------------
// Annotations (08-02-PLAN.md Task 2, IO-02/IO-03)
// ---------------------------------------------------------------------------

#[test]
fn two_record_annotations_file_imports_as_two_inputfield_rows() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{ANNOTATIONS}\n \nheader stuff\n==={PUB=w}{DOC=1001}{LABEL=tag1}===\nFirst value\n\
                ==={PUB=w}{DOC=1001}{LABEL=tag2}===\nSecond\nline value\n==={END}===";
    let records = parse_annotations_file(text).expect("parse");
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].label, "tag2");
    assert_eq!(records[1].value, "Second\nline value");

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_annotations(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }
    assert_eq!(count(&conn, "InputField"), 2);

    let second_value: String = conn
        .query_row("SELECT Value FROM InputField WHERE TextTag = 'tag2'", [], |r| r.get(0))
        .expect("read tag2 value");
    assert_eq!(second_value, "Second\nline value");
}

#[test]
fn annotations_reimport_updates_in_place_and_reports_overwritten() {
    let (_dir, db_path) = fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
        conn.execute(
            "INSERT INTO Location (DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (1001, 0, 'w', NULL, 0)",
            [],
        )
        .expect("insert location");
        let location_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?1, 'tag1', 'Old Value')",
            rusqlite::params![location_id],
        )
        .expect("insert inputfield");
    }

    let text = "{ANNOTATIONS}\n \nheader\n==={PUB=w}{DOC=1001}{LABEL=tag1}===\nNew Value\n==={END}===";
    let records = parse_annotations_file(text).expect("parse");
    assert_eq!(records.len(), 1);

    let mut conn = Connection::open(&db_path).expect("reopen");
    let before = count(&conn, "InputField");
    let report = dry_run_import_annotations(&mut conn, &records).expect("dry run");
    assert_eq!(report.overwritten.get("InputField"), Some(&1));
    assert_eq!(count(&conn, "InputField"), before, "dry run must not commit");
}

#[test]
fn annotation_without_issue_bracket_creates_location_with_zero_not_null() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{ANNOTATIONS}\n \nheader\n==={PUB=w}{DOC=1001}{LABEL=tag1}===\nValue\n==={END}===";
    let records = parse_annotations_file(text).expect("parse");
    assert_eq!(records[0].issue, None);

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_annotations(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let issue: i64 = conn
        .query_row(
            "SELECT IssueTagNumber FROM Location WHERE KeySymbol = 'w'",
            [],
            |r| r.get(0),
        )
        .expect("read issue tag number");
    assert_eq!(issue, 0, "a missing {{ISSUE}} bracket must fill IssueTagNumber to 0, never NULL");
}
