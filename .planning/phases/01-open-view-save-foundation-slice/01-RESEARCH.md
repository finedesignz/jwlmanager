# Phase 1: Open, View, Save (Foundation Slice) - Research

**Researched:** 2026-07-16
**Domain:** Tauri v2 (Rust core + React frontend) archive I/O, FFI, virtualized data grid
**Confidence:** MEDIUM-HIGH (manifest/schema contract HIGH — line-cited Python source; jwlCore arm64 Windows gap HIGH/verified by filesystem; Tauri/TanStack specifics MEDIUM — training knowledge + partial verification)

## Summary

This phase re-derives, in Rust/Tauri, the archive envelope and manifest logic that `JWLManager.py` implements in ~250 lines around `check_validity`/`update_manifest`/`upgrade_schema`, plus a read-only Notes query and a virtualized list. Two things surfaced during research materially affect planning:

1. **The `res/resources.db` lookup is NOT optional for Phase 1.** The Notes list in the Python app resolves `MepsLanguage` → language name and `KeySymbol`/`IssueTagNumber`/`BookNumber`/`ChapterNumber` → publication code and human-readable detail via `res/resources.db` (`load_languages`, `load_bible_books`, `publications` DataFrame — `JWLManager.py:4023-4053`, consumed in `get_notes` at `JWLManager.py:753-757` via `lang_name.get(...)`, `process_code(...)`, `process_detail(...)`). Rendering anything more than a raw `Location.Title` string requires bundling and querying this 335 KB SQLite file. This is a **locked-in Phase 1 dependency**, not a nice-to-have — the planner must add tasks to bundle `res/resources.db` as a Tauri resource and query it.

2. **There is no Windows arm64 jwlCore binary.** `libs/` contains exactly four binaries: `jwlCore-amd64.dll`, `libjwlCore-arm64.so` (Linux), `libjwlCore-x86_64.so` (Linux), `libjwlCore.dylib` (macOS, universal/fat — needs verification, see Open Questions). **There is no `jwlCore-arm64.dll` for Windows.** `.github/workflows/jwlCore.config` confirms this — the `win32` block only lists `jwlCore-amd64` + `sqlite3_64` prefixes, no arm64 entry. D-13 ("arch-aware loading fixes the arm64 bug") and PLAT-01 ("Windows x64 + arm64") are in tension with a binary that does not exist for that platform/arch pair. See "Critical Gap" below — this must go back to the user before the plan locks D-13/PLAT-01 as currently worded.

