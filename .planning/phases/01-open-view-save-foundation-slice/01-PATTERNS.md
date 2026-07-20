# Phase 1: Open, View, Save (Foundation Slice) - Pattern Map

**Mapped:** 2026-07-19
**Files analyzed:** 8 (new Rust/TS files to be created)
**Analogs found:** 7 / 8 (Python reference implementation; NO existing Rust/Tauri/TS code in repo — this is a from-scratch `app/` subdir)

**Framing:** Analogs here are Python source the Rust/TS code must reproduce the *behavior* of, not the syntax. Two analogs are explicitly flagged **anti-pattern-to-fix** — do not port their bug.

## File Classification

| New File | Role | Data Flow | Closest Analog (Python) | Match Quality | Disposition |
|----------|------|-----------|--------------------------|----------------|-------------|
| `app/src-tauri/src/archive/extract.rs` | service (archive I/O) | file-I/O | `JWLManager.py:977-978, 1097-1099` (`ZipFile.extractall`) | role-match | **anti-pattern-to-fix** (unvalidated extractall = zip-slip) |
| `app/src-tauri/src/archive/manifest.rs` | model + service | transform (serialize/deserialize) | `JWLManager.py:979-991` (`new_file`), `1154-1170` (`update_manifest`), `994-1008` (`check_validity`) | exact | pattern-to-reproduce (byte-compatible fields/order) |
| `app/src-tauri/src/db/notes.rs` | service (query) | CRUD (read) | `JWLManager.py:694-767` (`get_notes`/`load_independent`), `578-627` (`process_code`/`process_detail`) | exact | pattern-to-reproduce |
| `app/src-tauri/src/db/resources.rs` | service (lookup) | CRUD (read, bundled DB) | `JWLManager.py:4023-4053` (`read_resources`/`load_languages`/`load_bible_books`) | exact | pattern-to-reproduce |
| `app/src-tauri/src/jwlcore/loader.rs` | service (FFI bridge) | event-driven (load once, resolve symbols) | `jwlcore.py` (whole file, esp. `_platform_lib_name` lines 29-38, `_load_lib` lines 40-55) | exact | pattern-to-reproduce logic; **anti-pattern-to-fix** the OS-only arch-blind selection (line 30 `sys.platform`) |
| `app/src-tauri/src/archive/save.rs` | service (archive I/O) | file-I/O | `JWLManager.py:1121-1150` (`save_file`/`save_as_file`), `1152-1170` (`zip_file`/`update_manifest`) | exact | pattern-to-reproduce (hash-last ordering) |
| `app/src-tauri/tests/fixture_gen.rs` (or `xtask`) | test/utility | file-I/O (generator) | `res/blank`, `res/blank_playlist` (binary zip seeds — `default_thumbnail.png` + `userData.db`, no `manifest.json` inside) | role-match | pattern-to-reproduce as *reference*, but D-06 forbids shipping these binaries — regenerate schema/seed data programmatically |
| `app/src/components/NotesList.tsx` | component | streaming (windowed render) | none in repo | no analog | greenfield — build per TanStack Virtual docs |
| `app/src-tauri/src/category.rs` | model (enum) | transform | `JWLManager.py:560-570` (`if category == _('Notes')` chain) | role-match | **anti-pattern-to-fix** (translated-string keying; replace with Rust enum + `ts-rs`) |

## Pattern Assignments

### `app/src-tauri/src/archive/extract.rs` (service, file-I/O)

**Analog:** `JWLManager.py:965-979` (`new_file`) and `:1092-1106` (`load_file`) — **anti-pattern, do not reproduce as-is**

```python
# JWLManager.py:977-978 — new_file()
with ZipFile(PROJECT_PATH / 'res/blank', 'r') as zipped:
    zipped.extractall(TMP_PATH)

# JWLManager.py:1097-1099 — load_file()
try:
    with ZipFile(archive, 'r') as zipped:
        zipped.extractall(TMP_PATH)
    self.upgrade_schema(f'{TMP_PATH}/{DB_NAME}')
    ...
except:
    return None
```

