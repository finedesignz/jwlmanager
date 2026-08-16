//! Favorites export byte-exactness tests (08-01-PLAN.md Task 1, IO-01).
//!
//! Byte-compares the exported file against a hand-authored golden fixture
//! (`tests/fixtures/wire/favorites_golden.txt`) — never a normalized/parsed
//! comparison. The golden fixture is hand-authored to the documented wire
//! format, never produced by running this app's own exporter (would prove
//! only self-consistency, not Python compatibility).

mod common;

use jwlmanager_lib::db::color::NonEmptyBlockRangeIds;
use jwlmanager_lib::db::delete::{NonEmptyBookmarkIds, NonEmptyLocationIds, NonEmptyNoteIds};
use jwlmanager_lib::db::favorites::NonEmptyTagMapIds;
use jwlmanager_lib::db::io::export::{
    export_annotations, export_bookmarks, export_favorites, export_highlights, export_notes,
};
use jwlmanager_lib::db::io::header::ExportHeaderCtx;
use jwlmanager_lib::db::resources::{dev_resources_db_path, ResourceCatalog};
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
    assert_eq!(
        actual, golden,
        "exported bytes must byte-match the golden fixture exactly"
    );
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
    assert!(
        text.contains("|None|"),
        "a NULL column must render as the literal string None"
    );
}

#[test]
fn selection_scoped_export_contains_only_the_selected_rows() {
    let (_dir, db_path) = common::fresh_v16_db_for_favorites_io();
    seed_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    // The second TagMap row (Position=1) — resolve its id directly.
    let tagmap_id: i64 = conn
        .query_row("SELECT TagMapId FROM TagMap WHERE Position = 1", [], |r| {
            r.get(0)
        })
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

// ---------------------------------------------------------------------------
// Bookmarks (08-02-PLAN.md Task 1, IO-01): 12 flat pipe fields, `¦` escaping
// on Title/Snippet ONLY, no `{END}` sentinel.
// ---------------------------------------------------------------------------

fn pinned_bookmarks_header() -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: "{BOOKMARKS}",
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

/// Seeds the exact two-bookmark fixture `bookmarks_golden.txt` was
/// hand-authored against: a scripture bookmark (Location Type=0,
/// Book/Chapter present) whose Title contains a literal `|`, and a
/// publication bookmark (Location Type=0, DocumentId present) whose Snippet
/// contains a literal `|` — both must export with `¦` in place of `|`.
fn seed_bookmarks_golden_fixture_rows(db_path: &std::path::Path) -> (i64, i64) {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");

    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (1, 1, NULL, NULL, 0, 'nwt', 0, 0)",
        [],
    )
    .expect("insert scripture Location");
    let scripture_loc = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (NULL, NULL, 1001, NULL, 0, 'pub-x', 0, 0)",
        [],
    )
    .expect("insert publication Location");
    let pub_loc = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Location (KeySymbol, MepsLanguage, Type) VALUES ('nwt', 0, 1)",
        [],
    )
    .expect("insert scripture container Location");
    let scripture_container = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Location (KeySymbol, MepsLanguage, Type) VALUES ('pub-x', 0, 1)",
        [],
    )
    .expect("insert publication container Location");
    let pub_container = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title, Snippet, BlockType, BlockIdentifier) \
         VALUES (?1, ?2, 0, 'Genesis | Note', NULL, 1, 5)",
        rusqlite::params![scripture_loc, scripture_container],
    )
    .expect("insert scripture Bookmark");
    let bookmark1 = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title, Snippet, BlockType, BlockIdentifier) \
         VALUES (?1, ?2, 1, 'Pub Bookmark', 'Has a | pipe too', 0, NULL)",
        rusqlite::params![pub_loc, pub_container],
    )
    .expect("insert publication Bookmark");
    let bookmark2 = conn.last_insert_rowid();

    (bookmark1, bookmark2)
}

