//! Favorites import fail-fast tests (08-01-PLAN.md Task 3, D8-04): a
//! malformed file must be rejected entirely BEFORE any transaction opens —
//! `parse_favorites_file` runs as a pure, transaction-free parse pass, so
//! these tests assert the typed error AND that zero rows ever changed.

mod common;

use common::{fresh_v16_db, fresh_v16_db_for_favorites_io};
use jwlmanager_lib::db::io::import::{parse_annotations_file, parse_bookmarks_file, parse_favorites_file};
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;

fn tagmap_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .expect("count")
}

fn bookmark_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM Bookmark", [], |r| r.get(0))
        .expect("count")
}

fn inputfield_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM InputField", [], |r| r.get(0))
        .expect("count")
}

#[test]
fn missing_tag_line_is_rejected_at_line_1() {
    let (_dir, db_path) = fresh_v16_db_for_favorites_io();
    let conn = Connection::open(&db_path).expect("open db");
    let before = tagmap_count(&conn);

    let text = "this file has no tag line\n1001|None|0|nwt|0|1";
    let err = parse_favorites_file(text).expect_err("must reject a missing {FAVORITES} tag line");
    match err {
        ArchiveError::ImportMalformed { category, line, .. } => {
            assert_eq!(category, "Favorites");
            assert_eq!(line, 1);
        }
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert_eq!(tagmap_count(&conn), before, "a rejected parse must never touch the archive");
}

#[test]
fn a_five_field_line_is_rejected_and_leaves_tagmap_count_unchanged() {
    let (_dir, db_path) = fresh_v16_db_for_favorites_io();
    let conn = Connection::open(&db_path).expect("open db");
    let before = tagmap_count(&conn);

    // Well-formed header, but the data line has only 5 pipe-delimited fields.
    let text = "{FAVORITES}\n \nExported from x\nby y (1) on z\n****\n1001|None|0|nwt|0";
    let err = parse_favorites_file(text).expect_err("must reject a 5-field data line");
    match err {
        ArchiveError::ImportMalformed { line, reason, .. } => {
            assert_eq!(line, 6);
            assert!(reason.contains('5'), "reason should name the actual field count: {reason}");
        }
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert_eq!(tagmap_count(&conn), before, "a rejected parse must never touch the archive");
}

#[test]
fn a_seven_field_line_is_rejected() {
    let text = "{FAVORITES}\n1001|None|0|nwt|0|1|extra";
    let err = parse_favorites_file(text).expect_err("must reject a 7-field data line");
    assert!(matches!(err, ArchiveError::ImportMalformed { .. }));
}

#[test]
fn earlier_well_formed_lines_do_not_partially_apply_before_a_later_malformed_line() {
    // D8-04: the WHOLE file is parsed before any transaction opens, so a
    // malformed line 3 rejects the entire file even though line 2 alone
    // would have parsed cleanly. Nothing here calls `apply_import_favorites`
    // at all — proving no transaction is ever opened for a file that fails
    // to parse.
    let text = "{FAVORITES}\n1001|None|0|nwt|0|1\nbroken|line";
    let err = parse_favorites_file(text).expect_err("must reject the whole file");
    match err {
        ArchiveError::ImportMalformed { line, .. } => assert_eq!(line, 3),
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Bookmarks (08-02-PLAN.md Task 1, D8-04)
// ---------------------------------------------------------------------------

#[test]
fn bookmarks_missing_tag_line_is_rejected_at_line_1() {
    let (_dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open db");
    let before = bookmark_count(&conn);

    let text = "no tag line here\n1|1|None|0|nwt|0|0|0|Title|None|None|None";
    let err = parse_bookmarks_file(text).expect_err("must reject a missing {BOOKMARKS} tag line");
    match err {
        ArchiveError::ImportMalformed { category, line, .. } => {
            assert_eq!(category, "Bookmarks");
            assert_eq!(line, 1);
        }
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert_eq!(bookmark_count(&conn), before, "a rejected parse must never touch the archive");
}

#[test]
fn bookmarks_eleven_field_line_is_rejected_and_leaves_row_count_unchanged() {
    let (_dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open db");
    let before = bookmark_count(&conn);

    // 11 pipe-delimited fields instead of the required 12.
    let text = "{BOOKMARKS}\n1|1|None|0|nwt|0|0|0|Title|None|None";
    let err = parse_bookmarks_file(text).expect_err("must reject an 11-field line");
    match err {
        ArchiveError::ImportMalformed { line, reason, .. } => {
            assert_eq!(line, 2);
            assert!(reason.contains("11"), "reason should name the actual field count: {reason}");
        }
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert_eq!(bookmark_count(&conn), before, "a rejected parse must never touch the archive");
}

#[test]
fn bookmarks_thirteen_field_line_is_rejected() {
    let text = "{BOOKMARKS}\n1|1|None|0|nwt|0|0|0|Title|None|None|None|extra";
    let err = parse_bookmarks_file(text).expect_err("must reject a 13-field line");
    assert!(matches!(err, ArchiveError::ImportMalformed { .. }));
}

// ---------------------------------------------------------------------------
// Annotations (08-02-PLAN.md Task 2, D8-04)
// ---------------------------------------------------------------------------

#[test]
fn annotations_missing_tag_line_is_rejected_at_record_1() {
    let (_dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open db");
    let before = inputfield_count(&conn);

    let text = "no tag line\n==={PUB=w}{DOC=None}{LABEL=tag1}===\nValue\n==={END}===";
    let err = parse_annotations_file(text).expect_err("must reject a missing {ANNOTATIONS} tag line");
    match err {
        ArchiveError::ImportMalformed { category, line, .. } => {
            assert_eq!(category, "Annotations");
            assert_eq!(line, 1);
        }
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert_eq!(inputfield_count(&conn), before, "a rejected parse must never touch the archive");
}

#[test]
fn annotations_record_missing_required_key_is_rejected() {
    // No {DOC=...} attribute at all.
    let text = "{ANNOTATIONS}\n \nheader\n==={PUB=w}{LABEL=tag1}===\nValue\n==={END}===";
    let err = parse_annotations_file(text).expect_err("must reject a header missing {DOC=...}");
    assert!(matches!(err, ArchiveError::ImportMalformed { .. }));
}
