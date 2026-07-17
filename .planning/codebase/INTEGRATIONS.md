# External Integrations

**Analysis Date:** 2026-07-16

## APIs & External Services

**Update Check:**
- GitHub Releases API - checks for newer app version
  - Endpoint: `https://api.github.com/repos/erykjj/jwlmanager/releases/latest`
  - Client: `requests.get(url, headers=headers, timeout=5)` — `JWLManager.py:293-296` and again at `JWLManager.py:378-380`
  - Auth: none (public API)

**Crash/Error Telemetry:**
- ntfy.sh push notification - fire-and-forget POST when an unhandled exception occurs (crash reporting channel)
  - Endpoint: `https://ntfy.sh/reganamlwj`
  - Client: `requests.post(...)` — `JWLManager.py:332-333`
  - Auth: none (public topic, no API key)

**JW.org deep links (not API calls):**
- Generated hyperlinks to `https://www.jw.org/finder?...` for viewing Bible/publication references in a browser — `JWLManager.py:1615`, `1624`, `3021`, `3029`. Not fetched programmatically; opened by the OS default browser.

## Data Storage

**Databases:**
- SQLite (embedded, no server) — two distinct usages:
  - User's `.jwlibrary` archive's internal `userData.db` (extracted to a temp dir `TMP_PATH` at runtime, e.g. `JWLManager.py:933`, `1017`, `1162` and ~15 more call sites)
  - Bundled read-only reference DB `res/resources.db`, opened via `sqlite3.connect(PROJECT_PATH / 'res/resources.db')` — `JWLManager.py:4044`
  - Client: stdlib `sqlite3`, no ORM

**File Storage:**
- Local filesystem only. `.jwlibrary` files are ZIP archives (`zipfile.ZipFile`) containing the SQLite DB + media; extracted/repacked in a temp working directory created via `tempfile.mkdtemp(prefix='JWLManager_')`
- Native `jwlCore` library also does direct file-level merge operations on archive paths (`jwlcore.py: merge_databases`)

**Caching:**
- None (temp dir cleaned up on exit via `clean_up`)

## Authentication & Identity

**Auth Provider:**
- None. Desktop single-user application, no login/accounts/sessions.

## Monitoring & Observability

**Error Tracking:**
- Ad-hoc: unhandled exceptions caught, displayed in an in-app error dialog, and reported via anonymous `ntfy.sh` POST (see above). No structured logging framework, no Sentry/Bugsnag.

**Logs:**
- No file-based logging detected; feedback is via GUI dialogs (`QMessageBox`) and the ntfy crash notification only.

## CI/CD & Deployment

**Hosting:**
- None (desktop app; distributed via GitHub Releases, mirrored on GitLab per `README.md`)

**CI Pipeline:**
- GitHub Actions, build-only (no test stage):
  - `.github/workflows/build_linux.yml`
  - `.github/workflows/build_macOS.yml`
  - `.github/workflows/build_windows.yml`
  - `.github/workflows/release.yml`
  - CodeQL security scanning referenced in `README.md` badges (`github-code-scanning/codeql` workflow)

## Environment Configuration

**Required env vars:**
- None — no `.env` file, no environment-variable-driven config detected

**Secrets location:**
- None found in repo (no API keys required by any integration — GitHub API and ntfy.sh calls are unauthenticated)

## Webhooks & Callbacks

**Incoming:**
- None

**Outgoing:**
- Crash telemetry POST to `https://ntfy.sh/reganamlwj` (see Monitoring section above) — this is the only outbound webhook-style call

---

*Integration audit: 2026-07-16*
