//! Highlights import range-merge geometry tests (08-03-PLAN.md Task 2,
//! IO-02/IO-03, D8-05/D8-08) — the dedicated test file for the single most
//! dangerous piece of code in this milestone: the Phase-7
//! `db::highlights::merge_block_ranges` primitive's first production call
//! site, now driven by parsed `.txt` file content via
//! `db::io::usermark::merge_range_into`.
//!
//! Every test here imports records that resolve to the SAME `(Identifier,
//! LocationId)` grouping key (same scripture Location, same `Identifier`) so
//! the overlap/absorb/union geometry is actually exercised, not just the
//! find-or-insert Location plumbing (already covered in
//! `import_wireformat_tests.rs`).

mod common;

use common::fresh_v16_db;
use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::io::import::{
    apply_import_highlights, apply_import_notes, parse_highlights_file, parse_notes_file,
};
use rusqlite::Connection;

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

fn block_ranges(conn: &Connection) -> Vec<(i64, i64, i64)> {
    let mut stmt = conn
        .prepare("SELECT BlockRangeId, StartToken, EndToken FROM BlockRange ORDER BY StartToken")
        .expect("prepare");
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("read")
}

/// Applies one Highlights `.txt` file's worth of `records` inside its own
/// transaction, threading a fresh `available` id pool each call — mirrors
/// what `import_highlights_apply` does per IPC call (`compute_available_ids`
/// is recomputed once per real transaction, never reused across commits).
fn apply_lines(conn: &mut Connection, text: &str, guid_seed: u64) {
    let records = parse_highlights_file(text).expect("parse");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    apply_import_highlights(&tx, &records, &mut available, guid_seed).expect("apply");
    tx.commit().expect("commit");
}

#[test]
fn two_overlapping_ranges_merge_into_one_union_and_absorbed_row_is_gone() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    // Record A: Identifier 1, range [0, 5].
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0", 1);
    assert_eq!(block_ranges(&conn), vec![(1, 0, 5)]);

    // Record B: SAME (Identifier, Location) — range [3, 10] overlaps [0, 5].
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|3|10|2|1|1|1|None|0|nwt|0|0", 2);

    let ranges = block_ranges(&conn);
    assert_eq!(ranges.len(), 1, "the overlapping ranges must merge into exactly one row");
    assert_eq!((ranges[0].1, ranges[0].2), (0, 10), "the union must span [0, 10]");
    // The absorbed row's ORIGINAL content ([0, 5]) must no longer exist as
    // its own row — asserted by content rather than by `BlockRangeId`
    // because SQLite reuses a just-freed max-rowid on the very next INSERT
    // when a table's PK lacks the `AUTOINCREMENT` keyword (this schema's
    // `BlockRange` does), so the merged row can legitimately land on the
    // SAME integer id the absorbed row vacated — that is expected SQLite
    // rowid-reuse behavior, not evidence the delete never happened.
    let old_row_still_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM BlockRange WHERE StartToken = 0 AND EndToken = 5",
            [],
            |r| r.get(0),
        )
        .expect("count old-shaped row");
    assert_eq!(old_row_still_exists, 0, "the absorbed row's original [0, 5] shape must be gone");
}

#[test]
fn overlapping_range_of_a_different_color_still_merges() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    // ColorIndex 1 (field 4) on the first import.
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0", 10);
    // ColorIndex 5 on the overlapping second import — grouping is by
    // (Identifier, LocationId) ONLY, never filtered by color (D8-05).
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|4|12|5|1|1|1|None|0|nwt|0|0", 11);

    let ranges = block_ranges(&conn);
    assert_eq!(ranges.len(), 1, "a different ColorIndex must not prevent the merge");
    assert_eq!((ranges[0].1, ranges[0].2), (0, 12));
}

#[test]
fn three_chained_overlapping_imports_coalesce_to_one_range() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0", 1);
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|3|10|1|1|1|1|None|0|nwt|0|0", 2);
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|8|15|1|1|1|1|None|0|nwt|0|0", 3);

    let ranges = block_ranges(&conn);
    assert_eq!(ranges.len(), 1, "three chained overlaps must coalesce to one range");
    assert_eq!((ranges[0].1, ranges[0].2), (0, 15));
}

#[test]
fn disjoint_ranges_at_the_same_identifier_stay_as_separate_rows() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0", 1);
    // Disjoint: starts well past the first range's end (ce=5 < ns=100).
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|100|110|1|1|1|1|None|0|nwt|0|0", 2);

    let ranges = block_ranges(&conn);
    assert_eq!(ranges.len(), 2, "disjoint ranges must not merge");
}

#[test]
fn reimporting_the_same_file_converges_blockrange_geometry_while_usermark_grows() {
    // RESEARCH Pitfall 5 / must-have: Highlights import is explicitly NOT
    // idempotent at the UserMark level. This test asserts BlockRange
    // geometric convergence, NEVER UserMark row-count stability — asserting
    // the latter would be asserting a falsehood about Python's own behavior.
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");
    let text = "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0";

    apply_lines(&mut conn, text, 1);
    let after_first = block_ranges(&conn);
    assert_eq!(after_first.len(), 1);
    assert_eq!((after_first[0].1, after_first[0].2), (0, 5));
    assert_eq!(count(&conn, "UserMark"), 1);

    // Re-import the IDENTICAL file into the SAME archive.
    apply_lines(&mut conn, text, 2);

    let after_second = block_ranges(&conn);
    assert_eq!(
        after_second.len(),
        1,
        "the identical range must absorb itself back into one row, not duplicate"
    );
    assert_eq!((after_second[0].1, after_second[0].2), (0, 5), "geometry must be stable");
    assert_eq!(
        count(&conn, "UserMark"),
        2,
        "each import synthesizes a fresh UserMark — accepted non-idempotency, not a bug"
    );
}

