//! Notes incremental export behaviour tests (IO-04, 09-01-PLAN.md Task 2/3).
//!
//! Drives [`export_notes_incremental`] directly (this codebase's established
//! `*_impl`-is-directly-testable shape — the Tauri command in `lib.rs` is a
//! thin session/path wrapper over this pure function) against a synthetic
//! `res/blank`-seeded fixture, mutating the archive between a baseline
//! ("prior") export and the incremental export under test.

mod common;

use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::io::diff::{
    export_annotations_incremental, export_bookmarks_incremental, export_favorites_incremental,
    export_highlights_incremental, export_notes_incremental,
};
use jwlmanager_lib::db::io::export::{
    export_annotations, export_bookmarks, export_favorites, export_highlights, export_notes,
};
use jwlmanager_lib::db::io::header::ExportHeaderCtx;
use jwlmanager_lib::db::io::import::{
    apply_import_annotations, apply_import_highlights, apply_import_notes, parse_annotations_file,
    parse_bookmarks_file, parse_favorites_file, parse_highlights_file, parse_notes_file,
};
use jwlmanager_lib::db::resources::{dev_resources_db_path, ResourceCatalog};
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;
use tempfile::TempDir;

fn pinned_header() -> ExportHeaderCtx<'static> {
    pinned_header_for("{NOTES=}")
}

/// Same pinned deterministic values [`pinned_header`] uses, parameterized
/// over the category tag — shared by the Favorites/Bookmarks/Highlights
/// tests below (09-02-PLAN.md Task 2).
fn pinned_header_for(category_tag: &'static str) -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag,
        archive_name: "MyArchive.jwlibrary".to_string(),
        app_version: "0.1.0".to_string(),
        timestamp: "2026-01-01 @ 00:00:00".to_string(),
    }
}

fn catalog() -> ResourceCatalog {
    ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load")
}

/// Seeds one independent (untitled-shape-agnostic) Note, returning its
/// `NoteId`.
fn seed_one_note(db_path: &std::path::Path, title: &str, content: &str) -> i64 {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute_batch("PRAGMA foreign_keys = OFF").expect("fk off");
    conn.execute(
        "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content, BlockType, \
         BlockIdentifier, LastModified, Created) \
         VALUES ('note-fixture-1', NULL, NULL, ?1, ?2, 0, NULL, \
         '2024-01-01T00:00:00', '2024-01-01T00:00:00')",
        rusqlite::params![title, content],
    )
    .expect("insert fixture note");
    conn.last_insert_rowid()
}

fn export_baseline(db_path: &std::path::Path, out_path: &std::path::Path) -> String {
    let conn = Connection::open(db_path).expect("open db");
    export_notes(&conn, None, &catalog(), &pinned_header(), "2099-01-01T00:00:00Z", out_path)
        .expect("baseline export");
    std::fs::read_to_string(out_path).expect("read baseline export")
}

/// Reads the checked-in prior-export fixture — a real `export_notes` run
/// (pinned header, `pinned_header()`/`"2099-01-01T00:00:00Z"` `now`) over a
/// single `seed_one_note(db, "Title", "Content")` note, so tests that want a
/// STATIC prior file (rather than one generated fresh each run) can assert
/// against it directly.
fn read_notes_prior_fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/notes_prior.txt");
    std::fs::read_to_string(path).expect("read notes_prior.txt fixture")
}

#[test]
fn incremental_no_prior_file_exports_all() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path, "Title", "Content");
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let full_path = out_dir.path().join("full.txt");
    let incremental_path = out_dir.path().join("incremental.txt");

    export_notes(&conn, None, &catalog(), &pinned_header(), "2099-01-01T00:00:00Z", &full_path)
        .expect("full export");

    let summary = export_notes_incremental(
        &conn,
        None,
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    )
    .expect("incremental export with no prior file");

    let full_bytes = common::read_file_bytes(&full_path);
    let incremental_bytes = common::read_file_bytes(&incremental_path);
    assert_eq!(
        full_bytes, incremental_bytes,
        "no prior file must export the whole category, byte-identical to a full export (D9-05)"
    );
    assert_eq!(summary.added, 1);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.deleted_candidates, 0);
    assert_eq!(summary.exported, 1);
}

#[test]
fn timestamp_only_change_excluded() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path, "Title", "Content");

    let out_dir = TempDir::new().expect("tempdir");
    let prior_path = out_dir.path().join("prior.txt");
    let prior_text = export_baseline(&db_path, &prior_path);

    // Bump ONLY LastModified — never Content/Title/Tags/Color/Range.
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE Note SET LastModified = '2024-06-01T00:00:00'",
            [],
        )
        .expect("bump LastModified");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_notes_incremental(
        &conn,
        Some(&prior_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0, "IO-04 criterion 2: a timestamp-only change is not an add");
    assert_eq!(summary.modified, 0, "IO-04 criterion 2: a timestamp-only change is not a modify");
    assert_eq!(summary.exported, 0);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    let (_, records) = parse_notes_file(&text).expect("output must itself be a valid Notes file");
    assert!(records.is_empty(), "output file must contain zero records");
}

