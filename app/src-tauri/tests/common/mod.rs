//! Shared test harness: temp-dir zip extraction, normalized semantic table
//! diffing, and synthetic fixture generators (QA-01, D-06, D-08).
//!
//! Never compare archives byte-for-byte (save is not byte-preserving, per
//! CLAUDE.md) — always normalize to sorted row-sets first.
//!
//! Every fixture generator here is SYNTHETIC and built at test-run time.
//! `res/blank` (the real JW Library v16 empty-archive seed, already
//! `PRAGMA user_version = 16`) is used only as the schema/seed SOURCE — its
//! `userData.db` bytes are copied into a tempdir and then mutated with
//! synthetic INSERTed rows. NOTHING generated here is ever written back into
//! the repo; it all lives under `tempfile::TempDir` and is deleted on drop.
//! GDPR Art. 9 bright line: never commit a real or scrubbed `.jwlibrary`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path
#![allow(dead_code)] // not every helper is used by every test binary that includes this module

use rusqlite::Connection;
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

// ---------------------------------------------------------------------------
// Extraction / diffing helpers
// ---------------------------------------------------------------------------

/// Extracts a `.jwlibrary` (zip) into a fresh temp directory and returns the
/// directory handle (kept alive by the caller) plus its path.
pub fn extract_to_tempdir(archive_path: &Path) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("failed to create tempdir for extraction");
    let file = File::open(archive_path).expect("failed to open archive for extraction");
    let mut zip = ZipArchive::new(file).expect("failed to read zip archive");
    zip.extract(dir.path()).expect("failed to extract archive");
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// Reads every entry name out of a zip archive without extracting, for
/// structural assertions (e.g. "contains manifest.json").
pub fn list_zip_entries(archive_path: &Path) -> Vec<String> {
    let file = File::open(archive_path).expect("failed to open archive");
    let mut zip = ZipArchive::new(file).expect("failed to read zip archive");
    (0..zip.len())
        .map(|i| {
            zip.by_index(i)
                .expect("failed to read zip entry")
                .name()
                .to_string()
        })
        .collect()
}

/// Reads a whole table into a normalized, sorted, semantic representation —
/// a `BTreeMap` of stringified-row -> occurrence count. Row ORDER never
/// matters because it's sorted into the map key. Byte-diffing the underlying
/// `.db` file is explicitly forbidden (Core Value: save is not
/// byte-preserving; only semantic equivalence is checked).
pub fn normalized_table_rows(conn: &Connection, table: &str) -> BTreeMap<String, usize> {
    let sql = format!("SELECT * FROM {table}");
    let mut stmt = conn.prepare(&sql).expect("failed to prepare table scan");
    let col_count = stmt.column_count();
    let mut rows = stmt
        .query_map([], |row| {
            let mut cells = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let value: rusqlite::types::Value = row.get(i)?;
                cells.push(format!("{value:?}"));
            }
            Ok(cells.join("|"))
        })
        .expect("failed to query table rows");

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows.by_ref() {
        let row = row.expect("failed to read row");
        *counts.entry(row).or_insert(0) += 1;
    }
    counts
}

/// Reads a whole file into memory. Test-only convenience wrapper.
pub fn read_file_bytes(path: &Path) -> Vec<u8> {
    let mut buf = Vec::new();
    File::open(path)
        .expect("failed to open file")
        .read_to_end(&mut buf)
        .expect("failed to read file");
    buf
}

// ---------------------------------------------------------------------------
// Synthetic v16 fixture generator (res/blank-seeded)
// ---------------------------------------------------------------------------

/// Repo-root-relative path to the real v16 empty-archive seed. Read-only
/// source for fixture generation — never modified, never re-zipped as-is.
fn res_blank_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../res/blank")
}

/// Extracts `res/blank`'s `userData.db` + `default_thumbnail.png` bytes into
/// the given directory. Both come from the real v16 seed archive (verified
/// `PRAGMA user_version = 16` in this plan's research pass).
fn seed_from_res_blank(dest_dir: &Path) {
    let file = fs::File::open(res_blank_path()).expect("res/blank must exist");
    let mut zip = zip::ZipArchive::new(file).expect("res/blank must be a valid zip");
    for name in ["userData.db", "default_thumbnail.png"] {
        let mut entry = zip
            .by_name(name)
            .unwrap_or_else(|_| panic!("res/blank missing expected entry {name}"));
        let mut buf = Vec::new();
        std::io::copy(&mut entry, &mut buf).expect("failed to read res/blank entry");
        fs::write(dest_dir.join(name), buf).expect("failed to write seeded file");
    }
}

/// Inserts the minimum synthetic rows needed for a meaningful Notes-list
/// fixture: one LOCATED note (has a Location + UserMark) and one INDEPENDENT
/// note (`LocationId IS NULL`, `BlockType = 0`) — Pitfall 2 in RESEARCH.md:
/// the independent-notes query is a separate UNION and must have coverage.
fn insert_synthetic_notes(db_path: &Path) {
    let conn = Connection::open(db_path).expect("failed to open seeded userData.db");

    // A located note: Location -> UserMark -> Note.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (1, 1, 1, NULL, NULL, 0, 'nwt', 0, 0, 'Genesis 1:1', 0, NULL)",
        [],
    )
    .expect("insert Location");

    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (1, 1, 1, 0, 'fixture-usermark-guid-0001', 1)",
        [],
    )
    .expect("insert UserMark");

    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (1, 'fixture-note-guid-0001', 1, 1, 'Located note title', \
         'Located note content for fixture testing.', '2026-01-01T00:00:00Z', \
         '2026-01-01T00:00:00Z', 2, 1)",
        [],
    )
    .expect("insert located Note");

    // An independent note: no Location, no UserMark, BlockType 0.
    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (2, 'fixture-note-guid-0002', NULL, NULL, 'Independent note title', \
         'Independent note content for fixture testing.', '2026-01-01T00:00:00Z', \
         '2026-01-01T00:00:00Z', 0, NULL)",
        [],
    )
    .expect("insert independent Note");

    // A tag + tagmap so the Notes label-resolution path has something to join.
    // IDs start at 100 — res/blank already seeds TagId=1 ("Favorite").
    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (100, 1, 'Fixture Tag')",
        [],
    )
    .expect("insert Tag");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (100, NULL, NULL, 1, 100, 0)",
        [],
    )
    .expect("insert TagMap");

    conn.execute(
        "UPDATE LastModified SET LastModified = '2026-01-01T00:00:00Z'",
        [],
    )
    .expect("update LastModified");
}

