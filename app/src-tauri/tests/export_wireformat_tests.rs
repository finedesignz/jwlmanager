//! Favorites export byte-exactness tests (08-01-PLAN.md Task 1, IO-01).
//!
//! Byte-compares the exported file against a hand-authored golden fixture
//! (`tests/fixtures/wire/favorites_golden.txt`) — never a normalized/parsed
//! comparison. The golden fixture is hand-authored to the documented wire
//! format, never produced by running this app's own exporter (would prove
//! only self-consistency, not Python compatibility).

mod common;

use jwlmanager_lib::db::favorites::NonEmptyTagMapIds;
use jwlmanager_lib::db::io::export::export_favorites;
use jwlmanager_lib::db::io::header::ExportHeaderCtx;
use rusqlite::Connection;
use tempfile::TempDir;

fn pinned_header() -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: "{FAVORITES}",
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

/// Seeds the exact two-row fixture `favorites_golden.txt` was hand-authored
/// against: row 1 (`Position=0`, a Bible edition — `DocumentId`/`Track` both
/// NULL, `Type=1`'s CHECK branch) and row 2 (`Position=1`, a
/// publication/track — `DocumentId` NULL, `Track` present, `Type=0`'s Track
/// CHECK branch), each satisfying `Location`'s `Type`-scoped CHECK
/// constraint while still exercising the `'None'` NULL sentinel.
fn seed_golden_fixture_rows(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute("INSERT INTO Tag (Type, Name) VALUES (0, 'Favorite')", [])
        .expect("insert system tag");
    let tag_id = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Location (DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (NULL, NULL, 0, 'nwt', 0, 1)",
        [],
    )
    .expect("insert location 1");
    let loc1 = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO TagMap (PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (NULL, ?1, NULL, ?2, 0)",
        rusqlite::params![loc1, tag_id],
    )
    .expect("insert tagmap 1");

    conn.execute(
        "INSERT INTO Location (DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (NULL, 5, 0, 'pub-x', 0, 0)",
        [],
    )
    .expect("insert location 2");
    let loc2 = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO TagMap (PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (NULL, ?1, NULL, ?2, 1)",
        rusqlite::params![loc2, tag_id],
    )
    .expect("insert tagmap 2");
}

#[test]
fn exported_bytes_match_golden_fixture_exactly() {
    let (_dir, db_path) = common::fresh_v16_db_for_favorites_io();
    seed_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("favorites_out.txt");
    let count = export_favorites(&conn, None, &pinned_header(), &out_path).expect("export");
    assert_eq!(count, 2);

    let actual = common::read_file_bytes(&out_path);
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/favorites_golden.txt");
    let golden = common::read_file_bytes(&golden_path);
    assert_eq!(actual, golden, "exported bytes must byte-match the golden fixture exactly");
}

#[test]
fn exported_file_never_contains_end_sentinel() {
    let (_dir, db_path) = common::fresh_v16_db_for_favorites_io();
    seed_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("favorites_out.txt");
    export_favorites(&conn, None, &pinned_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(
        !text.contains("==={END}==="),
        "Favorites export must never write an {{END}} sentinel"
    );
}

#[test]
fn null_column_renders_as_literal_none() {
    let (_dir, db_path) = common::fresh_v16_db_for_favorites_io();
    seed_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("favorites_out.txt");
    export_favorites(&conn, None, &pinned_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("|None|"), "a NULL column must render as the literal string None");
}

#[test]
fn selection_scoped_export_contains_only_the_selected_rows() {
    let (_dir, db_path) = common::fresh_v16_db_for_favorites_io();
    seed_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    // The second TagMap row (Position=1) — resolve its id directly.
    let tagmap_id: i64 = conn
        .query_row("SELECT TagMapId FROM TagMap WHERE Position = 1", [], |r| r.get(0))
        .expect("read tagmap id");
    let ids = NonEmptyTagMapIds::try_from(vec![tagmap_id]).expect("non-empty selection");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("favorites_selected.txt");
    let count = export_favorites(&conn, Some(&ids), &pinned_header(), &out_path).expect("export");
    assert_eq!(count, 1);

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("None|5|0|pub-x|0|0"));
    assert!(!text.contains("None|None|0|nwt|0|1"));
}
