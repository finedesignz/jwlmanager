// Crate-level lint gate (D-15): no `.unwrap()`/`.expect()` on archive-data paths.
// Later plans (01-03, 01-05, 01-07) implement commands under this same crate and
// must not silently regress this gate.
#![deny(clippy::unwrap_used, clippy::expect_used)]

/// Bare Tauri builder for the Walking Skeleton scaffold.
///
/// `invoke_handler` is intentionally EMPTY here — commands are registered by
/// later plans:
///   - 01-07 registers `open_archive`
///   - 01-03 registers `check_jwlcore`
///   - 01-05 registers `save_archive` / `new_archive` / `save_archive_as`
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(err) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {err}");
        std::process::exit(1);
    }
}
