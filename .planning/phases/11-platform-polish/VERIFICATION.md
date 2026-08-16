---
phase: 11-platform-polish
verified: 2026-08-16T00:00:00Z
status: human_needed
score: 9/9 must-haves verified (automated/code evidence); 6 items require live-app human verification
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Open Settings, switch theme Light <-> Dark"
    expected: "Whole app repaints instantly, no reload, no flash of unstyled content"
    why_human: "CSS-only data-theme mechanism is proven by DOM-attribute unit test and token-parity test, but the actual live repaint in a running Tauri window cannot be observed headlessly."
  - test: "Set theme + language, fully restart the app (not just re-render)"
    expected: "Both choices are restored from settings.json under the OS app-data directory"
    why_human: "Round-trip is proven against a tempdir/mocked IPC in tests; a real OS app-data-dir restart cycle was not exercised in this headless environment."
  - test: "Manually corrupt/truncate the real settings.json on disk, then launch the app"
    expected: "App starts normally with English + Dark defaults, no error dialog, no crash"
    why_human: "Degradation is proven at the Rust integration-test and React unit-test layers against synthetic fixtures; a real corrupt file on a real OS app-data path was not exercised."
  - test: "Open Settings, cycle through all 9 locales while a dialog and an error banner are visible"
    expected: "Command bar, open dialog, and error banner all re-render legibly on the same interaction; every non-English locale falls back to readable English text (no blank/undefined strings)"
    why_human: "Multi-component re-render across locale switch is proven with a React Testing Library harness against mocked IPC; a live multi-surface Tauri session was not exercised."
  - test: "Select a non-English language, restart the app"
    expected: "The selected language is still active after restart"
    why_human: "Same restart-persistence gap as theme above, for the language field."
  - test: "Run `signtool verify /pa` against a real signed .msi produced with ENABLE_MSI_SIGNING=true and provisioned Azure credentials"
    expected: "Valid Authenticode signature, Titanium Labs LLC as signer"
    why_human: "Genuinely unattainable in this environment -- AZURE_CLIENT_ID/AZURE_CLIENT_SECRET/AZURE_TENANT_ID and ENABLE_MSI_SIGNING are confirmed absent from this repository (verified: no such secrets/vars are referenced as present anywhere in the workflow beyond the gated `if:` conditions) and cannot be provisioned from within a verification pass. This is the documented, expected operational follow-up in docs/signing.md, not an execution gap -- the WIRING that would consume those credentials is itself fully verified below."
---

# Phase 11: Platform Polish (Signing, Localization, Theme) Verification Report