#[test]
fn content_change_included() {
    let (_dir, db_path) = common::fresh_v16_db();
    // Matches the checked-in `notes_prior.txt` fixture exactly (Title/Content
    // and the same `LastModified`/`Created` timestamps), so this test proves
    // the STATIC fixture against a live archive rather than a freshly
    // self-generated prior.
    seed_one_note(&db_path, "Title", "Content");
    let prior_text = read_notes_prior_fixture();

    let out_dir = TempDir::new().expect("tempdir");
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute("UPDATE Note SET Content = 'Changed content'", [])
            .expect("change content");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_notes_incremental(
        &conn,
        Some(&prior_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 1);
    assert_eq!(summary.exported, 1);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    assert!(text.contains("Changed content"));
}

#[test]
fn added_row_included() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path, "Title", "Content");

    let out_dir = TempDir::new().expect("tempdir");
    let prior_path = out_dir.path().join("prior.txt");
    let prior_text = export_baseline(&db_path, &prior_path);

    seed_new_note(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_notes_incremental(
        &conn,
        Some(&prior_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.exported, 1);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    assert!(text.contains("Second note content"));
}

fn seed_new_note(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute(
        "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content, BlockType, \
         BlockIdentifier, LastModified, Created) \
         VALUES ('note-fixture-2', NULL, NULL, 'Second title', 'Second note content', 0, NULL, \
         '2024-02-01T00:00:00', '2024-02-01T00:00:00')",
        [],
    )
    .expect("insert second note");
}

#[test]
fn deleted_candidate_not_exported() {
    let (_dir, db_path) = common::fresh_v16_db();
    let first_id = seed_one_note(&db_path, "Title", "Content");
    seed_new_note(&db_path);

    let out_dir = TempDir::new().expect("tempdir");
    let prior_path = out_dir.path().join("prior.txt");
    let prior_text = export_baseline(&db_path, &prior_path);

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute("DELETE FROM Note WHERE NoteId = ?1", rusqlite::params![first_id])
            .expect("delete first note");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_notes_incremental(
        &conn,
        Some(&prior_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.exported, 0);
    assert_eq!(summary.deleted_candidates, 1);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    assert!(
        !text.contains("Content") || text.trim_matches(|c: char| c != '=').is_empty(),
        "deleted note's content must not be written to the output file"
    );
    let (_, records) = parse_notes_file(&text).expect("output must itself be a valid Notes file");
    assert!(records.is_empty());
}

#[test]
fn malformed_prior_file_aborts() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path, "Title", "Content");
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");

    let malformed_prior = "this is not a valid Notes export file at all";
    let result = export_notes_incremental(
        &conn,
        Some(malformed_prior),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(
        !incremental_path.exists(),
        "a malformed prior file must abort BEFORE any output file is created"
    );
}

/// Exports incrementally against a baseline prior file, imports that output
/// back into the SAME archive, then exports incrementally AGAIN — this time
/// using the just-produced (and just-reimported) output itself as the prior
/// — and asserts the second run reports zero added and zero modified: the
/// archive and its own most recent export are, by construction, identical.
///
/// NOTE (09-01-PLAN.md Task 3): the Highlights `UserMark`-growth property
/// Phase 8's round-trip suite accepted (a re-imported `UserMark` can grow a
/// `BlockRange` without changing semantic content) does NOT apply to Notes —
/// Notes' `apply_import_notes` writes `Content`/`Title`/tags/range verbatim
/// with no growth-only merge behavior. Plan 09-02 owns re-proving
/// convergence for Highlights against that different shape.
#[test]
fn incremental_export_converges() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path, "Title", "Original content");

    let out_dir = TempDir::new().expect("tempdir");
    let baseline_path = out_dir.path().join("baseline.txt");
    let baseline_prior_text = export_baseline(&db_path, &baseline_path);

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute("UPDATE Note SET Content = 'Changed content'", [])
            .expect("change content");
    }

    // First incremental export: against the ORIGINAL baseline — this is the
    // one real diff in this test (modified=1).
    let first_output_path = out_dir.path().join("first_incremental.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let first_summary = export_notes_incremental(
        &conn,
        Some(&baseline_prior_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &first_output_path,
    )
    .expect("first incremental export");
    assert_eq!(first_summary.modified, 1);
    drop(conn);

    let first_output_text =
        std::fs::read_to_string(&first_output_path).expect("read first incremental output");

    // Re-import that output into the SAME archive.
    {
        let (bucket, records) = parse_notes_file(&first_output_text).expect("parse first output");
        let mut conn = Connection::open(&db_path).expect("reopen db");
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute available ids");
        apply_import_notes(&tx, bucket, &records, &mut available, 1, "2099-01-01T00:00:00Z")
            .expect("apply re-import");
        tx.commit().expect("commit re-import");
    }

    // Second incremental export: against the FIRST output (now also the
    // archive's own current state, after re-import) — must converge.
    let conn = Connection::open(&db_path).expect("open db after re-import");
    let second_output_path = out_dir.path().join("second_incremental.txt");
    let second_summary = export_notes_incremental(
        &conn,
        Some(&first_output_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &second_output_path,
    )
    .expect("second incremental export");

    assert_eq!(second_summary.added, 0, "second run must converge to zero added");
    assert_eq!(second_summary.modified, 0, "second run must converge to zero modified");
}

// ---------------------------------------------------------------------------
// Favorites / Bookmarks / Highlights (09-02-PLAN.md Task 2) — the three flat
// pipe-delimited categories, mechanically applying the Notes design above.
// ---------------------------------------------------------------------------

/// Seeds one Favorite whose wire line is `None|None|0|nwt|0|1` — matches
/// `tests/fixtures/wire/favorites_prior.txt`'s single data row exactly, so
/// tests can assert a checked-in STATIC prior file against a live archive.
/// Relies on `res/blank` pre-seeding `Tag (TagId=1, Type=0, Name='Favorite')`.
fn seed_one_favorite(db_path: &std::path::Path) -> i64 {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute(
        "INSERT INTO Location (LocationId, DocumentId, Track, IssueTagNumber, KeySymbol, \
         MepsLanguage, Type) VALUES (900, NULL, NULL, 0, 'nwt', 0, 1)",
        [],
    )
    .expect("insert favorite Location");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (900, NULL, 900, NULL, 1, 0)",
        [],
    )
    .expect("insert favorite TagMap");
    900
}

/// Seeds a SECOND, distinct Favorite (different Location/wire line) for the
/// "added since prior" case.
fn seed_second_favorite(db_path: &std::path::Path) -> i64 {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute(
        "INSERT INTO Location (LocationId, DocumentId, Track, IssueTagNumber, KeySymbol, \
         MepsLanguage, Type) VALUES (901, 1001, 5, 0, 'pub-x', 0, 0)",
        [],
    )
    .expect("insert second favorite Location");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (901, NULL, 901, NULL, 1, 1)",
        [],
    )
    .expect("insert second favorite TagMap");
    901
}

fn read_favorites_prior_fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/favorites_prior.txt");
    std::fs::read_to_string(path).expect("read favorites_prior.txt fixture")
}

#[test]
fn favorites_no_change_reports_zero_and_writes_valid_empty_output() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_favorite(&db_path);
    let prior_text = read_favorites_prior_fixture();

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_favorites_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.deleted_candidates, 0);
    assert_eq!(summary.exported, 0);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    let records = parse_favorites_file(&text).expect("output must itself be a valid Favorites file");
    assert!(records.is_empty(), "output file must contain zero records");
}

#[test]
fn favorites_added_reports_one_added() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_favorite(&db_path);
    let prior_text = read_favorites_prior_fixture();

    seed_second_favorite(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_favorites_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.exported, 1);
}

#[test]
fn favorites_removed_reports_deleted_candidate_never_modified() {
    let (_dir, db_path) = common::fresh_v16_db();
    let favorite_id = seed_one_favorite(&db_path);
    let prior_text = read_favorites_prior_fixture();

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "DELETE FROM TagMap WHERE TagMapId = ?1",
            rusqlite::params![favorite_id],
        )
        .expect("remove favorite");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_favorites_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 0, "IO-04: Favorites has no mutable wire field, modified is always 0");
    assert_eq!(summary.exported, 0);
    assert_eq!(summary.deleted_candidates, 1);
}

/// Structural property from `<identity_key_specification>`: every field of
/// a Favorite's 6-field wire line is identity, so no possible archive
/// mutation can EVER surface as `modified` — only `added`/`deleted_candidates`.
#[test]
fn favorites_never_reports_modified() {
    let (_dir, db_path) = common::fresh_v16_db();
    let favorite_id = seed_one_favorite(&db_path);
    let prior_text = read_favorites_prior_fixture();

    // The only "mutation" a Favorite's wire fields can undergo is a full
    // Location swap (every field is identity) — simulate by pointing the
    // TagMap at a different Location entirely (same shape as `added`, from
    // the diff engine's point of view, never `modified`).
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO Location (LocationId, DocumentId, Track, IssueTagNumber, KeySymbol, \
             MepsLanguage, Type) VALUES (902, NULL, NULL, 0, 'nwt', 0, 2)",
            [],
        )
        .expect("insert replacement Location");
        conn.execute(
            "UPDATE TagMap SET LocationId = 902 WHERE TagMapId = ?1",
            rusqlite::params![favorite_id],
        )
        .expect("repoint favorite");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_favorites_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.modified, 0, "Favorites must never report modified (no mutable wire field)");
}

