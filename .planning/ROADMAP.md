# Roadmap — JWL Manager (Tauri)

**Core Value:** Never lose or corrupt a user's archive.
**Mode:** MVP (vertical slices) — every phase ships an end-to-end, user-visible working capability.
**Granularity:** fine (11 phases)

## Phases

- [x] **Phase 1: Open, View, Save (Foundation Slice)** - Open a real archive, browse Notes, save back a file JW Library still opens; jwlCore loads; CI + fixtures exist from day one.
 (completed 2026-07-20)
- [ ] **Phase 2: Safe Delete (Dry-Run + Trim + Transactions)** - User can delete Notes with a dry-run preview, transactional safety, and correct post-save trim/VACUUM.
- [x] **Phase 3: Schema Upgrade** - Any accepted archive (v12–16) opens correctly, auto-upgraded to v16 in memory. (completed 2026-07-21)
- [ ] **Phase 4: Schema Downgrade** - User can explicitly save a v14-compatible archive with the LocationId remap closure, previewed via dry-run, and the working copy stays v16.
- [ ] **Phase 5: Two-Archive Merge** - User can merge two archives via jwlCore with a dry-run preview and matching results to the Python app.
- [ ] **Phase 6: Full Data Browsing** - User can view, browse, and select (single + bulk) across all 6 categories, with valid operations surfaced per selection.
- [ ] **Phase 7: Full Editing** - User can edit colors, tags, order, favorites, cleaning/masking, and raw records across all categories.
- [ ] **Phase 8: Import / Export Parity** - User can export/import any category in the Python app's exact wire format, with ID-gap recycling.
- [ ] **Phase 9: Incremental Export** - User can export only what changed since a chosen point, using content-hash identity.
- [ ] **Phase 10: N-Way Merge Fold** - User can merge more than two archives in a single ordered operation.
- [ ] **Phase 11: Platform Polish (Signing, Localization, Theme)** - Windows binaries are signed; user can switch language and theme.

## Phase Details

### Phase 1: Open, View, Save (Foundation Slice)
**Goal**: Prove the riskiest integration (archive envelope, schema, jwlCore loading) end-to-end before breadth — user opens a real `.jwlibrary` file, sees real Notes data, and saves it back.
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: ARCH-01, ARCH-02, ARCH-03, ARCH-05, ARCH-06, ARCH-07, DATA-01, DATA-08, SAFE-05, QA-01, QA-03, PLAT-01
**Success Criteria** (what must be TRUE):
  1. User can open a `.jwlibrary` archive and see its Notes listed, scrolling smoothly at 9,000+ rows
  2. User can save the archive and both JW Library and the existing Python app open it without error
  3. User can create a new empty archive and save-as to a chosen path without altering the original working copy
  4. Opening a path-traversal-crafted (zip-slip) archive is rejected, not silently extracted
  5. Fixture archives + a CI pipeline exist and run on every push; errors surface to the user with actionable messages, never silently
**Plans**: 7 plans (5 waves)
- [x] 01-01-PLAN.md — Walking Skeleton scaffold + Wave-0 harness: Tauri/React shell, full deps/tauri.conf, synthetic v16 fixture, RED e2e test (Wave 1)
- [x] 01-07-PLAN.md — Core primitives: typed errors, category enum (ts-rs), zip-slip-safe extract, raw Notes query, open_archive + thin render (Wave 2)
- [x] 01-02-PLAN.md — Byte-compatible manifest, zip-slip rejection, four-leg CI matrix + clippy unwrap ban (Wave 3)
- [x] 01-03-PLAN.md — Arch-aware jwlCore load+resolve only; arm64-windows typed no-binary error (Wave 3)
- [x] 01-04-PLAN.md — Real Notes: resources.db labels, independent-notes union, TanStack Virtual at 9k rows (Wave 4)
- [x] 01-05-PLAN.md — Save/Save-As/New: atomic rename, hash-last, Python-app differential oracle (Wave 4)
- [x] 01-06-PLAN.md — Command bar + typed-error surface + arm64 jwlCore capability notice (Wave 5)

