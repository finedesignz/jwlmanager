//! `trim_db` orphan sweep + tag re-densify + VACUUM test matrix
//! (ARCH-04, SAFE-02, 02-01-PLAN.md). This is the data-integrity core of
//! Phase 2 — every behavior here maps to a threat register entry in
//! 02-01-PLAN.md's `<threat_model>`.
//!
//! Wave 0 (`test_tagmap_column_order_matches_redensify`,
//! `test_bundled_sqlite_supports_window_functions`) gates the rest of this
//! matrix: if either fails, the `trim_sweep` implementation's assumptions
//! about the schema/engine are wrong and every downstream test result is
//! suspect.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::archive::save::save_archive;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::db::trim::{trim_db, trim_sweep};
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Wave 0 — gate tests
// ---------------------------------------------------------------------------

#[test]
fn test_tagmap_column_order_matches_redensify() {
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted.join("userData.db")).expect("open extracted db");

    let mut stmt = conn.prepare("PRAGMA table_info(TagMap)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();

    assert_eq!(
        cols,
        vec![
            "TagMapId",
            "PlaylistItemId",
            "LocationId",
            "NoteId",
            "TagId",
            "Position"
        ],
        "TagMap column order must match the explicit column list the re-densify INSERT uses"
    );
}

#[test]
fn test_bundled_sqlite_supports_window_functions() {
    let conn = Connection::open_in_memory().expect("open in-memory connection");
    let result: Result<i64, _> =
        conn.query_row("SELECT ROW_NUMBER() OVER (ORDER BY 1)", [], |r| r.get(0));
    assert!(
        result.is_ok(),
        "bundled SQLite must support ROW_NUMBER() OVER: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), 1);
}

#[test]
fn test_trim_fixture_produces_expected_orphan_graph() {
    // Sanity check on the fixture itself, independent of trim: confirms the
    // multi-table orphan graph, survivor, and gapped positions exist exactly
    // as documented before any Note is deleted.
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted.join("userData.db")).expect("open extracted db");

    let positions: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT Position FROM TagMap WHERE TagId = 901 ORDER BY Position")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert_eq!(
        positions,
        vec![5, 9, 20],
        "fixture must seed gapped positions"
    );

    let bookmark_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Bookmark WHERE LocationId = 901",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bookmark_count, 1, "fixture must seed the survivor Bookmark");
}

// ---------------------------------------------------------------------------
// Task 2 — trim_sweep / trim_db mechanics
// ---------------------------------------------------------------------------

/// Deletes Note 900 (the orphan-producing Note from `generate_trim_fixture`)
/// on the given connection, matching D2-05: delete removes ONLY the Note
/// row itself; every resulting orphan is left for `trim_sweep`.
///
/// Forces `foreign_keys = OFF` first, exactly as the real delete op does
/// (`JWLManager.py:3681` sets `PRAGMA foreign_keys='OFF'` before the delete):
/// deleting a Note still referenced by an orphan-to-be UserMark/TagMap would
/// otherwise trip the FK constraint that is ON by default in this bundled
/// SQLite (the Phase 3 finding).
fn delete_note_900(conn: &Connection) {
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("disable foreign_keys for delete");
    conn.execute("DELETE FROM Note WHERE NoteId = 900", [])
        .expect("delete Note 900");
}

/// Deletes Note 901 (the survivor-Location-owning Note) on the given
/// connection. Forces `foreign_keys = OFF` first (see [`delete_note_900`]).
fn delete_note_901(conn: &Connection) {
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("disable foreign_keys for delete");
    conn.execute("DELETE FROM Note WHERE NoteId = 901", [])
        .expect("delete Note 901");
}