#[test]
fn favorites_no_prior_file_exports_all() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_favorite(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let full_path = out_dir.path().join("full.txt");
    let incremental_path = out_dir.path().join("incremental.txt");

    export_favorites(&conn, None, &pinned_header_for("{FAVORITES}"), &full_path).expect("full export");
    let summary = export_favorites_incremental(
        &conn,
        None,
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    )
    .expect("incremental export with no prior file");

    assert_eq!(
        common::read_file_bytes(&full_path),
        common::read_file_bytes(&incremental_path),
        "no prior file must export the whole category, byte-identical to a full export (D9-05)"
    );
    assert_eq!(summary.added, 1);
    assert_eq!(summary.exported, 1);
}

#[test]
fn favorites_malformed_prior_file_aborts() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_favorite(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let result = export_favorites_incremental(
        &conn,
        Some("this is not a valid Favorites export file at all"),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

/// Seeds one Bookmark whose wire line is
/// `1|1|None|0|nwt|0|0|0|Title|Snippet|0|None` — matches
/// `tests/fixtures/wire/bookmarks_prior.txt`'s single data row exactly.
fn seed_one_bookmark(db_path: &std::path::Path) -> i64 {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (910, 1, 1, NULL, NULL, 0, 'nwt', 0, 0)",
        [],
    )
    .expect("insert bookmark Location");
    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) \
         VALUES (910, 910, 910, 0, 'Title', 'Snippet', 0, NULL)",
        [],
    )
    .expect("insert Bookmark");
    910
}

fn read_bookmarks_prior_fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/bookmarks_prior.txt");
    std::fs::read_to_string(path).expect("read bookmarks_prior.txt fixture")
}

#[test]
fn bookmarks_no_change_reports_zero_and_writes_valid_empty_output() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_bookmark(&db_path);
    let prior_text = read_bookmarks_prior_fixture();

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_bookmarks_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{BOOKMARKS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.deleted_candidates, 0);
    assert_eq!(summary.exported, 0);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    let records = parse_bookmarks_file(&text).expect("output must itself be a valid Bookmarks file");
    assert!(records.is_empty());
}

#[test]
fn bookmarks_title_change_reports_one_modified() {
    let (_dir, db_path) = common::fresh_v16_db();
    let bookmark_id = seed_one_bookmark(&db_path);
    let prior_text = read_bookmarks_prior_fixture();

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE Bookmark SET Title = 'Changed Title' WHERE BookmarkId = ?1",
            rusqlite::params![bookmark_id],
        )
        .expect("change title");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_bookmarks_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{BOOKMARKS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 1);
    assert_eq!(summary.exported, 1);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    assert!(text.contains("Changed Title"));
}

#[test]
fn bookmarks_no_prior_file_exports_all() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_bookmark(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let full_path = out_dir.path().join("full.txt");
    let incremental_path = out_dir.path().join("incremental.txt");

    export_bookmarks(&conn, None, &pinned_header_for("{BOOKMARKS}"), &full_path).expect("full export");
    let summary = export_bookmarks_incremental(
        &conn,
        None,
        &pinned_header_for("{BOOKMARKS}"),
        &incremental_path,
    )
    .expect("incremental export with no prior file");

    assert_eq!(
        common::read_file_bytes(&full_path),
        common::read_file_bytes(&incremental_path),
        "no prior file must export the whole category, byte-identical to a full export (D9-05)"
    );
    assert_eq!(summary.added, 1);
    assert_eq!(summary.exported, 1);
}

#[test]
fn bookmarks_malformed_prior_file_aborts() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_bookmark(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let result = export_bookmarks_incremental(
        &conn,
        Some("this is not a valid Bookmarks export file at all"),
        &pinned_header_for("{BOOKMARKS}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

/// Seeds one Highlight whose wire line is `1|1|0|5|1|1|1|1|None|0|nwt|0|0` —
/// matches `tests/fixtures/wire/highlights_prior.txt`'s single data row
/// exactly. Returns the `UserMarkId` (the only column a recolor test mutates).
fn seed_one_highlight(db_path: &std::path::Path) -> i64 {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (920, 1, 1, NULL, NULL, 0, 'nwt', 0, 0)",
        [],
    )
    .expect("insert highlight Location");
    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (920, 1, 920, 0, 'fixture-highlight-usermark-0920', 1)",
        [],
    )
    .expect("insert UserMark");
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (920, 1, 1, 0, 5, 920)",
        [],
    )
    .expect("insert BlockRange");
    920
}

fn read_highlights_prior_fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/highlights_prior.txt");
    std::fs::read_to_string(path).expect("read highlights_prior.txt fixture")
}

#[test]
fn highlights_no_change_reports_zero_and_writes_valid_empty_output() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_highlight(&db_path);
    let prior_text = read_highlights_prior_fixture();

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_highlights_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{HIGHLIGHTS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.deleted_candidates, 0);
    assert_eq!(summary.exported, 0);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    let records = parse_highlights_file(&text).expect("output must itself be a valid Highlights file");
    assert!(records.is_empty());
}

#[test]
fn highlights_colorindex_change_reports_one_modified() {
    let (_dir, db_path) = common::fresh_v16_db();
    let user_mark_id = seed_one_highlight(&db_path);
    let prior_text = read_highlights_prior_fixture();

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE UserMark SET ColorIndex = 2 WHERE UserMarkId = ?1",
            rusqlite::params![user_mark_id],
        )
        .expect("change color");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_highlights_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{HIGHLIGHTS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 1);
    assert_eq!(summary.exported, 1);
}

#[test]
fn highlights_no_prior_file_exports_all() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_highlight(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let full_path = out_dir.path().join("full.txt");
    let incremental_path = out_dir.path().join("incremental.txt");

    export_highlights(&conn, None, &pinned_header_for("{HIGHLIGHTS}"), &full_path).expect("full export");
    let summary = export_highlights_incremental(
        &conn,
        None,
        &pinned_header_for("{HIGHLIGHTS}"),
        &incremental_path,
    )
    .expect("incremental export with no prior file");

    assert_eq!(
        common::read_file_bytes(&full_path),
        common::read_file_bytes(&incremental_path),
        "no prior file must export the whole category, byte-identical to a full export (D9-05)"
    );
    assert_eq!(summary.added, 1);
    assert_eq!(summary.exported, 1);
}