### Phase 2: Safe Delete (Dry-Run + Trim + Transactions)
**Goal**: The first destructive operation ships with the safety net the whole app depends on — dry-run preview, transactional rollback, and correct trim behavior on save.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: ARCH-04, EDIT-01, SAFE-01, SAFE-02, SAFE-03, SAFE-04, QA-02
**Success Criteria** (what must be TRUE):
  1. Before deleting selected Notes, user sees a preview stating what will be deleted, with a cancel option
  2. After confirming delete and saving, orphans are swept, tag positions re-densified, and the DB is VACUUMed
  3. A failed delete mid-transaction leaves the archive unchanged (rollback verified by round-trip test)
  4. Empty selections cannot trigger a delete at all (impossible by construction, not just a disabled button)
  5. All SQL executed is parameterized; a round-trip semantic-equivalence test exists for the delete operation
**Plans**: TBD

### Phase 3: Schema Upgrade
**Goal**: Any archive a real user might hand the app (schema v12–16) opens correctly and is normalized to v16 in memory.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: SCHEMA-01, SCHEMA-02
**Success Criteria** (what must be TRUE):
  1. Opening a v12, v13, v14, v15, or v16 fixture archive succeeds and data displays correctly
  2. Opening a v11-or-earlier archive fails with a clear, actionable message instead of corrupting or crashing
  3. Any accepted archive is upgraded to working version 16 immediately on open, verified by round-trip test
**Plans**: 3 plans (3 waves)
- [x] 03-01-PLAN.md - Versioned fixture generator (v11-v15/v17) + typed error variants + frontend copy (Wave 1)
- [x] 03-02-PLAN.md - Transactional upgrade_to_v16 DDL port + range gate widening + full test matrix (Wave 2)
- [x] 03-03-PLAN.md - v14-upgrade differential oracle + env-gated real-v14 acceptance (recorded manual gates) (Wave 3)

