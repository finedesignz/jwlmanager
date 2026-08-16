//! EDIT-07 coverage for `db::record_edit` (07-05-PLAN.md Task 1) on a
//! synthetic v16 fixture — every behavior bullet: Note field save + color
//! synthesis, Annotation Value save keyed by `(LocationId, TextTag)` with a
//! sibling TextTag left untouched, single-record delete for both categories,
//! and deterministic `LastModified` stamping.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::record_edit::{
    apply_record_delete, apply_record_edit, fetch_record_fields, RecordEditFields,
    RecordEditPayload, RecordIdentity,
};
use rusqlite::Connection;

const SEED: u64 = 909090;
const NOW: &str = "2026-02-01T00:00:00Z";

/// Seeds:
///   - Location 500 (scripture, English).
///   - Note 500: LocationId 500, UserMarkId NULL — plain note eligible for
///     color synthesis.
///   - Note 501: LocationId 500, UserMarkId = 500 (an existing UserMark,
///     ColorIndex 1) — already-colored note.
///   - InputField (500, 'tag-a') and (500, 'tag-b') — two TextTags at the
///     SAME LocationId, so a test can assert editing/deleting one leaves the
///     sibling intact (rule #10 / the load-bearing distinctness check).
fn seed_fixture(conn: &Connection) {
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (500, 1, 1, NULL, NULL, 0, 'nwt', 0, 0, 'Genesis 1:1', NULL, NULL)",
        [],
    )
    .expect("insert Location");

    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (500, 'fixture-record-edit-note-0500', NULL, 500, 'Original title', \
         'Original content', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 2, 1)",
        [],
    )
    .expect("insert Note 500 (plain, eligible for color synthesis)");

    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (500, 1, 500, 0, 'fixture-record-edit-usermark-0500', 1)",
        [],
    )
    .expect("insert UserMark 500");
    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (501, 'fixture-record-edit-note-0501', 500, 500, 'Already colored', \
         'content', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 2, 1)",
        [],
    )
    .expect("insert Note 501 (already has a UserMark)");

    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (500, 'tag-a', 'value a')",
        [],
    )
    .expect("insert InputField tag-a");
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (500, 'tag-b', 'value b')",
        [],
    )
    .expect("insert InputField tag-b");
}

fn open_seeded() -> (tempfile::TempDir, Connection) {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed_fixture(&conn);
    (dir, conn)
}

#[test]
fn saving_a_note_updates_title_content_and_stamps_last_modified() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let payload = RecordEditPayload::Notes {
        note_id: 501,
        title: "New title".to_string(),
        content: "New content".to_string(),
        color_index: None,
    };
    apply_record_edit(&tx, &payload, NOW, SEED).expect("apply_record_edit must succeed");

    let (title, content, last_modified): (String, String, String) = tx
        .query_row(
            "SELECT Title, Content, LastModified FROM Note WHERE NoteId = 501",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(title, "New title");
    assert_eq!(content, "New content");
    assert_eq!(last_modified, NOW);

    tx.rollback().unwrap();
}

