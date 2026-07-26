//! Cross-op semantic round-trip suite (ROADMAP criterion 5, 07-05-PLAN.md
//! Task 3): one test per Phase 7 op group — seed a synthetic v16 fixture,
//! apply the edit, run the REAL save pipeline (trim + VACUUM), reopen, and
//! assert NORMALIZED table state. NEVER a byte diff — save is not
//! byte-preserving (trim_db + VACUUM, mask's RNG, fresh timestamps), so only
//! `common::normalized_table_rows`/targeted-existence queries on the
//! reopened archive are used, per CLAUDE.md's Core Value.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::archive::save::save_archive;
use jwlmanager_lib::db::color::{apply_color, ColorSelection};
use jwlmanager_lib::db::delete::NonEmptyNoteIds;
use jwlmanager_lib::db::favorites::{
    apply_favorite_add, apply_favorite_remove, FavoriteEditionRef, NonEmptyTagMapIds,
};
use jwlmanager_lib::db::highlights::merge_block_ranges;
use jwlmanager_lib::db::record_edit::{apply_record_delete, apply_record_edit, RecordEditPayload, RecordIdentity};
use jwlmanager_lib::db::reorder::apply_reorder;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::db::scrub::{apply_clean, apply_mask};
use jwlmanager_lib::db::tags::apply_tag_edit;
use rusqlite::Connection;
use std::path::Path;

const SEED: u64 = 5150;
const NOW: &str = "2026-02-01T00:00:00Z";

fn open_working_conn(db_path: &Path) -> Connection {
    let conn = Connection::open(db_path).expect("open working db");
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    conn
}

fn save_and_reopen(session: &jwlmanager_lib::session::ArchiveSession) -> (tempfile::TempDir, Connection) {
    save_archive(session, "JWL Manager", "JWL Manager_test", NOW).expect("save must succeed");
    let (dir, reopened) = common::extract_to_tempdir(&session.target_path);
    let conn = Connection::open(reopened.join("userData.db")).expect("open reopened db");
    (dir, conn)
}

/// Color: recoloring a plain Note (LocationId set, UserMarkId NULL) both
/// synthesizes a UserMark AND survives the full save pipeline (EDIT-02).
#[test]
fn color_recolor_synthesizes_usermark_and_survives_save() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = open_working_conn(&session.db_path);
        let tx = conn.unchecked_transaction().unwrap();
        let selection = ColorSelection::Notes {
            ids: NonEmptyNoteIds::try_from(vec![700_i64]).unwrap(),
        };
        apply_color(&tx, &selection, 3, SEED).expect("apply_color must succeed");
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let user_mark_id: Option<i64> = conn
        .query_row("SELECT UserMarkId FROM Note WHERE NoteId = 700", [], |r| r.get(0))
        .unwrap();
    let user_mark_id = user_mark_id.expect("Note 700 must have a synthesized UserMarkId after save");
    let color_index: i64 = conn
        .query_row(
            "SELECT ColorIndex FROM UserMark WHERE UserMarkId = ?1",
            [user_mark_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(color_index, 3);
}

/// Highlights merge: overlapping BlockRanges at the same `Identifier`
/// coalesce into one row; a non-overlapping BlockRange at a DIFFERENT
/// `Identifier` on the same UserMark survives untouched (EDIT-02, D7-03's
/// standalone-primitive coverage requirement).
#[test]
fn highlights_merge_coalesces_overlapping_ranges_leaves_others_untouched() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = open_working_conn(&session.db_path);
        let tx = conn.unchecked_transaction().unwrap();
        // BlockRange 633 is (Identifier 1, Start 0, End 5) on UserMark 650.
        // A new range [3, 8] at Identifier 1 overlaps it and must absorb it.
        merge_block_ranges(&tx, 1, 500, 3, 8, 1, 650, None)
            .expect("merge_block_ranges must succeed");
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let (start, end): (i64, i64) = conn
        .query_row(
            "SELECT StartToken, EndToken FROM BlockRange b JOIN UserMark u USING (UserMarkId) \
             WHERE b.Identifier = 1 AND u.LocationId = 500",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((start, end), (0, 8), "the merged union must survive save");

    let identifier_1_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM BlockRange b JOIN UserMark u USING (UserMarkId) \
             WHERE b.Identifier = 1 AND u.LocationId = 500",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(identifier_1_count, 1, "the absorbed original row must be gone, not just superseded");

    // BlockRange 644 (Identifier 2, Start 6, End 10) is untouched.
    let (other_start, other_end): (i64, i64) = conn
        .query_row(
            "SELECT StartToken, EndToken FROM BlockRange b JOIN UserMark u USING (UserMarkId) \
             WHERE b.Identifier = 2 AND u.LocationId = 500",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((other_start, other_end), (6, 10), "an unrelated Identifier's range must survive untouched");
}

/// Tags: adding an existing tag and a brand-new tag to a Note both land
/// correctly and survive save's TagMap re-densify (EDIT-03).
#[test]
fn tags_add_existing_and_new_tag_survive_save() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = open_working_conn(&session.db_path);
        let tx = conn.unchecked_transaction().unwrap();
        let ids = NonEmptyNoteIds::try_from(vec![700_i64]).unwrap();
        apply_tag_edit(&tx, &ids, &[], &[600], &["Fixture New Tag".to_string()])
            .expect("apply_tag_edit must succeed");
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let existing_tag_mapped: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE NoteId = 700 AND TagId = 600",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(existing_tag_mapped, 1, "Note 700 must be mapped to the existing Tag 600");

    let new_tag_mapped: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM TagMap tm JOIN Tag t USING (TagId) \
             WHERE tm.NoteId = 700 AND t.Name = 'Fixture New Tag'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_tag_mapped, 1, "Note 700 must be mapped to the newly-created tag");
}

