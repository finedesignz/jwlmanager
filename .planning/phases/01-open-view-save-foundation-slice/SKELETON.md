# Walking Skeleton — JWL Manager (Tauri)

**Phase:** 1
**Generated:** 2026-07-19

## Capability Proven End-to-End

A user launches the Tauri app via `npm run tauri dev`, opens a synthetic v16 `.jwlibrary` fixture, and sees at least one real Note row (read from the archive's `userData.db`) rendered in the window — the full stack (React webview → Tauri IPC → Rust core → zip extract → rusqlite query) is exercised on the happy path. Opening populates a durable `ArchiveSession` that the save/save-as path later consumes.

## Core State Object — `ArchiveSession` (Tauri managed state)

The architectural spine of the phase is a single durable session object, held as Tauri managed state and populated by `open_archive` / `new_archive`, consumed by `save_archive` / `save_as`:

| Field | Purpose |
|---|---|
| `temp_dir: TempDir` | OWNS the extracted working copy for the whole session (must outlive open — dropping it after open would make save impossible) |
| `source_path: PathBuf` | The original opened file (read-only; never mutated in place — D-03) |
| `target_path: PathBuf` | Current save target (starts = source; follows the new path on save-as — D-05) |
| `db_path: PathBuf` | The extracted `userData.db` inside `temp_dir` |
| `manifest` | Parsed manifest metadata |
| `entries: Vec<ZipEntryMeta>` | FULL inventory of every original zip entry, so save round-trips loose media + unknown/forward-compat files (only `userData.db` + `manifest.json` are regenerated) |
| `dirty: bool` | Unsaved-changes flag |

This object is why save can be atomic AND lossless: it retains the working copy and the complete entry list across the open→edit→save lifecycle.

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| App shell | Tauri v2 (Rust core + web frontend) | Locked by PROJECT.md — replaces PySide6; Rust binds `jwlCore` via `libloading` |
| Frontend | Vite + React + TypeScript (D-09) | Widest maturity for the one hard requirement: virtualized lists at 9,000+ rows |
| List virtualization | `@tanstack/react-virtual` (D-10), fixed 44px single-line-truncated rows | WebKitGTK DOM performance collapse on Linux — 9k DOM nodes forbidden; single-line rows prevent fixed-size virtualizer mismeasure |
| SQLite access | `rusqlite` (feature `bundled`) | Window-function support needed for Phase 2 `trim_db`; sync fit for a single-user desktop app |
| Zip envelope | `zip` crate exact `2.x` pin **≥2.3.0** + committed `Cargo.lock` | CVE-2025-29787 symlink zip-slip variant fixed only at 2.3.0; `enclosed_name`-validated extraction; exact pin avoids future-major drift |
| Session state | `ArchiveSession` Tauri managed state (see above) | Save needs the working copy + full entry inventory + target path across the whole session; a bare `open() -> Vec<NotesRow>` cannot save safely |
| Schema acceptance (Phase 1) | v16 ONLY (`schemaVersion == 16` && `PRAGMA user_version == 16`) | Phase 1 has no upgrade path; accepting v12–15 would risk invalid output. SCHEMA-01/02 (Phase 3) widen this. Fixture + res/blank are v16 (D-08) |
| Errors | internal `thiserror` `ArchiveError` (wraps io/rusqlite/libloading) + sanitized `ErrorDto` over IPC (D-14) | thiserror enums wrapping source errors are not `Serialize`; only a sanitized DTO (code, operation, safe_file_name, message_key — no raw paths) crosses the boundary. Fixes the Python app's 29 bare `except:` |
| Category identity | Rust `enum` + `ts-rs` codegen (D-11) | Fixes `if category == _('Notes')` i18n control-flow bug; two sides cannot drift |
| Working copy | Temp-dir extraction, source read-only (D-03) | Core Value — a crash mid-session can never corrupt the opened file |
| Save | Same-dir temp + OS-correct atomic replace (D-04); full-inventory zip rebuild; delete-then-rename FORBIDDEN | Core Value — power loss during save leaves EITHER old OR new complete archive, never a truncated one; media + unknown entries preserved |
| New / fixtures | Seeded from `res/blank` (the real JW Library v16 empty seed) | Not a hand-built minimal DB — avoids the "false oracle / circular spec" risk; res/blank is already `user_version == 16` |
| jwlCore loading | `libloading`, arch-aware `(OS, ARCH)` table, load+resolve only (D-12/D-13); unified `JwlCoreStatus { loaded, arch, version, reason }` | Load is the part that fails on platform/arch mismatch (arm64-windows = `loaded: false`, not Err); merge call is Phase 5 |
| Directory layout | Tauri app in new `app/` subdir; Python app untouched at repo root (D-01) | Keeps upstream merges clean and the Python app available as the differential-test oracle |
| jwlCore source | Referenced from existing `libs/`, not vendored (D-02) | Two copies of a closed-source binary drift; one copy, one hash |

## Stack Touched in Phase 1

- [x] Project scaffold (Tauri v2 + Vite/React/TS, `cargo`/`npm`, `cargo test` + `vitest` w/ committed lockfiles, clippy/fmt)
- [x] Core state — `ArchiveSession` managed state owning the working copy + full zip-entry inventory
- [x] IPC command surface — real `open_archive`, `save_archive`, `save_as`, `new_archive`, `check_jwlcore` Tauri commands (all return `Result<_, ErrorDto>`)
- [x] Database — real read (`userData.db` Notes query via `rusqlite`) AND real write (save rebuilds the FULL archive atomically)
- [x] UI — virtualized Notes list (fixed 44px single-line rows) wired to the `open_archive` IPC command
- [x] Full-stack run — `npm run tauri dev` exercises webview → IPC → Rust → SQLite end to end; CI matrix builds all four targets

## Out of Scope (Deferred to Later Slices)

- `trim_db` on save (orphan sweep, tag re-densify, VACUUM) — ARCH-04, **Phase 2**
- Any destructive edit or delete + dry-run preview — EDIT-01/SAFE-01, **Phase 2**
- Schema version acceptance / upgrade / downgrade (v12–v15) — SCHEMA-*, **Phases 3–4** (Phase 1 is v16-ONLY)
- Calling `jwlCore` merge (Phase 1 only loads + resolves symbols) — MERGE-02, **Phase 5**
- The other five categories (Highlights, Bookmarks, Annotations, Favorites, Playlists) — DATA-02..06, **Phase 6**
- Import/export wire formats — IO-*, **Phase 8**
- Code signing, localization, theme switch — PLAT-02/03/04, **Phase 11**
- Duplicate-detection CTE branch of the Notes query (`self.dupes`) — later phase
- A native Windows arm64 `jwlCore` binary — does not exist upstream (D-13a); arm64 build ships open/view/save only

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without altering its architectural decisions:

- Phase 2: Safe delete with dry-run preview, transactions, `trim_db` on save
- Phase 3: Accept + upgrade schema v12–16 to v16 in memory (widens the Phase-1 v16-only gate)
- Phase 4: Explicit v14 downgrade save with the 7-table LocationId remap closure
- Phase 5: Two-archive merge via jwlCore with dry-run preview
- Phase 6: Browse + select across all six categories
- Phase 7: Full editing (colors, tags, order, favorites, clean/mask, raw editor)
- Phase 8: Import/export parity with the Python app's wire format
- Phase 9: Incremental export by content hash
- Phase 10: N-way merge fold
- Phase 11: Signing, localization, theme
</content>
