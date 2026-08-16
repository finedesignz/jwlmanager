# Phase 11: Platform Polish (Signing, Localization, Theme) - Research

**Researched:** 2026-07-26
**Domain:** Tauri v2 Windows Authenticode signing (Azure Trusted Signing), dependency-free React i18n, CSS-token theme switching, first-ever persisted app settings
**Confidence:** HIGH (signing wiring, theme mechanism, settings mechanism) / MEDIUM (i18n string inventory count, exact CI workflow shape)

## Summary

Phase 11 is release engineering plus a small settings layer, not new archive functionality. All four decisions (D11-01..D11-04) are already locked in 11-CONTEXT.md; this research verifies the concrete mechanics against a working sibling reference (`remo-code`) and the actual `jwlmanager` codebase state.

The signing wiring has a proven, copy-adaptable reference: `remo-code/supervisor/tauri/signing/sign.ps1` + `trusted-signing-metadata.json`, invoked from `remo-code/.github/workflows/release-supervisor.yml`. The pattern is: (1) a committed `sign.ps1` + `trusted-signing-metadata.json` that sit inert until invoked, (2) a CI step gated on `vars.ENABLE_MSI_SIGNING == 'true'` that installs the Trusted Signing dlib via NuGet, (3) a CI step that PATCHES `tauri.conf.json` to inject `bundle.windows.signCommand` for that build only (the committed file has NO signCommand — normal/PR/local builds are never signed and never touch Azure), (4) `tauri-action` (or `tauri build`) invoked with the three Azure secrets as env vars, harmless when signing is off because no signCommand means the dlib is never invoked. This exact pattern satisfies the environment constraint from the task brief: CI stays green with zero secrets configured (signing step is skipped entirely, no failure), and once the three repo secrets + `ENABLE_MSI_SIGNING=true` variable are provisioned, flipping them on is a zero-code-change operational action.

D11-02's i18n layer, D11-03's theme mechanism, and D11-04's settings persistence are all confirmed buildable with zero new dependencies against the actual current codebase: `app/package.json` has 5 runtime deps (react/react-dom/tanstack-virtual/tauri-api/plugin-dialog only), `app/src-tauri/Cargo.toml` already has `serde`+`serde_json`, and `app/src/styles.css` defines exactly 8 color tokens consumed uniformly by every component with zero hardcoded color literals found in a repo-wide check.

**Primary recommendation:** Copy the `remo-code` signing pattern nearly verbatim (script + metadata + gated CI injection step), scoped to the single existing `AZURE_TRUSTED_SIGNING account`; build i18n as a plain `Record<string,string>` catalog + React context; add a `[data-theme="light"]` CSS override block reusing the same 9 custom-property names; add a `settings.rs` Tauri command pair backed by `app_handle.path().app_data_dir()` + `std::fs` + existing `serde_json`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Windows Authenticode signing | CI / Build pipeline | Tauri bundler (`signCommand` hook) | Signing must happen inside `tauri build`'s bundling step (Tauri owns MSI/NSIS assembly), invoked by an external script CI wires in; it is not app runtime code at all |
| UI string translation (i18n) | Frontend (React) | — | Pure presentation concern; no backend involvement, no persisted-data implication beyond the locale *choice* |
| Theme switching | Frontend (CSS + tiny React state) | — | CSS custom-property cascade only; zero backend involvement beyond persisting the *choice* |
| Settings persistence (language + theme) | Backend (Rust/Tauri command) | Frontend (context calling `invoke`) | Must live in Rust because `app_data_dir()` is only reliably resolved via `tauri::Manager` in the Rust process; frontend is a thin client of `load_settings`/`save_settings` |
| About surface (version display) | Frontend (React) | Backend (already-exposed `app_version` via existing commands) | Version is already surfaced through existing typed commands (`env!("CARGO_PKG_VERSION")`, 10+ call sites in `lib.rs`); About is a presentation-only consumer, no new backend needed for version itself |

## Standard Stack

### Core
No new runtime libraries. Confirmed zero-dependency feasibility:

| Concern | Mechanism | Why zero-dependency is correct here |
|---------|-----------|--------------------------------------|
| Windows signing | Azure Trusted Signing CLI tooling (NuGet `Microsoft.Trusted.Signing.Client`) invoked from PowerShell in CI, `signtool.exe` from the Windows SDK (preinstalled on `windows-latest` GH runners) | Lives entirely in CI/build tooling, never becomes an app dependency (Cargo or npm) |
| i18n | Hand-rolled `Record<Key, string>` catalog + React `createContext`/`useContext` | `react-i18next`/`formatjs`/etc. cannot be legitimacy-cleared (no live npm registry check available); D11-02 already locks this in CONTEXT |
| Theme | CSS custom properties + `data-theme` attribute selector | App already uses this exclusively (`app/src/styles.css:1-17`); zero JS re-render needed |
| Settings persistence | `serde` + `serde_json` (already Cargo deps) + Rust `std::fs` + `tauri::Manager::path().app_data_dir()` (core Tauri API, not a plugin) | Confirmed: `app_data_dir()` is a method on the `Manager` trait already implemented for `AppHandle`/`App` in Tauri 2 core — no `@tauri-apps/plugin-*` package or extra Cargo crate required |

### Supporting
None required.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled i18n catalog | `react-i18next` / `@formatjs/intl` / `tauri-plugin-localization` | Cannot be legitimacy-cleared in this environment (no live registry access); also adds runtime bundle weight and a config surface for a 20-ish-string catalog that doesn't need pluralization/ICU-message features |
| Custom Rust settings module | `tauri-plugin-store` (official Tauri plugin) | Would be a NEW dependency (blocking legitimacy checkpoint per D11-04); the hand-rolled version is ~40 lines of Rust and reuses already-vetted `serde_json`, so there is no benefit worth the checkpoint |
| CSS-var theme toggle | CSS `@media (prefers-color-scheme)` only (no explicit user toggle) | Does not satisfy ROADMAP criterion 3 ("user can switch theme... and the change applies immediately") — media query alone gives OS-following behavior, not an explicit in-app switch. Both can coexist (`prefers-color-scheme` as the *default* before a saved choice exists), which is worth doing if trivial, but the explicit toggle is the actual requirement |