/// Compact-JSON manifest matching Python's field order and
/// `separators=(',', ':')` form (Pattern 1, 01-PATTERNS.md).
fn synthetic_manifest_json() -> String {
    synthetic_manifest_json_for(16)
}

/// Same compact-JSON manifest shape as [`synthetic_manifest_json`], but with
/// `userDataBackup.schemaVersion` parameterized. The gate cross-checks the
/// manifest's declared schema version against the DB's `PRAGMA user_version`
/// (03-CONTEXT.md D3-07), so a versioned fixture must carry a manifest whose
/// `schemaVersion` equals the DB's actual version.
fn synthetic_manifest_json_for(version: i64) -> String {
    format!(
        r#"{{"name":"JWL Manager Fixture","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0,"userDataBackup":{{"lastModifiedDate":"2026-01-01T00:00:00Z","deviceName":"JWL Manager Fixture_test","databaseName":"userData.db","hash":"0000000000000000000000000000000000000000000000000000000000000000","schemaVersion":{version}}}}}"#
    )
}

/// Inserts a REPRESENTATIVE set of additional `Location` rows (03-REVIEWS.md
/// finding 5) so 03-02's `Location_new` rebuild (`INSERT..SELECT` against the
/// UNIQUE + three CHECK constraints ported from `JWLManager.py:1026-1062`) is
/// actually exercised across every `Type` shape, not just the single scripture
/// row `insert_synthetic_notes` already seeds at `LocationId = 1`. Each row is
/// authored to LEGALLY satisfy the CHECK constraints so a correct upgrade
/// preserves all of them (T-03-12 in 03-01-PLAN.md's threat register — a
/// constraint violation surfaced here is a real upgrade defect, not a fixture
/// artifact). Must run BEFORE any reverse-mutation to a pre-v16 shape, while
/// `Location` still has the v16 `Specialty`/`Edition` columns.
fn insert_representative_locations(db_path: &Path) {
    let conn = Connection::open(db_path).expect("failed to open seeded userData.db");

    // Publication/document: Type 0, DocumentId present (non-zero), no
    // BookNumber/ChapterNumber/Track — satisfies the first Type-0 CHECK
    // branch (DocumentId IS NOT NULL AND DocumentId != 0).
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (20, NULL, NULL, 1001, NULL, 0, NULL, 0, 0, 'Fixture Publication', NULL, NULL)",
        [],
    )
    .expect("insert representative publication Location");

    // Media/track: Type 0, Track present + non-empty KeySymbol — satisfies
    // the Track branch of the Type-0 CHECK.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (21, NULL, NULL, NULL, 5, 0, 'pub-media', 0, 0, 'Fixture Media Track', NULL, NULL)",
        [],
    )
    .expect("insert representative media Location");

    // Type 1: KeySymbol required non-empty; Book/Chapter/DocumentId/Track
    // must all be NULL/zero per the Type-1 CHECK.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (22, NULL, NULL, NULL, NULL, 0, 'fixture-type1', 0, 1, 'Fixture Type 1', NULL, NULL)",
        [],
    )
    .expect("insert representative Type-1 Location");

    // Type 2: only Book/Chapter must be NULL/zero per the Type-2/3 CHECK;
    // everything else is unconstrained.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (23, NULL, NULL, 0, NULL, 0, NULL, 0, 2, 'Fixture Type 2', NULL, NULL)",
        [],
    )
    .expect("insert representative Type-2 Location");

    // Type 3: same shape as Type 2, distinct Type value.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (24, NULL, NULL, 0, NULL, 0, NULL, 0, 3, 'Fixture Type 3', NULL, NULL)",
        [],
    )
    .expect("insert representative Type-3 Location");

    // NULL-heavy: Type 2 again (least-constrained CHECK branch) but with
    // every other nullable column left NULL — the minimal legal row.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (25, NULL, NULL, NULL, NULL, 0, NULL, NULL, 2, NULL, NULL, NULL)",
        [],
    )
    .expect("insert representative NULL-heavy Location");
}