#[test]
fn highlights_malformed_prior_file_aborts() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_highlight(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let result = export_highlights_incremental(
        &conn,
        Some("this is not a valid Highlights export file at all"),
        &pinned_header_for("{HIGHLIGHTS}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

/// Highlights convergence (09-02-PLAN.md Task 2): export incrementally
/// against a baseline prior, re-import that output into the SAME archive,
/// then export incrementally AGAIN against that first output — must
/// converge to zero modified. Explicitly proves that Phase 8's accepted
/// `UserMark` row-growth property (a re-imported `UserMark` can grow a NEW
/// row rather than reusing the original, `apply_import_highlights`'s
/// `synthesize_usermark`) does NOT translate into a non-zero modified count
/// here — `UserMarkId` is not on the Highlights wire, so a fresh `UserMark`
/// with the same `(LocationId, ColorIndex, Version)` and merged `BlockRange`
/// produces the SAME wire line and therefore the SAME hash.
#[test]
fn highlights_incremental_converges() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_highlight(&db_path);

    let out_dir = TempDir::new().expect("tempdir");
    let baseline_path = out_dir.path().join("baseline.txt");
    let baseline_prior_text = {
        let conn = Connection::open(&db_path).expect("open db");
        export_highlights(&conn, None, &pinned_header_for("{HIGHLIGHTS}"), &baseline_path)
            .expect("baseline export");
        std::fs::read_to_string(&baseline_path).expect("read baseline export")
    };

    // Recolor — the one real diff in this test (modified=1).
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute("UPDATE UserMark SET ColorIndex = 2 WHERE UserMarkId = 920", [])
            .expect("recolor");
    }

    let first_output_path = out_dir.path().join("first_incremental.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let first_summary = export_highlights_incremental(
        &conn,
        Some(&baseline_prior_text),
        &pinned_header_for("{HIGHLIGHTS}"),
        &first_output_path,
    )
    .expect("first incremental export");
    assert_eq!(first_summary.modified, 1);
    drop(conn);

    let first_output_text =
        std::fs::read_to_string(&first_output_path).expect("read first incremental output");

    // Re-import that output into the SAME archive.
    {
        let records = parse_highlights_file(&first_output_text).expect("parse first output");
        let mut conn = Connection::open(&db_path).expect("reopen db");
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute available ids");
        apply_import_highlights(&tx, &records, &mut available, 1).expect("apply re-import");
        tx.commit().expect("commit re-import");
    }

    // Second incremental export: against the FIRST output — must converge,
    // even though re-import synthesized a BRAND NEW UserMark (Phase 8's
    // accepted growth property) rather than reusing UserMarkId 920.
    let conn = Connection::open(&db_path).expect("open db after re-import");
    let second_output_path = out_dir.path().join("second_incremental.txt");
    let second_summary = export_highlights_incremental(
        &conn,
        Some(&first_output_text),
        &pinned_header_for("{HIGHLIGHTS}"),
        &second_output_path,
    )
    .expect("second incremental export");

    assert_eq!(second_summary.added, 0, "second run must converge to zero added");
    assert_eq!(second_summary.modified, 0, "second run must converge to zero modified");
}

// ---------------------------------------------------------------------------
// Annotations (09-03-PLAN.md Task 2) — the composite-identity case: identity
// is the wire-recoverable (DOC, LABEL) pair, and export_annotations' own
// LocationId selection means a changed annotation's unchanged siblings ride
// along into the output (disclosed over-selection, never hidden).
// ---------------------------------------------------------------------------

/// Seeds one Annotation whose record is `==={PUB=w}{DOC=None}{LABEL=p1}===`
/// / `Some value` — matches `tests/fixtures/wire/annotations_prior.txt`'s
/// single record exactly, so tests can assert a checked-in STATIC prior file
/// against a live archive. Returns the shared `LocationId`.
fn seed_one_annotation(db_path: &std::path::Path) -> i64 {
    let conn = Connection::open(db_path).expect("open fixture db");
    // DocumentId is set (non-NULL, non-zero) rather than NULL: SQL `=` never
    // matches NULL, so `find_or_insert_annotation_location`'s existing-
    // Location lookup can only find a match (rather than always falling
    // through to a fresh insert) when DocumentId is a concrete value —
    // needed for `annotations_incremental_converges`' re-import to reuse the
    // SAME LocationId rather than growing a new one every run.
    conn.execute(
        "INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
         VALUES (930, 1001, 0, 'w', NULL, 0)",
        [],
    )
    .expect("insert annotation Location");
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (930, 'p1', 'Some value')",
        [],
    )
    .expect("insert annotation InputField");
    930
}

/// Seeds a SECOND annotation at the SAME `LocationId` as
/// [`seed_one_annotation`], with a DIFFERENT `TextTag` — the composite-
/// identity collision case (`annotations_composite_identity` below).
fn seed_second_annotation_at_same_location(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).expect("open fixture db");
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (930, 'p2', 'Sibling value')",
        [],
    )
    .expect("insert sibling annotation InputField");
}

fn read_annotations_prior_fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/annotations_prior.txt");
    std::fs::read_to_string(path).expect("read annotations_prior.txt fixture")
}

#[test]
fn annotations_no_change_reports_zero_and_writes_valid_empty_output() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let prior_text = read_annotations_prior_fixture();

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_annotations_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.deleted_candidates, 0);
    assert_eq!(summary.exported, 0);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    let records = parse_annotations_file(&text).expect("output must itself be a valid Annotations file");
    assert!(records.is_empty(), "output file must contain zero records");
    assert!(text.ends_with("==={END}==="), "the end sentinel must still be written");
}

#[test]
fn annotations_value_change_included() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let prior_text = read_annotations_prior_fixture();

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE InputField SET Value = 'Changed value' WHERE LocationId = 930 AND TextTag = 'p1'",
            [],
        )
        .expect("change value");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_annotations_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 1);
    assert_eq!(summary.exported, 1);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    assert!(text.contains("Changed value"));
}

#[test]
fn annotations_added_included() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let prior_text = read_annotations_prior_fixture();

    seed_second_annotation_at_same_location(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_annotations_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.modified, 0);
    // LocationId over-selection: p1 (unchanged) rides along with the newly
    // added p2 because both share LocationId 930 (disclosed, not hidden).
    assert_eq!(summary.exported, 2);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    assert!(text.contains("Sibling value"));
}

#[test]
fn annotations_deleted_candidate_not_exported() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let prior_text = read_annotations_prior_fixture();

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute("DELETE FROM InputField WHERE LocationId = 930 AND TextTag = 'p1'", [])
            .expect("delete annotation");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_annotations_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 0);
    assert_eq!(summary.modified, 0);
    assert_eq!(summary.exported, 0);
    assert_eq!(summary.deleted_candidates, 1);
}

#[test]
fn annotations_no_prior_file_exports_all() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let full_path = out_dir.path().join("full.txt");
    let incremental_path = out_dir.path().join("incremental.txt");

    export_annotations(&conn, None, &pinned_header_for("{ANNOTATIONS}"), &full_path).expect("full export");
    let summary = export_annotations_incremental(
        &conn,
        None,
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export with no prior file");

    assert_eq!(
        common::read_file_bytes(&full_path),
        common::read_file_bytes(&incremental_path),
        "no prior file must export the whole category, byte-identical to a full export (D9-05)"
    );
    assert_eq!(summary.added, 1);
    assert_eq!(summary.exported, 1);
}

