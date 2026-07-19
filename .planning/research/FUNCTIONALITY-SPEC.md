# JWLManager — Functional Specification (behavioral extraction for rewrite)

Extracted from `JWLManager.py` (v12.5.0, 4078 lines), `jwlcore.py`, `libs/`, `res/`, `README.md`.
Purpose: allow a from-scratch Tauri reimplementation **without reading the Python source again**.
Every claim cites `file.py:line`.

Legend: "the DB" = `userData.db` extracted from the opened `.jwlibrary` archive into `TMP_PATH`
(`JWLManager.py:64-65`).

---

## 1. Every user-facing capability

App identity: `APP = 'JWLManager'`, `VERSION = 'v12.5.0'`, `BETA` flag gates a "pre-release" warning
dialog on startup (`JWLManager.py:28-30, 175-176`).

Global state: `self.modified` (dirty flag), `self.loaded`, `self.current_archive`,
`self.save_filename`, `self.older_schema`, `self.dupes`, `self.tree_cache` (`JWLManager.py:112-135`).
Any mutation calls `archive_modified()` → sets dirty, enables Save, italicizes status label
(`JWLManager.py:1271-1276`). `archive_saved()` resets (`JWLManager.py:1278-1283`).

### 1.1 Signal → handler map (authoritative list of triggers)
All in `connect_signals()` (`JWLManager.py:80-110`):

| Trigger | Handler | Line |
|---|---|---|
| `actionQuit` | `close` + `clean_up` | 81, 87 |
| `actionHelp` | `help_box` | 82 |
| `actionAbout` | `about_box` | 83 |
| `actionNew` | `new_file` | 84 |
| `actionOpen` | `load_file` | 85 |
| `actionMerge` | `merge_file` | 86 |
| `actionSave` | `save_file` | 88 |
| `actionSave_As` | `save_as_file` | 89 |
| `actionClean` | `clean_items` | 90 |
| `actionObscure` | `obscure_items` | 91 |
| `actionSort` | `sort_notes` | 92 |
| `actionExpand_All` / `actionCollapse_All` | `expand_all` / `collapse_all` | 93-94 |
| `actionSelect_All` / `actionUnselect_All` | `select_all` / `unselect_all` | 95-96 |
| `actionTheme` | `toggle_theme` | 97 |
| `menuTitle_View` | `change_title` | 98 |
| `menuLanguage` | `change_language` | 99 |
| `combo_grouping` changed | `regroup(False)` | 100 |
| `combo_category` changed | `switchboard` | 101 |
| tree itemChanged | `tree_selection` | 102 |
| tree doubleClicked | `double_clicked` | 103 |
| `button_export` | `export_menu` | 104 |
| `button_import` | `import_items` | 105 |
| `button_add` | `add_items` | 106 |
| `button_delete` | `delete_items` | 107 |
| `button_view` | `data_viewer` | 108 |
| `button_color` | `select_color` | 109 |
| `button_tag` | `tag_notes` | 110 |

Note: `actionReindex` exists but is **hidden** (`self.actionReindex.setVisible(False)`,
`JWLManager.py:140`) — reindexing is now implicit inside `trim_db` (§4.6). A rewrite should not
expose a Reindex command.

### 1.2 Categories
Six categories drive everything: **Annotations, Bookmarks, Favorites, Highlights, Notes,
Playlists** (`switchboard`, `JWLManager.py:531-547`).

`switchboard` decides, per category, which buttons are visible and which grouping-combo entries
(indexes 0-6) are disabled; `disable_options(lst, add, exp, imp, view, col, tag)`
(`JWLManager.py:510-524`):

| Category | disabled grouping idx | add | export | import | view | color | tag | line |
|---|---|---|---|---|---|---|---|---|
| Notes | — | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ | 534 |
| Highlights | 4,6 | ✗ | ✓ | ✓ | ✗ | ✓ | ✗ | 538 |
| Bookmarks | 4,5,6 | ✗ | ✓ | ✓ | ✗ | ✗ | ✗ | 540 |
| Annotations | 2,4,5,6 | ✗ | ✓ | ✓ | ✓ | ✗ | ✗ | 542 |
| Favorites | 4,5,6 | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | 544 |
| Playlists | 1,2,3,4,5,6 (forced grouping = Title) | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | 546-547 |

Button enablement additionally requires a selection (`tree_selection`, `JWLManager.py:497-505`):
`view` only for Notes/Annotations (501), `color` only for Notes/Highlights (502), `tag` only for
Notes (503), `delete`/`export` for any selection (500, 504).

### 1.3 New archive
`new_file` (`JWLManager.py:965-992`). Preconditions: if dirty → `check_save` prompt (966-967).
Wipes `TMP_PATH` (972-976), extracts the bundled template `res/blank` (977-978), synthesizes a fresh
manifest (979-989, see §2.2), writes it, then `file_loaded(False)` — `loaded=False` so
Clean/Mask/Sort stay disabled until data exists (`enable_options`, 785-788).

### 1.4 Open
`load_file(archive='')` (`JWLManager.py:1077-1106`). Dirty → `check_save`. File dialog filter
`*.jwlibrary` (1081). Validity gate `check_validity` (1086; §2.3). Wipes temp, extracts zip,
**runs `upgrade_schema` immediately on load** (1100), loads `manifest.json` into `self.manifest`
(1101-1102). Whole extract path is wrapped in bare `try/except: return None` (1097-1106) — failures
are silent.

Alternate open paths:
- **CLI arg**: `python3 JWLManager.py [archive] [-xx]`; `sys.argv[-1]` is opened iff `is_zipfile()`
  (`JWLManager.py:4072-4075`, arg parsed at 4002/4012-4013).
- **Drag-drop** of `.jwlibrary` → `check_file` (`JWLManager.py:231-235`).
- **Single-instance lockfile**: a `JWLManager.lock` next to the executable. A second launch writes
  the requested path into the lockfile and exits (`get_language`, 4014-4020, `write_lockfile`
  3978-3980); the running instance polls it every 1000 ms via `QTimer` (169-171) and
  `check_lockfile` (198-205) reads+truncates it and opens the path if suffix is `.jwlibrary`.

