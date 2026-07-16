# erykjj Public Repos — Unified-App Merge Survey

Date: 2026-07-16. Source: GitHub REST API (`/users/erykjj/repos`, `/repos/{r}/readme`, `/repos/{r}/git/trees/HEAD?recursive=1`), plus local `C:\Users\artic\GitHub\jwlmanager` working tree.

Baseline app = **jwlmanager**: PySide6 GUI over `.jwlibrary` / `.jwlplaylist` backup archives (view/edit/delete/merge/import/export/cleanup of Annotations, Bookmarks, Favorites, Highlights, Notes, Playlists). Python entry `JWLManager.py` (~215 KB), ctypes bridge `jwlcore.py`, deps `PySide6 6.9`, `polars`, `XlsxWriter`, `xlsx2csv`, `pillow`, `puremagic`, `regex`, `requests` (`res/requirements.txt`).

## Inventory (16 public repos)

| repo | language | ★ | last push | classification | one-line purpose |
|---|---|---|---|---|---|
| jwlmanager | Python | 125 | 2026-07-16 | **BASELINE** | PySide6 GUI to view/edit/merge/export `.jwlibrary` backups |
| jwlFusion | Nim | 17 | 2026-07-04 | **CORE** | CLI merge utility for `.jwlibrary` backups (annotations/bookmarks/highlights/notes/favorites/playlists) |
| jwlFusion-app | — (binary-only) | 12 | 2026-07-04 | **CORE** | Android app doing the same merge, offline; Play Store listed |
| jwlIntegrator | — (binary-only) | 6 | 2026-06-16 | **ADJACENT** | CLI that injects custom `.jwpub` archives into installed JW Library (Win/macOS/Android) |
| jwpublib | — (content) | 41 | 2026-06-30 | **ADJACENT** | Collection of public-domain `.jwpub` reference publications (content, not code) |
| jwlFission-app | — (binary-only) | 6 | 2026-07-10 | **ADJACENT** | Android app to view/export VTT subtitles/transcripts of JW videos |
| nwtReactor-app | — (binary-only) | 1 | 2026-04-11 | **ADJACENT** | Android app streaming NWT Bible audio for parsed verse playlists |
| linkture | Python | 15 | 2026-07-05 | **LIBRARY** | Python lib/PyPI: parse/tag/hyperlink/translate/BCV-encode scripture refs (23 langs) |
| refractor | Nim | 8 | 2026-05-09 | **LIBRARY** | Nim CLI reference extractor for txt/docx (scriptures + publication refs) |
| traverture | TypeScript | 6 | 2026-07-13 | **UNRELATED** | Obsidian plugin: scripture ref parsing/preview/insert |
| nim-tabulator | CSS(Nim) | 1 | 2026-01-12 | **LIBRARY** | Nim plain-text table renderer (used by Nim CLIs) |
| nebulder | Shell | 13 | 2026-05-29 | **UNRELATED** | Nebula VPN deployment package builder |
| scrambler | Python | 2 | 2024-01-16 (archived) | **UNRELATED** | Word scrambler |
| fsarchiver | C | 0 | 2026-05-17 | **UNRELATED** | Fork — Linux filesystem archiver |
| news | PHP | 0 | 2026-05-26 | **UNRELATED** | Fork — RSS/Atom reader |
| threema-desktop-appimage | TypeScript | 0 | 2026-06-17 | **UNRELATED** | Fork — Threema AppImage packaging |

Licenses: jwlmanager/linkture/nebulder/nim-tabulator/traverture/scrambler = MIT; jwpublib = Apache-2.0; jwlFusion, jwlFusion-app, jwlFission-app, jwlIntegrator, nwtReactor-app, refractor = `NOASSERTION` (non-OSI custom terms — a real merge blocker to check before absorbing). No repo is archived except `scrambler`.

## CORE / ADJACENT detail

