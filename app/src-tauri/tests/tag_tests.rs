//! EDIT-03 coverage for `db::tags` (07-03-PLAN.md Task 1) on a synthetic v16
//! fixture — tri-state counts, per-selection add/remove, new-tag creation,
//! ID gap-fill recycling, `INSERT OR IGNORE` no-op re-check, and dry-run
//! leaving the DB unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::delete::NonEmptyNoteIds;
use jwlmanager_lib::db::tags::{apply_tag_edit, dry_run_tag_edit, tag_states};
use rusqlite::Connection;

/// Seeds:
///   - Notes 970, 980 (the selection), 990 (NOT selected).
///   - Tag 500 (Type 1, "Alpha"), mapped ONLY to Note 970 — partially
///     tagged within the selection (indeterminate).
///   - Tag 501 (Type 1, "Beta"), mapped to Note 970, Note 980 (both
///     selected — checked) AND Note 990 (not selected — must survive an
///     unmark of the selection untouched).
///   - Tag 502 (Type 1, "Gamma"), mapped to nothing — unchecked.
fn seed_tag_fixture(conn: &Connection) {
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (970, 1, 1, NULL, NULL, 0, 'nwt', 0, 0, 'Genesis 1:1', NULL, NULL)",
        [],
    )
    .expect("insert Location");

    for note_id in [970_i64, 980, 990] {
        conn.execute(
            "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, \
             LastModified, Created, BlockType, BlockIdentifier) \
             VALUES (?1, ?2, NULL, NULL, ?3, 'content', '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z', 0, NULL)",
            rusqlite::params![note_id, format!("fixture-tag-note-{note_id}"), format!("Note {note_id}")],
        )
        .expect("insert Note");
    }

    conn.execute("INSERT INTO Tag (TagId, Type, Name) VALUES (500, 1, 'Alpha')", [])
        .expect("insert Tag 500");
    conn.execute("INSERT INTO Tag (TagId, Type, Name) VALUES (501, 1, 'Beta')", [])
        .expect("insert Tag 501");
    conn.execute("INSERT INTO Tag (TagId, Type, Name) VALUES (502, 1, 'Gamma')", [])
        .expect("insert Tag 502");

    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (1, NULL, NULL, 970, 500, 0)",
        [],
    )
    .expect("insert TagMap 1 (Tag 500 x Note 970)");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (2, NULL, NULL, 970, 501, 0)",
        [],
    )
    .expect("insert TagMap 2 (Tag 501 x Note 970)");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (3, NULL, NULL, 980, 501, 1)",
        [],
    )
    .expect("insert TagMap 3 (Tag 501 x Note 980)");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (4, NULL, NULL, 990, 501, 2)",
        [],
    )
    .expect("insert TagMap 4 (Tag 501 x Note 990, NOT selected)");
}

fn open_seeded() -> (tempfile::TempDir, Connection) {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed_tag_fixture(&conn);
    (dir, conn)
}

fn selection() -> NonEmptyNoteIds {
    NonEmptyNoteIds::try_from(vec![970_i64, 980]).unwrap()
}

#[test]
fn tag_states_reports_checked_unchecked_and_indeterminate() {
    let (_dir, conn) = open_seeded();
    let states = tag_states(&conn, &selection()).expect("tag_states must succeed");
    assert_eq!(states.len(), 3);

    let by_name = |name: &str| states.iter().find(|s| s.name == name).unwrap().count;
    assert_eq!(by_name("Alpha"), 1, "Alpha: 1 of 2 selected notes -> indeterminate");
    assert!(by_name("Alpha") > 0 && by_name("Alpha") < 2, "must be strictly between 0 and selection size");
    assert_eq!(by_name("Beta"), 2, "Beta: both selected notes carry it -> checked");
    assert_eq!(by_name("Gamma"), 0, "Gamma: neither selected note carries it -> unchecked");
}