`check_file` (`JWLManager.py:179-196`): if nothing loaded and not dirty → open directly; else prompt
"Open archive or merge with current?" with Open/Merge buttons (186-195).

### 1.5 Save / Save As
- `save_file` (1121-1125): no `save_filename` → delegates to `save_as_file`.
- `save_as_file` (1127-1150): default name `MODIFIED_<YYYY-MM-DD>.jwlibrary` in `working_dir`
  (1128). Dialog is forced non-native (`DontUseNativeDialog`, 1130) so a **"Schema v14" checkbox**
  can be injected into its layout (1131-1134) — this checkbox sets `self.older_schema` (1141) and is
  the only way to trigger the downgrade path. Suffix forced to `.jwlibrary` (1139-1140).
  Overwriting the source archive requires an extra critical confirm (1142-1145).
- `zip_file` (1152-1268): the save pipeline, in strict order:
  1. `self.trim_db()` — always (1245; §4.6).
  2. If `older_schema`: copy DB to `userData_backup.db`, run `downgrade_schema()`
     (1246-1250). Else `upgrade_schema()` (1251-1252).
  3. `update_manifest()` (1253; §2.2).
  4. Rewrite the zip from `TMP_PATH` contents with `ZIP_DEFLATED`, **skipping
     `userData_backup.db`** (1255-1260).
  5. `finally`: if downgraded, restore the v16 DB over the downgraded one via
     `os.replace(db_backup, db_path)` (1266-1267) — **so the in-memory working copy stays v16 even
     after saving v14**. Critical invariant for a rewrite.
  6. `archive_saved()` (1268).

### 1.6 Merge
`merge_file` (1010-1014) → file dialog → `merge_items(path)`.
`merge_items` (2645-2694): validates (2661), installs a C progress callback
(`lib.setProgressCallback`, 2668-2669), extracts the incoming archive to `{TMP_PATH}/merge`
(2670-2671), calls the **native** `merge_databases(TMP_PATH, TMP_PATH/merge, False)` (2672),
removes the merge dir (2675). Non-zero return ⇒ `count = 0` ⇒ "NOT merged!" + `get_last_result()`
(2673-2674, 2686-2688). Progress dialog is frameless, modal, 0..15 steps (2648-2654); the callback
increments both a Python `count` (sum of ints from C) and the dialog value (2656-2659).
On success → `regroup(True, message)` + `archive_modified()` (2690-2694).
**The entire merge algorithm lives in the compiled `jwlCore` lib — there is no Python
implementation to port.** See §7.

### 1.7 Delete
`delete_items` (3622-3695). Confirm dialog (3673). Opens DB with
`PRAGMA temp_store=2; journal_mode='OFF'; foreign_keys='OFF'; BEGIN;` (3681), builds an inline
`IN (...)` list from `list_selected()` (3682), dispatches per category (3658-3671):
- Bookmarks → `DELETE FROM Bookmark WHERE BookmarkId IN (...)` (3660)
- Favorites → `DELETE FROM TagMap WHERE TagMapId IN (...)` (3662)
- Highlights → `DELETE FROM BlockRange WHERE BlockRangeId IN (...)` (3664) — **not** UserMark;
  orphaned UserMarks are swept later by `trim_db`.
- Notes → `DELETE FROM Note WHERE NoteId IN (...)` (3666)
- Annotations → `DELETE FROM InputField WHERE LocationId IN (...)` (3669) — note the key is
  **LocationId**, so deleting an "annotation" deletes *all* InputField rows at that location.
- Playlists → `delete_playlist_items()` (3627-3656; §4.5)
Re-enables `foreign_keys` before commit (3684). On `result > 0` → `regroup(True, msg)` +
`archive_modified()` (3691-3695).

### 1.8 Color change
`select_color` (3217-3235) shows a 7-entry menu with fixed swatches:
`0 Grey #808080, 1 Yellow #FAD929, 2 Green #81BD4F, 3 Blue #5EB4EF, 4 Red #DB5D8D,
5 Orange #FF862E, 6 Purple #7B57A7` (3219-3227).
`set_color(color)` (3237-…): **Highlights + color 0 (Grey) is a no-op** — early return (3255-3256).
For Notes, any selected note with a LocationId but no UserMark gets a **new UserMark synthesized**
(`StyleIndex 0`, fresh `uuid.uuid1()` GUID, `Version 1`) and linked back into `Note.UserMarkId`
(3243-3246). Then `UPDATE UserMark SET ColorIndex` over the resolved UserMarkId set (3251).
For Highlights the UserMarkIds come from the selected BlockRanges (3241).

### 1.9 Tagging notes
`tag_notes` (3281-…). Reads all `Tag WHERE Type = 1` with a per-tag count of how many *selected*
notes carry it (3287-3298) → tri-state UI. `delete_tags` removes TagMap rows for tags whose count is
0 (3317-3331); `add_tags` adds for count != 0 (3333+). Free IDs are recycled via `get_available_ids`
over `{TagMap, Tag}` (3303-3315) — same gap-filling algorithm as import (§4.4).

### 1.10 Add items
`add_items` (3389+) — only enabled for Favorites and Playlists (§1.2).
- `add_favorite` (3391+): dialog with language + publication combos populated from the bundled
  `favorites` table filtered by language (3395-3399); inserts a Location and a TagMap position.
- `add_images` (3462+): file picker; `update_db(playlist, files)` (3528+) de-duplicates:
  `check_name` appends `_1`, `_2`… to colliding IndependentMedia file names (3530-3536);
  `check_label` appends ` (1)`, ` (2)`… to colliding PlaylistItem labels (3538-3544);
  `add_tag` computes the next position as `ifnull(max(Position), -1) + 1` (3547-3548).
  Playlist tag: try `INSERT INTO Tag (Type, Name) VALUES (2, ?)`; on failure reuse existing
  `TagId WHERE Name = ? AND Type = 2` (3550-3556). Media identity is by **file path and hash**
  (`current_files` = FilePath col, `current_hashes` = hash col, 3558-3560).