#[test]
fn exported_bookmarks_match_golden_fixture_exactly() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_bookmarks_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("bookmarks_out.txt");
    let count =
        export_bookmarks(&conn, None, &pinned_bookmarks_header(), &out_path).expect("export");
    assert_eq!(count, 2);

    let actual = common::read_file_bytes(&out_path);
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/bookmarks_golden.txt");
    let golden = common::read_file_bytes(&golden_path);
    assert_eq!(
        actual, golden,
        "exported Bookmarks bytes must byte-match the golden fixture"
    );
}

#[test]
fn exported_bookmarks_never_contain_end_sentinel() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_bookmarks_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("bookmarks_out.txt");
    export_bookmarks(&conn, None, &pinned_bookmarks_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(
        !text.contains("==={END}==="),
        "Bookmarks export must never write an {{END}} sentinel"
    );
}

#[test]
fn bookmark_title_pipe_exports_as_broken_bar() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_bookmarks_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("bookmarks_out.txt");
    export_bookmarks(&conn, None, &pinned_bookmarks_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(
        text.contains("Genesis \u{A6} Note"),
        "a literal | in Title must export as \u{A6}"
    );
    assert!(
        !text.contains("Genesis | Note"),
        "the literal | must not survive export"
    );
}

#[test]
fn bookmarks_selection_scoped_export_contains_only_the_selected_row() {
    let (_dir, db_path) = common::fresh_v16_db();
    let (bookmark1, _bookmark2) = seed_bookmarks_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let ids = NonEmptyBookmarkIds::try_from(vec![bookmark1]).expect("non-empty selection");
    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("bookmarks_selected.txt");
    let count =
        export_bookmarks(&conn, Some(&ids), &pinned_bookmarks_header(), &out_path).expect("export");
    assert_eq!(count, 1);

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("Genesis \u{A6} Note"));
    assert!(!text.contains("Pub Bookmark"));
}

// ---------------------------------------------------------------------------
// Annotations (08-02-PLAN.md Task 2, IO-01): bracket-tag records, `{END}`
// sentinel, conditional `{ISSUE}` bracket.
// ---------------------------------------------------------------------------

fn pinned_annotations_header() -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: "{ANNOTATIONS}",
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

/// Seeds the exact two-record fixture `annotations_golden.txt` was
/// hand-authored against: one WITH an `{ISSUE}` bracket (IssueTagNumber
/// 20240101 > 10000000), one WITHOUT (1234 <= 10000000), and a multi-line
/// Value on the second, padded on the first to prove `.trim()` at export.
fn seed_annotations_golden_fixture_rows(db_path: &std::path::Path) -> (i64, i64) {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");

    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (NULL, NULL, NULL, 1, 20240101, 'w', 0, 0)",
        [],
    )
    .expect("insert issue Location");
    let issue_loc = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (NULL, NULL, NULL, 1, 1234, 'w', 0, 0)",
        [],
    )
    .expect("insert non-issue Location");
    let plain_loc = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?1, 'heading001', ' Some heading value ')",
        rusqlite::params![issue_loc],
    )
    .expect("insert heading InputField");

    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?1, 'note002', 'Line one\nLine two')",
        rusqlite::params![plain_loc],
    )
    .expect("insert note InputField");

    (issue_loc, plain_loc)
}

#[test]
fn exported_annotations_match_golden_fixture_exactly() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_annotations_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("annotations_out.txt");
    let count =
        export_annotations(&conn, None, &pinned_annotations_header(), &out_path).expect("export");
    assert_eq!(count, 2);

    let actual = common::read_file_bytes(&out_path);
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/annotations_golden.txt");
    let golden = common::read_file_bytes(&golden_path);
    assert_eq!(
        actual, golden,
        "exported Annotations bytes must byte-match the golden fixture"
    );
}

#[test]
fn exported_annotations_end_with_end_sentinel_and_no_trailing_newline() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_annotations_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("annotations_out.txt");
    export_annotations(&conn, None, &pinned_annotations_header(), &out_path).expect("export");

    let bytes = common::read_file_bytes(&out_path);
    let text = String::from_utf8(bytes).expect("utf8");
    assert!(
        text.ends_with("\n==={END}==="),
        "Annotations export must end with the {{END}} sentinel"
    );
    assert!(
        !text.ends_with('\n'),
        "no trailing newline after the {{END}} sentinel"
    );
}