/// Reverse-mutates a v16-shaped `userData.db` (as seeded by
/// [`seed_from_res_blank`]) down to the pre-v16 `Location` shape and sets
/// `PRAGMA user_version`.
///
/// This applies ONLY the documented, verified v16<->v14 delta from
/// `.planning/research/FUNCTIONALITY-SPEC.md` and `JWLManager.py:1016-1070`:
/// drop the `Specialty`/`Edition` columns from `Location`, drop the
/// `IX_Location_Media` unique index (which is defined over those columns and
/// cannot exist without them), then set the requested version.
///
/// IMPORTANT (03-RESEARCH.md Pitfall 2): this is the ONLY schema delta this
/// codebase has independently verified below v16. For `version < 14` (v11,
/// v12, v13) this fixture is a REVERSE-MUTATED v16 shape carrying the same
/// pre-v16 Location shape as v14/v15, NOT an independently-verified v12/v13
/// archive — no further schema differences below v14 are known or asserted
/// here. These fixtures exist to exercise the upgrade CODE PATH (gate
/// accept/reject + the single monolithic upgrade transformation, D3-01)
/// against every acceptable/rejectable `user_version`, not to claim bit-exact
/// fidelity with a real JW Library v12/v13 database.
fn reverse_mutate_to_pre_v16_shape(db_path: &Path, version: i64) {
    let conn = Connection::open(db_path).expect("failed to open db for reverse-mutation");
    // foreign_keys defaults OFF in this codebase (03-RESEARCH.md verified) —
    // explicitly OFF here too so the Location rebuild (other tables FK-ref
    // Location) doesn't trip enforcement on a build where the SQLite default
    // compile-time setting differs.
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
         DROP INDEX IF EXISTS IX_Location_Media;
         CREATE TABLE Location_old (
             LocationId     INTEGER NOT NULL PRIMARY KEY,
             BookNumber     INTEGER,
             ChapterNumber  INTEGER,
             DocumentId     INTEGER,
             Track          INTEGER,
             IssueTagNumber INTEGER NOT NULL DEFAULT 0,
             KeySymbol      TEXT,
             MepsLanguage   INTEGER,
             Type           INTEGER NOT NULL,
             Title          TEXT
         );
         INSERT INTO Location_old SELECT LocationId, BookNumber, ChapterNumber, DocumentId, \
             Track, IssueTagNumber, KeySymbol, MepsLanguage, Type, Title FROM Location;
         DROP TABLE Location;
         ALTER TABLE Location_old RENAME TO Location;",
    )
    .expect("failed to reverse-mutate Location to pre-v16 shape");
    conn.pragma_update(None, "user_version", version)
        .expect("failed to set PRAGMA user_version");
}

/// Shared archive-assembly step used by every `.jwlibrary` fixture generator:
/// zips the seeded `userData.db` + `default_thumbnail.png` from `work_dir`
/// alongside the given `manifest.json` body, plus the same loose-media and
/// unknown-entry fixtures `generate_v16_fixture` has always included (01-05
/// preservation round-trip coverage).
fn build_fixture_archive(work_dir: &Path, manifest_json: &str) -> (TempDir, PathBuf) {
    let archive_dir = TempDir::new().expect("failed to create fixture archive dir");
    let archive_path = archive_dir.path().join("fixture.jwlibrary");
    let zip_file = fs::File::create(&archive_path).expect("failed to create fixture archive");
    let mut writer = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default();

    writer
        .start_file("manifest.json", options)
        .expect("start manifest.json");
    writer
        .write_all(manifest_json.as_bytes())
        .expect("write manifest.json");

    for name in ["userData.db", "default_thumbnail.png"] {
        writer.start_file(name, options).expect("start entry");
        let bytes = fs::read(work_dir.join(name)).expect("read seeded entry");
        writer.write_all(&bytes).expect("write entry");
    }

    writer
        .start_file("media/test.png", options)
        .expect("start media entry");
    writer
        .write_all(b"\x89PNG\r\n\x1a\nfixture-not-a-real-png")
        .expect("write media entry");

    writer
        .start_file("future_unknown.dat", options)
        .expect("start unknown entry");
    writer
        .write_all(b"forward-compat placeholder bytes")
        .expect("write unknown entry");

    writer.finish().expect("finish fixture zip");
    (archive_dir, archive_path)
}

/// Builds a synthetic `.jwlibrary` fixture at an arbitrary pre-v16
/// `user_version` (11-15), seeded from `res/blank`, carrying the same
/// located/independent Notes as [`generate_v16_fixture`] PLUS the
/// representative `Location` Type coverage from
/// [`insert_representative_locations`] (finding 5), then reverse-mutated to
/// the pre-v16 `Location` shape (see [`reverse_mutate_to_pre_v16_shape`] for
/// the v12/v13 fidelity caveat). The manifest's `schemaVersion` is set to the
/// same `version` (D3-07 cross-check).
pub fn generate_fixture_pre_v16_shape(version: i64) -> (TempDir, PathBuf) {
    assert!(
        (11..16).contains(&version),
        "generate_fixture_pre_v16_shape is for versions 11-15; got {version}"
    );
    let work_dir = TempDir::new().expect("failed to create fixture work dir");
    seed_from_res_blank(work_dir.path());
    let db_path = work_dir.path().join("userData.db");
    insert_synthetic_notes(&db_path);
    insert_representative_locations(&db_path);
    reverse_mutate_to_pre_v16_shape(&db_path, version);

    build_fixture_archive(work_dir.path(), &synthetic_manifest_json_for(version))
}

/// Builds a synthetic out-of-range `.jwlibrary` fixture at `user_version =
/// 17` (a hypothetical newer-than-supported schema, D3-09's reject path).
/// Keeps the full v16 `Location` shape (Specialty/Edition + IX_Location_Media)
/// since a real newer schema would only ever add to v16, never regress it.
pub fn generate_fixture_v17_shape() -> (TempDir, PathBuf) {
    let work_dir = TempDir::new().expect("failed to create fixture work dir");
    seed_from_res_blank(work_dir.path());
    let db_path = work_dir.path().join("userData.db");
    insert_synthetic_notes(&db_path);
    insert_representative_locations(&db_path);
    let conn = Connection::open(&db_path).expect("failed to open db to set user_version");
    conn.pragma_update(None, "user_version", 17_i64)
        .expect("failed to set PRAGMA user_version to 17");
    drop(conn);

    build_fixture_archive(work_dir.path(), &synthetic_manifest_json_for(17))
}

