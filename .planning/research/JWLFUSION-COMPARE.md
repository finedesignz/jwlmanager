# jwlFusion vs JWLManager — Comparison & Absorption Analysis

*Research date: 2026-07-16. jwlFusion @ v2.7.1 (`ad12fec`), shallow clone of https://github.com/erykjj/jwlFusion.*
*Local context from `.planning/codebase/ARCHITECTURE.md` + `STACK.md` (not re-derived).*

---

## Headline finding

**jwlFusion is not a second merge implementation. It is a second *front-end* over the exact same merge engine JWLManager already links.** Both projects load byte-identical `jwlCore` binaries, authored by the same person (Eryk J.), and both call the same five exported C symbols. The merge/conflict-resolution logic exists in neither repo's source — it lives in the closed-source `jwlCore` shared library that both vendor as prebuilt blobs.

Verified by hash (first 16 hex chars of SHA-256):

| Library | `jwlmanager/libs/` | `jwlFusion/lib/` | Identical |
|---|---|---|---|
| `jwlCore-amd64.dll` | `063b8e8573bc5253` | `063b8e8573bc5253` | **YES** |
| `libjwlCore.dylib` | `7f2f7b1f0c627dc0` | `7f2f7b1f0c627dc0` | **YES** |
| `libjwlCore-arm64.so` | `47f58e88f82443fc` | `47f58e88f82443fc` | **YES** |
| `libjwlCore-x86_64.so` | `3a68bfa047fad517` | `3a68bfa047fad517` | **YES** |

This single fact drives every recommendation below.

---

## What jwlFusion is (evidence-cited)

**A ~300-line single-file Nim CLI wrapper.** Entire tracked source tree (`git ls-files`):

```
.github/workflows/{build_lin.yml,build_mac.yml,release.yml}
CHANGELOG.md  LICENSE.md  README.md
lib/{jwlCore-amd64.dll, jwlCore-arm64.dll, libjwlCore-arm64.so,
     libjwlCore-x86_64.so, libjwlCore.dylib,
     bzip2.dll, sqlite3_64.dll, sqlite3_ARM.dll, unzip.exe, zip.exe}
res/{dbFusion_wide.png, logo_tm.png}
src/jwlFusion.nim          <-- the ONLY source file
```

There is no `src/merge.nim`, no schema module, no conflict engine. `src/jwlFusion.nim` is the whole program.

### What the Nim file actually does

**1. Declares the same FFI surface `jwlcore.py` declares** (`src/jwlFusion.nim`):

```nim
proc mergeDatabase(path1, path2: cstring, downgrade: bool): cint {.cdecl, dynlib: libName, importc.}
proc getCoreVersion(): cstring {.cdecl, dynlib: libName, importc.}
proc getZuluTime(): cstring {.cdecl, dynlib: libName, importc.}
proc getLastResult(): cstring {.cdecl, dynlib: libName, importc.}
proc setProgressCallback(cb: ProgressCallback) {.cdecl, dynlib: libName, importc.}
```

Compare `jwlmanager/jwlcore.py`, which binds four of the same five:

```python
lib.mergeDatabase.argtypes  = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_bool]
lib.getLastResult.restype   = ctypes.c_char_p
lib.getCoreVersion.restype  = ctypes.c_char_p
lib.setProgressCallback.argtypes = [CALLBACKTYPE]
```

The only symbol jwlFusion uses that JWLManager does not bind is **`getZuluTime()`** — JWLManager generates timestamps in Python instead.

**2. Unzip → validate → merge loop → rezip.** `unzipArchive` shells out to `unzip`, parses `manifest.json`, and gates on schema:

```nim
let schema = if userDataBackup.hasKey("schemaVersion"): userDataBackup["schemaVersion"].getInt else: 0
if schema > 11:
  return path
echo "Old schema version!"
return ""
```

`main(inputFiles, outputFile, downgrade)` then folds each subsequent archive into the first via repeated `mergeDatabase(db1Path, db2Path, downgrade)`, bailing on any non-zero status.

**3. Rewrites the manifest and rehashes.** `createArchive`:

```nim
let schemaVersion = if downgrade: 14 else: 16
manifest["userDataBackup"]["deviceName"] = %fmt"{App}_v{Version}"
manifest["userDataBackup"]["lastModifiedDate"] = %tz
let hash = sha256File(dbFile)
manifest["userDataBackup"]["hash"] = %hash
manifest["userDataBackup"]["schemaVersion"] = %int(schemaVersion)
```

