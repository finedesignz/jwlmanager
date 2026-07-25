//! EDIT-05 favorites coverage (07-01-PLAN.md). Task 1 covers unmark — the
//! TRACER slice proving the whole edit-op safety envelope (typed non-empty
//! selection, `apply_*` inside the caller's transaction, rolled-back
//! `dry_run_*` under `PragmaGuard`, semantic `DryRunReport`) end to end on
//! one op group before Plans 02-05 build four more on the same `db::edit`
//! spine. Task 2 (this same file) adds mark coverage.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use jwlmanager_lib::db::favorites::{
    apply_favorite_add, apply_favorite_remove, dry_run_favorite_add, dry_run_favorite_remove,
    FavoriteEditionRef, NonEmptyTagMapIds,
};
use jwlmanager_lib::db::resources::{dev_resources_db_path, ResourceCatalog};
use jwlmanager_lib::error::ArchiveError;
use rusqlite::Connection;

/// The system `Favorite` tag's `TagId`. `res/blank` (the real v16
/// blank-archive seed every fixture is built from) ships this row
/// PRE-SEEDED (`Type=0, Name='Favorite'`, `TagId 1`) — `Tag` carries
/// `UNIQUE(Type, Name)`, so a test must never insert a second one.
fn favorite_tag_id(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite'",
        [],
        |r| r.get(0),
    )
    .expect("res/blank must ship the pre-seeded system Favorite tag")
}

/// Inserts one favorite instance: a scripture `Location` + a `TagMap` row
/// (`PlaylistItemId`/`NoteId` both `NULL`, per the TagMap one-of CHECK)
/// linking it to the pre-seeded system Favorite tag — the exact shape
/// `apply_favorite_add` (Task 2) also produces. `id` seeds
/// `LocationId`/`TagMapId` together so each call can pick a fresh,
/// non-colliding id.
fn seed_favorite(conn: &Connection, id: i64) {
    let tag_id = favorite_tag_id(conn);
    // Mirrors the `Position = ifnull(max(Position), -1) + 1` rule Task 2's
    // `apply_favorite_add` uses — `TagMap` carries `UNIQUE(TagId, Position)`,
    // so a second favorite sharing the system tag can't reuse `Position 0`.
    let position: i64 = conn
        .query_row(
            "SELECT IFNULL(MAX(Position), -1) + 1 FROM TagMap WHERE TagId = ?1",
            rusqlite::params![tag_id],
            |r| r.get(0),
        )
        .expect("compute next favorite Position");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (?1, NULL, NULL, NULL, NULL, 0, 'nwt', 0, 1, NULL, NULL, NULL)",
        rusqlite::params![id],
    )
    .expect("insert favorite Location");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (?1, NULL, ?1, NULL, ?2, ?3)",
        rusqlite::params![id, tag_id, position],
    )
    .expect("insert favorite TagMap");
}

/// A test asserting `dry_run_favorite_remove` leaves the TagMap row count
/// unchanged (SAFE-01) while still reporting the favorite as `deleted`.
#[test]
fn test_dry_run_favorite_remove_leaves_tagmap_count_unchanged() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open seeded db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        seed_favorite(&conn, 900);
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();

    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let report = dry_run_favorite_remove(&mut conn, &ids).expect("dry run must succeed");

    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        before, after,
        "dry-run must leave the working copy's TagMap row count unchanged"
    );
    assert_eq!(
        report.deleted.get("TagMap").copied().unwrap_or(0),
        1,
        "report must show the favorite's TagMap row as deleted: {:?}",
        report.deleted
    );
}

/// A test asserting `apply_favorite_remove` removes exactly the selected
/// TagMapIds — a sibling favorite must survive untouched.
#[test]
fn test_apply_favorite_remove_removes_exactly_selected_tagmapids() {
    let (_dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed_favorite(&conn, 900);
    seed_favorite(&conn, 901); // untouched sibling favorite

    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let tx = conn.unchecked_transaction().expect("open tx");
    let removed = apply_favorite_remove(&tx, &ids).expect("apply must succeed");
    assert_eq!(removed, 1, "exactly one TagMap row must be removed");

    let tagmap_900: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE TagMapId = 900",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tagmap_900, 0, "TagMap 900 must be gone");

    let tagmap_901: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM TagMap WHERE TagMapId = 901",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        tagmap_901, 1,
        "sibling favorite TagMap 901 must survive untouched"
    );

    tx.rollback().unwrap();
}

