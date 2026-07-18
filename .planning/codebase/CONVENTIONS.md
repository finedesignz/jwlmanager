# Coding Conventions

**Analysis Date:** 2026-07-16

## Naming Patterns

**Files:**
- Snake_case module files: `jwlcore.py`, `res/ui_extras.py`, `res/ui_main_window.py`
- Entry point uses PascalCase to match app name: `JWLManager.py`

**Functions:**
- snake_case throughout, verb-first: `load_file`, `save_as_file`, `check_validity`, `merge_databases` (`JWLManager.py`, `jwlcore.py`)
- Private/internal helpers prefixed with single underscore: `_platform_lib_name`, `_load_lib` (`jwlcore.py:29`, `jwlcore.py:38`)
- Nested closures used heavily inside methods for local helper logic, e.g. `center()`, `connect_signals()`, `set_vars()` defined inside `Window.__init__` (`JWLManager.py:73-116`); `send_report()`/`do_send()` nested inside `crash_box` (`JWLManager.py:310-345`)

**Variables:**
- snake_case, descriptive: `save_filename`, `title_format`, `int_total`, `tmp_path`
- Module-level constants in ALL_CAPS: `APP`, `VERSION`, `CORE_VERSION`, `PROJECT_PATH`, `TMP_PATH`, `DB_NAME`, `CALLBACKTYPE` (`JWLManager.py:27-63`)

**Classes:**
- PascalCase: `Window(QMainWindow, Ui_MainWindow)` (`JWLManager.py:69`), `AboutBox`, `HelpBox`, `DataViewer`, `DropList`, `MergeDialog`, `TagDialog`, `ThemeManager`, `ViewerItem` (`res/ui_extras.py`)

## Code Style

**Formatting:**
- No formatter/linter config present (no `.flake8`, `pyproject.toml`, `.pylintrc`, or `pre-commit` config found in repo root)
- Style is manual/consistent-by-convention rather than tool-enforced
- Multiple imports per line for stdlib grouped by usage: `import argparse, ctypes, gettext, json, puremagic, os, regex, requests, shutil, sqlite3, sys, uuid` (`JWLManager.py:57`)

**Line length:**
- Long lines tolerated, especially for Qt widget wiring and PySide6 imports (single import line spans 200+ chars) (`JWLManager.py:35`)

**Docstrings:**
- Every top-level module file opens with a triple-quoted MIT license header block, not a functional docstring (`JWLManager.py:3-25`, `jwlcore.py:3-24`)
- Function/method-level docstrings are rare; code relies on descriptive naming instead

## Import Organization

**Order observed in `JWLManager.py`:**
1. Module constants (`APP`, `VERSION`, `BETA`) declared before imports
2. Local UI modules: `from res.ui_main_window import Ui_MainWindow`, `from res.ui_extras import ...`
3. PySide6 (Qt) imports grouped by submodule: `QtCore`, `QtGui`, `QtWidgets`
4. Stdlib `from X import Y` imports (alphabetized by module name): `collections`, `datetime`, `functools`, `glob`, `hashlib`, `pathlib`, `PIL`, `platform`, `random`, `tempfile`, `textwrap`, `time`, `traceback`, `xlsxwriter`, `zipfile`
5. Bare stdlib `import` statements comma-separated on one line
6. Third-party `import polars as pl`
7. Local project import last: `from jwlcore import merge_databases, get_core_version, get_last_result, lib, CALLBACKTYPE`

**Path Aliases:**
- None (no bundler/module alias system; plain relative package imports via `res.*`)

## Error Handling

