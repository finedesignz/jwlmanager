// Crate-level lint gate (D-15): no `.unwrap()`/`.expect()` on archive-data paths.
// Later plans (01-03, 01-05, 01-07) implement commands under this same crate and
// must not silently regress this gate.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod archive;
pub mod category;
pub mod db;
pub mod error;
pub mod guid;
pub mod jwlcore;
pub mod session;
pub mod settings;
pub mod time;

use category::Category;
use db::color::{ColorSelection, NonEmptyBlockRangeIds};
use db::delete::{NonEmptyBookmarkIds, NonEmptyLocationIds, NonEmptyNoteIds};
use db::edit::DryRunReport;
use db::favorites::{FavoriteEditionRef, NonEmptyTagMapIds};
use db::ids::compute_available_ids;
use db::io::diff::{
    export_annotations_incremental as export_annotations_incremental_impl,
    export_bookmarks_incremental as export_bookmarks_incremental_impl,
    export_favorites_incremental as export_favorites_incremental_impl,
    export_highlights_incremental as export_highlights_incremental_impl,
    export_notes_incremental as export_notes_incremental_impl, IncrementalExportSummary,
};
use db::io::export::{
    export_annotations as export_annotations_impl, export_bookmarks as export_bookmarks_impl,
    export_favorites as export_favorites_impl, export_highlights as export_highlights_impl,
    export_notes as export_notes_impl,
};
use db::io::header::ExportHeaderCtx;
use db::io::import::{
    apply_import_annotations, apply_import_bookmarks, apply_import_favorites,
    apply_import_highlights, apply_import_notes, dry_run_import_annotations,
    dry_run_import_bookmarks, dry_run_import_favorites, dry_run_import_highlights,
    dry_run_import_notes, parse_annotations_file, parse_bookmarks_file, parse_favorites_file,
    parse_highlights_file, parse_notes_file,
};
use db::notes::BrowseRow;
use db::playlist_io::NonEmptyPlaylistItemIds;
use db::record_edit::{RecordEditFields, RecordEditPayload, RecordIdentity};
use db::tags::TagState;
use error::ErrorDto;
use serde::Serialize;
use session::{ArchiveSession, SessionState};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use ts_rs::TS;

/// Wall-clock-derived seed for [`guid::format_guid_v4`], threaded through
/// exactly like `now: &str` is threaded at `save_archive`/`save_as` above —
/// the command layer is the ONLY place that reaches for real time; every
/// core `db::color` function takes `guid_seed: u64` as a plain parameter so
/// tests can supply a fixed literal instead (07-RESEARCH.md Shared Pattern 6).
fn guid_seed_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Wall-clock-derived seed for [`db::scrub::obscure_text`]'s [`SplitMix64`],
/// threaded through exactly like [`guid_seed_now`] — Mask's `dry_run`/
/// `apply` command pair shares ONE seed per user action so the preview's
/// counts and shape stay consistent with what `mask_apply` actually writes.
/// A distinct function (not a reuse of `guid_seed_now`) so a future change
/// to one seed source can't silently perturb the other.
fn mask_seed_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Fixed app identity used for `manifest.json`'s `name`/`deviceName` fields
/// on every save (mirrors `JWLManager.py:28-29`'s `APP`/`VERSION` constants).
/// Kept as a literal here rather than pulling `tauri.conf.json`'s
/// `productName`/`version` at runtime — Phase 1 has no update-channel
/// concept yet, and the manifest field is informational only (never
/// interpreted by JW Library or this app on read).
const APP_NAME: &str = "JWL Manager";
const APP_DEVICE_NAME: &str = "JWL Manager_0.1.0";

/// Extracts, validates (v16-only gate), and queries `path`, storing the
/// resulting `ArchiveSession` in managed state and returning the Notes rows
/// for the frontend's first render. Every internal `ArchiveError` is mapped
/// to a sanitized `ErrorDto` at this IPC boundary (D-14, SAFE-05) — never the
/// raw error, never the absolute path.
#[tauri::command]
fn open_archive(
    path: String,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<Vec<BrowseRow>, ErrorDto> {
    let path_buf = PathBuf::from(&path);
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|err| err.to_dto("open_archive", Some(path_buf.as_path())))?;
    let (session, notes) = archive::open_and_validate(&path_buf, &resources_db_path)
        .map_err(|err| err.to_dto("open_archive", Some(path_buf.as_path())))?;

    let mut guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("open_archive", Some(path_buf.as_path()))
    })?;
    *guard = Some(session);

    Ok(notes)
}

/// Re-queries the Notes rows for the currently open session WITHOUT reopening
/// a file. Used after `merge_commit` mutates the session DB in place so the
/// frontend can re-render the merged Notes list (MERGE-02, "Confirm ...
/// refreshes"). Requires an archive to already be open.
#[tauri::command]
fn list_notes(
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<Vec<BrowseRow>, ErrorDto> {
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|err| err.to_dto("list_notes", None))?;
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("list_notes", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("list_notes", None))?;

    archive::reload_notes(session, &resources_db_path).map_err(|err| err.to_dto("list_notes", None))
}

/// Re-queries the browse rows for the currently open session for ANY category,
/// WITHOUT reopening a file — the single generic category-switch dispatch
/// (D6-09). Keyed by the ts-rs-exported [`Category`] enum, NEVER a translated
/// display string (the enum exists precisely to kill the Python
/// `if category == _('Notes')` i18n control-flow bug class). Requires an
/// archive to already be open. Mirrors `list_notes`'s session-lock +
/// `resolve_resources_db_path` + `ResourceCatalog::load` + `ErrorDto` mapping.
#[tauri::command]
fn list_category(
    category: Category,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<Vec<BrowseRow>, ErrorDto> {
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|err| err.to_dto("list_category", None))?;
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("list_category", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("list_category", None))?;

    // Same connection-acquisition as `archive::reload_notes`: open the session
    // DB read handle, load the resource catalog, dispatch to the getter.
    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("list_category", Some(session.target_path.as_path()))
    })?;
    let catalog = db::resources::ResourceCatalog::load(&resources_db_path, "en")
        .map_err(|err| err.to_dto("list_category", None))?;

    match category {
        Category::Notes => db::notes::query_notes(&conn, &catalog),
        Category::Bookmarks => db::browse::query_bookmarks(&conn, &catalog),
        Category::Favorites => db::browse::query_favorites(&conn, &catalog),
        Category::Highlights => db::browse::query_highlights(&conn, &catalog),
        Category::Annotations => db::browse::query_annotations(&conn, &catalog),
        Category::Playlists => db::browse::query_playlists(&conn, &catalog),
    }
    .map_err(|err| err.to_dto("list_category", None))
}

/// Saves the currently open session back to its own `target_path` (D-04:
/// same-directory temp + atomic replace, full-inventory rebuild, hash-last
/// manifest). Requires an archive to already be open (`open_archive` or
/// `new_archive` must have populated the session first).
#[tauri::command]
fn save_archive(state: tauri::State<SessionState>) -> Result<(), ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("save_archive", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("save_archive", None))?;

    archive::save::save_archive(session, APP_NAME, APP_DEVICE_NAME, &time::now_iso8601_utc())
        .map_err(|err| err.to_dto("save_archive", Some(session.target_path.as_path())))?;
    Ok(())
}

/// Saves the currently open session to a NEW chosen path (D-05: original
/// untouched, session target follows the new path on success).
#[tauri::command]
fn save_as(path: String, state: tauri::State<SessionState>) -> Result<(), ErrorDto> {
    let new_target = PathBuf::from(&path);
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("save_as", None))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("save_as", None))?;

    archive::new::save_as(
        session,
        &new_target,
        APP_NAME,
        APP_DEVICE_NAME,
        &time::now_iso8601_utc(),
    )
    .map_err(|err| err.to_dto("save_as", Some(new_target.as_path())))?;

    // Session target follows the new path only after a successful save
    // (D-05) — the source file itself was never touched by save_as.
    session.target_path = new_target;
    session.dirty = false;
    Ok(())
}

/// Creates a brand-new empty v16 archive (seeded from `res/blank`, finding 5)
/// and installs it as the current session, ready to be saved to `path`.
#[tauri::command]
fn new_archive(path: String, state: tauri::State<SessionState>) -> Result<(), ErrorDto> {
    let target = PathBuf::from(&path);
    let session =
        archive::new::new_archive(&target, APP_NAME, APP_DEVICE_NAME, &time::now_iso8601_utc())
            .map_err(|err| err.to_dto("new_archive", Some(target.as_path())))?;

    let mut guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("new_archive", Some(target.as_path()))
    })?;
    *guard = Some(session);
    Ok(())
}

/// Previews the effect of deleting the given `Note` selection WITHOUT
/// mutating the working copy (SAFE-01): opens the session's `db_path`,
/// runs the real delete + trim inside a rolled-back transaction, and
/// returns the resulting semantic [`DryRunReport`]. `ids` cannot be empty —
/// an empty array fails IPC deserialization before this command body ever
/// runs (SAFE-03, D2-06), because [`NonEmptyNoteIds`] is the parameter type.
#[tauri::command]
fn delete_notes_dry_run(
    ids: NonEmptyNoteIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("delete_notes_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("delete_notes_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("delete_notes_dry_run", Some(session.target_path.as_path()))
    })?;

    db::delete::dry_run_delete_notes(&mut conn, &ids)
        .map_err(|err| err.to_dto("delete_notes_dry_run", Some(session.target_path.as_path())))
}