/// Rule #16: the system `Tag (Type=0, Name='Favorite')` row is never GC'd,
/// even when an unmark leaves it with zero remaining TagMap rows —
/// `trim_sweep`'s `unused_tag` predicate is scoped to `Type > 0`.
#[test]
fn test_favorite_tag_never_gc_d_after_unmark() {
    let (_dir, db_path) = common::fresh_v16_db();
    let tag_id = {
        let conn = Connection::open(&db_path).expect("open seeded db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        seed_favorite(&conn, 900);
        favorite_tag_id(&conn)
    };

    let mut conn = Connection::open(&db_path).expect("reopen db");
    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let report = dry_run_favorite_remove(&mut conn, &ids).expect("dry run must succeed");

    // The dry-run's internal `trim_sweep` already ran (inside its rolled-back
    // transaction) — if the system tag were wrongly GC'd, it would show up
    // here as `deleted["Tag"] == 1`.
    assert_eq!(
        report.deleted.get("Tag").copied().unwrap_or(0),
        0,
        "system Favorite tag (Type=0) must survive trim_sweep even though this \
         was its only TagMap reference — rule #16: {:?}",
        report.deleted
    );

    // Confirm against a real apply too (apply alone never calls trim_sweep —
    // that only happens on save — so this just reconfirms the tag is never
    // touched by the mutation itself).
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_favorite_remove(&tx, &ids).expect("apply must succeed");
    let tag_survives: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM Tag WHERE TagId = ?1",
            rusqlite::params![tag_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        tag_survives, 1,
        "the system Favorite tag must survive the real apply"
    );
    tx.rollback().unwrap();
}

/// A test asserts a `Vec<i64>` of length 0 fails `TryFrom` for
/// `NonEmptyTagMapIds` — an empty selection is unrepresentable at IPC
/// deserialization, before any command body or DB access runs.
#[test]
fn test_empty_selection_fails_deserialization() {
    let empty: Result<NonEmptyTagMapIds, _> = serde_json::from_str("[]");
    assert!(empty.is_err(), "empty selection must fail to deserialize");

    let non_empty: Result<NonEmptyTagMapIds, _> = serde_json::from_str("[42]");
    assert!(non_empty.is_ok(), "non-empty selection must deserialize");
}

/// Dry-run must leave the connection's PRAGMA state restored, matching the
/// `PragmaGuard` contract already proven for `dry_run_delete_notes`.
#[test]
fn test_dry_run_favorite_remove_restores_pragmas() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open seeded db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        seed_favorite(&conn, 900);
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    let fk_before: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();

    let ids = NonEmptyTagMapIds::try_from(vec![900_i64]).unwrap();
    let _report = dry_run_favorite_remove(&mut conn, &ids).expect("dry run must succeed");

    let fk_after: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        fk_before, fk_after,
        "dry-run must restore the connection's prior foreign_keys pragma"
    );
}

// --- Task 2: Favorites mark (apply_favorite_add / dry_run_favorite_add) ---

/// Adding a favorite to an archive lacking the system tag creates exactly
/// one `Tag` row with `Type = 0` and `Name = 'Favorite'`.
#[test]
fn test_apply_favorite_add_creates_system_tag_when_absent() {
    let (_dir, db_path) = common::fresh_v16_db_without_favorite_tag();
    let conn = Connection::open(&db_path).expect("open db without system tag");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");

    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM Tag WHERE Type = 0", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, 0, "fixture must genuinely lack the system tag");

    let edition = FavoriteEditionRef {
        symbol: "nwt".to_string(),
        language: 0,
    };
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_favorite_add(&tx, &edition).expect("apply must succeed");

    let after: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM Tag WHERE Type = 0 AND Name = 'Favorite'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        after, 1,
        "exactly one system Favorite tag must exist after apply"
    );
    tx.rollback().unwrap();
}

/// A second add of the same (edition, language) returns
/// `ArchiveError::FavoriteDuplicate` and performs zero INSERTs — `SELECT
/// COUNT(*) FROM TagMap` is unchanged.
#[test]
fn test_apply_favorite_add_duplicate_returns_error_and_tagmap_unchanged() {
    let (_dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");

    let edition = FavoriteEditionRef {
        symbol: "nwt".to_string(),
        language: 0,
    };
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_favorite_add(&tx, &edition).expect("first apply must succeed");

    let count_after_first: i64 = tx
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();

    let result = apply_favorite_add(&tx, &edition);
    assert!(
        matches!(result, Err(ArchiveError::FavoriteDuplicate { .. })),
        "second add of the same edition must return FavoriteDuplicate, got: {result:?}"
    );

    let count_after_second: i64 = tx
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        count_after_first, count_after_second,
        "a rejected duplicate must perform zero INSERTs into TagMap"
    );
    tx.rollback().unwrap();
}

/// The inserted `TagMap.Position` equals the prior max Position for that tag
/// plus one, not a hardcoded 0 — a second favorite under the SAME system tag
/// must not collide with an already-seeded favorite's Position.
#[test]
fn test_apply_favorite_add_position_is_prior_max_plus_one() {
    let (_dir, db_path) = common::fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    // Pre-existing favorite at LocationId/TagMapId 900, Position 0 (first
    // under the tag).
    seed_favorite(&conn, 900);

    // A DIFFERENT edition (distinct KeySymbol) so this is a genuinely new
    // Location, not a duplicate of the seeded one.
    let edition = FavoriteEditionRef {
        symbol: "nwtsty".to_string(),
        language: 0,
    };
    let tx = conn.unchecked_transaction().expect("open tx");
    apply_favorite_add(&tx, &edition).expect("apply must succeed");

    let new_position: i64 = tx
        .query_row(
            "SELECT tm.Position FROM TagMap tm \
             JOIN Location loc ON loc.LocationId = tm.LocationId \
             WHERE loc.KeySymbol = 'nwtsty' AND loc.MepsLanguage = 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        new_position, 1,
        "new favorite's Position must be the prior max (0) plus one"
    );
    tx.rollback().unwrap();
}

