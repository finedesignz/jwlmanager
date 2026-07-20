# Walking Skeleton — JWL Manager (Tauri)

**Phase:** 1
**Generated:** 2026-07-19

## Capability Proven End-to-End

A user launches the Tauri app via `npm run tauri dev`, opens a synthetic v16 `.jwlibrary` fixture, and sees at least one real Note row (read from the archive's `userData.db`) rendered in the window — the full stack (React webview → Tauri IPC → Rust core → zip extract → rusqlite query) is exercised on the happy path.

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| App shell | Tauri v2 (Rust core + web frontend) | Locked by PROJECT.md — replaces PySide6; Rust binds `jwlCore` via `libloading` |
| Frontend | Vite + React + TypeScript (D-09) | Widest maturity for the one hard requirement: virtualized lists at 9,000+ rows |
| List virtualization | `@tanstack/react-virtual` (D-10) | WebKitGTK DOM performance collapse on Linux — 9k DOM nodes forbidden, not discouraged |
| SQLite access | `rusqlite` (feature `bundled`) | Window-function support needed for Phase 2 `trim_db`; sync fit for a single-user desktop app |
| Zip envelope | `zip` crate **≥2.3.0** | CVE-2025-29787 symlink zip-slip variant fixed only at 2.3.0; `enclosed_name`-validated extraction |
| Errors | `thiserror` typed enums serialized over IPC (D-14) | Fixes the Python app's 29 bare `except:` — silent swallowing becomes a compile-time impossibility |
| Category identity | Rust `enum` + `ts-rs` codegen (D-11) | Fixes `if category == _('Notes')` i18n control-flow bug; two sides cannot drift |
| Working copy | Temp-dir extraction, source read-only (D-03) | Core Value — a crash mid-session can never corrupt the opened file |
| Save | Write sibling temp + atomic rename (D-04) | Core Value — power loss during save leaves the old archive intact |
| jwlCore loading | `libloading`, arch-aware `(OS, ARCH)` table, load+resolve only (D-12/D-13) | Load is the part that fails on platform/arch mismatch; merge call is Phase 5 |
| Directory layout | Tauri app in new `app/` subdir; Python app untouched at repo root (D-01) | Keeps upstream merges clean and the Python app available as the differential-test oracle |
| jwlCore source | Referenced from existing `libs/`, not vendored (D-02) | Two copies of a closed-source binary drift; one copy, one hash |

## Stack Touched in Phase 1

- [x] Project scaffold (Tauri v2 + Vite/React/TS, `cargo`/`npm`, `cargo test` + `vitest`, clippy/fmt)
- [x] IPC command surface — real `open_archive`, `save_archive`, `new_archive`, `check_jwlcore` Tauri commands
- [x] Database — real read (`userData.db` Notes query via `rusqlite`) AND real write (save rebuilds the archive)
- [x] UI — virtualized Notes list wired to the `open_archive` IPC command
- [x] Full-stack run — `npm run tauri dev` exercises webview → IPC → Rust → SQLite end to end; CI matrix builds all four targets

## Out of Scope (Deferred to Later Slices)

- `trim_db` on save (orphan sweep, tag re-densify, VACUUM) — ARCH-04, **Phase 2**
- Any destructive edit or delete + dry-run preview — EDIT-01/SAFE-01, **Phase 2**
- Schema version acceptance / upgrade / downgrade (v12–v15) — SCHEMA-*, **Phases 3–4**
- Calling `jwlCore` merge (Phase 1 only loads + resolves symbols) — MERGE-02, **Phase 5**
- The other five categories (Highlights, Bookmarks, Annotations, Favorites, Playlists) — DATA-02..06, **Phase 6**
- Import/export wire formats — IO-*, **Phase 8**
- Code signing, localization, theme switch — PLAT-02/03/04, **Phase 11**
- Duplicate-detection CTE branch of the Notes query (`self.dupes`) — later phase
- A native Windows arm64 `jwlCore` binary — does not exist upstream (D-13a); arm64 build ships open/view/save only

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without altering its architectural decisions:

- Phase 2: Safe delete with dry-run preview, transactions, `trim_db` on save
- Phase 3: Accept + upgrade schema v12–16 to v16 in memory
- Phase 4: Explicit v14 downgrade save with the 7-table LocationId remap closure
- Phase 5: Two-archive merge via jwlCore with dry-run preview
- Phase 6: Browse + select across all six categories
- Phase 7: Full editing (colors, tags, order, favorites, clean/mask, raw editor)
- Phase 8: Import/export parity with the Python app's wire format
- Phase 9: Incremental export by content hash
- Phase 10: N-way merge fold
- Phase 11: Signing, localization, theme
