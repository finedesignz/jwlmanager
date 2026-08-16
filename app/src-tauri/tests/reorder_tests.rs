//! EDIT-04 coverage for `db::reorder` (07-03-PLAN.md Task 2) on a synthetic
//! v16 fixture — 0-based dense positions ordered by NoteId, the adversarial
//! max-collision permutation, the zero-change fixture, Type=0/2 tags left
//! untouched, and idempotent composition with save's `trim_sweep`
//! re-densify.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::reorder::apply_reorder;
use jwlmanager_lib::db::trim::trim_sweep;
use rusqlite::Connection;

fn insert_note(conn: &Connection, note_id: i64) {
    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, \
         LastModified, Created, BlockType, BlockIdentifier) \
         VALUES (?1, ?2, NULL, NULL, ?3, 'content', '2026-01-01T00:00:00Z', \
         '2026-01-01T00:00:00Z', 0, NULL)",
        rusqlite::params![
            note_id,
            format!("fixture-reorder-note-{note_id}"),
            format!("Note {note_id}")
        ],
    )
    .expect("insert Note");
}

/// The adversarial max-collision fixture: `Tag 600`'s three `TagMap` rows
/// are seeded at a `Position` ordering that is the EXACT INVERSE of the
/// desired `NoteId`-ascending order — TagMapId 1 (NoteId 30) holds
/// `Position 0`, TagMapId 2 (NoteId 20) holds `Position 1` (already
/// correct), TagMapId 3 (NoteId 10) holds `Position 2`. Writing the target
/// ordering (10->0, 20->1, 30->2) with a naive single-statement-per-row
/// UPDATE would collide: TagMapId 3 wants `Position 0`, which TagMapId 1
/// currently occupies, at every intermediate step of a naive top-to-bottom
/// rewrite — the exact `UNIQUE(TagId, Position)` hazard D7-05 exists to
/// solve.
fn seed_max_collision_fixture(conn: &Connection) {
    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (600, 1, 'Collision Tag')",
        [],
    )
    .expect("insert Tag 600");
    for note_id in [10_i64, 20, 30] {
        insert_note(conn, note_id);
    }
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (1, NULL, NULL, 30, 600, 0)",
        [],
    )
    .expect("insert TagMap 1 (NoteId 30, Position 0)");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (2, NULL, NULL, 20, 600, 1)",
        [],
    )
    .expect("insert TagMap 2 (NoteId 20, Position 1)");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (3, NULL, NULL, 10, 600, 2)",
        [],
    )
    .expect("insert TagMap 3 (NoteId 10, Position 2)");
}

fn open_seeded_with(seed: impl FnOnce(&Connection)) -> (tempfile::TempDir, Connection) {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed(&conn);
    (dir, conn)
}