#[test]
fn annotations_malformed_prior_file_aborts() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let conn = Connection::open(&db_path).expect("open db");

    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let result = export_annotations_incremental(
        &conn,
        Some("this is not a valid Annotations export file at all"),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

/// The composite-identity collision test (RESEARCH's sharpest correctness
/// trap): two annotations at the SAME `LocationId` but DIFFERENT `TextTag`s.
/// Editing only one must report exactly ONE modified — proving the two are
/// diffed independently, never collapsed into one `LocationId`-keyed identity
/// — while the WRITTEN record count is TWO, because `export_annotations`
/// selects by `LocationId` and pulls the unchanged sibling in. Neither
/// annotation is omitted, and the two counts (`modified` vs `exported`) do
/// not contradict each other — the disclosed over-selection this plan exists
/// to prove.
#[test]
fn annotations_composite_identity() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    seed_second_annotation_at_same_location(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let baseline_path = out_dir.path().join("baseline.txt");
    export_annotations(&conn, None, &pinned_header_for("{ANNOTATIONS}"), &baseline_path)
        .expect("baseline export");
    let baseline_text = std::fs::read_to_string(&baseline_path).expect("read baseline export");
    drop(conn);

    // Edit ONLY the first annotation (p1); p2 (the sibling at the same
    // LocationId) stays untouched.
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE InputField SET Value = 'Changed value' WHERE LocationId = 930 AND TextTag = 'p1'",
            [],
        )
        .expect("change p1's value");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_annotations_incremental(
        &conn,
        Some(&baseline_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(
        summary.modified, 1,
        "only p1 changed — the two TextTags must be diffed independently, never collapsed into one identity"
    );
    assert_eq!(summary.added, 0);
    assert_eq!(
        summary.exported, 2,
        "LocationId over-selection pulls p2 (the unchanged sibling) into the same output file"
    );

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental export");
    let records = parse_annotations_file(&text).expect("output must itself be a valid Annotations file");
    assert_eq!(records.len(), 2, "neither annotation is omitted");
    assert!(text.contains("Changed value"), "the changed record is present");
    assert!(text.contains("Sibling value"), "the unchanged sibling rides along, disclosed, never hidden");
}

/// Convergence (09-03-PLAN.md Task 2): export changed, re-import that output
/// into the SAME archive, export changed again against the same prior file —
/// must report zero modified. This is the case where the Phase 8 upsert on
/// the `(LocationId, TextTag)` conflict target
/// (`apply_import_annotations`'s `ON CONFLICT(LocationId, TextTag) DO UPDATE`)
/// is what makes convergence hold, so this test asserts it rather than
/// assuming it.
#[test]
fn annotations_incremental_converges() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);

    let out_dir = TempDir::new().expect("tempdir");
    let baseline_path = out_dir.path().join("baseline.txt");
    let baseline_prior_text = {
        let conn = Connection::open(&db_path).expect("open db");
        export_annotations(&conn, None, &pinned_header_for("{ANNOTATIONS}"), &baseline_path)
            .expect("baseline export");
        std::fs::read_to_string(&baseline_path).expect("read baseline export")
    };

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE InputField SET Value = 'Changed value' WHERE LocationId = 930 AND TextTag = 'p1'",
            [],
        )
        .expect("change value");
    }

    let first_output_path = out_dir.path().join("first_incremental.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let first_summary = export_annotations_incremental(
        &conn,
        Some(&baseline_prior_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &first_output_path,
    )
    .expect("first incremental export");
    assert_eq!(first_summary.modified, 1);
    drop(conn);

    let first_output_text =
        std::fs::read_to_string(&first_output_path).expect("read first incremental output");

    // Re-import that output into the SAME archive.
    {
        let records = parse_annotations_file(&first_output_text).expect("parse first output");
        let mut conn = Connection::open(&db_path).expect("reopen db");
        let tx = conn.transaction().expect("begin tx");
        let mut available = compute_available_ids(&tx).expect("compute available ids");
        apply_import_annotations(&tx, &records, &mut available).expect("apply re-import");
        tx.commit().expect("commit re-import");
    }

    let conn = Connection::open(&db_path).expect("open db after re-import");
    let second_output_path = out_dir.path().join("second_incremental.txt");
    let second_summary = export_annotations_incremental(
        &conn,
        Some(&first_output_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &second_output_path,
    )
    .expect("second incremental export");

    assert_eq!(second_summary.added, 0, "second run must converge to zero added");
    assert_eq!(second_summary.modified, 0, "second run must converge to zero modified");
}

// ---------------------------------------------------------------------------
// Cross-category adversarial invariant, CRLF-equivalence, wrong-category and
// empty-prior-body suites (09-04-PLAN.md Task 2). These prove the phase's
// central property category by category: the exported set is
// `{live records whose hash is absent from the prior hash set}` and NEVER
// consults the identity key — so an identity collision, ambiguity, or churn
// can only bias toward over-export, never under-export.
// ---------------------------------------------------------------------------

use std::collections::HashSet;

/// Extracts each Note's own content body into a set — a content-level
/// fingerprint, not a count, so a coincidental record-count match can never
/// pass this comparison.
fn notes_record_set(text: &str) -> HashSet<String> {
    let (_, records) = parse_notes_file(text).expect("valid Notes file");
    records.into_iter().map(|r| r.note).collect()
}

fn favorites_record_set(text: &str) -> HashSet<String> {
    let records = parse_favorites_file(text).expect("valid Favorites file");
    records.into_iter().map(|r| format!("{r:?}")).collect()
}

fn bookmarks_record_set(text: &str) -> HashSet<String> {
    let records = parse_bookmarks_file(text).expect("valid Bookmarks file");
    records.into_iter().map(|r| format!("{r:?}")).collect()
}

fn highlights_record_set(text: &str) -> HashSet<String> {
    let records = parse_highlights_file(text).expect("valid Highlights file");
    records.into_iter().map(|r| format!("{r:?}")).collect()
}

fn annotations_record_set(text: &str) -> HashSet<String> {
    let records = parse_annotations_file(text).expect("valid Annotations file");
    records.into_iter().map(|r| r.value).collect()
}

/// **The invariant test (Notes).** Two notes with an IDENTICAL `{CREATED=}`
/// value — a genuine identity-key collision, not merely a theoretical one —
/// prove `diff_records` never uses the key to decide membership: it iterates
/// the live Vec directly, so a collision can only mislabel added-vs-modified,
/// never drop a record from the exported set. One collides-with-and-is-edited,
/// the sibling with the SAME key stays untouched; a third, brand-new note is
/// also added. Asserted by comparing extracted content sets, never counts.
#[test]
fn notes_invariant_identity_collision_and_new_record_all_exported() {
    let (_dir, db_path) = common::fresh_v16_db();
    let same_created = "2024-03-01T00:00:00";
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content, BlockType, \
             BlockIdentifier, LastModified, Created) \
             VALUES ('note-collide-1', NULL, NULL, 'T1', 'Untouched content', 0, NULL, ?1, ?1)",
            rusqlite::params![same_created],
        )
        .expect("insert collider 1");
        conn.execute(
            "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content, BlockType, \
             BlockIdentifier, LastModified, Created) \
             VALUES ('note-collide-2', NULL, NULL, 'T2', 'Original content 2', 0, NULL, ?1, ?1)",
            rusqlite::params![same_created],
        )
        .expect("insert collider 2");
    }

    let out_dir = TempDir::new().expect("tempdir");
    let prior_path = out_dir.path().join("prior.txt");
    let prior_text = export_baseline(&db_path, &prior_path);

    // Edit ONLY the second colliding note; the first (same identity key)
    // stays untouched, and a brand-new note is also added.
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE Note SET Content = 'Edited content 2' WHERE Guid = 'note-collide-2'",
            [],
        )
        .expect("edit collider 2");
    }
    seed_new_note(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_notes_incremental(
        &conn,
        Some(&prior_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 1, "the identity collision must not swallow the new note into 'modified'");
    assert_eq!(summary.modified, 1);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental output");
    let exported = notes_record_set(&text);
    assert!(
        exported.contains("Edited content 2"),
        "the edited record must be exported despite sharing its identity key with an untouched sibling"
    );
    assert!(
        exported.contains("Second note content"),
        "the brand-new record must be exported"
    );
    assert!(
        !exported.contains("Untouched content"),
        "the untouched sibling — same identity key as the edited record — must NOT be exported"
    );
}

/// **The invariant test (Favorites).** Every wire field is identity (D9-02
/// `<identity_key_specification>`), so there is no possible edited-but-same-
/// key case — this proves the weaker but still content-level property: an
/// untouched Favorite is excluded and a newly added one is included, via a
/// record-set comparison rather than a count.
#[test]
fn favorites_invariant_untouched_excluded_added_included() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_favorite(&db_path);
    let prior_text = read_favorites_prior_fixture();

    seed_second_favorite(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    export_favorites_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    )
    .expect("incremental export");

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental output");
    let exported = favorites_record_set(&text);
    let prior_records = favorites_record_set(&prior_text);
    assert_eq!(exported.len(), 1, "exactly the newly added favorite is written");
    assert!(
        exported.is_disjoint(&prior_records),
        "the untouched prior favorite must not appear in the exported set"
    );
}

