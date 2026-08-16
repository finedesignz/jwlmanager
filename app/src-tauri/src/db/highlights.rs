//! `merge_block_ranges` — the geometric `BlockRange` union-merge primitive
//! (EDIT-02, 07-02-PLAN.md Task 2), ported from `add_usermark`
//! (`JWLManager.py:2160-2184`, specifically the overlap test at `:2174`, the
//! union expansion at `:2175-2176`, and the delete-absorbed-then-insert-merged
//! shape at `:2177-2184`).
//!
//! **This primitive has NO user-facing trigger this phase.** D7-03 (recorded
//! in `07-02-SUMMARY.md`) resolves the ROADMAP-criterion/Python-behavior
//! mismatch as strict parity: the Python `set_color` (the only Phase 7
//! recolor path) does not merge — the union-merge exists exclusively in the
//! IMPORT path (`add_usermark`), which is Phase 8's concern. This module
//! exists so the phase's most dangerous algorithm ships as a standalone,
//! exhaustively round-trip-tested unit ahead of that need, satisfying ROADMAP
//! criterion 1 via the primitive's existence and test coverage rather than
//! via any recolor-time invocation. `db::color` (this plan's other module)
//! never calls into this file — enforced by a negative grep in
//! `07-02-PLAN.md`'s prohibitions.
//!
//! [`plan_merge`] is a PURE function over `&[(id, start, end)]` triples — no
//! SQL, no `Transaction`, no rusqlite type anywhere in its signature or body
//! — so the geometry is exhaustively unit-testable without a database
//! round-trip, following the "compute the plan in Rust first, then mutate"
//! shape `app/src-tauri/src/archive/downgrade.rs:519-537`'s
//! `compute_merge_groups` already establishes in this repo (the closest
//! in-repo analog; the overlap-test algorithm itself has none).
//! [`merge_block_ranges`] is the thin SQL executor built on top of it.

use crate::error::ArchiveError;
use rusqlite::Transaction;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::DeleteFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Pure geometry: given `existing` `(BlockRangeId, StartToken, EndToken)`
/// triples already present at one `(Identifier, LocationId)` grouping key,
/// and a new range `[ns, ne]`, returns the absorbed `BlockRangeId`s and the
/// expanded union `(ns, ne)`.
///
/// Ports the overlap test verbatim from `JWLManager.py:2174`:
/// `ce >= ns and ne >= cs` (half-open-token-inclusive) — deliberately NOT
/// filtered by `ColorIndex` (the Python groups purely by `Identifier`/
/// `LocationId`; a highlight of a different color at the same location still
/// absorbs). Iterates to a fixed point so chained/transitive overlaps (three
/// or more ranges each overlapping the next) all coalesce into one union in
/// a single call, matching what repeated Python `add_usermark` invocations
/// against the same growing `BlockRange` set would eventually converge to.
///
/// No SQL, no `rusqlite` type anywhere in this signature or body — testable
/// with zero database setup.
pub(crate) fn plan_merge(existing: &[(i64, i64, i64)], ns: i64, ne: i64) -> (Vec<i64>, (i64, i64)) {
    let mut ns = ns;
    let mut ne = ne;
    let mut absorbed: Vec<i64> = Vec::new();
    let mut remaining: Vec<(i64, i64, i64)> = existing.to_vec();

    loop {
        let mut changed = false;
        remaining.retain(|&(id, cs, ce)| {
            if ce >= ns && ne >= cs {
                ns = ns.min(cs);
                ne = ne.max(ce);
                absorbed.push(id);
                changed = true;
                false
            } else {
                true
            }
        });
        if !changed {
            break;
        }
    }

    (absorbed, (ns, ne))
}