/// Thin by-name wrappers over [`generate_fixture_pre_v16_shape`] for callers
/// that want a specific version without repeating the magic number.
pub fn generate_v11_fixture() -> (TempDir, PathBuf) {
    generate_fixture_pre_v16_shape(11)
}
pub fn generate_v12_fixture() -> (TempDir, PathBuf) {
    generate_fixture_pre_v16_shape(12)
}
pub fn generate_v13_fixture() -> (TempDir, PathBuf) {
    generate_fixture_pre_v16_shape(13)
}
pub fn generate_v14_fixture() -> (TempDir, PathBuf) {
    generate_fixture_pre_v16_shape(14)
}
pub fn generate_v15_fixture() -> (TempDir, PathBuf) {
    generate_fixture_pre_v16_shape(15)
}

/// Builds a full synthetic v16 `.jwlibrary` fixture, seeded from `res/blank`,
/// containing:
///   - userData.db (v16, res/blank-seeded, + synthetic located/independent Notes)
///   - manifest.json (compact, schemaVersion 16)
///   - default_thumbnail.png (from res/blank)
///   - media/test.png (loose-media entry, for the 01-05 preservation round-trip test)
///   - future_unknown.dat (unknown/forward-compat entry, same purpose)
///
/// Returns the TempDir (keep alive) and the path to the built `.jwlibrary`.
pub fn generate_v16_fixture() -> (TempDir, PathBuf) {
    let work_dir = TempDir::new().expect("failed to create fixture work dir");
    seed_from_res_blank(work_dir.path());
    insert_synthetic_notes(&work_dir.path().join("userData.db"));

    build_fixture_archive(work_dir.path(), &synthetic_manifest_json())
}

// ---------------------------------------------------------------------------
// Schema-downgrade (v16 -> v14) fixture generators (04-01-PLAN.md)
// ---------------------------------------------------------------------------
//
// Each generator seeds a fresh `res/blank` v16 `userData.db` into a tempdir
// and returns `(TempDir, db_path)` — `downgrade_to_v14` operates on a
// `Connection` over that db, so no `.jwlibrary` wrapper is needed. All rows
// are synthetic. Inserts run with FK enforcement OFF (as the real
// delete/downgrade paths do), so a fixture may carry cross-table references
// without ordering constraints.
//
// The canonical colliding-group key shape (all six v14-U2 columns non-null,
// so the merge is genuinely required — without it U2 would not even fire):
//   BookNumber=NULL, ChapterNumber=NULL, DocumentId=1001, Track=5,
//   IssueTagNumber=0, KeySymbol='pub-x', MepsLanguage=0, Type=0.

/// Seeds a fresh `res/blank` v16 `userData.db` into a tempdir; returns the dir
/// handle (keep alive) and the path to the `userData.db`.
pub fn fresh_v16_db() -> (TempDir, PathBuf) {
    let work_dir = TempDir::new().expect("failed to create downgrade fixture dir");
    seed_from_res_blank(work_dir.path());
    let db_path = work_dir.path().join("userData.db");
    (work_dir, db_path)
}

/// Inserts a colliding `Location` group at the canonical key shape. `ids[0]`
/// is the intended (lowest) survivor when `ids` is ascending.
///
/// Each row carries a DISTINCT `Specialty` (`s<id>`). This is the load-bearing
/// real-world scenario: in v16 the `IX_Location_Media` UNIQUE index covers
/// `Specialty`/`Edition`, so rows sharing the six v14-U2 columns are legal in
/// v16 ONLY when they differ by Specialty/Edition — which v14 drops, collapsing
/// them into a single U2 group that the merge must reconcile.
fn insert_collision_group(conn: &Connection, ids: &[i64]) {
    for &id in ids {
        conn.execute(
            "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
             IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
             VALUES (?1, NULL, NULL, 1001, 5, 0, 'pub-x', 0, 0, ?2, ?3, NULL)",
            rusqlite::params![id, format!("Collision Location {id}"), format!("s{id}")],
        )
        .expect("insert collision-group Location");
    }
}

/// Main collision fixture: a 3-way colliding group (50/20/90, survivor 20) with
/// a dependent row across ALL 7 FK columns pointing at a NON-survivor id, plus
/// a scripture Location (non-null Book/Chapter) that must NOT be remapped.
pub fn generate_v16_collision_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");

    insert_collision_group(&conn, &[50, 20, 90]);

    // Scripture Location (non-null Book/Chapter) — excluded from the merge.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (100, 1, 1, NULL, NULL, 0, 'nwt', 0, 0, 'Genesis 1:1', NULL, NULL)",
        [],
    )
    .expect("insert scripture Location");

    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (700, 1, 'DG Tag')",
        [],
    )
    .expect("insert Tag");

    // Bookmark: LocationId -> 50, PublicationLocationId -> 90 (both non-survivor).
    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) VALUES (1, 50, 90, 0, 'DG Bookmark', NULL, 0, NULL)",
        [],
    )
    .expect("insert Bookmark");
    // Note -> 50.
    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) VALUES (10, 'dg-note-guid-0010', NULL, 50, \
         'DG Note', 'content', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, NULL)",
        [],
    )
    .expect("insert Note");
    // UserMark -> 90.
    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (10, 1, 90, 0, 'dg-usermark-guid-0010', 1)",
        [],
    )
    .expect("insert UserMark");
    // InputField -> 50.
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (50, 't1', 'v1')",
        [],
    )
    .expect("insert InputField");
    // TagMap -> 90 (LocationId form: NoteId/PlaylistItemId NULL).
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (700, NULL, 90, NULL, 700, 0)",
        [],
    )
    .expect("insert TagMap");
    // PlaylistItemLocationMap -> 50.
    conn.execute(
        "INSERT INTO PlaylistItemLocationMap (PlaylistItemId, LocationId, MajorMultimediaType, \
         BaseDurationTicks) VALUES (1, 50, 1, NULL)",
        [],
    )
    .expect("insert PlaylistItemLocationMap");

    (dir, db_path)
}

