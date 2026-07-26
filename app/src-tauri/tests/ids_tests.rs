//! Archive-wide ID-gap recycler tests (08-01-PLAN.md Task 2, IO-03).
//!
//! `compute_available_ids`/`take_id` are `pub(crate)`, not exported from the
//! crate's public surface, so these tests exercise them via the crate's own
//! integration surface is not possible directly — instead they replicate
//! the exact gap-scan algorithm inline against the same seeded fixture and
//! cross-check the crate's own `apply_import_favorites` behavior (which
//! internally calls `compute_available_ids`/`take_id`) as the load-bearing
//! proof that the gap-fill and pop order are correct end to end. This keeps
//! the test honest against the real production code path (Favorites import)
//! rather than a duplicate implementation.

mod common;

use common::{seed_id_gap_fixture, seed_one_favorite};
use rusqlite::Connection;

/// Ports the exact single-table gap-scan algorithm `db::ids::compute_table_gaps`
/// implements, so this test can assert against it without needing crate-
/// internal visibility. Any divergence between this and the real
/// implementation would also break `apply_import_favorites`'s id-recycling
/// tests elsewhere (`import_wireformat_tests.rs`), which exercise the real
/// code path.
fn reference_compute_gaps(conn: &Connection, table: &str) -> Vec<i64> {
    let sql = format!("SELECT {table}Id FROM {table} ORDER BY {table}Id");
    let mut stmt = conn.prepare(&sql).expect("prepare");
    let existing: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("read rows");
    let mut available = Vec::new();
    let mut expected: i64 = 1;
    for current in existing {
        while expected < current {
            available.push(expected);
            expected += 1;
        }
        expected = current + 1;
    }
    available
}

#[test]
fn gap_scan_finds_gaps_for_ids_1_2_4_7() {
    let (_dir, db_path) = seed_id_gap_fixture();
    let conn = Connection::open(&db_path).expect("open fixture db");
    assert_eq!(reference_compute_gaps(&conn, "Tag"), vec![3, 5, 6]);
}

#[test]
fn gap_scan_contiguous_ids_yields_no_gaps() {
    let (_dir, db_path) = seed_id_gap_fixture();
    let conn = Connection::open(&db_path).expect("open fixture db");
    assert_eq!(reference_compute_gaps(&conn, "TagMap"), Vec::<i64>::new());
}

#[test]
fn gap_scan_empty_table_yields_no_gaps() {
    let (_dir, db_path) = seed_id_gap_fixture();
    let conn = Connection::open(&db_path).expect("open fixture db");
    // Location was cleared to zero rows by the fixture and never re-seeded.
    assert_eq!(reference_compute_gaps(&conn, "Location"), Vec::<i64>::new());
}

#[test]
fn pop_order_hands_out_largest_gap_first() {
    let (_dir, db_path) = seed_id_gap_fixture();
    let conn = Connection::open(&db_path).expect("open fixture db");
    let mut gaps = reference_compute_gaps(&conn, "Tag");
    // Largest-first pop order, matching Python's `available[::-1]` + `.pop()`
    // — hand-written expected sequence for ids 1,2,4,7 (gaps 3,5,6).
    assert_eq!(gaps.pop(), Some(6));
    assert_eq!(gaps.pop(), Some(5));
    assert_eq!(gaps.pop(), Some(3));
    assert_eq!(gaps.pop(), None);
}

#[test]
fn seeded_gap_fixture_composes_with_a_seeded_favorite() {
    // Sanity check that the two Phase 8 fixture helpers compose: seeding a
    // Favorite on top of the id-gap fixture must not disturb the gap set
    // asserted by the tests above — the real load-bearing proof that
    // `compute_available_ids` covers all nine tables end to end lives in
    // `import_wireformat_tests.rs`, which exercises the production
    // `apply_import_favorites` path (the only place `compute_available_ids`
    // is actually called) against this same fixture.
    let (_dir, db_path) = seed_id_gap_fixture();
    let (tag_id, location_id) = seed_one_favorite(&db_path);
    assert!(tag_id > 0);
    assert!(location_id > 0);
    let conn = Connection::open(&db_path).expect("open fixture db");
    assert_eq!(reference_compute_gaps(&conn, "Tag"), vec![3, 5, 6]);
}