### jwlFusion (Nim, 17★) — CORE — merge difficulty **LOW (already done)**
- **Does:** `jwlFusion [-o:out] <original> <merge>...`, merges annotations, bookmarks (10/pub cap), highlights, notes+tags, favorites, playlists+media; `--downgrade` writes schema v14. Single source file `src/jwlFusion.nim` (294 lines); repo ships prebuilt `.dll/.so/.dylib` alongside the `.exe`.
- **Overlap:** total — jwlmanager's README already advertises merge, and `jwlcore.py` is a ctypes bridge to a **native `jwlCore` shared library** (`libjwlCore-x86_64.so` / `libjwlCore.dylib` / `jwlCore-amd64.dll`) exposing `mergeDatabase(char*, char*, bool)`, `setProgressCallback`, `getLastResult`, `getCoreVersion`. The `bool` is the same `--downgrade` flag. Recent commits `61f0a4b8 Fix jwlCore bindings` / `1a1e3213 Make call class-level upgrade_schema` confirm this is live.
- **Unique value:** the standalone CLI/headless entry point and the ARM64 builds.
- **Difficulty:** low — **the engine is already absorbed**; only the CLI wrapper is separate. Language mismatch (Nim vs Python) is already solved via the FFI boundary, so "merging" here means retiring the standalone CLI repo, not porting code.

### jwlFusion-app (Android, 12★) — CORE — merge difficulty **HIGH**
- **Does:** same merge feature set as jwlFusion, offline, primary-over-secondary precedence; requires JW Library ≥ 15.8 to restore. Distributed via Play Store (`org.infiniti.jwlfusion.android`).
- **Overlap:** 100 % functional overlap with jwlmanager's merge.
- **Unique value:** mobile/Android reach + Share-to import/export flow.
- **Difficulty:** high — **repo is release/docs only** (13 tree entries: 5 `.md`, `.svg`, `.gif`, `.png`; no `.kt`/`.java`/`.gradle`), so the source isn't public to merge. PySide6 has no viable Android target. Realistic unification = both frontends calling the same `jwlCore` native lib, not one codebase.

