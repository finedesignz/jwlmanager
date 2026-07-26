//! EDIT-02 coverage for `db::color` (07-02-PLAN.md Task 3) on a synthetic
//! v16 fixture — every `set_color` behavior bullet: Highlights recolor,
//! Highlights+Grey no-op, Note→UserMark synthesis, already-marked Note
//! recolor, and deterministic GUID seeding.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::color::{apply_color, ColorSelection, NonEmptyBlockRangeIds};
use jwlmanager_lib::db::delete::NonEmptyNoteIds;
use rusqlite::Connection;

const SEED: u64 = 424242;

/// Seeds:
///   - Location 950 (scripture, English)
///   - UserMark 950 (ColorIndex 1) on Location 950, with one BlockRange 951
///     — a Highlight, identity = BlockRangeId 951.
///   - Note 960: LocationId 950, UserMarkId NULL — a plain note eligible for
///     synthesis.
///   - Note 961: LocationId 950, UserMarkId = 950 — a note ALREADY linked to
///     a UserMark (shares UserMark 950 with the highlight above).
///   - Note 962: LocationId NULL — an independent note, must never be
///     touched by recolor.
fn seed_color_fixture(conn: &Connection) {
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (950, 1, 1, NULL, NULL, 0, 'nwt', 0, 0, 'Genesis 1:1', NULL, NULL)",
        [],
    )
    .expect("insert Location");

    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (950, 1, 950, 0, 'fixture-color-usermark-0950', 1)",
        [],
    )
    .expect("insert UserMark 950");
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (951, 1, 1, 0, 5, 950)",
        [],
    )
    .expect("insert BlockRange 951");

    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (960, 'fixture-color-note-0960', NULL, 950, 'Plain note', 'content', \
         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 2, 1)",
        [],
    )
    .expect("insert Note 960 (plain, eligible for synthesis)");

    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (961, 'fixture-color-note-0961', 950, 950, 'Already-marked note', 'content', \
         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 2, 1)",
        [],
    )
    .expect("insert Note 961 (already has a UserMark)");

    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (962, 'fixture-color-note-0962', NULL, NULL, 'Independent note', 'content', \
         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, NULL)",
        [],
    )
    .expect("insert Note 962 (independent, LocationId NULL)");
}

fn open_seeded() -> (tempfile::TempDir, Connection) {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed_color_fixture(&conn);
    (dir, conn)
}

#[test]
fn recolor_highlights_updates_usermark_colorindex_only() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let selection = ColorSelection::Highlights {
        ids: NonEmptyBlockRangeIds::try_from(vec![951_i64]).unwrap(),
    };
    apply_color(&tx, &selection, 3, SEED).expect("apply_color must succeed");

    let color: i64 = tx
        .query_row("SELECT ColorIndex FROM UserMark WHERE UserMarkId = 950", [], |r| r.get(0))
        .unwrap();
    assert_eq!(color, 3);

    // No BlockRange row is touched by recolor.
    let (start, end): (i64, i64) = tx
        .query_row(
            "SELECT StartToken, EndToken FROM BlockRange WHERE BlockRangeId = 951",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((start, end), (0, 5));

    tx.rollback().unwrap();
}

#[test]
fn recolor_highlights_with_grey_is_a_silent_no_op() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let selection = ColorSelection::Highlights {
        ids: NonEmptyBlockRangeIds::try_from(vec![951_i64]).unwrap(),
    };
    apply_color(&tx, &selection, 0, SEED).expect("Highlights + Grey must not error");

    let color: i64 = tx
        .query_row("SELECT ColorIndex FROM UserMark WHERE UserMarkId = 950", [], |r| r.get(0))
        .unwrap();
    assert_eq!(color, 1, "ColorIndex must be UNCHANGED (still the seeded value 1)");

    tx.rollback().unwrap();
}