/// Applies the delete of the given `Note` selection — a single
/// `DELETE FROM Note` committed inside its own transaction (SAFE-02,
/// SAFE-04). Returns a report reflecting the DIRECT Note delete only; the
/// orphan sweep (UserMark/BlockRange/TagMap/Tag/Location) happens later, on
/// save, via `trim_db` — the caller already saw the FULL effect via
/// `delete_notes_dry_run` before confirming. Marks the session dirty on
/// success.
#[tauri::command]
fn delete_notes_apply(
    ids: NonEmptyNoteIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("delete_notes_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("delete_notes_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("delete_notes_apply", Some(session.target_path.as_path()))
    })?;

    // Mirrors `JWLManager.py:3681`/`trim_db`: Note deletion must run with
    // `foreign_keys` OFF (TagMap.NoteId still references the row being
    // deleted until trim sweeps it on save), restored via `PragmaGuard`.
    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("delete_notes_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("delete_notes_apply", Some(session.target_path.as_path()))
        })?;

    // `unchecked_transaction` (shared `&self`) because `guard_pragma` already
    // holds a shared borrow of `conn` for the duration of this function —
    // same pattern as `trim_db`/`dry_run_delete_notes`.
    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("delete_notes_apply", Some(session.target_path.as_path()))
    })?;
    let deleted = db::delete::delete_notes(&tx, &ids)
        .map_err(|err| err.to_dto("delete_notes_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("delete_notes_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut deleted_map = BTreeMap::new();
    if deleted > 0 {
        deleted_map.insert("Note".to_string(), deleted);
    }
    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: BTreeMap::new(),
        deleted: deleted_map,
        total_deleted: deleted,
        skipped: BTreeMap::new(),
    })
}

/// Previews the effect of unmarking the given Favorites selection WITHOUT
/// mutating the working copy (SAFE-01, EDIT-05): opens the session's
/// `db_path`, runs the real unmark + trim inside a rolled-back transaction,
/// and returns the resulting semantic [`DryRunReport`]. `ids` cannot be
/// empty — an empty array fails IPC deserialization before this command
/// body ever runs, because [`NonEmptyTagMapIds`] is the parameter type.
#[tauri::command]
fn favorite_remove_dry_run(
    ids: NonEmptyTagMapIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("favorite_remove_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("favorite_remove_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "favorite_remove_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    db::favorites::dry_run_favorite_remove(&mut conn, &ids).map_err(|err| {
        err.to_dto(
            "favorite_remove_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies the unmark of the given Favorites selection — a single
/// `DELETE FROM TagMap` committed inside its own transaction (EDIT-05).
/// Marks the session dirty on success.
#[tauri::command]
fn favorite_remove_apply(
    ids: NonEmptyTagMapIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("favorite_remove_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("favorite_remove_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_remove_apply", Some(session.target_path.as_path()))
    })?;

    // Defensive/uniform PragmaGuard + `foreign_keys` OFF around every edit
    // apply, matching `delete_notes_apply`'s established shape. Not
    // load-bearing here the way it is for Note delete (nothing references
    // `TagMap.TagMapId` as a foreign key, so a plain TagMap delete never
    // trips FK enforcement) — kept for uniformity so a future
    // favorites-adjacent migration can't silently reintroduce the hazard
    // unnoticed.
    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_remove_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("favorite_remove_apply", Some(session.target_path.as_path()))
        })?;

    // `unchecked_transaction` (shared `&self`) because `guard_pragma` already
    // holds a shared borrow of `conn` for the duration of this function —
    // same pattern as `trim_db`/`delete_notes_apply`.
    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_remove_apply", Some(session.target_path.as_path()))
    })?;
    let removed = db::favorites::apply_favorite_remove(&tx, &ids)
        .map_err(|err| err.to_dto("favorite_remove_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_remove_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut deleted_map = BTreeMap::new();
    if removed > 0 {
        deleted_map.insert("TagMap".to_string(), removed);
    }
    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: BTreeMap::new(),
        deleted: deleted_map,
        total_deleted: removed,
        skipped: BTreeMap::new(),
    })
}

/// Every display language name the Favorite Dialog's Language `<select>` can
/// offer (07-01-PLAN.md Task 2/3, EDIT-05 mark) — the full bundled
/// `Languages` catalog, NOT narrowed to languages that currently have a
/// favorite-eligible edition (`ResourceCatalog::all_language_names`'s doc
/// comment explains why the narrower Python behavior isn't ported here). No
/// open archive session required: `resources.db` is bundled app-wide, not
/// archive-specific — same reason `open_archive` resolves it independently
/// of session state.
#[tauri::command]
fn list_favorite_languages(app: tauri::AppHandle) -> Result<Vec<String>, ErrorDto> {
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|err| err.to_dto("list_favorite_languages", None))?;
    let catalog = db::resources::ResourceCatalog::load(&resources_db_path, "en")
        .map_err(|err| err.to_dto("list_favorite_languages", None))?;
    Ok(catalog
        .all_language_names()
        .into_iter()
        .map(str::to_string)
        .collect())
}

/// The Bible editions available to favorite for a display `language`
/// (`Favorites.Lang`) — the Favorite Dialog's edition list, filtered by
/// whichever language the user picked from `list_favorite_languages`. Empty
/// (never an error) when the language has no favorite-eligible editions
/// (07-UI-SPEC.md's "No editions found for {Language}" empty state).
#[tauri::command]
fn list_favorite_editions(
    language: String,
    app: tauri::AppHandle,
) -> Result<Vec<db::resources::FavoriteEdition>, ErrorDto> {
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|err| err.to_dto("list_favorite_editions", None))?;
    let catalog = db::resources::ResourceCatalog::load(&resources_db_path, "en")
        .map_err(|err| err.to_dto("list_favorite_editions", None))?;
    Ok(catalog.load_favorite_editions(&language))
}

/// Previews the effect of marking the given Bible edition as a Favorite
/// WITHOUT mutating the working copy (SAFE-01, EDIT-05): opens the session's
/// `db_path`, runs the real mark + trim inside a rolled-back transaction, and
/// returns the resulting semantic [`DryRunReport`]. A duplicate favorite
/// surfaces as a `favorite_duplicate` typed error here too, never a raw
/// constraint violation (07-PATTERNS.md Correction #3).
#[tauri::command]
fn favorite_add_dry_run(
    edition: FavoriteEditionRef,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("favorite_add_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("favorite_add_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_add_dry_run", Some(session.target_path.as_path()))
    })?;

    db::favorites::dry_run_favorite_add(&mut conn, &edition)
        .map_err(|err| err.to_dto("favorite_add_dry_run", Some(session.target_path.as_path())))
}

/// Applies marking the given Bible edition as a Favorite — ensures the
/// system tag/Location (creating either if absent) and inserts one `TagMap`
/// row, all committed inside one transaction (EDIT-05). Marks the session
/// dirty on success.
#[tauri::command]
fn favorite_add_apply(
    edition: FavoriteEditionRef,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("favorite_add_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("favorite_add_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_add_apply", Some(session.target_path.as_path()))
    })?;

    // Defensive/uniform PragmaGuard + `foreign_keys` OFF around every edit
    // apply, matching `favorite_remove_apply`'s established shape.
    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_add_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("favorite_add_apply", Some(session.target_path.as_path()))
        })?;

    // `unchecked_transaction` (shared `&self`) because `guard_pragma` already
    // holds a shared borrow of `conn` for the duration of this function —
    // same pattern as `favorite_remove_apply`/`delete_notes_apply`.
    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_add_apply", Some(session.target_path.as_path()))
    })?;
    let report = db::favorites::apply_favorite_add_reporting(&tx, &edition)
        .map_err(|err| err.to_dto("favorite_add_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("favorite_add_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(report)
}

/// Previews the effect of a recolor WITHOUT mutating the working copy
/// (SAFE-01, EDIT-02): opens the session's `db_path`, runs the real
/// `apply_color` + `trim_sweep` inside a rolled-back transaction, and returns
/// the resulting semantic [`DryRunReport`]. `selection` cannot be empty per
/// category — an empty array fails IPC deserialization before this command
/// body ever runs, because [`ColorSelection`]'s per-variant `ids` field is a
/// typed non-empty wrapper.
#[tauri::command]
fn color_dry_run(
    selection: ColorSelection,
    color_index: i64,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("color_dry_run", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("color_dry_run", None))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("color_dry_run", Some(session.target_path.as_path()))
    })?;

    db::color::dry_run_color(&mut conn, &selection, color_index, guid_seed_now())
        .map_err(|err| err.to_dto("color_dry_run", Some(session.target_path.as_path())))
}

