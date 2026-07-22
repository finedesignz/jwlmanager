//! 05-01 Task 3: real-DLL jwlCore `mergeDatabase` FFI integration test.
//!
//! Drives `jwlmanager_lib::jwlcore::merge::run_merge_with_lib_path` against the
//! VENDORED jwlCore binary (`libs/jwlCore-amd64.dll` on this host + its
//! co-located `libs/sqlite3_64.dll`, resolved via the loader's Windows
//! PATH-prepend — no root-staging is needed for the Rust leg, D5-10) over two
//! synthetic v16 fixtures built by `common::generate_merge_pair`.
//!
//! SKIP-AS-PASS off-host: mirrors `loader.rs`'s
//! `jwlcore_status_real_load_current_host` — if the current `(OS, ARCH)` has no
//! shipped binary (e.g. aarch64-windows), `host_dev_lib_path()` returns `None`
//! and the test returns early (a pass, never a silent false pass — the reason
//! is printed).
//!
//! After a successful merge the DESTINATION db must contain every SOURCE record
//! identity (by Guid / UserMarkGuid / (Type,Name) / Label — never by physical
//! PK, which jwlCore may remap), carry NO duplicate primary keys, and pass
//! `PRAGMA foreign_key_check` with zero rows.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use common::{
    generate_merge_pair, MERGE_DEST_ONLY_NOTE_GUID, MERGE_SHARED_NOTE_GUID, MERGE_SHARED_TAG_NAME,
    MERGE_SHARED_USERMARK_GUID, MERGE_SRC_ONLY_NOTE_GUID, MERGE_SRC_ONLY_TAG_NAME,
    MERGE_SRC_ONLY_USERMARK_GUID,
};
use jwlmanager_lib::jwlcore::merge::{host_dev_lib_path, run_merge_with_lib_path};
use rusqlite::Connection;
use std::collections::BTreeSet;

/// Reads a single TEXT column from a table into a sorted set.
fn text_set(conn: &Connection, table: &str, column: &str) -> BTreeSet<String> {
    let sql = format!("SELECT {column} FROM {table} WHERE {column} IS NOT NULL");
    let mut stmt = conn.prepare(&sql).expect("prepare text_set scan");
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query text_set");
    rows.map(|r| r.expect("read text_set row")).collect()
}

/// Reads the set of `Type|Name` Tag identities.
fn tag_identities(conn: &Connection) -> BTreeSet<String> {
    let mut stmt = conn
        .prepare("SELECT Type, Name FROM Tag")
        .expect("prepare tag scan");
    let rows = stmt
        .query_map([], |r| {
            Ok(format!(
                "{}|{}",
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?
            ))
        })
        .expect("query tags");
    rows.map(|r| r.expect("read tag row")).collect()
}

/// Asserts `COUNT(*) == COUNT(DISTINCT pk)` for a single-PK table.
fn assert_no_dup_pk(conn: &Connection, table: &str, pk: &str) {
    let total: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count total");
    let distinct: i64 = conn
        .query_row(
            &format!("SELECT COUNT(DISTINCT {pk}) FROM {table}"),
            [],
            |r| r.get(0),
        )
        .expect("count distinct pk");
    assert_eq!(
        total, distinct,
        "table {table} has duplicate {pk}: {total} rows but {distinct} distinct keys"
    );
}

/// Counts rows where a TEXT column equals `value`.
fn count_where_text(conn: &Connection, table: &str, column: &str, value: &str) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE {column} = ?1"),
        [value],
        |r| r.get(0),
    )
    .expect("count_where_text")
}