**4. Terminal chrome.** A four-frame ASCII spinner (`['\\','|','/','-']`) driven by the `setProgressCallback` hook — the CLI analogue of JWLManager's `QProgressDialog`.

### Its release/versioning story

**jwlFusion is a release vehicle for jwlCore, not a product with its own roadmap.** Nearly every tag is a lib bump — from `git log`:

```
280b05a Update jwlCore libs to v0.32.1     -> v2.7.1
5ed4981 Update jwlCore libs to v0.32.0     -> v2.7.0
b334dbd Update jwlCore libs to v0.31.1
3624822 Add downgrade option               <- the only real feature in recent history
6363520 Update jwlCore libs to v0.31.0
d8ee711 Set schema version to 14
```

CHANGELOG confirms the pattern — v2.2.0 through v2.7.1 are *all* "Updated jwlCore libs to vX", with the changelog describing fixes that happened **inside the closed lib** ("Fixed potential tag reindexing issues", "Don't clean up (remove) empty *tagged* notes", "Fix NULL Title"). CI (`.github/workflows/release.yml`) draft-releases prebuilt binaries for linux x86_64/arm64, macOS universal, Windows amd64/arm64 on `v*` tags.

**Licensing (decisive):**
- jwlFusion `LICENSE.md` — **Infiniti Noncommercial License v1.2**: "You are expressly PROHIBITED from: sharing, distributing, publishing, or otherwise making available the original software or any derivative works, in whole or in part, to any third party" + no commercial use. Enforcement rests on a "CODERS' CODE OF HONOR".
- JWLManager `LICENSE` — **MIT**, Copyright (c) 2025 Eryk J.
- `jwlmanager/jwlcore.py` header — **MIT**, Copyright (c) 2025 Eryk J.

Same author, deliberately different licenses. The MIT-licensed ctypes bridge is the author's *sanctioned* path to jwlCore; the Nim CLI is not redistributable.

---

## Capability matrix

| Capability | JWLManager | jwlFusion | Notes |
|---|---|---|---|
| **Merge engine** | jwlCore (ctypes) | jwlCore (Nim FFI) | **Identical binary.** Zero differentiation. |
| Conflict resolution | delegated to jwlCore | delegated to jwlCore | Neither repo implements it — see Conflict section |
| Interface | GUI only (PySide6) | CLI only | **Complementary** |
| Merge arity | one archive per action (`merge_items(file)`, `JWLManager.py:2637`) | **N archives, ordered fold** (`main(inputFiles: seq[string], ...)`) | jwlFusion's real edge |
| Merge precedence control | implicit (current ← incoming) | explicit by argv order; "the most 'definitive' should be listed last" (README) | jwlFusion documents the semantics; JWLManager does not surface them |
| `--downgrade` at merge | **no** — hardcoded `merge_databases(..., False)` (`JWLManager.py:2672`) | **yes** — `--downgrade` → schema v14 | See Conflict #2 |
| Schema downgrade | yes, but re-implemented in Python (`downgrade_schema`, `JWLManager.py:1172`), applied at save | yes, via jwlCore's own `downgrade` flag | **Overlap — done twice, two different ways** |
| Schema upgrade | yes (`upgrade_schema`, `JWLManager.py:1016`) | **no** — rejects `schema <= 11` and exits | JWLManager-only |
| Input schema gate | `schema > 11` (`JWLManager.py:1003`) | `schema > 11` (`unzipArchive`) | **Agreed** |
| Output schema default | 16 (`JWLManager.py:989`, `:1810`) | 16 (`createArchive`) | **Agreed** |
| View / browse records | yes (tree, `DataViewer`) | no | JWLManager-only |
| Edit / delete / tag / recolor | yes | no | JWLManager-only |
| Export (xlsx / md / txt) | yes (`export_items`, `JWLManager.py:1307`) | no | JWLManager-only |
| Import (xlsx / csv / playlist) | yes (`import_*`) | no | JWLManager-only |
| Cleanup of orphan records | yes (README) | no | JWLManager-only |
| i18n | 9 locales (`res/locales/`) | English only | JWLManager-only |
| Progress reporting | `QProgressDialog` + `CALLBACKTYPE` | ASCII spinner + same callback | Same hook, different sink |
| Zip handling | stdlib `zipfile` / `ZipFile` | **shells out** to `zip.exe`/`unzip.exe` (`execShellCmd`) | JWLManager strictly better |
| mkdir/rmdir | `pathlib`/`shutil` | **shells out** (`mkdir `, `rmdir /S /Q `) | JWLManager strictly better |
| Windows arm64 | **no** — `libs/` has no `jwlCore-arm64.dll` | **yes** — ships `jwlCore-arm64.dll` | **Gap in JWLManager** |
| Linux arm64 | ships `libjwlCore-arm64.so` but **never selects it** | selects at compile time via `dynlib` | **Latent bug — see below** |
| Language / runtime | Python 3 + PySide6 + polars + xlsxwriter | Nim + nimcrypto + parseopt | No shared toolchain |
| License | MIT | Infiniti Noncommercial v1.2 | **Absorption blocker for code, not for capability** |

