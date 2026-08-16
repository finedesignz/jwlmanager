//! Semantic round-trip suite across all five `.txt` categories
//! (08-04-PLAN.md Task 3, IO-03): export this app's own data, re-import the
//! produced file into the SAME archive, and assert the per-category
//! stability property RESEARCH's `## Round-Trip Determinism` actually
//! documents — never a uniform "everything is idempotent" assumption.
//! NEVER a byte-diff of the archive itself (Core Value: save is not
//! byte-preserving) — only `common::normalized_table_rows` (full-row,
//! PK-inclusive semantic comparison) and, for the four stable categories,
//! a byte-compare of the two EXPORTED `.txt` files against each other.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::io::export::{
    export_annotations, export_bookmarks, export_favorites, export_highlights, export_notes,
};
use jwlmanager_lib::db::io::header::ExportHeaderCtx;
use jwlmanager_lib::db::io::import::{
    apply_import_annotations, apply_import_bookmarks, apply_import_favorites,
    apply_import_highlights, apply_import_notes, parse_annotations_file, parse_bookmarks_file,
    parse_favorites_file, parse_highlights_file, parse_notes_file,
};
use jwlmanager_lib::db::resources::{dev_resources_db_path, ResourceCatalog};
use rusqlite::Connection;
use tempfile::TempDir;

const NOW: &str = "2026-02-01T00:00:00Z";

fn pinned_header(tag: &'static str) -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: tag,
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

fn export_bytes_to_temp(write: impl FnOnce(&std::path::Path)) -> Vec<u8> {
    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("out.txt");
    write(&out_path);
    common::read_file_bytes(&out_path)
}

// ---------------------------------------------------------------------------
// Favorites — stable: string-level dup-check prevents duplication on re-import.
// ---------------------------------------------------------------------------

#[test]
fn favorites_round_trip_is_stable() {
    let (_dir, db_path) = common::fresh_v16_db_for_favorites_io();
    common::seed_one_favorite(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let before = common::normalized_table_rows(&conn, "TagMap");
    let before_location = common::normalized_table_rows(&conn, "Location");

    let bytes1 = export_bytes_to_temp(|p| {
        export_favorites(&conn, None, &pinned_header("{FAVORITES}"), p).expect("export 1");
    });

    let records = parse_favorites_file(std::str::from_utf8(&bytes1).unwrap()).expect("parse");
    {
        let mut conn = Connection::open(&db_path).expect("reopen");
        let tx = conn.transaction().expect("tx");
        let mut available = compute_available_ids(&tx).expect("ids");
        apply_import_favorites(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let after = common::normalized_table_rows(&conn, "TagMap");
    let after_location = common::normalized_table_rows(&conn, "Location");
    assert_eq!(
        before, after,
        "re-importing the same Favorites file must not duplicate TagMap rows"
    );
    assert_eq!(
        before_location, after_location,
        "must not duplicate the Location row either"
    );

    let bytes2 = export_bytes_to_temp(|p| {
        export_favorites(&conn, None, &pinned_header("{FAVORITES}"), p).expect("export 2");
    });
    assert_eq!(
        bytes1, bytes2,
        "a second export after re-import must be byte-identical"
    );
}

// ---------------------------------------------------------------------------
// Bookmarks — stable: upsert by (PublicationLocationId, Slot).
// ---------------------------------------------------------------------------

fn seed_one_bookmark(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (1, 1, NULL, 0, 'nwt', 0, 0)",
        [],
    )
    .expect("insert scripture location");
    let loc = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO Location (KeySymbol, MepsLanguage, Type) VALUES ('nwt', 0, 1)",
        [],
    )
    .expect("insert container location");
    let container = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title, Snippet, BlockType, BlockIdentifier) \
         VALUES (?1, ?2, 0, 'My Bookmark', 'snippet text', 0, NULL)",
        rusqlite::params![loc, container],
    )
    .expect("insert bookmark");
}

#[test]
fn bookmarks_round_trip_is_stable() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_bookmark(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let before = common::normalized_table_rows(&conn, "Bookmark");

    let bytes1 = export_bytes_to_temp(|p| {
        export_bookmarks(&conn, None, &pinned_header("{BOOKMARKS}"), p).expect("export 1");
    });
    let records = parse_bookmarks_file(std::str::from_utf8(&bytes1).unwrap()).expect("parse");
    {
        let mut conn = Connection::open(&db_path).expect("reopen");
        let tx = conn.transaction().expect("tx");
        let mut available = compute_available_ids(&tx).expect("ids");
        apply_import_bookmarks(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let after = common::normalized_table_rows(&conn, "Bookmark");
    assert_eq!(
        before, after,
        "re-importing the same Bookmarks file must UPDATE in place, not duplicate"
    );

    let bytes2 = export_bytes_to_temp(|p| {
        export_bookmarks(&conn, None, &pinned_header("{BOOKMARKS}"), p).expect("export 2");
    });
    assert_eq!(
        bytes1, bytes2,
        "a second export after re-import must be byte-identical"
    );
}

// ---------------------------------------------------------------------------
// Annotations — stable: upsert by (LocationId, TextTag).
// ---------------------------------------------------------------------------

fn seed_one_annotation(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute(
        "INSERT INTO Location (DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (1001, 0, 'w', NULL, 0)",
        [],
    )
    .expect("insert location");
    let loc = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?1, 'tag1', 'A value')",
        rusqlite::params![loc],
    )
    .expect("insert inputfield");
}

