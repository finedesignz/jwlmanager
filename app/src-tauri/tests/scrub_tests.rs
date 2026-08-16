//! EDIT-06 coverage for `db::scrub` (07-04-PLAN.md) on a synthetic v16
//! fixture — Clean's Unicode-separator row counting and Mask's shape
//! invariants (length, case, non-letter identity, determinism-under-seed,
//! and publication-content tables untouched).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::scrub::{apply_clean, apply_mask};
use rusqlite::Connection;

fn open_seeded_with(seed: impl FnOnce(&Connection)) -> (tempfile::TempDir, Connection) {
    let (dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed(&conn);
    (dir, conn)
}

// ---------------------------------------------------------------------------
// Clean
// ---------------------------------------------------------------------------

fn insert_input_field(conn: &Connection, location_id: i64, tag: &str, value: &str) {
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (?1, NULL, NULL, 0, NULL, 0, NULL, 0, 2, NULL, NULL, NULL)",
        rusqlite::params![location_id],
    )
    .expect("insert Location for InputField");
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?1, ?2, ?3)",
        rusqlite::params![location_id, tag, value],
    )
    .expect("insert InputField");
}

fn insert_note(conn: &Connection, note_id: i64, title: &str, content: &str) {
    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, \
         LastModified, Created, BlockType, BlockIdentifier) \
         VALUES (?1, ?2, NULL, NULL, ?3, ?4, '2026-01-01T00:00:00Z', \
         '2026-01-01T00:00:00Z', 0, NULL)",
        rusqlite::params![
            note_id,
            format!("fixture-scrub-note-{note_id}"),
            title,
            content
        ],
    )
    .expect("insert Note");
}

#[test]
fn clean_normalizes_nbsp_and_ideographic_space_to_ascii_space_in_input_field() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_input_field(conn, 1, "tag1", "a\u{00A0}b\u{3000}c");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let counts = apply_clean(&tx).expect("apply_clean must succeed");
    assert_eq!(counts.get("InputField"), Some(&1));

    let value: String = tx
        .query_row(
            "SELECT Value FROM InputField WHERE TextTag = 'tag1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value, "a b c");

    tx.rollback().unwrap();
}

#[test]
fn clean_leaves_ascii_space_unchanged() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_input_field(conn, 1, "tag1", "a b c");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let counts = apply_clean(&tx).expect("apply_clean must succeed");
    assert!(
        !counts.contains_key("InputField"),
        "an already-clean ASCII-only row must not be counted"
    );
    tx.rollback().unwrap();
}

#[test]
fn clean_removes_line_and_paragraph_separators() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_input_field(conn, 1, "tag1", "a\u{2028}b\u{2029}c");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_clean(&tx).expect("apply_clean must succeed");
    let value: String = tx
        .query_row(
            "SELECT Value FROM InputField WHERE TextTag = 'tag1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        value, "abc",
        "Zl/Zp separators must be removed, not space-substituted"
    );
    tx.rollback().unwrap();
}

#[test]
fn clean_converts_cr_to_lf() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_note(conn, 1, "title", "line1\r\nline2");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let counts = apply_clean(&tx).expect("apply_clean must succeed");
    assert_eq!(counts.get("Note"), Some(&1));
    let content: String = tx
        .query_row("SELECT Content FROM Note WHERE NoteId = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(content, "line1\n\nline2");
    tx.rollback().unwrap();
}

#[test]
fn clean_row_with_two_separators_counts_once() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_input_field(conn, 1, "tag1", "a\u{00A0}b\u{00A0}c");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let counts = apply_clean(&tx).expect("apply_clean must succeed");
    assert_eq!(
        counts.get("InputField"),
        Some(&1),
        "a row with two separators must increment the count by exactly 1"
    );
    tx.rollback().unwrap();
}

#[test]
fn clean_note_touches_title_and_content_independently_but_counts_row_once() {
    let (_dir, conn) = open_seeded_with(|conn| {
        // Only Title has a separator; Content is already clean.
        insert_note(conn, 1, "a\u{00A0}b", "already clean");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let counts = apply_clean(&tx).expect("apply_clean must succeed");
    assert_eq!(counts.get("Note"), Some(&1));
    let (title, content): (String, String) = tx
        .query_row(
            "SELECT Title, Content FROM Note WHERE NoteId = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(title, "a b");
    assert_eq!(
        content, "already clean",
        "untouched field must be preserved verbatim"
    );
    tx.rollback().unwrap();
}

// ---------------------------------------------------------------------------
// Mask
// ---------------------------------------------------------------------------

fn insert_bookmark(
    conn: &Connection,
    bookmark_id: i64,
    location_id: i64,
    title: &str,
    snippet: &str,
) {
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (?1, NULL, NULL, 0, NULL, 0, NULL, 0, 2, NULL, NULL, NULL)",
        rusqlite::params![location_id],
    )
    .expect("insert Location for Bookmark");
    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) VALUES (?1, ?2, ?2, 0, ?3, ?4, 0, NULL)",
        rusqlite::params![bookmark_id, location_id, title, snippet],
    )
    .expect("insert Bookmark");
}

fn insert_location_with_title(conn: &Connection, location_id: i64, title: &str) {
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (?1, NULL, NULL, 0, NULL, 0, NULL, 0, 2, ?2, NULL, NULL)",
        rusqlite::params![location_id, title],
    )
    .expect("insert Location with Title");
}

