# Technology Stack

**Analysis Date:** 2026-07-16

## Languages

**Primary:**
- Python 3.11+ - Main application logic: `JWLManager.py` (4077 lines), `jwlcore.py` (83 lines), `res/ui_extras.py` (640 lines), `res/ui_main_window.py` (536 lines, PySide6-uic generated)

**Secondary:**
- C/C++ (compiled, vendored) - `jwlCore` native shared library providing fast merge/upgrade-schema operations, distributed as prebuilt binaries: `libs/jwlCore-amd64.dll`, `libs/libjwlCore-x86_64.so`, `libs/libjwlCore-arm64.so`, `libs/libjwlCore.dylib`. Source not in this repo; built/configured via `.github/workflows/jwlCore.config`.
- SQL (SQLite dialect) - inline queries throughout `JWLManager.py` against `.jwlibrary` (SQLite) archives

## Runtime

**Environment:**
- CPython 3.11+ (per `README.md`)
- No virtualenv/poetry lockfile checked in — plain `pip install -r res/requirements.txt`

**Package Manager:**
- pip
- Lockfile: missing (requirements files are unpinned except PySide6/shiboken6)
  - `res/requirements.txt`
  - `res/requirements-winarm.txt` (ARM64 / older Intel variant)

## Frameworks

**Core:**
- PySide6 `==6.9.*` (Qt for Python) - GUI framework, all widgets/dialogs/signals in `JWLManager.py`, `res/ui_extras.py`, `res/ui_main_window.py`
- shiboken6 `==6.9.*` - PySide6 binding generator runtime dependency

**Testing:**
- None detected — no test framework, no `tests/` directory, no CI test step (workflows are build-only)

**Build/Dev:**
- PyInstaller (implied by `sys._MEIPASS` handling in `jwlcore.py:43` and `.github/workflows/JWLManager.exe.spec`, `JWLManager.zip.spec`) - packages app + Python + deps into self-contained binaries per platform
- GitHub Actions - `.github/workflows/build_linux.yml`, `build_macOS.yml`, `build_windows.yml`, `release.yml`

## Key Dependencies

**Critical:**
- `PySide6` / `shiboken6` - entire UI layer
- `polars` - dataframe operations for spreadsheet/CSV import-export (`import polars as pl` in `JWLManager.py:57`)
- `XlsxWriter` - writes `.xlsx` exports (`xlsxwriter.Workbook`)
- `xlsx2csv` - reads `.xlsx` for import
- `Pillow` (`PIL.Image`) - image/thumbnail handling for playlist media
- `regex` - advanced regex operations beyond stdlib `re`
- `puremagic` - file-type sniffing for imported media/attachments
- `requests` - GitHub release-check HTTP calls, telemetry POST
- `certifi` - CA bundle for `requests`/TLS

**Infrastructure:**
- `sqlite3` (stdlib) - reads/writes the `.jwlibrary` archive's internal `userData.db` SQLite database, and bundled `res/resources.db`
- `ctypes` (stdlib) - FFI bridge to native `jwlCore` library, wrapped in `jwlcore.py`
- `gettext` (stdlib) - i18n/l10n, translation catalogs under `res/locales/{de,en,es,fr,it,pl,pt,ru,uk}`

## Configuration

**Environment:**
- No `.env`/environment-variable-based config
- Runtime state persisted to `<app_dir>/JWLManager.conf` via `QSettings` (INI format) — see `JWLManager.py:3971-3976`
- CLI args via `argparse` — language override, e.g. `python3 JWLManager.py -es`

**Build:**
- `.github/workflows/JWLManager.exe.spec` — PyInstaller spec for Windows exe bundle
- `.github/workflows/JWLManager.zip.spec` — PyInstaller spec for zip/other-platform bundle
- `.github/workflows/jwlCore.config` — native lib build config

## Platform Requirements

**Development:**
- Python 3.11+
- Qt 6.9 runtime (via PySide6 wheel, no separate Qt install needed)
- Platform-specific native `jwlCore` binary must exist alongside script (`libs/` on Linux/macOS, project root on Windows per `jwlcore.py:_load_lib`)

**Production:**
- Distributed as self-contained platform binaries (Linux binary, Windows .exe, macOS .app) via GitHub Releases — no Python install needed by end user
- Windows: unsigned executable triggers SmartScreen warning (see `.github/SECURITY.md`)
- macOS: requires `xattr -cr JWLManager.app` to bypass Gatekeeper quarantine (unsigned/unnotarized)
- Linux: requires `chmod +x`

---

*Stack analysis: 2026-07-16*
