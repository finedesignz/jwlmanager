# Phase 2: Safe Delete (Dry-Run + Trim + Transactions) - Context

**Gathered:** 2026-07-21
**Status:** Ready for planning

<domain>
## Phase Boundary

The first destructive operation (delete selected Notes) ships together with the safety net every later destructive phase reuses: a dry-run preview before any change, a transaction that rolls back cleanly on failure, and the `trim_db` orphan-sweep + tag re-densify + VACUUM applied on save.

**In scope:** delete selected Notes (EDIT-01); a dry-run preview stating what will be added/overwritten/deleted with a cancel option (SAFE-01); `trim_db` on save — orphan sweep across all referenced tables, tag position re-densify via ROW_NUMBER, `Location.Title=""` where NULL, VACUUM (ARCH-04); every mutation in a transaction with clean rollback (SAFE-04); empty-selection deletes impossible by construction, not merely a disabled button (SAFE-03); all SQL parameterized incl. IN-clauses (SAFE-02); a round-trip semantic-equivalence test for delete (QA-02).

**Out of scope (own phases):** editing colors/tags/order/favorites/clean/raw (Phase 7), the other five categories' delete (this phase deletes Notes only — the mechanism generalizes later), merge (Phase 5), schema up/down (Phases 3/4, 3 done), import/export (Phase 8).

**Requirements:** ARCH-04, EDIT-01, SAFE-01, SAFE-02, SAFE-03, SAFE-04, QA-02

</domain>

<decisions>
## Implementation Decisions

Auto-selected (`--auto`), recommended default per gray area, rationale for audit.

### trim_db — the orphan sweep

- **D2-01:** Port `trim_db` from `JWLManager.py:3858-3935` **statement-for-statement in the same order**, as static parameterless DDL/DML. The sweep order is load-bearing: Notes/InputField emptied first, then TagMap orphans, then unused Tags, then the TagMap ROW_NUMBER re-densify, then UserMark/BlockRange, then the Playlist* cascade, then unused Location, then `Location.Title=""` where NULL.
  `[auto] trim shape — Q: "Reimplement the sweep logic or port verbatim order?" → Selected: "Port verbatim order" (recommended default)`
  **Rationale:** The order encodes referential dependencies (delete children before parents; re-densify tags after orphan TagMap removal). Reordering risks leaving or removing the wrong rows. This is the engine that keeps a saved archive internally consistent — match the proven app exactly.

- **D2-02 (defect NOT to port):** The Python `trim_db` wraps everything in `try/except Exception: crash_box + sys.exit()`. The Rust port surfaces failure as a typed `ArchiveError` and rolls the working copy back — never a partial trim, never a process exit. Same SAFE-05 posture as Phase 3's `upgrade_to_v16`.
  `[auto] trim errors — Q: "Mirror the crash-and-exit, or typed error + rollback?" → Selected: "Typed error + rollback" (recommended default)`

- **D2-03:** The `trim_db` body runs inside a single explicit transaction (the Python `BEGIN;...COMMIT;`), and **`foreign_keys` is explicitly forced OFF for the duration and restored after** — matching the Python PRAGMA toggling AND the empirical finding from Phase 3 that FK does NOT default off in this build's bundled SQLite (03-02 SUMMARY). VACUUM runs OUTSIDE the transaction (SQLite requires it), after COMMIT, as the Python code does.
  `[auto] trim txn — Q: "One transaction with FK-off, VACUUM outside?" → Selected: "Yes, match Python" (recommended default)`
  **Rationale:** The orphan deletes intentionally break-then-rebuild referential relationships (e.g. the TagMap delete+reinsert during re-densify); FK enforcement mid-sweep would abort it. Phase 3 already proved FK is on by default here, so forcing it off is mandatory, not optional.