#[test]
fn issue_bracket_present_only_above_ten_million() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_annotations_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("annotations_out.txt");
    export_annotations(&conn, None, &pinned_annotations_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("{ISSUE=20240101}"));
    // The non-issue record's header must carry no {ISSUE at all.
    let note_header_start = text.find("{LABEL=note002}").expect("find note002 header");
    let preceding = &text[..note_header_start];
    let last_boundary = preceding.rfind("\n===").expect("find record boundary");
    assert!(
        !preceding[last_boundary..].contains("{ISSUE"),
        "the 1234 IssueTagNumber must not produce an {{ISSUE}} bracket"
    );
}

#[test]
fn annotations_selection_scoped_export_contains_only_the_selected_location() {
    let (_dir, db_path) = common::fresh_v16_db();
    let (issue_loc, _plain_loc) = seed_annotations_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let ids = NonEmptyLocationIds::try_from(vec![issue_loc]).expect("non-empty selection");
    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("annotations_selected.txt");
    let count = export_annotations(&conn, Some(&ids), &pinned_annotations_header(), &out_path)
        .expect("export");
    assert_eq!(count, 1);

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("heading001"));
    assert!(!text.contains("note002"));
}

// ---------------------------------------------------------------------------
// Highlights (08-03-PLAN.md Task 1, IO-01): 13 flat pipe fields, no `¦`
// escaping, no `{END}` sentinel — the range-merge category.
// ---------------------------------------------------------------------------

fn pinned_highlights_header() -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: "{HIGHLIGHTS}",
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

/// Seeds the exact two-highlight fixture `highlights_golden.txt` was
/// hand-authored against: a scripture highlight (Location Type=0,
/// Book/Chapter present, `DocumentId NULL` -> `None`) and a publication
/// highlight (Location Type=0, `DocumentId` present, Book/Chapter both
/// `NULL` -> `None`).
fn seed_highlights_golden_fixture_rows(db_path: &std::path::Path) -> (i64, i64) {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");

    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (1, 1, NULL, NULL, 0, 'nwt', 0, 0)",
        [],
    )
    .expect("insert scripture Location");
    let scripture_loc = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (1, ?1, 0, 'fixture-highlight-scripture', 1)",
        rusqlite::params![scripture_loc],
    )
    .expect("insert scripture UserMark");
    let scripture_um = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (1, 1, 0, 5, ?1)",
        rusqlite::params![scripture_um],
    )
    .expect("insert scripture BlockRange");
    let range1 = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (NULL, NULL, 1001, NULL, 0, 'pub-x', 0, 0)",
        [],
    )
    .expect("insert publication Location");
    let pub_loc = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (3, ?1, 0, 'fixture-highlight-publication', 1)",
        rusqlite::params![pub_loc],
    )
    .expect("insert publication UserMark");
    let pub_um = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (2, 2, 10, 20, ?1)",
        rusqlite::params![pub_um],
    )
    .expect("insert publication BlockRange");
    let range2 = conn.last_insert_rowid();

    (range1, range2)
}

#[test]
fn exported_highlights_match_golden_fixture_exactly() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_highlights_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("highlights_out.txt");
    let count =
        export_highlights(&conn, None, &pinned_highlights_header(), &out_path).expect("export");
    assert_eq!(count, 2);

    let actual = common::read_file_bytes(&out_path);
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/highlights_golden.txt");
    let golden = common::read_file_bytes(&golden_path);
    assert_eq!(
        actual, golden,
        "exported Highlights bytes must byte-match the golden fixture"
    );
}

#[test]
fn exported_highlights_never_contain_end_sentinel() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_highlights_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("highlights_out.txt");
    export_highlights(&conn, None, &pinned_highlights_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(
        !text.contains("==={END}==="),
        "Highlights export must never write an {{END}} sentinel"
    );
}

