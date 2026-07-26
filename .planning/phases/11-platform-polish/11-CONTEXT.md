# Phase 11: Platform Polish (Signing, Localization, Theme) - Context

Gathered: 2026-07-26 (autonomous mode)
Status: Ready for planning

## Phase Boundary

The FINAL phase of the v1 milestone. A user gets a Windows binary that installs without
a SmartScreen warning, can switch the UI language and see translated strings, and can
switch between light and dark theme with the change applying immediately. This phase is
release engineering + UI settings, not new archive functionality -- it touches the build
pipeline and adds the app's first-ever persisted (non-archive) state.

In scope (PLAT-02, PLAT-03, PLAT-04, literally as ROADMAP states them):
- Windows Authenticode signing via Azure Trusted Signing, wired into bundle.windows.signCommand
  so signing runs DURING tauri build bundling, not as a post-build pass.
- CI wiring: repo secrets AZURE_CLIENT_ID/AZURE_CLIENT_SECRET/AZURE_TENANT_ID + repo
  variable ENABLE_MSI_SIGNING=true, consumed by .github/workflows/app-ci.yml (or a new
  release workflow -- see D11-01).
- A minimal, dependency-free i18n layer for the NEW Tauri/React UI's OWN strings (see D11-02
  -- this is NOT the same string set as the old PySide6 app's gettext catalogs), English
  complete, architecture ready for additional locales.
- A language switcher UI affordance (settings surface) that swaps the active catalog and
  re-renders without reload.
- A light theme CSS variant added alongside the existing (currently dark-only) :root token
  set in app/src/styles.css, toggled at runtime via a data-theme attribute, applying
  instantly across the whole app (D11-03).
- A theme switcher UI affordance.
- The app's first persisted settings surface (language + theme choice, durable across
  restarts) using a NEW small Rust-side settings file under Tauri's app_data_dir, written
  with already-declared serde_json + Rust std fs -- zero new Cargo or npm dependencies
  (D11-04).
