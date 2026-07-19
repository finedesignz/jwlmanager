<!-- GSD:project-start source:PROJECT.md -->
## Project

**JWL Manager (Tauri)**

A cross-platform desktop app for managing `.jwlibrary` backup archives from JW Library — viewing, editing, exporting, importing, and merging personal study data (notes, highlights, bookmarks, tags, annotations, playlists, favorites). This is a from-scratch Tauri rewrite (Rust core + web frontend) replacing the existing PySide6 Python app, built to reach parity slice by slice and then go beyond it.

**Core Value:** **Never lose or corrupt a user's archive.** These are years of irreplaceable personal study notes. If everything else fails, the data must survive intact.

### Constraints

- **Tech stack**: Tauri v2 (Rust core + web frontend) — replaces PySide6. Rust binds `jwlCore` via `libloading`.
- **Compatibility**: Must read and write archives interchangeably with the existing Python app and JW Library itself. Format warts are load-bearing and must be preserved (`'None'` null sentinel, `|`→`¦` escaping, `==={END}===` parser sentinel, compact manifest JSON separators).
- **Data safety**: Save is not byte-preserving (`trim_db` + VACUUM). Parity must be verified semantically (normalized table state), never by byte-diffing outputs.
- **Security**: Fix zip-slip (`ZipFile.extractall` equivalent must validate paths). No f-string/format-string SQL interpolation — parameterize.
- **Licensing**: MIT. Do not ingest Infiniti Noncommercial (jwlFusion) or unlicensed (`NOASSERTION`) sibling code.
- **Platform**: Windows (incl. arm64), macOS, Linux. Linux WebKitGTK has documented performance issues with DOM-heavy grids — virtualize any large list.
- **Bandwidth**: Solo/hobby scale. Vertical MVP slices so every phase ships working value.
<!-- GSD:project-end -->

<!-- GSD:stack-start source:codebase/STACK.md -->
## Technology Stack

## Languages
- Python 3.11+ - Main application logic: `JWLManager.py` (4077 lines), `jwlcore.py` (83 lines), `res/ui_extras.py` (640 lines), `res/ui_main_window.py` (536 lines, PySide6-uic generated)
- C/C++ (compiled, vendored) - `jwlCore` native shared library providing fast merge/upgrade-schema operations, distributed as prebuilt binaries: `libs/jwlCore-amd64.dll`, `libs/libjwlCore-x86_64.so`, `libs/libjwlCore-arm64.so`, `libs/libjwlCore.dylib`. Source not in this repo; built/configured via `.github/workflows/jwlCore.config`.
- SQL (SQLite dialect) - inline queries throughout `JWLManager.py` against `.jwlibrary` (SQLite) archives
## Runtime
- CPython 3.11+ (per `README.md`)
- No virtualenv/poetry lockfile checked in — plain `pip install -r res/requirements.txt`
- pip
- Lockfile: missing (requirements files are unpinned except PySide6/shiboken6)
## Frameworks
- PySide6 `==6.9.*` (Qt for Python) - GUI framework, all widgets/dialogs/signals in `JWLManager.py`, `res/ui_extras.py`, `res/ui_main_window.py`
- shiboken6 `==6.9.*` - PySide6 binding generator runtime dependency
- None detected — no test framework, no `tests/` directory, no CI test step (workflows are build-only)
- PyInstaller (implied by `sys._MEIPASS` handling in `jwlcore.py:43` and `.github/workflows/JWLManager.exe.spec`, `JWLManager.zip.spec`) - packages app + Python + deps into self-contained binaries per platform
- GitHub Actions - `.github/workflows/build_linux.yml`, `build_macOS.yml`, `build_windows.yml`, `release.yml`
## Key Dependencies
- `PySide6` / `shiboken6` - entire UI layer
- `polars` - dataframe operations for spreadsheet/CSV import-export (`import polars as pl` in `JWLManager.py:57`)
- `XlsxWriter` - writes `.xlsx` exports (`xlsxwriter.Workbook`)
- `xlsx2csv` - reads `.xlsx` for import
- `Pillow` (`PIL.Image`) - image/thumbnail handling for playlist media
- `regex` - advanced regex operations beyond stdlib `re`
- `puremagic` - file-type sniffing for imported media/attachments
- `requests` - GitHub release-check HTTP calls, telemetry POST
- `certifi` - CA bundle for `requests`/TLS
- `sqlite3` (stdlib) - reads/writes the `.jwlibrary` archive's internal `userData.db` SQLite database, and bundled `res/resources.db`
- `ctypes` (stdlib) - FFI bridge to native `jwlCore` library, wrapped in `jwlcore.py`
- `gettext` (stdlib) - i18n/l10n, translation catalogs under `res/locales/{de,en,es,fr,it,pl,pt,ru,uk}`
## Configuration
- No `.env`/environment-variable-based config
- Runtime state persisted to `<app_dir>/JWLManager.conf` via `QSettings` (INI format) — see `JWLManager.py:3971-3976`
- CLI args via `argparse` — language override, e.g. `python3 JWLManager.py -es`
- `.github/workflows/JWLManager.exe.spec` — PyInstaller spec for Windows exe bundle
- `.github/workflows/JWLManager.zip.spec` — PyInstaller spec for zip/other-platform bundle
- `.github/workflows/jwlCore.config` — native lib build config
## Platform Requirements
- Python 3.11+
- Qt 6.9 runtime (via PySide6 wheel, no separate Qt install needed)
- Platform-specific native `jwlCore` binary must exist alongside script (`libs/` on Linux/macOS, project root on Windows per `jwlcore.py:_load_lib`)
- Distributed as self-contained platform binaries (Linux binary, Windows .exe, macOS .app) via GitHub Releases — no Python install needed by end user
- Windows: unsigned executable triggers SmartScreen warning (see `.github/SECURITY.md`)
- macOS: requires `xattr -cr JWLManager.app` to bypass Gatekeeper quarantine (unsigned/unnotarized)
- Linux: requires `chmod +x`
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

