import { createContext, useContext, useMemo, type ReactNode } from "react";
import { en } from "./en";
import { de } from "./de";
import { es } from "./es";
import { fr } from "./fr";
import { it } from "./it";
import { pl } from "./pl";
import { pt } from "./pt";
import { ru } from "./ru";
import { uk } from "./uk";
import type { StringKey } from "./strings";

/** Every catalog keyed by locale code (D11-02). `en` is the only complete
 * one; the rest are the scaffolded `Partial<Record<StringKey, string>>`
 * files. An unrecognized locale code (hand-edited settings file, or a
 * locale not yet added) simply has no entry here, so `catalogs[locale]` is
 * `undefined` and every key falls through to `en` via the same expression
 * used for a missing key in a real locale -- one mechanism covers both
 * cases (T-11-12). */
const catalogs: Record<string, Partial<Record<StringKey, string>>> = {
  en,
  de,
  es,
  fr,
  it,
  pl,
  pt,
  ru,
  uk,
};

interface I18nContextValue {
  locale: string;
  setLocale: (locale: string) => void;
  t: (key: StringKey, params?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue | null>(null);

/**
 * Dependency-free i18n context (D11-02). Deliberately a CONTROLLED
 * component -- mirrors `ThemeProvider`'s `theme`/`setTheme` shape exactly:
 * `locale`/`setLocale` are owned by `SettingsProvider` (which
 * write-throughs every change to `save_settings`), not by this module.
 *
 * `t(key, params)` resolves `catalogs[locale]?.[key] ?? en[key]` -- the ONE
 * fallback expression every consumer relies on (key_links). When `params`
 * is given, every `{name}` token in the resolved template is replaced via a
 * bounded, single-pass regex substitution (T-11-14); an unmatched token is
 * left as the literal `{name}` rather than throwing.
 */
export function I18nProvider({
  locale,
  setLocale,
  children,
}: {
  locale: string;
  setLocale: (locale: string) => void;
  children: ReactNode;
}) {
  const t = useMemo(() => {
    return (key: StringKey, params?: Record<string, string | number>): string => {
      const template = catalogs[locale]?.[key] ?? en[key];
      if (!params) {
        return template;
      }
      return template.replace(/\{(\w+)\}/g, (match, token: string) => {
        const value = params[token];
        return value === undefined ? match : String(value);
      });
    };
  }, [locale]);

  return (
    <I18nContext.Provider value={{ locale, setLocale, t }}>{children}</I18nContext.Provider>
  );
}

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error("useI18n must be used within an I18nProvider");
  }
  return ctx;
}