#[test]
fn new_block_range_id_consumes_a_recycled_gap_before_autoincrement() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    // Seed a gap at BlockRangeId 1: insert ids 1 and 2, then delete 1.
    {
        let tx_conn = Connection::open(&db_path).expect("open db");
        tx_conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
        tx_conn
            .execute(
                "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
                 VALUES (9, 9, NULL, NULL, 0, 'placeholder', 0, 0)",
                [],
            )
            .expect("insert placeholder location");
        let loc_id = tx_conn.last_insert_rowid();
        tx_conn
            .execute(
                "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
                 VALUES (0, ?1, 0, 'placeholder-um', 1)",
                rusqlite::params![loc_id],
            )
            .expect("insert placeholder usermark");
        let um_id = tx_conn.last_insert_rowid();
        for id in [1_i64, 2] {
            tx_conn
                .execute(
                    "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
                     VALUES (?1, 1, 500, 900, 910, ?2)",
                    rusqlite::params![id, um_id],
                )
                .expect("seed placeholder blockrange");
        }
        tx_conn
            .execute("DELETE FROM BlockRange WHERE BlockRangeId = 1", [])
            .expect("delete to create gap at id 1");
    }

    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0", 1);

    let new_range_id: i64 = conn
        .query_row(
            "SELECT BlockRangeId FROM BlockRange WHERE StartToken = 0 AND EndToken = 5",
            [],
            |r| r.get(0),
        )
        .expect("read new range id");
    assert_eq!(new_range_id, 1, "the new BlockRange must consume the recycled gap id 1");
}

// ---------------------------------------------------------------------------
// Notes' `RANGE` attribute — the SECOND `merge_range_into` call site
// (08-04-PLAN.md Task 2, D8-05). Sub-ranges within ONE record must merge
// SEQUENTIALLY, and Notes' sub-ranges converge through the exact SAME
// `db::highlights::merge_block_ranges` primitive Highlights uses above.
// ---------------------------------------------------------------------------

fn apply_note_lines(conn: &mut Connection, text: &str, guid_seed: u64) {
    let (bucket, records) = parse_notes_file(text).expect("parse");
    let tx = conn.transaction().expect("begin tx");
    let mut available = compute_available_ids(&tx).expect("compute ids");
    apply_import_notes(&tx, bucket, &records, &mut available, guid_seed, "2099-01-01T00:00:00Z")
        .expect("apply");
    tx.commit().expect("commit");
}

#[test]
fn notes_sequential_sub_ranges_merge_into_one_row() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    let text = "{NOTES=}\nheader\n==={CREATED=2024-01-01T00:00:00}{MODIFIED=2024-01-01T00:00:00}{TAGS=}{LANG=0}{PUB=nwt}{BK=1}{CH=1}{VS=5}{Reference=01001005}{COLOR=1}{RANGE=1:5-9;1:8-12}===\nTitle\nNote\n==={END}===";
    apply_note_lines(&mut conn, text, 1);

    let ranges = block_ranges(&conn);
    assert_eq!(ranges.len(), 1, "the second sub-range must see the first sub-range's insert");
    assert_eq!((ranges[0].1, ranges[0].2), (5, 12));
}

#[test]
fn notes_sub_ranges_at_different_identifiers_stay_separate() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    let text = "{NOTES=}\nheader\n==={CREATED=2024-01-01T00:00:00}{MODIFIED=2024-01-01T00:00:00}{TAGS=}{LANG=0}{PUB=nwt}{BK=1}{CH=1}{VS=5}{Reference=01001005}{COLOR=1}{RANGE=1:5-9;2:8-12}===\nTitle\nNote\n==={END}===";
    apply_note_lines(&mut conn, text, 1);

    let ranges = block_ranges(&conn);
    assert_eq!(ranges.len(), 2, "sub-ranges naming different identifiers must not merge");
}

#[test]
fn notes_range_merges_into_an_existing_highlight_range_via_the_shared_primitive() {
    let (_dir, db_path) = fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open db");

    // A Highlights import lands [0, 5] at (Identifier=1, the scripture Location).
    apply_lines(&mut conn, "{HIGHLIGHTS}\n1|1|0|5|1|1|1|1|None|0|nwt|0|0", 1);
    assert_eq!(block_ranges(&conn), vec![(1, 0, 5)]);

    // A Notes import at the SAME (Identifier, Location) with an overlapping
    // RANGE must merge into the SAME row via the one shared primitive — a
    // separate implementation would leave two disjoint rows instead.
    let text = "{NOTES=}\nheader\n==={CREATED=2024-01-01T00:00:00}{MODIFIED=2024-01-01T00:00:00}{TAGS=}{LANG=0}{PUB=nwt}{BK=1}{CH=1}{VS=1}{COLOR=1}{RANGE=3-8}===\nTitle\nNote\n==={END}===";
    apply_note_lines(&mut conn, text, 2);

    let ranges = block_ranges(&conn);
    assert_eq!(ranges.len(), 1, "Notes' range must merge into the existing Highlights BlockRange");
    assert_eq!((ranges[0].1, ranges[0].2), (0, 8));
}