---

## Overlap / Complement / Conflict

### OVERLAP (same job done twice)

**O1 — The FFI bridge.** `jwlcore.py`'s `_platform_lib_name`/`_load_lib` and jwlFusion's `when defined(windows)/elif defined(macosx)/else` block solve the identical problem: pick the right `jwlCore` blob per platform and bind five symbols. Two languages, one contract. This is duplication *by necessity* (different runtimes), not by accident.

**O2 — Archive envelope handling.** Both unzip, read `manifest.json`, gate on `schemaVersion > 11`, rewrite `deviceName`/`lastModifiedDate`/`hash`/`schemaVersion`, and rezip. JWLManager does it in Python (`load_file`, `zip_file`, `check_validity`); jwlFusion in Nim (`unzipArchive`, `createArchive`). Same rules, independently maintained — which means they can silently drift when JW Library changes the manifest.

**O3 — Schema downgrade.** *Genuine, wasteful overlap.* jwlCore's `mergeDatabase` accepts a `downgrade: bool` that performs the downgrade natively — jwlFusion uses it. JWLManager **passes `False`** and instead runs a hand-rolled Python `downgrade_schema()` (`JWLManager.py:1172`) at save time. The same transformation exists in two places, one of them tested by the upstream author and one of them not.

### COMPLEMENT (each does what the other can't)

**C1 — jwlFusion → JWLManager: headless + N-way.** JWLManager's `merge_items` merges exactly one archive per invocation and requires a human at a `QFileDialog`. jwlFusion folds `<original> <merge1> <merge2> ...` in one shot with defined precedence, scriptable, no display server. For "consolidate five family devices' backups nightly", JWLManager cannot do the job at all.

**C2 — jwlFusion → JWLManager: Windows arm64.** jwlFusion ships `jwlCore-arm64.dll`; JWLManager's `libs/` has no such file, so JWLManager cannot run natively on Windows-on-ARM.

**C3 — JWLManager → jwlFusion: everything else.** View, edit, delete, tag, recolor, export (xlsx/markdown/text), import, orphan cleanup, schema *upgrade*, 9-locale i18n. jwlFusion does none of it and has no architecture to grow it into — it is a fold over `mergeDatabase` and nothing more.

**C4 — Non-overlapping robustness.** JWLManager uses stdlib `zipfile`; jwlFusion `execShellCmd`s `zip`/`unzip`/`mkdir`/`rmdir`, which breaks on paths with spaces/quotes and drags `zip.exe`, `unzip.exe`, `bzip2.dll` into its release payload. The complement runs *toward* JWLManager here.

### CONFLICT (incompatible assumptions)

**F1 — License asymmetry (hard blocker for code reuse).** jwlFusion is Infiniti Noncommercial v1.2, forbidding distribution of the software *or derivative works, in whole or in part*. JWLManager is MIT. **Copying any part of `jwlFusion.nim` into JWLManager would relicense-launder noncommercial code into an MIT product and violate the license.** Note this is not adversarial — it is the same author drawing a deliberate line. The MIT `jwlcore.py` is the sanctioned reuse path.

**F2 — Downgrade authority is contested.** jwlFusion asserts jwlCore owns downgrade (`mergeDatabase(..., downgrade)` → v14). JWLManager asserts *Python* owns downgrade (`merge_databases(..., False)` at `:2672`, then `downgrade_schema()` at `:1172`). Both write `schemaVersion` into the manifest, from different code. If jwlCore's downgrade and JWLManager's Python downgrade ever disagree on a table, the archive JWLManager emits differs from the one jwlFusion emits **from identical inputs**. This is a real correctness conflict, not a style difference.