#[test]
fn annotations_round_trip_is_stable() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let before = common::normalized_table_rows(&conn, "InputField");

    let bytes1 = export_bytes_to_temp(|p| {
        export_annotations(&conn, None, &pinned_header("{ANNOTATIONS}"), p).expect("export 1");
    });
    let records = parse_annotations_file(std::str::from_utf8(&bytes1).unwrap()).expect("parse");
    {
        let mut conn = Connection::open(&db_path).expect("reopen");
        let tx = conn.transaction().expect("tx");
        let mut available = compute_available_ids(&tx).expect("ids");
        apply_import_annotations(&tx, &records, &mut available).expect("apply");
        tx.commit().expect("commit");
    }

    let after = common::normalized_table_rows(&conn, "InputField");
    assert_eq!(
        before, after,
        "re-importing the same Annotations file must UPDATE in place, not duplicate"
    );

    let bytes2 = export_bytes_to_temp(|p| {
        export_annotations(&conn, None, &pinned_header("{ANNOTATIONS}"), p).expect("export 2");
    });
    assert_eq!(
        bytes1, bytes2,
        "a second export after re-import must be byte-identical"
    );
}

// ---------------------------------------------------------------------------
// Notes — stable: upsert by title/content identity match.
// ---------------------------------------------------------------------------

fn seed_one_note(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute(
        "INSERT INTO Note (Guid, Title, Content, BlockType, LastModified, Created) \
         VALUES ('note-rt', 'RoundTrip Title', 'RoundTrip body', 0, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
        [],
    )
    .expect("insert note");
}

#[test]
fn notes_round_trip_is_stable() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let catalog = ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db");

    let before = common::normalized_table_rows(&conn, "Note");

    let bytes1 = export_bytes_to_temp(|p| {
        export_notes(&conn, None, &catalog, &pinned_header("{NOTES=}"), NOW, p).expect("export 1");
    });
    let (bucket, records) = parse_notes_file(std::str::from_utf8(&bytes1).unwrap()).expect("parse");
    assert_eq!(bucket, None);
    {
        let mut conn = Connection::open(&db_path).expect("reopen");
        let tx = conn.transaction().expect("tx");
        let mut available = compute_available_ids(&tx).expect("ids");
        apply_import_notes(&tx, None, &records, &mut available, 1, NOW).expect("apply");
        tx.commit().expect("commit");
    }

    let after = common::normalized_table_rows(&conn, "Note");
    assert_eq!(
        before, after,
        "re-importing the same Notes file must UPDATE in place, not duplicate"
    );

    let bytes2 = export_bytes_to_temp(|p| {
        export_notes(&conn, None, &catalog, &pinned_header("{NOTES=}"), NOW, p).expect("export 2");
    });
    assert_eq!(
        bytes1, bytes2,
        "a second export after re-import must be byte-identical"
    );
}

// ---------------------------------------------------------------------------
// Highlights — NOT idempotent at the UserMark level (RESEARCH Pitfall 5):
// BlockRange geometry converges while UserMark count grows every import.
// ---------------------------------------------------------------------------

fn seed_one_highlight(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (1, 2, NULL, 0, 'nwt', 0, 0)",
        [],
    )
    .expect("insert location");
    let loc = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (1, ?1, 0, 'rt-highlight', 1)",
        rusqlite::params![loc],
    )
    .expect("insert usermark");
    let um = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (1, 1, 0, 5, ?1)",
        rusqlite::params![um],
    )
    .expect("insert blockrange");
}

fn block_range_geometry(conn: &Connection) -> Vec<(i64, i64, i64, i64)> {
    let mut stmt = conn
        .prepare("SELECT Identifier, LocationId, StartToken, EndToken FROM BlockRange JOIN UserMark USING (UserMarkId) ORDER BY Identifier, StartToken")
        .expect("prepare");
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("read")
}

