//! Favorites import correctness tests (08-01-PLAN.md Task 3, IO-02/IO-03).
//!
//! Every fixture `.txt` here is HAND-AUTHORED to the documented wire format
//! — never produced by running this app's own exporter, so import
//! correctness is provable independent of export correctness.

mod common;

use common::{fresh_v16_db, fresh_v16_db_for_favorites_io, seed_id_gap_fixture, seed_one_favorite};
use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::io::export::export_annotations;
use jwlmanager_lib::db::io::header::ExportHeaderCtx;
use jwlmanager_lib::db::io::import::{
    apply_import_annotations, apply_import_bookmarks, apply_import_favorites,
    apply_import_highlights, apply_import_notes, dry_run_import_annotations,
    dry_run_import_bookmarks, dry_run_import_favorites, dry_run_import_highlights,
    dry_run_import_notes, parse_annotations_file, parse_bookmarks_file, parse_favorites_file,
    parse_highlights_file, parse_notes_file,
};
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;

fn pinned_annotations_header() -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: "{ANNOTATIONS}",
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

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

/// Pre-existing Phase 8 defect (found during Phase 9): `{DOC=None}` with no
/// matching `Location` already present must be rejected with a typed error,
/// not a raw SQLite CHECK-constraint violation. `find_or_insert_annotation_location`
/// (`db/io/import.rs`) never sets `Track`/`BookNumber`/`ChapterNumber`, so a
/// freshly-inserted `Location` can only satisfy the `Type=0` CHECK
/// (`archive/upgrade.rs`'s `CREATE_LOCATION_NEW`, byte-exact port of
/// `JWLManager.py:1026-1062`) when `DocumentId` is present and non-zero.
/// `JWLManager.py`'s own `add_location` (`:1909-1919`) has the identical gap
/// and would raise `sqlite3.IntegrityError` on this same input (caught by its
/// bare `except:` at `:1931` and surfaced as a generic "Error on import!"
/// dialog + `ROLLBACK`) — this is oracle parity, not a Rust-only rejection.
#[test]
fn annotation_without_doc_and_no_existing_location_rejected_with_typed_error() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{ANNOTATIONS}\n \nheader\n==={PUB=w}{DOC=None}{LABEL=p1}===\nValue\n==={END}===";
    let records = parse_annotations_file(text).expect("parse");
    assert_eq!(records[0].doc, None);

    let mut conn = Connection::open(&db_path).expect("open db");
    let before_location = count(&conn, "Location");
    let before_inputfield = count(&conn, "InputField");

    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    let result = apply_import_annotations(&tx, &records, &mut available);
    match result {
        Err(ArchiveError::ImportFailed { reason }) => {
            assert!(
                reason.contains("DOC"),
                "typed error should name the missing DOC as the reason: {reason}"
            );
        }
        other => panic!("expected ArchiveError::ImportFailed, got {other:?}"),
    }
    // Roll back explicitly rather than dropping `tx` — mirrors Python's
    // `con.execute('ROLLBACK;')` on the same failure (`JWLManager.py:1933`).
    tx.rollback().expect("rollback");

    assert_eq!(count(&conn, "Location"), before_location, "rejected import must not create a Location row");
    assert_eq!(count(&conn, "InputField"), before_inputfield, "rejected import must not create an InputField row");
}

/// Round-trip stability for a DOC-less annotation that already exists in the
/// archive (a scripture-shaped `Type=0` Location — `BookNumber`/
/// `ChapterNumber`/`KeySymbol` set, `DocumentId` NULL — the one shape that
/// legitimately produces `{DOC=None}` on export, `export.rs`'s
/// `AnnotationExportRow::doc` doc comment). Such a record can never be
/// RE-IMPORTED (the existing-Location `SELECT` binds `DocumentId = NULL`,
/// which SQL `=` never matches, so it always falls through to the same
/// rejected INSERT as the test above) — in EITHER this app or the Python
/// oracle. What must hold is that export stays byte-identical before and
/// after the rejected import attempt: the DB is untouched, so re-exporting
/// produces the exact same wire bytes.
#[test]
fn doc_less_annotation_export_is_unchanged_by_a_rejected_reimport() {
    let (_dir, db_path) = fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
        conn.execute(
            "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (930, 1, 1, 'nwt', NULL, 0)",
            [],
        )
        .expect("insert scripture-shaped location");
        conn.execute(
            "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (930, 'p1', 'Some value')",
            [],
        )
        .expect("insert inputfield");
    }

    let conn = Connection::open(&db_path).expect("reopen");
    let export_path_1 = _dir.path().join("first.txt");
    export_annotations(&conn, None, &pinned_annotations_header(), &export_path_1).expect("first export");
    let first_text = std::fs::read_to_string(&export_path_1).expect("read first export");
    assert!(first_text.contains("{DOC=None}"), "scripture-shaped Location must export DOC=None:\n{first_text}");

    let records = parse_annotations_file(&first_text).expect("parse re-exported text");
    assert_eq!(records[0].doc, None);

    let mut conn = Connection::open(&db_path).expect("reopen for import attempt");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    let result = apply_import_annotations(&tx, &records, &mut available);
    assert!(matches!(result, Err(ArchiveError::ImportFailed { .. })), "re-import of a DOC-less record must be rejected: {result:?}");
    tx.rollback().expect("rollback");

    let conn = Connection::open(&db_path).expect("reopen after rejected import");
    let export_path_2 = _dir.path().join("second.txt");
    export_annotations(&conn, None, &pinned_annotations_header(), &export_path_2).expect("second export");
    let second_text = std::fs::read_to_string(&export_path_2).expect("read second export");

    assert_eq!(first_text, second_text, "export must be byte-identical before/after a rejected re-import");
}