/// `load_favorite_editions` returns a non-empty vec for a seeded (real,
/// bundled) language and an empty vec for an unknown one — exercised
/// through the `favorites_tests` binary specifically, since the plan's
/// `<verify>` command targets this integration test, not the lib's own
/// unit tests.
#[test]
fn test_load_favorite_editions_seeded_vs_unknown_language() {
    let catalog = ResourceCatalog::load(&dev_resources_db_path(), "en")
        .expect("resources.db must load for the English UI language");

    let english = catalog.load_favorite_editions("English");
    assert!(
        !english.is_empty(),
        "English must have at least one favorite-eligible edition"
    );

    let unknown = catalog.load_favorite_editions("Not A Real Language Name");
    assert!(
        unknown.is_empty(),
        "an unknown language must return an empty vec, not an error"
    );
}

/// `dry_run_favorite_add` reports `added: {Location: 1, TagMap: 1}` (the
/// system tag already exists via `res/blank`'s pre-seed, so `Tag` shows no
/// diff) and leaves the DB unchanged.
#[test]
fn test_dry_run_favorite_add_reports_location_and_tagmap_added_leaves_db_unchanged() {
    let (_dir, db_path) = common::fresh_v16_db();
    let mut conn = Connection::open(&db_path).expect("open seeded db");

    let location_count_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM Location", [], |r| r.get(0))
        .unwrap();
    let tagmap_count_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();

    let edition = FavoriteEditionRef {
        symbol: "nwt".to_string(),
        language: 0,
    };
    let report = dry_run_favorite_add(&mut conn, &edition).expect("dry run must succeed");

    assert_eq!(
        report.added.get("Location").copied().unwrap_or(0),
        1,
        "report must show one Location added: {:?}",
        report.added
    );
    assert_eq!(
        report.added.get("TagMap").copied().unwrap_or(0),
        1,
        "report must show one TagMap added: {:?}",
        report.added
    );
    assert_eq!(
        report.added.get("Tag").copied().unwrap_or(0),
        0,
        "system tag already exists via res/blank's pre-seed, so Tag must show \
         no diff: {:?}",
        report.added
    );

    let location_count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM Location", [], |r| r.get(0))
        .unwrap();
    let tagmap_count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM TagMap", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        location_count_before, location_count_after,
        "dry-run must leave the working copy's Location row count unchanged"
    );
    assert_eq!(
        tagmap_count_before, tagmap_count_after,
        "dry-run must leave the working copy's TagMap row count unchanged"
    );
}

/// Against a fixture genuinely lacking the system tag, `dry_run_favorite_add`
/// ALSO reports `Tag: 1` added — and still leaves the DB unchanged (the
/// tag-creation itself rolls back too).
#[test]
fn test_dry_run_favorite_add_without_system_tag_reports_tag_added_and_leaves_db_unchanged() {
    let (_dir, db_path) = common::fresh_v16_db_without_favorite_tag();
    let mut conn = Connection::open(&db_path).expect("open db without system tag");

    let tag_count_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM Tag", [], |r| r.get(0))
        .unwrap();

    let edition = FavoriteEditionRef {
        symbol: "nwt".to_string(),
        language: 0,
    };
    let report = dry_run_favorite_add(&mut conn, &edition).expect("dry run must succeed");

    assert_eq!(
        report.added.get("Tag").copied().unwrap_or(0),
        1,
        "report must show the system tag as newly added: {:?}",
        report.added
    );

    let tag_count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM Tag", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        tag_count_before, tag_count_after,
        "dry-run must leave the working copy's Tag row count unchanged, even \
         though the tag was staged for creation inside the rolled-back \
         transaction"
    );
}

/// A duplicate favorite surfaces as `Err(ArchiveError::FavoriteDuplicate)`
/// from `dry_run_favorite_add` too, before any row is staged — mirroring
/// what `apply_favorite_add` does directly.
#[test]
fn test_dry_run_favorite_add_duplicate_returns_error() {
    let (_dir, db_path) = common::fresh_v16_db();
    {
        let conn = Connection::open(&db_path).expect("open seeded db");
        conn.execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        seed_favorite(&conn, 900); // KeySymbol='nwt', MepsLanguage=0
    }

    let mut conn = Connection::open(&db_path).expect("reopen db");
    let edition = FavoriteEditionRef {
        symbol: "nwt".to_string(),
        language: 0,
    };
    let result = dry_run_favorite_add(&mut conn, &edition);
    assert!(
        matches!(result, Err(ArchiveError::FavoriteDuplicate { .. })),
        "dry-run of a duplicate favorite must return FavoriteDuplicate, got: {result:?}"
    );
}