**Installation:** none — no `npm install` / `cargo add` needed for this phase's own decisions.

**Version verification:** N/A — no new package versions to verify. The one external version pin needed is the NuGet package `Microsoft.Trusted.Signing.Client`; `remo-code`'s CI currently pins `1.0.60` (`.github/workflows/release-supervisor.yml:83`, comment explains a prior `-ExcludeVersion` flag bug — do not reintroduce that flag). Confirm current version at implementation time with `nuget list Microsoft.Trusted.Signing.Client -Source https://api.nuget.org/v3/index.json -AllVersions` if bumping; reusing `1.0.60` (the proven-working pin) is the lower-risk default. [CITED: remo-code/.github/workflows/release-supervisor.yml]

## Package Legitimacy Audit

**Not applicable this phase** — zero new Cargo or npm dependencies are introduced by any of D11-01..D11-04. The only new external tooling reference is the NuGet package `Microsoft.Trusted.Signing.Client`, which is CI-only tooling (never becomes a project dependency, never ships in the app binary or `Cargo.lock`/`package-lock.json`) and is already running in production in the sibling `remo-code` repo under the same Azure account — no legitimacy checkpoint applies to CI-only signing tooling per the project's established pattern.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
Windows Signing (CI-time, not runtime)
----------------------------------------
GitHub tag push (v*.*.* or release-tagged)
        |
   [app-ci.yml or new release-app.yml]
        |
   vars.ENABLE_MSI_SIGNING == 'true' ?
        |--- NO  --> tauri build (no signCommand injected) --> unsigned MSI/NSIS --> release asset
        |--- YES --> install Trusted Signing dlib (NuGet)
                     --> patch tauri.conf.json: bundle.windows.signCommand = "powershell ... sign.ps1 %1"
                     --> tauri build
                            |
                            v
                     Tauri bundler assembles MSI/NSIS
                            |
                            v
                     signCommand invoked with %1 = artifact path  <-- HAPPENS DURING BUNDLING
                            |
                            v
                     sign.ps1 --> signtool.exe + Trusted Signing dlib + Azure creds --> Authenticode-signed artifact
                            |
                            v
                     (no updater .sig step exists in this app yet -- future-proofed ordering)
                            |
                            v
                     signed MSI --> release asset

Runtime Settings + i18n + Theme (app-time)
----------------------------------------
App start
   |
   v
index.html inline bootstrap (optional anti-flash) --> reads nothing synchronously available yet
   |
   v
React mounts --> SettingsProvider (top of App.tsx)
   |
   v
invoke("load_settings")  ------------------------->  Rust: settings::load_settings(app: AppHandle)
   |                                                      |
   |                                                      v
   |                                                 app.path().app_data_dir()/settings.json
   |                                                      |
   |                                              exists & valid?  --- NO --> return AppSettings::default() (English/dark)
   |                                                      |--- YES --> serde_json::from_str --> AppSettings
   v
SettingsProvider state = { locale, theme } (defaults until resolved, then real values)
   |
   +--> ThemeContext consumer: sets document.documentElement.dataset.theme = theme
   |          --> CSS [data-theme="light"] block overrides the 9 tokens --> instant re-paint, zero JS re-render
   |
   +--> I18nContext consumer: t(key) looks up catalogs[locale][key] ?? catalogs["en"][key]
   |
   +--> user flips language/theme in Settings/About dialog
              |
              v
        setLocale/setTheme (React state update, immediate UI effect)
              |
              v
        invoke("save_settings", { settings })  -------> Rust: settings::save_settings(app, settings)
                                                              |
                                                              v
                                                         serde_json::to_string --> std::fs::write(app_data_dir/settings.json)
                                                         (app_data_dir is OS-managed, NEVER the user's chosen .jwlibrary path;
                                                          ArchiveSession/archive::save::atomic_replace is never touched)
```

### Recommended Project Structure
```
app/src-tauri/
├── signing/                      # NEW - inert until CI injects signCommand
│   ├── sign.ps1                  # adapted from remo-code/supervisor/tauri/signing/sign.ps1
│   └── trusted-signing-metadata.json
├── src/
│   ├── settings.rs                # NEW - load_settings/save_settings commands, AppSettings, SettingsError
│   └── lib.rs                     # register the two new commands in invoke_handler

app/src/
├── i18n/
│   ├── strings.ts                 # NEW - typed key union (e.g. `export type StringKey = keyof typeof en`)
│   ├── en.ts                      # NEW - the ONE complete catalog
│   ├── de.ts, es.ts, fr.ts, ...   # NEW - scaffolded with same keys, placeholder/empty values (D11-02)
│   └── I18nContext.tsx            # NEW - t(key), locale, setLocale
├── theme/
│   └── ThemeContext.tsx           # NEW - theme, setTheme, applies data-theme attribute
├── settings/
│   └── SettingsProvider.tsx        # NEW - composes I18n + Theme, calls load_settings/save_settings once
└── components/
    └── SettingsDialog.tsx          # NEW (or AboutDialog.tsx) - language/theme switcher + version display