- A minimal About surface (app name, version read at RUNTIME via env!("CARGO_PKG_VERSION")
  -- never hardcoded, matching the existing app_version pattern already used 10+ places in
  lib.rs) reachable from the settings/menu surface, since it is cheap and a natural home
  for the language/theme switchers anyway (Claude's Discretion, see below).

Out of scope (own follow-up milestone, or explicitly not required by ROADMAP's 3 literal
success criteria -- scope discipline is this phase's sharpest risk, see Integration
Point/risk):
- Porting the Python app's nine gettext catalogs verbatim -- they translate the OLD
  PySide6 UI's strings, which are largely NOT the same strings as the rewritten React UI
  (different component structure, different copy). "Parity" here means the NEW UI has its
  own translatable string set, not a byte-for-byte catalog port (D11-02).
- Translating the other 8 languages' content for the new UI's actual strings -- no
  translator resource exists in this environment to produce correct German/Spanish/French/
  Italian/Polish/Portuguese/Russian/Ukrainian text for brand-new UI copy; shipping fabricated
  machine-translated strings for a "years of irreplaceable personal study notes" app would be
  a quality regression, not parity. English-complete + ready-to-extend structure only.
- Full custom brand icon set / logo redesign -- existing Tauri-scaffold icons
  (app/src-tauri/icons/*) already exist and function; a bespoke brand icon is a design
  task, not a PLAT-02/03/04 requirement, and is not implied by any of the 3 success criteria.
- Auto-update pipeline (tauri-plugin-updater, update server, signing-key generation/
  distribution) -- large commitment (new dependency needing a legitimacy checkpoint, plus
  server infrastructure this project has none of), not mentioned by any Phase 11 success
  criterion. Explicit follow-up milestone item.
- Persisted window geometry, persisted last-selected category, persisted sort-column,
  duplicate-notes CTE filter, grouping/tree hierarchy, title-view modes -- these were
  informally deferred here by phases 6/7/9/10 as "polish/Phase 11" language, but NONE of
  them appear in ROADMAP's 3 literal Phase 11 success criteria. Bundling them in risks the
  exact "bloated final phase that never converges" failure mode the team lead flagged.
  Deferred again, explicitly, to a genuine post-1.0 follow-up milestone.
- macOS notarization -- needs an Apple Developer certificate, which does not exist in this
  environment's credential set (Azure Trusted Signing covers Windows only). If macOS
  distribution without Gatekeeper friction is desired, that is a new milestone item needing
  a credential this phase cannot self-serve.
- Any change to archive read/write/merge/import/export logic -- this phase is release
  engineering + a settings/theme layer, never the archive data path.

Requirements: PLAT-02, PLAT-03, PLAT-04 (ROADMAP Phase 11; REQUIREMENTS.md:83-85, 158-160).

Depends on: Phase 1 (CI skeleton at .github/workflows/app-ci.yml with no signing steps --
D-17/PLAT-02 explicitly deferred here per 01-CONTEXT.md:105; app/src/styles.css token
system; app_version-at-runtime pattern already established in lib.rs). Also draws on
every phase's "Localization, theme -> Phase 11" deferral note (06/07/08/09/10-CONTEXT.md) --
this phase is the closure point for all of them, but only for the 3 literal criteria (see
scope-discipline note above).

## Implementation Decisions

Auto-selected; recommended default per gray area; rationale for audit.

### Central question: Windows code signing wiring (criterion 1)

D11-01 (signing runs via Tauri's bundle.windows.signCommand during tauri build, using
Azure Trusted Signing's CLI/signtool-wrapper invoked as the sign command, never a post-build
signtool pass): this is a hard global project rule, not a phase-local choice -- CLAUDE.md
rule 22a states the post-build-pass ordering bug explicitly ("signs AFTER the updater .sig
is computed and silently breaks updater verification") and names the exact account/cert/RG/
tenant to use. Reference implementation: remo-code/.github/workflows/release-supervisor.yml
+ supervisor/tauri/signing/ (a sibling repo in this same GitHub root, already shipping this
pattern in production).
[auto] signing mechanism -> Selected: tauri.conf.json bundle.windows.signCommand wired
to Azure Trusted Signing, invoked from a NEW release-oriented CI workflow leg (or extending
app-ci.yml's existing matrix -- Claude's Discretion on which file) gated behind
ENABLE_MSI_SIGNING=true (recommended default, per global rule)
Rationale: this is not actually a gray area -- the global rule is prescriptive and gives
both the exact hazard to avoid and a working reference implementation in this same
environment. Re-deriving a different signing approach would contradict an explicit,
evidence-backed project standard.
Note for the planner: no per-app cert exists yet for com.titaniumlabs.jwlmanager --
confirm with remo-code's reference whether the same titaniumlabs-signing account/
TitaniumLabsLLC cert profile is reused as-is (recommended -- one certificate profile,
many signed products, matching how Trusted Signing is designed) rather than provisioning
a new profile.

### Localization scope and mechanism (criterion 2)

D11-02 (build a minimal, dependency-free i18n layer for the NEW UI's own strings; ship
English complete; structure ready for future locales; do NOT attempt to port the Python
app's nine gettext catalogs as-is): verified against the actual catalog content, not
assumed -- res/locales/en.pot has 130 msgids, all sourced from JWLManager.py line
references (PySide6 dialog/menu/label text). The rewritten React UI (app/src/components/
*.tsx) has its own, largely different component structure and copy -- there is no 1:1
string mapping to port. "Localization parity" for THIS UI necessarily means building a new
translatable string catalog for the strings this UI actually renders, not replaying the old
.po files. No i18n npm library (react-i18next, formatjs, etc.) can be legitimacy-cleared
in this environment (08-RESEARCH.md's addendum: the live npm registry check is unavailable)
-- so the mechanism is a plain TypeScript Record<string, string> catalog per locale (e.g.
app/src/i18n/en.ts, app/src/i18n/strings.ts exporting a typed key union) plus a small
React context providing a t(key) lookup function and a setLocale/locale pair, zero
runtime dependencies beyond React itself (already present).
[auto] localization scope -> Selected: dependency-free custom catalog + context, English
complete, other 8 locale files scaffolded with the SAME keys but placeholder/untranslated
values clearly marked (falling back to English at runtime when a key is missing), NOT
machine-translated (recommended default)
Rationale: directly satisfies criterion 2's literal text ("all user-facing strings render
translated") for the shipped locale (English) while keeping the architecture genuinely
extensible -- adding a real translated locale later is then a pure content-file addition,
no code change. Fabricating machine-translated strings for the other 8 locales would create
a false impression of completeness for a personal-data app where users read every word
carefully (Core Value adjacency: mistranslated UI around irreplaceable notes is a trust
problem even though it is not data corruption). If real translations later become available
(e.g. community-contributed, matching how the existing .po files list a human translator
Eryk J. <infiniti@inventati.org>), they drop into the same catalog shape.
[auto] locale coverage -> Selected: English-complete + extensible structure only; the other
8 languages are a follow-up milestone item requiring a translation resource this environment
does not have (recommended default)
Rationale: matches the team lead brief's own framing ("ship an i18n framework with English
only and the structure for the rest") and avoids the two rejected alternatives it named
(porting catalogs 1:1 doesn't fit the new UI's string set; a new npm i18n dependency is a
blocking checkpoint this phase should not force).

### Theme switching (criterion 3)

D11-03 (add a light-theme CSS variant using the SAME custom-property names already defined
in app/src/styles.css's :root block, toggled via a data-theme attribute on <html> or
<body>, applied instantly with zero re-render cost since it is pure CSS cascade): verified
against the actual stylesheet, not assumed -- app/src/styles.css:1-15 defines exactly 9
color tokens (--bg-primary, --bg-secondary, --bg-tertiary, --brand-primary,
--destructive, --text-primary, --text-muted, --border-hairline) plus spacing tokens,
ALL currently dark values, with color-scheme: dark hardcoded at styles.css:17 and used
by every component (19 total -- custom-property definitions/comments confirmed via grep).
There is no existing light variant anywhere in the Tauri app (the Python app's
res/light.qss/res/dark.qss are Qt stylesheets for a different UI toolkit and are not
directly portable CSS, but ARE useful as a REFERENCE for what light-mode color values the
project has historically chosen).
[auto] theme mechanism -> Selected: [data-theme="light"] selector overriding the
same 9 tokens, data-theme attribute set on document.documentElement, persisted choice
(D11-04) restored on app start before first paint if feasible (or accepted first-paint-dark-
then-flip if not -- Claude's Discretion on the exact anti-flash technique) (recommended
default)
Rationale: the entire app already consumes these 9 tokens exclusively (no component hardcodes
a raw color per the established pattern) -- a CSS-only override is the minimal, correct
mechanism with zero JS-driven re-render and zero new dependency. color-scheme itself should
become dynamic too (light dark or swapped per-theme) so native form controls/scrollbars
match.
Rationale for light-value sourcing: use res/light.qss as a starting palette reference (not
a literal port -- Qt stylesheet color roles do not map 1:1 to these 9 tokens) so the light
theme feels like a deliberate continuation of the project's existing visual identity rather
than an invented palette; final values are a design judgment call for the UI-SPEC/implementer,
not locked here.

### Persisted settings surface (new risk surface, supports criteria 2+3)

D11-04 (a NEW Tauri command pair load_settings/save_settings reading/writing a single
small JSON file under app_handle.path().app_data_dir() -- e.g. settings.json -- using
Rust std fs + the already-declared serde_json dependency; zero new Cargo or npm
dependency): Phase 9 verified the app currently persists NOTHING (no app_data_dir usage
anywhere in the codebase, confirmed again here via grep across app/src and
app/src-tauri/src -- zero hits). This phase introduces the FIRST persistent app state,
which is explicitly flagged by the team lead as a new risk surface requiring confirmation it
cannot corrupt or leak into the user's .jwlibrary archive.
[auto] persistence mechanism -> Selected: bespoke Rust command using tauri::Manager::
path().app_data_dir() (a path Tauri computes independently of any user-chosen archive
path) + std::fs::write/read_to_string + serde_json::to_string/from_str, gated by a
typed SettingsError (mirrors ArchiveError conventions) rather than reusing
ArchiveError itself (keeps the settings surface a visibly separate, lower-stakes error
domain from archive data) (recommended default)
Rationale: satisfies the "cannot corrupt or leak into the archive" requirement by
construction -- app_data_dir() is a fixed OS-appdata location entirely independent of
ArchiveSession.temp_dir/db_path, the settings file is never zipped, never opened as
SQLite, and the archive-save path (archive::save::atomic_replace) is never touched by this
code. Using existing serde_json (already a Cargo dependency, preserve_order feature not
needed here -- a plain Deserialize/Serialize struct suffices) means this is the ONLY
Phase 11 decision that could have needed a new dependency (a settings/store plugin) and does
not.
[auto] persisted keys -> Selected: { language: String, theme: "light" | "dark" } ONLY --
window geometry, last-category, sort-column explicitly excluded per the scope-discipline
call in Phase Boundary (recommended default)
Rationale: these two keys are the minimum needed for criteria 2 and 3 to mean "switch" in a
durable, real-world-daily-use sense (the ROADMAP goal text: "comfortable for real-world
daily use") without importing the broader settings surface (geometry/last-category/sort)
that was never a literal success criterion and that several phases only mentioned in passing
as a "nicety."

### Claude's Discretion
Exact anti-flash-of-wrong-theme technique on startup (inline <script> in index.html
reading a synchronously-available value vs. accepting a one-frame flash before the async
Tauri command resolves -- both are minor, neither touches archive safety); whether the
settings command pair lives in a new app/src-tauri/src/settings.rs module or is added to
an existing module (recommend new module, mirrors the project's one-concern-per-file
convention); exact locale-catalog file layout (app/src/i18n/<locale>.ts per file vs. one
file with nested locale keys -- recommend per-file, mirrors res/locales/<lang>/ precedent);
whether the About surface is a new dialog component or a tab within an existing
settings/menu surface (recommend a new minimal SettingsDialog/AboutDialog pair, reusing
existing dialog component conventions from TagDialog.tsx/FavoriteAddDialog.tsx); which
CI workflow file hosts the signing leg (extend app-ci.yml's existing matrix vs. a new
release.yml mirroring the Python app's .github/workflows/release.yml naming -- lean new
release workflow since signing should only run on actual release builds/tags, not every PR
push, but this needs the CI/release-cut convention this repo settles on to be confirmed
against remo-code's reference before locking); exact light-theme color values (design
judgment, reference res/light.qss per D11-03's rationale, finalize in UI-SPEC).

## Canonical References -- downstream agents MUST read

### Signing (D11-01)
- Global CLAUDE.md rule 22a -- the complete, authoritative signing spec (account, cert
  profile, RG, tenant, CI secrets/variable names, the post-build-pass hazard).
- remo-code/.github/workflows/release-supervisor.yml + remo-code/supervisor/tauri/
  signing/ -- working reference implementation, same GitHub root, same signing account.
- app/src-tauri/tauri.conf.json -- current bundle block (no windows.signCommand yet);
  this is the file the planner must extend.
- .planning/phases/01-open-view-save-foundation-slice/01-02-PLAN.md:151 and
  01-CONTEXT.md:105-106 -- confirms Phase 1 deliberately shipped CI with zero signing
  steps, deferring PLAT-02 here in full.

### Localization (D11-02)
- res/locales/en.pot (130 msgids), res/locales/{de,es,fr,it,pl,pt,ru,uk}/LC_MESSAGES/
  messages.po -- the OLD app's catalogs; reference for what KINDS of strings existed
  (dialog titles, error messages, menu labels) and prior translator attribution, not a
  literal source to port.
- app/src/components/*.tsx -- the actual string set that needs a NEW catalog; every
  hardcoded user-facing string across these ~20 components is the real Phase 11 i18n
  inventory (a planner task should enumerate them, not assume the Python list applies).
- .planning/phases/01-open-view-save-foundation-slice/01-04-SUMMARY.md:43 -- confirms
  "UI language hardcoded to 'en'" is a deliberate, recorded Phase 1 deferral, exactly the
  gap this phase closes.

### Theme (D11-03)
- app/src/styles.css:1-17 -- the 9-token :root block + color-scheme: dark to make
  dynamic; the file's own inline comments (388,508-509,571,846) show every component
  already references tokens by name, confirming the CSS-only override approach is safe.
- res/light.qss / res/dark.qss -- Qt stylesheet palette reference for light-theme color
  choices (not a literal port target).

### Persisted settings (D11-04)
- app/src-tauri/Cargo.toml:16-42 -- current dependency list; confirms serde/serde_json
  already present and sufficient, no new dependency needed for this decision.
- app/src-tauri/src/lib.rs (10+ app_version: env!("CARGO_PKG_VERSION") sites) -- the
  established runtime-version pattern to reuse verbatim for the About surface, never
  hardcode a version string.
- .planning/phases/09-incremental-export/09-CONTEXT.md (and the team lead brief) --
  confirms zero app_data_dir usage exists anywhere in the codebase today; this phase is
  the first to introduce it.

### Source of truth for the underlying ask
- ROADMAP.md Phase 11 section (.planning/ROADMAP.md:232-245) -- goal and all 3 success
  criteria, quoted in Phase Boundary above. Note Mode: mvp and UI hint: yes.
- REQUIREMENTS.md:83-85 -- PLAT-02/03/04 literal text; PLAT-01 (Phase 1, already shipped)
  is the cross-platform build baseline this phase signs on top of.

## Existing Code Insights
- The app currently has exactly ONE first-party plugin dependency (@tauri-apps/plugin-
  dialog / tauri-plugin-dialog) -- confirming the project's demonstrated pattern of
  minimal dependency surface holds even for official Tauri plugins; D11-04's zero-new-
  dependency settings approach is consistent with, not a departure from, that pattern.
- app/src/styles.css has NO existing @media (prefers-color-scheme) query and NO
  existing light values anywhere in the file (grep confirmed) -- the light theme is being
  authored from scratch, not toggling a dormant existing variant.
- The gettext .po files show zero fuzzy markers in the sampled locales (de, fr) and a
  fuzzy flag only on the .pot template header itself -- the OLD app's translations were
  apparently completed/reviewed, which is exactly why fabricating NEW machine-translated
  strings for the rewritten UI (rather than waiting for real translation work) would be a
  visible quality regression against the project's own history (informs D11-02).
- app/src-tauri/icons/* already contains a full icon set (32x32, 128x128, 128x128@2x,
  .icns, .ico, .png) from the Tauri scaffold -- a distributable icon set already exists;
  custom rebranding is a nice-to-have, not a blocker for any of the 3 success criteria.

## Established Patterns
- Typed errors (ArchiveError/ErrorDto-style), never unwrap/panic -- extend the same
  discipline to the new SettingsError domain (D11-04).
- CSS custom properties as the SOLE color mechanism, no hardcoded colors in components --
  the exact property already in place that makes D11-03's CSS-only theme switch safe.
- Runtime version via env!("CARGO_PKG_VERSION"), never hardcoded -- reuse verbatim for
  About.
- Synthetic fixtures only where tests touch persisted settings (never a real user's OS
  app-data directory in tests -- use a tempdir-overridden path in Rust tests).
- No new Cargo or npm dependency without an explicit legitimacy checkpoint -- this phase is
  designed to need NONE (D11-01 uses Azure CLI tooling in CI, not a Cargo/npm package;
  D11-02/03/04 are all hand-rolled with existing dependencies).

## Integration Point / risk
- **Scope convergence is the phase's central risk**, not any single technical decision --
  this is the terminal phase of the milestone and every prior phase dumped a "-> Phase 11"
  deferral note here. The Phase Boundary section above draws the line deliberately tight
  around the 3 literal ROADMAP criteria; the planner must resist re-absorbing window-
  geometry/last-category/sort-column/grouping/auto-update/custom-branding scope that keeps
  getting informally attributed to "Phase 11" in prior CONTEXT files but was never in the
  ROADMAP criteria text itself.
- **New persistent-state surface (D11-04) is the sharpest Core-Value-adjacent risk**: this
  is the first code in the entire app that writes to disk outside an explicit user-
  initiated archive save. It MUST use app_data_dir() (OS-managed, independent of any
  user-chosen archive path) and MUST be covered by a test asserting a settings read/write
  failure degrades gracefully (falls back to English/dark defaults) rather than blocking
  app startup or touching any ArchiveSession state.
- **Post-build signing ordering bug (D11-01)** is a known, documented failure mode
  (updater .sig computed before signing corrupts update verification) -- even though this
  project has no updater (auto-update is explicitly out of scope), get the signCommand
  wiring right now so a future auto-update follow-up milestone does not inherit a
  mis-ordered pipeline that has to be re-discovered and fixed later.
- **Localization catalog drift**: because there is no existing catalog for the NEW UI's
  strings, the biggest execution risk is under-inventorying hardcoded strings across ~20
  components. The planner should treat "grep every .tsx for JSX text nodes and string
  literal props (aria-label, title, placeholder)" as a concrete task, not an assumption.

## Specific Ideas
- Command surface: load_settings(app: AppHandle) -> Result<AppSettings, ErrorDto> and
  save_settings(app: AppHandle, settings: AppSettings) -> Result<(), ErrorDto>, mirroring
  the existing typed-command convention; AppSettings { language: String, theme: Theme }
  with Theme as a ts-rs-exported enum (Light/Dark) matching the project's existing
  ts-rs-generated-types pattern for other DTOs.
- A single SettingsProvider React context wrapping App.tsx, exposing { locale, setLocale,
  theme, setTheme, t }, loading persisted settings once on mount (calling load_settings)
  and writing through on every change (calling save_settings), so no component needs to
  know persistence exists.
- CI: extend app-ci.yml's Windows legs (or a new tag-triggered release.yml) with a
  signing step gated on ENABLE_MSI_SIGNING == 'true' AND the Azure secrets being present,
  so unsigned local/PR builds still work without the secrets configured.

## Constraints in force (project)
- Never lose or corrupt a user's archive (Core Value) -- the new persisted-settings surface
  must remain provably isolated from ArchiveSession/.jwlibrary state (see Integration
  Point/risk).
- No new Cargo or npm dependency without an explicit legitimacy checkpoint -- this phase's
  decisions (D11-01..D11-04) are each designed to need none; if implementation reveals one
  IS genuinely required (e.g. Azure's CLI tooling turns out to need an npm-side helper),
  that is a blocking checkpoint, not an assumption to route around silently.
- Typed errors, never unwrap/panic. Synthetic fixtures only, including any settings-
  persistence tests. MIT licence -- jwlCore binary only, no jwlFusion/NOASSERTION ingestion.
- ASCII-only punctuation in any user-facing document text produced by this phase's planning
  artifacts.
- Windows signing MUST run during Tauri bundling via signCommand, never a post-build pass
  (CLAUDE.md rule 22a, non-negotiable).

## Deferred Ideas
- Full 8-language translation of the new UI's actual strings -> follow-up milestone,
  pending a real translation resource (D11-02).
- Persisted window geometry, last-selected category, sort-column persistence, grouping/
  tree hierarchy, title-view modes, duplicate-notes CTE filter -> follow-up milestone; not
  literal Phase 11 success criteria despite being informally called "Phase 11" by several
  prior phases.
- Custom brand icon/logo redesign -> follow-up milestone; existing scaffold icons are
  functional and sufficient for 1.0.
- Auto-update pipeline (tauri-plugin-updater, signing-key distribution, update server) ->
  follow-up milestone; large new dependency + infrastructure commitment, not implied by any
  Phase 11 success criterion.
- macOS notarization -> follow-up milestone; needs an Apple Developer credential this
  environment does not have.

---

Phase: 11-Platform-Polish
Context gathered: 2026-07-26
