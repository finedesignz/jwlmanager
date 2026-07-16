# Codebase Structure

**Analysis Date:** 2026-07-16

## Directory Layout

```
jwlmanager/
├── JWLManager.py           # Main application entry point + Window (God-object GUI/logic class)
├── jwlcore.py              # ctypes bridge to prebuilt native jwlCore merge library
├── libs/                   # Prebuilt (vendored, binary) native libraries — no source
│   ├── jwlCore-amd64.dll
│   ├── libjwlCore-x86_64.so
│   ├── libjwlCore-arm64.so
│   ├── libjwlCore.dylib
│   └── sqlite3_64.dll
├── res/                    # All non-code resources + Qt-generated UI code
│   ├── ui_main_window.py   # Qt Designer–generated main window layout (Ui_MainWindow)
│   ├── ui_extras.py        # Hand-written dialog/helper widget classes
│   ├── blank                # Empty .jwlibrary template archive (for "New")
│   ├── blank_playlist       # Empty playlist template archive
│   ├── resources.db         # Bundled lookup DB (verse/publication references)
│   ├── dark.qss / light.qss # Qt stylesheets for theming
│   ├── icons/dark/, icons/light/  # Theme-specific icon assets
│   ├── locales/<lang>/LC_MESSAGES/ # gettext .po/.mo translation catalogs (de, en, es, fr, it, pl, pt, ru, uk)
│   ├── requirements.txt / requirements-winarm.txt  # pip dependency lists (standard vs ARM64/older Intel)
│   └── HELP.md / HILFE.md   # End-user help docs (English / German)
├── .github/
│   ├── workflows/            # CI (build/release, CodeQL scan per README badges)
│   └── SECURITY.md           # Windows SmartScreen / security notice
├── .planning/                # GSD planning artifacts (this analysis lives under codebase/)
├── CHANGELOG.md
├── README.md
└── LICENSE
```

## Directory Purposes

**Root (`.`):**
- Purpose: application entry point and native-library bridge live at top level (no `src/` layout).
- Contains: `JWLManager.py` (main script), `jwlcore.py` (FFI bridge), project metadata files.
- Key files: `JWLManager.py`, `jwlcore.py`.

**`libs/`:**
- Purpose: holds precompiled, platform-specific native binaries the app loads at runtime via ctypes.
- Contains: one shared library per OS/arch target (`jwlCore-amd64.dll` for Windows, `libjwlCore-x86_64.so` for Linux, `libjwlCore-arm64.so` for Linux ARM64, `libjwlCore.dylib` for macOS) plus a vendored `sqlite3_64.dll` (likely a Windows sqlite3 DLL dependency).
- Key files: resolved dynamically by `_platform_lib_name()` in `jwlcore.py`; no source code for these binaries in this repo (built elsewhere, committed as artifacts).

**`res/`:**
- Purpose: catch-all for everything that isn't the two top-level Python files — generated UI code, translations, stylesheets, icons, document templates, and end-user docs.
- Contains: Qt-generated (`ui_main_window.py`) and hand-authored (`ui_extras.py`) Python UI modules; binary/template archives (`blank`, `blank_playlist`, `resources.db`); Qt stylesheets (`.qss`); icon sets split by theme; gettext locale trees; pip requirement files; markdown help docs.
- Key files: `res/ui_main_window.py`, `res/ui_extras.py`, `res/blank`, `res/resources.db`.

**`res/locales/<lang>/LC_MESSAGES/`:**
- Purpose: gettext translation catalogs per supported language (German, English, Spanish, French, Italian, Polish, Portuguese, Russian, Ukrainian).
- Contains: compiled/raw gettext files consumed via `gettext` + Qt `QTranslator` (`JWLManager.py` imports both).
- Also present: `res/locales/Resources`, `res/locales/UI` — likely source `.pot`/template directories feeding the per-language catalogs (Weblate-based translation workflow per README).

**`.github/workflows/`:**
- Purpose: CI pipelines for release builds (per-platform executables referenced in README) and CodeQL security scanning.

## Key File Locations

**Entry Point:**
- `JWLManager.py`: parses CLI args (`argparse`), sets up i18n, builds `QApplication`, instantiates `Window`, starts the Qt event loop.

**Configuration:**
- No repo-level app config file; runtime user settings persisted externally in a `JWLManager.conf` file (via `QSettings`, `JWLManager/*` keys) created next to the installed app, not in this repo.
- `res/requirements.txt` / `res/requirements-winarm.txt`: pip dependency manifests (two variants for ARM64/older Intel CPUs — no `pyproject.toml`/lockfile).