**What to fix:** `ZipFile.extractall()` performs no path-traversal validation (zip-slip, ARCH-05). The bare `except: return None` (line 1105-1106) also swallows the real failure reason — SAFE-05 requires a typed error instead. Reproduce the *behavior* (extract archive contents into a session temp dir, then read `userData.db`), not the unvalidated call or the swallowed exception.

**Rust replacement pattern (from RESEARCH.md, verified against docs.rs):**
```rust
use zip::ZipArchive; // pin >=2.3.0 — CVE-2025-29787
fn safe_extract(archive_path: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(dest)?; // validates each entry via enclosed_name, incl. symlink chains at >=2.3.0
    Ok(())
}
```

**Temp-dir lifecycle to mirror:** `JWLManager.py:972-976, 1093-1096` clear the temp dir with a glob+remove loop wrapped in bare `except: pass` before every extract — reproduce the "clear before extract" *intent* (fresh per-session state, D-03) via `tempfile::TempDir` (auto-cleanup on drop) rather than a manual glob-delete-and-swallow.

---

### `app/src-tauri/src/archive/manifest.rs` (model + service, transform)

**Analog:** `JWLManager.py:979-991` (new-archive manifest), `:1152-1170` (`update_manifest`), `:994-1008` (`check_validity`)

**New-archive manifest shape** (`JWLManager.py:979-989`):
```python
self.manifest = {
    'name': APP,
    'creationDate': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
    'version': 1,
    'type': 0,
    'userDataBackup': {
        'lastModifiedDate': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
        'deviceName': f'{APP}_{VERSION}',
        'databaseName': 'userData.db',
        'hash': '',
        'schemaVersion': 16 } }
```

