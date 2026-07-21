# Plan 02-01 — Summary

**Plan:** 02-01 (trim_db orphan sweep + tag re-densify + VACUUM, wired into save)
**Phase:** 2 (Safe Delete)
**Status:** Complete
**Requirements:** ARCH-04, SAFE-02, SAFE-04 (rollback), QA-02 (semantic fixtures)

## What shipped

- `app/src-tauri/src/db/trim.rs` — `trim_sweep(tx)` (DML-only, VACUUM-free, reusable by the Wave-2 dry-run) + `trim_db(conn)` (sweep in a transaction → PRAGMA restore → VACUUM outside the transaction). Ported from `JWLManager.py:3858-3935` in verbatim statement order.
- `app/src-tauri/src/db/pragma_guard.rs` — RAII guard snapshotting `foreign_keys`/`journal_mode`/`synchronous`/`temp_store` and restoring on drop (commit AND rollback/error paths).
- `app/src-tauri/src/archive/save.rs` — `trim_db` runs on save immediately before the manifest hash (hash-last, D2-04).
- `app/src-tauri/src/error.rs` — `ArchiveError::TrimFailed { reason }` + ErrorDto mapping.
- `app/src-tauri/tests/common/mod.rs` — `generate_trim_fixture` (multi-table orphan graph, survivor highlight, Bookmark-referenced survivor Location, gapped tag positions).
- `app/src-tauri/tests/trim_tests.rs` — 15 tests, all green (incl. 3 `#[ignore]`d Python-oracle legs, VERIFIED PASSING locally).

## Data-integrity decisions (cross-AI review, 02-REVIEWS.md)

- **`except:crash_box:sys.exit` NOT ported (D2-02):** a failed trim → typed `TrimFailed` + full rollback; never a partial trim or process exit.
- **Nullable `NOT IN` → `NOT EXISTS` (finding 3):** Python's Location + PlaylistItem orphan predicates NULL-poison (one NULL makes `NOT IN` match nothing) so they never sweep those orphans; rewritten as `NOT EXISTS`, which sweeps correctly. Deliberate safety fix over the Python latent bug; non-nullable predicates kept verbatim for order fidelity.
- **PragmaGuard (finding 4):** PRAGMAs are not rolled back by SQLite, so the guard restores the prior connection state on every path. Tests assert all four PRAGMAs restored after trim success AND failure; `foreign_key_check` clean after a real trim.
- **Explicit-column re-densify:** `INSERT INTO TagMap (TagMapId, PlaylistItemId, LocationId, NoteId, TagId, Position) SELECT ... FROM TagMapNew` — never `SELECT *`; Wave-0 `PRAGMA table_info(TagMap)` + window-function tests gate it.
- **Correct highlight semantics (finding 1 — the over-deletion concern):** deleting a Note orphans ONLY its TagMap entry. The highlight it anchored (UserMark + BlockRange + Location) is durable and SURVIVES — a UserMark with a BlockRange is a real highlight, not an orphan (matches `JWLManager.py` exactly; FUNCTIONALITY-SPEC:140). trim sweeps only GENUINE orphans: a no-BlockRange/no-Note UserMark, a dangling BlockRange, an unreferenced Location. Bookmark-referenced Locations survive (finding 9).

## Deviations / findings (documented)

1. **`PRAGMA temp_store = 'MEMORY'` drops TEMP triggers.** The forced-failure rollback test originally used a `CREATE TEMP TRIGGER`, which SQLite silently deletes the instant trim sets `temp_store=MEMORY` — so the forced failure never fired and the test falsely passed as `Ok`. Fixed by using a **permanent** trigger (on the throwaway fixture DB). This is a genuine SQLite gotcha worth remembering for any future temp-object-based failure injection.
2. **Foreign keys ON by default here (Phase 3 finding) applies to setup too.** The test delete helpers force `foreign_keys=OFF` before deleting a Note — exactly as the real delete op does (`JWLManager.py:3681`) — since a Note still referenced by a soon-to-be-orphan row would otherwise trip the FK constraint.
3. **The trim fixture intentionally carries a dangling BlockRange (an invalid-until-trimmed archive).** That is the whole point of trim — a pre-trim archive can carry vendor/edit churn. After a trimmed save the archive is clean and `foreign_key_check` passes.

## Deferred (recorded)

- **Save-time trim is destructive without its own preview** (finding 7): a bare save silently removes empty untagged Notes, empty InputFields, and unused Tags (matches the Python app). A documentary test records exactly what a bare save trims. A dedicated save-time trim preview is deferred to a later phase — the delete flow's dry-run (Wave 2) covers the explicit-delete case.

## Verification

- `cargo fmt --check` clean · `cargo clippy --all-targets -- -D warnings` clean
- `cargo test` full workspace: 85 tests pass, 0 failed
- `cargo test --test trim_tests -- --include-ignored`: 15/15 pass (incl. Python oracle)
- `npm run build` clean

**Next:** 02-02 (delete backend + dry-run) consumes `trim_sweep` + `TrimFailed`.
</content>
