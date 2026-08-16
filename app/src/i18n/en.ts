/**
 * The ONE complete catalog (D11-02, 11-03-PLAN.md Task 1). `StringKey`
 * (../i18n/strings.ts) is derived as `keyof typeof en` -- there is no
 * separate hand-maintained key list, so `en` cannot omit a key any other
 * code references without a compile error.
 *
 * Keys are dot-namespaced by owning surface (`app.*` for App.tsx's shell
 * chrome, `settings.*` for SettingsDialog.tsx). The product name
 * ("JWL Manager") is deliberately NOT a catalog entry -- proper nouns are
 * not translated.
 *
 * Mixed-markup convention: for a sentence whose middle segment is wrapped
 * in a literal JSX element (App.tsx's empty-state `.jwlibrary` sentence),
 * split it into a "before" and "after" key around the literal element
 * rather than inventing a markup-aware template language. Plan 11-04 reuses
 * this exact split for CommandBar's JSX-embedded summary sentences.
 *
 * Template values may contain a `{name}` token, substituted at call time by
 * `I18nContext.t(key, params)` (e.g. `settings.versionLine`'s `{version}`).
 */
export const en = {
  "app.settingsButton": "Settings…",
  "app.emptyState.title": "No archive open",
  "app.emptyState.bodyBefore": "Open a ",
  "app.emptyState.bodyAfter": " file to view your Notes, or create a new archive.",

  "settings.title": "Settings",
  "settings.themeLabel": "Theme",
  "settings.themeLight": "Light",
  "settings.themeDark": "Dark",
  "settings.languageLabel": "Language",
  "settings.aboutTitle": "About",
  "settings.versionLine": "Version {version}",
  "settings.closeButton": "Close",
} as const;