.github/workflows/
└── release-app.yml OR app-ci.yml extended  # NEW leg - tag-triggered, gated on ENABLE_MSI_SIGNING
```

### Pattern 1: Gated signCommand injection (never commit a live signCommand)
**What:** The committed `tauri.conf.json` never has `bundle.windows.signCommand` set. A CI step running only under `vars.ENABLE_MSI_SIGNING == 'true'` patches the JSON file in the CI workspace immediately before `tauri build` runs, then the build proceeds normally.
**When to use:** Any Tauri project doing conditional signing where the credential set may not exist yet (this project's exact situation).
**Example:**
```powershell
# Source: remo-code/.github/workflows/release-supervisor.yml:100-128 (adapt path for jwlmanager: app/src-tauri/tauri.conf.json)
$confPath = 'app/src-tauri/tauri.conf.json'
$conf = Get-Content -Raw -LiteralPath $confPath | ConvertFrom-Json
$signCmd = 'powershell -ExecutionPolicy Bypass -File ../signing/sign.ps1 %1'
if (-not $conf.bundle.windows) {
    $conf.bundle | Add-Member -NotePropertyName windows -NotePropertyValue (@{}) -Force
}
$conf.bundle.windows | Add-Member -NotePropertyName signCommand -NotePropertyValue $signCmd -Force
($conf | ConvertTo-Json -Depth 20) | Set-Content -LiteralPath $confPath -Encoding utf8
```
[VERIFIED: remo-code, running production pattern in a sibling repo under the same Azure account]

### Pattern 2: Fail-loud signing script (never silently ship unsigned)
**What:** `sign.ps1` itself checks for `TRUSTED_SIGNING_DLIB` and exits non-zero if unset/missing rather than silently no-op'ing, so a misconfigured signing build fails the CI job loudly instead of shipping a falsely-labeled-signed artifact.
**When to use:** Any script wired as `signCommand` — this is the exact mechanism that satisfies the task brief's "do NOT design anything that would silently produce an unsigned artifact while reporting success" requirement, because once `signCommand` IS injected (i.e., `ENABLE_MSI_SIGNING=true`), the script itself refuses to succeed without real credentials.
**Example:** see `remo-code/supervisor/tauri/signing/sign.ps1:40-55` (the `if ([string]::IsNullOrWhiteSpace($Dlib))` block). [VERIFIED: remo-code]

### Pattern 3: Typed catalog with a build-time-checked key union
**What:** `en.ts` exports a plain object of `{ key: "English string" }` pairs; `strings.ts` derives `export type StringKey = keyof typeof en;` and every OTHER locale file is typed `Record<StringKey, string>` (via `satisfies` or an explicit type annotation), so TypeScript raises a compile error if a locale is missing a key or has an extra one.
**When to use:** Exactly this phase's i18n requirement — English-complete, other locales must not silently omit keys.
**Example:**
```typescript
// app/src/i18n/en.ts
export const en = {
  "commandBar.open": "Open",
  "commandBar.save": "Save",
  // ...
} as const;

// app/src/i18n/strings.ts
import { en } from "./en";
export type StringKey = keyof typeof en;

// app/src/i18n/de.ts (scaffolded, D11-02: placeholder, NOT machine-translated)
import type { StringKey } from "./strings";
// Deliberately incomplete/placeholder record -- Partial<> makes the fallback-to-English
// path a type-safe lookup, not a runtime crash risk.
export const de: Partial<Record<StringKey, string>> = {
  // intentionally empty/sparse until real translation work happens (D11-02)
};
```
```typescript
// app/src/i18n/I18nContext.tsx -- lookup with fallback (D11-02: falls back to English on missing key)
function t(key: StringKey): string {
  const active = catalogs[locale]?.[key];
  return active ?? en[key];
}
```
[ASSUMED: pattern is standard TypeScript, not sourced from a specific library doc since none applies]

### Pattern 4: CSS-only theme override, zero re-render
**What:** A second block using the identical 9 custom-property names, scoped under `[data-theme="light"]`, plus dynamic `color-scheme`.
**When to use:** Exactly D11-03.
**Example:**
```css
/* app/src/styles.css -- existing block stays as the dark default */
:root {
  --bg-primary: #1a1a1a;
  --bg-secondary: #242424;
  --bg-tertiary: #2c2c2c;
  --brand-primary: #2563eb;
  --destructive: #dc2626;
  --text-primary: #f5f5f5;
  --text-muted: #9a9a9a;
  --border-hairline: #333333;
  color-scheme: dark;
}