/// Inserts a 3-way colliding group (20/50/90, survivor 20) whose SURVIVOR is
/// kept trim-stable (a content-bearing Note references it) and where every FK
/// remap target (except the composite PlaylistItemLocationMap) carries exactly
/// one trim-stable dependent pointing at a NON-survivor id — so the dry-run's
/// per-target repoint count is a clean, asserted-by-value `1` and no dependent
/// is swept out from under the count by trim-first. No dedup collisions (the
/// survivor carries none of the composite-key dependents), so all repointed
/// rows are `overwritten`, and only the two merged-away Locations are `deleted`.
fn insert_dryrun_collision_graph(conn: &Connection) {
    insert_collision_group(conn, &[20, 50, 90]);
    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (700, 1, 'DG Tag')",
        [],
    )
    .expect("insert Tag");

    // Survivor 20 kept alive by a content-bearing Note (trim-stable).
    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) VALUES (11, 'dg-note-guid-0011', NULL, 20, \
         'Survivor note', 'survivor content', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, NULL)",
        [],
    )
    .expect("insert survivor Note");

    // Bookmark: LocationId -> 50, PublicationLocationId -> 90 (both non-survivor).
    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) VALUES (1, 50, 90, 0, 'DG Bookmark', NULL, 0, NULL)",
        [],
    )
    .expect("insert Bookmark");
    // Note -> 50 (content-bearing, trim-stable).
    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) VALUES (10, 'dg-note-guid-0010', NULL, 50, \
         'DG Note', 'content', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, NULL)",
        [],
    )
    .expect("insert Note");
    // UserMark -> 90, kept trim-stable by a BlockRange.
    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (10, 1, 90, 0, 'dg-usermark-guid-0010', 1)",
        [],
    )
    .expect("insert UserMark");
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (10, 1, 1, 0, 5, 10)",
        [],
    )
    .expect("insert BlockRange keeping UserMark trim-stable");
    // InputField -> 50 (non-empty Value, trim-stable).
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (50, 't1', 'v1')",
        [],
    )
    .expect("insert InputField");
    // TagMap -> 90 (LocationId form: NoteId/PlaylistItemId NULL -> trim-stable).
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (700, NULL, 90, NULL, 700, 0)",
        [],
    )
    .expect("insert TagMap");
}

/// Bare-db dedicated dry-run fixture (see [`insert_dryrun_collision_graph`]).
/// Survivor 20 stable under trim-first; deterministic per-target repoint counts.
pub fn generate_v16_dryrun_collision_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    insert_dryrun_collision_graph(&conn);
    (dir, db_path)
}

/// Full `.jwlibrary` archive wrapping [`generate_v16_collision_db`]'s graph:
/// the collision group's lowest-id survivor (20) is UNREFERENCED and thus
/// trim-eligible, so a trim-FIRST v14 save shifts the survivor to 50 (HIGH-2).
/// Used by the orchestration tests that need a real `ArchiveSession` via
/// `open_and_validate`.
pub fn generate_v16_collision_archive() -> (TempDir, PathBuf) {
    let work_dir = TempDir::new().expect("failed to create fixture work dir");
    seed_from_res_blank(work_dir.path());
    let db_path = work_dir.path().join("userData.db");
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    insert_collision_group(&conn, &[50, 20, 90]);
    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) VALUES (1, 50, 90, 0, 'DG Bookmark', NULL, 0, NULL)",
        [],
    )
    .expect("insert Bookmark");
    drop(conn);
    build_fixture_archive(work_dir.path(), &synthetic_manifest_json())
}

/// Full `.jwlibrary` archive wrapping the HIGH-1 un-downgradeable graph — a
/// real `ArchiveSession` whose v14 save must fail with `SchemaDowngradeFailed`
/// and write nothing.
pub fn generate_high1_undowngradeable_archive() -> (TempDir, PathBuf) {
    let work_dir = TempDir::new().expect("failed to create fixture work dir");
    seed_from_res_blank(work_dir.path());
    let db_path = work_dir.path().join("userData.db");
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (60, 1, NULL, 3001, 5, 0, 'nwt', 0, 0, 'U2 collide A', 'spec-a', NULL)",
        [],
    )
    .expect("insert U2-collide Location A");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (61, 1, NULL, 3001, 5, 0, 'nwt', 0, 0, 'U2 collide B', 'spec-b', NULL)",
        [],
    )
    .expect("insert U2-collide Location B");
    drop(conn);
    build_fixture_archive(work_dir.path(), &synthetic_manifest_json())
}

/// Composite-collision fixture builders. Each seeds the 2-way group (20/90,
/// survivor 20) plus a survivor + merged-away row that SHARE the secondary key
/// on one composite-key target — proving dedup-then-repoint.
pub fn generate_composite_tagmap_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    insert_collision_group(&conn, &[20, 90]);
    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (700, 1, 'DG Tag')",
        [],
    )
    .expect("insert Tag");
    // Survivor (TagId 700, LocationId 20) + merged-away (TagId 700, LocationId 90).
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (1, NULL, 20, NULL, 700, 0)",
        [],
    )
    .expect("insert survivor TagMap");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (2, NULL, 90, NULL, 700, 1)",
        [],
    )
    .expect("insert merged-away TagMap");
    (dir, db_path)
}

pub fn generate_composite_bookmark_slot_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    insert_collision_group(&conn, &[20, 90]);
    // Survivor (PublicationLocationId 20, Slot 0) + merged-away (90, Slot 0).
    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) VALUES (1, 20, 20, 0, 'Survivor', NULL, 0, NULL)",
        [],
    )
    .expect("insert survivor Bookmark");
    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) VALUES (2, 90, 90, 0, 'Merged', NULL, 0, NULL)",
        [],
    )
    .expect("insert merged-away Bookmark");
    (dir, db_path)
}