/// Applies a recolor — the committed counterpart to [`color_dry_run`]
/// (EDIT-02). Marks the session dirty on success.
#[tauri::command]
fn color_apply(
    selection: ColorSelection,
    color_index: i64,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("color_apply", None))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("color_apply", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("color_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto("color_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("color_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto("color_apply", Some(session.target_path.as_path()))
    })?;
    let report = db::color::apply_color_reporting(&tx, &selection, color_index, guid_seed_now())
        .map_err(|err| err.to_dto("color_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto("color_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(report)
}

/// Previews the effect of deleting the given Highlights selection WITHOUT
/// mutating the working copy (SAFE-01, D7-10): removes `BlockRange` rows
/// only, never `UserMark` (rule #9).
#[tauri::command]
fn highlight_delete_dry_run(
    ids: NonEmptyBlockRangeIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("highlight_delete_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("highlight_delete_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "highlight_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    db::delete::dry_run_delete_highlights(&mut conn, &ids).map_err(|err| {
        err.to_dto(
            "highlight_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies the delete of the given Highlights selection — a single
/// `DELETE FROM BlockRange` committed inside its own transaction (D7-10).
/// Marks the session dirty on success.
#[tauri::command]
fn highlight_delete_apply(
    ids: NonEmptyBlockRangeIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("highlight_delete_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("highlight_delete_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "highlight_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "highlight_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto(
                "highlight_delete_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "highlight_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let deleted = db::delete::delete_highlights(&tx, &ids).map_err(|err| {
        err.to_dto(
            "highlight_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "highlight_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut deleted_map = BTreeMap::new();
    if deleted > 0 {
        deleted_map.insert("BlockRange".to_string(), deleted);
    }
    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: BTreeMap::new(),
        deleted: deleted_map,
        total_deleted: deleted,
        skipped: BTreeMap::new(),
    })
}

/// Previews the effect of deleting the given Bookmarks selection WITHOUT
/// mutating the working copy (SAFE-01, D7-10): removes `Bookmark` rows,
/// identity = `BookmarkId` (`browse.rs:33-37`, NOT the first-SELECTed
/// `LocationId`).
#[tauri::command]
fn bookmark_delete_dry_run(
    ids: NonEmptyBookmarkIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("bookmark_delete_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("bookmark_delete_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "bookmark_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    db::delete::dry_run_delete_bookmarks(&mut conn, &ids).map_err(|err| {
        err.to_dto(
            "bookmark_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies the delete of the given Bookmarks selection — a single `DELETE
/// FROM Bookmark` committed inside its own transaction (D7-10). Marks the
/// session dirty on success.
#[tauri::command]
fn bookmark_delete_apply(
    ids: NonEmptyBookmarkIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("bookmark_delete_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("bookmark_delete_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("bookmark_delete_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("bookmark_delete_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("bookmark_delete_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("bookmark_delete_apply", Some(session.target_path.as_path()))
    })?;
    let deleted = db::delete::delete_bookmarks(&tx, &ids)
        .map_err(|err| err.to_dto("bookmark_delete_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("bookmark_delete_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut deleted_map = BTreeMap::new();
    if deleted > 0 {
        deleted_map.insert("Bookmark".to_string(), deleted);
    }
    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: BTreeMap::new(),
        deleted: deleted_map,
        total_deleted: deleted,
        skipped: BTreeMap::new(),
    })
}

/// Previews the effect of deleting the given Annotations selection (by
/// `LocationId`) WITHOUT mutating the working copy (SAFE-01, D7-10): removes
/// ALL `InputField` rows at each selected location — an intentional
/// over-deletion (rule #10) the returned report surfaces truthfully via its
/// `InputField` count.
#[tauri::command]
fn annotation_delete_dry_run(
    ids: NonEmptyLocationIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("annotation_delete_dry_run", None)
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("annotation_delete_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "annotation_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    db::delete::dry_run_delete_annotations(&mut conn, &ids).map_err(|err| {
        err.to_dto(
            "annotation_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies the delete of the given Annotations selection (by `LocationId`) —
/// a single `DELETE FROM InputField` committed inside its own transaction
/// (D7-10, rule #10). Marks the session dirty on success.
#[tauri::command]
fn annotation_delete_apply(
    ids: NonEmptyLocationIds,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("annotation_delete_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("annotation_delete_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "annotation_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "annotation_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto(
                "annotation_delete_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "annotation_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let deleted = db::delete::delete_annotations(&tx, &ids).map_err(|err| {
        err.to_dto(
            "annotation_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "annotation_delete_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut deleted_map = BTreeMap::new();
    if deleted > 0 {
        deleted_map.insert("InputField".to_string(), deleted);
    }
    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: BTreeMap::new(),
        deleted: deleted_map,
        total_deleted: deleted,
        skipped: BTreeMap::new(),
    })
}

/// Fetches the current field values for one record so the Record Editor can
/// prefill them (EDIT-07) — `BrowseRow` never carries a Note's Title/Content
/// or an Annotation's Value (see `db::record_edit` module docs).
#[tauri::command]
fn record_fetch(
    identity: RecordIdentity,
    state: tauri::State<SessionState>,
) -> Result<RecordEditFields, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("record_fetch", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("record_fetch", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("record_fetch", Some(session.target_path.as_path()))
    })?;

    db::record_edit::fetch_record_fields(&conn, &identity)
        .map_err(|err| err.to_dto("record_fetch", Some(session.target_path.as_path())))
}

/// Previews saving a record edit WITHOUT mutating the working copy (SAFE-01,
/// EDIT-07, D7-09): opens the session's `db_path`, runs the real
/// `apply_record_edit` + `trim_sweep` inside a rolled-back transaction, and
/// returns the resulting semantic [`DryRunReport`].
#[tauri::command]
fn record_edit_dry_run(
    payload: RecordEditPayload,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("record_edit_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("record_edit_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_edit_dry_run", Some(session.target_path.as_path()))
    })?;

    db::record_edit::dry_run_record_edit(
        &mut conn,
        &payload,
        &time::now_iso8601_utc(),
        guid_seed_now(),
    )
    .map_err(|err| err.to_dto("record_edit_dry_run", Some(session.target_path.as_path())))
}

/// Applies a record edit — the committed counterpart to [`record_edit_dry_run`]
/// (EDIT-07). Marks the session dirty on success.
#[tauri::command]
fn record_edit_apply(
    payload: RecordEditPayload,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("record_edit_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("record_edit_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_edit_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_edit_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("record_edit_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_edit_apply", Some(session.target_path.as_path()))
    })?;
    let report = db::record_edit::apply_record_edit_reporting(
        &tx,
        &payload,
        &time::now_iso8601_utc(),
        guid_seed_now(),
    )
    .map_err(|err| err.to_dto("record_edit_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_edit_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(report)
}

/// Previews deleting one record from the editor WITHOUT mutating the working
/// copy (SAFE-01, EDIT-07): Notes -> `NoteId`; Annotations -> `(LocationId,
/// TextTag)` — NEVER the browse-list's over-deleting `LocationId`-only
/// delete (rule #10).
#[tauri::command]
fn record_delete_dry_run(
    identity: RecordIdentity,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("record_delete_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("record_delete_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_delete_dry_run", Some(session.target_path.as_path()))
    })?;

    db::record_edit::dry_run_record_delete(&mut conn, &identity)
        .map_err(|err| err.to_dto("record_delete_dry_run", Some(session.target_path.as_path())))
}

/// Applies deleting one record from the editor — the committed counterpart
/// to [`record_delete_dry_run`] (EDIT-07, D7-10). Marks the session dirty on
/// success.
#[tauri::command]
fn record_delete_apply(
    identity: RecordIdentity,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("record_delete_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("record_delete_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_delete_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_delete_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("record_delete_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_delete_apply", Some(session.target_path.as_path()))
    })?;
    let deleted = db::record_edit::apply_record_delete(&tx, &identity)
        .map_err(|err| err.to_dto("record_delete_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("record_delete_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut deleted_map = BTreeMap::new();
    if deleted > 0 {
        let table = match identity {
            RecordIdentity::Notes { .. } => "Note",
            RecordIdentity::Annotations { .. } => "InputField",
        };
        deleted_map.insert(table.to_string(), deleted);
    }
    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: BTreeMap::new(),
        deleted: deleted_map,
        total_deleted: deleted,
        skipped: BTreeMap::new(),
    })
}

/// Every `Tag WHERE Type = 1` row with its tri-state count for `ids` (EDIT-03)
/// — the Tag Dialog's checklist source. `ids` cannot be empty — an empty
/// array fails IPC deserialization before this command body ever runs,
/// because [`NonEmptyNoteIds`] is the parameter type.
#[tauri::command]
fn tag_states(
    ids: NonEmptyNoteIds,
    state: tauri::State<SessionState>,
) -> Result<Vec<TagState>, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("tag_states", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("tag_states", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("tag_states", Some(session.target_path.as_path()))
    })?;

    db::tags::tag_states(&conn, &ids)
        .map_err(|err| err.to_dto("tag_states", Some(session.target_path.as_path())))
}

/// Previews the effect of a tag edit WITHOUT mutating the working copy
/// (SAFE-01, EDIT-03): opens the session's `db_path`, runs the real
/// `apply_tag_edit` + `trim_sweep` inside a rolled-back transaction, and
/// returns the resulting semantic [`DryRunReport`].
#[tauri::command]
fn tag_dry_run(
    ids: NonEmptyNoteIds,
    removed_tag_ids: Vec<i64>,
    added_tag_ids: Vec<i64>,
    new_tag_names: Vec<String>,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("tag_dry_run", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("tag_dry_run", None))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("tag_dry_run", Some(session.target_path.as_path()))
    })?;

    db::tags::dry_run_tag_edit(
        &mut conn,
        &ids,
        &removed_tag_ids,
        &added_tag_ids,
        &new_tag_names,
    )
    .map_err(|err| err.to_dto("tag_dry_run", Some(session.target_path.as_path())))
}

/// Applies a tag edit — the committed counterpart to [`tag_dry_run`]
/// (EDIT-03). Marks the session dirty on success.
#[tauri::command]
fn tag_apply(
    ids: NonEmptyNoteIds,
    removed_tag_ids: Vec<i64>,
    added_tag_ids: Vec<i64>,
    new_tag_names: Vec<String>,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("tag_apply", None))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("tag_apply", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("tag_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto("tag_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto("tag_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto("tag_apply", Some(session.target_path.as_path()))
    })?;
    let report = db::tags::apply_tag_edit_reporting(
        &tx,
        &ids,
        &removed_tag_ids,
        &added_tag_ids,
        &new_tag_names,
    )
    .map_err(|err| err.to_dto("tag_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto("tag_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(report)
}

/// Previews the effect of archive-wide "Sort Tags…" WITHOUT mutating the
/// working copy (SAFE-01, EDIT-04): opens the session's `db_path`, runs the
/// real `apply_reorder` inside a rolled-back transaction, and returns the
/// resulting semantic [`DryRunReport`]. No selection required — this op is
/// archive-wide (07-UI-SPEC.md: Sort Tags deliberately does NOT enter
/// `operations.ts`'s capability descriptor).
#[tauri::command]
fn reorder_dry_run(state: tauri::State<SessionState>) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("reorder_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("reorder_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("reorder_dry_run", Some(session.target_path.as_path()))
    })?;

    db::reorder::dry_run_reorder(&mut conn)
        .map_err(|err| err.to_dto("reorder_dry_run", Some(session.target_path.as_path())))
}

/// Applies archive-wide "Sort Tags…" — the committed counterpart to
/// [`reorder_dry_run`] (EDIT-04). Marks the session dirty on success.
#[tauri::command]
fn reorder_apply(state: tauri::State<SessionState>) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("reorder_apply", None))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("reorder_apply", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("reorder_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto("reorder_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("reorder_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto("reorder_apply", Some(session.target_path.as_path()))
    })?;
    let changed = db::reorder::apply_reorder(&tx)
        .map_err(|err| err.to_dto("reorder_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto("reorder_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(db::reorder::reorder_report(changed))
}

/// Previews the effect of archive-wide "Clean Archive…" WITHOUT mutating the
/// working copy (SAFE-01, EDIT-06): opens the session's `db_path`, runs the
/// real `apply_clean` inside a rolled-back transaction, and returns the
/// resulting semantic [`DryRunReport`]. No selection required — this op is
/// archive-wide (07-UI-SPEC.md: Clean/Mask deliberately do NOT enter
/// `operations.ts`'s capability descriptor, same as Sort Tags).
#[tauri::command]
fn clean_dry_run(state: tauri::State<SessionState>) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("clean_dry_run", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("clean_dry_run", None))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("clean_dry_run", Some(session.target_path.as_path()))
    })?;

    db::scrub::dry_run_clean(&mut conn)
        .map_err(|err| err.to_dto("clean_dry_run", Some(session.target_path.as_path())))
}

/// Applies archive-wide "Clean Archive…" — the committed counterpart to
/// [`clean_dry_run`] (EDIT-06). Marks the session dirty on success.
#[tauri::command]
fn clean_apply(state: tauri::State<SessionState>) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("clean_apply", None))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("clean_apply", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("clean_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto("clean_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("clean_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto("clean_apply", Some(session.target_path.as_path()))
    })?;
    let counts = db::scrub::apply_clean(&tx)
        .map_err(|err| err.to_dto("clean_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto("clean_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: counts,
        deleted: BTreeMap::new(),
        total_deleted: 0,
        skipped: BTreeMap::new(),
    })
}

/// Previews the effect of archive-wide "Mask Archive…" WITHOUT mutating the
/// working copy (SAFE-01, EDIT-06, D7-08): opens the session's `db_path`,
/// runs the real `apply_mask` inside a rolled-back transaction under a
/// freshly-drawn [`mask_seed_now`] seed, and returns the resulting semantic
/// [`DryRunReport`]. No selection required, same as [`clean_dry_run`].
#[tauri::command]
fn mask_dry_run(state: tauri::State<SessionState>) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("mask_dry_run", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("mask_dry_run", None))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("mask_dry_run", Some(session.target_path.as_path()))
    })?;

    db::scrub::dry_run_mask(&mut conn, mask_seed_now())
        .map_err(|err| err.to_dto("mask_dry_run", Some(session.target_path.as_path())))
}

/// Applies archive-wide "Mask Archive…" — the committed counterpart to
/// [`mask_dry_run`] (EDIT-06, D7-08). Draws its OWN fresh [`mask_seed_now`]
/// seed (the preview's masked TEXT is never shown to the user — only counts
/// — so the apply need not reproduce the preview's exact seed, only its
/// shape). Marks the session dirty on success.
#[tauri::command]
fn mask_apply(state: tauri::State<SessionState>) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("mask_apply", None))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("mask_apply", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("mask_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto("mask_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto("mask_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto("mask_apply", Some(session.target_path.as_path()))
    })?;
    let counts = db::scrub::apply_mask(&tx, mask_seed_now())
        .map_err(|err| err.to_dto("mask_apply", Some(session.target_path.as_path())))?;
    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto("mask_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(DryRunReport {
        added: BTreeMap::new(),
        overwritten: counts,
        deleted: BTreeMap::new(),
        total_deleted: 0,
        skipped: BTreeMap::new(),
    })
}

/// Previews the effect of downgrading the open session to v14 WITHOUT mutating
/// the working copy (D4-08): opens the session's `db_path` and runs the real
/// trim + merge inside a rolled-back transaction (trim-FIRST, identical order to
/// the actual v14 save), returning the semantic [`DryRunReport`] — merged-away
/// Locations and dedup-DELETED study rows surface as `deleted` (data loss),
/// repointed rows as `overwritten`.
#[tauri::command]
fn downgrade_dry_run(state: tauri::State<SessionState>) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("downgrade_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("downgrade_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("downgrade_dry_run", Some(session.target_path.as_path()))
    })?;

    archive::downgrade::dry_run_downgrade(&mut conn)
        .map_err(|err| err.to_dto("downgrade_dry_run", Some(session.target_path.as_path())))
}

/// Writes a v14-compatible copy of the open session to `path` (SCHEMA-03/05,
/// D4-06/D4-07). Runs the lossy downgrade on a throwaway copy so the LIVE
/// session stays byte-identical at v16 — `as_ref` (never mutated). An
/// un-downgradeable archive (HIGH-1) fails with a typed error and writes
/// nothing to `path`.
#[tauri::command]
fn save_v14_copy(path: String, state: tauri::State<SessionState>) -> Result<(), ErrorDto> {
    let target = PathBuf::from(&path);
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("save_v14_copy", Some(target.as_path()))
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("save_v14_copy", Some(target.as_path()))
    })?;

    archive::downgrade::save_v14_copy(
        session,
        &target,
        APP_NAME,
        APP_DEVICE_NAME,
        &time::now_iso8601_utc(),
    )
    .map_err(|err| err.to_dto("save_v14_copy", Some(target.as_path())))?;
    Ok(())
}

/// Previews merging the archive at `source_path` INTO the open session WITHOUT
/// mutating the live session (MERGE-01/02): runs the REAL jwlCore merge on a
/// throwaway `fs::copy` of the session DB and content-signature-diffs it,
/// returning the semantic [`DryRunReport`] (`overwritten` reflects in-place row
/// UPDATEs, not mere PK membership). The FFI + `getLastResult` read happen under
/// this single lock critical section (D5-06 serialization). A missing/wrong-arch
/// jwlCore binary maps to the `merge_unavailable` code.
#[tauri::command]
fn merge_dry_run(
    source_path: String,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let source = PathBuf::from(&source_path);
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("merge_dry_run", Some(source.as_path()))
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("merge_dry_run", Some(source.as_path()))
    })?;

    archive::merge::dry_run_merge(&app, session, &source)
        .map_err(|err| err.to_dto("merge_dry_run", Some(source.as_path())))
}