**Write pattern — compact, exact field order** (`JWLManager.py:990-991`, `:1169-1170`):
```python
with open(f'{TMP_PATH}/manifest.json', 'w') as json_file:
    json.dump(m, json_file, indent=None, separators=(',', ':'))
```
Rust: `serde_json::to_string(&manifest)` on an ordered `struct` (never a `HashMap`/loose `Value` — struct field declaration order is what preserves the key order Python's dict-literal order produces).

**update_manifest — hash computed LAST, after DB mutation** (`JWLManager.py:1154-1170`):
```python
def update_manifest():
    t = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    m = self.manifest
    m['name'] = APP
    m['creationDate'] = t
    m['userDataBackup']['deviceName'] = f'{APP}_{VERSION}'
    m['userDataBackup']['lastModifiedDate'] = t
    m['userDataBackup']['databaseName'] = DB_NAME
    con = sqlite3.connect(f'{TMP_PATH}/{DB_NAME}')
    con.execute('UPDATE LastModified SET LastModified = ?;', (m['userDataBackup']['lastModifiedDate'],))
    schema_version = con.execute('PRAGMA user_version;').fetchone()[0]
    con.commit()
    con.close()
    m['userDataBackup']['schemaVersion'] = schema_version
    m['userDataBackup']['hash'] = sha256hash(f'{TMP_PATH}/{DB_NAME}')  # LAST — hashes final on-disk bytes
    with open(f'{TMP_PATH}/manifest.json', 'w') as json_file:
        json.dump(m, json_file, indent=None, separators=(',', ':'))
```
Critical ordering to reproduce: DB write (`LastModified` UPDATE) → close/flush handle → hash the file bytes → write manifest. Reordering breaks ARCH-03 (Common Pitfall 1 in RESEARCH.md).

**check_validity — acceptance gate** (`JWLManager.py:994-1008`):
```python
def check_validity(self, archive):
    file = Path(archive).name
    if is_zipfile(archive):
        with ZipFile(archive) as zipped:
            if 'manifest.json' in zipped.namelist():
                with zipped.open('manifest.json') as j:
                    manifest = json.load(j)
                    if manifest.get('userDataBackup'):
                        schema = manifest['userDataBackup'].get('schemaVersion', 0)
                        if schema > 11:
                            return True
                        # reject: too old
                        return False
    # reject: not a valid archive
    return False
```
Reproduce the gate logic (`is_zipfile` → has `manifest.json` → has `userDataBackup` → `schemaVersion > 11`) as a Rust function returning `Result<(), ArchiveError>` with typed variants for each rejection reason (SAFE-05) instead of the Python's boolean-return + `QMessageBox` side effect. Use strict `serde` struct deserialization (parse failure = reject) rather than `.get(...).unwrap_or(0)` loose reads (RESEARCH.md Security Domain: type-confusion tampering pattern).

---

### `app/src-tauri/src/db/notes.rs` (service, CRUD read)

**Analog:** `JWLManager.py:694-767` (`get_notes`), `578-627` (`process_code`/`process_detail`)

**Two-query union — do not drop the independent-notes query** (`JWLManager.py:696-704, 762-766`):
```python
def load_independent():
    lst = []
    for row in con.execute("SELECT NoteId Id, ColorIndex Color, GROUP_CONCAT(Name, ' | ') Tags, substr(LastModified, 0, 11) Modified FROM (SELECT * FROM Note n LEFT JOIN TagMap tm USING (NoteId) LEFT JOIN Tag t USING (TagId) LEFT JOIN UserMark u USING (UserMarkId) ORDER BY t.Name) n WHERE n.BlockType = 0 AND LocationId IS NULL GROUP BY n.NoteId;").fetchall():
        ...
    return pl.DataFrame(lst, schema=schema, orient='row')
...
notes = merge_df(notes)
if not self.dupes:
    i_notes = load_independent()
    self.current_data = pl.concat([i_notes, notes])
```

**Main Notes query** (`JWLManager.py:751-757`):
```python
sql = f"{dupes} SELECT NoteId Id, MepsLanguage Language, KeySymbol Symbol, IssueTagNumber Issue, BookNumber Book, ChapterNumber Chapter, ColorIndex Color, GROUP_CONCAT(Name, ' | ') Tags, substr(LastModified, 0, 11) Modified FROM (SELECT * FROM Note n JOIN Location l USING (LocationId) LEFT JOIN TagMap tm USING (NoteId) LEFT JOIN Tag t USING (TagId) LEFT JOIN UserMark u USING (UserMarkId) ORDER BY t.Name) n {where} GROUP BY n.NoteId;"
for row in con.execute(sql).fetchall():
    lng = lang_name.get(row[1], f'#{row[1]}')
    code, year = process_code(row[2], row[3])
    detail1, year, detail2 = process_detail(row[2], row[4], row[5], row[3], year)
    col = process_color(row[6] or 0)
    note = [row[0], lng, code or _('* OTHER *'), col, row[7] or _('* NO TAG *'), row[8] or '', year, detail1, detail2]
```
Note: Phase 1 is read-only, so the `self.dupes` branch (lines 707-750, duplicate-detection CTE) is out of scope — only the base query + independent-notes union are needed for DATA-01 in this phase.

**Label synthesis to port exactly** (`JWLManager.py:578-627`):
```python
def process_code(code, issue):
    if code == 'ws' and issue == 0:
        code = 'ws-'
    elif not code:
        code = ''
    elif regex.match(code_jwb, code):
        code = 'jwb-'
    yr = ''
    dated = regex.search(code_yr, code)
    if dated:
        prefix = dated.group(1); suffix = dated.group(2)
        if prefix not in {'bi', 'br', 'brg', 'kn', 'ks', 'pt', 'tp'}:
            code = prefix
            yr = ('19' if int(suffix) >= 50 else '20') + suffix
    return code, yr

def process_detail(symbol, book, chapter, issue: int, year):
    if symbol in {'Rbi8','bi10','bi12','bi22','bi7','by','int','nwt','nwtsty','rh','sbi1','sbi2'}:
        detail1 = _('* OTHER *')
    else:
        detail1 = None
    if isinstance(issue, int) and issue > 19000000:
        iss = str(issue); y, m, d = iss[0:4], iss[4:6], iss[6:]
        detail1 = f'{y}-{m}' if d == '00' else f'{y}-{m}-{d}'
        if not year: year = y
    if book and chapter:
        bk = str(book).rjust(2, '0') + f': {bible_books[book]}'
        detail1 = bk
        detail2 = _('Chap.') + str(chapter).rjust(4, ' ')
    else:
        detail2 = None
    if not detail1 and year: detail1 = year
    if not year: year = None
    return detail1, year, detail2
```
`code_jwb`/`code_yr` regex constants are defined elsewhere in the module — grep for them (`code_jwb =`, `code_yr =`) before porting; not captured in this excerpt. Port these two functions as pure Rust functions with unit tests against known `(symbol, book, chapter, issue, year)` tuples — RESEARCH.md Open Question 4 flags this as needing a direct read/verification pass, which this pattern map has now done.

---

### `app/src-tauri/src/db/resources.rs` (service, CRUD read, bundled DB)

**Analog:** `JWLManager.py:4023-4053` (`read_resources`)

```python
def read_resources(lng):
    def load_bible_books(lng):
        for row in con.execute('SELECT Number, Name FROM BibleBooks WHERE Language = ?;', (lng,)).fetchall():
            bible_books[row[0]] = row[1]

    def load_languages():
        for row in con.execute('SELECT Language, Name, Code, Symbol FROM Languages;').fetchall():
            lang_name[row[0]] = row[1]
            lang_symbol[row[0]] = row[3]
            if row[2] == lng:
                ui_lang = row[0]
        return ui_lang

    con = sqlite3.connect(PROJECT_PATH / 'res/resources.db')
    ui_lang = load_languages()
    load_bible_books(ui_lang)
    pubs = pl.read_database(f"SELECT Symbol, ShortTitle Short, Title 'Full', Year, [Group] Type FROM Publications p JOIN Types USING (Type, Language) WHERE Language = {ui_lang};", con)
    extras = pl.read_database(f"SELECT Symbol, ShortTitle Short, Title 'Full', Year, [Group] Type FROM Extras p JOIN Types USING (Type, Language) WHERE Language = {ui_lang};", con)
    publications = pl.concat([pubs, extras])
    con.close()
```
**Anti-pattern flag:** the `pl.read_database(f"...WHERE Language = {ui_lang};", ...)` calls are f-string-interpolated SQL (CONCERNS.md finding). `ui_lang` here is always an internal integer from `load_languages`, not user input, so the injection risk is low in practice — but the Rust port must still parameterize (`rusqlite` bound params), not interpolate, per CLAUDE.md's "No f-string/format-string SQL interpolation" constraint. Bundle `res/resources.db` (335 KB) as a Tauri resource (RESEARCH.md Pitfall 3) — load once at startup/first-use, cache `lang_name`/`lang_symbol`/`bible_books`/`publications` maps in memory, mirroring the module-global cache pattern here (`global _, bible_books, ...`) but via a Rust struct/`OnceCell` instead of module globals.

---

### `app/src-tauri/src/jwlcore/loader.rs` (service, FFI bridge)

**Analog:** `jwlcore.py` (whole file) — pattern-to-reproduce for the bridge shape, **anti-pattern-to-fix** for arch selection

```python
def _platform_lib_name(base="jwlCore"):
    sysname = sys.platform
    if sysname.startswith("linux"):
        return f"lib{base}-x86_64.so"       # BUG: always x86_64, never checks arch —
    elif sysname == "darwin":                # arm64 Linux boxes get the wrong .so name
        return f"lib{base}.dylib"
    elif sysname == "win32":
        return f"{base}-amd64.dll"
    else:
        raise OSError(f"Unsupported platform: {sysname}")

def _load_lib():
    name = _platform_lib_name()
    try:
        base_path = sys._MEIPASS
    except Exception:
        base_path = os.path.dirname(__file__)
    if sys.platform == "win32":
        lib_path = os.path.join(base_path, name)
    else:
        lib_path = os.path.join(base_path, "libs", name)
    if os.path.exists(lib_path):
        kwargs = {}
        if hasattr(os, "RTLD_LOCAL") and sys.platform != "win32":
            kwargs["mode"] = os.RTLD_LOCAL
        return ctypes.CDLL(lib_path, **kwargs)
    raise OSError(f"Could not find shared library {name} at {lib_path}")

lib = _load_lib()   # ANTI-PATTERN: runs at module import time — any failure crashes
                     # the whole app before the UI shows (RESEARCH.md Pitfall 4)

lib.mergeDatabase.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_bool]
lib.mergeDatabase.restype  = ctypes.c_int
lib.getLastResult.argtypes = []
lib.getLastResult.restype  = ctypes.c_char_p
lib.getCoreVersion.argtypes = []
lib.getCoreVersion.restype  = ctypes.c_char_p

def merge_databases(path1: str, path2: str, downgrade: bool = False) -> int:
    return lib.mergeDatabase(path1.encode("utf-8"), path2.encode("utf-8"), downgrade)
def get_last_result() -> str | None:
    p = lib.getLastResult()
    return p.decode("utf-8") if p else None
def get_core_version() -> str | None:
    p = lib.getCoreVersion()
    return p.decode("utf-8") if p else None
```
**What to fix (D-13):** select by `(OS, ARCH)` tuple, not OS alone:
```rust
fn resolve_lib_name() -> Result<&'static str, JwlCoreError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64")  => Ok("jwlCore-amd64.dll"),
        ("windows", "aarch64") => Err(JwlCoreError::NoArm64WindowsBinary), // D-13a: known gap, typed error
        ("linux", "x86_64")    => Ok("libs/libjwlCore-x86_64.so"),
        ("linux", "aarch64")   => Ok("libs/libjwlCore-arm64.so"),
        ("macos", _)           => Ok("libs/libjwlCore.dylib"),
        (os, arch)              => Err(JwlCoreError::UnsupportedPlatform(os.into(), arch.into())),
    }
}
```
**What to fix (Pitfall 4):** do NOT load eagerly at Rust module-init/Tauri `setup()` mirroring `lib = _load_lib()` at Python import time. Wrap in a Tauri command (`check_jwlcore()`) called lazily post-mount, returning `Result<JwlCoreStatus, JwlCoreError>` — D-12 scope is load + symbol resolution only, no merge call, callable and retriable from the frontend on failure.

Function/symbol shape to mirror 1:1 (`setProgressCallback`, `mergeDatabase`, `getLastResult`, `getCoreVersion`) — Phase 1 resolves all four symbols but only calls `getCoreVersion` (per D-12, "resolve symbols" not "call merge").

---

### `app/src-tauri/src/archive/save.rs` (service, file-I/O)

**Analog:** `JWLManager.py:1121-1150` (`save_file`/`save_as_file`)

```python
def save_file(self):
    if not self.save_filename:
        return self.save_as_file()
    else:
        self.zip_file()

def save_as_file(self):
    ...
    if Path(fname) == self.current_archive:
        # confirm overwrite of original
    self.save_filename = fname
    self.working_dir = Path(fname).parent
    self.current_archive = self.save_filename   # D-05: session follows the NEW path
    self.zip_file()
```
Reproduce: save with no path → save-as flow; save-as sets working identity to the new path, original untouched until the atomic rename replaces only the target path (never the source archive, since it was opened read-only into a temp dir per D-03). The Rust `atomic_save` idiom from RESEARCH.md (write sibling temp file, `fs::rename` over target) implements D-04 — no direct Python analog exists for the atomic-rename step since the Python app writes the zip in place via `zipfile.ZipFile(..., 'w')`; this is a genuine improvement over the Python behavior, not a port.

---

### `app/src-tauri/tests/fixture_gen.rs` (test utility, file-I/O generator)

**Analog:** `res/blank`, `res/blank_playlist` (binary zip seeds — inspected via `unzip -l`, not committed as Phase 1 fixtures per D-06)

```
res/blank contents:
  default_thumbnail.png (542 bytes)
  userData.db (208896 bytes)
  # NOTE: no manifest.json inside the zip itself — new_file() (JWLManager.py:979-991)
  # builds and writes manifest.json separately AFTER extracting res/blank to TMP_PATH.
```
**Disposition:** D-06 forbids committing real/scrubbed archives (GDPR Art. 9 special-category data risk); `res/blank`'s *shape* (an empty `userData.db` + `manifest.json` written on top, matching the schema described in Pattern 1 above) is the reference to replicate programmatically, not a file to copy. The fixture generator should build a `userData.db` from the v16 schema (extend `upgrade_schema`'s `Location_new` DDL, `JWLManager.py:1023-1070`, as the schema reference) and synthesize a matching `manifest.json` via the Rust `Manifest` struct from `manifest.rs`, plus a crafted zip with a `../`-escaping entry for the ARCH-05 zip-slip fixture (D-08).

