//! RED end-to-end test for the `open_archive` Tauri command.
//!
//! Compile-safe by construction (finding 7, 01-01-PLAN.md): this test does
//! NOT reference the `open_archive` command symbol, which does not exist
//! yet — that would break `cargo test` for the whole integration-test crate
//! before 01-07 lands it. Instead it is `#[ignore]`d and its body only
//! exercises the fixture (already real, from `tests/common/mod.rs`) so the
//! assertions 01-07 needs are proven out against real data shapes ahead of
//! time. 01-07 removes `#[ignore]` and replaces the body with a real call
//! through the Tauri command, turning this test green.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use rusqlite::Connection;

/// RED (01-07 turns this green): asserts the res/blank-seeded synthetic
/// fixture, once opened, would yield a Notes list with at least one row
/// (located + independent, per RESEARCH.md Pitfall 2). Ignored until the
/// real `open_archive` command exists to drive this through the actual
/// Tauri IPC boundary rather than a direct DB query.
#[test]
#[ignore = "RED until 01-07 implements the open_archive command — un-ignore there"]
fn test_open_archive_lists_at_least_one_note() {
    // TODO(01-07): replace this direct-DB assertion with a real call through
    // the `open_archive` Tauri command once it exists, e.g.:
    //   let result = jwlmanager_lib::archive::open_archive(fixture_path)?;
    //   assert!(result.notes.len() >= 1);
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_extract_dir, extracted_path) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted_path.join("userData.db")).expect("db must open");
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM Note", [], |row| row.get(0))
        .expect("note count query");
    assert!(
        total >= 1,
        "fixture must list at least one note once opened"
    );
}