/// Commits merging the archive at `source_path` INTO the open session: runs the
/// merge on a staging copy, folds any new staging media into `session.entries`,
/// and ATOMICALLY promotes the merged DB onto `session.db_path`
/// (rename-with-replace, never a byte copy — Core Value). Marks the session
/// dirty; the source archive is only READ. Serialized under the SessionState
/// mutex (D5-06). A missing/wrong-arch binary maps to `merge_unavailable`.
#[tauri::command]
fn merge_commit(
    source_path: String,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<(), ErrorDto> {
    let source = PathBuf::from(&source_path);
    let mut guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("merge_commit", Some(source.as_path()))
    })?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("merge_commit", Some(source.as_path()))
    })?;

    archive::merge::merge_commit(&app, session, &source)
        .map_err(|err| err.to_dto("merge_commit", Some(source.as_path())))?;
    Ok(())
}

/// Previews an N-way fold of `source_paths` INTO the open session, in the
/// CALLER's list order (MERGE-03, D10-01) — never sorted, deduplicated, or
/// filtered — WITHOUT mutating the live session: runs the SAME fold chain the
/// commit uses, under a throwaway root, and content-signature-diffs the
/// ORIGINAL session DB against the FINAL folded state, so a row overwritten
/// at an intermediate step is reported once, with its final content. Fewer
/// than 3 sources is rejected with `merge_failed`, never silently degraded to
/// a Phase-5-equivalent single merge. A missing/wrong-arch jwlCore binary
/// maps to `merge_unavailable`.
#[tauri::command]
fn fold_merge_dry_run(
    source_paths: Vec<String>,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let sources: Vec<PathBuf> = source_paths.into_iter().map(PathBuf::from).collect();
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("fold_merge_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("fold_merge_dry_run", None)
    })?;

    archive::merge::fold_dry_run_merge(&app, session, &sources)
        .map_err(|err| err.to_dto("fold_merge_dry_run", None))
}