pub fn generate_composite_inputfield_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    insert_collision_group(&conn, &[20, 90]);
    // Survivor (LocationId 20, TextTag 'x') + merged-away (90, 'x').
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (20, 'x', 'survivor')",
        [],
    )
    .expect("insert survivor InputField");
    conn.execute(
        "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (90, 'x', 'merged')",
        [],
    )
    .expect("insert merged-away InputField");
    (dir, db_path)
}

pub fn generate_composite_playlist_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    insert_collision_group(&conn, &[20, 90]);
    // Survivor (PlaylistItemId 1, LocationId 20) + merged-away (1, LocationId 90).
    conn.execute(
        "INSERT INTO PlaylistItemLocationMap (PlaylistItemId, LocationId, MajorMultimediaType, \
         BaseDurationTicks) VALUES (1, 20, 1, NULL)",
        [],
    )
    .expect("insert survivor PILM");
    conn.execute(
        "INSERT INTO PlaylistItemLocationMap (PlaylistItemId, LocationId, MajorMultimediaType, \
         BaseDurationTicks) VALUES (1, 90, 1, NULL)",
        [],
    )
    .expect("insert merged-away PILM");
    (dir, db_path)
}

/// MED-4: a `KeySymbol='None'` Location (30) alongside a `KeySymbol IS NULL`
/// Location (40) sharing the other five key columns — must NOT be merged
/// (typed key distinguishes the literal string from SQL NULL).
pub fn generate_med4_none_key_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (30, NULL, NULL, 2001, 5, 0, 'None', 0, 0, 'Literal None key', NULL, NULL)",
        [],
    )
    .expect("insert KeySymbol='None' Location");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (40, NULL, NULL, 2001, 5, 0, NULL, 0, 0, 'NULL key', NULL, NULL)",
        [],
    )
    .expect("insert KeySymbol IS NULL Location");
    (dir, db_path)
}

/// HIGH-1: two EXCLUDED (non-null BookNumber) Locations sharing all six v14-U2
/// columns, differing only by Specialty — legal in v16, un-downgradeable to v14
/// (the merge never touches them, and the rebuild's U2 would be violated).
pub fn generate_high1_undowngradeable_db() -> (TempDir, PathBuf) {
    let (dir, db_path) = fresh_v16_db();
    let conn = Connection::open(&db_path).expect("open seeded db");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (60, 1, NULL, 3001, 5, 0, 'nwt', 0, 0, 'U2 collide A', 'spec-a', NULL)",
        [],
    )
    .expect("insert U2-collide Location A");
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (61, 1, NULL, 3001, 5, 0, 'nwt', 0, 0, 'U2 collide B', 'spec-b', NULL)",
        [],
    )
    .expect("insert U2-collide Location B");
    (dir, db_path)
}

// ---------------------------------------------------------------------------
// Trim (orphan sweep) fixture generator (02-01-PLAN.md Wave 0)
// ---------------------------------------------------------------------------

