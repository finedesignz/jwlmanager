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
