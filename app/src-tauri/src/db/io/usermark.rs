//! Import-side UserMark synthesis + range-merge call site (IO-02/IO-03,
//! D8-05/D8-08, 08-03-PLAN.md Task 2) — ports `add_usermark`
//! (`JWLManager.py:2160-2184`).
//!
//! This module is the SINGLE shared entry point Highlights import (this
//! plan) and Notes import (08-04-PLAN.md's RANGE-driven sub-ranges) both
//! reuse UNCHANGED — the first production call site of the Phase-7
//! `db::highlights::merge_block_ranges` primitive, now driven by untrusted
//! external file content (T-08-14/T-08-15). [`merge_range_into`] delegates
//! the geometry ENTIRELY to that shipped primitive; this module never
//! re-derives the overlap/absorb/union algorithm (07-02-PLAN.md's
//! prohibition, re-enforced by 08-03-PLAN.md's negative-grep check).
//!
//! [`synthesize_usermark`] ALWAYS inserts a fresh `UserMark` row — Highlights
//! import never looks up an existing UserMark by ColorIndex/Version the way
//! Bookmark/Favorite dedup reuses existing rows (RESEARCH `## Wire Formats`
//! Highlights subsection). This IS the source of the accepted UserMark
//! non-idempotency (RESEARCH Pitfall 5): re-importing the same file always
//! creates a new UserMark row every time, even though the BlockRange
//! geometry converges via [`merge_range_into`].

use crate::db::highlights::merge_block_ranges;
use crate::db::ids::take_id;
use crate::error::ArchiveError;
use crate::guid::format_guid_v4;
use rusqlite::Transaction;
use std::collections::HashMap;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::ImportFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Inserts a fresh `UserMark` row — `StyleIndex` fixed at `0` (matching
/// Python's hardcoded `0` at `JWLManager.py:2164`/`:2166`), a
/// `format_guid_v4`-generated GUID (never `uuid`/`rand` — no new
/// dependency), `color_index`/`version`/`location_id` as given. Allocates the
/// id via [`take_id`] before falling back to autoincrement (D8-08). Returns
/// the new `UserMarkId`.
pub fn synthesize_usermark(
    tx: &Transaction,
    location_id: i64,
    color_index: i64,
    version: i64,
    guid_seed: u64,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let guid = format_guid_v4(guid_seed);
    if let Some(id) = take_id(available, "UserMark") {
        tx.execute(
            "INSERT INTO UserMark (UserMarkId, ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
             VALUES (?1, ?2, ?3, 0, ?4, ?5)",
            rusqlite::params![id, color_index, location_id, guid, version],
        )
        .map_err(|e| map_sqlite_err(e, "synthesize_usermark: insert recycled id"))?;
        Ok(id)
    } else {
        tx.execute(
            "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version) \
             VALUES (?1, ?2, 0, ?3, ?4)",
            rusqlite::params![color_index, location_id, guid, version],
        )
        .map_err(|e| map_sqlite_err(e, "synthesize_usermark: insert autoincrement"))?;
        Ok(tx.last_insert_rowid())
    }
}

/// Delegates the range geometry entirely to
/// [`crate::db::highlights::merge_block_ranges`] (D8-05) — the ONLY
/// overlap/absorb/union implementation in this codebase. Allocates the
/// merged row's id via [`take_id`] first (D8-08) and threads it through as
/// `merge_block_ranges`'s `recycled_id` parameter, falling back to
/// autoincrement (`None`) exactly like every other insert in this module.
/// Returns the new/merged `BlockRangeId`.
#[allow(clippy::too_many_arguments)] // each param is a distinct typed value threaded straight
                                      // through to `merge_block_ranges`; a struct would add
                                      // ceremony for a single-call-site internal primitive
pub fn merge_range_into(
    tx: &Transaction,
    identifier: i64,
    location_id: i64,
    start: i64,
    end: i64,
    block_type: i64,
    user_mark_id: i64,
    available: &mut HashMap<&'static str, Vec<i64>>,
) -> Result<i64, ArchiveError> {
    let recycled_id = take_id(available, "BlockRange");
    merge_block_ranges(
        tx,
        identifier,
        location_id,
        start,
        end,
        block_type,
        user_mark_id,
        recycled_id,
    )
}

// No unit test module here: every function in this file needs a real
// `res/blank`-seeded v16 database (`UserMark`/`BlockRange`/`Location`'s CHECK
// constraints), which only the `tests/common` fixture harness provides.
// Coverage lives in `tests/import_range_merge_tests.rs` (id-recycling,
// overlap/chain-merge, cross-color, re-import convergence) — see that file
// rather than duplicating a second DB harness inside this module.