/// **The invariant test (Bookmarks).** Two bookmarks share an IDENTICAL
/// identity key (BookNumber/Chapter/DocumentId/IssueTagNumber/KeySymbol/
/// MepsLanguage/Type/Slot all equal) but different `Title` — a real identity
/// collision, constructed via two Bookmarks sharing one `LocationId` and
/// `Slot`. Editing the first and adding the second means BOTH live records
/// differ in hash from the single prior entry at that shared key —
/// `diff_records` never removes a key from `prior_keys` once matched, so a
/// genuine collision like this labels BOTH `modified` rather than one
/// `added`/one `modified`. The mislabeling is exactly the "identity is wrong
/// or ambiguous" case this test exists to probe: the LABEL can be wrong, but
/// membership in the exported set (hash-based, never key-based) cannot drop
/// either record.
#[test]
fn bookmarks_invariant_identity_collision_and_new_record_all_exported() {
    let (_dir, db_path) = common::fresh_v16_db();
    let bookmark_id = seed_one_bookmark(&db_path);
    let prior_text = read_bookmarks_prior_fixture();

    // A second bookmark at a DIFFERENT PublicationLocationId, but pointing at
    // the SAME LocationId (910) as `seed_one_bookmark`'s — since Bookmarks'
    // identity key is built entirely from the resolved Location fields plus
    // Slot, two Bookmarks sharing one LocationId AND Slot are a genuine
    // identity collision (`Location`'s own UNIQUE constraint on
    // BookNumber/ChapterNumber/KeySymbol/MepsLanguage/Type rules out a
    // second, distinct Location row with identical resolved fields).
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
             IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (911, NULL, NULL, 1001, 5, 0, 'pub-x', 0, 0)",
            [],
        )
        .expect("insert second PublicationLocationId");
        conn.execute(
            "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
             Snippet, BlockType, BlockIdentifier) \
             VALUES (911, 910, 911, 0, 'Untouched Title', 'Untouched Snippet', 0, NULL)",
            [],
        )
        .expect("insert colliding Bookmark");
    }

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE Bookmark SET Title = 'Edited Title' WHERE BookmarkId = ?1",
            rusqlite::params![bookmark_id],
        )
        .expect("edit first bookmark");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_bookmarks_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{BOOKMARKS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(
        summary.modified, 2,
        "an identity-key collision must mislabel neither bookmark OUT of the exported set"
    );
    assert_eq!(summary.added, 0);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental output");
    assert!(text.contains("Edited Title"), "the edited bookmark must be exported");
    assert!(text.contains("Untouched Snippet"), "the new colliding bookmark must be exported");
}

/// **The invariant test (Highlights).** Two highlights share an IDENTICAL
/// identity key (BlockType/Identifier/StartToken/EndToken plus all seven
/// Location fields) but different `ColorIndex` — a real collision. Recoloring
/// only one must not suppress its export, and must not falsely export the
/// untouched collider.
#[test]
fn highlights_invariant_identity_collision_and_new_record_all_exported() {
    let (_dir, db_path) = common::fresh_v16_db();
    let user_mark_id = seed_one_highlight(&db_path);
    let prior_text = read_highlights_prior_fixture();

    // A second highlight — its OWN UserMark/BlockRange, but pointing at the
    // SAME LocationId (920) and the SAME BlockType/Identifier/StartToken/
    // EndToken as `seed_one_highlight`'s — since Highlights' identity key
    // excludes ColorIndex/Version, this is a genuine identity collision
    // (`Location`'s own UNIQUE constraint rules out a second, distinct
    // Location row with identical resolved fields).
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
             VALUES (921, 2, 920, 0, 'fixture-highlight-usermark-0921', 1)",
            [],
        )
        .expect("insert colliding UserMark");
        conn.execute(
            "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
             VALUES (921, 1, 1, 0, 5, 921)",
            [],
        )
        .expect("insert colliding BlockRange");
    }

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE UserMark SET ColorIndex = 3 WHERE UserMarkId = ?1",
            rusqlite::params![user_mark_id],
        )
        .expect("recolor first highlight");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_highlights_incremental(
        &conn,
        Some(&prior_text),
        &pinned_header_for("{HIGHLIGHTS}"),
        &incremental_path,
    )
    .expect("incremental export");

    // Both live highlights share ONE identity key (ColorIndex/Version are
    // excluded from it) and BOTH differ in hash from the single prior entry
    // at that key — `diff_records` never removes a key from `prior_keys`
    // once matched, so a genuine collision like this labels BOTH `modified`
    // rather than one `added`/one `modified`. This mislabeling is exactly
    // the "identity is wrong or ambiguous" case the invariant test exists to
    // probe: the label can be wrong, but membership in the exported set
    // (hash-set based, never key-based) cannot drop either record.
    assert_eq!(
        summary.modified, 2,
        "an identity-key collision must mislabel neither highlight OUT of the exported set"
    );
    assert_eq!(summary.added, 0);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental output");
    let exported = highlights_record_set(&text);
    let recolored = exported.iter().any(|r| r.contains("color_index: 3"));
    let new_one = exported.iter().any(|r| r.contains("color_index: 2"));
    let untouched_original = exported.iter().any(|r| r.contains("color_index: 1"));
    assert!(recolored, "the recolored highlight must be exported");
    assert!(new_one, "the new colliding highlight must be exported");
    assert!(
        !untouched_original,
        "the prior's own (now-superseded) colorindex-1 hash must not itself appear as a live record"
    );
}

/// **The invariant test (Annotations)**, the phase's sharpest case: 09-03
/// already proved the composite `(DOC, LABEL)` identity is diffed
/// independently at one `LocationId` (`annotations_composite_identity`); this
/// adds a THIRD, brand-new annotation at a different Location on top of that
/// same edited/untouched pair, so the assertion spans an edit, an add, and an
/// untouched sibling together, via record-set comparison.
#[test]
fn annotations_invariant_edit_add_and_untouched_sibling_all_correct() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    seed_second_annotation_at_same_location(&db_path);

    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let baseline_path = out_dir.path().join("baseline.txt");
    export_annotations(&conn, None, &pinned_header_for("{ANNOTATIONS}"), &baseline_path)
        .expect("baseline export");
    let baseline_text = std::fs::read_to_string(&baseline_path).expect("read baseline export");
    drop(conn);

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE InputField SET Value = 'Edited value' WHERE LocationId = 930 AND TextTag = 'p1'",
            [],
        )
        .expect("edit p1");
        conn.execute(
            "INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type) \
             VALUES (931, 1002, 0, 'w', NULL, 0)",
            [],
        )
        .expect("insert new Location");
        conn.execute(
            "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (931, 'p1', 'Brand new value')",
            [],
        )
        .expect("insert new annotation");
    }

    let conn = Connection::open(&db_path).expect("open db");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_annotations_incremental(
        &conn,
        Some(&baseline_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.modified, 1);

    let text = std::fs::read_to_string(&incremental_path).expect("read incremental output");
    let exported = annotations_record_set(&text);
    assert!(exported.contains("Edited value"), "the edited annotation must be exported");
    assert!(exported.contains("Brand new value"), "the brand-new annotation must be exported");
    assert!(
        exported.contains("Sibling value"),
        "p2 (the unchanged sibling at p1's LocationId) rides along — LocationId over-selection, disclosed not hidden"
    );
}