**Patterns:**
- Broad bare `except:` is the dominant pattern for UI-facing operations, silently swallowing exceptions in file/dialog flows (e.g. `JWLManager.py:975`, `:1095`, `:1105`, `:1116`, `:1899`, `:1931`, `:1952`, `:2029`, `:2109`, `:2197`, `:2257`, `:2417`, `:2438`)
- `except Exception as ex:` used where the exception is surfaced to the user via `crash_box(ex, ...)` for unexpected/unhandled failures (`JWLManager.py:942`, `:1848`, `:2630`, `:2676`)
- Crash reporting funnels through a single `crash_box(self, ex, msg=None)` method that builds a traceback string via `format_exception` and offers to POST it to `https://ntfy.sh/reganamlwj` (`JWLManager.py:310-360`)
- Network/report-send failures inside the crash handler itself are caught and only `print()`-ed, never re-raised (`JWLManager.py:331-342`)
- Library boundary (`jwlcore.py`) raises typed `OSError` with descriptive messages instead of swallowing errors: `raise OSError(f"Unsupported platform: {sysname}")` (`jwlcore.py:35`), `raise OSError(f"Could not find shared library {name} at {lib_path}")` (`jwlcore.py:53`)

**Guidance for new code:** Follow the existing split — low-level bridge/library code (`jwlcore.py` style) should raise specific exceptions with clear messages; UI-layer code (`JWLManager.py` style) should catch broadly and route unexpected failures to `crash_box`, not print silently to console, when the exception is unexpected. Avoid adding more bare `except:` without a comment on why it's safe to ignore — this is legacy pattern, not necessarily to be replicated for new code paths.

## Logging

**Framework:** None — no `logging` module usage found in the codebase

**Patterns:**
- Diagnostic output uses `print()` sparingly, mainly for crash-report send failures (`JWLManager.py:341`)
- User-facing errors surface through `crash_box` dialog (traceback + optional user comment), not log files

## Comments

**When to Comment:**
- Comments are sparse; used mainly to label import groupings (`# Python wrappers` in `jwlcore.py:70`) or flag intent inline
- Code favors self-explanatory names and short nested functions over comment blocks

**JSDoc/TSDoc equivalent:**
- Not applicable (Python). Type hints used selectively for public wrapper functions in `jwlcore.py`: `def merge_databases(path1: str, path2: str, downgrade: bool = False) -> int:`, `def get_last_result() -> str | None:` — the ctypes bridge module is the most consistently type-hinted part of the codebase. `JWLManager.py` mostly omits type hints.

## Function Design

**Size:**
- `JWLManager.py` methods are large and monolithic (single `Window` class spans the entire 4077-line file); many methods (e.g. `regroup`, `export_items`, `import_items`) run several hundred lines and use nested closures for sub-steps rather than extracting separate top-level functions
- `jwlcore.py` functions are small, single-purpose (3-6 lines), reflecting its role as a thin ctypes bridge

**Parameters:**
- Optional params use Python default values liberally, e.g. `def load_file(self, archive=''):`, `def merge_items(self, file=''):`, `def crash_box(self, ex, msg=None):`

**Return Values:**
- Bridge functions in `jwlcore.py` return typed primitives (`int`, `str | None`) matching the underlying C ABI
- `Window` methods mostly return `None`/perform side effects (Qt UI mutation) rather than returning values

## Module Design

**Exports:**
- No `__all__` declarations; modules are imported by explicit name (`from jwlcore import merge_databases, get_core_version, get_last_result, lib, CALLBACKTYPE`)
- `res/ui_main_window.py` is Qt Designer generated UI code (not hand-edited) mixed with hand-written `res/ui_extras.py` helper dialog classes

**Barrel Files:**
- None — flat structure: single entry point (`JWLManager.py`), one native bridge module (`jwlcore.py`), UI assets/helpers under `res/`

## Localization Convention

- All user-facing strings wrapped in gettext `_()` calls, e.g. `_('Send crash report')`, `_('Oops! Something went wrong…')` (`JWLManager.py:314-350`)
- Translation catalogs under `res/locales/`; any new user-facing string must be wrapped in `_()` to stay translatable

## Native Library Bridge Convention (jwlcore.py)

- Platform-specific shared library resolution centralized in `_platform_lib_name()` / `_load_lib()` (`jwlcore.py:29-53`)
- ctypes `argtypes`/`restype` explicitly declared for every exported C function before wrapping (`jwlcore.py:58-67`)
- Each C function gets a thin, typed Python wrapper function rather than being called directly from UI code — follow this pattern for any new native calls

---

*Convention analysis: 2026-07-16*
</content>