### Phase 4: Schema Downgrade
**Goal**: User who needs v14 compatibility (older JW Library) can explicitly opt into a downgraded save without losing data integrity.
**Mode:** mvp
**Depends on**: Phase 2, Phase 3
**Requirements**: SCHEMA-03, SCHEMA-04, SCHEMA-05
**Success Criteria** (what must be TRUE):
  1. User can explicitly choose to save a v14-compatible archive (not a default/implicit path)
  2. Before the downgrade save, user sees a dry-run preview (reusing Phase 2's mechanism) of what the downgrade will change
  3. The 7-table LocationId remap closure (Bookmark ×2, Note, UserMark, InputField, TagMap, PlaylistItemLocationMap) produces a semantically correct v14 archive, verified by round-trip test
  4. After a v14 save, the app's working in-memory copy remains at v16 (backup/restore verified)
**Plans**: TBD

### Phase 5: Two-Archive Merge
**Goal**: User can merge two archives via the jwlCore native engine with the same safety net as any other destructive operation, and trust the result matches the proven Python app.
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: MERGE-01, MERGE-02, MERGE-04
**Success Criteria** (what must be TRUE):
  1. App loads the correct jwlCore binary for the host OS and CPU architecture (including arm64) automatically
  2. User sees a dry-run preview before merge (add/overwrite/delete counts) and can cancel
  3. Merging two fixture archives produces results matching the Python app's output for the same inputs, verified by semantic round-trip test
  4. If the native library is missing or fails to load, the user sees a clear, actionable error — not a crash
**Plans**: TBD

### Phase 6: Full Data Browsing
**Goal**: User can view and select across every category the archive holds, not just Notes.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: DATA-02, DATA-03, DATA-04, DATA-05, DATA-06, DATA-07
**Success Criteria** (what must be TRUE):
  1. User can browse Highlights, Bookmarks, Annotations, Favorites, and Playlists, each rendering real archive data
  2. User can select one or many items in any category
  3. The set of valid operations shown updates based on the current selection (e.g., bulk delete only appears when items are selected)
**Plans**: TBD
**UI hint**: yes

### Phase 7: Full Editing
**Goal**: User can perform every edit operation the Python app supports, across all categories, with the same safety guarantees established in Phase 2.
**Mode:** mvp
**Depends on**: Phase 6
**Requirements**: EDIT-02, EDIT-03, EDIT-04, EDIT-05, EDIT-06, EDIT-07
**Success Criteria** (what must be TRUE):
  1. User can change a highlight's color, and overlapping ranges are union-merged exactly as the Python app does
  2. User can add, remove, and rename tags, and reorder items using the two-pass negative-position technique (no TagMap uniqueness violations)
  3. User can mark/unmark items as favorites and clean/mask data
  4. User can open a raw data viewer/editor and directly edit underlying records
  5. Each of the above is covered by a round-trip semantic-equivalence test
**Plans**: TBD
**UI hint**: yes

### Phase 8: Import / Export Parity
**Goal**: User's existing export files (produced by the Python app, or shared with other users of it) remain interchangeable with this app in both directions.
**Mode:** mvp
**Depends on**: Phase 6
**Requirements**: IO-01, IO-02, IO-03
**Success Criteria** (what must be TRUE):
  1. User can export any category and the file preserves the exact wire warts (`'None'` sentinel, `|`→`¦` escaping, `==={END}===` sentinel, UTF-8 header)
  2. User can import a file the Python app produced, for any category, with data landing correctly
  3. Imported items recycle ID gaps the same way the Python app does, verified by round-trip test
**Plans**: TBD

### Phase 9: Incremental Export
**Goal**: User doing repeated exports (the #188 upstream ask) only has to review what actually changed.
**Mode:** mvp
**Depends on**: Phase 8
**Requirements**: IO-04
**Success Criteria** (what must be TRUE):
  1. User can choose a prior export point and export only items changed since then
  2. Note identity for the diff is resolved via content hashing, not vendor timestamps, so re-exports are stable even when timestamps drift
**Plans**: TBD

### Phase 10: N-Way Merge Fold
**Goal**: User with more than two archives to reconcile (a documented multi-device pain point) can do it in one operation instead of chaining pairwise merges.
**Mode:** mvp
**Depends on**: Phase 5
**Requirements**: MERGE-03
**Success Criteria** (what must be TRUE):
  1. User can select 3+ archives and merge them in one ordered fold operation
  2. The dry-run preview from Phase 5 extends to show the cumulative effect across all inputs
  3. Result matches performing the equivalent sequence of pairwise merges, verified by round-trip test
**Plans**: TBD

### Phase 11: Platform Polish (Signing, Localization, Theme)
**Goal**: The app is distributable and comfortable for real-world daily use across platforms and languages.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: PLAT-02, PLAT-03, PLAT-04
**Success Criteria** (what must be TRUE):
  1. Windows release binaries are Authenticode-signed via Azure Trusted Signing as part of the bundling step (not a post-build pass)
  2. User can switch UI language and all user-facing strings render translated
  3. User can switch theme (light/dark) and the change applies immediately across the app
**Plans**: TBD
**UI hint**: yes

## Phase Details — Progress Table

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Open, View, Save | 7/7 | Complete   | 2026-07-20 |
| 2. Safe Delete | 0/TBD | Not started | - |
| 3. Schema Upgrade | 3/3 | Complete   | 2026-07-21 |
| 4. Schema Downgrade | 0/TBD | Not started | - |
| 5. Two-Archive Merge | 0/TBD | Not started | - |
| 6. Full Data Browsing | 0/TBD | Not started | - |
| 7. Full Editing | 0/TBD | Not started | - |
| 8. Import / Export Parity | 0/TBD | Not started | - |
| 9. Incremental Export | 0/TBD | Not started | - |
| 10. N-Way Merge Fold | 0/TBD | Not started | - |
| 11. Platform Polish | 0/TBD | Not started | - |

## Coverage

47/47 v1 requirements mapped, no orphans, no duplicates. See `.planning/REQUIREMENTS.md` Traceability table.