// ---------------------------------------------------------------------------
// CRLF equivalence — the real Python-on-Windows byte shape (08-DIFFERENTIAL-
// WIRE.md's finding). A prior file with CRLF line endings must diff
// identically to the same content with LF endings, for every category.
// ---------------------------------------------------------------------------

#[test]
fn notes_crlf_prior_diffs_identically_to_lf() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path, "Title", "Content");
    let prior_lf = read_notes_prior_fixture();
    let prior_crlf = prior_lf.replace('\n', "\r\n");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute("UPDATE Note SET Content = 'Changed content'", [])
            .expect("change content");
    }

    let out_dir = TempDir::new().expect("tempdir");
    let lf_path = out_dir.path().join("lf.txt");
    let crlf_path = out_dir.path().join("crlf.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let lf_summary = export_notes_incremental(
        &conn, Some(&prior_lf), &catalog(), &pinned_header(), "2099-01-01T00:00:00Z", &lf_path,
    )
    .expect("lf export");
    let crlf_summary = export_notes_incremental(
        &conn, Some(&prior_crlf), &catalog(), &pinned_header(), "2099-01-01T00:00:00Z", &crlf_path,
    )
    .expect("crlf export");

    assert_eq!(lf_summary.added, crlf_summary.added);
    assert_eq!(lf_summary.modified, crlf_summary.modified);
    assert_eq!(lf_summary.deleted_candidates, crlf_summary.deleted_candidates);
    assert_eq!(
        notes_record_set(&std::fs::read_to_string(&lf_path).unwrap()),
        notes_record_set(&std::fs::read_to_string(&crlf_path).unwrap()),
    );
}

#[test]
fn favorites_crlf_prior_diffs_identically_to_lf() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_favorite(&db_path);
    let prior_lf = read_favorites_prior_fixture();
    let prior_crlf = prior_lf.replace('\n', "\r\n");
    seed_second_favorite(&db_path);

    let out_dir = TempDir::new().expect("tempdir");
    let lf_path = out_dir.path().join("lf.txt");
    let crlf_path = out_dir.path().join("crlf.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let lf_summary = export_favorites_incremental(&conn, Some(&prior_lf), &pinned_header_for("{FAVORITES}"), &lf_path)
        .expect("lf export");
    let crlf_summary =
        export_favorites_incremental(&conn, Some(&prior_crlf), &pinned_header_for("{FAVORITES}"), &crlf_path)
            .expect("crlf export");

    assert_eq!(lf_summary.added, crlf_summary.added);
    assert_eq!(lf_summary.modified, crlf_summary.modified);
    assert_eq!(lf_summary.deleted_candidates, crlf_summary.deleted_candidates);
    assert_eq!(
        favorites_record_set(&std::fs::read_to_string(&lf_path).unwrap()),
        favorites_record_set(&std::fs::read_to_string(&crlf_path).unwrap()),
    );
}

#[test]
fn bookmarks_crlf_prior_diffs_identically_to_lf() {
    let (_dir, db_path) = common::fresh_v16_db();
    let bookmark_id = seed_one_bookmark(&db_path);
    let prior_lf = read_bookmarks_prior_fixture();
    let prior_crlf = prior_lf.replace('\n', "\r\n");
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE Bookmark SET Title = 'Changed Title' WHERE BookmarkId = ?1",
            rusqlite::params![bookmark_id],
        )
        .expect("change title");
    }

    let out_dir = TempDir::new().expect("tempdir");
    let lf_path = out_dir.path().join("lf.txt");
    let crlf_path = out_dir.path().join("crlf.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let lf_summary = export_bookmarks_incremental(&conn, Some(&prior_lf), &pinned_header_for("{BOOKMARKS}"), &lf_path)
        .expect("lf export");
    let crlf_summary =
        export_bookmarks_incremental(&conn, Some(&prior_crlf), &pinned_header_for("{BOOKMARKS}"), &crlf_path)
            .expect("crlf export");

    assert_eq!(lf_summary.added, crlf_summary.added);
    assert_eq!(lf_summary.modified, crlf_summary.modified);
    assert_eq!(lf_summary.deleted_candidates, crlf_summary.deleted_candidates);
    assert_eq!(
        bookmarks_record_set(&std::fs::read_to_string(&lf_path).unwrap()),
        bookmarks_record_set(&std::fs::read_to_string(&crlf_path).unwrap()),
    );
}

#[test]
fn highlights_crlf_prior_diffs_identically_to_lf() {
    let (_dir, db_path) = common::fresh_v16_db();
    let user_mark_id = seed_one_highlight(&db_path);
    let prior_lf = read_highlights_prior_fixture();
    let prior_crlf = prior_lf.replace('\n', "\r\n");
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE UserMark SET ColorIndex = 2 WHERE UserMarkId = ?1",
            rusqlite::params![user_mark_id],
        )
        .expect("change color");
    }

    let out_dir = TempDir::new().expect("tempdir");
    let lf_path = out_dir.path().join("lf.txt");
    let crlf_path = out_dir.path().join("crlf.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let lf_summary =
        export_highlights_incremental(&conn, Some(&prior_lf), &pinned_header_for("{HIGHLIGHTS}"), &lf_path)
            .expect("lf export");
    let crlf_summary =
        export_highlights_incremental(&conn, Some(&prior_crlf), &pinned_header_for("{HIGHLIGHTS}"), &crlf_path)
            .expect("crlf export");

    assert_eq!(lf_summary.added, crlf_summary.added);
    assert_eq!(lf_summary.modified, crlf_summary.modified);
    assert_eq!(lf_summary.deleted_candidates, crlf_summary.deleted_candidates);
    assert_eq!(
        highlights_record_set(&std::fs::read_to_string(&lf_path).unwrap()),
        highlights_record_set(&std::fs::read_to_string(&crlf_path).unwrap()),
    );
}

#[test]
fn annotations_crlf_prior_diffs_identically_to_lf() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let prior_lf = read_annotations_prior_fixture();
    let prior_crlf = prior_lf.replace('\n', "\r\n");
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "UPDATE InputField SET Value = 'Changed value' WHERE LocationId = 930 AND TextTag = 'p1'",
            [],
        )
        .expect("change value");
    }

    let out_dir = TempDir::new().expect("tempdir");
    let lf_path = out_dir.path().join("lf.txt");
    let crlf_path = out_dir.path().join("crlf.txt");
    let conn = Connection::open(&db_path).expect("open db");
    let lf_summary =
        export_annotations_incremental(&conn, Some(&prior_lf), &pinned_header_for("{ANNOTATIONS}"), &lf_path)
            .expect("lf export");
    let crlf_summary =
        export_annotations_incremental(&conn, Some(&prior_crlf), &pinned_header_for("{ANNOTATIONS}"), &crlf_path)
            .expect("crlf export");

    assert_eq!(lf_summary.added, crlf_summary.added);
    assert_eq!(lf_summary.modified, crlf_summary.modified);
    assert_eq!(lf_summary.deleted_candidates, crlf_summary.deleted_candidates);
    assert_eq!(
        annotations_record_set(&std::fs::read_to_string(&lf_path).unwrap()),
        annotations_record_set(&std::fs::read_to_string(&crlf_path).unwrap()),
    );
}