/* NEW -- light variant, values are a design/UI-SPEC decision (reference res/light.qss) */
:root[data-theme="light"] {
  --bg-primary: #ffffff;
  --bg-secondary: #f1f1f1;
  --bg-tertiary: #e6e6e6;
  --brand-primary: #2563eb;   /* brand color likely stays constant across themes -- confirm in UI-SPEC */
  --destructive: #c80b0b;      /* res/light.qss QWidget[class='confirm'] reference value */
  --text-primary: #1a1a1a;
  --text-muted: #4f4f4f;       /* res/light.qss QWidget[class='meta'] reference value */
  --border-hairline: #cccccc;  /* res/light.qss QWidget[class='info'] border reference */
  color-scheme: light;
}
```
```typescript
// app/src/theme/ThemeContext.tsx
function applyTheme(theme: "light" | "dark") {
  document.documentElement.dataset.theme = theme;
}
```
[VERIFIED: app/src/styles.css:1-17 confirms exact current token set and confirms no existing light values anywhere in the file]

### Pattern 5: Rust settings command pair, isolated from ArchiveSession
**What:** A new `settings.rs` module exposing two `#[tauri::command]` functions that never touch `ArchiveSession` state or any archive path.
**Example:**
```rust
// app/src-tauri/src/settings.rs (new module)
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
pub struct AppSettings {
    pub language: String,
    pub theme: Theme,
}

#[derive(Debug, Serialize, Deserialize, TS, Clone, Copy, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { language: "en".to_string(), theme: Theme::Dark }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not resolve app data directory: {0}")]
    AppDataDirUnavailable(String),
    #[error("failed to read settings file: {0}")]
    ReadFailed(String),
    #[error("failed to write settings file: {0}")]
    WriteFailed(String),
    #[error("settings file contained invalid JSON: {0}")]
    ParseFailed(String),
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> AppSettings {
    // Degrade to defaults on ANY failure -- never blocks startup, never surfaces
    // an error dialog for a missing/corrupt settings file (Integration Point/risk).
    (|| -> Result<AppSettings, SettingsError> {
        let dir = app.path().app_data_dir()
            .map_err(|e| SettingsError::AppDataDirUnavailable(e.to_string()))?;
        let path = dir.join("settings.json");
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| SettingsError::ReadFailed(e.to_string()))?;
        serde_json::from_str(&raw)
            .map_err(|e| SettingsError::ParseFailed(e.to_string()))
    })()
    .unwrap_or_default()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let dir = app.path().app_data_dir()
        .map_err(|e| SettingsError::AppDataDirUnavailable(e.to_string()).to_string())?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| SettingsError::WriteFailed(e.to_string()).to_string())?;
    let raw = serde_json::to_string(&settings)
        .map_err(|e| SettingsError::WriteFailed(e.to_string()).to_string())?;
    std::fs::write(dir.join("settings.json"), raw)
        .map_err(|e| SettingsError::WriteFailed(e.to_string()).to_string())
}
```
[VERIFIED against actual repo: `tauri::Manager` trait provides `.path().app_data_dir()` in Tauri 2 core (no plugin needed); `app/src-tauri/Cargo.toml` confirms `tauri = { version = "2" }`, `serde`/`serde_json`/`ts-rs`/`thiserror` all already present]

Note on `load_settings` returning a bare `AppSettings` (never `Result`/`ErrorDto`): this is a deliberate deviation from the project's `Result<T, ErrorDto>` convention used everywhere else, because the Integration-Point risk explicitly requires "a settings read/write failure degrades gracefully... rather than blocking app startup" — surfacing a `Result` to the frontend would tempt a component to show an error banner on first launch for a brand-new user with no settings file yet, which is not an error. `save_settings` DOES return `Result<(), String>` (or a typed `SettingsError` DTO) since a failed *save* is worth surfacing (e.g., disk full), just not a failed initial *load*. This asymmetry should be called out explicitly in the plan so a reviewer doesn't flag it as convention drift.