/// Inserts the multi-table-orphan graph 02-01-PLAN.md's Wave 0 requires:
///
/// - PRE-EXISTING genuine orphans (swept by trim regardless of any delete):
///   Location 950 (referenced by nothing), UserMark 951 (no BlockRange, no
///   Note — swept via the not-in-BlockRange-and-not-in-Note branch), and a
///   dangling BlockRange 952 (UserMarkId 999 does not exist). These exercise
///   the sweep + the two NOT-EXISTS-rewritten predicates (Location,
///   PlaylistItem) that a verbatim nullable `NOT IN` would fail to sweep
///   (finding 3).
/// - SURVIVING highlight (anti-over-delete, finding 1): UserMark 890 +
///   BlockRange 890 + Location 890 — a valid highlight referenced by no
///   deleted Note; trim must NEVER sweep it.
/// - Note 900 anchors highlight UserMark 900 (-> BlockRange 900, Location
///   900) and carries TagMap 900. Deleting Note 900 orphans ONLY TagMap 900
///   (swept); the highlight UserMark 900/BlockRange 900/Location 900 SURVIVES
///   the note deletion (a highlight is durable, not owned by the note —
///   FUNCTIONALITY-SPEC:140).
/// - SURVIVOR (finding 9): Note 901 references Location 901, which is ALSO
///   referenced by a Bookmark (`Bookmark.LocationId = 901`). Deleting Note
///   901 must NOT sweep Location 901 — the Bookmark reference keeps it live.
/// - GAPPED TAG POSITIONS (finding 6): Tag 901 carries three TagMap rows at
///   Positions 5, 9, 20 (never duplicated within one TagId — that would
///   violate `UNIQUE(TagId, Position)` and refuse to insert into a valid v16
///   DB) linking three additional independent Notes. Re-densify must compact
///   these to contiguous 0-based positions 0, 1, 2 (ordered by original
///   Position then TagMapId).
fn insert_trim_fixture_rows(db_path: &Path) {
    let conn = Connection::open(db_path).expect("failed to open seeded userData.db");
    // A pre-trim archive can legitimately carry dangling rows (prior edits,
    // vendor churn) — that is WHY trim exists. Insert the intentional
    // pre-existing orphans below with FK enforcement off, exactly as the real
    // delete/trim paths run (JWLManager.py:3681/3862 both force it off).
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("disable foreign_keys for orphan fixture inserts");

    // --- Genuine PRE-EXISTING orphans (swept by trim regardless of any
    //     delete) ----------------------------------------------------------
    // Location 950: referenced by nothing → swept once UserMark 951 is gone.
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (950, NULL, NULL, 9501, NULL, 0, NULL, 0, 0, 'Trim Genuine-Orphan Location', NULL, NULL)",
        [],
    )
    .expect("insert genuine-orphan Location 950");
    // UserMark 951: NO BlockRange, NO Note references it → swept via the
    // (not-in-BlockRange AND not-in-Note) branch. A UserMark WITH a BlockRange
    // is a durable highlight and is intentionally NOT an orphan.
    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (951, 1, 950, 0, 'fixture-trim-usermark-0951', 1)",
        [],
    )
    .expect("insert genuine-orphan UserMark 951");
    // BlockRange 952: UserMarkId 999 does not exist → dangling → swept.
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (952, 1, 1, 0, 5, 999)",
        [],
    )
    .expect("insert dangling BlockRange 952");

    // --- SURVIVING highlight (anti-over-delete, codex finding 1): UserMark
    //     890 + BlockRange 890 + Location 890 form a valid highlight that no
    //     deleted Note references. Trim must NEVER sweep it. ----------------
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (890, NULL, NULL, 8901, NULL, 0, NULL, 0, 0, 'Trim Survivor Highlight Location', NULL, NULL)",
        [],
    )
    .expect("insert survivor-highlight Location 890");
    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (890, 2, 890, 0, 'fixture-trim-usermark-0890', 1)",
        [],
    )
    .expect("insert survivor-highlight UserMark 890");
    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (890, 1, 1, 0, 8, 890)",
        [],
    )
    .expect("insert survivor-highlight BlockRange 890");

    // --- Note-900 graph: Note 900 anchors the highlight UserMark 900 →
    //     BlockRange 900 (Location 900). Deleting Note 900 orphans ONLY its
    //     TagMap 900 entry — the highlight (UserMark 900/BlockRange 900/
    //     Location 900) SURVIVES, because a highlight is durable and is not
    //     owned by the note (codex finding 1 / FUNCTIONALITY-SPEC:140). -----
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (900, NULL, NULL, 9001, NULL, 0, NULL, 0, 0, 'Trim Orphan Location', NULL, NULL)",
        [],
    )
    .expect("insert orphan-producing Location 900");

    conn.execute(
        "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
         VALUES (900, 1, 900, 0, 'fixture-trim-usermark-0900', 1)",
        [],
    )
    .expect("insert UserMark 900");

    conn.execute(
        "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (900, 1, 1, 0, 5, 900)",
        [],
    )
    .expect("insert BlockRange 900");

    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (900, 'fixture-trim-note-guid-0900', 900, 900, 'Trim orphan note title', \
         'Trim orphan note content.', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, NULL)",
        [],
    )
    .expect("insert Note 900");

    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (900, 1, 'Fixture Trim Tag 900')",
        [],
    )
    .expect("insert Tag 900");
    conn.execute(
        "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
         VALUES (900, NULL, NULL, 900, 900, 0)",
        [],
    )
    .expect("insert TagMap 900");

    // --- Survivor Location (finding 9) --------------------------------------
    conn.execute(
        "INSERT INTO Location (LocationId, BookNumber, ChapterNumber, DocumentId, Track, \
         IssueTagNumber, KeySymbol, MepsLanguage, Type, Title, Specialty, Edition) \
         VALUES (901, NULL, NULL, 9011, NULL, 0, NULL, 0, 0, 'Trim Survivor Location', NULL, NULL)",
        [],
    )
    .expect("insert survivor Location 901");

    conn.execute(
        "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, LastModified, \
         Created, BlockType, BlockIdentifier) \
         VALUES (901, 'fixture-trim-note-guid-0901', NULL, 901, 'Trim survivor note title', \
         'Trim survivor note content.', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, NULL)",
        [],
    )
    .expect("insert Note 901");

    conn.execute(
        "INSERT INTO Bookmark (BookmarkId, LocationId, PublicationLocationId, Slot, Title, \
         Snippet, BlockType, BlockIdentifier) \
         VALUES (900, 901, 901, 0, 'Fixture Trim Bookmark', NULL, 0, NULL)",
        [],
    )
    .expect("insert Bookmark referencing Location 901");

    // --- Gapped tag positions (finding 6) -----------------------------------
    conn.execute(
        "INSERT INTO Tag (TagId, Type, Name) VALUES (901, 1, 'Fixture Trim Tag 901')",
        [],
    )
    .expect("insert Tag 901");

    for (note_id, tag_map_id, position) in [(902, 902, 5), (903, 903, 9), (904, 904, 20)] {
        conn.execute(
            &format!(
                "INSERT INTO Note (NoteId, Guid, UserMarkId, LocationId, Title, Content, \
                 LastModified, Created, BlockType, BlockIdentifier) VALUES ({note_id}, \
                 'fixture-trim-note-guid-0{note_id}', NULL, NULL, 'Trim gapped note {note_id}', \
                 'Trim gapped note content {note_id}.', '2026-01-01T00:00:00Z', \
                 '2026-01-01T00:00:00Z', 0, NULL)"
            ),
            [],
        )
        .unwrap_or_else(|_| panic!("insert gapped-position Note {note_id}"));

        conn.execute(
            &format!(
                "INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) \
                 VALUES ({tag_map_id}, NULL, NULL, {note_id}, 901, {position})"
            ),
            [],
        )
        .unwrap_or_else(|_| panic!("insert gapped-position TagMap {tag_map_id}"));
    }

    conn.execute(
        "UPDATE LastModified SET LastModified = '2026-01-01T00:00:00Z'",
        [],
    )
    .expect("update LastModified");
}

