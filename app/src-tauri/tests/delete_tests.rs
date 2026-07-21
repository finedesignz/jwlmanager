//! EDIT-01 / SAFE-02 / SAFE-03 / SAFE-04 coverage for the delete backend
//! (02-02-PLAN.md Task 2). Command-registration wiring is exercised
//! indirectly via the core `db::delete` functions the commands call —
//! Tauri commands themselves aren't invokable outside a running app, so the
//! same core fns / typed boundary these commands wrap are asserted here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::delete::{delete_notes, NonEmptyNoteIds};
use rusqlite::Connection;

/// EDIT-01: deleting a selection removes exactly the targeted `Note` rows —
/// nothing else — and the fixture's highlight (UserMark/BlockRange 900)
/// survives the delete step untouched.
#[test]
fn test_delete_notes_removes_selected_rows() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let conn = Connection::open(&db_path).expect("open extracted db");
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let ids = NonEmptyNoteIds::try_from(vec![900_i64]).unwrap();

    let tx = conn.unchecked_transaction().expect("open tx");
    let deleted = delete_notes(&tx, &ids).expect("delete_notes must succeed");
    assert_eq!(deleted, 1, "exactly one Note row must be removed");

    let note_exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM Note WHERE NoteId = 900)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!note_exists, "Note 900 must be gone");

    let usermark_exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM UserMark WHERE UserMarkId = 900)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let blockrange_exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM BlockRange WHERE BlockRangeId = 900)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        usermark_exists && blockrange_exists,
        "UserMark/BlockRange 900 must survive the delete step (only trim sweeps genuine orphans)"
    );

    tx.rollback().unwrap();
}

/// SAFE-03/D2-06: an empty array must fail to deserialize into
/// `NonEmptyNoteIds` — never reaching the DB — while a non-empty one
/// deserializes fine.
#[test]
fn test_empty_selection_fails_deserialization() {
    let empty: Result<NonEmptyNoteIds, _> = serde_json::from_str("[]");
    assert!(empty.is_err(), "empty selection must fail to deserialize");

    let non_empty: Result<NonEmptyNoteIds, _> = serde_json::from_str("[42]");
    assert!(non_empty.is_ok(), "non-empty selection must deserialize");
}

/// SAFE-02: the delete SQL is always a single static
/// "DELETE FROM Note WHERE NoteId IN (?,?,...)" with only the placeholder
/// COUNT varying; a large/adversarial-looking id list still binds safely
/// and deletes nothing spurious (ids are typed `i64`, so a SQL-injection
/// string is impossible to construct as an id in the first place).
#[test]
fn test_delete_sql_is_parameterized() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let conn = Connection::open(&db_path).expect("open extracted db");
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();

    // A large, adversarial-shaped id list (negative, huge, and the real
    // target 900) — since ids are i64, there is no string-interpolation
    // channel to exploit; only the real matching id (900) is ever removed.
    let ids = NonEmptyNoteIds::try_from(vec![
        900_i64,
        -1,
        i64::MAX,
        123_456_789_012_345,
        -987_654_321,
    ])
    .unwrap();

    let tx = conn.unchecked_transaction().expect("open tx");
    let deleted = delete_notes(&tx, &ids).expect("delete_notes must succeed");
    assert_eq!(
        deleted, 1,
        "only the one real matching NoteId (900) should be deleted; \
         out-of-range/negative ids must bind harmlessly, never SQL-inject"
    );

    // Every other Note must survive untouched.
    let other_notes_remaining: i64 = tx
        .query_row("SELECT COUNT(*) FROM Note WHERE NoteId != 900", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        other_notes_remaining > 0,
        "unrelated Note rows must be untouched by the parameterized delete"
    );

    tx.rollback().unwrap();
}

/// SAFE-04: a forced failure mid-transaction (after `delete_notes` runs but
/// before commit) leaves the archive's Note table unchanged — the
/// transaction rolls back on drop, mirroring
/// `test_upgrade_rollback_leaves_original_version`.
#[test]
fn test_delete_rollback_on_forced_failure() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let conn = Connection::open(&db_path).expect("open extracted db");
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();

    let before_notes = {
        let ro = Connection::open(&db_path).expect("open readonly check conn");
        common::normalized_table_rows(&ro, "Note")
    };

    let ids = NonEmptyNoteIds::try_from(vec![900_i64]).unwrap();
    let result: Result<(), rusqlite::Error> = (|| {
        let tx = conn.unchecked_transaction()?;
        delete_notes(&tx, &ids).map_err(|_| {
            rusqlite::Error::InvalidQuery // unreachable; delete_notes succeeds here
        })?;
        // Force a failure: attempt a statement that must error (unknown
        // column), so the transaction is dropped without a commit.
        tx.execute("SELECT ForcedFailureColumn FROM Note", [])?;
        tx.commit()?;
        Ok(())
    })();
    assert!(result.is_err(), "the forced failure must surface as an Err");
    drop(conn);

    let after_conn = Connection::open(&db_path).expect("reopen db after forced failure");
    let after_notes = common::normalized_table_rows(&after_conn, "Note");
    assert_eq!(
        before_notes, after_notes,
        "Note table must be unchanged after a mid-transaction failure (rollback)"
    );
}