/// Reorder: a `Type = 1` tag's gapped `TagMap.Position` values end up
/// 0-based dense, ordered by `NoteId`, and REMAIN so after save's own
/// trim-path re-densify runs on top (idempotent composition, EDIT-04).
#[test]
fn reorder_densifies_positions_idempotently_across_save() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = open_working_conn(&session.db_path);
        conn.execute("INSERT INTO Tag (TagId, Type, Name) VALUES (9000, 1, 'Reorder Fixture Tag')", [])
            .unwrap();
        for (note_id, position) in [(9001_i64, 9_i64), (9002, 0), (9003, 5)] {
            conn.execute(
                "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, \
                 LastModified, Created, BlockType, BlockIdentifier) \
                 VALUES (?1, ?2, NULL, NULL, 'Reorder note', 'content', '2026-01-01T00:00:00Z', \
                 '2026-01-01T00:00:00Z', 0, NULL)",
                rusqlite::params![note_id, format!("fixture-reorder-note-{note_id}")],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
                 VALUES (?1, NULL, NULL, ?2, 9000, ?3)",
                rusqlite::params![note_id, note_id, position],
            )
            .unwrap();
        }
        let tx = conn.unchecked_transaction().unwrap();
        apply_reorder(&tx).expect("apply_reorder must succeed");
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let mut stmt = conn
        .prepare("SELECT NoteId, Position FROM TagMap WHERE TagId = 9000 ORDER BY NoteId")
        .unwrap();
    let rows: Vec<(i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![(9001, 0), (9002, 1), (9003, 2)],
        "positions must be 0-based dense, ordered by NoteId ascending, and survive save"
    );
}

/// Favorites: unmarking an existing favorite removes exactly that TagMap
/// row; marking a new edition inserts one Location + one TagMap row against
/// the system Favorite tag; both survive save (EDIT-05).
#[test]
fn favorites_mark_and_unmark_survive_save() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = open_working_conn(&session.db_path);
        let tx = conn.unchecked_transaction().unwrap();
        // Unmark the fixture's existing favorite (TagMap 622).
        let remove_ids = NonEmptyTagMapIds::try_from(vec![622_i64]).unwrap();
        apply_favorite_remove(&tx, &remove_ids).expect("apply_favorite_remove must succeed");
        // Mark a brand-new edition as a favorite.
        let edition = FavoriteEditionRef {
            symbol: "nwt".to_string(),
            language: 5,
        };
        apply_favorite_add(&tx, &edition).expect("apply_favorite_add must succeed");
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let old_favorite_gone: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap WHERE TagMapId = 622", [], |r| r.get(0))
        .unwrap();
    assert_eq!(old_favorite_gone, 0, "the unmarked favorite must be gone after save");

    let new_favorite_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM TagMap tm JOIN Tag t USING (TagId) JOIN Location l USING (LocationId) \
             WHERE t.Type = 0 AND t.Name = 'Favorite' AND l.KeySymbol = 'nwt' AND l.MepsLanguage = 5",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_favorite_count, 1, "the newly-marked favorite must survive save");
}