// ---------------------------------------------------------------------------
// Wrong-category prior file — the category-tag guard (T-09-14). Feeding a
// prior file exported from a DIFFERENT category returns the typed malformed
// error and writes no output, for every category.
// ---------------------------------------------------------------------------

#[test]
fn notes_wrong_category_prior_file_rejected() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_note(&db_path, "Title", "Content");
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");

    let result = export_notes_incremental(
        &conn,
        Some(&read_favorites_prior_fixture()),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists(), "no output file when the prior file's category tag is wrong");
}

#[test]
fn favorites_wrong_category_prior_file_rejected() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_favorite(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");

    let result = export_favorites_incremental(
        &conn,
        Some(&read_bookmarks_prior_fixture()),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

#[test]
fn bookmarks_wrong_category_prior_file_rejected() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_bookmark(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");

    let result = export_bookmarks_incremental(
        &conn,
        Some(&read_highlights_prior_fixture()),
        &pinned_header_for("{BOOKMARKS}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

#[test]
fn highlights_wrong_category_prior_file_rejected() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_highlight(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");

    let result = export_highlights_incremental(
        &conn,
        Some(&read_annotations_prior_fixture()),
        &pinned_header_for("{HIGHLIGHTS}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

#[test]
fn annotations_wrong_category_prior_file_rejected() {
    let (_dir, db_path) = common::fresh_v16_db();
    seed_one_annotation(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");

    let result = export_annotations_incremental(
        &conn,
        Some(&read_notes_prior_fixture()),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    );

    match result {
        Err(ArchiveError::ImportMalformed { .. }) => {}
        other => panic!("expected ImportMalformed, got {other:?}"),
    }
    assert!(!incremental_path.exists());
}

// ---------------------------------------------------------------------------
// Empty-prior-body — header present, zero records. Must behave exactly as
// "no prior file" for what gets exported (everything reports as `added`, no
// `deleted_candidates`), while still being a real, valid prior file for
// fail-fast validation purposes (the category-tag check still runs and
// passes; this is NOT the malformed-file path).
// ---------------------------------------------------------------------------

#[test]
fn notes_empty_prior_body_behaves_as_no_prior_file() {
    let (_dir, db_path) = common::fresh_v16_db();
    // An empty-but-valid prior: a real export of a database with zero Notes.
    let empty_prior_dir = TempDir::new().expect("tempdir");
    let empty_prior_path = empty_prior_dir.path().join("empty_prior.txt");
    {
        let conn = Connection::open(&db_path).expect("open db");
        export_notes(&conn, None, &catalog(), &pinned_header(), "2099-01-01T00:00:00Z", &empty_prior_path)
            .expect("export empty prior");
    }
    let empty_prior_text = std::fs::read_to_string(&empty_prior_path).expect("read empty prior");

    seed_one_note(&db_path, "Title", "Content");
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let full_path = out_dir.path().join("full.txt");
    let incremental_path = out_dir.path().join("incremental.txt");
    export_notes(&conn, None, &catalog(), &pinned_header(), "2099-01-01T00:00:00Z", &full_path)
        .expect("full export");

    let summary = export_notes_incremental(
        &conn,
        Some(&empty_prior_text),
        &catalog(),
        &pinned_header(),
        "2099-01-01T00:00:00Z",
        &incremental_path,
    )
    .expect("incremental export against an empty-but-valid prior");

    assert_eq!(summary.added, 1, "an empty prior body must export everything, exactly like no prior file");
    assert_eq!(summary.deleted_candidates, 0);
    assert_eq!(
        common::read_file_bytes(&full_path),
        common::read_file_bytes(&incremental_path),
        "empty-prior-body output must be byte-identical to a full export, same as D9-05's no-prior-file case"
    );
}

#[test]
fn favorites_empty_prior_body_behaves_as_no_prior_file() {
    let (_dir, db_path) = common::fresh_v16_db();
    let empty_prior_dir = TempDir::new().expect("tempdir");
    let empty_prior_path = empty_prior_dir.path().join("empty_prior.txt");
    {
        let conn = Connection::open(&db_path).expect("open db");
        export_favorites(&conn, None, &pinned_header_for("{FAVORITES}"), &empty_prior_path)
            .expect("export empty prior");
    }
    let empty_prior_text = std::fs::read_to_string(&empty_prior_path).expect("read empty prior");

    seed_one_favorite(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_favorites_incremental(
        &conn,
        Some(&empty_prior_text),
        &pinned_header_for("{FAVORITES}"),
        &incremental_path,
    )
    .expect("incremental export against an empty-but-valid prior");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.deleted_candidates, 0);
}

#[test]
fn bookmarks_empty_prior_body_behaves_as_no_prior_file() {
    let (_dir, db_path) = common::fresh_v16_db();
    let empty_prior_dir = TempDir::new().expect("tempdir");
    let empty_prior_path = empty_prior_dir.path().join("empty_prior.txt");
    {
        let conn = Connection::open(&db_path).expect("open db");
        export_bookmarks(&conn, None, &pinned_header_for("{BOOKMARKS}"), &empty_prior_path)
            .expect("export empty prior");
    }
    let empty_prior_text = std::fs::read_to_string(&empty_prior_path).expect("read empty prior");

    seed_one_bookmark(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_bookmarks_incremental(
        &conn,
        Some(&empty_prior_text),
        &pinned_header_for("{BOOKMARKS}"),
        &incremental_path,
    )
    .expect("incremental export against an empty-but-valid prior");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.deleted_candidates, 0);
}

#[test]
fn highlights_empty_prior_body_behaves_as_no_prior_file() {
    let (_dir, db_path) = common::fresh_v16_db();
    let empty_prior_dir = TempDir::new().expect("tempdir");
    let empty_prior_path = empty_prior_dir.path().join("empty_prior.txt");
    {
        let conn = Connection::open(&db_path).expect("open db");
        export_highlights(&conn, None, &pinned_header_for("{HIGHLIGHTS}"), &empty_prior_path)
            .expect("export empty prior");
    }
    let empty_prior_text = std::fs::read_to_string(&empty_prior_path).expect("read empty prior");

    seed_one_highlight(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_highlights_incremental(
        &conn,
        Some(&empty_prior_text),
        &pinned_header_for("{HIGHLIGHTS}"),
        &incremental_path,
    )
    .expect("incremental export against an empty-but-valid prior");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.deleted_candidates, 0);
}

#[test]
fn annotations_empty_prior_body_behaves_as_no_prior_file() {
    let (_dir, db_path) = common::fresh_v16_db();
    let empty_prior_dir = TempDir::new().expect("tempdir");
    let empty_prior_path = empty_prior_dir.path().join("empty_prior.txt");
    {
        let conn = Connection::open(&db_path).expect("open db");
        export_annotations(&conn, None, &pinned_header_for("{ANNOTATIONS}"), &empty_prior_path)
            .expect("export empty prior");
    }
    let empty_prior_text = std::fs::read_to_string(&empty_prior_path).expect("read empty prior");

    seed_one_annotation(&db_path);
    let conn = Connection::open(&db_path).expect("open db");
    let out_dir = TempDir::new().expect("tempdir");
    let incremental_path = out_dir.path().join("incremental.txt");
    let summary = export_annotations_incremental(
        &conn,
        Some(&empty_prior_text),
        &pinned_header_for("{ANNOTATIONS}"),
        &incremental_path,
    )
    .expect("incremental export against an empty-but-valid prior");

    assert_eq!(summary.added, 1);
    assert_eq!(summary.deleted_candidates, 0);
}