**F3 — Manifest ownership.** Both stamp `deviceName` (`jwlFusion_v2.7.1` vs JWLManager's own string) and recompute the db hash. Round-tripping an archive through both tools produces churn in provenance fields. Cosmetic, but it means "merged by" is last-writer-wins.

**F4 — Latent arm64 bug in JWLManager (found during this review).** `jwlcore.py:_platform_lib_name` is architecture-blind:

```python
if sysname.startswith("linux"):
    return f"lib{base}-x86_64.so"       # <-- always x86_64
elif sysname == "win32":
    return f"{base}-amd64.dll"          # <-- always amd64
```

`sys.platform` is `"linux"` on aarch64 too. So `libs/libjwlCore-arm64.so` — which **is** shipped and **is** byte-identical to jwlFusion's — is dead weight that can never load; JWLManager on Linux/arm64 will try to `CDLL` an x86_64 `.so` and fail. jwlFusion sidesteps this by resolving `dynlib` per build target. Fix is `platform.machine()`, ~6 lines. **Not a conflict with jwlFusion — a bug jwlFusion's existence exposes.**

---

## Absorption recommendation

### Verdict: **ABSORB-LOGIC-ONLY** — and note the logic is already absorbed.

Not absorb-wholesale (license forbids it, and the Nim shells-out-to-`zip.exe` implementation is worse than what JWLManager already has). Not supersede (jwlFusion is upstream's CLI release channel for jwlCore; JWLManager does not own it). Not pure keep-separate either — there are three concrete capabilities worth pulling across, none of which require touching jwlFusion's source.

**The core insight: there is nothing to port.** JWLManager already links the same merge engine, already binds `mergeDatabase`, `getLastResult`, `getCoreVersion`, `setProgressCallback` via MIT-licensed `jwlcore.py`. jwlFusion's differentiators are *argument marshalling and a `seq[string]` loop*, not algorithms. Everything below is reimplementation-from-behavior against a public C ABI — clean-room by construction, since the ABI is already declared in `jwlcore.py` under MIT.

### What to absorb, and cost

| # | Item | Approach | Effort |
|---|---|---|---|
| 1 | **N-way merge** | Loop `merge_databases()` over an ordered list; document last-wins precedence. `MergeDialog` (`res/ui_extras.py:117`) already has `DropList` for drag-drop — wire multi-select `QFileDialog` → ordered list. | **S** — ~half a day. Engine already reentrant (jwlFusion proves it by folding N archives through the same call). |
| 2 | **Fix F2: use jwlCore's native downgrade** | Change `merge_databases(..., False)` → pass the real flag; retire the hand-rolled `downgrade_schema()` (`JWLManager.py:1172`). | **S–M** — ~1 day + differential testing. Deletes code, removes a drift surface. Verify byte-equality against current output before cutting over. |
| 3 | **Fix F4: arch-aware lib loading** | `platform.machine()` in `_platform_lib_name`; add `jwlCore-arm64.dll` to `libs/`. | **S** — ~1 hour. Unlocks the already-shipped `libjwlCore-arm64.so` + Windows arm64. |
| 4 | **Headless CLI mode** | Blocked by the god-object (`class Window` per ARCHITECTURE.md anti-patterns). Wants merge extracted to a UI-free service first. | **L** — deliberately deferred; see below. |

**Total for items 1–3: ~2 days.** High value, low risk, no license exposure.

**Item 4 is where "unified app" gets expensive** — and the honest answer is: **don't chase it as a jwlFusion-absorption story.** The blocker is not jwlFusion; it is that JWLManager's merge logic is welded into a ~4000-line `Window` class (ARCHITECTURE.md: "any future GUI framework swap or headless/CLI mode would require rewriting nearly everything"). A CLI mode only becomes cheap *after* the service-layer extraction the architecture map already prescribes for its own reasons. And even then, a user who wants a scriptable merge can just run jwlFusion — a 700 KB static binary with no Python runtime. **Duplicating that inside JWLManager buys little.** Recommend keep-separate for the CLI, absorb items 1–3.

### On "unified app"

**Recommend against.** They already share the only hard part. Merging them would mean either (a) shipping a Python+PySide6+polars runtime to users who want a one-shot CLI merge — a large regression in the exact use case jwlFusion serves; or (b) relicensing noncommercial code into MIT, which is not ours to do. The current split — MIT GUI + noncommercial CLI, both over a shared closed core — is a coherent design by the author, not an accident to be cleaned up. **The right unification already happened at the `jwlCore` layer.**

---

## Reusable assets for a Tauri rewrite

**The single most important asset is the `jwlCore` C ABI itself — not any file in either repo.**

A Tauri/Rust rewrite should bind `jwlCore` directly via `libloading`/`extern "C"`, reusing the *contract* (already documented under MIT in `jwlcore.py`), and inherit the merge engine for free. Merge is the hardest feature in the product and it is **already a solved, prebuilt dependency**. This substantially de-risks a rewrite: the port is UI + query + export, never merge.

| Asset | Where | Lift | Value |
|---|---|---|---|
| **jwlCore C ABI** (`mergeDatabase(c_char_p, c_char_p, c_bool) -> c_int`, `getLastResult() -> c_char_p`, `getCoreVersion() -> c_char_p`, `setProgressCallback(fn(c_int))`, `getZuluTime() -> c_char_p`) | declared in `jwlmanager/jwlcore.py` (MIT); `getZuluTime` additionally evidenced in `jwlFusion/src/jwlFusion.nim` | Rust `extern "C"` + `libloading::Library` | **Critical.** Merge engine for free. |
| **Platform lib-name resolution** — *with the F4 bug fixed* | `jwlcore.py:_platform_lib_name` / `_load_lib` (incl. `_MEIPASS` handling — drop that for Tauri) | Rewrite in Rust using `cfg!(target_arch)` — compile-time, no runtime sniffing, kills F4 by construction | High |
| **Progress-callback pattern** | `jwlcore.py` `CALLBACKTYPE`; consumed at `JWLManager.py:2668` (`py_progress`) | Rust `extern "C" fn` → Tauri event → frontend progress bar | Medium |
| **Merge-fold algorithm** (ordered N-way, bail-on-nonzero, last-wins) | *behavior* of `jwlFusion.nim:main`; reimplement, do not copy | ~30 lines of Rust | Medium — trivial to write, valuable to know is safe |
| **Schema gate `> 11` + output 16 / downgrade 14** | `JWLManager.py:1003`, `:989`, `:1810`; corroborated in `jwlFusion.nim:unzipArchive`/`createArchive` | Constants | Medium — two independent sources agree, so treat as the real contract |
| **Manifest rewrite recipe** (`name`, `creationDate`, `deviceName`, `lastModifiedDate`, `hash` = sha256 of `userData.db`, `databaseName`, `schemaVersion`) | `JWLManager.py:989`; corroborated by `jwlFusion.nim:createArchive` | `serde_json` + `sha2` | High — cross-validated by two implementations |
| **`res/resources.db`, `res/blank`, `res/blank_playlist`** | jwlmanager `res/` | Copy as-is | High — data, not code; no rewrite needed |
| **`res/locales/` (9 langs)** | jwlmanager | gettext → i18n framework | Medium |

**Explicitly do NOT lift from jwlFusion:**
- `execShellCmd`-based `zip`/`unzip`/`mkdir`/`rmdir` — shell injection on odd paths, drags `zip.exe`/`unzip.exe`/`bzip2.dll` into the bundle. Rust `zip` crate is strictly better, and JWLManager's stdlib `zipfile` already is.
- Any literal Nim source — **Infiniti Noncommercial v1.2 forbids derivative works.** Reimplement from the ABI + observed behavior only.

---

## Summary

1. **jwlFusion duplicates nothing that matters** — it is a ~300-line Nim CLI over the byte-identical `jwlCore` blob JWLManager already links. Merge logic is in neither repo.
2. **Real complement:** N-way ordered merge, headless operation, Windows arm64.
3. **Real conflict:** downgrade authority is split (jwlCore native vs JWLManager's Python `downgrade_schema`) and can silently diverge; plus an incompatible license that blocks code reuse (but not capability reuse).
4. **Bug surfaced:** `jwlcore.py:_platform_lib_name` is arch-blind — shipped `libjwlCore-arm64.so` is unloadable.
5. **Do:** absorb N-way merge + native downgrade + arch-aware loading (~2 days). **Don't:** unify the apps or port the Nim.
6. **For a Tauri rewrite:** bind `jwlCore` directly. Merge — the hardest feature — is a prebuilt dependency, not a port.
