//! Notes incremental export behaviour tests (IO-04, 09-01-PLAN.md Task 2/3).
//!
//! Drives [`export_notes_incremental`] directly (this codebase's established
//! `*_impl`-is-directly-testable shape — the Tauri command in `lib.rs` is a
//! thin session/path wrapper over this pure function) against a synthetic
//! `res/blank`-seeded fixture, mutating the archive between a baseline
//! ("prior") export and the incremental export under test.

mod common;

use jwlmanager_lib::db::ids::compute_available_ids;
use jwlmanager_lib::db::io::diff::export_notes_incremental;
use jwlmanager_lib::db::io::export::export_notes;
use jwlmanager_lib::db::io::header::ExportHeaderCtx;
use jwlmanager_lib::db::io::import::{apply_import_notes, parse_notes_file};
use jwlmanager_lib::db::resources::{dev_resources_db_path, ResourceCatalog};
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;
use tempfile::TempDir;

fn pinned_header() -> ExportHeaderCtx<'static> {
    ExportHeaderCtx {
        category_tag: "{NOTES=}",
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