#[test]
fn merge_databases_ffi_merges_synthetic_pair() {
    // Skip-as-pass off-host: no shipped binary for this (OS, ARCH).
    let Some(lib_path) = host_dev_lib_path() else {
        eprintln!(
            "no jwlCore binary for this (OS, ARCH) — skipping the real-DLL merge FFI test \
             (would only fire on e.g. an aarch64-windows runner)."
        );
        return;
    };
    assert!(
        lib_path.exists(),
        "expected vendored jwlCore binary at {lib_path:?} for this host"
    );

    let ((dest_dir, dest_db), (_src_dir, src_db)) = generate_merge_pair();

    // Capture SOURCE identities before the merge, from the untouched source db.
    let (src_notes, src_usermarks, src_tags) = {
        let src = Connection::open(&src_db).expect("open source db for identity capture");
        (
            text_set(&src, "Note", "Guid"),
            text_set(&src, "UserMark", "UserMarkGuid"),
            tag_identities(&src),
        )
    };

    // Materialize the two-directory layout jwlCore wants (D5-03): the dest root
    // already holds `userData.db`; the source db goes under `<dest_root>/merge`.
    let merge_dir = dest_dir.path().join("merge");
    std::fs::create_dir_all(&merge_dir).expect("create <dest_root>/merge");
    std::fs::copy(&src_db, merge_dir.join("userData.db")).expect("stage source userData.db");

    // The real FFI call into the vendored jwlCore merge engine.
    run_merge_with_lib_path(lib_path.as_path(), dest_dir.path(), &merge_dir, false)
        .expect("real jwlCore mergeDatabase must succeed on the synthetic pair");

    // Assert against the (merged, in place) destination db.
    let dest = Connection::open(&dest_db).expect("reopen merged destination db");

    // (1) Every SOURCE record identity is present in the destination.
    let dest_notes = text_set(&dest, "Note", "Guid");
    for g in &src_notes {
        assert!(
            dest_notes.contains(g),
            "merged dest missing source Note.Guid {g}"
        );
    }
    let dest_usermarks = text_set(&dest, "UserMark", "UserMarkGuid");
    for g in &src_usermarks {
        assert!(
            dest_usermarks.contains(g),
            "merged dest missing source UserMark.UserMarkGuid {g}"
        );
    }
    let dest_tags = tag_identities(&dest);
    for t in &src_tags {
        assert!(dest_tags.contains(t), "merged dest missing source Tag {t}");
    }

    // Named-identity spot checks: the disjoint source records arrived AND the
    // pre-existing dest-only record survived.
    assert!(
        dest_notes.contains(MERGE_SRC_ONLY_NOTE_GUID),
        "source-only note not merged in"
    );
    assert!(
        dest_notes.contains(MERGE_DEST_ONLY_NOTE_GUID),
        "dest-only note lost"
    );
    assert!(
        dest_usermarks.contains(MERGE_SRC_ONLY_USERMARK_GUID),
        "source-only usermark not merged in"
    );
    assert!(
        dest_tags.contains(&format!("1|{MERGE_SRC_ONLY_TAG_NAME}")),
        "source-only tag not merged in"
    );

    // (2) Overlapping identities were NOT duplicated (present exactly once).
    assert_eq!(
        count_where_text(&dest, "Note", "Guid", MERGE_SHARED_NOTE_GUID),
        1,
        "shared Note.Guid duplicated by merge"
    );
    assert_eq!(
        count_where_text(
            &dest,
            "UserMark",
            "UserMarkGuid",
            MERGE_SHARED_USERMARK_GUID
        ),
        1,
        "shared UserMark duplicated by merge"
    );
    assert_eq!(
        count_where_text(&dest, "Tag", "Name", MERGE_SHARED_TAG_NAME),
        1,
        "shared Tag duplicated by merge"
    );

    // (3) No duplicate primary keys in any merge-affected single-PK table.
    for (table, pk) in [
        ("Location", "LocationId"),
        ("Note", "NoteId"),
        ("UserMark", "UserMarkId"),
        ("BlockRange", "BlockRangeId"),
        ("Bookmark", "BookmarkId"),
        ("Tag", "TagId"),
        ("TagMap", "TagMapId"),
    ] {
        assert_no_dup_pk(&dest, table, pk);
    }

    // (4) Referential integrity holds across the merged database.
    assert_foreign_keys_clean(&dest);
}

/// Fails if `PRAGMA foreign_key_check` returns any row.
fn assert_foreign_keys_clean(conn: &Connection) {
    let mut stmt = conn
        .prepare("PRAGMA foreign_key_check")
        .expect("prepare foreign_key_check");
    let violations: Vec<String> = stmt
        .query_map([], |r| {
            Ok(format!(
                "table={:?} rowid={:?} parent={:?} fkid={:?}",
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?
            ))
        })
        .expect("run foreign_key_check")
        .map(|r| r.expect("read fk violation row"))
        .collect();
    assert!(
        violations.is_empty(),
        "merged database has referential-integrity violations: {violations:?}"
    );
}