#[test]
fn test_trim_sweeps_orphans_and_vacuums() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let size_before = std::fs::metadata(&db_path).unwrap().len();

    {
        let conn = Connection::open(&db_path).expect("open db");
        delete_note_900(&conn);
        delete_note_901(&conn);
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    trim_db(&mut conn).expect("trim_db must succeed");

    let count = |c: &Connection, sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap() };

    // The deleted Note's TagMap entry is a true orphan → swept.
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM TagMap WHERE TagMapId = 900"),
        0,
        "orphan TagMap 900 (its Note was deleted) must be swept"
    );

    // ANTI-OVER-DELETE (codex finding 1): the highlight the deleted Note
    // anchored is durable and MUST survive — deleting a Note never deletes
    // its highlight's UserMark/BlockRange/Location.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM UserMark WHERE UserMarkId = 900"
        ),
        1,
        "UserMark 900 is a durable highlight and must SURVIVE the note deletion"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM BlockRange WHERE BlockRangeId = 900"
        ),
        1,
        "BlockRange 900 must survive with its UserMark"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Location WHERE LocationId = 900"
        ),
        1,
        "Location 900 is still referenced by the surviving highlight → must survive"
    );

    // GENUINE pre-existing orphans → swept (incl. via the NOT-EXISTS rewrite).
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM UserMark WHERE UserMarkId = 951"
        ),
        0,
        "genuine-orphan UserMark 951 (no BlockRange, no Note) must be swept"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM BlockRange WHERE BlockRangeId = 952"
        ),
        0,
        "dangling BlockRange 952 (UserMark 999 absent) must be swept"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Location WHERE LocationId = 950"
        ),
        0,
        "genuine-orphan Location 950 (referenced by nothing after UserMark 951 swept) \
         must be swept via the NOT-EXISTS-rewritten predicate"
    );

    // SURVIVING standalone highlight (no deleted note references it) → kept.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM UserMark WHERE UserMarkId = 890"
        ),
        1,
        "standalone survivor highlight UserMark 890 must NOT be swept"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM Location WHERE LocationId = 890"
        ),
        1,
        "survivor-highlight Location 890 must NOT be swept"
    );

    let null_titles: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE Title IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        null_titles, 0,
        "Location.Title must never be NULL after trim"
    );

    drop(conn);
    let size_after = std::fs::metadata(&db_path).unwrap().len();
    assert!(
        size_after <= size_before,
        "trim_db must reclaim space via VACUUM: before={size_before} after={size_after}"
    );
}

#[test]
fn test_trim_location_survivor_referenced_by_bookmark() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        delete_note_901(&conn);
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    trim_db(&mut conn).expect("trim_db must succeed");

    let location_901: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE LocationId = 901",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        location_901, 1,
        "Location 901 must survive trim — it is still referenced by a Bookmark"
    );
}

#[test]
fn test_trim_reindexes_tag_positions() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let mut conn = Connection::open(&db_path).expect("open db");
    trim_db(&mut conn).expect("trim_db must succeed");

    let rows: Vec<(i64, i64)> = {
        let mut stmt = conn
            .prepare("SELECT NoteId, Position FROM TagMap WHERE TagId = 901 ORDER BY Position")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };

    assert_eq!(
        rows,
        vec![(902, 0), (903, 1), (904, 2)],
        "gapped positions 5,9,20 must compact to contiguous 0,1,2, \
         preserving original NoteId/order"
    );
}

#[test]
fn test_trim_rollback_on_forced_failure() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let mut conn = Connection::open(&db_path).expect("open db");

    let before = common::normalized_table_rows(&conn, "TagMap");

    // A trigger that aborts on INSERT INTO TagMap so the failure lands
    // (permanent, NOT `TEMP`: `trim_db` sets `PRAGMA temp_store='MEMORY'`,
    // which SQLite documents as immediately deleting all temp triggers — a
    // TEMP trigger would silently vanish before the re-densify INSERT and the
    // forced failure would never fire)
    // AFTER the re-densify's destructive `DELETE FROM TagMap` (finding 5) —
    // proving delete-then-reinsert is fully recoverable via rollback.
    conn.execute_batch(
        "CREATE TRIGGER abort_tagmap_insert BEFORE INSERT ON TagMap \
         BEGIN SELECT RAISE(ABORT, 'forced trim failure'); END;",
    )
    .expect("install forced-failure trigger");

    let result = trim_db(&mut conn);
    match result {
        Err(ArchiveError::TrimFailed { .. }) => {}
        other => panic!("expected TrimFailed, got {other:?}"),
    }

    // Drop the trigger (it is TEMP and connection-local) before re-reading,
    // so the assertion query itself isn't affected — TagMap is only ever
    // read here, not inserted into.
    let after = common::normalized_table_rows(&conn, "TagMap");
    assert_eq!(
        before, after,
        "TagMap rows must be fully restored after a forced mid-redensify failure"
    );
}