/// Commits an N-way fold of `source_paths` INTO the open session, in the
/// CALLER's list order (MERGE-03, D10-01) — the list order IS the fold order
/// and is the user's, never re-sequenced. Runs `source_paths.len()`
/// sequential merges under one staging root, folding media back after every
/// step, then performs EXACTLY ONE atomic promote onto `session.db_path`
/// after the LAST step succeeds. A step failure leaves the session untouched
/// and NOT dirty. Fewer than 3 sources is rejected with `merge_failed`.
/// Serialized under the SessionState mutex (D5-06). A missing/wrong-arch
/// binary maps to `merge_unavailable`.
#[tauri::command]
fn fold_merge_commit(
    source_paths: Vec<String>,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<(), ErrorDto> {
    let sources: Vec<PathBuf> = source_paths.into_iter().map(PathBuf::from).collect();
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("fold_merge_commit", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("fold_merge_commit", None)
    })?;

    archive::merge::fold_merge_commit(&app, session, &sources)
        .map_err(|err| err.to_dto("fold_merge_commit", None))?;
    Ok(())
}

/// Exports Favorites to `path` (whole category when `ids` is `None`, D8-10
/// selection-optional) as a `.txt` file — pure read + file write, never
/// mutates the archive or sets `session.dirty` (D8-09). The header's archive
/// name is the current target path's base file name (`Path(current_archive)
/// .name`, `JWLManager.py:1829`); the timestamp and app version are injected
/// HERE — the only place real wall-clock time is read — never inside
/// `build_export_header` itself, so `export_wireformat_tests.rs`'s
/// golden-fixture byte comparison stays deterministic.
#[tauri::command]
fn export_favorites(
    path: String,
    ids: Option<NonEmptyTagMapIds>,
    state: tauri::State<SessionState>,
) -> Result<usize, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("export_favorites", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_favorites", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("export_favorites", Some(session.target_path.as_path()))
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{FAVORITES}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let out_path = PathBuf::from(&path);
    export_favorites_impl(&conn, ids.as_ref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_favorites", Some(out_path.as_path())))
}

/// Exports only the Favorites changed since a prior export (IO-04,
/// 09-02-PLAN.md) — same shape as [`export_notes_incremental`], minus the
/// resources catalog/wall-clock `now` Notes needs (Favorites has no
/// resource-name lookups on its wire). `prior_path` is `None` for "no prior
/// file", which exports the whole category exactly as [`export_favorites`]
/// does (D9-05).
#[tauri::command]
fn export_favorites_incremental(
    path: String,
    prior_path: Option<String>,
    state: tauri::State<SessionState>,
) -> Result<IncrementalExportSummary, ErrorDto> {
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("export_favorites_incremental", None)
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_favorites_incremental", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "export_favorites_incremental",
            Some(session.target_path.as_path()),
        )
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{FAVORITES}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let prior_text = match &prior_path {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("export_favorites_incremental", Some(Path::new(p)))
        })?),
        None => None,
    };

    let out_path = PathBuf::from(&path);
    export_favorites_incremental_impl(&conn, prior_text.as_deref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_favorites_incremental", Some(out_path.as_path())))
}

/// Previews a Favorites `.txt` import (IO-02) WITHOUT mutating the working
/// copy: reads `path` as strict UTF-8, parses it FULLY before any
/// transaction opens (D8-04 fail-fast — a malformed file returns
/// `import_malformed` here and `EditPreviewDialog` never opens), then runs
/// the real apply + trim inside a rolled-back transaction and returns the
/// resulting semantic `DryRunReport` (`skipped` populated, PD-2).
#[tauri::command]
fn import_favorites_dry_run(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_favorites_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_favorites_dry_run", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_favorites_dry_run", Some(in_path.as_path()))
    })?;
    let records = parse_favorites_file(&text)
        .map_err(|err| err.to_dto("import_favorites_dry_run", Some(in_path.as_path())))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_favorites_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    dry_run_import_favorites(&mut conn, &records).map_err(|err| {
        err.to_dto(
            "import_favorites_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies a Favorites `.txt` import (IO-02/IO-03) — re-parses `path` (D8-10:
/// the double-parse is accepted, no cached parse state crosses the two IPC
/// calls) and commits the real apply inside one transaction. Marks the
/// session dirty on success.
#[tauri::command]
fn import_favorites_apply(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_favorites_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_favorites_apply", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_favorites_apply", Some(in_path.as_path()))
    })?;
    let records = parse_favorites_file(&text)
        .map_err(|err| err.to_dto("import_favorites_apply", Some(in_path.as_path())))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_favorites_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_favorites_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto(
                "import_favorites_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_favorites_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let mut available = compute_available_ids(&tx).map_err(|err| {
        err.to_dto(
            "import_favorites_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let before =
        db::edit::snapshot_tables(&tx, db::edit::FAVORITE_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_favorites_apply",
                Some(session.target_path.as_path()),
            )
        })?;
    let skipped = apply_import_favorites(&tx, &records, &mut available).map_err(|err| {
        err.to_dto(
            "import_favorites_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let after =
        db::edit::snapshot_tables(&tx, db::edit::FAVORITE_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_favorites_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_favorites_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut report = db::edit::diff_snapshots(&before, &after);
    if skipped > 0 {
        report.skipped.insert("TagMap".to_string(), skipped);
    }
    Ok(report)
}

/// Exports Bookmarks to `path` (whole category when `ids` is `None`, D8-10
/// selection-optional) as a `.txt` file — same shape as [`export_favorites`].
#[tauri::command]
fn export_bookmarks(
    path: String,
    ids: Option<NonEmptyBookmarkIds>,
    state: tauri::State<SessionState>,
) -> Result<usize, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("export_bookmarks", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_bookmarks", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("export_bookmarks", Some(session.target_path.as_path()))
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{BOOKMARKS}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let out_path = PathBuf::from(&path);
    export_bookmarks_impl(&conn, ids.as_ref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_bookmarks", Some(out_path.as_path())))
}

/// Exports only the Bookmarks changed since a prior export (IO-04,
/// 09-02-PLAN.md) — same shape as [`export_favorites_incremental`].
#[tauri::command]
fn export_bookmarks_incremental(
    path: String,
    prior_path: Option<String>,
    state: tauri::State<SessionState>,
) -> Result<IncrementalExportSummary, ErrorDto> {
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("export_bookmarks_incremental", None)
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_bookmarks_incremental", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "export_bookmarks_incremental",
            Some(session.target_path.as_path()),
        )
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{BOOKMARKS}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let prior_text = match &prior_path {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("export_bookmarks_incremental", Some(Path::new(p)))
        })?),
        None => None,
    };

    let out_path = PathBuf::from(&path);
    export_bookmarks_incremental_impl(&conn, prior_text.as_deref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_bookmarks_incremental", Some(out_path.as_path())))
}

/// Previews a Bookmarks `.txt` import (IO-02) — same shape as
/// [`import_favorites_dry_run`].
#[tauri::command]
fn import_bookmarks_dry_run(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_bookmarks_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_bookmarks_dry_run", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_bookmarks_dry_run", Some(in_path.as_path()))
    })?;
    let records = parse_bookmarks_file(&text)
        .map_err(|err| err.to_dto("import_bookmarks_dry_run", Some(in_path.as_path())))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_bookmarks_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    dry_run_import_bookmarks(&mut conn, &records).map_err(|err| {
        err.to_dto(
            "import_bookmarks_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies a Bookmarks `.txt` import (IO-02/IO-03) — same shape as
/// [`import_favorites_apply`].
#[tauri::command]
fn import_bookmarks_apply(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_bookmarks_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_bookmarks_apply", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_bookmarks_apply", Some(in_path.as_path()))
    })?;
    let records = parse_bookmarks_file(&text)
        .map_err(|err| err.to_dto("import_bookmarks_apply", Some(in_path.as_path())))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_bookmarks_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_bookmarks_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto(
                "import_bookmarks_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_bookmarks_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let mut available = compute_available_ids(&tx).map_err(|err| {
        err.to_dto(
            "import_bookmarks_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let before =
        db::edit::snapshot_tables(&tx, db::edit::BOOKMARK_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_bookmarks_apply",
                Some(session.target_path.as_path()),
            )
        })?;
    apply_import_bookmarks(&tx, &records, &mut available).map_err(|err| {
        err.to_dto(
            "import_bookmarks_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let after =
        db::edit::snapshot_tables(&tx, db::edit::BOOKMARK_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_bookmarks_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_bookmarks_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(db::edit::diff_snapshots(&before, &after))
}

/// Exports Annotations to `path` (whole category when `ids` is `None`,
/// D8-10 selection-optional) as a `.txt` file — same shape as
/// [`export_favorites`], selection typed over `LocationId` (the Annotations
/// browse-list identity — `db::delete::NonEmptyLocationIds`).
#[tauri::command]
fn export_annotations(
    path: String,
    ids: Option<NonEmptyLocationIds>,
    state: tauri::State<SessionState>,
) -> Result<usize, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("export_annotations", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_annotations", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("export_annotations", Some(session.target_path.as_path()))
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{ANNOTATIONS}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let out_path = PathBuf::from(&path);
    export_annotations_impl(&conn, ids.as_ref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_annotations", Some(out_path.as_path())))
}

/// Exports only the Annotations changed since a prior export (IO-04,
/// 09-03-PLAN.md) — same shape as [`export_favorites_incremental`]. Because
/// [`export_annotations`] can only select by `LocationId`, a changed
/// annotation's unchanged siblings at the same `LocationId` are written
/// alongside it (a disclosed over-selection, not a bug): the returned
/// summary's `exported` count is the exporter's OWN written-record count,
/// which can therefore exceed `added + modified` — see
/// `export_annotations_incremental`'s doc comment in `db::io::diff`.
#[tauri::command]
fn export_annotations_incremental(
    path: String,
    prior_path: Option<String>,
    state: tauri::State<SessionState>,
) -> Result<IncrementalExportSummary, ErrorDto> {
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("export_annotations_incremental", None)
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_annotations_incremental", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "export_annotations_incremental",
            Some(session.target_path.as_path()),
        )
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{ANNOTATIONS}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let prior_text = match &prior_path {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("export_annotations_incremental", Some(Path::new(p)))
        })?),
        None => None,
    };

    let out_path = PathBuf::from(&path);
    export_annotations_incremental_impl(&conn, prior_text.as_deref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_annotations_incremental", Some(out_path.as_path())))
}

/// Previews an Annotations `.txt` import (IO-02) — same shape as
/// [`import_favorites_dry_run`].
#[tauri::command]
fn import_annotations_dry_run(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("import_annotations_dry_run", None)
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_annotations_dry_run", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_annotations_dry_run", Some(in_path.as_path()))
    })?;
    let records = parse_annotations_file(&text)
        .map_err(|err| err.to_dto("import_annotations_dry_run", Some(in_path.as_path())))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_annotations_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    dry_run_import_annotations(&mut conn, &records).map_err(|err| {
        err.to_dto(
            "import_annotations_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies an Annotations `.txt` import (IO-02/IO-03) — same shape as
/// [`import_favorites_apply`].
#[tauri::command]
fn import_annotations_apply(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_annotations_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_annotations_apply", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_annotations_apply", Some(in_path.as_path()))
    })?;
    let records = parse_annotations_file(&text)
        .map_err(|err| err.to_dto("import_annotations_apply", Some(in_path.as_path())))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_annotations_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_annotations_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto(
                "import_annotations_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_annotations_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let mut available = compute_available_ids(&tx).map_err(|err| {
        err.to_dto(
            "import_annotations_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let before =
        db::edit::snapshot_tables(&tx, db::edit::ANNOTATION_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_annotations_apply",
                Some(session.target_path.as_path()),
            )
        })?;
    apply_import_annotations(&tx, &records, &mut available).map_err(|err| {
        err.to_dto(
            "import_annotations_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let after =
        db::edit::snapshot_tables(&tx, db::edit::ANNOTATION_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_annotations_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_annotations_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(db::edit::diff_snapshots(&before, &after))
}

/// Exports Highlights to `path` (whole category when `ids` is `None`, D8-10
/// selection-optional) as a `.txt` file — same shape as [`export_bookmarks`],
/// selection typed over `BlockRangeId` (`db::color::NonEmptyBlockRangeIds`,
/// the same wrapper Highlights recolor/delete already share).
#[tauri::command]
fn export_highlights(
    path: String,
    ids: Option<NonEmptyBlockRangeIds>,
    state: tauri::State<SessionState>,
) -> Result<usize, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("export_highlights", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_highlights", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("export_highlights", Some(session.target_path.as_path()))
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{HIGHLIGHTS}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let out_path = PathBuf::from(&path);
    export_highlights_impl(&conn, ids.as_ref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_highlights", Some(out_path.as_path())))
}

/// Exports only the Highlights changed since a prior export (IO-04,
/// 09-02-PLAN.md) — same shape as [`export_favorites_incremental`].
#[tauri::command]
fn export_highlights_incremental(
    path: String,
    prior_path: Option<String>,
    state: tauri::State<SessionState>,
) -> Result<IncrementalExportSummary, ErrorDto> {
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("export_highlights_incremental", None)
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_highlights_incremental", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "export_highlights_incremental",
            Some(session.target_path.as_path()),
        )
    })?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{HIGHLIGHTS}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let prior_text = match &prior_path {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("export_highlights_incremental", Some(Path::new(p)))
        })?),
        None => None,
    };

    let out_path = PathBuf::from(&path);
    export_highlights_incremental_impl(&conn, prior_text.as_deref(), &header, &out_path)
        .map_err(|err| err.to_dto("export_highlights_incremental", Some(out_path.as_path())))
}