### 1.11 Clean text
`clean_items` (3698-3748). Confirm (3725). Strips Unicode separator junk:
`spaces = [\p{Zs}--\x20]` → `' '`, `joiners = [\p{Zl}\p{Zp}]` → `''`, `\r` → `\n`
(3700-3703, 3730-3732; `regex.V1` set-subtraction syntax). Applies to `InputField.Value` keyed by
**TextTag** (3705-3711) and `Note.Title`/`Note.Content` keyed by NoteId (3713-3723). Only rows
matching `combined` are touched, and the count is of *rows*, not replacements.

### 1.12 Mask / obscure (privacy)
`obscure_items` (3750-3823). Replaces every Unicode letter (`\p{L}`, 3806) with letters cycled from
a randomly chosen word out of `['obscured', 'yada', 'bla', 'gibberish', 'børk']` (3805), preserving
case (3759-3762), non-letters, and string length. Applied to InputField.Value, Bookmark
Title+Snippet, Note Title+Content, Location.Title (3810-3813). Always marks modified (3823).

### 1.13 Reorder notes
`sort_notes` (3825-3855). For every `Tag WHERE Type = 1` (3828), rewrites `TagMap.Position` ordered
by NoteId. **Two-pass with negative sentinel values** (`-pos` first, then `abs(pos)-1`,
3829-3834) — necessary because `TagMap` has a uniqueness constraint on (TagId, Position); a naive
single-pass rewrite collides. Non-obvious; must be preserved or replaced by a temp-table rewrite.

### 1.14 Data viewer / editor
`data_viewer` (2697-…), enabled for Notes and Annotations only. Provides: in-place title/body
editing (2699-2716), rich-text toolbar (2717-2723), per-item color change (2746-2760), navigation
(2724-2745), filtering (2815-2832), single-item delete with row/col restore (2788-2814), save to txt
(2888-2906), and a DB write-back (`update_db` → `update_notes` / `update_annotations`,
2833-2877). Escape closes (3160+).

### 1.15 Language switch
`change_language` (405-429) + `retranslate_viewer` (430-444). Runtime switch, no restart.

### 1.16 Theme
`toggle_theme` (397-404) via `ThemeManager` (`res/ui_extras.py`); persists `JWLManager/theme`;
swaps `res/dark.qss`/`res/light.qss` and `res/icons/{dark,light}/`.

### 1.17 Title view
`change_title` (445-457) with three modes `{code, short, full}` (118) persisted as
`JWLManager/title`.

### 1.18 About / update check
`about_box` (282-308) does a live `GET https://api.github.com/repos/erykjj/jwlmanager/releases/latest`
(timeout 5 s, 293-297) and compares to `VERSION` component-wise (284-290): local > remote ⇒
"Pre-release"; remote > local ⇒ red "update available!" link. Result cached in `self.latest`.

### 1.19 Crash reporting (telemetry — note for rewrite)
`crash_box` (310-…) formats the traceback and offers **"Send crash report"**, which POSTs the
traceback + up to 200 chars of user comment to `https://ntfy.sh/reganamlwj`
(324-343). Opt-in (user must click). Any handled exception in delete/clean/mask/sort/trim/zip calls
`crash_box(ex)` then `clean_up()` then `sys.exit()` — i.e. **the app hard-exits on those errors**
(e.g. 3687-3690, 3930-3933, 1261-1264).

### 1.20 Exit
`closeEvent` (212-223): if dirty → Yes/No/Cancel "Save before quitting?"; Save failure aborts the
close (216-218). `clean_up` (3936-3960): closes child windows, `shutil.rmtree(TMP_PATH)`, persists
all QSettings (language, category, title format, theme, column widths, sort col+direction, export
format, window/viewer/help/tag geometry), and **removes the lockfile** (3959-3960).

---

## 2. The `.jwlibrary` archive format contract  ← highest-value section

### 2.1 Zip layout
A `.jwlibrary` is a plain zip (`is_zipfile`, 996) containing at minimum:
- `manifest.json` — required (998)
- `userData.db` — SQLite; name is `DB_NAME = 'userData.db'` (65) and is *asserted* into the manifest
  on save rather than read from it (1161)
- zero or more media/thumbnail files at paths referenced by
  `IndependentMedia.FilePath` / `PlaylistItem.ThumbnailFilePath` (3628-3646 removes them from
  `TMP_PATH` by that relative path)

Save writes **every file in `TMP_PATH`** back into the zip, flat, with `ZIP_DEFLATED`, excluding only
`userData_backup.db` (1255-1260). Extraction is `zipped.extractall(TMP_PATH)` (1099).

A `.jwlplaylist` is the same shape (separate template `res/blank_playlist`), exported via
`export_playlist` (1725+), which builds a fresh DB and inserts `Tag (TagId=1, Type=2,
Name=<file stem>)` (1728) and copies `android_metadata.locale` from the source if that table exists
(1730-1733).

### 2.2 manifest.json structure
Created for a new archive (`new_file`, 979-989):

```json
{
  "name": "JWLManager",
  "creationDate": "<UTC %Y-%m-%dT%H:%M:%SZ>",
  "version": 1,
  "type": 0,
  "userDataBackup": {
    "lastModifiedDate": "<UTC %Y-%m-%dT%H:%M:%SZ>",
    "deviceName": "JWLManager_v12.5.0",
    "databaseName": "userData.db",
    "hash": "",
    "schemaVersion": 16
  }
}
```

On save, `update_manifest()` (1154-1170) mutates the **loaded** manifest (preserving unknown keys —
important: do not reconstruct from scratch on save):
- `name` = `APP` (1157)
- `creationDate` = now, UTC, `%Y-%m-%dT%H:%M:%SZ` (1155, 1158) — yes, *creationDate* is overwritten
  on every save
- `userDataBackup.deviceName` = `f'{APP}_{VERSION}'` (1159)
- `userDataBackup.lastModifiedDate` = same timestamp (1160), and the **same value is written into
  the DB**: `UPDATE LastModified SET LastModified = ?` (1163)
- `userDataBackup.databaseName` = `'userData.db'` (1161)
- `userDataBackup.schemaVersion` = **read back from `PRAGMA user_version`**, not hardcoded (1164,
  1167)
