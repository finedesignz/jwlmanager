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
    // Field order + compact separators intentionally match
    // JWLManager.py:979-991 / :1152-1170 (see 01-PATTERNS.md Pattern 1).
    r#"{"name":"JWL Manager Fixture","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0,"userDataBackup":{"lastModifiedDate":"2026-01-01T00:00:00Z","deviceName":"JWL Manager Fixture_test","databaseName":"userData.db","hash":"0000000000000000000000000000000000000000000000000000000000000000","schemaVersion":16}}"#.to_string()
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

    let archive_dir = TempDir::new().expect("failed to create fixture archive dir");
    let archive_path = archive_dir.path().join("fixture.jwlibrary");
    let zip_file = fs::File::create(&archive_path).expect("failed to create fixture archive");
    let mut writer = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default();

    writer
        .start_file("manifest.json", options)
        .expect("start manifest.json");
    writer
        .write_all(synthetic_manifest_json().as_bytes())
        .expect("write manifest.json");

    for name in ["userData.db", "default_thumbnail.png"] {
        writer.start_file(name, options).expect("start entry");
        let bytes = fs::read(work_dir.path().join(name)).expect("read seeded entry");
        writer.write_all(&bytes).expect("write entry");
    }

    // Loose-media entry (01-05 preservation test).
    writer
        .start_file("media/test.png", options)
        .expect("start media entry");
    writer
        .write_all(b"\x89PNG\r\n\x1a\nfixture-not-a-real-png")
        .expect("write media entry");

    // Unknown/forward-compat entry (01-05 preservation test).
    writer
        .start_file("future_unknown.dat", options)
        .expect("start unknown entry");
    writer
        .write_all(b"forward-compat placeholder bytes")
        .expect("write unknown entry");

    writer.finish().expect("finish fixture zip");
    (archive_dir, archive_path)
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
