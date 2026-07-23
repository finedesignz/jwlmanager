# Requirements — JWL Manager (Tauri) v1

**Milestone goal:** A Tauri app that fully replaces the PySide6 JWL Manager for real use, plus the dry-run safety net and incremental export that users have asked for.

**Core Value guardrail:** Never lose or corrupt a user's archive. Every requirement below is subordinate to that.

**Derivation:** Parity requirements are traced to `.planning/research/FUNCTIONALITY-SPEC.md` (line-cited against the working Python app). New-value requirements are traced to `.planning/research/FEATURE-IDEAS.md` (evidence from 277 upstream issues).

---

## v1 Requirements

### Archive Core (ARCH)

- [x] **ARCH-01**: App can open a `.jwlibrary` archive and list its contents (zip envelope: `manifest.json` + `userData.db` + loose media)
- [x] **ARCH-02**: App can save an archive that JW Library and the existing Python app both open without error
- [x] **ARCH-03**: App writes `manifest.json` byte-compatibly (compact `separators=(',',':')`, `hash` = sha256 of final DB bytes, `schemaVersion` read from `PRAGMA user_version`, `creationDate` refreshed on save)
- [ ] **ARCH-04**: App runs `trim_db` on save (orphan sweep, tag position re-densify via ROW_NUMBER, `Location.Title=""` where NULL, VACUUM)
- [x] **ARCH-05**: App rejects archive paths that escape the extraction root (zip-slip protection)
- [x] **ARCH-06**: App can create a new empty archive
- [x] **ARCH-07**: App can save-as to a user-chosen path without mutating the working copy

### Schema (SCHEMA)

- [x] **SCHEMA-01**: App accepts schema versions 12–16 and rejects ≤11 with a clear message
- [x] **SCHEMA-02**: App upgrades any accepted archive to working version 16 on open
- [x] **SCHEMA-03**: App can save a v14 downgrade (the `Location.Specialty`/`Edition` + `IX_Location_Media` index delta) via an explicit user choice
- [x] **SCHEMA-04**: Downgrade performs the 7-table LocationId remap closure (Bookmark ×2 cols, Note, UserMark, InputField, TagMap, PlaylistItemLocationMap) with explicit, documented, tested ordering semantics
- [x] **SCHEMA-05**: Working copy remains at v16 after a v14 save (backup/restore)

### Merge (MERGE)

- [x] **MERGE-01**: App loads the `jwlCore` native library, selecting the correct binary for the host OS **and CPU architecture**
- [x] **MERGE-02**: User can merge two archives via `jwlCore` and the result matches what the Python app produces for the same inputs
- [ ] **MERGE-03**: User can merge N archives in one operation (ordered fold)
- [ ] **MERGE-04**: App surfaces a clear, actionable error when the native library is missing or fails to load

### Data Views (DATA)

- [x] **DATA-01**: User can view and browse Notes, with the list staying responsive at 9,000+ rows (virtualized)
- [x] **DATA-02**: User can view and browse Highlights
- [x] **DATA-03**: User can view and browse Bookmarks
- [x] **DATA-04**: User can view and browse Annotations
- [x] **DATA-05**: User can view and browse Favorites
- [x] **DATA-06**: User can view and browse Playlists
- [ ] **DATA-07**: User can select items (individually and in bulk) and see the valid operations for that selection
- [x] **DATA-08**: App identifies categories by stable enums, never by translated display strings

### Editing (EDIT)

- [x] **EDIT-01**: User can delete selected items from any category
- [ ] **EDIT-02**: User can change highlight colors, with overlapping ranges union-merged as the Python app does
- [ ] **EDIT-03**: User can add, remove, and rename tags
- [ ] **EDIT-04**: User can reorder items (preserving the two-pass negative-position technique that dodges TagMap uniqueness)
- [ ] **EDIT-05**: User can mark items as favorites
- [ ] **EDIT-06**: User can clean/mask data
- [ ] **EDIT-07**: User can view and edit underlying records directly (data viewer/editor)

### Import / Export (IO)

- [ ] **IO-01**: User can export any category to the Python app's existing formats, preserving its wire warts (`'None'` null sentinel, `|`→`¦` escaping, `==={END}===` sentinel, UTF-8 header forcing)
- [ ] **IO-02**: User can import any category from files the Python app produces
- [ ] **IO-03**: Import recycles ID gaps as the Python app does
- [ ] **IO-04**: User can export only items changed since a chosen point (incremental export), with note identity resolved by content hashing rather than vendor timestamps

### Safety (SAFE)