#[test]
fn highlights_data_lines_have_exactly_thirteen_fields() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_highlights_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("highlights_out.txt");
    export_highlights(&conn, None, &pinned_highlights_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    for line in text.lines().skip(5) {
        // Skips the 5-line header (tag, blank, "Exported from", "by...on...",
        // stars) — every remaining line is a data row.
        assert_eq!(
            line.split('|').count(),
            13,
            "every Highlights data line must have exactly 13 fields: {line:?}"
        );
    }
}

#[test]
fn highlights_null_document_id_renders_as_literal_none() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_highlights_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("highlights_out.txt");
    export_highlights(&conn, None, &pinned_highlights_header(), &out_path).expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(
        text.contains("|None|0|nwt|"),
        "a NULL DocumentId must render as the literal string None"
    );
}

#[test]
fn highlights_selection_scoped_export_contains_only_the_selected_row() {
    let (_dir, db_path) = common::fresh_v16_db();
    let (range1, _range2) = seed_highlights_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let ids = NonEmptyBlockRangeIds::try_from(vec![range1]).expect("non-empty selection");
    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("highlights_selected.txt");
    let count = export_highlights(&conn, Some(&ids), &pinned_highlights_header(), &out_path)
        .expect("export");
    assert_eq!(count, 1);

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("nwt"));
    assert!(!text.contains("pub-x"));
}

// ---------------------------------------------------------------------------
// Notes (08-04-PLAN.md Task 1): bracket-tag records, the widest optional-tag
// vocabulary, `{END}` sentinel. Ports `export_notes`'s txt branch
// (`JWLManager.py:1636-1668`).
// ---------------------------------------------------------------------------

fn pinned_notes_header() -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: "{NOTES=}",
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

/// Seeds the exact three-note fixture `notes_golden.txt` was hand-authored
/// against, in export order (`ORDER BY BlockType, LastModified DESC`):
/// an untitled multi-line independent note (BlockType 0), a tagless
/// publication note carrying a `RANGE` (BlockType 1), and a tagged,
/// multi-line Bible verse note with a PRESET Location Title (BlockType 2,
/// exercising the `HEADING` `":VS"` append path rather than the auto-fill).
/// Returns the Note ids in export order.
fn seed_notes_golden_fixture_rows(db_path: &std::path::Path) -> (i64, i64, i64) {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");

    // Independent (untitled, multi-line, no tags).
    conn.execute(
        "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content, BlockType, BlockIdentifier, LastModified, Created) \
         VALUES ('note-indep', NULL, NULL, '', 'indep line1\nindep line2', 0, NULL, '2024-03-01T00:00:00', '2024-03-01T00:00:00')",
        [],
    )
    .expect("insert independent note");
    let indep_id = conn.last_insert_rowid();

    // Publication (BLOCK + COLOR + RANGE, empty heading, no tags).
    conn.execute(
        "INSERT INTO Location (DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Title, Type) \
         VALUES (1001, 0, 'pub-x', 0, '', 0)",
        [],
    )
    .expect("insert publication location");
    let pub_loc = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (1, ?1, 0, 'fixture-note-publication', 1)",
        rusqlite::params![pub_loc],
    )
    .expect("insert publication usermark");
    let pub_um = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (1, 3, 10, 20, ?1)",
        rusqlite::params![pub_um],
    )
    .expect("insert publication blockrange");
    conn.execute(
        "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content, BlockType, BlockIdentifier, LastModified, Created) \
         VALUES ('note-pub', ?1, ?2, 'Pub Note', 'Some content', 1, 3, '2024-02-01T00:00:00', '2024-02-01T00:00:00')",
        rusqlite::params![pub_um, pub_loc],
    )
    .expect("insert publication note");
    let pub_id = conn.last_insert_rowid();

    // Bible verse note (VS present, preset Location Title, tagged, multi-line).
    conn.execute(
        "INSERT INTO Location (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Title, Type) \
         VALUES (1, 1, 'nwt', 0, 'Genesis 1', 0)",
        [],
    )
    .expect("insert bible location");
    let bible_loc = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content, BlockType, BlockIdentifier, LastModified, Created) \
         VALUES ('note-bible', NULL, ?1, 'My Title', 'Line one\nLine two', 2, 5, '2024-01-02T03:04:05', '2024-01-01T00:00:00')",
        rusqlite::params![bible_loc],
    )
    .expect("insert bible note");
    let bible_id = conn.last_insert_rowid();

    for (name, position) in [("alpha", 0), ("beta", 1)] {
        conn.execute(
            "INSERT INTO Tag (Type, Name) VALUES (1, ?1)",
            rusqlite::params![name],
        )
        .expect("insert tag");
        let tag_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO TagMap (PlaylistItemId, LocationId, NoteId, TagId, Position) \
             VALUES (NULL, NULL, ?1, ?2, ?3)",
            rusqlite::params![bible_id, tag_id, position],
        )
        .expect("insert tagmap");
    }

    (indep_id, pub_id, bible_id)
}