/// Previews a Highlights `.txt` import (IO-02/IO-03) — same shape as
/// [`import_bookmarks_dry_run`], threading a wall-clock [`guid_seed_now`]
/// through to [`dry_run_import_highlights`] for the fresh `UserMark`s'
/// GUIDs (`db::io::usermark::synthesize_usermark`).
#[tauri::command]
fn import_highlights_dry_run(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("import_highlights_dry_run", None)
    })?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_highlights_dry_run", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_highlights_dry_run", Some(in_path.as_path()))
    })?;
    let records = parse_highlights_file(&text)
        .map_err(|err| err.to_dto("import_highlights_dry_run", Some(in_path.as_path())))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_highlights_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    dry_run_import_highlights(&mut conn, &records, guid_seed_now()).map_err(|err| {
        err.to_dto(
            "import_highlights_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies a Highlights `.txt` import (IO-02/IO-03) — same shape as
/// [`import_bookmarks_apply`], over [`db::edit::HIGHLIGHT_SNAPSHOT_TABLES`].
#[tauri::command]
fn import_highlights_apply(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_highlights_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_highlights_apply", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_highlights_apply", Some(in_path.as_path()))
    })?;
    let records = parse_highlights_file(&text)
        .map_err(|err| err.to_dto("import_highlights_apply", Some(in_path.as_path())))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_highlights_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_highlights_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err).to_dto(
                "import_highlights_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_highlights_apply",
            Some(session.target_path.as_path()),
        )
    })?;

    let mut available = compute_available_ids(&tx).map_err(|err| {
        err.to_dto(
            "import_highlights_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let before =
        db::edit::snapshot_tables(&tx, db::edit::HIGHLIGHT_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_highlights_apply",
                Some(session.target_path.as_path()),
            )
        })?;
    apply_import_highlights(&tx, &records, &mut available, guid_seed_now()).map_err(|err| {
        err.to_dto(
            "import_highlights_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    let after =
        db::edit::snapshot_tables(&tx, db::edit::HIGHLIGHT_SNAPSHOT_TABLES).map_err(|err| {
            err.to_dto(
                "import_highlights_apply",
                Some(session.target_path.as_path()),
            )
        })?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_highlights_apply",
            Some(session.target_path.as_path()),
        )
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(db::edit::diff_snapshots(&before, &after))
}

/// Exports Notes to `path` (whole category when `ids` is `None`, D8-10
/// selection-optional) as a `.txt` file — same shape as [`export_highlights`],
/// plus the `ResourceCatalog` load [`list_category`] already establishes
/// (needed for the HEADING auto-fill's Bible book name lookup).
#[tauri::command]
fn export_notes(
    path: String,
    ids: Option<NonEmptyNoteIds>,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<usize, ErrorDto> {
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|err| err.to_dto("export_notes", None))?;
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("export_notes", None))?;
    let session = guard
        .as_ref()
        .ok_or_else(|| error::ArchiveError::MissingUserDataBackup.to_dto("export_notes", None))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("export_notes", Some(session.target_path.as_path()))
    })?;
    let catalog = db::resources::ResourceCatalog::load(&resources_db_path, "en")
        .map_err(|err| err.to_dto("export_notes", None))?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{NOTES=}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let out_path = PathBuf::from(&path);
    export_notes_impl(
        &conn,
        ids.as_ref(),
        &catalog,
        &header,
        &time::now_iso8601_utc(),
        &out_path,
    )
    .map_err(|err| err.to_dto("export_notes", Some(out_path.as_path())))
}

/// Exports only the Notes changed since a prior export (IO-04, D9-01..D9-05,
/// 09-01-PLAN.md) — read-only on the archive, same never-mutates contract as
/// [`export_notes`]. `prior_path` is `None` for "no prior file", which
/// exports the whole category exactly as [`export_notes`] does (D9-05).
/// When present, `prior_path`'s text is read as strict UTF-8 and handed to
/// [`export_notes_incremental_impl`], which runs it through the same
/// fail-fast `parse_notes_file` validation gate the shipped Notes import
/// path uses BEFORE any output file is written — a malformed prior file
/// surfaces the typed `import_malformed` error and writes nothing.
#[tauri::command]
fn export_notes_incremental(
    path: String,
    prior_path: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
) -> Result<IncrementalExportSummary, ErrorDto> {
    let resources_db_path = db::resources::resolve_resources_db_path(&app)
        .map_err(|err| err.to_dto("export_notes_incremental", None))?;
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("export_notes_incremental", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_notes_incremental", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "export_notes_incremental",
            Some(session.target_path.as_path()),
        )
    })?;
    let catalog = db::resources::ResourceCatalog::load(&resources_db_path, "en")
        .map_err(|err| err.to_dto("export_notes_incremental", None))?;

    let archive_name = session
        .target_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "NEW ARCHIVE".to_string());
    let header = ExportHeaderCtx {
        category_tag: "{NOTES=}",
        archive_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: time::now_export_header_timestamp(),
    };

    let prior_text = match &prior_path {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|err| {
                error::ArchiveError::from(err)
                    .to_dto("export_notes_incremental", Some(Path::new(p)))
            })?;
            Some(text)
        }
        None => None,
    };

    let out_path = PathBuf::from(&path);
    export_notes_incremental_impl(
        &conn,
        prior_text.as_deref(),
        &catalog,
        &header,
        &time::now_iso8601_utc(),
        &out_path,
    )
    .map_err(|err| err.to_dto("export_notes_incremental", Some(out_path.as_path())))
}

/// The Tauri-facing result of a Notes import preview (IO-02, D8-09) — the
/// standard [`DryRunReport`] PLUS the file's own detected bucket-delete
/// character (if any), so the frontend can render the Notes-only extra
/// preview clause and require an explicit opt-in BEFORE calling
/// [`import_notes_apply`] with that same character. `report` already
/// reflects what would happen if `bucket` were applied — this preview runs
/// [`dry_run_import_notes`] with the FILE's own bucket (never a caller-
/// supplied one, since there is no caller opt-in yet at preview time), all
/// inside a transaction that is never committed (SAFE-01) — showing the true
/// effect is harmless because nothing is ever persisted from a dry run.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/NotesImportPreview.ts")]
pub struct NotesImportPreview {
    pub report: DryRunReport,
    pub bucket: Option<String>,
}