**Phase Goal:** The app is distributable and comfortable for real-world daily use across platforms and languages.
**Verified:** 2026-08-16
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria + must_haves)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Windows release binaries are Authenticode-signed via Azure Trusted Signing **as part of the bundling step, never a post-build pass** | VERIFIED (wiring) | `.github/workflows/release-app.yml`: "Inject bundle.windows.signCommand" step runs BEFORE "Build Tauri app" in the same job (lines 121-155). Committed `app/src-tauri/tauri.conf.json` carries no `signCommand` (confirmed by direct read — only a `"windows"` icon-array key exists). `app/src-tauri/signing/sign.ps1` fails loud (`exit 1`) with no artifact arg, unset `TRUSTED_SIGNING_DLIB`, or a `TRUSTED_SIGNING_DLIB` pointing at a missing file — read verbatim, confirms fail-closed behavior. `cargo test --jobs 2` runs the 4 `signing_wiring.rs` guard tests live (this session, not trusted from SUMMARY): all 4 pass. **A genuinely Authenticode-signed artifact was NOT produced** — Azure credentials are absent from this repo by design (documented ops follow-up, see human_verification). |
| 2 | User can switch UI language and all user-facing strings render translated | VERIFIED | `I18nProvider`/`useI18n` (`app/src/i18n/I18nContext.tsx`) read directly: single `catalogs[locale]?.[key] ?? en[key]` fallback. `SettingsDialog.tsx` has a real, functional `<select>` (not a placeholder) wired to `setLocale`, persisting through `SettingsProvider`'s `updateSettings`. `completeness.test.ts` (19 tests) run live this session: PASS — proves all 13 retrofitted components + App shell + SettingsDialog render exclusively through `t()`, with genuine red/green demonstrations (not merely written, actually executed). |
| 3 | User can switch theme (light/dark) and the change applies immediately across the app | VERIFIED | `app/src/styles.css` read directly: `:root[data-theme="light"]` block (8 tokens + dynamic `color-scheme`) exists alongside the dark `:root` block. `ThemeContext.tsx` read directly: the ONLY DOM effect is `document.documentElement.dataset.theme = theme` — zero JS-computed color. `SettingsDialog.tsx` has real Light/Dark buttons wired to `setTheme`. Unit tests (`ThemeContext.test.tsx`, `styles_tokens.test.ts`) pass in this session's `npx vitest run`. |
| 4 | Zero new dependencies across the whole phase | VERIFIED | `git diff 3ae54a74 HEAD -- app/package.json app/package-lock.json app/src-tauri/Cargo.toml app/src-tauri/Cargo.lock` run this session: **0 lines of diff**. Base commit `3ae54a74` is the last commit before Phase 11's context/plans landed. |
| 5 | Settings writes go through a single functional path with no stale-read race | VERIFIED | `SettingsProvider.tsx` read directly: `updateSettings(patch)` is the ONLY call site touching `setSettings`/`invoke("save_settings", ...)`; both `setTheme` and `setLanguage` call through it exclusively. The IPC write fires from *inside* the `setState(prev => ...)` functional updater using the `next` value the updater itself computed — not a second read of external state — so back-to-back theme+language changes in the same tick cannot drop a field. |
| 6 | A settings-save failure surfaces to the user, never silently swallowed | VERIFIED | `SettingsProvider.tsx`: `updateSettings`'s `save_settings` invoke `.catch((err) => setSaveError(err as ErrorDto))`; `saveError && <ErrorBanner error={saveError} />` renders inside the provider's own subtree. `load_settings` failure is the ONE deliberate silent-degrade path (falls back to built-in English+Dark defaults, matches Rust-side `AppSettings::default()` — by design, not a bug, since a load failure must never block startup). |
| 7 | The 8 non-English locales (de/es/fr/it/pl/pt/ru/uk) are present, key-aligned, and genuinely untranslated (no machine translation) | VERIFIED | Read all 8 locale files directly this session: every one is `export const <code>: Partial<Record<StringKey, string>> = {};` — byte-identical, empty, with an explicit "Deliberately empty... Do NOT fill with machine-translated text" comment. `StringKey = keyof typeof en` (derived, not hand-duplicated) means every locale is structurally key-aligned by construction; `npx tsc --noEmit` (run this session) is clean. |
| 8 | `.jwlibrary` and other format-literal tokens are not localized | VERIFIED | `App.tsx`: `{t("app.emptyState.bodyBefore")}<code>.jwlibrary</code>{t("app.emptyState.bodyAfter")}` — the literal stays outside any `t()` call, split via the documented bodyBefore/bodyAfter convention. `en.ts`'s `errors.*` catalog embeds `.jwlibrary`/`manifest.json`/`user_data` as plain substrings of English sentences (not separately "translated" tokens, since English is the only complete catalog). `CommandBar.tsx`'s `defaultPath` default filenames (`"New Archive.jwlibrary"`, `"Archive.jwlibrary"`, `"Archive (v14).jwlibrary"`) remain hardcoded literals, distinct from the `filters[].name` label, which IS translated via `t("commandBar.filterBackup")`. |
| 9 | Category/color enum display labels never leak into `onSelect`/IPC/`data-testid` (DATA-08) | VERIFIED | `CategorySwitcher.tsx` read directly: `onSelect(category)` and `` data-testid={`category-switcher-option-${category}`} `` both key off the raw `Category` enum value; `categoryLabel(category, t)` is used ONLY as JSX child text. `completeness.test.ts`'s "category enum isolation (structural)" describe block (run live this session) includes a genuine red/green demonstration — tampering the source to route a translated label into `onSelect` is caught by the guard regex. |