### jwlIntegrator (Android/Win/macOS CLI, 6★) — ADJACENT — merge difficulty **MED–HIGH**
- **Does:** `jwlIntegrator <JWPUB archive>` — installs a custom `.jwpub` into the *installed JW Library app's* own catalog/DB (Windows, macOS; Android via Termux as root, or downgrade to v15.6.1).
- **Overlap:** ~none with jwlmanager. jwlmanager touches only backup archives — `grep 'jwpub'` over `JWLManager.py` returns nothing.
- **Unique value:** a whole second domain (installed-app publication catalog) that jwlmanager doesn't address; natural companion to jwpublib.
- **Difficulty:** med–high — **binary-only repo** (28 entries: `.exe`, `.dll`, `.sh`, `.jpg`; no source), different data model (JW Library's live catalog DB + filesystem paths vs a portable backup zip), requires root/OS-specific install paths, and its README carries an explicit conscience/ToS disclaimer. Bundling it into jwlmanager imports that risk surface into a 125★ MIT app.

### jwpublib (Apache-2.0, 41★) — ADJACENT — merge difficulty **LOW to link, N/A to absorb**
- **Does:** public-domain `.jwpub` publications (concordance, Strong's, Vine's, Josephus, KJV+Strong's, timelines…) — **content, not code**; explicitly says installation is via jwlIntegrator.
- **Overlap:** none.
- **Unique value:** the payload jwlIntegrator installs.
- **Difficulty:** absorbing is a category error (it's a publication repo, tens of MB of archives). If jwlmanager ever gained a "browse/install add-on publications" tab, jwpublib would be its remote catalog — a fetch target, not a merge.

### jwlFission-app (Android, 6★) — ADJACENT — merge difficulty **HIGH**
- **Does:** takes a shared JW Library/JW.ORG video link, shows the VTT subtitles as timestamped subs or a reflowed transcript; language switch, save/share, deep-link to a segment.
- **Overlap:** none (`vtt`/`subtitle` absent from `JWLManager.py`); only shares the JW-media domain and the Playlist concept.
- **Unique value:** VTT fetch/parse + transcript reflow.
- **Difficulty:** high — binary-only repo, mobile share-intent-driven UX with no desktop analogue, network-dependent.

### nwtReactor-app (Android, 1★) — ADJACENT — merge difficulty **HIGH**
- **Does:** parses pasted text (≤15 k chars) for scripture refs, builds a playlist, streams NWT audio via the jw.org API in 12 languages; playlists saved/shared as text.
- **Overlap:** thin — jwlmanager manages `.jwlplaylist` playlists, but those are JW Library media playlists, not a streaming queue.
- **Unique value:** ref-parse → audio playlist; strongest cross-pollination is its *reference parser* (i.e. linkture), not the player.
- **Difficulty:** high — binary-only repo, requires internet + jw.org API dependence, 1★ demand, and it would drag a media player into a data-editor app.

### LIBRARY (do not merge — depend on instead)
- **linkture** (MIT, PyPI, Python): the one clean technical fit — same language, packaged on PyPI, and jwlmanager does *not* currently depend on it (`res/requirements.txt` has no linkture). Adding it as a dependency would let Notes/Highlights show resolved scripture refs. That's `pip install linkture`, not a merge.
- **refractor** (Nim) and **nim-tabulator** (Nim): refractor is a terminal-output reference extractor; tabulator is its plain-text table renderer. Both are CLI-shaped and Nim; a Qt GUI has no use for ANSI tables. Leave.

## Merge Recommendation

Ranked by (value of merging) ÷ (difficulty):

1. **jwlFusion — ABSORB (finish the job). Value high / difficulty low.** The merge engine is *already* inside jwlmanager as the `jwlCore` native lib behind `jwlcore.py`. Remaining work is only to declare jwlmanager the front-end of record and either keep jwlFusion as a thin CLI over the same lib or retire it. Evidence: `lib.mergeDatabase(char*, char*, bool)` mirrors `jwlFusion --downgrade`; commit `61f0a4b8 Fix jwlCore bindings`.
2. **linkture — DEPEND, don't merge. Value med / difficulty low.** Add to `res/requirements.txt` if scripture-ref rendering is wanted. Merging a 15★ PyPI library into a GUI app would strip its reusability for refractor/traverture/nwtReactor.
3. **jwlIntegrator + jwpublib — OPTIONAL FUTURE FEATURE, not a merge. Value med / difficulty high.** Together they'd give jwlmanager a real new capability ("install public-domain add-on publications"). Blocked by: no public source for jwlIntegrator, `NOASSERTION` licence vs jwlmanager's MIT, root/OS-path requirements, and the ToS-risk disclaimer. Revisit only if the author intends to open the source and accept the licence change.
4. **jwlFusion-app / jwlFission-app / nwtReactor-app — LEAVE STANDALONE. Value low / difficulty high.** All three are release-only repos (no source in the tree), Android-only, and PySide6 cannot ship to Android. Unification, if desired, is at the **native-lib layer** (`jwlCore`), not the app layer.
5. **refractor, nim-tabulator, traverture — LEAVE STANDALONE.** Different runtimes (Nim, Obsidian/TS) and different hosts; zero overlap with a desktop archive editor.
6. **nebulder, scrambler, fsarchiver, news, threema-desktop-appimage — LEAVE ALONE.** Unrelated; three are forks, one is archived.

**Recommended absorb set:** jwlFusion (formalize; already effectively in), + linkture as a dependency.
**Recommended leave-standalone set:** all three Android apps, jwlIntegrator, jwpublib, refractor, nim-tabulator, traverture, and the 5 unrelated/fork repos.

### Cross-cutting blockers
- **Licence:** every JW-ecosystem repo except jwlmanager/linkture/jwpublib is `NOASSERTION`. Absorbing any of them into MIT-licensed jwlmanager needs an explicit relicence from the author.
- **Source availability:** 5 of the 7 CORE/ADJACENT repos (`jwlFusion-app`, `jwlFission-app`, `nwtReactor-app`, `jwlIntegrator`, `jwpublib`) publish **no source code** — trees contain only docs, images, and prebuilt binaries. You cannot merge what isn't published.
- **Data model:** jwlmanager operates on a portable backup zip (`userData.db`, schema v14+); jwlIntegrator operates on the installed app's live catalog; the Android apps carry their own storage. Only jwlFusion shares jwlmanager's schema.