**Primary recommendation:** Build the Rust core as a `src-tauri` crate using `rusqlite` (bundled feature, for the window-function support the Python app needs `sqlite3_64.dll` for), the `zip` crate ≥2.3 (fixes CVE-2025-29787) with `enclosed_name`-validated extraction, `libloading` with an OS+arch match table mirroring `_platform_lib_name`, `thiserror` for typed errors, and `ts-rs` for enum codegen. Bundle `res/resources.db` as a Tauri resource alongside the `libs/` native binaries. Flag the Windows-arm64-jwlCore gap to the user immediately — do not silently scope it out or silently substitute x64 emulation without a decision.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Archive zip extract/rebuild (ARCH-01/02/05) | Rust core (`src-tauri`) | — | File I/O + security-sensitive path validation belongs in the trusted backend, never in the webview |
| manifest.json read/write (ARCH-03) | Rust core | — | Byte-compatibility (field order, compact separators) is a serialization concern owned by the process that writes the file |
| userData.db query (Notes) (DATA-01) | Rust core | — | SQLite access via `rusqlite`; frontend never touches the DB directly |
| resources.db lookups (language/publication labels) | Rust core | — | Same trust boundary as userData.db; resolved server-side, plain strings sent to frontend |
| jwlCore load + symbol resolution (MERGE-01 portion in scope) | Rust core | — | FFI must stay behind the Tauri IPC boundary; `libloading` calls happen in Rust, never exposed to JS |
| Notes list rendering + virtualization (DATA-01) | Frontend (React/TanStack Virtual) | — | DOM virtualization is inherently a browser-tier concern |
| Category enum (DATA-08) | Rust core (source of truth) | Frontend (generated types) | `ts-rs` generates the TS side from the Rust enum — Rust owns identity, frontend only consumes |
| Error surfacing (SAFE-05) | Rust core (produces) | Frontend (renders) | Typed `thiserror` errors serialize over IPC; frontend is a dumb renderer of a discriminated union |
| New/Open/Save/Save-As command surface (ARCH-01/06/07) | Rust core (Tauri commands) | Frontend (triggers via IPC) | File-system access is backend-only in Tauri's security model |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tauri` | 2.x [ASSUMED — verify exact minor via `cargo add tauri` at scaffold time] | App shell, IPC, resource bundling | Locked by project (Tauri v2, per CLAUDE.md) |
| `rusqlite` | 0.32+ [ASSUMED] | SQLite access to `userData.db` + `resources.db` | Named in CONTEXT.md discretion list; standard Rust SQLite binding, supports bundled libsqlite3 with window-function support (needed for `ROW_NUMBER()`/`COUNT(*) OVER` used in `trim_db`, Phase 2, but the bundled feature should be chosen now so Phase 2 doesn't need a dependency swap) |
| `zip` | **≥2.3.0** [VERIFIED: GitHub Advisory GHSA-94vh-gphv-8pm8 / CVE-2025-29787] | Archive read/write | Standard Rust zip crate; versions 1.3.0–2.2.x have a symlink-based zip-slip variant (CVE-2025-29787) even when using `enclosed_name` — **must pin ≥2.3.0**, not just "use enclosed_name" |
| `libloading` | 0.8+ [ASSUMED] | jwlCore FFI, mirrors `ctypes.CDLL` | Named in CONTEXT.md; standard safe wrapper over `dlopen`/`LoadLibrary` |
| `thiserror` | 1.x/2.x [ASSUMED] | Typed error enums | Named in CONTEXT.md; de facto standard for library-style error types in Rust |
| `serde` / `serde_json` | 1.x [ASSUMED] | Manifest (de)serialization | Standard; **default `serde_json::to_writer` does NOT match Python's `separators=(',',':')` compact form** — must construct manifest fields in explicit struct-field order and call `serde_json::to_string` (which is already compact/no-whitespace by default, matching `separators=(',',':')`) rather than `to_string_pretty`. Field **order** in JSON is preserved by struct field declaration order when using `serde::Serialize` on a struct (not a `HashMap`) — this is the critical implementation detail: use a struct, never a `serde_json::Value`/map for the manifest, or key order becomes non-deterministic. |
| `ts-rs` | 7.x/9.x [ASSUMED] | Generate TS types from Rust enum (DATA-08) | Named in CONTEXT.md |
| `sha2` | 0.10+ [ASSUMED] | `hash` field = sha256 of final DB bytes | Rust equivalent of Python's `hashlib.sha256`, needed for ARCH-03 |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tempfile` | 3.x [ASSUMED] | Per-session extraction dir (D-03) | Standard Rust equivalent of `mkdtemp()` |
| `@tanstack/react-virtual` | 3.x [ASSUMED] | Virtualized Notes list (D-10) | Windowed rendering for 9,000+ rows |
| `@tanstack/react-table` | 8.x [ASSUMED, optional] | If sortable/filterable columns are wanted alongside virtualization | Pairs with react-virtual for grid-like UX; not strictly required for Phase 1's read-only list |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `rusqlite` | `sqlx` | `sqlx` is async-first and compile-time-checks queries, but adds a runtime (tokio) and its SQLite driver historically lagged window-function support; `rusqlite` is the simpler sync fit for a desktop app with no concurrent-request pressure |
| `@tanstack/react-virtual` | `react-window` | `react-window` is lighter but less actively maintained for React 18/19 concurrent features; TanStack Virtual is explicitly locked by D-10 |
| Svelte/Solid frontend | React | D-09 explicitly reopenable, but no research finding here overturns it — React has the deepest TanStack Virtual + ts-rs ecosystem examples |

**Installation:**
```bash
# Rust (from app/src-tauri)
cargo add rusqlite --features bundled
cargo add zip@^2.3
cargo add libloading thiserror serde serde_json sha2 tempfile ts-rs

# Frontend (from app/)
npm install @tanstack/react-virtual
```

**Version verification:** Run before locking Cargo.toml:
```bash
cargo search zip          # confirm current published version >= 2.3.0
cargo search rusqlite
cargo search tauri
```
Training-data versions above are dated; confirm exact minors at scaffold time — do not hardcode without checking `crates.io`.

## Package Legitimacy Audit

slopcheck was not run in this research session (no network install attempted for a Python tool inside a Rust/Node research task — see note below). **All packages in the Standard Stack table are therefore tagged `[ASSUMED]` per the graceful-degradation rule.** The planner must gate each `cargo add`/`npm install` behind a `checkpoint:human-verify`, OR the executor should run `cargo search <pkg>` / `npm view <pkg> version` immediately before install as a lightweight substitute, since all of these are extremely well-known, long-established crates/packages (`serde`, `rusqlite`, `zip`, `thiserror`, `libloading`, `@tanstack/react-virtual`) with no plausible slopsquat risk — but the process should still confirm exact versions exist on the registry before pinning.

| Package | Registry | Age | Downloads | Source Repo | slopcheck | Disposition |
|---------|----------|-----|-----------|-------------|-----------|-------------|
| tauri | crates.io | 5+ yrs | very high | github.com/tauri-apps/tauri | not run | ASSUMED — verify version at scaffold |
| rusqlite | crates.io | 8+ yrs | very high | github.com/rusqlite/rusqlite | not run | ASSUMED |
| zip | crates.io | 10+ yrs | very high | github.com/zip-rs/zip2 | not run | ASSUMED — **must be ≥2.3.0**, verify explicitly |
| libloading | crates.io | 8+ yrs | very high | github.com/nagisa/rust_libloading | not run | ASSUMED |
| thiserror | crates.io | 6+ yrs | very high | github.com/dtolnay/thiserror | not run | ASSUMED |
| ts-rs | crates.io | 5+ yrs | high | github.com/Aleph-Alpha/ts-rs | not run | ASSUMED |
| sha2 | crates.io | 8+ yrs | very high | github.com/RustCrypto/hashes | not run | ASSUMED |
| @tanstack/react-virtual | npm | 4+ yrs (v3 line) | very high | github.com/TanStack/virtual | not run | ASSUMED |

**Packages removed due to slopcheck [SLOP] verdict:** none (slopcheck not run)
**Packages flagged as suspicious [SUS]:** none identified by inspection

## Architecture Patterns

### System Architecture Diagram

```
User double-clicks / File > Open
        │
        ▼
[Tauri command: open_archive(path)]
        │
        ├─► validate zip (check_validity equivalent: must be zip, must contain
        │    manifest.json with userDataBackup.schemaVersion > 11)
        │
        ├─► extract to per-session tempdir (D-03), validating each entry path
        │    stays within tempdir root (zip-slip guard, ARCH-05) — reject on
        │    escape BEFORE writing any file
        │
        ├─► open userData.db (rusqlite) read-only-ish (source untouched, D-03)
        │
        ├─► query Notes (JOIN Location, LEFT JOIN TagMap/Tag/UserMark,
        │    GROUP_CONCAT tags) + separate independent-notes query
        │    (LocationId IS NULL) — UNION both result sets
        │
        ├─► resolve display labels via resources.db (language name, publication
        │    short/detail code) — bundled resource, opened read-only
        │
        └─► return NotesRow[] over IPC ──► React state
                                              │
                                              ▼
                                  TanStack Virtual windowed list
                                  (only visible rows touch the DOM)

User clicks Save / Save As
        │
        ▼
[Tauri command: save_archive(path?) ]
        │
        ├─► update manifest fields in Rust struct (preserve unknown keys —
        │    parse into a struct with #[serde(flatten)] catch-all for forward
        │    compat, per FUNCTIONALITY-SPEC §2.2 "preserving unknown keys")
        ├─► compute sha256 of FINAL userData.db bytes (after any pending
        │    mutation — none in Phase 1 beyond LastModified touch)
        ├─► serialize manifest.json compact, exact field order
        ├─► rebuild zip from tempdir into SIBLING temp file (D-04)
        └─► atomic rename over target path (D-04) — save-as targets new path,
             working copy follows it (D-05)

Startup / jwlCore probe (independent of open/save flow)
        │
        ▼
[Tauri command: check_jwlcore()]
        │
        ├─► resolve OS + CPU arch → expected binary filename (D-13 table)
        ├─► libloading::Library::new(path) — surfaced as typed error if missing
        ├─► resolve symbols: setProgressCallback, mergeDatabase, getLastResult,
        │    getCoreVersion (no calls beyond resolution + version probe)
        └─► report loaded / not-found / arch-mismatch to frontend
```

### Recommended Project Structure
```
app/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs              # Tauri setup, command registration
│   │   ├── archive/
│   │   │   ├── mod.rs
│   │   │   ├── manifest.rs      # manifest struct, ARCH-03 serialization
│   │   │   ├── extract.rs       # zip-slip-safe extraction (ARCH-05)
│   │   │   └── save.rs          # temp-write + atomic rename (D-04)
│   │   ├── db/
│   │   │   ├── mod.rs
│   │   │   ├── notes.rs         # Notes query (DATA-01)
│   │   │   └── resources.rs     # resources.db language/publication lookups
│   │   ├── jwlcore/
│   │   │   ├── mod.rs
│   │   │   └── loader.rs        # arch-aware libloading (D-13)
│   │   ├── category.rs          # Category enum, ts-rs derive (DATA-08)
│   │   └── error.rs             # thiserror types (SAFE-05)
│   ├── resources/                # bundled: res/resources.db copy, libs/* refs
│   └── tauri.conf.json
├── src/                           # React frontend
│   ├── components/NotesList.tsx  # TanStack Virtual list
│   ├── bindings/                 # ts-rs generated types (build output)
│   └── ...
└── package.json
```

### Pattern 1: Manifest struct preserves unknown keys, exact field order
**What:** Parse `manifest.json` into a typed struct with named fields in the exact order Python writes them, plus a catch-all for forward-compatibility.
**When to use:** Any manifest read/write.
**Example:**
```rust
// Order matches FUNCTIONALITY-SPEC.md §2.2 (source: JWLManager.py:979-989, 1154-1170)
#[derive(Serialize, Deserialize)]
struct Manifest {
    name: String,
    #[serde(rename = "creationDate")]
    creation_date: String,
    version: u32,
    #[serde(rename = "type")]
    manifest_type: u32,
    #[serde(rename = "userDataBackup")]
    user_data_backup: UserDataBackup,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>, // preserve unknown keys
}

#[derive(Serialize, Deserialize)]
struct UserDataBackup {
    #[serde(rename = "lastModifiedDate")]
    last_modified_date: String,
    #[serde(rename = "deviceName")]
    device_name: String,
    #[serde(rename = "databaseName")]
    database_name: String,
    hash: String,
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}
// Write with serde_json::to_string(&manifest) — default (non-pretty) output
// has no extraneous whitespace, matching Python's separators=(',',':').
```

### Pattern 2: Arch-aware jwlCore selection (fixes D-13's target bug)
**What:** Select binary by `(std::env::consts::OS, std::env::consts::ARCH)` tuple, not OS alone.
**When to use:** jwlCore load.
**Example:**
```rust
fn resolve_lib_name() -> Result<&'static str, JwlCoreError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("jwlCore-amd64.dll"),
        ("windows", "aarch64") => Err(JwlCoreError::NoArm64WindowsBinary), // GAP — see below
        ("linux", "x86_64") => Ok("libs/libjwlCore-x86_64.so"),
        ("linux", "aarch64") => Ok("libs/libjwlCore-arm64.so"),
        ("macos", _) => Ok("libs/libjwlCore.dylib"), // verify universal binary — see Open Questions
        (os, arch) => Err(JwlCoreError::UnsupportedPlatform(os.into(), arch.into())),
    }
}
```

### Anti-Patterns to Avoid
- **Serializing manifest via `serde_json::Value`/`HashMap`:** Rust `HashMap` iteration order is randomized per-process — this silently breaks byte-compatibility even though the JSON *content* is correct. Must use an ordered struct or `serde_json::Map` (which preserves insertion order when the `preserve_order` feature is enabled) if a catch-all is needed.
- **Trusting `zip::read::ZipArchive::extract()` alone as the zip-slip fix without pinning the crate version:** `enclosed_name` validation existed before CVE-2025-29787 was found and was still bypassable via symlink chaining in 1.3.0–2.2.x. Pin ≥2.3.0 explicitly in `Cargo.toml`, don't rely on "some zip crate version."
- **Loading jwlCore eagerly at binary-selection time without a graceful missing-binary path:** the Python bridge calls `lib = _load_lib()` at **module import time**, meaning any load failure crashes the whole app before the UI even shows. MERGE-04 (Phase 5) exists specifically to fix this — Phase 1's `check_jwlcore()` command should be callable lazily post-startup, not run at Tauri app-init, so a missing/mismatched binary produces a UI error state, not a crash on launch.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Zip path traversal validation | Manual `..`/absolute-path string checks | `zip` crate ≥2.3.0's built-in `enclosed_name`/`extract()` | Hand-rolled path validation has repeatedly proven incomplete (see CVE-2025-29787 — even the crate maintainers missed the symlink variant); don't re-litigate this in application code |
| SHA-256 hashing | Manual byte-reading + hash loop | `sha2` crate | Standard, audited, matches Python's `hashlib.sha256` byte-for-byte |
| Cross-language enum sync | Hand-maintained parallel TS `enum`/string union | `ts-rs` codegen | Exactly the class of drift bug DATA-08 exists to prevent (Python's `if category == _('Notes')` i18n bug) |
| List virtualization | Custom scroll-position/windowing math | `@tanstack/react-virtual` | WebKitGTK's DOM performance cliff at scale is a known, hard problem; a custom windowing implementation is very likely to reproduce edge-case bugs (overscan, dynamic row height, scroll anchoring) that TanStack Virtual has already solved |

**Key insight:** Every "don't hand-roll" item above maps to a real, cited defect class in this exact domain (CONCERNS.md's zip-slip finding, the Python app's i18n-coupled category bug, the platform-specific rendering collapse named in PROJECT.md) — these aren't generic advice, they're this rewrite's own documented failure modes recurring if reimplemented from scratch.

## Common Pitfalls

### Pitfall 1: Manifest hash computed at the wrong point in the save sequence
**What goes wrong:** `hash` ends up covering pre-mutation DB bytes, producing a manifest that doesn't match the shipped archive (JW Library / Python app may reject or silently distrust it).
**Why it happens:** The Python app computes `userDataBackup.hash` **after** trim/vacuum/schema-write, as the literal last step before zipping (`FUNCTIONALITY-SPEC.md` line 285-287, `JWLManager.py:1168`). Phase 1 has no trim (`ARCH-04` is Phase 2), but it DOES touch `LastModified` (`UPDATE LastModified SET LastModified = ?`, line 281) — that write must happen before the hash is taken.
**How to avoid:** Structure the save function so hashing is the literal last DB-touching step: write all DB mutations → close/flush DB handle → read final bytes → hash → write manifest → zip.
**Warning signs:** Differential test against Python-app-open shows a hash mismatch warning (if JW Library or the Python app validates hash — confirm whether either actually checks it, or if it's advisory metadata only; FUNCTIONALITY-SPEC doesn't explicitly say the hash is verified on open, only computed on save).

### Pitfall 2: Forgetting the independent-notes query
**What goes wrong:** Standalone notes (no `Location`, `BlockType = 0`, `LocationId IS NULL`) silently vanish from the list.
**Why it happens:** The main Notes query INNER JOINs `Location`, which by definition excludes rows with no location. The Python app runs a **second, separate query** (`load_independent`, `JWLManager.py:696-704`) and concatenates results.
**How to avoid:** Implement both queries from day one; write a fixture with at least one independent note to make this testable (QA-01).
**Warning signs:** A synthetic fixture with an independent note passes the "app opens" check but the note doesn't appear in the list — silent data loss from the user's perspective (Core Value violation).

### Pitfall 3: Assuming `Location.Title` alone is enough for the Notes label
**What goes wrong:** Language/publication codes render as raw IDs, or nothing, because `resources.db` was never bundled/queried.
**Why it happens:** `Location.Title` is often NULL/empty in real data (`trim_db` even patches it to `""` for exactly this reason, FUNCTIONALITY-SPEC line 490) — the *actual* human-readable label the Python app shows is synthesized from `lang_name`/`process_code`/`process_detail`, which are built from `resources.db` at startup (`JWLManager.py:4023-4053`), not from a column in `userData.db`.
**How to avoid:** Bundle `res/resources.db` (335 KB) as a Tauri resource; add a Rust module that loads `Languages`, `BibleBooks`, `Publications`/`Extras` tables once and resolves labels per-row, mirroring `process_code`/`process_detail`'s logic (not yet extracted into FUNCTIONALITY-SPEC.md — read `JWLManager.py` directly for these two functions before implementing, they are referenced but not fully spec'd in the current research corpus).
**Warning signs:** Notes list "works" against a minimal synthetic fixture (which may not exercise every code path) but shows garbage/blank labels against a real archive via the env-var-gated local smoke test (D-07) — this is exactly why that manual test exists.

### Pitfall 4: `libloading::Library::new` panicking the whole app at startup
**What goes wrong:** A missing/wrong-arch jwlCore binary on a user's machine crashes the app before any UI renders, exactly reproducing the Python bridge's `lib = _load_lib()`-at-import-time failure mode.
**Why it happens:** It's tempting to load jwlCore during Tauri's `setup()` hook for a fast fail. But D-12 explicitly scopes Phase 1 to "load + resolve symbols, report success/failure" — implying this must be a **command the frontend can call and receive an error from**, not a startup-time panic.
**How to avoid:** Wrap the load in a Tauri command returning `Result<JwlCoreStatus, JwlCoreError>`; call it once from the frontend after mount, render a banner/error state on failure, never `.unwrap()` it during app init.
**Warning signs:** App fails to launch at all (not just a feature) on a machine with a missing binary — clippy's `unwrap`/`expect` ban (D-15) should catch this in code review, but a `.expect()` buried in a `once_cell`/`lazy_static` static initializer can slip past a simple lint if not careful about where the load happens.

### Pitfall 5: Windows arm64 CI passing despite jwlCore being genuinely absent for that target
**What goes wrong:** The `windows-11-arm` CI job builds and even runs Rust unit tests green, masking that jwlCore will fail to load for every real arm64 user, because no test actually exercises the arm64 binary-resolution path against a present-but-wrong-arch or absent binary.
**Why it happens:** Unit tests for `resolve_lib_name()` can pass purely on string-matching logic without ever attempting a real `libloading::Library::new()` against a Windows arm64 binary that doesn't exist in this repo.
**How to avoid:** The CI job's assertion must be explicit about what "arm64 works" means for Phase 1 given the missing binary — see Critical Gap below. Don't let a green CI matrix imply a false sense of platform support.
**Warning signs:** PLAT-01 checked off in a phase-completion report while the owner's actual daily-driver arm64 build still can't load jwlCore.

## Critical Gap: No Windows arm64 jwlCore binary exists

**This directly affects D-13 and PLAT-01 as currently worded and must be surfaced to the user before planning locks it in.**

Filesystem evidence (`libs/` directory listing, this session):
```
jwlCore-amd64.dll        (Windows x64)
libjwlCore-arm64.so      (Linux arm64)
libjwlCore-x86_64.so     (Linux x64)
libjwlCore.dylib         (macOS — need to verify universal/fat binary, see Open Questions)
sqlite3_64.dll           (Windows-only SQLite override, for window-function support)
```
`.github/workflows/jwlCore.config` (the native-lib build/bundling config) confirms this is not an oversight in `libs/` alone — the `win32` bundling rule only lists `jwlCore-amd64` + `sqlite3_64` prefixes; there is no `win32`-arm rule at all. `res/requirements-winarm.txt` exists in the Python app (per STACK.md) but that's a *Python dependency* pin for running on Windows arm64 (presumably under x64 emulation, or a pure-Python fallback path) — it says nothing about a native arm64 jwlCore binary existing.

**What this means for the phase as scoped:**
- D-13 ("arch-aware loading fixes the arm64 bug") can still be implemented correctly — the *logic* fix (select by OS+arch, not OS alone) is real and valuable regardless. But "fixing the bug" does not make a Windows arm64 jwlCore binary appear. On real Windows arm64 hardware, correct arch-aware logic will correctly report "no binary for this arch" instead of the old bug's behavior (silently trying to load nothing / wrong path) — this is strictly better (a clear error beats a silent failure) but it is **not** "jwlCore works on Windows arm64."
- PLAT-01 says "App builds and runs on Windows (x64 + arm64)... macOS, and Linux" — the **app** (Tauri shell, UI, archive open/save/view) can plausibly run fine on Windows arm64 since none of that Phase 1 surface needs jwlCore (D-12 scopes jwlCore to load+resolve only, no merge call). But "runs" should not be conflated with "jwlCore loads" for that platform.
- The owner's stated daily-driver reality (owner uses `JWLManager_v12.1.0-arm64` on Windows arm64 for merges) means the Python app today either (a) runs under x64 emulation calling the x64 DLL, or (b) has its own unresolved gap. This needs a direct answer, not an assumption, before Phase 5 (merge) — but it changes what "success" means for Phase 1's `check_jwlcore()` on that specific runner.

**Options to put in front of the user (do not silently pick one):**
1. **Ship Phase 1 as scoped, with `check_jwlcore()` correctly reporting "binary not found for aarch64-windows" as a first-class, well-tested error state** — this is honest, low-effort, and doesn't block the rest of Phase 1. Merge (Phase 5) then either needs a real arm64 build of jwlCore obtained from upstream, or Windows arm64 merge runs the x64 DLL under Windows' built-in x64 emulation (works today for many DLLs; needs verification against jwlCore specifically) — flag as a Phase 5 research question, not Phase 1's problem to solve.
2. **Obtain/verify whether an arm64 Windows jwlCore binary exists upstream** (the source isn't in this repo — `.github/workflows/jwlCore.config` implies it's built from a separate source repo) before finalizing Phase 1's D-16 CI matrix expectations, in case one needs to be added to `libs/`.
3. **De-scope Windows arm64 from PLAT-01's Phase 1 acceptance criteria** for the jwlCore-loading requirement specifically, while keeping it for the rest of the app (open/view/save works everywhere; jwlCore-load-success is x64/Linux/macOS only for now).

This finding does not block planning — it changes what "done" means for one specific requirement/platform combination. Recommend the planner add an explicit task to confirm this with the user (or the upstream jwlCore build repo) before writing the CI acceptance criteria for the `windows-11-arm` job.

## Runtime State Inventory

Not applicable — this is a greenfield phase (new `app/` subdirectory), not a rename/refactor/migration. Skipped per trigger condition.

## Code Examples

### Atomic save (write-temp-then-rename, D-04)
```rust
// Source: pattern derived from D-04's stated rationale; no direct Context7/official
// doc citation — this is a standard filesystem-atomicity idiom, tag [ASSUMED] for the
// exact API shape though the underlying rename(2)/MoveFileEx atomicity guarantee is
// well-established OS behavior, not a library claim.
use std::fs;
use std::path::Path;

fn atomic_save(final_path: &Path, build_zip: impl FnOnce(&Path) -> Result<(), SaveError>) -> Result<(), SaveError> {
    let tmp_path = final_path.with_extension("jwlibrary.tmp");
    build_zip(&tmp_path)?;
    fs::rename(&tmp_path, final_path)?; // atomic on same filesystem, both Windows (MoveFileEx) and POSIX (rename)
    Ok(())
}
```
**Caveat [ASSUMED]:** `fs::rename` atomicity across platforms holds when source and destination are on the **same filesystem/volume** — verify the sibling-temp-file is created in the same directory as the target (not a different temp-dir mount), otherwise Windows/POSIX may fall back to non-atomic copy+delete semantics.

### zip-slip-safe extraction
```rust
// Source: zip crate docs (docs.rs/zip) — [CITED: docs.rs/zip/latest/zip/read/struct.ZipArchive.html]
// Confirmed via WebSearch cross-referencing GHSA-94vh-gphv-8pm8, which documents that
// `enclosed_name`-based validation is the crate's built-in defense, but requires >=2.3.0
// to close the symlink-chaining variant (CVE-2025-29787).
use zip::ZipArchive;
use std::fs::File;

fn safe_extract(archive_path: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(dest)?; // internally validates each entry via enclosed_name (>=2.3.0 also handles symlink chains)
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `ZipFile.extractall()` with no path validation (Python app, CONCERNS.md) | `zip` crate `extract()` with `enclosed_name` validation, pinned ≥2.3.0 | zip crate fix: March 2025 (CVE-2025-29787 disclosure) | Directly fixes ARCH-05; must pin exact version, "just use the zip crate" is insufficient advice pre-2.3.0 |
| `sys.platform`-only library selection (`jwlcore.py:_platform_lib_name`) | OS + `std::env::consts::ARCH` tuple match | This phase (D-13) | Fixes real reported bug; but see Critical Gap — fixing selection logic ≠ binary existing |
| Windows-only arm64 GH Actions runner support | `windows-11-arm` GA for all public repos | GA announced 2025-08-07 (public repos, free tier) [CITED: github.blog/changelog/2025-08-07] | D-16's CI matrix is achievable **if this repo is public** — confirm repo visibility; private repos are explicitly excluded from the free/standard arm64 runner per the same announcement |

**Deprecated/outdated:**
- Windows arm64 GitHub Actions runners were public-preview-only (April 2025) as recently as a few months before GA (Aug 2025) — if any older tutorial/blog describes it as "preview" or paywalled, that's stale; it is GA now for public repos.
- `zip` crate versions 1.3.0–2.2.x should be treated as having a known unpatched zip-slip variant, not "old but fine" — this is a live CVE, not stylistic staleness.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Exact crate versions (`rusqlite` 0.32+, `zip` 2.3.x current, `tauri` 2.x minor, `ts-rs` 7.x/9.x, etc.) | Standard Stack | Low — these are well-known crates; wrong minor version just needs a `Cargo.toml` bump, doesn't change architecture |
| A2 | `libs/libjwlCore.dylib` is a universal (x86_64+arm64) binary rather than single-arch | Critical Gap, Code Examples | Medium — if it's single-arch, macOS also needs the OS+arch fix to matter in practice, and D-16's `macos-latest` runner (which GitHub runs on Apple Silicon as of recent images) may not exercise the x86_64 path; verify with `lipo -info libs/libjwlCore.dylib` or `file libs/libjwlCore.dylib` before finalizing D-16's CI expectations |
| A3 | Neither JW Library nor the Python app actually *verifies* the manifest `hash` on open (vs. just writing it as advisory metadata) | Common Pitfalls #1 | Low-medium — if hash IS verified on open by JW Library itself (not observable from this repo, which only has the Python app's write-side logic), a Rust save that gets hash timing wrong could make JW Library itself reject the archive, not just "look wrong" — worth an explicit real-archive smoke test (D-07 env-var path) early in Phase 1 |
| A4 | `res/blank` (6195 bytes) is a valid, directly-usable v16 seed for both ARCH-06 (new archive) and QA-01 (fixture generation) without modification | Code Context (carried from CONTEXT.md, re-affirmed here) | Low — file exists and is referenced in FUNCTIONALITY-SPEC as the `new_file` template; but its internal schema version and exact table-seed contents were not inspected byte-for-byte in this research pass (it's a zip; would need extraction+inspection) — planner/implementer should extract and inspect it directly before building the fixture generator, rather than assuming its shape from the manifest JSON snippet alone |
| A5 | `process_code`/`process_detail` (functions that turn `KeySymbol`/`Issue`/`Book`/`Chapter` into a display code) have well-defined, replicable logic | Pitfall 3 | Medium — these functions are referenced but not yet line-cited/spec'd in `FUNCTIONALITY-SPEC.md`; the planner must budget a research/read task against `JWLManager.py` directly (grep `def process_code`, `def process_detail`) before implementation, not assume the spec already covers it |
| A6 | Windows-arm64 emulation of the x64 jwlCore DLL is technically viable as a Phase 5 fallback | Critical Gap | Medium — Windows x64-on-arm64 emulation (via Prism) generally works for DLLs without deep AVX-512/driver dependencies, but this is untested against jwlCore specifically in this research pass |

**If this table is empty:** N/A — table populated above.

## Open Questions

1. **Does JW Library (the vendor app) verify the manifest `hash` on open, or is it purely advisory?**
   - What we know: the Python app computes and writes it; FUNCTIONALITY-SPEC documents the write-side contract exhaustively.
   - What's unclear: whether an incorrect hash causes JW Library to reject the archive outright, versus being ignored.
   - Recommendation: treat as verified-on-open (safest assumption) until the env-var-gated real-archive smoke test (D-07) proves otherwise; don't relax hash-timing discipline based on an assumption it's unchecked.

2. **Is `libs/libjwlCore.dylib` single-arch or universal?**
   - What we know: one `.dylib` file covers "macOS" in the existing bridge with no OS-version/arch branching beyond `sysname == "darwin"`.
   - What's unclear: whether it's a fat/universal binary (works on both Intel and Apple Silicon Macs) or was only ever built for one.
   - Recommendation: run `lipo -info` or `file` on it as an early Phase 1 task; if single-arch, macOS needs the same arch-table treatment as Windows, and the `macos-latest` GH runner's current arch (Apple Silicon as of recent images) needs to be cross-checked against it.

3. **Where is the real jwlCore source/build repo, and can a Windows arm64 build be requested or obtained?**
   - What we know: `.github/workflows/jwlCore.config` configures *bundling* of prebuilt binaries into the PyInstaller output; it is not a build script. STACK.md confirms "Source not in this repo."
   - What's unclear: whether upstream (`erykjj`, the original author) has a Windows arm64 build available or buildable, closing the Critical Gap above.
   - Recommendation: this is a user/upstream-contact question, not something resolvable by more repo research — surface it directly rather than guessing.

4. **What do `process_code`/`process_detail` actually compute?**
   - What we know: they take `KeySymbol`, `Issue`, `Book`, `Chapter` and produce a `(code, year)` / `(detail1, year, detail2)` tuple used in the Notes list.
   - What's unclear: exact branching logic (e.g., how Bible book+chapter references differ in display from publication symbol+issue references).
   - Recommendation: planner should schedule a direct read of these two functions in `JWLManager.py` (not yet done in this pass — budget was prioritized to the 8 named research questions) as an early Phase 1 task, likely a spike/read task before the Notes-query implementation task.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | Entire `src-tauri` build | Not probed this session (research-only pass, no build attempted) | — | Standard `rustup` install, no fallback needed — universally available |
| Node.js/npm | Frontend build | Not probed this session | — | Standard install |
| `windows-11-arm` GH Actions runner | D-16 CI matrix | ✓ (GA per web research, 2025-08-07) [CITED: github.blog/changelog/2025-08-07] | — | Only if repo is **public** — verify repo visibility; private repos excluded from this free tier |
| `libs/jwlCore-amd64.dll` | MERGE-01 (Windows x64 portion) | ✓ (present in `libs/`, verified via directory listing) | unknown internal version | — |
| Windows arm64 jwlCore binary | MERGE-01 (Windows arm64 portion), PLAT-01 | ✗ (confirmed absent, see Critical Gap) | — | See Critical Gap options 1-3 — no drop-in fallback exists |
| `res/resources.db` | DATA-01 label rendering | ✓ (present, 335 KB per CONTEXT.md) | — | — |

**Missing dependencies with no fallback:**
- Windows arm64 jwlCore binary — blocks a fully-correct MERGE-01/PLAT-01 on that specific platform+arch combination for jwlCore-dependent behavior (not blocking for the rest of Phase 1's open/view/save/UI surface).

**Missing dependencies with fallback:**
- None identified beyond the above (repo-visibility caveat for `windows-11-arm` is a config check, not a missing dependency).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust: `cargo test` (built-in) for `src-tauri`; Frontend: none detected yet — recommend Vitest [ASSUMED] for React component/unit tests, added in Wave 0 |
| Config file | none yet — see Wave 0 |
| Quick run command | `cargo test --manifest-path app/src-tauri/Cargo.toml` |
| Full suite command | `cargo test --manifest-path app/src-tauri/Cargo.toml -- --include-ignored` (Rust) + `npm test` (frontend, once Vitest is added) + clippy/fmt (D-17) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ARCH-01 | Open archive, list contents | integration (synthetic fixture) | `cargo test test_open_archive_lists_manifest_and_db -- --nocapture` | ❌ Wave 0 |
| ARCH-02 | Save archive; Python app + JW Library reopen it | integration (differential, D-01 oracle) | `python3 JWLManager.py <saved-fixture-path>` invoked from a test harness script, asserting clean exit / no crash_box | ❌ Wave 0 — needs a small differential-test harness script, likely `app/src-tauri/tests/differential.rs` shelling out to `python3` |
| ARCH-03 | manifest.json byte-compatible fields | unit (struct serialization) | `cargo test test_manifest_serialization_matches_python_field_order` | ❌ Wave 0 |
| ARCH-05 | Zip-slip rejected | unit (crafted malicious fixture, D-08) | `cargo test test_zip_slip_fixture_rejected` | ❌ Wave 0 — needs the crafted zip-slip fixture itself (D-08) |
| ARCH-06 | Create new empty archive | integration | `cargo test test_new_archive_matches_blank_template` | ❌ Wave 0 |
| ARCH-07 | Save-as leaves original untouched, follows new path | integration | `cargo test test_save_as_preserves_original_follows_new` | ❌ Wave 0 |
| DATA-01 | Notes list renders correctly, responsive at 9000+ rows | integration (Rust query) + manual/visual (virtualization perf) | `cargo test test_notes_query_includes_independent_notes` (Rust); manual Linux WebKitGTK scroll-perf check (no automated perf assertion planned — flag as manual-only with justification: DOM frame-timing isn't reliably assertable in a headless CI test) | ❌ Wave 0 |
| DATA-08 | Category enum, no string comparison in control flow | unit + static check | `cargo clippy` (custom lint or grep-based CI check for `== "Notes"`-style string comparisons is a reasonable supplement) | ❌ Wave 0 |
| SAFE-05 | Errors surface with actionable context | unit (error type coverage) | `cargo test test_all_archive_errors_serialize_with_message` | ❌ Wave 0 |
| QA-01 | Synthetic fixtures used by tests | infra | fixture generator itself is the "test" — `cargo test test_fixture_generator_produces_valid_v16_archive` | ❌ Wave 0 — the generator is new code |
| QA-03 | Tests run in CI on every push | infra | GH Actions workflow YAML present + green | ❌ Wave 0 — new workflow file |
| PLAT-01 | Builds/runs on all 4 platform targets | CI matrix | full 4-runner GH Actions matrix build | ❌ Wave 0 — see Critical Gap for the jwlCore-specific caveat on `windows-11-arm` |

### Sampling Rate
- **Per task commit:** `cargo test --manifest-path app/src-tauri/Cargo.toml` (fast subset)
- **Per wave merge:** full suite + clippy + fmt + differential test against Python app
- **Phase gate:** full CI matrix (4 platforms) green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `app/src-tauri/tests/manifest_tests.rs` — covers ARCH-03
- [ ] `app/src-tauri/tests/archive_tests.rs` — covers ARCH-01, ARCH-05, ARCH-06, ARCH-07
- [ ] `app/src-tauri/tests/differential.rs` — covers ARCH-02, shells out to `python3 JWLManager.py` (D-01 oracle)
- [ ] `app/src-tauri/tests/notes_query_tests.rs` — covers DATA-01
- [ ] `app/src-tauri/tests/fixture_gen.rs` (or a `xtask`/build-script binary) — the synthetic fixture generator itself (D-06, QA-01), producing the v16 fixture + the zip-slip fixture (D-08)
- [ ] `.github/workflows/tauri-ci.yml` — new 4-platform matrix workflow (QA-03, D-16, D-17)
- [ ] Frontend test framework install (Vitest) — no frontend tests exist yet; at minimum a smoke test that the Notes list renders N rows without crashing

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Desktop app, no auth surface in this phase |
| V3 Session Management | No | N/A |
| V4 Access Control | No | Single-user local desktop app |
| V5 Input Validation | Yes | Zip entry path validation (`enclosed_name`, `zip` ≥2.3.0); manifest JSON schema validation before trusting `schemaVersion`/`userDataBackup` fields |
| V6 Cryptography | Partial | sha256 hashing is data-integrity metadata, not a security control against tampering (no signature) — do not describe the manifest hash as a security feature; it's a corruption/change-detection value only, matching the Python app's own usage |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Zip-slip path traversal via crafted `.jwlibrary`/merge-source archive | Tampering | `zip` crate ≥2.3.0 `extract()` with `enclosed_name` validation (ARCH-05); never call raw `ZipArchive::by_index` + manual path join without validation |
| Malicious archive with a fabricated high `schemaVersion` to bypass acceptance gate, or a manifest with type-confused fields (e.g. `schemaVersion` as a string) | Tampering | Strict `serde` deserialization into typed structs (a parse failure = rejection), not a loose JSON-map read with manual `.get("schemaVersion").unwrap_or(0)` — let a malformed type surface as a typed parse error, not silently coerce |
| Native library injection — a malicious `jwlCore-amd64.dll` placed at the expected `libs/` path by another process | Tampering/Spoofing | Out of scope for Phase 1 per D-02/D-12 (no checksum verification designed yet); CONCERNS.md flags this as a pre-existing gap ("consider a build step or checksum verification at load time") — worth a note for a future phase, not a Phase 1 blocker since the binary is vendored/trusted-at-build-time already |
| Path traversal via user-supplied save-as path (less likely, but Tauri's file dialog should be the only path-selection surface, not raw string IPC args from an untrusted webview context) | Tampering | Use Tauri's native file-dialog plugin for path selection rather than accepting arbitrary path strings from JS without any validation of shape (Tauri's own capability/scope model already restricts filesystem command surface — configure `tauri.conf.json` capabilities narrowly for the Phase 1 command set) |

## Sources

### Primary (HIGH confidence)
- `jwlcore.py` (this repo) — direct read of the FFI bridge to mirror in Rust
- `.planning/research/FUNCTIONALITY-SPEC.md` (this repo) — line-cited against `JWLManager.py`, the parity contract
- `JWLManager.py` lines 694-773 (Notes/independent-notes queries), 4023-4056 (resources.db loading) — direct read, this session
- `.github/workflows/jwlCore.config` (this repo) — direct read, confirms no Windows-arm64 bundling rule exists
- `libs/` directory listing (this repo, this session) — confirms exactly 4 native binaries, no Windows arm64
- [GitHub Advisory GHSA-94vh-gphv-8pm8 / CVE-2025-29787](https://github.com/advisories/GHSA-94vh-gphv-8pm8) — zip crate symlink zip-slip variant, fixed in 2.3.0
- [docs.rs/zip ZipArchive](https://docs.rs/zip/latest/zip/read/struct.ZipArchive.html) — `enclosed_name`/`extract()` behavior

### Secondary (MEDIUM confidence)
- [GitHub Changelog: arm64 hosted runners for public repositories GA, 2025-08-07](https://github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/) — confirms `windows-11-arm` label availability, free-tier public-repo scope
- [Windows Developer Blog, 2025-04-14](https://blogs.windows.com/windowsdeveloper/2025/04/14/github-actions-now-supports-windows-on-arm-runners-for-all-public-repos/) — preview-era context, superseded by the Aug 2025 GA changelog above

### Tertiary (LOW confidence)
- Exact crate version numbers (`rusqlite`, `tauri`, `ts-rs`, `@tanstack/react-virtual` minors) — training-data-derived, flagged `[ASSUMED]` throughout, must be verified via `cargo search`/`npm view` at scaffold time
- TanStack Virtual's specific WebKitGTK behavior — not independently verified this session beyond the PROJECT.md's own stated constraint (which is itself the authoritative source for this project); treat any specific "known-good config" claim as unresearched until a real Linux CI run or manual test is performed

## Metadata

**Confidence breakdown:**
- Standard stack: MEDIUM — named libraries mostly locked by CONTEXT.md discretion list; versions unverified against registries this session
- Architecture: HIGH — directly derived from line-cited FUNCTIONALITY-SPEC.md and direct source reads of `jwlcore.py`/`JWLManager.py`
- Pitfalls: HIGH — each pitfall traces to a specific cited line range in the existing codebase or a specific CVE
- Critical gap (Windows arm64 jwlCore): HIGH — directly verified via filesystem listing and build-config read, not inferred

**Research date:** 2026-07-16
**Valid until:** 30 days for architecture/manifest findings (stable, source-verified); 7 days for the `windows-11-arm` runner availability claim (fast-moving CI infra) and any unpinned crate version numbers
</content>