- [x] **SAFE-01**: Before any destructive operation (merge, import, delete, downgrade), user sees a dry-run preview stating what will be added, overwritten, and deleted, with a cancel option
- [x] **SAFE-02**: All SQL is parameterized — no string-interpolated values, including IN-clauses
- [x] **SAFE-03**: Empty-selection operations are impossible by construction, not merely guarded by button state
- [x] **SAFE-04**: Every archive-mutating operation runs in a transaction that rolls back cleanly on failure
- [x] **SAFE-05**: Errors surface to the user with actionable context — no silently swallowed exceptions

### Quality (QA)

- [x] **QA-01**: Fixture `.jwlibrary` archives exist covering each accepted schema version and are used by automated tests
- [x] **QA-02**: Every archive-mutating operation has a round-trip test asserting semantic (normalized-table) equivalence — never byte equality
- [x] **QA-03**: Tests run in CI on every push

### Platform (PLAT)

- [x] **PLAT-01**: App builds and runs on Windows (x64 + arm64), macOS, and Linux
- [ ] **PLAT-02**: Windows binaries are Authenticode-signed via Azure Trusted Signing during bundling
- [ ] **PLAT-03**: User can switch UI language; all user-facing strings are localized
- [ ] **PLAT-04**: User can switch theme

---

## v2 Requirements (deferred)

- **Full-text search across notes** — high value, but no user has actually filed it; validate demand before building
- **Round-trip markdown editing** — most-wanted by inference, deliberately gated behind SAFE-01 (dry-run diff) existing first
- **Standalone Rust CLI over the same core** — the ecosystem's real unlock (#188's user wants a cron-able binary), but the GUI must prove the core first
- **Tauri mobile (iOS/Android)** — Tauri v2 mobile is not yet first-class; revisit when plugin gaps close
- **Auto-update** — deferred by explicit decision; target users prefer a frozen known-good build

## Out of Scope

- **Bundling/caching publication text** — copyrighted content, not user data. Legal bright line.
- **Cloud sync, AI features, telemetry** — GDPR Art. 9 special-category data (religious affiliation). Decided once, at the architecture level.
- **Writing to JW Library's live database** — requires reverse-engineering undocumented vendor behavior against irreplaceable data.
- **Reimplementing merge/de-dup/conflict logic** — `jwlCore` is the sanctioned prebuilt engine, already shared by both apps.
- **Ingesting jwlFusion or sibling-repo source** — license conflict (Infiniti Noncommercial / `NOASSERTION` vs MIT); 5 of 7 publish no source anyway.
- **Byte-for-byte save reproduction** — impossible by design; `trim_db`+VACUUM make save non-byte-preserving. Semantic equivalence is the standard.

---

## Traceability

<!-- Filled by roadmap creation. -->

| REQ-ID | Phase |
|--------|-------|
| ARCH-01 | Phase 1 |
| ARCH-02 | Phase 1 |
| ARCH-03 | Phase 1 |
| ARCH-04 | Phase 2 |
| ARCH-05 | Phase 1 |
| ARCH-06 | Phase 1 |
| ARCH-07 | Phase 1 |
| SCHEMA-01 | Phase 3 |
| SCHEMA-02 | Phase 3 |
| SCHEMA-03 | Phase 4 |
| SCHEMA-04 | Phase 4 |
| SCHEMA-05 | Phase 4 |
| MERGE-01 | Phase 5 |
| MERGE-02 | Phase 5 |
| MERGE-03 | Phase 10 |
| MERGE-04 | Phase 5 |
| DATA-01 | Phase 1 |
| DATA-02 | Phase 6 |
| DATA-03 | Phase 6 |
| DATA-04 | Phase 6 |
| DATA-05 | Phase 6 |
| DATA-06 | Phase 6 |
| DATA-07 | Phase 6 |
| DATA-08 | Phase 1 |
| EDIT-01 | Phase 2 |
| EDIT-02 | Phase 7 |
| EDIT-03 | Phase 7 |
| EDIT-04 | Phase 7 |
| EDIT-05 | Phase 7 |
| EDIT-06 | Phase 7 |
| EDIT-07 | Phase 7 |
| IO-01 | Phase 8 |
| IO-02 | Phase 8 |
| IO-03 | Phase 8 |
| IO-04 | Phase 9 |
| SAFE-01 | Phase 2 |
| SAFE-02 | Phase 2 |
| SAFE-03 | Phase 2 |
| SAFE-04 | Phase 2 |
| SAFE-05 | Phase 1 |
| QA-01 | Phase 1 |
| QA-02 | Phase 2 |
| QA-03 | Phase 1 |
| PLAT-01 | Phase 1 |
| PLAT-02 | Phase 11 |
| PLAT-03 | Phase 11 |
| PLAT-04 | Phase 11 |