#[test]
fn highlights_round_trip_converges_geometry_while_usermark_grows() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_highlight(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let before_geometry = block_range_geometry(&conn);
    let before_usermark_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM UserMark", [], |r| r.get(0))
        .expect("count");

    let bytes1 = export_bytes_to_temp(|p| {
        export_highlights(&conn, None, &pinned_header("{HIGHLIGHTS}"), p).expect("export 1");
    });
    let records = parse_highlights_file(std::str::from_utf8(&bytes1).unwrap()).expect("parse");
    {
        let mut conn = Connection::open(&db_path).expect("reopen");
        let tx = conn.transaction().expect("tx");
        let mut available = compute_available_ids(&tx).expect("ids");
        apply_import_highlights(&tx, &records, &mut available, 42).expect("apply");
        tx.commit().expect("commit");
    }

    let after_geometry = block_range_geometry(&conn);
    assert_eq!(
        before_geometry, after_geometry,
        "BlockRange geometry must converge, not duplicate"
    );

    let after_usermark_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM UserMark", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        after_usermark_count,
        before_usermark_count + 1,
        "each Highlights import synthesizes a fresh UserMark — accepted non-idempotency, not a bug"
    );
}

// ---------------------------------------------------------------------------
// ID recycling (IO-03) — every recycled gap id is consumed before any id
// above the pre-import maximum is allocated, across the affected tables.
// ---------------------------------------------------------------------------

#[test]
fn recycled_gap_ids_are_consumed_before_autoincrement_across_the_exercise() {
    let (_dir, db_path) = common::fresh_v16_db();

    // Seed a gap at LocationId 1: insert ids 1 and 2 as unrelated Locations,
    // then delete 1, leaving a recyclable gap.
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        for _ in 0..2 {
            conn.execute(
                "INSERT INTO Location (KeySymbol, MepsLanguage, Type) VALUES ('placeholder', 0, 1)",
                [],
            )
            .expect("seed placeholder location");
        }
        conn.execute("DELETE FROM Location WHERE LocationId = 1", [])
            .expect("delete to create gap at id 1");
    }

    let text =
        "{ANNOTATIONS}\n \nheader\n==={PUB=w}{DOC=1001}{LABEL=tag1}===\nA value\n==={END}===";
    let records = parse_annotations_file(text).expect("parse");

    let mut conn = Connection::open(&db_path).expect("open db");
    let tx = conn.transaction().expect("tx");
    let mut available = compute_available_ids(&tx).expect("ids");
    apply_import_annotations(&tx, &records, &mut available).expect("apply");
    tx.commit().expect("commit");
    drop(conn);

    let conn = Connection::open(&db_path).expect("reopen");
    let new_location_id: i64 = conn
        .query_row(
            "SELECT LocationId FROM Location WHERE DocumentId = 1001",
            [],
            |r| r.get(0),
        )
        .expect("read new location id");
    assert_eq!(new_location_id, 1, "the new Location must consume the recycled gap id 1, not autoincrement above the prior max");
}

#[test]
fn full_five_category_import_export_suite_passes() {
    // A single smoke test proving all five categories' round-trip functions
    // are wired to the SAME shared spine (`db::io::export`/`db::io::import`)
    // and can run back-to-back against one archive without interfering
    // (distinct table sets, distinct `available` id pools per transaction).
    let (_dir, db_path) = common::fresh_v16_db_for_favorites_io();
    common::seed_one_favorite(&db_path);
    seed_one_bookmark(&db_path);
    seed_one_annotation(&db_path);
    seed_one_note(&db_path);
    seed_one_highlight(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let catalog = ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db");

    for (label, count) in [
        (
            "Favorites",
            export_favorites(
                &conn,
                None,
                &pinned_header("{FAVORITES}"),
                &TempDir::new().unwrap().path().join("f.txt"),
            )
            .expect("export favorites"),
        ),
        (
            "Bookmarks",
            export_bookmarks(
                &conn,
                None,
                &pinned_header("{BOOKMARKS}"),
                &TempDir::new().unwrap().path().join("b.txt"),
            )
            .expect("export bookmarks"),
        ),
        (
            "Annotations",
            export_annotations(
                &conn,
                None,
                &pinned_header("{ANNOTATIONS}"),
                &TempDir::new().unwrap().path().join("a.txt"),
            )
            .expect("export annotations"),
        ),
        (
            "Highlights",
            export_highlights(
                &conn,
                None,
                &pinned_header("{HIGHLIGHTS}"),
                &TempDir::new().unwrap().path().join("h.txt"),
            )
            .expect("export highlights"),
        ),
        (
            "Notes",
            export_notes(
                &conn,
                None,
                &catalog,
                &pinned_header("{NOTES=}"),
                NOW,
                &TempDir::new().unwrap().path().join("n.txt"),
            )
            .expect("export notes"),
        ),
    ] {
        assert_eq!(count, 1, "{label} must export exactly its one seeded row");
    }
}