#[test]
fn saving_a_note_with_no_usermark_synthesizes_one_via_the_shared_color_path() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let payload = RecordEditPayload::Notes {
        note_id: 500,
        title: "Title".to_string(),
        content: "Content".to_string(),
        color_index: Some(4),
    };
    apply_record_edit(&tx, &payload, NOW, SEED).expect("apply_record_edit must succeed");

    let user_mark_id: Option<i64> = tx
        .query_row("SELECT UserMarkId FROM Note WHERE NoteId = 500", [], |r| {
            r.get(0)
        })
        .unwrap();
    let user_mark_id = user_mark_id.expect("Note 500 must now have a synthesized UserMarkId");
    let color_index: i64 = tx
        .query_row(
            "SELECT ColorIndex FROM UserMark WHERE UserMarkId = ?1",
            [user_mark_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(color_index, 4);

    tx.rollback().unwrap();
}

#[test]
fn saving_a_note_with_an_existing_usermark_updates_its_colorindex_no_new_synthesis() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let before_count: i64 = tx
        .query_row("SELECT COUNT(*) FROM UserMark", [], |r| r.get(0))
        .unwrap();

    let payload = RecordEditPayload::Notes {
        note_id: 501,
        title: "Already colored".to_string(),
        content: "content".to_string(),
        color_index: Some(6),
    };
    apply_record_edit(&tx, &payload, NOW, SEED).expect("apply_record_edit must succeed");

    let after_count: i64 = tx
        .query_row("SELECT COUNT(*) FROM UserMark", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        before_count, after_count,
        "no new UserMark must be synthesized"
    );

    let color_index: i64 = tx
        .query_row(
            "SELECT ColorIndex FROM UserMark WHERE UserMarkId = 500",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(color_index, 6);

    tx.rollback().unwrap();
}

#[test]
fn saving_an_annotation_updates_only_that_texttag_sibling_untouched() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let payload = RecordEditPayload::Annotations {
        location_id: 500,
        text_tag: "tag-a".to_string(),
        value: "updated a".to_string(),
    };
    apply_record_edit(&tx, &payload, NOW, SEED).expect("apply_record_edit must succeed");

    let value_a: String = tx
        .query_row(
            "SELECT Value FROM InputField WHERE LocationId = 500 AND TextTag = 'tag-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value_a, "updated a");

    let value_b: String = tx
        .query_row(
            "SELECT Value FROM InputField WHERE LocationId = 500 AND TextTag = 'tag-b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        value_b, "value b",
        "sibling TextTag at the same LocationId must be untouched"
    );

    tx.rollback().unwrap();
}

#[test]
fn deleting_a_note_record_removes_only_that_note() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let deleted = apply_record_delete(&tx, &RecordIdentity::Notes { note_id: 500 })
        .expect("apply_record_delete must succeed");
    assert_eq!(deleted, 1);

    let remaining: i64 = tx
        .query_row("SELECT COUNT(*) FROM Note WHERE NoteId = 500", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(remaining, 0);
    let other_note_survives: i64 = tx
        .query_row("SELECT COUNT(*) FROM Note WHERE NoteId = 501", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(other_note_survives, 1);

    tx.rollback().unwrap();
}

/// The load-bearing distinctness check: the editor's Annotation delete is
/// keyed by `(LocationId, TextTag)` and must NEVER over-delete like the
/// browse-list's by-LocationId delete does (rule #10).
#[test]
fn deleting_an_annotation_record_leaves_the_sibling_texttag_at_the_same_location_intact() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    let deleted = apply_record_delete(
        &tx,
        &RecordIdentity::Annotations {
            location_id: 500,
            text_tag: "tag-a".to_string(),
        },
    )
    .expect("apply_record_delete must succeed");
    assert_eq!(deleted, 1);

    let tag_a_gone: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 500 AND TextTag = 'tag-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tag_a_gone, 0);

    let tag_b_survives: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 500 AND TextTag = 'tag-b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        tag_b_survives, 1,
        "sibling TextTag at the same LocationId must survive the editor's scoped delete"
    );

    tx.rollback().unwrap();
}

#[test]
fn apply_record_edit_same_now_produces_identical_last_modified_across_two_calls() {
    let (_dir1, conn1) = open_seeded();
    let tx1 = conn1.unchecked_transaction().expect("open tx1");
    let payload = RecordEditPayload::Notes {
        note_id: 501,
        title: "T".to_string(),
        content: "C".to_string(),
        color_index: None,
    };
    apply_record_edit(&tx1, &payload, NOW, SEED).unwrap();
    let last_modified_1: String = tx1
        .query_row(
            "SELECT LastModified FROM Note WHERE NoteId = 501",
            [],
            |r| r.get(0),
        )
        .unwrap();
    tx1.rollback().unwrap();

    let (_dir2, conn2) = open_seeded();
    let tx2 = conn2.unchecked_transaction().expect("open tx2");
    apply_record_edit(&tx2, &payload, NOW, SEED).unwrap();
    let last_modified_2: String = tx2
        .query_row(
            "SELECT LastModified FROM Note WHERE NoteId = 501",
            [],
            |r| r.get(0),
        )
        .unwrap();
    tx2.rollback().unwrap();

    assert_eq!(
        last_modified_1, last_modified_2,
        "same injected `now` must produce identical LastModified"
    );
    assert_eq!(last_modified_1, NOW);
}

#[test]
fn fetch_record_fields_returns_current_note_values_and_no_color_when_unmarked() {
    let (_dir, conn) = open_seeded();
    let fields = fetch_record_fields(&conn, &RecordIdentity::Notes { note_id: 500 })
        .expect("fetch_record_fields must succeed");
    match fields {
        RecordEditFields::Notes {
            title,
            content,
            color_index,
        } => {
            assert_eq!(title, "Original title");
            assert_eq!(content, "Original content");
            assert_eq!(color_index, None, "Note 500 has no UserMark yet");
        }
        RecordEditFields::Annotations { .. } => panic!("expected Notes variant"),
    }
}

#[test]
fn fetch_record_fields_returns_current_color_when_a_usermark_is_linked() {
    let (_dir, conn) = open_seeded();
    let fields = fetch_record_fields(&conn, &RecordIdentity::Notes { note_id: 501 })
        .expect("fetch_record_fields must succeed");
    match fields {
        RecordEditFields::Notes { color_index, .. } => assert_eq!(color_index, Some(1)),
        RecordEditFields::Annotations { .. } => panic!("expected Notes variant"),
    }
}

#[test]
fn fetch_record_fields_returns_current_annotation_value() {
    let (_dir, conn) = open_seeded();
    let fields = fetch_record_fields(
        &conn,
        &RecordIdentity::Annotations {
            location_id: 500,
            text_tag: "tag-b".to_string(),
        },
    )
    .expect("fetch_record_fields must succeed");
    match fields {
        RecordEditFields::Annotations { value } => assert_eq!(value, "value b"),
        RecordEditFields::Notes { .. } => panic!("expected Annotations variant"),
    }
}
