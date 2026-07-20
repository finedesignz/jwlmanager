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

use db::notes::NotesRow;
use error::ErrorDto;
use session::{ArchiveSession, SessionState};
use std::path::PathBuf;
use std::sync::Mutex;

/// Extracts, validates (v16-only gate), and queries `path`, storing the
/// resulting `ArchiveSession` in managed state and returning the Notes rows
/// for the frontend's first render. Every internal `ArchiveError` is mapped
/// to a sanitized `ErrorDto` at this IPC boundary (D-14, SAFE-05) — never the
/// raw error, never the absolute path.
#[tauri::command]
fn open_archive(
    path: String,
    state: tauri::State<SessionState>,
) -> Result<Vec<NotesRow>, ErrorDto> {
    let path_buf = PathBuf::from(&path);
    let (session, notes) = archive::open_and_validate(&path_buf)
        .map_err(|err| err.to_dto("open_archive", Some(path_buf.as_path())))?;

    let mut guard = state.lock().map_err(|_| {
        error::ArchiveError::StatePoisoned.to_dto("open_archive", Some(path_buf.as_path()))
    })?;
    *guard = Some(session);

    Ok(notes)
}

/// Tauri builder wiring for the Walking Skeleton.
///
/// `open_archive` (01-07) and `check_jwlcore` (01-03) are registered here.
/// `check_jwlcore` is invoked lazily by the frontend after mount, NOT from
/// `setup()` — a missing/wrong-arch jwlCore binary must render a status,
/// never crash launch (Pitfall 4). Remaining commands are registered by
/// later plans:
///   - 01-05 registers `save_archive` / `new_archive` / `save_archive_as`
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(err) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(None::<ArchiveSession>))
        .invoke_handler(tauri::generate_handler![
            open_archive,
            jwlcore::loader::check_jwlcore
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {err}");
        std::process::exit(1);
    }
}