#[test]
fn mask_preserves_length_for_mixed_script_fixture() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_note(conn, 1, "Héllo Мир 123 !@# \u{1F600}", "content");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_mask(&tx, 42).expect("apply_mask must succeed");
    let title: String = tx
        .query_row("SELECT Title FROM Note WHERE NoteId = 1", [], |r| r.get(0))
        .unwrap();
    let input = "Héllo Мир 123 !@# \u{1F600}";
    assert_eq!(input.chars().count(), title.chars().count());
    tx.rollback().unwrap();
}

#[test]
fn mask_leaves_every_non_letter_position_byte_identical() {
    let input = "a1 b2! c3\u{1F600}";
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_note(conn, 1, input, "content");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_mask(&tx, 7).expect("apply_mask must succeed");
    let title: String = tx
        .query_row("SELECT Title FROM Note WHERE NoteId = 1", [], |r| r.get(0))
        .unwrap();
    for (i, o) in input.chars().zip(title.chars()) {
        if !i.is_alphabetic() {
            assert_eq!(i, o, "non-letter char must be byte-identical");
        }
    }
    tx.rollback().unwrap();
}

#[test]
fn mask_preserves_case_per_character() {
    let input = "AbCdEf";
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_note(conn, 1, input, "content");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_mask(&tx, 99).expect("apply_mask must succeed");
    let title: String = tx
        .query_row("SELECT Title FROM Note WHERE NoteId = 1", [], |r| r.get(0))
        .unwrap();
    for (i, o) in input.chars().zip(title.chars()) {
        assert_eq!(i.is_uppercase(), o.is_uppercase());
    }
    tx.rollback().unwrap();
}

#[test]
fn mask_same_seed_produces_identical_table_state() {
    let build = || {
        open_seeded_with(|conn| {
            insert_note(conn, 1, "Same Seed Title", "Same Seed Content");
            insert_input_field(conn, 10, "tag1", "Some Value Here");
        })
    };

    let (_dir1, conn1) = build();
    let tx1 = conn1.unchecked_transaction().expect("open tx1");
    apply_mask(&tx1, 12345).expect("apply_mask must succeed");
    let note1: (String, String) = tx1
        .query_row(
            "SELECT Title, Content FROM Note WHERE NoteId = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let field1: String = tx1
        .query_row(
            "SELECT Value FROM InputField WHERE TextTag = 'tag1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    tx1.rollback().unwrap();

    let (_dir2, conn2) = build();
    let tx2 = conn2.unchecked_transaction().expect("open tx2");
    apply_mask(&tx2, 12345).expect("apply_mask must succeed");
    let note2: (String, String) = tx2
        .query_row(
            "SELECT Title, Content FROM Note WHERE NoteId = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let field2: String = tx2
        .query_row(
            "SELECT Value FROM InputField WHERE TextTag = 'tag1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    tx2.rollback().unwrap();

    assert_eq!(
        note1, note2,
        "same seed must produce identical Note table state"
    );
    assert_eq!(
        field1, field2,
        "same seed must produce identical InputField table state"
    );
}

#[test]
fn mask_covers_input_field_bookmark_note_and_location() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_input_field(conn, 1, "annot-tag", "annotation value");
        insert_bookmark(conn, 2, 2, "Bookmark Title", "Bookmark Snippet");
        insert_note(conn, 3, "Note Title", "Note Content");
        insert_location_with_title(conn, 4, "Location Title");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let counts = apply_mask(&tx, 5).expect("apply_mask must succeed");
    assert_eq!(counts.get("InputField"), Some(&1));
    assert_eq!(counts.get("Bookmark"), Some(&1));
    assert_eq!(counts.get("Note"), Some(&1));
    assert_eq!(counts.get("Location"), Some(&1));

    let field: String = tx
        .query_row(
            "SELECT Value FROM InputField WHERE TextTag = 'annot-tag'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(field, "annotation value");
    let (bm_title, bm_snippet): (String, String) = tx
        .query_row(
            "SELECT Title, Snippet FROM Bookmark WHERE BookmarkId = 2",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_ne!(bm_title, "Bookmark Title");
    assert_ne!(bm_snippet, "Bookmark Snippet");
    let loc_title: String = tx
        .query_row("SELECT Title FROM Location WHERE LocationId = 4", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(loc_title, "Location Title");

    tx.rollback().unwrap();
}

#[test]
fn mask_never_touches_publication_content_tables() {
    // `Resource`/`Tag`/`PlaylistItem` etc. are never in scope for mask — this
    // asserts the seeded system Favorite Tag row (Type=0, res/blank) is
    // byte-identical before/after, proving mask's column allowlist holds.
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_note(conn, 1, "Note Title", "Note Content");
    });
    let before = common::normalized_table_rows(&conn, "Tag");
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_mask(&tx, 3).expect("apply_mask must succeed");
    let after = common::normalized_table_rows(&tx, "Tag");
    assert_eq!(before, after, "Tag table must be untouched by mask");
    tx.rollback().unwrap();
}

#[test]
fn mask_skips_empty_rows_and_does_not_count_them() {
    let (_dir, conn) = open_seeded_with(|conn| {
        insert_input_field(conn, 1, "empty-tag", "");
    });
    let tx = conn.unchecked_transaction().expect("open tx");
    let counts = apply_mask(&tx, 1).expect("apply_mask must succeed");
    assert!(
        !counts.contains_key("InputField"),
        "an empty-value row has nothing to mask and must not be counted"
    );
    tx.rollback().unwrap();
}

#[test]
fn no_rand_or_fancy_regex_dependency_declared() {
    let cargo_toml = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("read Cargo.toml");
    assert!(
        !cargo_toml.lines().any(
            |l| l.trim_start().starts_with("rand") || l.trim_start().starts_with("fancy-regex")
        ),
        "no rand/fancy-regex dependency may be declared without a recorded legitimacy checkpoint"
    );
}