- `userDataBackup.hash` = `sha256(<whole userData.db bytes>).hexdigest()` (1168, `sha256hash`
  4055-4056) — computed **after** trim/vacuum and after the schema up/downgrade, i.e. it must be the
  final on-disk bytes.
- Serialized **compactly**: `json.dump(m, f, indent=None, separators=(',', ':'))` (1170, 991).

### 2.3 Validity gate
`check_validity` (994-1008): must be a zip, must contain `manifest.json`, that JSON must have
`userDataBackup`, and `userDataBackup.schemaVersion` (default 0) must be **> 11**. Schema ≤ 11 ⇒
"cannot handle this old archive format. You can convert it using JW Library." Anything else ⇒
"This is not a valid JW Library backup archive."

### 2.4 Schema versions handled
- **Accepted on open**: 12–16 (>11 gate, 1003).
- **Working version**: 16. Upgrade runs on every open (1100) and every non-downgrade save (1252).
- **Downgrade target**: 14 (`PRAGMA user_version = 14`, 1236), user-selected via the Save-As
  "Schema v14" checkbox.

### 2.5 `upgrade_schema(db_path)` — v<16 → v16 (1016-1075)
Idempotent guard: `PRAGMA user_version >= 16` ⇒ return (1018-1021).
Steps (single `executescript`, 1023-1070):
1. `ALTER TABLE Location ADD COLUMN Specialty TEXT;` and `ADD COLUMN Edition TEXT;` (1024-1025)
2. Create `Location_new` with the **v16 Location definition** (1026-1062) — columns
   `LocationId INTEGER NOT NULL PRIMARY KEY, BookNumber, ChapterNumber, DocumentId, Track,
   IssueTagNumber INTEGER NOT NULL DEFAULT 0, KeySymbol TEXT, MepsLanguage INTEGER,
   Type INTEGER NOT NULL, Title TEXT, Specialty TEXT, Edition TEXT`, plus
   `UNIQUE (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)` and the three Type CHECK
   constraints (see §3.2).
3. Copy rows with `Specialty`/`Edition` = NULL (1063-1064), drop old, rename (1065-1066).
4. Indexes (1067-1069):
   - `IX_Location_KeySymbol_MepsLanguage_BookNumber_ChapterNumber`
   - `IX_Location_MepsLanguage_DocumentId`
   - **`IX_Location_Media` UNIQUE** on
     `(KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type,
     COALESCE(Specialty,''), COALESCE(Edition,''))`
5. `PRAGMA user_version = 16;`

**Non-obvious**: the whole thing is wrapped in `try: … except: pass` (1022, 1072-1073) — an upgrade
failure is *silently swallowed* and the archive continues to be used at its old version. Since
`update_manifest` reads `user_version` back from the DB (1164), a failed upgrade simply produces a
manifest with the old version rather than a corrupt claim. A rewrite that raises here would change
behavior on already-v16-ish or unusual archives.

### 2.6 `downgrade_schema()` — v16 → v14 (1172-1243)
Runs only when `older_schema` is set. Two distinct phases:

**Phase A — collapse Location duplicates that v14's stricter UNIQUE cannot hold (1174-1192).**
v16 keys media locations by `(KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type,
Specialty, Edition)`; v14 has `UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track,
Type)` with **no Specialty/Edition**. So rows differing only by Specialty/Edition would collide.
Algorithm:
- Select `LocationId, KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type`
  `FROM Location WHERE BookNumber IS NULL AND ChapterNumber IS NULL` (1175) — **only non-scripture
  locations**.
- Group by the pipe-joined key `KeySymbol|IssueTagNumber|MepsLanguage|DocumentId|Track|Type`
  (1176-1179).
- For each group with >1 member: keep `ids[0]` (**first in row order — not min(), not newest**),
  and for each other id remap every referencing FK, then delete the row (1181-1192):
  `Bookmark.LocationId`, `Bookmark.PublicationLocationId`, `Note.LocationId`,
  `UserMark.LocationId`, `InputField.LocationId`, `TagMap.LocationId`,
  `PlaylistItemLocationMap.LocationId`. **This exact list of 7 remap targets is the complete
  referential closure of LocationId in this schema** — a rewrite that misses one orphans data.

**Phase B — rebuild Location without Specialty/Edition (1193-1236):** create `Location_new` with the
v14 definition (note the **extra** `UNIQUE (KeySymbol, IssueTagNumber, MepsLanguage, DocumentId,
Track, Type)` at 1206 that v16 replaces with the `IX_Location_Media` index), copy the 10 v14 columns
(1230-1233), drop, rename, `PRAGMA user_version = 14;`.
The v16 indexes are **not** recreated in the v14 table.
Any exception here ⇒ `crash_box` + `clean_up` + `sys.exit()` (1238-1241) — unlike the upgrade path,
this one is fatal.

Downgrade is destructive; hence the `userData_backup.db` copy before (1249) and the restore after
zipping (1266-1267).

### 2.7 SQLite pragmas used
- Bulk-edit sessions: `PRAGMA temp_store=2; journal_mode='OFF'; foreign_keys='OFF'; BEGIN;`
  (3681, 3735, 3809, 3844) — FK enforcement is explicitly disabled during deletes and re-enabled
  before commit (3684).
- `trim_db`: `temp_store='MEMORY'; synchronous='OFF'; journal_mode='MEMORY'; foreign_keys='OFF'`,
  then restored to `'ON'/'FULL'/'DELETE'/'DEFAULT'` and finally `VACUUM` (3862-3926).
- The vendored `libs/sqlite3_64.dll` exists because the Windows-bundled SQLite must support the
  window functions (`ROW_NUMBER() OVER (PARTITION BY …)`, 3883; `COUNT(*) OVER (…)`, 712) used here.

---

## 3. Data model + invariants