---

### `app/src/components/NotesList.tsx` (component, streaming/windowed render)

**No analog in this repo.** Greenfield — build against `@tanstack/react-virtual` v3 per RESEARCH.md Pattern/Code Examples section. Consumes `NotesRow[]` returned over Tauri IPC from `db/notes.rs`; must render only visible rows (D-10 hard constraint, WebKitGTK perf cliff on Linux).

---

### `app/src-tauri/src/category.rs` (model, enum)

**Analog:** `JWLManager.py:560-570` — **anti-pattern-to-fix**

```python
elif category == _('Notes'):
    get_notes()
elif category == _('Annotations'):
    get_annotations()
```
**What to fix (DATA-08):** control flow keyed on the *translated* display string (`_('Notes')`) is a latent i18n bug — under a non-English UI locale, `category` (sourced from a combo box, presumably already localized) coincidentally matches `_('Notes')` only because both sides run through the same `gettext` catalog at once; any drift (stale catalog, mixed-locale state) breaks dispatch silently. Replace with a Rust `enum Category { Notes, Bookmarks, Favorites, Highlights, Annotations, Playlists }` deriving `ts-rs::TS`, generating the frontend's TS union — translated strings become display-only labels, never participate in matching/control flow (per D-11).

---

## Shared Patterns