### Anti-Patterns to Avoid
- **Post-build `signtool` pass:** Never sign after `tauri build` completes. Confirmed hazard (CLAUDE.md rule 22a, `remo-code`'s own comments at `.github/workflows/release-supervisor.yml:73-76`): a post-build pass runs after any updater `.sig` computation and silently breaks updater verification. This project has no updater yet (out of scope), but wiring the correct order NOW avoids a re-discovery cost later.
- **Committing a live `signCommand` unconditionally:** Would break every unsigned local/dev/PR build the moment the script path resolves (or hard-fail every build if secrets are absent, since the script fails loudly by design) — always inject conditionally in CI, never commit it to the tracked `tauri.conf.json`.
- **Machine-translating the 8 non-English locale files:** Explicitly rejected by D11-02 — would create a false impression of completeness for a personal-data app. Locale files must remain visibly incomplete (a sparse/`Partial<>` catalog, empty by default) rather than filled with fabricated text.
- **Persisting settings via `ArchiveError`/inside `ArchiveSession`:** Would blur the line between archive data and app preferences. D11-04 mandates a separate `SettingsError` domain and `app_data_dir()` (never the archive's `temp_dir`/`db_path`).
- **Using `tauri-plugin-store` or any new Cargo/npm crate for settings:** Blocking legitimacy checkpoint per binding constraints; the hand-rolled ~40-line module is simpler than clearing a new dependency for two string fields.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Authenticode signing algorithm/timestamp protocol | A custom signing invocation from scratch | `signtool.exe` (Windows SDK, preinstalled on `windows-latest`) + Azure Trusted Signing dlib, exactly as `remo-code`'s `sign.ps1` already does | Re-deriving signtool flags (`/fd SHA256 /tr .../td SHA256 /dlib /dmdf`) risks a subtly wrong signature; the working invocation already exists in this GitHub root |
| CSS cascade / theming primitive | A React state-driven re-render of every component's inline styles | `data-theme` attribute + CSS custom-property override block | The app already proves CSS-var-only theming works with zero component changes; a JS-driven approach would be strictly worse (re-render cost, drift risk) |

**Key insight:** every one of this phase's four decisions is deliberately "boring" — copy a proven pattern (signing), extend an existing convention (typed errors, CSS tokens, `app_version` pattern), or write the smallest possible new module (settings.rs). Nothing here calls for a new abstraction.

## Runtime State Inventory

Not applicable — Phase 11 is not a rename/refactor/migration phase. It DOES introduce a genuinely new runtime state category (persisted settings file), which is covered under Integration Point/risk in CONTEXT.md and the settings.rs pattern above, not under this section's rename-specific checklist.

## Common Pitfalls

### Pitfall 1: Signing wired but silently inert due to a path mismatch
**What goes wrong:** The `signCommand` string `powershell -ExecutionPolicy Bypass -File ../signing/sign.ps1 %1` is relative to Tauri's bundling working directory (`app/src-tauri`), NOT the repo root or the CI job's working directory. If `sign.ps1` is placed at `app/src-tauri/signing/sign.ps1` but the CI script writes `signCommand` assuming a different relative depth, Tauri will either fail to find the script (loud failure, safe) or — worse — silently find a DIFFERENT stale script if one happens to exist at that relative path (dangerous).
**Why it happens:** `remo-code`'s reference lives at `supervisor/tauri/signing/sign.ps1` relative to `supervisor/tauri/src-tauri/tauri.conf.json` (one level up, `../signing/`). `jwlmanager`'s structure is `app/src-tauri/tauri.conf.json` — the equivalent path is `app/src-tauri/signing/sign.ps1` referenced as `../signing/sign.ps1` FROM `app/src-tauri` (i.e., signing/ sits inside src-tauri, not one level above it, since `app/src-tauri` is already the project root Tauri uses).
**How to avoid:** Place `sign.ps1` at `app/src-tauri/signing/sign.ps1` and reference it from `signCommand` as `signing/sign.ps1` (not `../signing/sign.ps1` — that would look for it at `app/signing/`). Verify with `tauri build --config` dry paths or a scratch local test where `TRUSTED_SIGNING_DLIB` is deliberately unset (should fail loud, confirming the script IS being invoked).
**Warning signs:** A "signed" release artifact that `signtool verify /pa` reports as unsigned, with no CI failure at all — this means the signCommand silently never ran (Tauri treats a missing `signCommand` script path inconsistently across versions; some fail the build, so ALWAYS test the fail-closed path deliberately before trusting a green build means "signed").

### Pitfall 2: Anti-flash-of-wrong-theme race with the async `invoke`
**What goes wrong:** `load_settings` is an async Tauri IPC round-trip. If the app always paints with default (`dark`) CSS on first frame and only applies the persisted `light` choice after the IPC promise resolves, a user with a saved light-theme preference sees a one-frame (or longer, if IPC is slow) flash of the wrong theme on every launch.
**Why it happens:** React mounts and the browser paints before any `useEffect` calling `invoke` can resolve; IPC is not synchronous.
**How to avoid:** CONTEXT.md explicitly leaves this to Claude's Discretion and calls a one-frame flash acceptable if the synchronous alternative isn't feasible. The synchronous alternative (an inline `<script>` in `index.html` reading a value BEFORE React mounts) is not fully available here because the value lives in a Tauri-Rust-owned file, not `localStorage` — a synchronous read would require either (a) accepting the async flash (simplest, matches CONTEXT's explicit fallback), or (b) writing settings to BOTH `app_data_dir()/settings.json` (source of truth) AND a `localStorage` mirror the inline script can read synchronously on next launch (adds a second write path — more complexity than this phase's scope justifies). **Recommendation: accept the one-frame flash** (CONTEXT's explicitly sanctioned fallback) rather than adding a second persistence path.
**Warning signs:** Visible flash reported in manual QA; not a correctness bug, just a polish gap explicitly permitted by CONTEXT.

### Pitfall 3: Under-inventorying hardcoded UI strings
**What goes wrong:** A grep for JSX text nodes alone misses `aria-label`/`title`/`placeholder` string-literal props, error messages constructed via template literals, and dialog-only strings that only render conditionally (e.g., only visible mid-merge-preview).
**Why it happens:** JSX text content and JSX attribute strings have different syntactic shapes; a single regex pass catches one but not the other.
**How to avoid:** CONTEXT.md's own risk section calls this out explicitly. A rough scan of `app/src/components/*.tsx` (23 component files, ~12 with matching `.test.tsx` pairs) found ~11 JSX-text-node string matches and ~21 `aria-label`/`title`/`placeholder` attribute matches in a narrow sampling regex alone (undercounts real total — multiline JSX text and template-literal-built strings are NOT captured by a single-line regex). Treat the actual inventory as a dedicated implementation task: grep separately for `>[text]<` JSX children, `aria-label=`/`title=`/`placeholder=` attributes, and any user-facing string built via template literal or passed to an error/toast helper — do not trust any single regex pass as exhaustive.
**Warning signs:** A component renders untranslated English text even when a non-English locale is active and that locale HAS a catalog entry for a similar string elsewhere — usually means a string was missed during inventory, not a lookup bug.

### Pitfall 4: `nuget install` flag regression
**What goes wrong:** Adding `-ExcludeVersion:$false` (or similar cmdlet-style switch syntax) to the `nuget install` invocation silently breaks the NuGet install step because `nuget.exe` is a native binary, not a PowerShell cmdlet — PowerShell passes the literal string `-ExcludeVersion:False` and `nuget.exe` errors with "Unknown option", which can fail the CI job (or, per `remo-code`'s own recorded incident, silently skip signing on a build that appeared to succeed).
**Why it happens:** Documented as an ACTUAL incident in the reference implementation (`remo-code/.github/workflows/release-supervisor.yml:85-90`, "v0.13.1's first signed build").
**How to avoid:** Copy the exact `nuget install Microsoft.Trusted.Signing.Client -Version $ver -OutputDirectory $out` invocation with NO `-ExcludeVersion` flag at all (its absence is intentional — nuget's default keeps the version in the folder path, which the subsequent `$dll` path construction expects).
**Warning signs:** CI step fails with "Unknown option" from nuget.exe, or (worse) the step reports success but `$dll` path doesn't exist because the folder name lacks the expected version suffix.

## Code Examples

See Architecture Patterns section above (Patterns 1-5) — each includes a source-cited or repo-verified code example inline, avoiding duplication here.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| Python app: unsigned executables, users manually bypass SmartScreen/Gatekeeper (`.github/SECURITY.md` reference in repo docs) | Tauri app: signed via Azure Trusted Signing during bundling | This phase | Eliminates the SmartScreen warning for Windows users; macOS Gatekeeper bypass (`xattr -cr`) remains necessary since macOS notarization is explicitly out of scope |
| Python app: `gettext` catalogs, `.po`/`.mo` compiled translation files, `QTranslator` | Tauri app: plain TS `Record` catalogs, React context, no compilation step | This phase | Simpler toolchain (no `msgfmt`/`.mo` compile step), but loses gettext's plural-forms/ICU features — acceptable given the current string set has no pluralization needs (confirm during implementation; if a plural form is discovered, note it as a known catalog limitation, do not add a pluralization library) |
| Python app: `res/dark.qss`/`res/light.qss` (separate Qt stylesheet files per theme) | Tauri app: single stylesheet, `data-theme` attribute selector override | This phase | One file to maintain instead of two; CSS custom properties make token reuse structurally guaranteed (impossible to reference a color that doesn't exist in both themes without a lint/visual gap) |

**Deprecated/outdated:** N/A — no library version deprecations apply to this phase's zero-new-dependency scope.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `Microsoft.Trusted.Signing.Client` NuGet version `1.0.60` (copied from `remo-code`'s current pin) is still current/available at implementation time | Standard Stack, Version verification | Low — if stale, `nuget install` will still succeed with an explicit version pin (NuGet keeps old versions available); only a security/feature reason would require bumping, and the pin is a known-working baseline, not a hard requirement |
| A2 | `sign.ps1`'s relative path adaptation (`signing/sign.ps1` referenced as `signing/sign.ps1`, not `../signing/sign.ps1`, from `app/src-tauri`) is correct for this repo's directory depth | Pitfall 1 | Medium — a wrong relative path causes the signCommand to silently fail to find the script (should fail the build loudly per Tauri's own script-not-found handling, but this MUST be verified locally/in CI with a deliberately-broken `TRUSTED_SIGNING_DLIB` before trusting any signed release) |
| A3 | No pluralization/ICU-message-format need exists in the current or near-future UI string set | State of the Art | Low — if a plural form is later needed (e.g., "1 item selected" vs "3 items selected"), the flat `Record<string,string>` catalog can still express it via parameterized keys (e.g., separate `itemSelected.one`/`itemSelected.other` keys with manual branching in `t()`), just less ergonomically than a real i18n library — not a blocking gap for Phase 11's scope |
| A4 | Tauri 2's `AppHandle`/`App` implementing `Manager` exposes `.path().app_data_dir()` without any additional capability/permission entry needed in `capabilities/default.json` (since it's invoked from a first-party `#[tauri::command]`, not a JS-facing plugin API) | Architecture Patterns Pattern 5 | Low-Medium — if Tauri 2's permission model DOES gate `app_data_dir()` resolution behind a capability even for first-party commands, the settings commands would fail at runtime with a permission-denied error; this is inferred from Tauri 2's general pattern (capabilities gate PLUGIN-exposed JS APIs, not arbitrary Rust code inside your own `#[tauri::command]` functions) but was not exercised against a running build in this research pass — the planner should add a Wave-0 smoke test invoking `load_settings`/`save_settings` early to surface this immediately if wrong |

**If this table is empty:** N/A — table is populated above.

## Open Questions

1. **Which CI workflow file hosts the signing leg — extend `app-ci.yml` or add a new tag-triggered `release-app.yml`?**
   - What we know: CONTEXT.md leaves this to Claude's Discretion, leaning toward a new release workflow "since signing should only run on actual release builds/tags, not every PR push." The existing Python app's `.github/workflows/release.yml` is tag-triggered (`push: tags: ['v*']`) and orchestrates a 3-platform build+release-draft flow — a close structural precedent already in this exact repo.
   - What's unclear: Whether the NEW Tauri app's release tag pattern should reuse bare `v*` (colliding with the Python app's existing tag scheme, since both live in the same repo per the fork-of-upstream structure) or use a distinguishing prefix.
   - Recommendation: Use a distinguishing tag prefix for the Tauri app's releases (e.g., `app-v*.*.*`, mirroring the milestone-code collision-avoidance convention this project already uses elsewhere) to avoid ambiguity with the Python app's `v*` tags in the same repo, and create a NEW `.github/workflows/release-app.yml` (not extending `app-ci.yml`, which is explicitly documented as the PR/push matrix with "No code signing here" — mixing concerns into it would break that documented boundary). Confirm exact tag scheme with the user/planner since it also affects how the Python app's existing `release.yml` and this repo's fork-vs-upstream relationship is communicated to users browsing GitHub Releases.

2. **Is the `libloading`-loaded `jwlCore` native binary itself in scope for signing, or only the Tauri-produced MSI/installer?**
   - What we know: `bundle.windows.signCommand` signs whatever artifact(s) Tauri's bundler produces (the MSI/NSIS installer, and per some Tauri versions, the main `.exe` inside it). The `jwlCore-amd64.dll` bundled as a `resource` (per `tauri.conf.json`'s `bundle.resources` block) is a separate prebuilt binary NOT built by this repo's CI.
   - What's unclear: Whether Windows SmartScreen friction is fully eliminated if the outer MSI is signed but an embedded/resource DLL is not (in practice, SmartScreen reputation applies primarily to the top-level installer/executable a user directly runs, and embedded resource DLLs loaded via `libloading` at runtime are not independently SmartScreen-checked the same way — this is a reasonable assumption but not independently verified in this research pass).
   - Recommendation: Proceed with signing only what `tauri build` itself produces (the MSI and its bundled `.exe`), consistent with `remo-code`'s reference pattern (which also bundles a compiled sidecar binary without separately signing it). If SmartScreen friction is later observed specifically on `jwlCore-amd64.dll` post-launch, that would need re-scoping — flag as a residual risk in the plan's verification section, not a blocker to this phase.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `AZURE_CLIENT_ID`/`AZURE_CLIENT_SECRET`/`AZURE_TENANT_ID` repo secrets on `finedesignz/jwlmanager` | Live signing (D11-01) | ✗ (confirmed via `gh secret list` — empty) | — | Wire the signCommand injection + script so CI stays green UNSIGNED; document the exact provisioning steps for when credentials exist (see Signing Verification below) |
| `ENABLE_MSI_SIGNING` repo variable | Gates the signing CI leg | ✗ (confirmed via `gh variable list` — empty) | — | Gate defaults to falsy/absent, which the pattern already treats as "signing off" — no code path assumes it's set |
| Windows SDK / `signtool.exe` on `windows-latest` GH runners | Signing script | ✓ (per `remo-code`'s working CI, same runner image family) | Whatever ships on current `windows-latest` | N/A — this is GitHub-hosted-runner-provided, not project-controlled |
| NuGet `Microsoft.Trusted.Signing.Client` package | Signing script | ✓ (public NuGet package, installed fresh each CI run) | pin `1.0.60` per `remo-code`'s working reference (verify current at implementation time) | N/A |
| `nuget.exe` on `windows-latest` runners | Installing the Trusted Signing dlib | ✓ (per `remo-code`'s comment: "nuget.exe ships on windows-latest runners") | — | Explicit fallback noted in `remo-code`'s own script comment if absent: direct install; not needed given it's confirmed present |

**Missing dependencies with no fallback:**
- The three Azure secrets + `ENABLE_MSI_SIGNING` variable — genuinely cannot be provisioned from within this environment (no Azure service-principal creation capability, no write access to GitHub repo secrets demonstrated in this research pass). This IS the documented, expected state per the task brief; the phase's job is to make the ABSENCE of these harmless (unsigned-but-green CI), not to provision them.

**Missing dependencies with fallback:**
- None beyond the above — everything else needed (Windows SDK, NuGet, existing Cargo/npm deps) is already available.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust: built-in `cargo test` (existing convention across all 10 prior phases); Frontend: `vitest` (`app/package.json` `"test": "vitest run"`) |
| Config file | `app/src-tauri/Cargo.toml` (no separate test config); `app/vite.config.ts` (vitest config, existing) |
| Quick run command | `cargo test --jobs 2` (Rust, MANDATORY `--jobs 2` per binding constraints — default parallelism OOMs the linker); `npx vitest run` (frontend) |
| Full suite command | Same two commands — no separate "full" tier exists in this project; add `cargo clippy --all-targets -- -D warnings` and `npx tsc --noEmit` as required gates per binding constraints |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PLAT-02 (signing wiring correctness) | `tauri.conf.json` committed state has NO `signCommand`; a scratch/CI-simulated injection produces valid JSON; `sign.ps1` fails loudly with `TRUSTED_SIGNING_DLIB` unset | unit/integration (script logic) + manual (actual signed-artifact verification is blocked on credentials, see below) | A Rust or shell test asserting `tauri.conf.json`'s `bundle.windows` key is absent/has no `signCommand` in the committed file; a manual local run of `sign.ps1` with `TRUSTED_SIGNING_DLIB` unset, asserting non-zero exit | ❌ Wave 0 |
| PLAT-03 (i18n coverage) | Every `StringKey` has an English value; missing-key fallback works; language switch re-renders visible text | unit (TS) | `npx vitest run app/src/i18n/*.test.ts` -- assert `Object.keys(en).length === Object.keys(strings/StringKey union).length` and that a `t()` call for a key absent from a non-English catalog returns the English value | ❌ Wave 0 |
| PLAT-04a (theme token completeness) | Both `:root` and `:root[data-theme="light"]` define the SAME 9 custom-property names, none missing | unit/lint (CSS parse) or a small Node/TS script test | A vitest test (or a small `ctx_execute` script during planning-verification) parsing `styles.css`, extracting property names under each selector block, asserting set equality | ❌ Wave 0 |
| PLAT-04b (theme switch applies instantly) | Toggling theme updates `document.documentElement.dataset.theme` and a subsequent computed-style check reflects new token values | integration (React Testing Library, matches existing `*.test.tsx` convention) | `npx vitest run app/src/theme/ThemeContext.test.tsx` | ❌ Wave 0 |
| D11-04 (settings round-trip + corrupt-file degradation) | `save_settings` then `load_settings` returns the same values; a corrupt/missing settings.json degrades to `AppSettings::default()` without panicking | unit (Rust, `tempfile`-overridden path per Established Patterns) | `cargo test --jobs 2 settings::` | ❌ Wave 0 |
| D11-04 (settings never touches archive state) | `save_settings`/`load_settings` compile and run with zero references to `ArchiveSession`/`archive::save` | code-review / grep-based check, not a runtime test | `grep -n "ArchiveSession\|archive::save" app/src-tauri/src/settings.rs` should return nothing | N/A (structural check) |

### Sampling Rate
- **Per task commit:** `cargo test --jobs 2` + `npx vitest run` + `npx tsc --noEmit`
- **Per wave merge:** same, plus `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`
- **Phase gate:** full suite green before `/gsd-verify-work`; additionally, a MANUAL step verifying `tauri.conf.json` as committed to git has no `signCommand` key (a structural safety check, since an accidentally-committed live signCommand would break every future unsigned build)

### Wave 0 Gaps
- [ ] `app/src-tauri/src/settings.rs` + its `#[cfg(test)]` module — covers D11-04 round-trip + corrupt-file degradation, using `tempfile` to override the app-data path per Established Patterns (never touch a real OS app-data dir in tests)
- [ ] `app/src/i18n/strings.test.ts` (or similar) — covers PLAT-03 key-coverage assertion
- [ ] `app/src/theme/ThemeContext.test.tsx` — covers PLAT-04b
- [ ] A CSS-token-completeness test/script — covers PLAT-04a (no existing test touches `styles.css` directly; this is new test infrastructure, however small)
- [ ] Signing script structural test — no existing test infrastructure covers `.ps1` script correctness; the planner should decide whether a PowerShell-runnable assertion (Windows-only, matches the signing script's own platform) or a documentation-only manual verification step is sufficient, since this project's CI matrix is cross-platform but signing is Windows-only

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Phase adds no auth surface |
| V3 Session Management | No | N/A |
| V4 Access Control | No | N/A |
| V5 Input Validation | Yes (narrow) | `AppSettings.language` and `.theme` deserialized via `serde_json` are already type-constrained (`Theme` is a closed enum; `language` is a plain `String` with no injection surface since it's never used to build a path, query, or shell command — only used as a lookup key into an in-memory TS `Record`, which safely returns `undefined`/falls back to English for any unrecognized value) |
| V6 Cryptography | No | Signing uses Azure's managed key material (Trusted Signing never exposes private key material to this repo/CI at all — that is the entire point of the "Trusted Signing" service vs. a locally-held `.pfx`), no cryptographic code is written by this project |
| V12 File Handling (informal ASVS extension relevant here) | Yes | `settings.json` path is ALWAYS derived from `app_data_dir()` (OS-computed, never user input), never from a user-chosen path — this closes off any path-traversal concern for the settings file specifically, distinct from (and much narrower than) the zip-slip concern that applies to `.jwlibrary` archive extraction (already handled in Phase 1, unrelated to this phase) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious/corrupted `settings.json` crafted to crash the app on load | Denial of Service (localized) | `load_settings` degrades to `AppSettings::default()` on ANY parse/read failure (Pattern 5 above) rather than propagating an error or panicking; since the file lives in an OS-user-writable app-data directory, a local attacker with filesystem access could already do much worse (this is not a meaningfully elevated attack surface) |
| Signing credential leakage via CI logs | Information Disclosure | The three Azure secrets are consumed as env vars by `tauri-action`/`tauri build`, never echoed by the reference `sign.ps1` script (only `$Dlib`/`$Metadata`/`$SignTool` PATHS are logged via `Write-Host`, never credential values) — replicate this exact logging discipline, never add a debug `Write-Host $env:AZURE_CLIENT_SECRET` or similar during implementation/troubleshooting |
| Supply-chain risk in the NuGet-fetched Trusted Signing dlib | Tampering | This is Microsoft's own official NuGet package (`Microsoft.Trusted.Signing.Client`), fetched fresh each CI run from the public NuGet feed — same trust boundary GitHub Actions/`windows-latest` runners already operate within; no additional mitigation beyond pinning a known-working version (A1 above) is warranted for this phase's scope |

## Sources

### Primary (HIGH confidence)
- `C:\Users\artic\GitHub\remo-code\supervisor\tauri\signing\sign.ps1` — full working Azure Trusted Signing script, read in full
- `C:\Users\artic\GitHub\remo-code\supervisor\tauri\signing\trusted-signing-metadata.json` — exact account/cert-profile/endpoint JSON shape
- `C:\Users\artic\GitHub\remo-code\.github\workflows\release-supervisor.yml` — full gated CI wiring pattern, read in full (lines 1-145)
- `C:\Users\artic\GitHub\jwlmanager\app\src-tauri\tauri.conf.json` — current bundle config, confirmed no signCommand exists
- `C:\Users\artic\GitHub\jwlmanager\app\src-tauri\Cargo.toml` — confirmed exact dependency set (no new dep needed)
- `C:\Users\artic\GitHub\jwlmanager\app\package.json` — confirmed exact frontend dependency set
- `C:\Users\artic\GitHub\jwlmanager\app\src\styles.css` (lines 1-40) — confirmed exact current token set, no light values exist
- `C:\Users\artic\GitHub\jwlmanager\.github\workflows\app-ci.yml` — confirmed "No code signing here (PLAT-02 is Phase 11)" comment and 4-platform matrix shape
- `C:\Users\artic\GitHub\jwlmanager\res\dark.qss`, `res\light.qss` — Qt stylesheet palette reference values used in Pattern 4
- `C:\Users\artic\GitHub\jwlmanager\app\src-tauri\src\lib.rs` — confirmed `env!("CARGO_PKG_VERSION")` pattern at 3 call sites
- `C:\Users\artic\GitHub\jwlmanager\app\src-tauri\capabilities\default.json` — confirmed current capability set (`core:default`, `dialog:default` only)
- CLAUDE.md rule 22a (global) — authoritative Azure account/cert/RG/tenant identifiers, quoted verbatim into User Constraints handling above

### Secondary (MEDIUM confidence)
- `C:\Users\artic\GitHub\jwlmanager\.github\workflows\release.yml` — the OLD Python app's tag-triggered release pattern, used as a structural precedent for Open Question 1, not a literal template (different toolchain entirely)
- Repo-wide grep for JSX text/attribute strings across `app/src/components/*.tsx` — a narrow single-line-regex sampling pass, undercounts the true inventory (flagged explicitly in Pitfall 3 and the Assumptions Log)

### Tertiary (LOW confidence)
- Assumption A4 (capability/permission model not gating first-party `#[tauri::command]` calls to `app_data_dir()`) — inferred from general Tauri 2 architecture knowledge, not independently exercised against a running build in this research pass

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new dependencies confirmed against actual `Cargo.toml`/`package.json`, not assumed
- Architecture (signing): HIGH — copied from a verified, currently-running production pattern in a sibling repo under the same Azure account
- Architecture (i18n/theme/settings): HIGH — mechanisms confirmed buildable against actual current file contents (`styles.css`, `Cargo.toml`, `lib.rs`)
- Pitfalls: HIGH for signing (one is a documented real incident from the reference repo); MEDIUM for the i18n string-inventory count (explicitly flagged as an undercount, real number needs a dedicated implementation task, not a research-phase deliverable)

**Research date:** 2026-07-26
**Valid until:** 30 days (stable domain — no fast-moving library APIs involved; the one time-sensitive fact, the NuGet package version pin, should be re-checked if this research is consulted after roughly 60-90 days)