### 3.1 Tables actually touched
`Location`, `Bookmark`, `Note`, `UserMark`, `BlockRange`, `InputField`, `TagMap`, `Tag`,
`PlaylistItem`, `PlaylistItemMarker`, `PlaylistItemLocationMap`, `PlaylistItemIndependentMediaMap`,
`PlaylistItemMarkerBibleVerseMap`, `PlaylistItemMarkerParagraphMap`, `IndependentMedia`,
`LastModified`, `android_metadata` (see `trim_db` 3858-3927, `downgrade_schema` 1185-1191,
`update_manifest` 1163, 1730-1733).

### 3.2 Location semantics
- `Type = 0` — a document/scripture/track location. CHECK requires one of: non-zero DocumentId; or a
  Track plus (KeySymbol or DocumentId); or BookNumber+KeySymbol with no ChapterNumber; or
  ChapterNumber+BookNumber+KeySymbol (1040-1051).
- `Type = 1` — a *publication* location: no BookNumber/ChapterNumber/DocumentId, KeySymbol required,
  Track NULL (1052-1058). This is the `Bookmark.PublicationLocationId` target.
- `Type IN (2,3)` — no BookNumber/ChapterNumber (1059-1062).
- `UNIQUE (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)` — scripture identity.
- v16 media identity = the `IX_Location_Media` unique index (§2.5 step 4).

### 3.3 Category → identity key (what the tree's checkbox IDs actually are)
From `regroup`'s getters (641-775) and `delete_items` (3658-3671) — these must agree:

| Category | Id column | source query |
|---|---|---|
| Annotations | `Location.LocationId` | 643 |
| Bookmarks | `Bookmark.BookmarkId` | 656, 660 |
| Favorites | `TagMap.TagMapId` | 669, 673 |
| Highlights | `BlockRange.BlockRangeId` | 682, 687 |
| Notes | `Note.NoteId` | 698, 751 |
| Playlists | `PlaylistItem.PlaylistItemId` | 770 |

### 3.4 Category query definitions (rewrite these verbatim)
- **Annotations** (643): `InputField JOIN Location USING (LocationId)`.
- **Bookmarks** (656): `Bookmark b JOIN Location l USING (LocationId)`.
- **Favorites** (669): `TagMap tm JOIN Location l USING (LocationId) WHERE tm.NoteId IS NULL
  ORDER BY tm.Position` — i.e. a Favorite is a **TagMap row with no NoteId**. Export narrows further
  to `TagId = (SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite')` (1460) — the literal
  tag name `'Favorite'` with `Type = 0` is load-bearing.
- **Highlights** (682): `UserMark u JOIN Location l USING (LocationId), BlockRange b USING
  (UserMarkId)` — one row **per BlockRange**, so a multi-block highlight appears as multiple items.
- **Notes** (751): `Note n JOIN Location l USING (LocationId) LEFT JOIN TagMap USING (NoteId)
  LEFT JOIN Tag USING (TagId) LEFT JOIN UserMark USING (UserMarkId)`, inner-ordered by `t.Name`,
  then `GROUP BY NoteId` with `GROUP_CONCAT(Name, ' | ')` — tags are concatenated in name order via
  a **subquery-ordered GROUP_CONCAT**, an SQLite-specific trick. `' | '` is the canonical tag
  separator throughout (698, 751, 1640, 1718).
- **Notes, independent** (698): a *separate* query for `BlockType = 0 AND LocationId IS NULL`,
  labeled `* INDEPENDENT *`, concatenated onto the main frame (762-764). Independent notes have **no
  Location** and are excluded from the main join — a naive single-query rewrite loses them.
- **Playlists** (770): `PlaylistItem JOIN TagMap USING (PlaylistItemId) JOIN Tag t USING (TagId)
  WHERE t.Type = 2 ORDER BY Name, Position` — playlist membership is a `Tag Type = 2`.

Tag types (derived): **0 = system ("Favorite"), 1 = user note tag, 2 = playlist** (1460, 3296, 3828,
3551, 770, 3880).

### 3.5 Duplicate-notes filter
`self.dupes` toggles a CTE (708-747) defining a duplicate as any Note sharing
`(LocationId, BlockIdentifier, BlockType)` with another note **and** matching on one of three
disjuncts: same non-empty Title; same non-empty Content; or both empty. When `dupes` is on,
independent notes are excluded entirely (762-766).

### 3.6 ID recycling invariant
`get_available_ids()` (1857-1869) scans `Location, Bookmark, UserMark, Note, BlockRange, TagMap,
PlaylistItem, IndependentMedia, Tag` and returns, per table, the list of **unused integer IDs in the
gaps** (reversed, so `.pop()` yields the lowest first). Imports prefer a recycled ID and only fall
back to autoincrement when the pool is empty (e.g. 1914-1918, 2162-2166, 2180-2184). Rationale: keep
IDs dense so downstream JW Library / merge behaves. A rewrite that just autoincrements will produce
different — though probably still valid — archives; the gap-filling is deliberate and should be
preserved for byte-comparable output.

### 3.7 UserMark invariants
New UserMarks always get `StyleIndex = 0` and a fresh `uuid.uuid1()` in `UserMarkGuid`
(2161-2166, 3244-3245). `Version` is carried from the import record (2164) or defaults to 1 (3245).

### 3.8 Highlight range merging (import)
`add_usermark` (2160-2184): before inserting, it fetches all existing BlockRanges at the same
`(Identifier, LocationId)` (2167) and **coalesces any overlapping range** — overlap test
`ce >= ns and ne >= cs` (2174), expand to `min/max` (2175-2176), delete the absorbed BlockRanges
(2177-2179), then insert one merged range. Highlights are therefore **union-merged, never
duplicated**.

### 3.9 Annotation upsert
`INSERT INTO InputField (LocationId, TextTag, Value) … ON CONFLICT (LocationId, TextTag) DO UPDATE
SET Value = excluded.Value` (1930) — `(LocationId, TextTag)` is the annotation's natural key, and
Value is `.strip()`ed.

### 3.10 Annotation locations have NULL MepsLanguage
`add_location` for annotations matches and inserts with `MepsLanguage IS NULL / NULL` and `Type = 0`
(1910, 1916, 1918). Annotations are language-less by construction; the tree labels them
`* NO LANGUAGE *` (644).

---

