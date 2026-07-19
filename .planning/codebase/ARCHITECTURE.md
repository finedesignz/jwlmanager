<!-- refreshed: 2026-07-16 -->
# Architecture

**Analysis Date:** 2026-07-16

## System Overview

```text
┌─────────────────────────────────────────────────────────────┐
│                    PySide6 GUI (Qt Widgets)                  │
│   `JWLManager.py` (class Window) + `res/ui_main_window.py`   │
│   (generated Ui_MainWindow) + `res/ui_extras.py` (dialogs)    │
└──────────────────────────┬────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Archive / DB layer (in-process, no server)                   │
│  - ZipFile open/extract of .jwlibrary → temp dir (mkdtemp)     │
│  - sqlite3 connection to extracted `user_data.db`              │
│  - polars DataFrames for in-memory querying/aggregation        │
│  `JWLManager.py`: load_file, zip_file, regroup, upgrade_schema  │
└──────────────────────────┬────────────────────────────────────┘
                            │ (merge only)
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Native jwlCore library (Rust/C ABI, prebuilt, vendored)       │
│  `libs/jwlCore-amd64.dll`, `libs/libjwlCore-x86_64.so`,        │
│  `libs/libjwlCore-arm64.so`, `libs/libjwlCore.dylib`           │
│  Bound via ctypes bridge: `jwlcore.py`                         │
└─────────────────────────────────────────────────────────────┘
```

There is no client/server split and no network layer (aside from an optional crash-report HTTP POST and a version-check request). This is a single-process desktop app: one Python entry script drives a Qt event loop, reads/writes a temporary SQLite database extracted from a zip archive, and calls out to a native shared library only for the "merge two archives" operation.

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| `Window` (QMainWindow) | All app logic: UI wiring, archive load/save, tree building, export/import, tagging, merging, settings | `JWLManager.py` |
| `Ui_MainWindow` | Generated Qt Designer layout (widgets, menus, toolbars) | `res/ui_main_window.py` |
| Dialog/helper classes (`AboutBox`, `HelpBox`, `DataViewer`, `DropList`, `MergeDialog`, `TagDialog`, `ThemeManager`, `ViewerItem`) | Secondary dialogs, drag/drop list widget, theme (light/dark) application, record viewer | `res/ui_extras.py` |
| jwlcore ctypes bridge | Loads platform-specific native lib, declares `argtypes`/`restype`, exposes Python wrapper functions | `jwlcore.py` |
| Native jwlCore lib | Fast/robust merge of two `.jwlibrary` SQLite databases (record de-dup, conflict resolution), schema downgrade support | `libs/jwlCore-*` (compiled binary, no source in this repo) |
| Resources | Blank archive template, resources DB (verse/publication lookups), stylesheets, icons, translations | `res/blank`, `res/blank_playlist`, `res/resources.db`, `res/dark.qss`, `res/light.qss`, `res/icons/`, `res/locales/` |

## Pattern Overview

**Overall:** Monolithic single-file desktop application (God-object `Window` class) on top of Qt's MVC-ish widget model, with a narrow ctypes FFI boundary to a native performance-critical component.

**Key Characteristics:**
- Single class (`Window` in `JWLManager.py`, ~4000 lines) owns nearly all behavior: file I/O, business logic, UI event handlers, and rendering — no separate model/service layer.
- Archive is treated as a temp-extracted SQLite DB + zip container; state lives on disk in a per-session temp directory (`mkdtemp()`), not in a persistent server or long-lived DB connection.
- Data querying/manipulation for tree views done via `polars` DataFrames (`import polars as pl`) computed on demand from SQL query results, then cached in `self.tree_cache` (a dict keyed by category/grouping).
- Native library is a stateless utility invoked once per merge action — not a long-running service, no IPC beyond a single function call + callback for progress.
- i18n via `gettext` + Qt `QTranslator`, resources compiled per-locale under `res/locales/<lang>/LC_MESSAGES`.

## Layers

**Presentation (Qt widgets):**
- Purpose: render tree of Annotations/Bookmarks/Favorites/Highlights/Notes/Playlists, menus, dialogs, drag-drop, theming.
- Location: `JWLManager.py` (methods under `class Window`), `res/ui_main_window.py`, `res/ui_extras.py`.
- Depends on: PySide6 (`QtCore`, `QtGui`, `QtWidgets`).
- Used by: entry point only (this is the top layer).

**Archive/data-access (embedded, no separate module):**
- Purpose: extract `.jwlibrary` (zip) to temp dir, open/query/mutate the embedded SQLite `user_data.db`, rebuild/export the zip on save, manage schema upgrade/downgrade.
- Location: `JWLManager.py` — `load_file`, `zip_file`, `check_validity`, `upgrade_schema`, `regroup`/`get_annotations`/`get_bookmarks`/etc. inner functions, `export_items`/`export_file`.
- Contains: raw `sqlite3` calls (parameterized SQL strings), `polars` DataFrame transforms, `zipfile` archive manipulation.
- Depends on: `sqlite3`, `polars`, `zipfile`, `res/blank` template.
- Used by: presentation layer (called directly from UI event handlers — no intermediary service objects).