#[test]
fn test_pragmas_restored_after_trim_success() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let mut conn = Connection::open(&db_path).expect("open db");
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = 'DELETE'; \
         PRAGMA synchronous = 2; PRAGMA temp_store = 0;",
    )
    .expect("set known baseline pragmas");

    let fk_before: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    let journal_before: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let sync_before: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    let temp_before: i64 = conn
        .query_row("PRAGMA temp_store", [], |r| r.get(0))
        .unwrap();

    trim_db(&mut conn).expect("trim_db must succeed");

    let fk_after: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    let journal_after: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let sync_after: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    let temp_after: i64 = conn
        .query_row("PRAGMA temp_store", [], |r| r.get(0))
        .unwrap();

    assert_eq!(fk_before, fk_after, "foreign_keys must be restored");
    assert_eq!(
        journal_before, journal_after,
        "journal_mode must be restored"
    );
    assert_eq!(sync_before, sync_after, "synchronous must be restored");
    assert_eq!(temp_before, temp_after, "temp_store must be restored");
}

#[test]
fn test_pragmas_restored_after_trim_failure() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let mut conn = Connection::open(&db_path).expect("open db");
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = 'DELETE'; \
         PRAGMA synchronous = 2; PRAGMA temp_store = 0;",
    )
    .expect("set known baseline pragmas");

    let fk_before: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    let journal_before: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let sync_before: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    let temp_before: i64 = conn
        .query_row("PRAGMA temp_store", [], |r| r.get(0))
        .unwrap();

    conn.execute_batch(
        "CREATE TRIGGER abort_tagmap_insert BEFORE INSERT ON TagMap \
         BEGIN SELECT RAISE(ABORT, 'forced trim failure'); END;",
    )
    .expect("install forced-failure trigger");

    let result = trim_db(&mut conn);
    assert!(
        result.is_err(),
        "trim_db must fail with the forced trigger installed"
    );

    let fk_after: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    let journal_after: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let sync_after: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    let temp_after: i64 = conn
        .query_row("PRAGMA temp_store", [], |r| r.get(0))
        .unwrap();

    assert_eq!(
        fk_before, fk_after,
        "foreign_keys must be restored after failure"
    );
    assert_eq!(
        journal_before, journal_after,
        "journal_mode must be restored after failure"
    );
    assert_eq!(
        sync_before, sync_after,
        "synchronous must be restored after failure"
    );
    assert_eq!(
        temp_before, temp_after,
        "temp_store must be restored after failure"
    );
}

#[test]
fn test_foreign_key_check_clean_after_trim() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        delete_note_900(&conn);
        delete_note_901(&conn);
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    trim_db(&mut conn).expect("trim_db must succeed");

    let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
    let violation_count = stmt.query_map([], |_| Ok(())).unwrap().count();
    assert_eq!(
        violation_count, 0,
        "PRAGMA foreign_key_check must report zero dangling references after trim"
    );
}

#[test]
fn test_bare_save_trim_is_destructive() {
    // Documentary test (finding 7): a bare trim (no explicit delete) still
    // removes empty untagged Notes, empty InputFields, and unused Tags,
    // matching the Python app's behavior. This is expected and DELIBERATE —
    // save-time-trim preview is a deferred idea, not built this phase.
    let (_dir, archive_path) = common::generate_v16_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, \
             LastModified, Created, BlockType, BlockIdentifier) VALUES \
             (9000, 'fixture-empty-note-guid', NULL, NULL, '', '', '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z', 0, NULL)",
            [],
        )
        .expect("insert empty untagged Note");
        conn.execute(
            "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
             IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
             VALUES (9001, NULL, NULL, 9099, NULL, 0, NULL, 0, 0, 'Bare Trim Location', NULL, NULL)",
            [],
        )
        .expect("insert Location for InputField");
        conn.execute(
            "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (9001, 'tag', '')",
            [],
        )
        .expect("insert empty InputField");
        conn.execute(
            "INSERT INTO Tag (TagId, Type, Name) VALUES (9002, 1, 'Unused Bare Trim Tag')",
            [],
        )
        .expect("insert unused Tag");
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    trim_db(&mut conn).expect("trim_db must succeed");

    let empty_note: i64 = conn
        .query_row("SELECT COUNT(*) FROM Note WHERE NoteId = 9000", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        empty_note, 0,
        "empty untagged Note must be swept by a bare trim"
    );

    let empty_input_field: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM InputField WHERE LocationId = 9001",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        empty_input_field, 0,
        "empty InputField must be swept by a bare trim"
    );

    let unused_tag: i64 = conn
        .query_row("SELECT COUNT(*) FROM Tag WHERE TagId = 9002", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(unused_tag, 0, "unused Tag must be swept by a bare trim");
}