// ---------------------------------------------------------------------------
// Highlights (08-03-PLAN.md Task 2, IO-02/IO-03) — basic correctness. The
// overlap/chain-merge/cross-color/re-import-convergence geometry lives in
// `import_range_merge_tests.rs`.
// ---------------------------------------------------------------------------

#[test]
fn scripture_highlight_creates_location_usermark_and_blockrange() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0";
    let records = parse_highlights_file(text).expect("parse");
    assert_eq!(records.len(), 1);

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_highlights(&tx, &records, &mut available, 42).expect("apply");
        tx.commit().expect("commit");
    }

    assert_eq!(count(&conn, "Location"), 1);
    assert_eq!(count(&conn, "UserMark"), 1);
    assert_eq!(count(&conn, "BlockRange"), 1);

    let (start, end): (i64, i64) = conn
        .query_row("SELECT StartToken, EndToken FROM BlockRange", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("read range");
    assert_eq!((start, end), (0, 5));
}

#[test]
fn publication_highlight_when_book_number_field_is_empty() {
    let (_dir, db_path) = fresh_v16_db();
    // Field 6 (BookNumber) empty -> the publication branch (Python's own
    // `if attribs[6]:` truthiness check).
    let text = "{HIGHLIGHTS}\n2|2|10|20|3|1||0|1001|0|pub-x|0|0";
    let records = parse_highlights_file(text).expect("parse");
    assert_eq!(records.len(), 1);

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_highlights(&tx, &records, &mut available, 7).expect("apply");
        tx.commit().expect("commit");
    }

    let location_type: (Option<i64>, String) = conn
        .query_row("SELECT DocumentId, KeySymbol FROM Location", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("read location");
    assert_eq!(location_type, (Some(1001), "pub-x".to_string()));
}

#[test]
fn scripture_highlight_reuses_existing_location_not_a_duplicate() {
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

    let text = "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0";
    let records = parse_highlights_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("reopen");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_highlights(&tx, &records, &mut available, 1).expect("apply");
        tx.commit().expect("commit");
    }

    assert_eq!(count(&conn, "Location"), 1, "must reuse the existing scripture Location");
    let user_mark_location: i64 = conn
        .query_row("SELECT LocationId FROM UserMark", [], |r| r.get(0))
        .expect("read usermark location");
    assert_eq!(user_mark_location, existing_id);
}

#[test]
fn reimporting_the_same_highlight_creates_a_second_usermark_but_one_blockrange() {
    // RESEARCH Pitfall 5 / must-have: Highlights import is NOT idempotent at
    // the UserMark level — re-importing grows UserMark count while
    // BlockRange geometry converges (the same range absorbs itself).
    let (_dir, db_path) = fresh_v16_db();
    let text = "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0";
    let records = parse_highlights_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    for seed in [1_u64, 2_u64] {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_highlights(&tx, &records, &mut available, seed).expect("apply");
        tx.commit().expect("commit");
    }

    assert_eq!(count(&conn, "UserMark"), 2, "each import synthesizes a fresh UserMark");
    assert_eq!(count(&conn, "BlockRange"), 1, "the identical range absorbs into itself, not two rows");
}

#[test]
fn highlights_dry_run_leaves_every_affected_table_row_count_unchanged() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0";
    let records = parse_highlights_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    let before_location = count(&conn, "Location");
    let before_usermark = count(&conn, "UserMark");
    let before_blockrange = count(&conn, "BlockRange");

    let report = dry_run_import_highlights(&mut conn, &records, 99).expect("dry run");
    assert_eq!(report.added.get("Location"), Some(&1));
    assert_eq!(report.added.get("UserMark"), Some(&1));
    assert_eq!(report.added.get("BlockRange"), Some(&1));

    assert_eq!(count(&conn, "Location"), before_location, "dry run must not commit");
    assert_eq!(count(&conn, "UserMark"), before_usermark, "dry run must not commit");
    assert_eq!(count(&conn, "BlockRange"), before_blockrange, "dry run must not commit");
}