/// Previews a Notes `.txt` import (IO-02/IO-03, D8-09) — same shape as
/// [`import_highlights_dry_run`], extended with [`NotesImportPreview`]'s
/// bucket-delete signal. The bucket the FILE names is used for the preview
/// itself (an honest "what would happen"); [`import_notes_apply`] takes its
/// OWN separate `bucket` argument, decoupled from what the file requested,
/// so the user's explicit opt-in (or lack of one) is what actually governs
/// the commit.
#[tauri::command]
fn import_notes_dry_run(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<NotesImportPreview, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_notes_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_notes_dry_run", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_notes_dry_run", Some(in_path.as_path()))
    })?;
    let (bucket, records) = parse_notes_file(&text)
        .map_err(|err| err.to_dto("import_notes_dry_run", Some(in_path.as_path())))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_notes_dry_run", Some(session.target_path.as_path()))
    })?;

    let report = dry_run_import_notes(
        &mut conn,
        bucket,
        &records,
        guid_seed_now(),
        &time::now_iso8601_utc(),
    )
    .map_err(|err| err.to_dto("import_notes_dry_run", Some(session.target_path.as_path())))?;

    Ok(NotesImportPreview {
        report,
        bucket: bucket.map(|c| c.to_string()),
    })
}

/// Applies a Notes `.txt` import (IO-02/IO-03, D8-09) — same shape as
/// [`import_highlights_apply`]. `bucket` is the user's EXPLICIT opt-in
/// (`None` unless the frontend's preview dialog opt-in was checked): the
/// bucket delete never runs just because the file's own tag line named one.
#[tauri::command]
fn import_notes_apply(
    path: String,
    bucket: Option<String>,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_notes_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_notes_apply", None)
    })?;

    let in_path = PathBuf::from(&path);
    let text = std::fs::read_to_string(&in_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto("import_notes_apply", Some(in_path.as_path()))
    })?;
    let (_file_bucket, records) = parse_notes_file(&text)
        .map_err(|err| err.to_dto("import_notes_apply", Some(in_path.as_path())))?;
    let opted_in_bucket = bucket.and_then(|s| s.chars().next());

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_notes_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_notes_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("import_notes_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_notes_apply", Some(session.target_path.as_path()))
    })?;

    let mut available = compute_available_ids(&tx)
        .map_err(|err| err.to_dto("import_notes_apply", Some(session.target_path.as_path())))?;
    let before = db::edit::snapshot_tables(&tx, db::edit::NOTE_IMPORT_SNAPSHOT_TABLES)
        .map_err(|err| err.to_dto("import_notes_apply", Some(session.target_path.as_path())))?;
    apply_import_notes(
        &tx,
        opted_in_bucket,
        &records,
        &mut available,
        guid_seed_now(),
        &time::now_iso8601_utc(),
    )
    .map_err(|err| err.to_dto("import_notes_apply", Some(session.target_path.as_path())))?;
    let after = db::edit::snapshot_tables(&tx, db::edit::NOTE_IMPORT_SNAPSHOT_TABLES)
        .map_err(|err| err.to_dto("import_notes_apply", Some(session.target_path.as_path())))?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_notes_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(db::edit::diff_snapshots(&before, &after))
}

/// The Tauri-facing result of a Playlist import preview (IO-02, 08-05-PLAN.md)
/// — the standard [`DryRunReport`] PLUS the playlist's name and its media
/// file count, so the frontend can render the leading UI-SPEC clause *"This
/// adds the playlist "{Name}" and its {N} media file{s}."* before the
/// standard added/updated/skipped lines.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/PlaylistImportPreview.ts")]
struct PlaylistImportPreview {
    report: DryRunReport,
    playlist_name: String,
    media_count: usize,
}

/// Derives the playlist's name from the imported file's own stem — matches
/// Python's `playlist_name = Path(file).stem` when the suffix is
/// `.jwlplaylist` (`JWLManager.py:2622-2623`, this command only ever handles
/// that suffix).
fn playlist_name_from_path(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Playlist".to_string())
}

/// Exports the `PlaylistItem`s in `ids` to `path` as a `.jwlplaylist`
/// (IO-01) — pure read + file write, never mutates the archive (D8-09).
#[tauri::command]
fn export_playlist(
    path: String,
    ids: NonEmptyPlaylistItemIds,
    state: tauri::State<SessionState>,
) -> Result<db::playlist_io::PlaylistExportReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("export_playlist", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("export_playlist", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("export_playlist", Some(session.target_path.as_path()))
    })?;

    let out_path = PathBuf::from(&path);
    db::playlist_io::export_playlist(
        &conn,
        session.temp_dir.path(),
        &ids,
        &out_path,
        APP_NAME,
        APP_DEVICE_NAME,
        &time::now_iso8601_utc(),
    )
    .map_err(|err| err.to_dto("export_playlist", Some(out_path.as_path())))
}

/// Previews a `.jwlplaylist` import (IO-02) WITHOUT mutating the working
/// copy: extracts + validates the container FULLY before any transaction
/// opens (D8-04 — a zip-slip or missing-member container returns a typed
/// error here and `EditPreviewDialog` never opens), then runs the real
/// apply + trim inside a rolled-back transaction.
#[tauri::command]
fn import_playlist_dry_run(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<PlaylistImportPreview, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_playlist_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_playlist_dry_run", None)
    })?;

    let in_path = PathBuf::from(&path);
    let playlist_name = playlist_name_from_path(&in_path);

    let container = db::playlist_io::read_playlist_container(&in_path)
        .map_err(|err| err.to_dto("import_playlist_dry_run", Some(in_path.as_path())))?;
    let media_count = db::playlist_io::count_container_media(&container)
        .map_err(|err| err.to_dto("import_playlist_dry_run", Some(in_path.as_path())))?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "import_playlist_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    let report = db::playlist_io::dry_run_import_playlist(&mut conn, &container, &playlist_name)
        .map_err(|err| {
            err.to_dto(
                "import_playlist_dry_run",
                Some(session.target_path.as_path()),
            )
        })?;

    Ok(PlaylistImportPreview {
        report,
        playlist_name,
        media_count,
    })
}

/// Applies a `.jwlplaylist` import (IO-02/IO-03) — re-extracts `path`
/// (D8-10: the double-extraction is accepted, no cached container state
/// crosses the two IPC calls) and commits the real apply inside one
/// transaction. Media files are copied into the live session's working
/// directory only AFTER every DB write is staged (PD-3); a copy failure
/// aborts before `tx.commit()`, leaving the archive untouched.
#[tauri::command]
fn import_playlist_apply(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<DryRunReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("import_playlist_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("import_playlist_apply", None)
    })?;

    let in_path = PathBuf::from(&path);
    let playlist_name = playlist_name_from_path(&in_path);

    let container = db::playlist_io::read_playlist_container(&in_path)
        .map_err(|err| err.to_dto("import_playlist_apply", Some(in_path.as_path())))?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_playlist_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_playlist_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("import_playlist_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_playlist_apply", Some(session.target_path.as_path()))
    })?;

    let mut available = compute_available_ids(&tx)
        .map_err(|err| err.to_dto("import_playlist_apply", Some(session.target_path.as_path())))?;
    let before = db::edit::snapshot_tables(&tx, db::edit::PLAYLIST_IMPORT_SNAPSHOT_TABLES)
        .map_err(|err| err.to_dto("import_playlist_apply", Some(session.target_path.as_path())))?;
    let skipped = db::playlist_io::apply_import_playlist(
        &tx,
        &container,
        &playlist_name,
        Some(session.temp_dir.path()),
        &mut available,
    )
    .map_err(|err| err.to_dto("import_playlist_apply", Some(session.target_path.as_path())))?;
    let after = db::edit::snapshot_tables(&tx, db::edit::PLAYLIST_IMPORT_SNAPSHOT_TABLES)
        .map_err(|err| err.to_dto("import_playlist_apply", Some(session.target_path.as_path())))?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("import_playlist_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    let mut report = db::edit::diff_snapshots(&before, &after);
    if skipped > 0 {
        report.skipped.insert("PlaylistItem".to_string(), skipped);
    }
    Ok(report)
}

/// Pre-checks the given file `paths` for the currently open session's
/// Playlists media add (IO-02, 08-06-PLAN.md): classifies each as `"new"` /
/// `"duplicate"` / `"unsupported"` via SHA-256 content-hash dedup + magic-
/// byte sniffing. Performs NO writes of any kind — this IS the confirm
/// surface `MediaAddDialog` renders (D8-06, UI-SPEC).
#[tauri::command]
fn media_add_precheck(
    paths: Vec<String>,
    state: tauri::State<SessionState>,
) -> Result<Vec<db::media::MediaPrecheckResult>, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("media_add_precheck", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("media_add_precheck", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("media_add_precheck", Some(session.target_path.as_path()))
    })?;

    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let prechecks = db::media::media_precheck(&conn, &path_bufs)
        .map_err(|err| err.to_dto("media_add_precheck", Some(session.target_path.as_path())))?;

    Ok(prechecks
        .iter()
        .map(db::media::MediaPrecheck::to_dto)
        .collect())
}

/// The Tauri-facing result of [`media_add_apply`] — how many of the
/// (already-precheck-filtered-to-New) files were actually added. A copy
/// failure is a WHOLE-BATCH failure (PD-3, this app's first staged-DB-then-
/// files commit) — it never partially lands, so there is no per-file
/// "failed" count here; the frontend derives its per-row "added"/"skipped"/
/// "unreadable" glyphs from the PRECHECK response plus this success/failure
/// outcome, never a third, separate per-file apply result.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/MediaAddApplyReport.ts")]
struct MediaAddApplyReport {
    added: usize,
}