**Native bridge (FFI):**
- Purpose: expose the compiled jwlCore merge/version functions to Python.
- Location: `jwlcore.py`.
- Contains: ctypes `CDLL` load logic (platform-name resolution `_platform_lib_name`, PyInstaller `_MEIPASS` awareness), `argtypes`/`restype` declarations, thin wrapper functions (`merge_databases`, `get_last_result`, `get_core_version`).
- Depends on: `libs/*` prebuilt shared libraries.
- Used by: `JWLManager.py` merge_file / merge dialog flow (imports `merge_databases, get_core_version, get_last_result, lib, CALLBACKTYPE`).

## Data Flow

### Primary Request Path (open → view → edit → save)

1. User opens a `.jwlibrary` file via file dialog, drag-drop, or CLI arg — `load_file` (`JWLManager.py:1077`).
2. `check_validity` (`JWLManager.py:994`) verifies it's a real zip containing the expected manifest/DB.
3. Archive extracted with `ZipFile(archive, 'r')` into a per-run temp dir created by `mkdtemp()`.
4. `regroup` (`JWLManager.py:551`) opens `sqlite3.connect(f'{TMP_PATH}/{DB_NAME}')`, runs category-specific queries (`get_annotations`, `get_bookmarks`, `get_favorites`, `get_highlights`, `get_notes`, `get_playlists` — nested functions inside `regroup`, `JWLManager.py:641-777`), and builds `polars` DataFrames.
5. `build_tree`/`traverse` (`JWLManager.py:797-909`) converts DataFrame rows into `QTreeWidgetItem` hierarchies, using `self.tree_cache` to avoid recomputation when the same category/grouping is revisited.
6. Edits (tag, color change, delete, import) mutate the SQLite DB directly via `sqlite3` statements, then call `archive_modified` (`JWLManager.py:1271`) to mark the app state dirty and trigger a `regroup` refresh.
7. Save: `zip_file` (`JWLManager.py:1152`) updates the manifest, optionally runs `downgrade_schema` (nested fn, `JWLManager.py:1172`), and re-zips the temp dir contents into the target `.jwlibrary` file with `ZipFile(..., 'w', compression=ZIP_DEFLATED)`.
8. `archive_saved` (`JWLManager.py:1278`) resets dirty state; window title updated via `change_title`.

### Merge Flow

1. User picks second archive via `MergeDialog` (`res/ui_extras.py`).
2. `merge_file` (`JWLManager.py:1010`) extracts the second archive to a temp path.
3. `merge_databases(path1, path2, downgrade)` (`jwlcore.py:74`) calls into the native lib (`lib.mergeDatabase`), which performs the actual record merge/de-dup in Rust/C — result code returned.
4. `get_last_result()` (`jwlcore.py:77`) retrieves a status/error string from the native side via `getLastResult()`.
5. Progress reported back to Qt via a `CFUNCTYPE` callback (`CALLBACKTYPE`, `jwlcore.py:59`) registered with `lib.setProgressCallback`.
6. On success, the merged temp DB replaces/updates the working DB and `regroup` re-renders the tree.

### Export Flow

1. `export_menu` (`JWLManager.py:1286`) collects selected tree items.
2. `export_items` (`JWLManager.py:1307`) dispatches per-category export functions (`export_annotations`, `export_bookmarks`, `export_favorites`, ... nested from `JWLManager.py:1371` onward).
3. Output written via `xlsxwriter.Workbook` (`create_xlsx`, `JWLManager.py:1345`) for spreadsheet export or plain text/markdown writers for other formats.

**State Management:**
- All working state (extracted archive, temp DB, dirty flag, tree cache, current selection) lives as instance attributes on the single `Window` object and files under a session-scoped temp directory (`TMP_PATH`). There is no separate state store or global singleton beyond `QSettings` (persisted app preferences in `JWLManager.conf`) and a lockfile (`write_lockfile`, `JWLManager.py:3978`) used to detect a second instance operating on the same archive.

## Key Abstractions

**Category/grouping tree cache (`self.tree_cache`):**
- Purpose: avoid re-querying SQLite/rebuilding `polars` DataFrames every time the user switches between categories (Annotations/Bookmarks/etc.) or regroups the tree.
- Examples: populated/read in `regroup` (`JWLManager.py:551` area, `get_data`/`rebuild_cached`/`recurse` nested functions).
- Pattern: dict keyed by `[category][grouping] -> {'data': DataFrame, 'tree': cached node structure}`; invalidated wholesale on `archive_modified`.

**Category export/import dispatch functions:**
- Purpose: one function per data category (Annotations, Bookmarks, Favorites, Highlights, Notes, Playlists) for both querying (`get_*`) and exporting (`export_*`), following a consistent naming convention.
- Examples: `get_annotations`/`export_annotations`, `get_bookmarks`/`export_bookmarks`, etc. (`JWLManager.py`, nested inside `regroup` and `export_items` respectively).
- Pattern: nested closures capturing shared locals (`cat`, `con`, `TMP_PATH`) rather than standalone module-level functions — keeps them scoped to the single `Window` method call but makes them hard to test in isolation.

