//! SQL-layer coverage for `db::highlights::merge_block_ranges` (EDIT-02,
//! 07-02-PLAN.md Task 2) on a synthetic v16 fixture. `plan_merge`'s pure-fn
//! boundary cases already live as unit tests inside `db/highlights.rs`
//! itself; this file exercises the SQL executor built on top of it —
//! absorbed rows genuinely DELETEd, exactly one merged row INSERTed, its
//! `BlockType` carried through from the absorbed rows (never defaulted to
//! 0), and grouping keyed on `(Identifier, LocationId)` regardless of
//! `ColorIndex`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::highlights::merge_block_ranges;
use rusqlite::Connection;

/// Seeds a Location + UserMark (given ColorIndex) + one BlockRange
/// (BlockType 1, given Identifier/StartToken/EndToken) — the minimal shape
/// `merge_block_ranges`'s SELECT joins against.
fn seed_highlight_fixture(conn: &Connection) {
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (900, 1, 1, NULL, NULL, 0, 'nwt', 0, 0, 'Genesis 1:1', NULL, NULL)",
        [],
    )
    .expect("insert Location");

    // Two UserMarks at the SAME LocationId with DIFFERENT ColorIndex, each
    // owning one BlockRange at the SAME Identifier — the "merge ignores
    // color" fixture shape.
    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (900, 1, 900, 0, 'fixture-merge-usermark-0900', 1)",
        [],
    )
    .expect("insert UserMark 900 (color 1)");
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (901, 1, 5, 0, 10, 900)",
        [],
    )
    .expect("insert BlockRange 901");

    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (910, 4, 900, 0, 'fixture-merge-usermark-0910', 1)",
        [],
    )
    .expect("insert UserMark 910 (color 4)");
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (911, 1, 5, 15, 25, 910)",
        [],
    )
    .expect("insert BlockRange 911");

    // A disjoint range at the SAME Identifier — must survive untouched.
    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (920, 2, 900, 0, 'fixture-merge-usermark-0920', 1)",
        [],
    )
    .expect("insert UserMark 920 (disjoint)");
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (921, 1, 5, 100, 110, 920)",
        [],
    )
    .expect("insert disjoint BlockRange 921");
}

#[test]
fn merge_block_ranges_absorbs_overlapping_ranges_regardless_of_color() {
    let (_dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed_highlight_fixture(&conn);

    let tx = conn.unchecked_transaction().expect("open tx");
    // New incoming range [8, 20] overlaps BOTH BlockRange 901 (0-10) and 911
    // (15-25) at Identifier 5, LocationId 900 — despite their differing
    // ColorIndex — but NOT the disjoint 921 (100-110).
    let new_id = merge_block_ranges(&tx, 5, 900, 8, 20, 1, 900, None)
        .expect("merge_block_ranges must succeed");

    let remaining_at_identifier_5: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM BlockRange WHERE Identifier = 5",
            [],
            |r| r.get(0),
        )
        .expect("count BlockRange rows at Identifier 5");
    // The two absorbed rows (901, 911) are gone; the new merged row plus the
    // disjoint 921 remain — 2 rows total.
    assert_eq!(
        remaining_at_identifier_5, 2,
        "absorbed rows must be deleted, merged + disjoint must remain"
    );

    let exists_901: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM BlockRange WHERE BlockRangeId = 901)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!exists_901, "absorbed BlockRange 901 must be deleted");
    let exists_911: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM BlockRange WHERE BlockRangeId = 911)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!exists_911, "absorbed BlockRange 911 must be deleted");
    let exists_921: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM BlockRange WHERE BlockRangeId = 921)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(exists_921, "disjoint BlockRange 921 must NOT be touched");

    let (start, end, block_type): (i64, i64, i64) = tx
        .query_row(
            "SELECT StartToken, EndToken, BlockType FROM BlockRange WHERE BlockRangeId = ?1",
            [new_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("read merged row");
    assert_eq!(
        (start, end),
        (0, 25),
        "union must span both absorbed ranges"
    );
    assert_eq!(
        block_type, 1,
        "merged row's BlockType must be carried through, never 0"
    );

    tx.rollback().unwrap();
}

#[test]
fn merge_block_ranges_with_no_overlap_inserts_a_new_row_and_absorbs_nothing() {
    let (_dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed_highlight_fixture(&conn);

    let tx = conn.unchecked_transaction().expect("open tx");
    let before: i64 = tx
        .query_row("SELECT COUNT(*) FROM BlockRange", [], |r| r.get(0))
        .unwrap();

    let new_id = merge_block_ranges(&tx, 5, 900, 200, 210, 1, 900, None)
        .expect("merge_block_ranges must succeed");

    let after: i64 = tx
        .query_row("SELECT COUNT(*) FROM BlockRange", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        after,
        before + 1,
        "a disjoint new range only inserts, never absorbs"
    );

    let exists_901: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM BlockRange WHERE BlockRangeId = 901)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(exists_901, "no overlap must leave existing rows untouched");

    let (start, end): (i64, i64) = tx
        .query_row(
            "SELECT StartToken, EndToken FROM BlockRange WHERE BlockRangeId = ?1",
            [new_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((start, end), (200, 210));

    tx.rollback().unwrap();
}