## Naming Patterns
- Snake_case module files: `jwlcore.py`, `res/ui_extras.py`, `res/ui_main_window.py`
- Entry point uses PascalCase to match app name: `JWLManager.py`
- snake_case throughout, verb-first: `load_file`, `save_as_file`, `check_validity`, `merge_databases` (`JWLManager.py`, `jwlcore.py`)
- Private/internal helpers prefixed with single underscore: `_platform_lib_name`, `_load_lib` (`jwlcore.py:29`, `jwlcore.py:38`)
- Nested closures used heavily inside methods for local helper logic, e.g. `center()`, `connect_signals()`, `set_vars()` defined inside `Window.__init__` (`JWLManager.py:73-116`); `send_report()`/`do_send()` nested inside `crash_box` (`JWLManager.py:310-345`)
- snake_case, descriptive: `save_filename`, `title_format`, `int_total`, `tmp_path`
- Module-level constants in ALL_CAPS: `APP`, `VERSION`, `CORE_VERSION`, `PROJECT_PATH`, `TMP_PATH`, `DB_NAME`, `CALLBACKTYPE` (`JWLManager.py:27-63`)
- PascalCase: `Window(QMainWindow, Ui_MainWindow)` (`JWLManager.py:69`), `AboutBox`, `HelpBox`, `DataViewer`, `DropList`, `MergeDialog`, `TagDialog`, `ThemeManager`, `ViewerItem` (`res/ui_extras.py`)
## Code Style
- No formatter/linter config present (no `.flake8`, `pyproject.toml`, `.pylintrc`, or `pre-commit` config found in repo root)
- Style is manual/consistent-by-convention rather than tool-enforced
- Multiple imports per line for stdlib grouped by usage: `import argparse, ctypes, gettext, json, puremagic, os, regex, requests, shutil, sqlite3, sys, uuid` (`JWLManager.py:57`)
- Long lines tolerated, especially for Qt widget wiring and PySide6 imports (single import line spans 200+ chars) (`JWLManager.py:35`)
- Every top-level module file opens with a triple-quoted MIT license header block, not a functional docstring (`JWLManager.py:3-25`, `jwlcore.py:3-24`)
- Function/method-level docstrings are rare; code relies on descriptive naming instead
## Import Organization
- None (no bundler/module alias system; plain relative package imports via `res.*`)
## Error Handling
- Broad bare `except:` is the dominant pattern for UI-facing operations, silently swallowing exceptions in file/dialog flows (e.g. `JWLManager.py:975`, `:1095`, `:1105`, `:1116`, `:1899`, `:1931`, `:1952`, `:2029`, `:2109`, `:2197`, `:2257`, `:2417`, `:2438`)
- `except Exception as ex:` used where the exception is surfaced to the user via `crash_box(ex, ...)` for unexpected/unhandled failures (`JWLManager.py:942`, `:1848`, `:2630`, `:2676`)
- Crash reporting funnels through a single `crash_box(self, ex, msg=None)` method that builds a traceback string via `format_exception` and offers to POST it to `https://ntfy.sh/reganamlwj` (`JWLManager.py:310-360`)
- Network/report-send failures inside the crash handler itself are caught and only `print()`-ed, never re-raised (`JWLManager.py:331-342`)
- Library boundary (`jwlcore.py`) raises typed `OSError` with descriptive messages instead of swallowing errors: `raise OSError(f"Unsupported platform: {sysname}")` (`jwlcore.py:35`), `raise OSError(f"Could not find shared library {name} at {lib_path}")` (`jwlcore.py:53`)
## Logging
- Diagnostic output uses `print()` sparingly, mainly for crash-report send failures (`JWLManager.py:341`)
- User-facing errors surface through `crash_box` dialog (traceback + optional user comment), not log files
## Comments
- Comments are sparse; used mainly to label import groupings (`# Python wrappers` in `jwlcore.py:70`) or flag intent inline
- Code favors self-explanatory names and short nested functions over comment blocks
- Not applicable (Python). Type hints used selectively for public wrapper functions in `jwlcore.py`: `def merge_databases(path1: str, path2: str, downgrade: bool = False) -> int:`, `def get_last_result() -> str | None:` — the ctypes bridge module is the most consistently type-hinted part of the codebase. `JWLManager.py` mostly omits type hints.
## Function Design
- `JWLManager.py` methods are large and monolithic (single `Window` class spans the entire 4077-line file); many methods (e.g. `regroup`, `export_items`, `import_items`) run several hundred lines and use nested closures for sub-steps rather than extracting separate top-level functions
- `jwlcore.py` functions are small, single-purpose (3-6 lines), reflecting its role as a thin ctypes bridge
- Optional params use Python default values liberally, e.g. `def load_file(self, archive=''):`, `def merge_items(self, file=''):`, `def crash_box(self, ex, msg=None):`
- Bridge functions in `jwlcore.py` return typed primitives (`int`, `str | None`) matching the underlying C ABI
- `Window` methods mostly return `None`/perform side effects (Qt UI mutation) rather than returning values
## Module Design
- No `__all__` declarations; modules are imported by explicit name (`from jwlcore import merge_databases, get_core_version, get_last_result, lib, CALLBACKTYPE`)
- `res/ui_main_window.py` is Qt Designer generated UI code (not hand-edited) mixed with hand-written `res/ui_extras.py` helper dialog classes
- None — flat structure: single entry point (`JWLManager.py`), one native bridge module (`jwlcore.py`), UI assets/helpers under `res/`
## Localization Convention
- All user-facing strings wrapped in gettext `_()` calls, e.g. `_('Send crash report')`, `_('Oops! Something went wrong…')` (`JWLManager.py:314-350`)
- Translation catalogs under `res/locales/`; any new user-facing string must be wrapped in `_()` to stay translatable
## Native Library Bridge Convention (jwlcore.py)
- Platform-specific shared library resolution centralized in `_platform_lib_name()` / `_load_lib()` (`jwlcore.py:29-53`)
- ctypes `argtypes`/`restype` explicitly declared for every exported C function before wrapping (`jwlcore.py:58-67`)
- Each C function gets a thin, typed Python wrapper function rather than being called directly from UI code — follow this pattern for any new native calls
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

