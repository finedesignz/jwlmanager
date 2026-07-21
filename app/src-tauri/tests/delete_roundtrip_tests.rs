//! QA-02 semantic round-trip: delete -> save (trim runs) -> reopen equals
//! the expected normalized post-state (02-02-PLAN.md Task 3). NEVER asserts
//! byte equality — save is not byte-preserving (VACUUM + tag re-densify),
//! only `common::normalized_table_rows`/targeted-existence assertions on the
//! reopened archive are used, per CLAUDE.md's Core Value.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::archive::save::save_archive;
use jwlmanager_lib::db::delete::{delete_notes, NonEmptyNoteIds};
use jwlmanager_lib::db::resources::dev_resources_db_path;
use rusqlite::Connection;

fn apply_delete(db_path: &std::path::Path, note_ids: Vec<i64>) {
    let conn = Connection::open(db_path).expect("open working db");
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let ids = NonEmptyNoteIds::try_from(note_ids).unwrap();
    let tx = conn.unchecked_transaction().expect("open tx");
    delete_notes(&tx, &ids).expect("delete_notes must succeed");
    tx.commit().expect("commit delete");
}

/// Multi-table-orphan round-trip (QA-02): deleting Note 900 genuinely
/// orphans ONLY its TagMap 900 / Tag 900 mapping — its highlight (UserMark
/// 900 -> BlockRange 900 -> Location 900) is durable and SURVIVES (D2-05
/// corrected scope / 02-01-SUMMARY.md finding 1: a UserMark with a
/// BlockRange is a real highlight, not owned by the Note). Save's trim
/// sweeps the TagMap/Tag orphans and re-densifies Tag 901's gapped TagMap
/// positions. Unrelated rows (the survivor highlight UserMark/BlockRange/
/// Location 890, the base fixture's located/independent notes) are
/// preserved.
#[test]
fn test_delete_round_trip_semantic_equivalence() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    apply_delete(&session.db_path, vec![900]);

    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save must succeed");

    let (_reopened_dir, reopened) = common::extract_to_tempdir(&session.target_path);
    let conn = Connection::open(reopened.join("userData.db")).expect("open reopened db");

    // Note 900 gone.
    let note_900: i64 = conn
        .query_row("SELECT COUNT(*) FROM Note WHERE NoteId = 900", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(note_900, 0, "Note 900 must be gone after save");

    // Its highlight (UserMark 900 -> BlockRange 900 -> Location 900) is
    // DURABLE and SURVIVES the Note's deletion (D2-05 corrected scope /
    // 02-01-SUMMARY.md finding 1): a UserMark with a BlockRange is a real
    // highlight, not an orphan, regardless of whether the Note that used to
    // anchor it still exists. Only the Note's TagMap entry is genuinely
    // orphaned by this delete.
    for (table, col, id) in [
        ("UserMark", "UserMarkId", 900),
        ("BlockRange", "BlockRangeId", 900),
        ("Location", "LocationId", 900),
    ] {
        let count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE {col} = {id}"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "{table} {id} is a durable highlight and must SURVIVE the Note's deletion"
        );
    }

    // TagMap 900 / Tag 900 (Note 900's own tag mapping) gone.
    let tagmap_900: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE TagMapId = 900",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tagmap_900, 0, "TagMap 900 must be gone");
    let tag_900: i64 = conn
        .query_row("SELECT COUNT(*) FROM Tag WHERE TagId = 900", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(tag_900, 0, "Tag 900 must be unused and swept");

    // Tag 901's THREE unrelated TagMap rows (902@5, 903@9, 904@20) must be
    // re-densified to contiguous 0-based positions, ordered by original
    // Position then TagMapId, and must all still reference Tag 901.
    let mut stmt = conn
        .prepare("SELECT TagMapId, Position FROM TagMap WHERE TagId = 901 ORDER BY Position")
        .unwrap();
    let rows: Vec<(i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![(902, 0), (903, 1), (904, 2)],
        "Tag 901's TagMap rows must be re-densified to contiguous 0-based positions"
    );

    // The SURVIVING highlight (UserMark/BlockRange/Location 890) — anchored
    // to no deleted Note — must be untouched.
    for (table, col, id) in [
        ("UserMark", "UserMarkId", 890),
        ("BlockRange", "BlockRangeId", 890),
        ("Location", "LocationId", 890),
    ] {
        let count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE {col} = {id}"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "surviving highlight {table} {id} must be preserved"
        );
    }

    // The base fixture's own located/independent notes (unrelated rows)
    // must be preserved.
    let base_notes = common::normalized_table_rows(&conn, "Note");
    assert!(
        base_notes.keys().any(|k| k.starts_with("Integer(1)|")),
        "base fixture Note 1 (located) must be preserved: {base_notes:?}"
    );
}