**ctypes FFI wrapper (`jwlcore.py`):**
- Purpose: isolate all platform-specific native-library loading and C-type marshalling behind three simple Python functions.
- Examples: `merge_databases`, `get_last_result`, `get_core_version`.
- Pattern: module-level `CDLL` load at import time (fails fast if the platform lib is missing); explicit `argtypes`/`restype` on every native call for safety.

## Entry Points

**`JWLManager.py` (script entry, `if __name__ == '__main__'` at file bottom / `main()`-style flow via `argparse`):**
- Location: `JWLManager.py`.
- Triggers: run directly (`python3 JWLManager.py [archive] [-lang]`) or via bundled OS executable (PyInstaller, per README).
- Responsibilities: parse CLI args (`argparse`, language flags, optional archive path), set up `gettext`/`QTranslator` locale, instantiate `QApplication` + `Window`, run Qt event loop.

## Architectural Constraints

- **Threading:** Primarily single-threaded Qt event loop; the native merge call is synchronous (blocking) from Python's perspective, with progress reported via a C callback (`CALLBACKTYPE`) rather than a background thread — long merges will block the UI unless a `QProgressDialog`/`processEvents` pattern is used around it (see `QProgressDialog` import in `JWLManager.py`).
- **Global state:** Module-level `lib` (loaded `CDLL` handle) in `jwlcore.py` is a de facto singleton — reloading/swapping the native library at runtime is not supported. `Window` itself holds nearly all app state as instance attributes; no other module holds shared mutable state.
- **Temp-directory lifecycle:** Every open archive is fully extracted to a new `mkdtemp()` directory (`TMP_PATH`); nothing enforces cleanup ordering if the app crashes mid-session (crash handling exists via `crash_box`, `JWLManager.py:310`, but this is user-facing, not a guaranteed temp-file cleanup).
- **Native binary coupling:** `jwlcore.py` requires an exact-name prebuilt binary in `libs/` (or bundled via PyInstaller `_MEIPASS`) per platform; there is no fallback pure-Python merge path, no source for the native lib in this repo (compiled artifacts only).

## Anti-Patterns

### God object / no service layer

**What happens:** `class Window` in `JWLManager.py` implements UI event handling, SQL querying, DataFrame transforms, zip/file I/O, export formatting, and business rules for merge/tag/delete all in one ~4000-line class, frequently via deeply nested inner functions (e.g., `regroup` contains `get_data`, `process_code`, `process_color`, `process_detail`, `merge_df`, `get_annotations`, ... `build_tree`, `traverse`, `rebuild_cached`, `recurse`, `define_views` — all nested inside one method body).
**Why it's wrong:** Nested closures can't be unit-tested independently, code navigation requires scrolling a single huge method, and any future GUI framework swap or headless/CLI mode would require rewriting nearly everything.
**Do this instead:** New category-specific logic (query/export/import) should be extracted to standalone functions/modules (e.g., a `data/` or `services/` module per category) that accept explicit parameters (DB connection, category name) rather than closing over `self`/locals — even without a full refactor, new features should avoid adding further nesting depth to `regroup`/`export_items`.

### Direct SQL string construction inline with UI code

**What happens:** SQL queries are built and executed directly inside UI-triggered methods (`regroup`, tag/color-change handlers, `upgrade_schema`, `downgrade_schema`) rather than through a data-access module.
**Why it's wrong:** Schema-version-specific SQL (`upgrade_schema`, `JWLManager.py:1016`; `downgrade_schema`, `JWLManager.py:1172`) is scattered across the file, making it easy to miss a required update when the JW Library backup schema changes version.
**Do this instead:** Centralize schema-version SQL and category query strings; when adding a new schema version, search for all `sqlite3.connect` call sites in `JWLManager.py` (`regroup`, `zip_file`, `upgrade_schema`) rather than assuming one location covers it.

## Error Handling

**Strategy:** Broad `try`/`except` around risky I/O (archive validity, native lib calls) surfaced to the user via `QMessageBox`; unhandled exceptions routed to a custom crash dialog.

**Patterns:**
- `check_validity` (`JWLManager.py:994`) wraps `ZipFile` open in a try/except to detect a non-archive or corrupt file before proceeding.
- `crash_box` (`JWLManager.py:310`) is a dedicated dialog (uses `traceback.format_exception`) offering to send a crash report (`send_report`/`do_send`, nested), used as a global excepthook.
- Native lib results are checked by return code from `merge_databases` plus a follow-up `get_last_result()` string for detail, rather than raised exceptions crossing the FFI boundary.

## Cross-Cutting Concerns

**Logging:** No structured logging framework; errors surface via Qt dialogs and the crash-report flow (`crash_box`) rather than log files.
**Validation:** File-type/archive validation via `check_validity` (zip signature check + expected members) and `puremagic` (magic-byte detection, imported at top of `JWLManager.py`); no schema/data validation library — relies on SQL constraints and manual checks.
**Authentication:** Not applicable — local desktop file-editing tool, no auth/user accounts. A simple lockfile (`write_lockfile`) guards against two instances editing the same archive concurrently.

---

*Architecture analysis: 2026-07-16*