## System Overview
```text
```
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
- Single class (`Window` in `JWLManager.py`, ~4000 lines) owns nearly all behavior: file I/O, business logic, UI event handlers, and rendering — no separate model/service layer.
- Archive is treated as a temp-extracted SQLite DB + zip container; state lives on disk in a per-session temp directory (`mkdtemp()`), not in a persistent server or long-lived DB connection.
- Data querying/manipulation for tree views done via `polars` DataFrames (`import polars as pl`) computed on demand from SQL query results, then cached in `self.tree_cache` (a dict keyed by category/grouping).
- Native library is a stateless utility invoked once per merge action — not a long-running service, no IPC beyond a single function call + callback for progress.
- i18n via `gettext` + Qt `QTranslator`, resources compiled per-locale under `res/locales/<lang>/LC_MESSAGES`.
## Layers
- Purpose: render tree of Annotations/Bookmarks/Favorites/Highlights/Notes/Playlists, menus, dialogs, drag-drop, theming.
- Location: `JWLManager.py` (methods under `class Window`), `res/ui_main_window.py`, `res/ui_extras.py`.
- Depends on: PySide6 (`QtCore`, `QtGui`, `QtWidgets`).
- Used by: entry point only (this is the top layer).
- Purpose: extract `.jwlibrary` (zip) to temp dir, open/query/mutate the embedded SQLite `user_data.db`, rebuild/export the zip on save, manage schema upgrade/downgrade.
- Location: `JWLManager.py` — `load_file`, `zip_file`, `check_validity`, `upgrade_schema`, `regroup`/`get_annotations`/`get_bookmarks`/etc. inner functions, `export_items`/`export_file`.
- Contains: raw `sqlite3` calls (parameterized SQL strings), `polars` DataFrame transforms, `zipfile` archive manipulation.
- Depends on: `sqlite3`, `polars`, `zipfile`, `res/blank` template.
- Used by: presentation layer (called directly from UI event handlers — no intermediary service objects).
- Purpose: expose the compiled jwlCore merge/version functions to Python.
- Location: `jwlcore.py`.
- Contains: ctypes `CDLL` load logic (platform-name resolution `_platform_lib_name`, PyInstaller `_MEIPASS` awareness), `argtypes`/`restype` declarations, thin wrapper functions (`merge_databases`, `get_last_result`, `get_core_version`).
- Depends on: `libs/*` prebuilt shared libraries.
- Used by: `JWLManager.py` merge_file / merge dialog flow (imports `merge_databases, get_core_version, get_last_result, lib, CALLBACKTYPE`).
## Data Flow
### Primary Request Path (open → view → edit → save)
### Merge Flow
### Export Flow
- All working state (extracted archive, temp DB, dirty flag, tree cache, current selection) lives as instance attributes on the single `Window` object and files under a session-scoped temp directory (`TMP_PATH`). There is no separate state store or global singleton beyond `QSettings` (persisted app preferences in `JWLManager.conf`) and a lockfile (`write_lockfile`, `JWLManager.py:3978`) used to detect a second instance operating on the same archive.
## Key Abstractions
- Purpose: avoid re-querying SQLite/rebuilding `polars` DataFrames every time the user switches between categories (Annotations/Bookmarks/etc.) or regroups the tree.
- Examples: populated/read in `regroup` (`JWLManager.py:551` area, `get_data`/`rebuild_cached`/`recurse` nested functions).
- Pattern: dict keyed by `[category][grouping] -> {'data': DataFrame, 'tree': cached node structure}`; invalidated wholesale on `archive_modified`.
- Purpose: one function per data category (Annotations, Bookmarks, Favorites, Highlights, Notes, Playlists) for both querying (`get_*`) and exporting (`export_*`), following a consistent naming convention.
- Examples: `get_annotations`/`export_annotations`, `get_bookmarks`/`export_bookmarks`, etc. (`JWLManager.py`, nested inside `regroup` and `export_items` respectively).
- Pattern: nested closures capturing shared locals (`cat`, `con`, `TMP_PATH`) rather than standalone module-level functions — keeps them scoped to the single `Window` method call but makes them hard to test in isolation.
- Purpose: isolate all platform-specific native-library loading and C-type marshalling behind three simple Python functions.
- Examples: `merge_databases`, `get_last_result`, `get_core_version`.
- Pattern: module-level `CDLL` load at import time (fails fast if the platform lib is missing); explicit `argtypes`/`restype` on every native call for safety.
## Entry Points
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
### Direct SQL string construction inline with UI code
## Error Handling
- `check_validity` (`JWLManager.py:994`) wraps `ZipFile` open in a try/except to detect a non-archive or corrupt file before proceeding.
- `crash_box` (`JWLManager.py:310`) is a dedicated dialog (uses `traceback.format_exception`) offering to send a crash report (`send_report`/`do_send`, nested), used as a global excepthook.
- Native lib results are checked by return code from `merge_databases` plus a follow-up `get_last_result()` string for detail, rather than raised exceptions crossing the FFI boundary.
## Cross-Cutting Concerns
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
