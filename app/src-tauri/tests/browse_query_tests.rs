//! Per-category browse-query coverage (06-02, DATA-02..06). Proves each of the
//! five new getters returns its seeded fixture row AND — the load-bearing
//! assertion — surfaces the CORRECT identity PK per FUNCTIONALITY-SPEC §3.3
//! (Bookmarks=BookmarkId, Favorites=TagMapId, Highlights=BlockRangeId,
//! Annotations=LocationId, Playlists=PlaylistItemId), each chosen DISTINCT from
//! the row's LocationId so an identity/join mix-up FAILS loudly (a wrong key
//! browses fine but silently mis-targets every Phase 7 mutation).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::db::browse::{
    query_annotations, query_bookmarks, query_favorites, query_highlights, query_playlists,
};
use jwlmanager_lib::db::resources::{dev_resources_db_path, ResourceCatalog};
use rusqlite::Connection;

/// Opens the shared all-categories fixture and returns the live `userData.db`
/// connection + the English `ResourceCatalog`, keeping the two tempdirs alive
/// for the caller's assertions.
fn open_fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Connection,
    ResourceCatalog,
) {
    let (archive_dir, archive_path) = common::generate_v16_all_categories_fixture();
    let (extract_dir, extracted_path) = common::extract_to_tempdir(&archive_path);
    let conn = Connection::open(extracted_path.join("userData.db")).expect("db must open");
    let catalog =
        ResourceCatalog::load(&dev_resources_db_path(), "en").expect("resources.db must load");
    (archive_dir, extract_dir, conn, catalog)
}

#[test]
fn annotations_query() {
    let (_a, _e, conn, catalog) = open_fixture();
    let rows = query_annotations(&conn, &catalog).expect("query_annotations must succeed");

    assert_eq!(rows.len(), 1, "exactly one seeded annotation");
    let row = &rows[0];
    // Annotation identity IS LocationId (500) per FUNCTIONALITY-SPEC §3.3.
    assert_eq!(row.id, 500, "annotation identity is LocationId");
    // Labels synthesized via resources.db, not raw IDs.
    assert_eq!(row.language.as_deref(), Some("English"));
    assert_eq!(row.detail1.as_deref(), Some("01: Genesis"));
    assert_eq!(row.symbol, "nwt");
    // Annotations carry no color/tags/modified.
    assert_eq!(row.color, None);
    assert_eq!(row.tags, None);
    assert_eq!(row.modified, None);
}

#[test]
fn bookmarks_query() {
    let (_a, _e, conn, catalog) = open_fixture();
    let rows = query_bookmarks(&conn, &catalog).expect("query_bookmarks must succeed");

    assert_eq!(rows.len(), 1, "exactly one seeded bookmark");
    let row = &rows[0];
    // LOAD-BEARING: identity is BookmarkId (611), NOT the join's LocationId (500).
    assert_eq!(
        row.id, 611,
        "bookmark identity must be BookmarkId, never LocationId"
    );
    assert_ne!(row.id, 500, "a LocationId leak (500) must fail loudly");
    assert_eq!(row.language.as_deref(), Some("English"));
    assert_eq!(row.detail1.as_deref(), Some("01: Genesis"));
    assert_eq!(row.color, None);
}

#[test]
fn favorites_query() {
    let (_a, _e, conn, catalog) = open_fixture();
    let rows = query_favorites(&conn, &catalog).expect("query_favorites must succeed");

    // Exactly the NULL-NoteId favorite; the note-tag mapping (TagMapId 623,
    // NoteId 700) is EXCLUDED.
    assert_eq!(rows.len(), 1, "favorites must exclude the note-tag mapping");
    let row = &rows[0];
    // LOAD-BEARING: identity is TagMapId (622), NOT the join's LocationId (500).
    assert_eq!(
        row.id, 622,
        "favorite identity must be TagMapId, never LocationId"
    );
    assert_ne!(row.id, 500, "a LocationId leak (500) must fail loudly");
    assert_ne!(row.id, 623, "the excluded note-tag TagMap must not appear");
    assert_eq!(row.language.as_deref(), Some("English"));
    // Favorites SELECT no Book/Chapter -> no scripture detail2.
    assert_eq!(row.detail2, None);
}

#[test]
fn highlights_query() {
    let (_a, _e, conn, catalog) = open_fixture();
    let rows = query_highlights(&conn, &catalog).expect("query_highlights must succeed");

    // ONE row per BlockRange: the single UserMark 650 spans TWO BlockRanges.
    assert_eq!(rows.len(), 2, "one row per BlockRange, not per UserMark");
    let mut ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    ids.sort_unstable();
    // LOAD-BEARING: identities are the two distinct BlockRangeIds (633, 644),
    // NOT the shared LocationId (500) or UserMarkId (650).
    assert_eq!(
        ids,
        vec![633, 644],
        "highlight identities must be the distinct BlockRangeIds"
    );
    for row in &rows {
        assert_ne!(row.id, 500, "a LocationId leak (500) must fail loudly");
        assert_ne!(row.id, 650, "a UserMarkId leak (650) must fail loudly");
        // Highlights is the only new category carrying a color (ColorIndex 2).
        assert_eq!(row.color.as_deref(), Some("Green"));
        assert_eq!(row.language.as_deref(), Some("English"));
        assert_eq!(row.detail1.as_deref(), Some("01: Genesis"));
    }
}

#[test]
fn playlists_query() {
    let (_a, _e, conn, catalog) = open_fixture();
    let rows = query_playlists(&conn, &catalog).expect("query_playlists must succeed");

    assert_eq!(rows.len(), 1, "exactly one seeded playlist item");
    let row = &rows[0];
    // LOAD-BEARING: identity is PlaylistItemId (5000).
    assert_eq!(row.id, 5000, "playlist identity must be PlaylistItemId");
    // Playlists synthesize labels with NO resources lookup: language is None,
    // symbol/short/full are the "* OTHER *" sentinel, type is "Other".
    assert_eq!(row.language, None);
    assert_eq!(row.symbol, "* OTHER *");
    assert_eq!(row.short, "* OTHER *");
    assert_eq!(row.full, "* OTHER *");
    assert_eq!(row.type_group, "Other");
    // Label comes from Tag.Name / PlaylistItem.Label, not resources.db.
    assert_eq!(row.tags.as_deref(), Some("Fixture Playlist"));
    assert_eq!(row.detail1.as_deref(), Some("Fixture Song Label"));
}