// ---------------------------------------------------------------------------
// Notes (08-04-PLAN.md)
// ---------------------------------------------------------------------------

fn independent_note_text(title: &str, note: &str) -> String {
    format!(
        "{{NOTES=}}\nheader\n==={{CREATED=2024-01-01T00:00:00}}{{MODIFIED=2024-01-01T00:00:00}}{{TAGS=}}===\n{title}\n{note}\n==={{END}}==="
    )
}

#[test]
fn independent_note_inserts_with_untitled_or_titled_match() {
    let (_dir, db_path) = fresh_v16_db();
    let text = independent_note_text("My Title", "My note body");
    let (bucket, records) = parse_notes_file(&text).expect("parse");
    assert_eq!(bucket, None);
    assert_eq!(records.len(), 1);

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_notes(&tx, None, &records, &mut available, 1, "2099-01-01T00:00:00Z")
            .expect("apply");
        tx.commit().expect("commit");
    }

    assert_eq!(count(&conn, "Note"), 1);
    let (title, content): (String, String) = conn
        .query_row("SELECT Title, Content FROM Note", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("read note");
    assert_eq!(title, "My Title");
    assert_eq!(content, "My note body");
}

#[test]
fn reimporting_titled_note_updates_rather_than_inserts() {
    let (_dir, db_path) = fresh_v16_db();
    let text = independent_note_text("My Title", "v1 body");
    let (_bucket, records) = parse_notes_file(&text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_notes(&tx, None, &records, &mut available, 1, "2099-01-01T00:00:00Z")
            .expect("apply");
        tx.commit().expect("commit");
    }

    let text2 = independent_note_text("My Title", "v2 body");
    let (_bucket2, records2) = parse_notes_file(&text2).expect("parse");
    {
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute ids");
        apply_import_notes(&tx, None, &records2, &mut available, 2, "2099-01-01T00:00:00Z")
            .expect("apply");
        tx.commit().expect("commit");
    }

    assert_eq!(count(&conn, "Note"), 1, "a titled re-import must UPDATE, not insert a second Note");
    let content: String = conn
        .query_row("SELECT Content FROM Note", [], |r| r.get(0))
        .expect("read note");
    assert_eq!(content, "v2 body");
}

#[test]
fn multiline_note_round_trips_internal_newlines() {
    let (_dir, db_path) = fresh_v16_db();
    let text = independent_note_text("Title", "line one\nline two\nline three");
    let (_bucket, records) = parse_notes_file(&text).expect("parse");
    assert_eq!(records[0].note, "line one\nline two\nline three");

    let mut conn = Connection::open(&db_path).expect("open db");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    apply_import_notes(&tx, None, &records, &mut available, 1, "2099-01-01T00:00:00Z")
        .expect("apply");
    tx.commit().expect("commit");
    drop(conn);

    let conn = Connection::open(&db_path).expect("reopen");
    let content: String = conn
        .query_row("SELECT Content FROM Note", [], |r| r.get(0))
        .expect("read note");
    assert_eq!(content, "line one\nline two\nline three");
}

#[test]
fn missing_created_falls_back_to_now_truncated_to_twenty_chars() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{NOTES=}\nheader\n==={MODIFIED=2024-01-01T00:00:00}{TAGS=}===\nTitle\nBody\n==={END}===";
    let (_bucket, records) = parse_notes_file(text).expect("parse");
    assert_eq!(records[0].created, None);

    let mut conn = Connection::open(&db_path).expect("open db");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    apply_import_notes(&tx, None, &records, &mut available, 1, "2099-06-15T12:00:00Z")
        .expect("apply");
    tx.commit().expect("commit");
    drop(conn);

    let conn = Connection::open(&db_path).expect("reopen");
    let created: String = conn
        .query_row("SELECT Created FROM Note", [], |r| r.get(0))
        .expect("read created");
    assert_eq!(created.len(), 20, "expected a 20-character timestamp, got: {created}");
    assert!(created.ends_with('Z'));
}