/// SQL executor around [`plan_merge`]: SELECTs the existing `BlockRange`
/// rows at `(identifier, location_id)` (joined through `UserMark` for
/// `LocationId`, since `BlockRange` itself carries no `LocationId` column —
/// `JWLManager.py:2167`'s `SELECT * FROM BlockRange JOIN UserMark
/// USING(UserMarkId) WHERE Identifier = ? AND LocationId = ?`), computes the
/// absorb/union plan, DELETEs exactly the absorbed rows (placeholder-count-
/// only dynamic SQL, `params_from_iter` — SAFE-02), and INSERTs one merged
/// row carrying `block_type` through (never defaulted — `BlockRange`'s
/// `CHECK (BlockType BETWEEN 1 AND 2)` would reject a stray `0`) and
/// `user_mark_id` as the row's owner. Returns the new row's `BlockRangeId`.
///
/// Runs inside the caller's transaction (never commits/rolls back itself),
/// exactly like every other Phase 7 `apply_*` primitive.
///
/// `recycled_id`, added in 08-03-PLAN.md Task 2 (D8-08/IO-03) for the
/// Highlights/Notes import call sites this primitive gained this phase: when
/// `Some(id)`, the merged row is INSERTed with that explicit `BlockRangeId`
/// (a gap the caller already popped via `db::ids::take_id`); when `None`
/// (every Phase 7 recolor/delete call site — recolor never calls this
/// function at all, per the module doc's D7-03 note, and no other Phase 7
/// caller exists), the row falls back to plain autoincrement exactly as
/// before. This is purely an INSERT-target change — the geometry in
/// [`plan_merge`] above is untouched, so this remains the single merge
/// implementation the prohibitions in `07-02-PLAN.md`/`08-03-PLAN.md`
/// require it to be.
#[allow(clippy::too_many_arguments)] // each param is a distinct typed value; a struct would add
                                     // ceremony for a single-call-site internal primitive