### Bare-except swallowing → typed errors (SAFE-05)
**Source (anti-pattern):** pervasive `except:` / `except Exception:` across `JWLManager.py` (e.g. `:972-976`, `:1092-1096`, `:1105-1106`, `:1022 upgrade_schema try/except: pass`).
**Apply to:** every new Rust file above. Replace every "catch and continue/silently fail" spot with a `thiserror` variant that reaches the frontend with an actionable message (D-14). No `unwrap()`/`expect()` on any archive-data path (D-15, clippy-enforced).

### Temp-dir working copy, source read-only (D-03)
**Source:** `JWLManager.py` `TMP_PATH` usage throughout `new_file`/`load_file`/`zip_file` — archive is always extracted to a session temp dir; the original file on disk is only ever read (open) or replaced (save), never mutated by SQL directly.
**Apply to:** `archive/extract.rs`, `archive/save.rs`, `db/notes.rs`, `db/resources.rs`.

### Manifest hash-last ordering (ARCH-03, Pitfall 1)
**Source:** `JWLManager.py:1162-1168` (`update_manifest`).
**Apply to:** `archive/save.rs`, `archive/manifest.rs` — hashing must be the literal last DB-touching step before zipping.

### Zip-slip validation via crate, not hand-rolled path checks (ARCH-05)
**Source:** `zip` crate ≥2.3.0 `extract()`/`enclosed_name` (RESEARCH.md "Don't Hand-Roll"), replacing `JWLManager.py`'s unvalidated `ZipFile.extractall()`.
**Apply to:** `archive/extract.rs`, and the crafted zip-slip fixture consumer in `fixture_gen.rs`/`archive_tests.rs`.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `app/src/components/NotesList.tsx` | component | streaming | No existing frontend in this repo (PySide6 is a different UI paradigm entirely — Qt tree/list widgets are not a useful analog for React/TanStack Virtual DOM windowing); build per RESEARCH.md's TanStack Virtual guidance. |
| `app/src-tauri/src/category.rs` (the enum shape itself, not the bug it fixes) | model | transform | The enum *identity* is new; only the anti-pattern it replaces (`if category == _('Notes')`) has a Python analog. |

## Metadata

**Analog search scope:** `JWLManager.py` (4077 lines, targeted reads at cited line ranges), `jwlcore.py` (83 lines, full read), `res/blank` + `res/blank_playlist` (binary, inspected via `unzip -l`).
**Files scanned:** 2 Python source files (full/targeted), 2 binary fixture archives (listing only).
**Pattern extraction date:** 2026-07-19
</content>