#[test]
fn bible_note_color_zero_with_range_creates_no_usermark_or_blockrange() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{NOTES=}\nheader\n==={CREATED=2024-01-01T00:00:00}{MODIFIED=2024-01-01T00:00:00}{TAGS=}{LANG=0}{PUB=nwt}{BK=1}{CH=1}{VS=5}{Reference=01001005}{COLOR=0}{RANGE=1:5-9}===\nTitle\nBody\n==={END}===";
    let (_bucket, records) = parse_notes_file(text).expect("parse");
    assert_eq!(records[0].color, 0);

    let mut conn = Connection::open(&db_path).expect("open db");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    apply_import_notes(&tx, None, &records, &mut available, 1, "2099-01-01T00:00:00Z")
        .expect("apply");
    tx.commit().expect("commit");
    drop(conn);

    let conn = Connection::open(&db_path).expect("reopen");
    assert_eq!(count(&conn, "UserMark"), 0);
    assert_eq!(count(&conn, "BlockRange"), 0);
    assert_eq!(count(&conn, "Note"), 1);
}

#[test]
fn bible_note_with_range_creates_usermark_and_merged_blockrange() {
    let (_dir, db_path) = fresh_v16_db();
    let text = "{NOTES=}\nheader\n==={CREATED=2024-01-01T00:00:00}{MODIFIED=2024-01-01T00:00:00}{TAGS=}{LANG=0}{PUB=nwt}{BK=1}{CH=1}{VS=5}{Reference=01001005}{COLOR=1}{RANGE=1:5-9;1:8-12}===\nTitle\nBody\n==={END}===";
    let (_bucket, records) = parse_notes_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    apply_import_notes(&tx, None, &records, &mut available, 1, "2099-01-01T00:00:00Z")
        .expect("apply");
    tx.commit().expect("commit");
    drop(conn);

    let conn = Connection::open(&db_path).expect("reopen");
    assert_eq!(count(&conn, "UserMark"), 1);
    assert_eq!(count(&conn, "BlockRange"), 1, "sequential sub-ranges merge into one BlockRange");
    let (start, end): (i64, i64) = conn
        .query_row("SELECT StartToken, EndToken FROM BlockRange", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("read range");
    assert_eq!((start, end), (5, 12));
}

#[test]
fn notes_bucket_delete_none_leaves_bucket_notes_untouched() {
    let (_dir, db_path) = fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO Note (Guid, Title, Content, BlockType, LastModified, Created) \
             VALUES ('note-a', 'apple', 'x', 0, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("seed bucket note");
    }

    let text = "{NOTES=a}\nheader\n==={CREATED=2024-01-01T00:00:00}{MODIFIED=2024-01-01T00:00:00}{TAGS=}===\nOther Title\nBody\n==={END}===";
    let (bucket, records) = parse_notes_file(text).expect("parse");
    assert_eq!(bucket, Some('a'));

    let mut conn = Connection::open(&db_path).expect("open db");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    // Caller passes `None` regardless of the file's own bucket — the opt-in
    // decision belongs to the frontend, never inferred from the file.
    let deleted = apply_import_notes(&tx, None, &records, &mut available, 1, "2099-01-01T00:00:00Z")
        .expect("apply");
    tx.commit().expect("commit");
    drop(conn);

    assert_eq!(deleted, 0);
    let conn = Connection::open(&db_path).expect("reopen");
    assert_eq!(count(&conn, "Note"), 2, "bucket delete must not run without explicit opt-in");
}

#[test]
fn notes_dry_run_with_bucket_reports_deleted() {
    let (_dir, db_path) = fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO Note (Guid, Title, Content, BlockType, LastModified, Created) \
             VALUES ('note-a', 'apple', 'x', 0, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("seed bucket note");
        // A second, higher-id, NON-bucket note — keeps `Note`'s max rowid
        // above 1 after the bucket delete, so the "Other Title" insert below
        // can never SQLite-reuse the just-freed id (rowid reuse without an
        // `AUTOINCREMENT` column would otherwise make a genuine
        // delete+insert look like a same-PK `overwritten` in the snapshot
        // diff — an artifact of THIS test's fixture shape, not of the
        // deletion logic itself).
        conn.execute(
            "INSERT INTO Note (Guid, Title, Content, BlockType, LastModified, Created) \
             VALUES ('note-z', 'zebra', 'y', 0, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("seed decoy note");
    }

    let text = "{NOTES=a}\nheader\n==={CREATED=2024-01-01T00:00:00}{MODIFIED=2024-01-01T00:00:00}{TAGS=}===\nOther Title\nBody\n==={END}===";
    let (_bucket, records) = parse_notes_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    let report =
        dry_run_import_notes(&mut conn, Some('a'), &records, 1, "2099-01-01T00:00:00Z").expect("dry run");
    assert_eq!(report.deleted.get("Note"), Some(&1));
    assert_eq!(count(&conn, "Note"), 2, "dry run must not commit the delete");
}