/// Finding 9 survivor: deleting Note 901 (which references Location 901)
/// must NOT sweep Location 901 away, because `Bookmark.LocationId` /
/// `Bookmark.PublicationLocationId` ALSO reference it.
#[test]
fn test_deleted_note_location_survives_when_bookmark_references_it() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    apply_delete(&session.db_path, vec![901]);

    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save must succeed");

    let (_reopened_dir, reopened) = common::extract_to_tempdir(&session.target_path);
    let conn = Connection::open(reopened.join("userData.db")).expect("open reopened db");

    let note_901: i64 = conn
        .query_row("SELECT COUNT(*) FROM Note WHERE NoteId = 901", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(note_901, 0, "Note 901 must be gone after save");

    let location_901: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE LocationId = 901",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        location_901, 1,
        "Location 901 must SURVIVE — it is still referenced by Bookmark 900"
    );

    let bookmark_900: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Bookmark WHERE BookmarkId = 900 AND LocationId = 901",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bookmark_900, 1, "Bookmark 900 must be untouched");
}

/// Differential leg (mirrors `tests/differential.rs`'s real headless
/// oracle): a delete-then-saved archive must still pass Python
/// `check_validity`. `#[ignore]`d for the same reason as
/// `tests/differential.rs` (CI is a Rust-only matrix, no PySide6 install
/// step) — run explicitly with
/// `cargo test --test delete_roundtrip_tests -- --ignored` on a machine
/// with `res/requirements.txt` installed + the win32 root-staged
/// jwlCore/sqlite3 DLLs.
#[test]
#[ignore = "requires python3 + PySide6 (res/requirements.txt) + the win32 root-staged \
            jwlCore/sqlite3 DLLs; CI is a Rust-only matrix"]
fn test_python_accepts_delete_then_save() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    apply_delete(&session.db_path, vec![900]);

    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_archive must succeed before handing off to the Python oracle");

    let (ok, stdout, stderr) = run_python_check_validity(&archive_path);
    assert!(
        ok,
        "Python app (JWLManager.check_validity) did not accept the delete-then-saved \
         archive.\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Duplicated (not shared, `tests/differential.rs`'s copy is private to that
/// binary) minimal Python-oracle invocation: shells to `python3` and calls
/// `JWLManager.Window.check_validity` (unbound, `self=None`) against the
/// given archive path. Returns `(accepted, stdout, stderr)`.
fn run_python_check_validity(archive_path: &std::path::Path) -> (bool, String, String) {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let saved_path = archive_path.to_string_lossy().replace('\\', "\\\\");
    let python_code = format!(
        "import sys\n\
         sys.path.insert(0, r'{root}')\n\
         import JWLManager\n\
         ok = JWLManager.Window.check_validity(None, '{path}')\n\
         print('ORACLE_RESULT:' + ('PASS' if ok else 'FAIL'))\n",
        root = repo_root.display(),
        path = saved_path
    );

    let path_var = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let patched_path = format!("{}{}{}", repo_root.display(), sep, path_var);

    let output = std::process::Command::new("python3")
        .arg("-c")
        .arg(&python_code)
        .current_dir(&repo_root)
        .env("PATH", &patched_path)
        .output()
        .expect("failed to invoke python3 — is it on PATH?");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let ok = output.status.success() && stdout.contains("ORACLE_RESULT:PASS");
    (ok, stdout, stderr)
}