## 4. Non-obvious business rules (a naive rewrite gets these wrong)

1. **Save always trims first.** `trim_db()` runs unconditionally at the top of `zip_file`
   (`JWLManager.py:1245`) — every save silently garbage-collects and VACUUMs. Saving is not
   byte-preserving.
2. **The v14 downgrade is not applied to the working copy.** The pre-downgrade DB is backed up
   (1249) and restored in a `finally` after zipping (1266-1267). The user keeps editing v16.
3. **`userData_backup.db` must be excluded from the zip** (1258-1259) or the archive ships two DBs.
4. **`creationDate` is overwritten on every save** (1158), not preserved from the source manifest.
5. **The manifest hash covers the final DB bytes**, computed after trim+vacuum+schema change (1168)
   — any post-hash DB write invalidates the archive.
6. **`schemaVersion` in the manifest is read back from `PRAGMA user_version`** (1164-1167), never
   assumed.
7. **Downgrade keeps `ids[0]`**, the first row returned by an unordered SELECT (1183). Not min, not
   max. Reproducing "the same" archive requires the same row order (rowid order in practice).
8. **`upgrade_schema` swallows all errors** (1072-1073); `downgrade_schema` hard-exits (1238-1241).
   Asymmetric on purpose.
9. **Deleting a Highlight deletes only the BlockRange** (3664); the UserMark survives until
   `trim_db` decides it is orphaned (3889-3892).
10. **Deleting an Annotation deletes by LocationId** (3669) — all InputFields at that location go,
    not just the selected TextTag.
11. **Grey (index 0) cannot be applied to Highlights** (3255-3256) — silently ignored, no message.
12. **Coloring a Note may create a UserMark** where none existed (3243-3246), which turns a plain
    note into a highlighted one.
13. **`sort_notes` needs the negative-position two-pass** (3829-3834) to dodge the TagMap position
    uniqueness constraint.
14. **Tag positions are re-densified on every save** via `ROW_NUMBER() OVER (PARTITION BY TagId
    ORDER BY Position, TagMapId) - 1` (3883-3886) — positions are always 0-based contiguous per tag
    after a save. This is the "reindex" that the hidden `actionReindex` used to expose (140).
15. **`trim_db` sets `Location.Title = ""` where NULL** (3917) — comment: *"Fix missing note links"*.
    A NULL title breaks note links in JW Library. Do not skip.
16. **`Tag` rows are only GC'd when `Type > 0`** (3880) — the system `Favorite` tag (Type 0) is never
    deleted even if unused.
17. **A Note is only deleted as "empty" if it also has no TagMap** (3871) — an empty but tagged note
    survives.
18. **Playlist thumbnail/media files are shared**: `delete_playlist_items` checks whether the path is
    still referenced by a *non-deleted* PlaylistItem before deleting the `IndependentMedia` row and
    the file on disk (3628-3647). Reference-counted by **FilePath string**, not by id.
19. **`PlaylistItem` orphan rule is inverted**: `DELETE FROM PlaylistItem WHERE PlaylistItemId NOT IN
    (SELECT PlaylistItemId FROM TagMap)` (3898) — a playlist item with no tag mapping is garbage, so
    an item added without its TagMap row vanishes on the next save.
20. **SQL uses inline `IN (...)` built by Python `str(list)` string mangling**
    (`str(...).replace('[','(').replace(']',')')`, e.g. 3682, 3250, 2178). Empty selection produces
    `IN ()` — a syntax error; the code relies on buttons being disabled when nothing is selected
    (500). A rewrite must guard the empty case explicitly.
21. **Import merges highlights rather than appending** (§3.8).
22. **Import reuses ID gaps** (§3.6).
23. **Import wraps each record and issues `ROLLBACK` on the first bad record** (1901, 1933, 2199,
    2259) — imports are all-or-nothing per file.
24. **Markdown export skips unchanged files** by comparing the file's mtime to the note's MODIFIED
    timestamp, and stamps mtime with `os.utime` after writing (1489-1494). Re-exporting to the same
    directory is idempotent and mtime-stable (Obsidian-vault friendly).
25. **xlsx export force-rewrites the last column as a string** (1356) to stop XlsxWriter from
    auto-converting note bodies into hyperlinks.
26. **The txt export header starts with an invisible char** to force UTF-8 detection
    (`category + '\n \n' + …`, 1367-1369).
27. **Pipe-delimited exports escape `|` in free text to `¦`** (`REPLACE(b.Title, "|", "¦")`, 1444).
28. **`'None'` is the null sentinel in pipe exports** (1445, 1461, 1477) and is stripped back to `''`
    on import (`line.rstrip().replace('None','')`, 2191) — so a legitimate literal `None` in text is
    corrupted. Known wart; preserve for compatibility.
29. **Highlight import line validation is `^(\d+\|){6}`** (2188) — lines not matching are silently
    skipped, not errors.
30. **Single-instance handoff via lockfile is racy by design**: second instance writes the path and
    exits (4014-4018); the first polls at 1 Hz (171). The lockfile is deleted on clean exit
    (3959-3960) and also removed at startup if the settings file is missing (3974-3975).
31. **`TMP_PATH` is created at import time** (`mkdtemp`, 64) — one temp dir per process, reused for
    every archive; `load_file`/`new_file` wipe it with `glob(f'{TMP_PATH}/*')` + `os.remove`
    (973-976, 1093-1096) which **does not remove subdirectories** (`os.remove` fails on dirs and the
    exception is swallowed) — leftover media subdirs can leak between archives in one session.
32. **`switchboard` forces the grouping combo back to `Type`** if the current grouping is disabled
    for the new category (522-523), and to `Title` for Playlists (546).
33. **`tree_cache`** keyed by (category index, grouping) short-circuits `get_data` (554-557); any
    mutation must pass `new_data=True` to `regroup` to invalidate.

---

## 5. Supported inputs / outputs

### 5.1 Read (import)
Entry: `import_items(file='', category='')` (1855+); drag-drop sniffing in `dropEvent`
(231-267).