pub fn merge_block_ranges(
    tx: &Transaction,
    identifier: i64,
    location_id: i64,
    ns: i64,
    ne: i64,
    block_type: i64,
    user_mark_id: i64,
    recycled_id: Option<i64>,
) -> Result<i64, ArchiveError> {
    let existing: Vec<(i64, i64, i64)> = {
        let mut stmt = tx
            .prepare(
                "SELECT b.BlockRangeId, b.StartToken, b.EndToken FROM BlockRange b \
                 JOIN UserMark u USING (UserMarkId) \
                 WHERE b.Identifier = ?1 AND u.LocationId = ?2",
            )
            .map_err(|e| map_sqlite_err(e, "merge_block_ranges: prepare select"))?;
        let rows = stmt
            .query_map(rusqlite::params![identifier, location_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| map_sqlite_err(e, "merge_block_ranges: query existing ranges"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| map_sqlite_err(e, "merge_block_ranges: read existing ranges"))?;
        rows
    };

    let (absorbed, (union_ns, union_ne)) = plan_merge(&existing, ns, ne);

    if !absorbed.is_empty() {
        let placeholders: String = std::iter::repeat_n("?", absorbed.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM BlockRange WHERE BlockRangeId IN ({placeholders})");
        tx.execute(&sql, rusqlite::params_from_iter(absorbed.iter()))
            .map_err(|e| map_sqlite_err(e, "merge_block_ranges: delete absorbed"))?;
    }

    if let Some(id) = recycled_id {
        tx.execute(
            "INSERT INTO BlockRange (BlockRangeId, BlockType, Identifier, StartToken, EndToken, UserMarkId) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, block_type, identifier, union_ns, union_ne, user_mark_id],
        )
        .map_err(|e| map_sqlite_err(e, "merge_block_ranges: insert merged range (recycled id)"))?;
        return Ok(id);
    }

    tx.execute(
        "INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![block_type, identifier, union_ns, union_ne, user_mark_id],
    )
    .map_err(|e| map_sqlite_err(e, "merge_block_ranges: insert merged range (autoincrement)"))?;

    Ok(tx.last_insert_rowid())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn plan_merge_empty_existing_returns_new_range_unabsorbed() {
        let (absorbed, union) = plan_merge(&[], 10, 20);
        assert!(absorbed.is_empty());
        assert_eq!(union, (10, 20));
    }

    #[test]
    fn plan_merge_non_overlapping_before_is_untouched() {
        // ce < ns: existing range ends before the new one starts.
        let (absorbed, union) = plan_merge(&[(1, 0, 5)], 10, 20);
        assert!(absorbed.is_empty());
        assert_eq!(union, (10, 20));
    }

    #[test]
    fn plan_merge_non_overlapping_after_is_untouched() {
        // cs > ne: existing range starts after the new one ends.
        let (absorbed, union) = plan_merge(&[(1, 30, 40)], 10, 20);
        assert!(absorbed.is_empty());
        assert_eq!(union, (10, 20));
    }

    #[test]
    fn plan_merge_touching_boundary_ce_equals_ns_is_absorbed() {
        // ce == ns satisfies `ce >= ns` — inclusive-boundary hit.
        let (absorbed, union) = plan_merge(&[(1, 0, 10)], 10, 20);
        assert_eq!(absorbed, vec![1]);
        assert_eq!(union, (0, 20));
    }

    #[test]
    fn plan_merge_touching_boundary_ns_equals_ce_reverse_direction() {
        // Symmetric direction: new range ends exactly where existing starts (ne == cs).
        let (absorbed, union) = plan_merge(&[(1, 20, 30)], 10, 20);
        assert_eq!(absorbed, vec![1]);
        assert_eq!(union, (10, 30));
    }

    #[test]
    fn plan_merge_one_token_past_boundary_is_a_miss() {
        // ce = ns - 1: one token short of touching — must NOT absorb.
        let (absorbed, union) = plan_merge(&[(1, 0, 9)], 10, 20);
        assert!(absorbed.is_empty());
        assert_eq!(union, (10, 20));
    }

    #[test]
    fn plan_merge_one_token_past_boundary_reverse_direction_is_a_miss() {
        // cs = ne + 1: one token past the new range's end — must NOT absorb.
        let (absorbed, union) = plan_merge(&[(1, 21, 30)], 10, 20);
        assert!(absorbed.is_empty());
        assert_eq!(union, (10, 20));
    }

    #[test]
    fn plan_merge_fully_contained_existing_is_absorbed_union_unchanged() {
        let (absorbed, union) = plan_merge(&[(1, 12, 18)], 10, 20);
        assert_eq!(absorbed, vec![1]);
        assert_eq!(union, (10, 20));
    }

    #[test]
    fn plan_merge_existing_fully_containing_new_expands_union() {
        let (absorbed, union) = plan_merge(&[(1, 0, 100)], 10, 20);
        assert_eq!(absorbed, vec![1]);
        assert_eq!(union, (0, 100));
    }

    #[test]
    fn plan_merge_chained_overlaps_all_absorb_into_one_union() {
        // Three ranges, each overlapping the next: (0,5), (5,10), (10,15) — the
        // middle one only overlaps the new [12,20] directly; the first only
        // overlaps transitively once the union expands leftward. Fixed-point
        // iteration must catch all three.
        let existing = vec![(1, 0, 5), (2, 5, 10), (3, 10, 15)];
        let (mut absorbed, union) = plan_merge(&existing, 12, 20);
        absorbed.sort_unstable();
        assert_eq!(absorbed, vec![1, 2, 3]);
        assert_eq!(union, (0, 20));
    }

    #[test]
    fn plan_merge_ignores_color_grouping_key_is_identifier_location_only() {
        // plan_merge has no ColorIndex parameter at all — the grouping key is
        // established entirely by the CALLER's (Identifier, LocationId) SQL
        // filter, not by anything in this pure function. Two overlapping
        // ranges merge regardless of what color their owning UserMarks carry;
        // this test documents that by showing the geometry alone decides.
        let (absorbed, union) = plan_merge(&[(1, 0, 10), (2, 8, 15)], 9, 12);
        assert_eq!(absorbed.len(), 2);
        assert_eq!(union, (0, 15));
    }
}
