// Crate-level lint gate (D-15): no `.unwrap()`/`.expect()` on archive-data paths.
// Later plans (01-03, 01-05, 01-07) implement commands under this same crate and
// must not silently regress this gate.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod archive;
pub mod category;
pub mod db;
pub mod error;
pub mod jwlcore;
pub mod session;
pub mod time;

use db::delete::{DryRunReport, NonEmptyNoteIds};
use db::notes::NotesRow;
use error::ErrorDto;
use session::{ArchiveSession, SessionState};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

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
) -> Result<Vec<NotesRow>, ErrorDto> {
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
            downgrade_dry_run,
            save_v14_copy
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {err}");
        std::process::exit(1);
    }
}