| Input | Detection | Handler |
|---|---|---|
| `.jwlibrary` | suffix | open or merge (`check_file`, 234-235) |
| `.jwlplaylist` | suffix | `import_items(file, 'Playlists')` (238-239) |
| `.txt` `{ANNOTATIONS}` | first line (243) | `import_annotations` (1871) |
| `.txt` `{BOOKMARKS}` | first line (245) | `import_bookmarks` (1958) |
| `.txt` `{FAVORITES}` | first line (247) | `import_favorites` (2044) |
| `.txt` `{HIGHLIGHTS}` | first line (249) | `import_highlights` (2124) |
| `.txt` `{NOTES=` | regex on first line (251) | `import_notes` (2212) |
| `.xlsx` annotations | columns ⊇ `{PUB, ISSUE, DOC, LABEL, VALUE}` (256, 260) | `import_annotations` |
| `.xlsx` notes | columns ⊇ the 16 note columns (257, 262) | `import_notes` |

Unrecognized ⇒ `File "{}" not recognized!` (254, 265, 267). Import while no archive is open ⇒
`No archive has been opened!` (236-237).

### 5.2 Text (`.txt`) formats
Header written by `export_header` (1367-1369):
```
{CATEGORY}
<space>
Exported from <archive>
by JWLManager (v12.5.0) on <YYYY-MM-DD @ HH:MM:SS>
****************************************************************************
```
Then one record per line, or `===`-delimited blocks for Notes/Annotations.

- **{BOOKMARKS}** — pipe-joined, 12 fields (1444):
  `BookNumber|ChapterNumber|DocumentId|IssueTagNumber|KeySymbol|MepsLanguage|Type|Slot|Title|Snippet|BlockType|BlockIdentifier`
  (Title/Snippet with `|`→`¦`).
- **{FAVORITES}** — 6 fields (1460): `DocumentId|Track|IssueTagNumber|KeySymbol|MepsLanguage|Type`,
  ordered by `TagMap.Position`.
- **{HIGHLIGHTS}** — 13 fields (1476):
  `BlockType|Identifier|StartToken|EndToken|ColorIndex|Version|BookNumber|ChapterNumber|DocumentId|IssueTagNumber|KeySymbol|MepsLanguage|Type`.
- **{ANNOTATIONS}** — block format, header `==={…}===` then the value until the next `==={`
  (parsed 1892; attrs via `{(.*?)=(.*?)}` at 1885). Attribute schema `{PUB, ISSUE, DOC, LABEL}` +
  body `VALUE` (1903).
- **{NOTES=}** — block format (written 1637-1668, parsed 2245-2262). Header attributes, in the
  written order:
  `{CREATED}{MODIFIED}{TAGS}` then either the **scripture** branch
  `{LANG}{PUB}{BK}{CH}[{VS}][{BLOCK}][{Reference}][{HEADING}]{COLOR}[{RANGE}][{DOC=0}]`
  (1655-1659) or the **publication** branch
  `{LANG}{PUB}[{ISSUE}][{DOC}][{BLOCK}][{HEADING}]{COLOR}[{RANGE}]` (1660-1665).
  Body = first line is TITLE, remainder is NOTE (1666, 2254-2255). Tags joined with `|` in txt
  (1640) but `' | '` in the DB/xlsx. File terminated with `\n==={END}===` (1668) — the parser's
  lookahead `(?=\n==={)` **requires** this sentinel or the last note is dropped.
  Import schema/types (2261): `CREATED,MODIFIED:str; TAGS:str; COLOR:int; RANGE:str; LANG:int;
  PUB:str; BK,CH,VS,ISSUE,DOC,BLOCK:int; HEADING,TITLE,NOTE:str`.

### 5.3 Excel (`.xlsx`)
Written by `create_xlsx` (1345-1365): single sheet named `JWLManager`, bold header row, autofilter
rows 0..99999, freeze panes at row 1, col widths 0-2 = 20 / 3+ = 12, workbook properties carry
title = category and a provenance comment (1348).
- **Notes fields** (1634): `CREATED, MODIFIED, TAGS, COLOR, RANGE, LANG, PUB, BK, CH, VS, Reference,
  ISSUE, DOC, BLOCK, HEADING, Link, TITLE, NOTE`.
  `Reference` = `BK.zfill(2) + CH.zfill(3) + VS.zfill(3)` (`'000'` when no verse) (1610-1614).
  `Link` = `https://www.jw.org/finder?wtlocale={lang_symbol[LANG]}&pub={PUB}&bible={Reference}` for
  scripture (1615) or `…finder?wtlocale=…&docid={DOC}[&par={BLOCK}]` for publications (1622-1624);
  NULL when LANG or DOC is missing (1625-1626). `HEADING` defaults to `"<BibleBook> <CH>"` (1617)
  and gets `:VS` appended when it lacks a colon (1618-1619).
- **Annotations fields**: `PUB, ISSUE, DOC, LABEL, VALUE` (import requires exactly these, 1949).

Read back with `pl.read_excel(engine='xlsx2csv', …)` (258, 1951).

### 5.4 Markdown export (Notes only)
`export_items('md')` → directory picker (1343); one file per note (1669-1722).
Path: `<dir>/<PUB>-<langsym>/<BK.zfill(2)>_<BookName>/<CH.zfill(3)>/<VS.zfill(3)>_<title>_<guid[:8]>.md`
for scripture (1682-1684); `<dir>/<PUB>-<langsym>/[<issue>/]<DOC>/[<BLOCK.zfill(3)>_]<title>_<guid[:8]>.md`
for publications (1686-1692); `<dir>/INDEPENDENT/…` for independent notes (1679-1680).
Filename title sanitized by `shorten_title` (1496+): `:`→`.` after a digit else `-`, strip anything
not `[\w\s\-,().;]`, fall back to `UNTITLED` when empty (1497-1500).
YAML front matter keys, in order (1695-1721): `title, created, modified, [language], [publication],
[document], [heading], [link], color, [tags], guid`, then `# <TITLE>` and the body.
**Obsidian wiki-links are conditional on the current grouping**: `language` is `[[…]]` only when
grouping == Language (1699-1702); `color` is `[[…]]` only when grouping == Color (1712-1715);
`document`/`Reference` are always `[[…]]` (1704-1707).
Issue formatting `process_issue` (1309-1318): `YYYYMMDD` → `YYYY-MM[-DD]`, `DD == '00'` dropped.

