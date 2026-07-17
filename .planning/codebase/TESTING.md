# Testing Patterns

**Analysis Date:** 2026-07-16

## Test Framework

**Runner:**
- None present. No `pytest`, `unittest`, or other test runner configured.
- No `pytest.ini`, `tox.ini`, `pyproject.toml` `[tool.pytest]` section, `conftest.py`, or `setup.cfg` found in repo root.

**Assertion Library:**
- Not applicable — no test suite exists.

**Run Commands:**
```bash
# No test command exists. Confirmed by:
# - res/requirements.txt / res/requirements-winarm.txt contain no test/dev dependencies
#   (certifi, pillow, polars, puremagic, PySide6==6.9.*, regex, requests, shiboken6==6.9.*, xlsx2csv, XlsxWriter)
# - No CI workflow files found (no .github/workflows/*.yml)
# - No Makefile / tox.ini / noxfile.py present
```

## Test File Organization

**Location:**
- No test directory (`tests/`, `test/`) exists in the repo.

**Naming:**
- Not applicable — no test files (`*_test.py`, `test_*.py`) found anywhere in the tree.

**Structure:**
```
Not applicable. Current structure:
JWLManager.py       # main app + Window class (UI, business logic, file I/O all combined)
jwlcore.py           # thin ctypes bridge to native jwlCore libs
libs/                # prebuilt native libraries (dll/so/dylib) — no Python bindings to unit test in isolation
res/                 # Qt Designer generated UI (ui_main_window.py), hand-written dialogs (ui_extras.py), assets, locales
```

## Test Structure

Not applicable — no test suite exists in this codebase.

## Mocking

Not applicable — no mocking framework (`unittest.mock`, `pytest-mock`) is used or imported anywhere.

## Fixtures and Factories

**Test Data:**
- None. `res/blank` and `res/blank_playlist` are runtime template files consumed by the app (used for "New file" creation), not test fixtures — see references in `JWLManager.py` around `new_file`/`check_validity` (`JWLManager.py:965`, `:994`).
- `res/resources.db` is a bundled SQLite resource database used by the app itself, not test data.

**Location:**
- Not applicable.

## Coverage

**Requirements:** None enforced — no coverage tooling (`coverage.py`, `pytest-cov`) configured.

**View Coverage:**
```bash
# Not applicable, no coverage tooling present.
```

## Test Types

**Unit Tests:** None exist. The most unit-testable surface is `jwlcore.py` (pure functions `merge_databases`, `get_last_result`, `get_core_version`, `_platform_lib_name`) since it has minimal external side effects beyond the native library call.

**Integration Tests:** None exist. `JWLManager.py`'s `Window` class tightly couples Qt UI (PySide6 widgets/signals), filesystem operations (zipfile, sqlite3, shutil), and the native `jwlcore` bridge — this would require a Qt test harness (e.g. `pytest-qt`) plus fixture `.jwlibrary` archive files to test meaningfully.

**E2E Tests:** Not used. No Playwright/Selenium/UI automation tooling present; this is a desktop PySide6 app, not a web app, and no manual test scripts or checklists were found either.

## Recommendations for Adding Tests (none currently exist)

If a test suite is introduced:
- Use `pytest` (not stdlib `unittest`) to match the modern-Python style already used in `jwlcore.py` (type hints, f-strings)
- Isolate and test `jwlcore.py`'s pure wrapper functions first (`merge_databases`, `get_last_result`, `get_core_version`) — these are the only functions without direct Qt/UI coupling
- For `JWLManager.py`, consider `pytest-qt` (`qtbot` fixture) to drive `Window` interactions, since virtually all logic lives inside Qt-bound methods on the single `Window` class
- Sample `.jwlibrary` (zip/SQLite) fixture files would be needed under a new `tests/fixtures/` directory to exercise `load_file`, `save_file`, `merge_file`, `check_validity` (`JWLManager.py:994`, `:1010`, `:1077`, `:1121`)
- No existing convention to follow for assertions/mocking style — establishing pytest conventions from scratch is required

---

*Testing analysis: 2026-07-16*
</content>