#[test]
fn exported_notes_match_golden_fixture_exactly() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_notes_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("notes_out.txt");
    let count = export_notes(
        &conn,
        None,
        &catalog,
        &pinned_notes_header(),
        "2099-01-01T00:00:00Z",
        &out_path,
    )
    .expect("export");
    assert_eq!(count, 3);

    let actual = common::read_file_bytes(&out_path);
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/notes_golden.txt");
    let golden = common::read_file_bytes(&golden_path);
    assert_eq!(
        actual, golden,
        "exported Notes bytes must byte-match the golden fixture exactly"
    );
}

#[test]
fn exported_notes_end_with_end_sentinel_and_no_trailing_newline() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_notes_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("notes_out.txt");
    export_notes(
        &conn,
        None,
        &catalog,
        &pinned_notes_header(),
        "2099-01-01T00:00:00Z",
        &out_path,
    )
    .expect("export");

    let bytes = common::read_file_bytes(&out_path);
    assert!(bytes.ends_with(b"\n==={END}==="));
}

#[test]
fn notes_empty_heading_bracket_omitted() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_notes_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("notes_out.txt");
    export_notes(
        &conn,
        None,
        &catalog,
        &pinned_notes_header(),
        "2099-01-01T00:00:00Z",
        &out_path,
    )
    .expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    // The publication note's Location.Title is '' — its record must carry
    // no {HEADING substring at all.
    let pub_record = text
        .split("Pub Note")
        .next()
        .expect("publication record present");
    assert!(!pub_record.contains("{HEADING"));
}

#[test]
fn notes_tags_have_no_spaces_around_pipe() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_notes_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("notes_out.txt");
    export_notes(
        &conn,
        None,
        &catalog,
        &pinned_notes_header(),
        "2099-01-01T00:00:00Z",
        &out_path,
    )
    .expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("{TAGS=alpha|beta}"));
    assert!(!text.contains(" | "));
}

#[test]
fn notes_untitled_body_begins_with_blank_line() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_notes_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");

    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("notes_out.txt");
    export_notes(
        &conn,
        None,
        &catalog,
        &pinned_notes_header(),
        "2099-01-01T00:00:00Z",
        &out_path,
    )
    .expect("export");

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(
        text.contains("{TAGS=}===\n\nindep line1"),
        "an untitled note's body must begin with an empty first line: {text:?}"
    );
}

#[test]
fn notes_selection_scoped_export_contains_only_the_selected_note() {
    let (_dir, db_path) = common::fresh_v16_db();
    let (_indep_id, pub_id, _bible_id) = seed_notes_golden_fixture_rows(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");

    let ids = NonEmptyNoteIds::try_from(vec![pub_id]).expect("non-empty selection");
    let out_dir = TempDir::new().expect("tempdir");
    let out_path = out_dir.path().join("notes_selected.txt");
    let count = export_notes(
        &conn,
        Some(&ids),
        &catalog,
        &pinned_notes_header(),
        "2099-01-01T00:00:00Z",
        &out_path,
    )
    .expect("export");
    assert_eq!(count, 1);

    let text = std::fs::read_to_string(&out_path).expect("read exported file");
    assert!(text.contains("Pub Note"));
    assert!(!text.contains("My Title"));
    assert!(!text.contains("indep line1"));
}