### 5.5 Export file naming
`export_file` (1320-1343): default names `JWL_<Category>_<YYYY-MM-DD>.<ext>` in `working_dir`.
Highlights/Bookmarks/Favorites are **txt-only** (1322-1323). Playlists → `.jwlplaylist` (1324-1328).
Notes/Annotations offer a submenu: **MS Excel file / Custom text file / Markdown files**
(`export_menu`, 1286-1305) mapping to `xlsx` / `txt` / `md`; other categories go straight to
`export_items('')` (1304-1305). The last-used format persists as `JWLManager/format` (1334, 1340,
3951).

### 5.6 Settings persisted (QSettings, INI, `JWLManager.conf` next to the executable)
`set_settings_path` (3963-3976), `clean_up` (3943-3958). Keys: `JWLManager/{language, category,
title, theme, column1, column2, sort, direction, format}`, `Main_Window/{position, size}`,
`Viewer/{position, size}`, `Help/{position, size}`, `Tag/size`.

---

## 6. Localization

- **Mechanism**: stdlib `gettext` for app strings + Qt `QTranslator` for Qt's own strings.
- Catalogs live at `res/locales/<lang>/LC_MESSAGES/messages.mo`; loaded per language at startup with
  `gettext.translation('messages', localedir, fallback=True, languages=[k])` for **every** available
  language into a dict `tr[k]` (`JWLManager.py:3996, 4003-4005`) — all catalogs are loaded eagerly so
  runtime language switching needs no reload.
- `_` is bound globally as `tr[lng].gettext` in `read_resources` (4037-4039). Every user-facing
  string is wrapped in `_()`.
- **Ship list** (`available_languages`, 3984-3994): `de, en, es, fr, it, pl, pt, ru, uk` — German,
  English (default), Spanish, French, Italian, Polish, Portuguese, Russian, Ukrainian.
- **CLI**: each language is a mutually-exclusive flag `-de -en -es -fr -it -pl -pt -ru -uk`
  (3998-4004). Precedence: CLI flag > `QSettings JWLManager/language` > `'en'` (4007-4011).
- **Qt catalog**: `res/locales/UI/qt_<lang>.qm` installed on the QApplication (4066-4067).
- Runtime switch: `change_language` (405-429) rebinds `_`, re-reads resources, retranslates the UI
  (`changeEvent` on `QEvent.LanguageChange` → `retranslateUi`, 208-210) and the viewer
  (`retranslate_viewer`, 430-444). Menu entries for languages not in `available_languages` are hidden
  (127-131).
- **Language is also a data concern**, not just UI: `res/resources.db` holds `Languages(Language,
  Name, Code, Symbol)` and `BibleBooks(Number, Name, Language)` (4026, 4030). The UI language code
  maps to a MEPS language id, which selects the Bible book names and the publication titles
  (4033-4035, 4045-4046). `lang_symbol` feeds the jw.org finder URLs (1615, 1624).
- **Category names are translated strings and are used as control-flow keys** —
  `if category == _('Notes')` (e.g. 531, 1287, 3659, 3665). A rewrite must use stable enum
  identifiers instead; this is a latent bug class in the original.

---

## 7. Native `jwlCore` bridge (merge)

`jwlcore.py` (84 lines, full contract):
- Platform resolution `_platform_lib_name` (`jwlcore.py:29-38`): linux → `libjwlCore-x86_64.so`,
  darwin → `libjwlCore.dylib`, win32 → `jwlCore-amd64.dll`; anything else raises `OSError`.
  (`libs/libjwlCore-arm64.so` exists on disk but is **not** selected by this function — Linux ARM64
  is effectively unreachable.)
- Loading `_load_lib` (40-55): base = `sys._MEIPASS` under PyInstaller else the module dir; on
  Windows the DLL sits at the base, elsewhere under `libs/`; non-Windows uses `RTLD_LOCAL`.
- FFI surface (61-71):
  - `setProgressCallback(CFUNCTYPE(None, c_int)) -> None`
  - `mergeDatabase(c_char_p path1, c_char_p path2, c_bool downgrade) -> c_int` (0 = success)
  - `getLastResult() -> c_char_p` (UTF-8 message)
  - `getCoreVersion() -> c_char_p`
- Wrappers `merge_databases(path1, path2, downgrade=False)`, `get_last_result()`,
  `get_core_version()` (74-83). Paths are UTF-8 encoded; `path1` is the **destination**
  (`TMP_PATH`), `path2` the incoming extract (`JWLManager.py:2672`).
- `CORE_VERSION` is fetched at import and shown by `--version` (`JWLManager.py:62, 3999`).

**Rewrite risk**: merge de-dup/conflict-resolution semantics exist only as a compiled binary; there
is no Python reference. A Tauri rewrite either (a) links the same `jwlCore` binaries via FFI, or
(b) must reverse-engineer merge behavior from the binary/its upstream project. Nothing in this repo
documents its rules.

---

## 8. Dependencies a rewrite must replace

`PySide6`/`shiboken6` (UI) · `polars` (all tree data + xlsx read) · `XlsxWriter` (xlsx write) ·
`xlsx2csv` (xlsx read engine) · `Pillow` (playlist thumbnails) · `regex` (V1 set-subtraction classes
— `re` cannot express `[\p{Zs}--\x20]`, 3730-3732) · `puremagic` (media type sniffing) · `requests`
(update check 296, crash report 332) · stdlib `sqlite3`/`zipfile`/`gettext`/`ctypes`/`uuid`/`hashlib`
(`JWLManager.py:33-59`).

Bundled data: `res/blank`, `res/blank_playlist`, `res/resources.db`
(`Publications`, `Extras`, `Types`, `Favorites`, `Languages`, `BibleBooks` — 4026-4052),
`res/dark.qss`, `res/light.qss`, `res/icons/{dark,light}/`, `res/locales/`, `res/HELP.md`,
`res/HILFE.md`.