- **D2-04:** `trim_db` runs on **save** (ARCH-04), applied to the working-copy DB immediately before the manifest hash is computed (hash-last, Phase 1). This means a Phase 1 save (which did NOT trim) and a Phase 2 save of the same archive differ — that is correct and expected; parity is semantic, never byte-identical (this is exactly why the earlier real round-trips grew in size).
  `[auto] trim timing — Q: "Trim on every save, or only after a delete?" → Selected: "Every save (matches Python)" (recommended default)`

### Delete

- **D2-05:** Delete removes the selected Note rows (and their directly-owned rows — the Note's UserMark/BlockRange links) via parameterized DELETE; the broader orphan cleanup is then handled by `trim_db` on save rather than hand-cascaded in the delete itself. Delete marks the session dirty; nothing is written to disk until save.
  `[auto] delete scope — Q: "Cascade everything in delete, or delete Notes + let trim_db sweep orphans on save?" → Selected: "Delete Notes, trim sweeps orphans on save" (recommended default)`
  **Rationale:** Mirrors the Python app's division of labor (targeted delete + trim_db as the general orphan collector) and avoids duplicating the sweep logic in two places. The dry-run preview accounts for both the direct deletes and the orphan cascade so the user sees the true effect.

- **D2-06 (SAFE-03 — impossible by construction):** The delete command takes a **non-empty selection type** — the Tauri command signature and the frontend action are structured so an empty selection cannot reach the delete path at all (e.g. the command rejects an empty id list as a typed error before touching the DB, AND the UI only enables the flow with ≥1 selected). Not a disabled button alone.
  `[auto] empty-guard — Q: "Guard empty selection by button state or by construction?" → Selected: "By construction (typed rejection + type-level)" (recommended default)`

### Dry-run preview (SAFE-01 — reused by every later destructive phase)

- **D2-07:** The dry-run computes the effect **inside a transaction that is rolled back** (or against a scratch copy), returning a structured `DryRunReport { added, overwritten, deleted }` count/summary WITHOUT mutating the working copy. For delete: deleted = the selected Notes + the orphans `trim_db` would then remove. The preview is the source of truth the user confirms against.
  `[auto] dry-run mechanism — Q: "Simulate in a rolled-back transaction or diff a scratch copy?" → Selected: "Rolled-back transaction" (recommended default)`
  **Rationale:** A rolled-back transaction gives the exact real effect (same SQL, same order) with zero risk of divergence between preview and apply. This structure is deliberately general so merge (Phase 5) and downgrade (Phase 4) reuse it.

- **D2-08:** The preview is presented and requires explicit confirmation before apply; a cancel path leaves everything untouched. Reuse the Phase 1 command-bar + typed-error surface; add a preview/confirm UI element consistent with UI-SPEC tokens (calm, trustworthy, this is a destructive-action confirm).
  `[auto] confirm flow — Q: "Auto-apply after preview or require explicit confirm?" → Selected: "Require explicit confirm" (recommended default)`

### Verification (QA-02)

- **D2-09:** A **round-trip semantic-equivalence test**: build a fixture with known Notes + orphans, delete a selection, save (trim runs), reopen, assert the normalized table state equals the expected post-state — NEVER byte equality (trim+VACUUM make bytes non-deterministic). Include a fixture whose delete produces orphans in multiple tables so the sweep is actually exercised.
  `[auto] parity test — Q: "Byte-diff or normalized-table semantic equivalence?" → Selected: "Semantic (normalized-table)" (recommended default)`

- **D2-10 (SAFE-04 rollback proof):** A test that induces a failure mid-delete/mid-trim (e.g. a forced error after the first DELETE) and asserts the working-copy DB is unchanged from before the operation — the transaction rolled back fully. This is the Core-Value guarantee for every destructive op.

### Claude's Discretion
Module placement (likely `db/trim.rs` + `db/delete.rs` or an `ops/` module), the exact `DryRunReport` shape, and how the frontend renders the preview (within UI-SPEC).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### The sweep source of truth
- `JWLManager.py:3858-3935` — `trim_db`. The exact statements and ORDER to port (D2-01). The PRAGMA toggling (foreign_keys OFF/ON, journal/sync) and VACUUM-after-COMMIT. Lines 3936+ (`except: crash_box; sys.exit`) are the defect NOT ported (D2-02).
- `JWLManager.py:1245` — `self.trim_db()` call site (on save). `:1797` — a separate `VACUUM` path.

### Phase 1 + 3 foundations
- `.planning/phases/01-open-view-save-foundation-slice/01-CONTEXT.md` — D-04 atomic save (trim runs before hash-last), the ErrorDto surface, the command bar.
- `.planning/phases/03-schema-upgrade/03-02-SUMMARY.md` — the FK-not-default-off finding; the transactional-rusqlite pattern to mirror; `upgrade.rs` is the analog for how a transactional DB mutation + rollback test is structured.
- `app/src-tauri/src/session.rs` — ArchiveSession (dirty flag, db_path).
- `app/src-tauri/src/archive/save.rs` — where trim_db hooks in before the manifest hash.
- `app/src-tauri/src/error.rs` — add delete/trim/dry-run error variants.
- `app/src-tauri/src/db/notes.rs` — the Notes query; delete targets these rows.

### Format contract
- `.planning/research/FUNCTIONALITY-SPEC.md` — trim/save semantics, the non-byte-preserving save rule (semantic parity only).
- `.planning/codebase/CONCERNS.md` — the bare-except / crash-and-exit pattern D2-02 refuses.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Phase 1 atomic save (`archive/save.rs`) — trim_db slots in before the hash step; the save path already writes temp+atomic-rename.
- Phase 3 `upgrade.rs` — the template for a transactional, FK-off-forced, typed-error, rollback-tested DB mutation. trim_db has the same shape.
- Phase 1 fixture generator + the representative-Location rows added in Phase 3 — extend to seed Notes + orphans for the delete/trim tests.
- Phase 1 ErrorDto + command bar + the confirm/error UI surface — the dry-run preview is a new element on the same surface.
- The verified Python differential oracle — extend so a deleted-then-saved archive is accepted by check_validity.

### Established Patterns
- Working copy in temp dir; source never mutated; save = temp+atomic rename.
- Typed errors, no unwrap/expect on archive-data paths, all SQL parameterized.
- Semantic (normalized-table) parity, never byte-diff — trim+VACUUM make bytes non-deterministic.
- FK is ON by default in this bundled SQLite (Phase 3 finding) — trim MUST force it off for the sweep and restore after.

### Integration Points
- `trim_db` on save changes save output vs Phase 1 — expected; the differential oracle + semantic round-trip tests are the parity proof.
- The dry-run report shape is deliberately general (added/overwritten/deleted) so Phase 4 (downgrade) and Phase 5 (merge) reuse it — design it for reuse now.

</code_context>

<specifics>
## Specific Ideas

- The real round-trips in Phase 1/3 grew in size (5.94→6.29 MB; 389.5→390.1 MB) precisely because trim_db + VACUUM did not exist yet. Phase 2 is where a saved archive gets *smaller/cleaner*, matching the Python app. A good post-Phase-2 check: re-run the real v14 round-trip and confirm the output is now trimmed (size closer to or below source), still Python-accepted.
- trim_db is the single most consequential correctness surface after the archive envelope — a wrong orphan sweep silently deletes user data. Semantic round-trip fixtures with multi-table orphans are non-negotiable.

</specifics>

<deferred>
## Deferred Ideas
- Delete/edit for the other five categories → Phase 6/7 (the delete mechanism generalizes).
- Color/tag/order/favorite/clean/raw edits → Phase 7.
- Merge + downgrade dry-runs reuse this phase's DryRunReport → Phases 4/5.
</deferred>

---

*Phase: 2-Safe Delete*
*Context gathered: 2026-07-21*
</content>