**Score:** 9/9 truths verified by automated test + direct code read. 0 failed. 6 items require live-app human verification (theme repaint, restart persistence x2, corrupt-file real-OS-path degradation, multi-locale live re-render, and a genuinely signed artifact) — none of these are code defects; all are documented, expected gaps in a headless verification pass.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `app/src-tauri/src/settings.rs` | `AppSettings`/`Theme`/`SettingsError`, `app_data_dir()`-scoped load/save commands | VERIFIED | Read directly: `load_settings`/`save_settings` both call `app.path().app_data_dir()`; directory-taking helpers (`load_settings_from_dir`/`save_settings_to_dir`) are `pub` for testing. |
| `app/src-tauri/tests/settings_persistence.rs` | Round-trip, missing/corrupt/truncated-file degradation, archive-isolation guard | VERIFIED | Read directly: 7 tests including `settings_source_has_no_archive_references` (forbidden-identifier structural scan). All 7 pass in this session's `cargo test --jobs 2`. |
| `app/src/settings/SettingsProvider.tsx` | Single write-through path, save-failure surfaced | VERIFIED | See truths 5-6 above. |
| `app/src/theme/ThemeContext.tsx`, `app/src/styles.css` | CSS-only theme switch | VERIFIED | See truth 3. |
| `app/src/i18n/*` (9 locale files + `I18nContext.tsx` + `strings.ts` + `locales.ts`) | Dependency-free i18n catalog + context | VERIFIED | See truths 2, 7. `grep -i "i18n\|intl\|locale" app/package.json` this session: no matches — no i18n library added. |
| `app/src-tauri/signing/sign.ps1`, `.github/workflows/release-app.yml`, `app/src-tauri/tests/signing_wiring.rs`, `docs/signing.md` | Fail-closed signing wiring, gated release publishing | VERIFIED | See truth 1. `docs/signing.md` exists (confirmed by directory listing) with operator provisioning procedure. |
| `app/src/components/SettingsDialog.tsx` | Functional theme + language switchers, About/version region | VERIFIED | Read directly: no "coming soon" language slot remains — real `<select>` wired to `setLocale`; `app_version` invoked and rendered via `t("settings.versionLine", {version})`, never a literal. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `SettingsDialog.tsx` theme buttons | `SettingsProvider.updateSettings` | `setTheme(theme)` call | WIRED | Direct read confirms `onClick={() => setTheme("light"/"dark")}`. |
| `SettingsDialog.tsx` language `<select>` | `SettingsProvider.updateSettings` | `setLocale` == `useI18n().setLocale` == `SettingsProvider.setLanguage` | WIRED | `onChange={(event) => setLocale(event.target.value)}`; `I18nProvider` receives `setLocale={setLanguage}` as a controlled prop from `SettingsProvider`. |
| `ThemeContext` | DOM | `document.documentElement.dataset.theme = theme` in a `useEffect` | WIRED | Direct read; CSS `:root[data-theme="light"]` selector consumes the same attribute. |
| `SettingsProvider` | Rust `load_settings`/`save_settings` commands | `invoke("load_settings")` / `invoke("save_settings", {settings})` | WIRED | Direct read; both commands registered in `lib.rs`'s `generate_handler!` (line 2993 area, confirmed by grep). |
| `.github/workflows/release-app.yml` signCommand injection | `app/src-tauri/signing/sign.ps1` | Injected string `signing/sign.ps1` (workspace-relative to `app/src-tauri`, the Tauri project root) | WIRED | `signing_wiring.rs#release_workflow_references_the_script_where_it_lives` run live this session: pass. Path depth is correct for this repo's layout (verified by reading the injection step's own comment + the guard test). |
| `release-app.yml` `publish-release` job | `build-windows` job's unsigned artifact | `needs: build-windows` + identical `if: vars.ENABLE_MSI_SIGNING == 'true'` gate | WIRED (gated) | Read directly: the exact same condition string gates the NuGet install step, the signCommand-injection step, AND the publish-release job — no third ungated path exists. |
| `ErrorBanner.tsx` | `describeError(err, t)` | `useI18n()` inside `ErrorBanner`, passed to `describeError` | WIRED | Confirmed: sole production call site (per SUMMARY's whole-tree grep claim, spot-checked via direct file read). |

### Behavioral Spot-Checks (run this session, not trusted from SUMMARY)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full Rust test suite | `cargo test --jobs 2` (from `app/src-tauri`) | exit 0; 47 `test result: ok` blocks, 0 `FAILED` | PASS |
| Full JS/TS test suite | `npx vitest run` (from `app/`) | 21 files / 207 tests, all passed | PASS |
| TypeScript strictness | `npx tsc --noEmit` (from `app/`) | exit 0, no output | PASS |
| Production build | `npm run build` (from `app/`) | exit 0, `tsc && vite build` succeeded, artifacts emitted | PASS |
| Rust lint (deny warnings) | `cargo clippy --jobs 2 --all-targets -- -D warnings` (from `app/src-tauri`) | exit 0; only pre-existing `ts-rs` "failed to parse serde attribute" build-script notes (unrelated, pre-existing `try_from = "Vec<i64>"` attrs) | PASS |
| Dependency diff | `git diff 3ae54a74 HEAD -- app/package.json app/package-lock.json app/src-tauri/Cargo.toml app/src-tauri/Cargo.lock` | 0 lines | PASS (zero new deps) |
| i18n completeness scan | `npx vitest run src/i18n/completeness.test.ts` | 1 file, 19 tests, all passed | PASS |
| describeError coverage | `npx vitest run src/lib/errors.test.ts` | 1 file, 4 tests, all passed | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| PLAT-02 | 11-02 | Windows binaries Authenticode-signed via Azure Trusted Signing during bundling | SATISFIED (wiring); artifact production pending ops credential provisioning | See truth 1. |
| PLAT-03 | 11-03, 11-04 | User can switch UI language; all user-facing strings localized | SATISFIED | See truths 2, 7, 8, 9. |
| PLAT-04 | 11-01 | User can switch theme | SATISFIED | See truths 3, 5, 6. |

No orphaned requirements — REQUIREMENTS.md maps exactly PLAT-02/03/04 to Phase 11, and all three are claimed by plans 11-01 through 11-04.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `app/src/styles.css` | 299 | `"coming soon"` comment | None — pre-existing | Confirmed via `git log -S"coming soon"`: introduced by Phase 6 (`e890c025`, DATA-07), not by Phase 11. Phase 11 only added to this file (light-theme tokens); this line is untouched by Phase 11's diff hunks. Not a Phase-11 debt marker. |

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers were introduced by any of the 43 non-test files this phase modified (`git diff 3ae54a74 HEAD --name-only -- app/ .github/ docs/`, filtered to non-test files, then grepped for debt markers this session): exactly 1 hit total, the pre-existing `styles.css:299` line addressed above.

No orphaned/leftover "coming soon" language selector remains in `SettingsDialog.tsx` — 11-01's placeholder was replaced by a real, functional `<select>` in 11-03 (confirmed by direct read, no stray "coming soon" text in that file).

### Cross-AI Review Trail (11-REVIEWS.md, read this session)

11-01/11-02 went through 3 codex review rounds; the final round's verdict was **CLEAR TO EXECUTE** with only a documentation-only follow-up (`11-RESEARCH.md` staleness — a supporting doc, not the plan text). 11-03 was **CLEAR TO EXECUTE** on first review. 11-04 went through 2 codex review rounds; the final verdict was **BLOCKED** on two residual items:
1. A stale threat-model cross-reference at `11-04-PLAN.md:410` still describing an "iterate the real `ErrorDto["code"]` union" approach that contradicts the plan's own (correct, executed) regex-derived-from-Rust-source approach. This is a planning-document inconsistency, not an implementation defect — `errors.test.ts` (run live this session) proves the actually-implemented regex-derived coverage test works and passes.
2. `CommandBar.tsx`'s `defaultPath` default-filename strings (lines 102/136/162 in the current file) remain outside the retrofit/completeness scope. Confirmed still true by direct read this session — these are default *filenames* offered to a native OS save dialog, not translated UI copy; per this verification's item 7 (format-literal tokens should NOT be localized), this is defensible, consistent behavior, not a functional gap. Flagged here for visibility since the plan review never formally closed it, but it does not block PLAT-03 ("all user-facing strings render translated") since a suggested filename is not conventionally considered translatable UI text (cross-platform filename portability reasons apply).

Neither residual item was found to represent an actual code defect upon independent inspection.

## Gaps Summary

No BLOCKER-level gaps. All 9 must-have truths are backed by evidence from tests run live in this session plus direct reads of the actual source (not SUMMARY.md self-reports). Zero new dependencies confirmed by diff, not by SUMMARY claim. Fail-closed signing wiring confirmed by reading `sign.ps1` and `release-app.yml` directly plus running the guard-test suite live.

The only substantive open item is **PLAT-02's literal ROADMAP text** ("Windows release binaries are Authenticode-signed") — the WIRING is fully proven correct and fail-closed, but no binary has actually been signed, because Azure Trusted Signing credentials do not exist in this repository. This is an explicit, honestly-documented scope boundary from `11-CONTEXT.md` (no credential provisioning is possible from within this environment) and is not something a code change can close — it requires an operator to provision `AZURE_CLIENT_ID`/`AZURE_CLIENT_SECRET`/`AZURE_TENANT_ID` + `ENABLE_MSI_SIGNING=true` and then run the documented `docs/signing.md` fail-closed check once. Routed to human_verification, not gaps, since the mechanism itself (what code review CAN verify) is sound.

Five additional items need a live, running Tauri window to observe (instant theme repaint, settings/language restart-persistence x2, real-OS-path corrupt-settings degradation, multi-surface locale re-render) — all mechanisms are proven by automated test against synthetic fixtures/mocked IPC, consistently flagged by all four SUMMARY files as intentionally out of headless-execution reach.

---

*Verified: 2026-08-16*
*Verifier: Claude (gsd-verifier)*
