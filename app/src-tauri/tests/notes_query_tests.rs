//! Proves the independent-notes UNION isn't silently dropped (Core Value:
//! never lose the user's data) and that labels are synthesized via
//! resources.db, not raw IDs.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::db::notes::query_notes;
use jwlmanager_lib::db::resources::{dev_resources_db_path, ResourceCatalog};
use rusqlite::Connection;

#[test]
fn notes_query_includes_independent() {
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_extract_dir, extracted_path) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted_path.join("userData.db")).expect("db must open");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");

    let rows = query_notes(&conn, &catalog).expect("query_notes must succeed");

    let located = rows.iter().filter(|r| !r.independent).count();
    let independent = rows.iter().filter(|r| r.independent).count();
    assert!(located >= 1, "expected at least one located note");
    assert!(
        independent >= 1,
        "expected at least one independent note — the UNION must not drop it"
    );
    assert_eq!(rows.len(), located + independent);

    // Labels are synthesized (language name + resolved symbol/detail), not
    // raw IDs — the fixture's located note is a Genesis 1:1 reference under
    // MepsLanguage 0 (English).
    let located_row = rows
        .iter()
        .find(|r| !r.independent)
        .expect("a located row must be present");
    assert_eq!(located_row.language.as_deref(), Some("English"));
    assert_eq!(located_row.detail1.as_deref(), Some("01: Genesis"));
}