fn positions_for_tag(conn: &Connection, tag_id: i64) -> Vec<(i64, i64)> {
    let mut stmt = conn
        .prepare("SELECT NoteId, Position FROM TagMap WHERE TagId = ?1 ORDER BY NoteId")
        .unwrap();
    stmt.query_map([tag_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn reorder_produces_zero_based_dense_positions_ordered_by_note_id() {
    let (_dir, conn) = open_seeded_with(seed_max_collision_fixture);
    let tx = conn.unchecked_transaction().expect("open tx");
    let changed = apply_reorder(&tx).expect("apply_reorder must not raise a UNIQUE violation");
    assert_eq!(
        changed, 2,
        "NoteId 20 keeps its Position 1 unchanged; the other two rows move"
    );

    assert_eq!(
        positions_for_tag(&tx, 600),
        vec![(10, 0), (20, 1), (30, 2)],
        "positions must be 0-based dense, ordered by NoteId ascending"
    );

    tx.rollback().unwrap();
}

#[test]
fn every_tag_id_sorted_position_set_equals_0_to_n() {
    let (_dir, conn) = open_seeded_with(|conn| {
        conn.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (601, 1, 'Second Tag')",
            [],
        )
        .unwrap();
        for note_id in [100_i64, 200] {
            insert_note(conn, note_id);
        }
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (10, NULL, NULL, 200, 601, 5)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (11, NULL, NULL, 100, 601, 9)",
            [],
        )
        .unwrap();
        seed_max_collision_fixture(conn);
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_reorder(&tx).expect("apply_reorder must succeed");

    for tag_id in [600_i64, 601] {
        let mut positions: Vec<i64> = positions_for_tag(&tx, tag_id)
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        positions.sort_unstable();
        let expected: Vec<i64> = (0..positions.len() as i64).collect();
        assert_eq!(
            positions, expected,
            "TagId {tag_id}'s position set must equal 0..n"
        );
    }

    tx.rollback().unwrap();
}

#[test]
fn already_sorted_dense_fixture_reports_zero_changes_and_is_unchanged() {
    let (_dir, conn) = open_seeded_with(|conn| {
        conn.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (610, 1, 'Sorted Tag')",
            [],
        )
        .unwrap();
        for note_id in [1_i64, 2, 3] {
            insert_note(conn, note_id);
        }
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (20, NULL, NULL, 1, 610, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (21, NULL, NULL, 2, 610, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (22, NULL, NULL, 3, 610, 2)",
            [],
        )
        .unwrap();
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let before = positions_for_tag(&tx, 610);
    let changed = apply_reorder(&tx).expect("apply_reorder must succeed");
    assert_eq!(
        changed, 0,
        "an already-sorted-dense fixture must report zero changes"
    );
    assert_eq!(
        positions_for_tag(&tx, 610),
        before,
        "positions must be unchanged"
    );

    tx.rollback().unwrap();
}

#[test]
fn favorite_and_playlist_tags_are_never_touched() {
    let (_dir, conn) = open_seeded_with(|conn| {
        seed_max_collision_fixture(conn);
        // Type 0 (Favorite) and Type 2 (Playlist) tags with out-of-order
        // positions of their own — reorder must leave both untouched.
        conn.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (700, 0, 'Fixture Favorite Alt')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (701, 2, 'Playlist Tag')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
             IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
             VALUES (900, NULL, NULL, NULL, NULL, 0, 'fav', 0, 1, NULL, NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (50, NULL, 900, NULL, 700, 7)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO PlaylistItem (PlaylistItemId, Label, StartTrimOffsetTicks, \
             EndTrimOffsetTicks, Accuracy, EndAction, ThumbnailFilePath) \
             VALUES (900, 'Fixture Song', NULL, NULL, 1, 1, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (51, 900, NULL, NULL, 701, 9)",
            [],
        )
        .unwrap();
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_reorder(&tx).expect("apply_reorder must succeed");

    let favorite_position: i64 = tx
        .query_row("SELECT Position FROM TagMap WHERE TagMapId = 50", [], |r| {
            r.get(0)
        })
        .unwrap();
    let playlist_position: i64 = tx
        .query_row("SELECT Position FROM TagMap WHERE TagMapId = 51", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        favorite_position, 7,
        "Type=0 (Favorite) tag rows must be untouched"
    );
    assert_eq!(
        playlist_position, 9,
        "Type=2 (Playlist) tag rows must be untouched"
    );

    tx.rollback().unwrap();
}

#[test]
fn reorder_then_save_path_redensify_is_idempotent() {
    let (_dir, conn) = open_seeded_with(seed_max_collision_fixture);
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_reorder(&tx).expect("apply_reorder must succeed");
    let after_reorder = positions_for_tag(&tx, 600);

    // Compose with the save-path re-densify (trim_sweep, which includes
    // redensify_tag_positions) and assert IDENTICAL normalized state — the
    // second technique running over data the first already made dense must
    // be a true no-op.
    trim_sweep(&tx).expect("trim_sweep must succeed");
    let after_trim = positions_for_tag(&tx, 600);

    assert_eq!(
        after_reorder, after_trim,
        "reorder + save's re-densify must compose idempotently"
    );

    tx.rollback().unwrap();
}