/// Applies a media-add batch for the currently open session's Playlists
/// (IO-02, 08-06-PLAN.md): re-runs [`db::media::media_precheck`] on `paths`
/// (never trusts a stale client-side classification — a race with a
/// concurrent add is simply re-resolved by this fresh hash check), filters
/// to `New` entries, stages every DB write into one transaction
/// ([`db::media::apply_media_add`]), then copies every staged file
/// ([`db::media::perform_staged_copies`]) — committing ONLY if every copy
/// succeeded (PD-3). On any copy failure the transaction is dropped
/// (never committed) and every already-written file from THIS call is
/// deleted, so neither a phantom row nor a half-written batch survives.
/// Marks the session dirty on success.
#[tauri::command]
fn media_add_apply(
    paths: Vec<String>,
    playlist_name: String,
    state: tauri::State<SessionState>,
) -> Result<MediaAddApplyReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("media_add_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("media_add_apply", None)
    })?;

    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("media_add_apply", Some(session.target_path.as_path()))
    })?;

    let prechecks = db::media::media_precheck(&conn, &path_bufs)
        .map_err(|err| err.to_dto("media_add_apply", Some(session.target_path.as_path())))?;
    let new_items: Vec<db::media::MediaPrecheck> = prechecks
        .into_iter()
        .filter(|p| matches!(p.classification, db::media::MediaClassification::New { .. }))
        .collect();

    if new_items.is_empty() {
        return Ok(MediaAddApplyReport { added: 0 });
    }

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("media_add_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("media_add_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("media_add_apply", Some(session.target_path.as_path()))
    })?;

    let mut available = compute_available_ids(&tx)
        .map_err(|err| err.to_dto("media_add_apply", Some(session.target_path.as_path())))?;
    let mut staged = Vec::new();
    let added = db::media::apply_media_add(
        &tx,
        &playlist_name,
        &new_items,
        &mut staged,
        &mut available,
        guid_seed_now(),
    )
    .map_err(|err| err.to_dto("media_add_apply", Some(session.target_path.as_path())))?;

    // PD-3: files are copied AFTER every DB write is staged, and the
    // transaction is committed ONLY if every copy succeeded — `tx` is
    // simply dropped (never committed) on the `Err` path, rolling back the
    // whole batch atomically.
    db::media::perform_staged_copies(&staged, session.temp_dir.path())
        .map_err(|err| err.to_dto("media_add_apply", Some(session.target_path.as_path())))?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("media_add_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    session.dirty = true;

    Ok(MediaAddApplyReport { added })
}

/// Previews the effect of deleting the given Playlist item selection WITHOUT
/// mutating the working copy (SAFE-01, D8-07): opens the session's
/// `db_path`, runs the real delete + trim inside a rolled-back transaction,
/// and returns the resulting [`db::media::PlaylistDeleteReport`] — the
/// standard `DryRunReport` plus the media-removed/media-kept counts the
/// UI-SPEC's "shared media survives" summary needs.
#[tauri::command]
fn playlist_delete_dry_run(
    ids: NonEmptyPlaylistItemIds,
    state: tauri::State<SessionState>,
) -> Result<db::media::PlaylistDeleteReport, ErrorDto> {
    let guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("playlist_delete_dry_run", None))?;
    let session = guard.as_ref().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("playlist_delete_dry_run", None)
    })?;

    let mut conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err).to_dto(
            "playlist_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })?;

    db::media::dry_run_delete_playlist_items(&mut conn, &ids).map_err(|err| {
        err.to_dto(
            "playlist_delete_dry_run",
            Some(session.target_path.as_path()),
        )
    })
}

/// Applies the delete of the given Playlist item selection — the project's
/// FIRST irreversible on-disk media removal (T-08-30, checkpoint-gated at
/// plan time). Commits the DB delete FIRST ([`db::media::delete_playlist_items_db`]),
/// and ONLY THEN calls [`db::media::remove_media_files`] (best-effort, a
/// missing file silently ignored) — never the reverse (D8-07/PD-3). Marks
/// the session dirty on success.
#[tauri::command]
fn playlist_delete_apply(
    ids: NonEmptyPlaylistItemIds,
    state: tauri::State<SessionState>,
) -> Result<db::media::PlaylistDeleteReport, ErrorDto> {
    let mut guard = state
        .lock()
        .map_err(|_| error::ArchiveError::StatePoisoned.to_dto("playlist_delete_apply", None))?;
    let session = guard.as_mut().ok_or_else(|| {
        error::ArchiveError::MissingUserDataBackup.to_dto("playlist_delete_apply", None)
    })?;

    let conn = rusqlite::Connection::open(&session.db_path).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("playlist_delete_apply", Some(session.target_path.as_path()))
    })?;

    let guard_pragma = db::pragma_guard::PragmaGuard::new(&conn).map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("playlist_delete_apply", Some(session.target_path.as_path()))
    })?;
    conn.execute_batch("PRAGMA foreign_keys = 'OFF';")
        .map_err(|err| {
            error::ArchiveError::from(err)
                .to_dto("playlist_delete_apply", Some(session.target_path.as_path()))
        })?;

    let tx = conn.unchecked_transaction().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("playlist_delete_apply", Some(session.target_path.as_path()))
    })?;

    let before = db::edit::snapshot_tables(&tx, db::edit::MEDIA_DELETE_SNAPSHOT_TABLES)
        .map_err(|err| err.to_dto("playlist_delete_apply", Some(session.target_path.as_path())))?;
    let outcome = db::media::delete_playlist_items_db(&tx, &ids)
        .map_err(|err| err.to_dto("playlist_delete_apply", Some(session.target_path.as_path())))?;
    let after = db::edit::snapshot_tables(&tx, db::edit::MEDIA_DELETE_SNAPSHOT_TABLES)
        .map_err(|err| err.to_dto("playlist_delete_apply", Some(session.target_path.as_path())))?;

    tx.commit().map_err(|err| {
        error::ArchiveError::from(err)
            .to_dto("playlist_delete_apply", Some(session.target_path.as_path()))
    })?;
    drop(guard_pragma);

    // Filesystem removal happens ONLY here — after the DB transaction has
    // already committed (D8-07/PD-3).
    db::media::remove_media_files(session.temp_dir.path(), &outcome.removed_files);

    session.dirty = true;

    let report = db::edit::diff_snapshots(&before, &after);
    Ok(db::media::PlaylistDeleteReport {
        report,
        media_removed: outcome.removed_files.len(),
        media_kept: outcome.kept_count,
    })
}

/// Returns the crate's runtime version (`env!("CARGO_PKG_VERSION")`), the
/// same pattern already used inline at 10+ `ErrorDto` call sites throughout
/// this file. The FIRST callable, registered command exposing it -- prior
/// call sites are inline literals, not an invokable command -- so
/// `SettingsDialog`'s About region never hardcodes a version string
/// (11-01-PLAN.md Task 1).
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Tauri builder wiring for the Walking Skeleton.
///
/// `open_archive` (01-07) and `check_jwlcore` (01-03) are registered here.
/// `check_jwlcore` is invoked lazily by the frontend after mount, NOT from
/// `setup()` — a missing/wrong-arch jwlCore binary must render a status,
/// never crash launch (Pitfall 4). `save_archive` / `save_as` / `new_archive`
/// (01-05) round out the persistence slice.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(err) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(None::<ArchiveSession>))
        .invoke_handler(tauri::generate_handler![
            open_archive,
            jwlcore::loader::check_jwlcore,
            save_archive,
            save_as,
            new_archive,
            delete_notes_dry_run,
            delete_notes_apply,
            favorite_remove_dry_run,
            favorite_remove_apply,
            list_favorite_languages,
            list_favorite_editions,
            favorite_add_dry_run,
            favorite_add_apply,
            color_dry_run,
            color_apply,
            highlight_delete_dry_run,
            highlight_delete_apply,
            bookmark_delete_dry_run,
            bookmark_delete_apply,
            annotation_delete_dry_run,
            annotation_delete_apply,
            record_fetch,
            record_edit_dry_run,
            record_edit_apply,
            record_delete_dry_run,
            record_delete_apply,
            tag_states,
            tag_dry_run,
            tag_apply,
            reorder_dry_run,
            reorder_apply,
            clean_dry_run,
            clean_apply,
            mask_dry_run,
            mask_apply,
            downgrade_dry_run,
            save_v14_copy,
            merge_dry_run,
            merge_commit,
            fold_merge_dry_run,
            fold_merge_commit,
            list_notes,
            list_category,
            export_favorites,
            export_favorites_incremental,
            import_favorites_dry_run,
            import_favorites_apply,
            export_bookmarks,
            export_bookmarks_incremental,
            import_bookmarks_dry_run,
            import_bookmarks_apply,
            export_annotations,
            export_annotations_incremental,
            import_annotations_dry_run,
            import_annotations_apply,
            export_highlights,
            export_highlights_incremental,
            import_highlights_dry_run,
            import_highlights_apply,
            export_notes,
            export_notes_incremental,
            import_notes_dry_run,
            import_notes_apply,
            export_playlist,
            import_playlist_dry_run,
            import_playlist_apply,
            media_add_precheck,
            media_add_apply,
            playlist_delete_dry_run,
            playlist_delete_apply,
            settings::load_settings,
            settings::save_settings,
            app_version
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {err}");
        std::process::exit(1);
    }
}