**Core Logic:**
- `JWLManager.py`: everything — archive load/save (`load_file`, `zip_file`), tree building (`regroup`, `build_tree`), category get/export functions, schema upgrade/downgrade (`upgrade_schema`, `downgrade_schema`), merge orchestration (`merge_file`), export to xlsx/text/markdown (`export_items`).
- `jwlcore.py`: native library loading and FFI declarations.

**Testing:**
- No test directory or test framework detected in this repo (no `tests/`, no `pytest.ini`/`test_*.py` found).

**UI:**
- `res/ui_main_window.py`: generated main window layout — do not hand-edit if a `.ui` source exists elsewhere; regenerate via Qt Designer/`pyside6-uic` instead.
- `res/ui_extras.py`: hand-written dialogs/widgets (`AboutBox`, `HelpBox`, `DataViewer`, `DropList`, `MergeDialog`, `TagDialog`, `ThemeManager`, `ViewerItem`) — safe to edit directly.

## Naming Conventions

**Files:**
- Top-level Python modules: `PascalCase` for the main app script (`JWLManager.py`, matching the app/class name), lowercase for the support bridge (`jwlcore.py`).
- Resource/UI modules: `ui_*.py` prefix under `res/` (`ui_main_window.py`, `ui_extras.py`).
- Native libs: `<lib{base}>-<arch>.<ext>` per-platform naming resolved programmatically (`libjwlCore-x86_64.so`, `jwlCore-amd64.dll`, `libjwlCore.dylib`, `libjwlCore-arm64.so`) — must match `_platform_lib_name()` in `jwlcore.py` exactly if adding a new target.

**Directories:**
- `res/locales/<2-letter-lang-code>/LC_MESSAGES/` — standard gettext locale directory layout.
- `res/icons/<dark|light>/` — theme-scoped icon sets, mirrored structure per theme.

**Code (within `JWLManager.py`):**
- Methods: `snake_case` (`load_file`, `check_validity`, `tree_selection`).
- Per-category functions follow `get_<category>`/`export_<category>` naming (`get_annotations`/`export_annotations`, `get_bookmarks`/`export_bookmarks`, etc.) — nested inside `regroup`/`export_items` respectively.
- Classes: `PascalCase` (`Window`, `MergeDialog`, `TagDialog`, `ThemeManager`).

## Where to Add New Code

**New data category or export format:**
- Add a `get_<category>` function alongside the existing ones nested in `regroup` (`JWLManager.py`, ~line 641-777) for tree population.
- Add a matching `export_<category>` function alongside existing exporters nested in `export_items` (`JWLManager.py`, ~line 1307 onward) for spreadsheet/text export.
- Follow the existing naming convention exactly — other code (menu enablement in `switchboard`/`disable_options`, category combo box wiring) likely keys off category name strings.

**New dialog or secondary window:**
- Implementation: add a new class to `res/ui_extras.py` alongside `MergeDialog`/`TagDialog`/`AboutBox`, following the existing `QDialog` subclass pattern.
- Wire it up from `JWLManager.py` (import it in the `from res.ui_extras import ...` line, instantiate from the relevant menu/button handler).

**New schema version support:**
- Add corresponding SQL to both `upgrade_schema` (`JWLManager.py:1016`) and `downgrade_schema` (nested in `zip_file`, `JWLManager.py:1172`) — these must stay in sync (upgrade on load, downgrade on save if the target app version is older).

**New translation:**
- Add a new `res/locales/<lang>/LC_MESSAGES/` directory with the compiled catalog; register the new language flag in the `argparse` language group in `JWLManager.py` and in `change_language`/README's documented `-xx` flags.

**New native-lib platform target:**
- Add the binary to `libs/` following the existing naming scheme, and add a branch to `_platform_lib_name()` in `jwlcore.py`.

## Special Directories

**`libs/`:**
- Purpose: vendored, prebuilt native binaries (no build step in this repo produces them).
- Generated: Yes (built from an external jwlCore Rust/C project not included here).
- Committed: Yes (binary artifacts checked in directly).

**`res/locales/`:**
- Purpose: i18n translation catalogs.
- Generated: Partially (compiled `.mo` likely generated from `.po` sources via a Weblate-driven workflow per README footnotes).
- Committed: Yes.

**`.planning/`:**
- Purpose: GSD planning artifacts (roadmap, phase plans, codebase maps) — not part of the shipped application.
- Generated: Yes (by GSD tooling).
- Committed: Yes (per user's global git-tracking default).

---

*Structure analysis: 2026-07-16*