// ---------------------------------------------------------------------------
// trim_sweep called directly inside a rolled-back transaction (proves it
// never VACUUMs and is safe to reuse from a future dry-run path)
// ---------------------------------------------------------------------------

#[test]
fn test_trim_sweep_alone_never_vacuums_and_is_rollback_safe() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (_zip_dir, extracted) = common::extract_to_tempdir(&archive_path);
    let db_path = extracted.join("userData.db");

    let conn = Connection::open(&db_path).expect("open db");
    delete_note_900(&conn);

    let before = common::normalized_table_rows(&conn, "Location");
    let tx = conn.unchecked_transaction().expect("open transaction");
    let counts = trim_sweep(&tx).expect("trim_sweep must succeed");
    assert!(
        counts.contains_key("unused_location"),
        "trim_sweep must report a count for the unused_location label"
    );
    drop(tx); // rolled back on drop, never committed

    let after = common::normalized_table_rows(&conn, "Location");
    assert_eq!(
        before, after,
        "an uncommitted trim_sweep transaction must leave Location unchanged on rollback"
    );
}

// ---------------------------------------------------------------------------
// Task 3 — save wiring (hash-last)
// ---------------------------------------------------------------------------

#[test]
fn test_save_trims_and_stays_python_acceptable() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    {
        let conn = Connection::open(&session.db_path).expect("open working db");
        delete_note_900(&conn);
        delete_note_901(&conn);
    }

    let manifest = save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save must succeed");
    assert_eq!(manifest.user_data_backup.schema_version, 16);

    let (reopened_session, _reopened_notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("reopen must succeed");
    let conn = Connection::open(&reopened_session.db_path).expect("open reopened db");

    // A genuine pre-existing orphan is swept on save.
    let orphan_usermark: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM UserMark WHERE UserMarkId = 951",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        orphan_usermark, 0,
        "saved archive must have the genuine-orphan UserMark 951 swept"
    );

    // The highlight the deleted Note 900 anchored is durable → survives save.
    let highlight_usermark: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM UserMark WHERE UserMarkId = 900",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        highlight_usermark, 1,
        "the deleted Note's durable highlight must survive the trimmed save"
    );

    let survivor_location: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM Location WHERE LocationId = 901",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        survivor_location, 1,
        "saved archive must keep the Bookmark-referenced Location"
    );

    let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
    let violation_count = stmt.query_map([], |_| Ok(())).unwrap().count();
    assert_eq!(
        violation_count, 0,
        "saved archive must pass foreign_key_check"
    );
}

/// Asserts a trimmed save is never larger than an equivalent untrimmed
/// working-copy DB of the same source fixture (VACUUM reclaims deleted-row
/// space). Compares the on-disk `userData.db` bytes directly (not the whole
/// zip, whose compression can vary run-to-run), before vs. after `trim_db`.
#[test]
fn test_save_trim_does_not_grow_db() {
    let (_dir, archive_path) = common::generate_trim_fixture();
    let (session, _notes) =
        open_and_validate(&archive_path, &dev_resources_db_path()).expect("must open");

    let size_before_delete = std::fs::metadata(&session.db_path).unwrap().len();

    {
        let conn = Connection::open(&session.db_path).expect("open working db");
        delete_note_900(&conn);
        delete_note_901(&conn);
    }

    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save must succeed");

    let size_after_save = std::fs::metadata(&session.db_path).unwrap().len();
    assert!(
        size_after_save <= size_before_delete,
        "a trimmed save must not grow the working-copy DB: before={size_before_delete} \
         after={size_after_save}"
    );
}

#[test]
#[ignore] // gated like tests/differential.rs — requires the Python interpreter + JWLManager.py
fn test_python_accepts_trimmed_save() {
    // Deferred to the differential oracle harness (mirrors
    // tests/differential.rs's #[ignore]-gated pattern): a trimmed-then-saved
    // archive must still pass Python `check_validity`. Left as a documented
    // placeholder per 02-01-PLAN.md's verification section; the harness
    // itself (subprocess + venv wiring) is out of scope for this plan.
}