/// Builds a full synthetic v16 `.jwlibrary` fixture (seeded from `res/blank`,
/// carrying the same base located/independent Notes as
/// [`generate_v16_fixture`]) PLUS [`insert_trim_fixture_rows`]'s multi-table
/// orphan graph, survivor Location, and gapped tag positions
/// (02-01-PLAN.md Wave 0 / Task 1).
pub fn generate_trim_fixture() -> (TempDir, PathBuf) {
    let work_dir = TempDir::new().expect("failed to create fixture work dir");
    seed_from_res_blank(work_dir.path());
    let db_path = work_dir.path().join("userData.db");
    insert_synthetic_notes(&db_path);
    insert_trim_fixture_rows(&db_path);

    build_fixture_archive(work_dir.path(), &synthetic_manifest_json())
}

// ---------------------------------------------------------------------------
// Zip-slip fixture generator (6 variants, D-08)
// ---------------------------------------------------------------------------

/// The six zip-slip variants the 01-02 archive-extraction plan tests against
/// (finding 11, 01-01-PLAN.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipSlipVariant {
    UnixTraversal,
    AbsoluteUnix,
    AbsoluteWindows,
    BackslashTraversal,
    DuplicateEntry,
    SymlinkChain,
}

/// Builds a single-variant malicious zip fixture for zip-slip rejection
/// testing. Each variant crafts a raw zip entry name/bytes that a correct
/// `enclosed_name`-validated extractor (zip >=2.3.0) must reject.
pub fn generate_zip_slip_fixture(variant: ZipSlipVariant) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("failed to create zip-slip fixture dir");
    let archive_path = dir.path().join("zip_slip.jwlibrary");

    // The `zip` crate's own ZipWriter refuses to write a duplicate filename
    // (write.rs: "Duplicate filename") — a defense-in-depth side effect of
    // the same hardening this pin relies on. To still produce a fixture that
    // proves an extractor rejects/handles duplicate entries, this one
    // variant is assembled as raw zip bytes rather than via ZipWriter.
    if variant == ZipSlipVariant::DuplicateEntry {
        write_raw_duplicate_entry_zip(&archive_path);
        return (dir, archive_path);
    }

    let zip_file = fs::File::create(&archive_path).expect("failed to create zip-slip fixture");
    let mut writer = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default();

    match variant {
        ZipSlipVariant::UnixTraversal => {
            writer
                .start_file("../escaped_unix_traversal.txt", options)
                .expect("start entry");
            writer.write_all(b"malicious").expect("write entry");
        }
        ZipSlipVariant::AbsoluteUnix => {
            writer
                .start_file("/etc/escaped_absolute_unix.txt", options)
                .expect("start entry");
            writer.write_all(b"malicious").expect("write entry");
        }
        ZipSlipVariant::AbsoluteWindows => {
            writer
                .start_file("C:\\escaped_absolute_windows.txt", options)
                .expect("start entry");
            writer.write_all(b"malicious").expect("write entry");
        }
        ZipSlipVariant::BackslashTraversal => {
            writer
                .start_file("..\\..\\escaped_backslash_traversal.txt", options)
                .expect("start entry");
            writer.write_all(b"malicious").expect("write entry");
        }
        ZipSlipVariant::DuplicateEntry => unreachable!("handled above via raw zip assembly"),
        ZipSlipVariant::SymlinkChain => {
            // Unix symlink entry (external attrs encode S_IFLNK) pointing
            // outside the extraction root; a naive `enclosed_name` check on
            // the *target* content (not just the entry name) can miss this —
            // exactly the CVE-2025-29787 variant that pinning zip>=2.3.0 fixes.
            let symlink_options = options.unix_permissions(0o120777);
            writer
                .start_file("link_to_outside", symlink_options)
                .expect("start symlink entry");
            writer
                .write_all(b"../../../../etc/passwd")
                .expect("write symlink target");
        }
    }

    writer.finish().expect("finish zip-slip fixture");
    (dir, archive_path)
}

/// Minimal bitwise CRC-32 (ISO-3309/zip polynomial), no lookup table — the
/// fixture payloads here are a handful of bytes, so a table isn't worth an
/// extra dependency.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Hand-assembles a minimal, spec-valid (store method, no compression) ZIP
/// containing TWO entries with the identical name `duplicate.txt` — deliberately
/// bypassing `zip::ZipWriter`'s own duplicate-filename guard so the fixture can
/// exercise an extractor's duplicate-entry handling (D-08 variant 5/6).
fn write_raw_duplicate_entry_zip(path: &Path) {
    let name = "duplicate.txt";
    let first: &[u8] = b"first";
    let second: &[u8] = b"second-overwrites-first";

    let mut buf: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offsets = Vec::new();

    for data in [first, second] {
        offsets.push(buf.len() as u32);
        let crc = crc32(data);
        let name_bytes = name.as_bytes();

        // Local file header (store method = 0, no compression).
        buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes()); // local file header signature
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        buf.extend_from_slice(&0u16.to_le_bytes()); // method = store
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&crc.to_le_bytes()); // crc32
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed size
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed size
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // name len
        buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(data);
    }

    for (i, data) in [first, second].iter().enumerate() {
        let crc = crc32(data);
        let name_bytes = name.as_bytes();
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes()); // central dir signature
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // method = store
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offsets[i].to_le_bytes()); // local header offset
        central.extend_from_slice(name_bytes);
    }

    let central_dir_offset = buf.len() as u32;
    buf.extend_from_slice(&central);

    // End of central directory record.
    buf.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // this disk
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk with central dir
    buf.extend_from_slice(&2u16.to_le_bytes()); // entries on this disk
    buf.extend_from_slice(&2u16.to_le_bytes()); // total entries
    buf.extend_from_slice(&(central.len() as u32).to_le_bytes()); // central dir size
    buf.extend_from_slice(&central_dir_offset.to_le_bytes()); // central dir offset
    buf.extend_from_slice(&0u16.to_le_bytes()); // comment len

    fs::write(path, buf).expect("failed to write raw duplicate-entry zip");
}
