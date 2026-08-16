/**
 * Locale display metadata for the language switcher (D11-02). Names are
 * each language's own standard, universally-known native/endonym name --
 * these are NOT translated app content and are not machine translation,
 * matching how any language-picker UI conventionally lists its options.
 * Fixed, stable order (mirrors res/locales/ coverage).
 */
export interface SupportedLocale {
  code: string;
  nativeName: string;
}

export const SUPPORTED_LOCALES: SupportedLocale[] = [
  { code: "en", nativeName: "English" },
  { code: "de", nativeName: "Deutsch" },
  { code: "es", nativeName: "Español" },
  { code: "fr", nativeName: "Français" },
  { code: "it", nativeName: "Italiano" },
  { code: "pl", nativeName: "Polski" },
  { code: "pt", nativeName: "Português" },
  { code: "ru", nativeName: "Русский" },
  { code: "uk", nativeName: "Українська" },
];