/// Clean: Unicode separator junk in an Annotation's Value is normalized to a
/// plain ASCII space and the normalized row-accurate change survives save
/// (EDIT-06).
#[test]
fn clean_normalizes_unicode_separators_and_survives_save() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = open_working_conn(&session.db_path);
        conn.execute(
            "UPDATE InputField SET Value = 'dirty\u{00A0}value' WHERE LocationId = 500 AND TextTag = 'annot-tag'",
            [],
        )
        .unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        let counts = apply_clean(&tx).expect("apply_clean must succeed");
        assert_eq!(counts.get("InputField").copied().unwrap_or(0), 1);
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let value: String = conn
        .query_row(
            "SELECT Value FROM InputField WHERE LocationId = 500 AND TextTag = 'annot-tag'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value, "dirty value", "the NBSP must be normalized to an ASCII space and survive save");
}

/// Mask: every letter in an Annotation's Value is replaced, length and
/// non-letter positions are preserved, and the masked text survives save
/// (EDIT-06, D7-08).
#[test]
fn mask_preserves_shape_and_survives_save() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    let original = "Hello, World!";
    {
        let conn = open_working_conn(&session.db_path);
        conn.execute(
            "UPDATE InputField SET Value = ?1 WHERE LocationId = 500 AND TextTag = 'annot-tag'",
            [original],
        )
        .unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        apply_mask(&tx, SEED).expect("apply_mask must succeed");
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let masked: String = conn
        .query_row(
            "SELECT Value FROM InputField WHERE LocationId = 500 AND TextTag = 'annot-tag'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        masked.chars().count(),
        original.chars().count(),
        "masking must preserve character count (byte length may differ, e.g. the 'børk' word)"
    );
    assert_ne!(masked, original, "masking must actually change the letters");
    // Non-letter positions (comma, space, exclamation) must be preserved verbatim.
    for (a, b) in original.chars().zip(masked.chars()) {
        if !a.is_alphabetic() {
            assert_eq!(a, b, "non-letter characters must be preserved in place");
        }
    }
}

/// Record edit: saving a Note's Title/Content/Color and deleting a single
/// Annotation `(LocationId, TextTag)` record both survive the full save
/// pipeline without over-deleting the sibling TextTag at the same location
/// (EDIT-07, rule #10).
#[test]
fn record_edit_save_and_scoped_delete_survive_save() {
    let (_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (session, _rows) = open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = open_working_conn(&session.db_path);
        // A sibling TextTag at the SAME LocationId as the one we'll delete,
        // so the round-trip can assert it survives (rule #10 distinctness).
        conn.execute(
            "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (500, 'sibling-tag', 'sibling value')",
            [],
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        let edit_payload = RecordEditPayload::Notes {
            note_id: 700,
            title: "Edited title".to_string(),
            content: "Edited content".to_string(),
            color_index: Some(5),
        };
        apply_record_edit(&tx, &edit_payload, NOW, SEED).expect("apply_record_edit must succeed");
        apply_record_delete(
            &tx,
            &RecordIdentity::Annotations {
                location_id: 500,
                text_tag: "annot-tag".to_string(),
            },
        )
        .expect("apply_record_delete must succeed");
        tx.commit().unwrap();
    }

    let (_reopened_dir, conn) = save_and_reopen(&session);

    let (title, content, last_modified): (String, String, String) = conn
        .query_row(
            "SELECT Title, Content, LastModified FROM Note WHERE NoteId = 700",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(title, "Edited title");
    assert_eq!(content, "Edited content");
    assert_eq!(last_modified, NOW);

    let user_mark_id: Option<i64> = conn
        .query_row("SELECT UserMarkId FROM Note WHERE NoteId = 700", [], |r| r.get(0))
        .unwrap();
    let user_mark_id = user_mark_id.expect("Note 700 must have a synthesized UserMarkId");
    let color_index: i64 = conn
        .query_row(
            "SELECT ColorIndex FROM UserMark WHERE UserMarkId = ?1",
            [user_mark_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(color_index, 5);

    let deleted_gone: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 500 AND TextTag = 'annot-tag'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(deleted_gone, 0, "the deleted Annotation record must be gone");

    let sibling_survives: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 500 AND TextTag = 'sibling-tag'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        sibling_survives, 1,
        "the sibling TextTag at the same LocationId must survive the scoped delete (rule #10)"
    );
}