#[test]
fn unchecking_a_tag_removes_only_selected_notes_rows() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_tag_edit(&tx, &selection(), &[501], &[], &[]).expect("apply_tag_edit must succeed");

    let remaining_970: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE NoteId = 970 AND TagId = 501",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let remaining_980: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE NoteId = 980 AND TagId = 501",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let remaining_990: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE NoteId = 990 AND TagId = 501",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining_970, 0, "selected Note 970's mapping must be removed");
    assert_eq!(remaining_980, 0, "selected Note 980's mapping must be removed");
    assert_eq!(remaining_990, 1, "un-selected Note 990's mapping must survive");

    tx.rollback().unwrap();
}

#[test]
fn checking_a_tag_inserts_only_for_notes_missing_it_and_ignores_already_mapped() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    // Tag 500 is mapped to 970 already, not to 980.
    apply_tag_edit(&tx, &selection(), &[], &[500], &[]).expect("apply_tag_edit must succeed");

    let count_970: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE NoteId = 970 AND TagId = 500",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let count_980: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE NoteId = 980 AND TagId = 500",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count_970, 1, "already-mapped note must stay at exactly one row (INSERT OR IGNORE no-op)");
    assert_eq!(count_980, 1, "missing note must gain exactly one new row");

    tx.rollback().unwrap();
}

#[test]
fn adding_a_new_tag_name_creates_tag_row_and_maps_selection() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_tag_edit(&tx, &selection(), &[], &[], &["Delta".to_string()])
        .expect("apply_tag_edit must succeed");

    let (tag_id, tag_type): (i64, i64) = tx
        .query_row(
            "SELECT TagId, Type FROM Tag WHERE Name = 'Delta'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("new Tag 'Delta' must exist");
    assert_eq!(tag_type, 1);

    let mapped_count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE TagId = ?1 AND NoteId IN (970, 980)",
            [tag_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mapped_count, 2, "the new tag must map to both selected notes");

    tx.rollback().unwrap();
}

#[test]
fn new_tag_id_recycles_a_freed_gap_rather_than_extending_past_max() {
    let (_dir, conn) = open_seeded();
    let tx = conn.unchecked_transaction().expect("open tx");

    // Delete Tag 501 of {500, 501, 502}, freeing id 501 as a gap.
    tx.execute("DELETE FROM TagMap WHERE TagId = 501", [])
        .unwrap();
    tx.execute("DELETE FROM Tag WHERE TagId = 501", []).unwrap();

    apply_tag_edit(&tx, &selection(), &[], &[], &["Epsilon".to_string()])
        .expect("apply_tag_edit must succeed");

    let new_tag_id: i64 = tx
        .query_row("SELECT TagId FROM Tag WHERE Name = 'Epsilon'", [], |r| r.get(0))
        .expect("new Tag 'Epsilon' must exist");
    assert_eq!(new_tag_id, 501, "the new tag must reuse the freed gap id, not extend past max");

    tx.rollback().unwrap();
}

#[test]
fn dry_run_tag_edit_leaves_the_database_unchanged() {
    let (_dir, mut conn) = open_seeded();
    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();
    let before_tags: i64 = conn
        .query_row("SELECT COUNT(*) FROM Tag", [], |r| r.get(0))
        .unwrap();

    let _report = dry_run_tag_edit(&mut conn, &selection(), &[501], &[500], &["Zeta".to_string()])
        .expect("dry_run_tag_edit must succeed");

    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();
    let after_tags: i64 = conn
        .query_row("SELECT COUNT(*) FROM Tag", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after, "dry-run must leave TagMap row count unchanged");
    assert_eq!(before_tags, after_tags, "dry-run must leave Tag row count unchanged");
}

#[test]
fn dry_run_tag_edit_report_reflects_a_real_change() {
    let (_dir, mut conn) = open_seeded();
    let report = dry_run_tag_edit(&mut conn, &selection(), &[], &[500], &[])
        .expect("dry_run_tag_edit must succeed");
    // 500 already mapped to 970 (overwritten, PK survives), newly mapped to
    // 980 (a fresh TagMapId, added).
    let added = report.added.get("TagMap").copied().unwrap_or(0);
    assert!(added >= 1, "checking a tag for a previously-unmapped note must show as added");
}