#[test]
fn recolor_notes_synthesizes_usermark_for_plain_note() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let selection = ColorSelection::Notes {
        ids: NonEmptyNoteIds::try_from(vec![960_i64]).unwrap(),
    };
    apply_color(&tx, &selection, 5, SEED).expect("apply_color must succeed");

    let (user_mark_id,): (Option<i64>,) = tx
        .query_row("SELECT UserMarkId FROM Note WHERE NoteId = 960", [], |r| Ok((r.get(0)?,)))
        .unwrap();
    let user_mark_id = user_mark_id.expect("Note 960 must now have a synthesized UserMarkId");

    let (color_index, style_index, version, location_id, guid): (i64, i64, i64, i64, String) = tx
        .query_row(
            "SELECT ColorIndex, StyleIndex, Version, LocationId, UserMarkGuid FROM UserMark WHERE UserMarkId = ?1",
            [user_mark_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(color_index, 5);
    assert_eq!(style_index, 0);
    assert_eq!(version, 1);
    assert_eq!(location_id, 950);
    assert!(!guid.is_empty());

    tx.rollback().unwrap();
}

#[test]
fn recolor_notes_with_grey_is_not_a_no_op_still_synthesizes() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let selection = ColorSelection::Notes {
        ids: NonEmptyNoteIds::try_from(vec![960_i64]).unwrap(),
    };
    apply_color(&tx, &selection, 0, SEED).expect("apply_color must succeed");

    let (user_mark_id,): (Option<i64>,) = tx
        .query_row("SELECT UserMarkId FROM Note WHERE NoteId = 960", [], |r| Ok((r.get(0)?,)))
        .unwrap();
    assert!(user_mark_id.is_some(), "Notes + Grey must still synthesize (unlike Highlights + Grey)");

    tx.rollback().unwrap();
}

#[test]
fn recolor_notes_already_marked_updates_existing_usermark_no_new_synthesis() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let before_count: i64 = tx.query_row("SELECT COUNT(*) FROM UserMark", [], |r| r.get(0)).unwrap();

    let selection = ColorSelection::Notes {
        ids: NonEmptyNoteIds::try_from(vec![961_i64]).unwrap(),
    };
    apply_color(&tx, &selection, 6, SEED).expect("apply_color must succeed");

    let after_count: i64 = tx.query_row("SELECT COUNT(*) FROM UserMark", [], |r| r.get(0)).unwrap();
    assert_eq!(before_count, after_count, "no new UserMark must be synthesized");

    let color: i64 = tx
        .query_row("SELECT ColorIndex FROM UserMark WHERE UserMarkId = 950", [], |r| r.get(0))
        .unwrap();
    assert_eq!(color, 6);

    tx.rollback().unwrap();
}

#[test]
fn recolor_notes_independent_note_is_untouched() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let selection = ColorSelection::Notes {
        ids: NonEmptyNoteIds::try_from(vec![962_i64]).unwrap(),
    };
    apply_color(&tx, &selection, 2, SEED).expect("an independent note must not error");

    let (user_mark_id,): (Option<i64>,) = tx
        .query_row("SELECT UserMarkId FROM Note WHERE NoteId = 962", [], |r| Ok((r.get(0)?,)))
        .unwrap();
    assert!(user_mark_id.is_none(), "a LocationId-NULL note can never be recolored/synthesized");

    tx.rollback().unwrap();
}

#[test]
fn apply_color_same_seed_produces_identical_synthesized_guid() {
    let (_dir1, conn1) = open_seeded();
    let tx1 = conn1.unchecked_transaction().expect("open tx1");
    let selection1 = ColorSelection::Notes {
        ids: NonEmptyNoteIds::try_from(vec![960_i64]).unwrap(),
    };
    apply_color(&tx1, &selection1, 1, SEED).unwrap();
    let guid1: String = tx1
        .query_row(
            "SELECT UserMarkGuid FROM UserMark JOIN Note USING (UserMarkId) WHERE NoteId = 960",
            [],
            |r| r.get(0),
        )
        .unwrap();
    tx1.rollback().unwrap();

    let (_dir2, conn2) = open_seeded();
    let tx2 = conn2.unchecked_transaction().expect("open tx2");
    let selection2 = ColorSelection::Notes {
        ids: NonEmptyNoteIds::try_from(vec![960_i64]).unwrap(),
    };
    apply_color(&tx2, &selection2, 1, SEED).unwrap();
    let guid2: String = tx2
        .query_row(
            "SELECT UserMarkGuid FROM UserMark JOIN Note USING (UserMarkId) WHERE NoteId = 960",
            [],
            |r| r.get(0),
        )
        .unwrap();
    tx2.rollback().unwrap();

    assert_eq!(guid1, guid2, "same seed + same note id must synthesize the identical GUID");
}
