import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../bindings/AppSettings";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { Theme } from "../bindings/Theme";
import { ThemeProvider } from "../theme/ThemeContext";
import { I18nProvider } from "../i18n/I18nContext";
import ErrorBanner from "../components/ErrorBanner";

/** Built-in defaults (English + Dark) -- rendered until `load_settings`
 * resolves (the accepted one-frame flash, 11-CONTEXT.md D11-03/Claude's
 * Discretion) and whenever the load itself is unavailable. Mirrors
 * `AppSettings::default()` on the Rust side (settings.rs) -- kept as a
 * literal here rather than derived, since there is no synchronous way to
 * import a Rust default into TypeScript. */
const DEFAULT_SETTINGS: AppSettings = { language: "en", theme: "dark" };

interface SettingsContextValue {
  language: string;
  setLanguage: (language: string) => void;
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

/**
 * Wraps the app (D11-04): calls `load_settings` once on mount and adopts
 * the returned values; renders with the built-in defaults until that
 * resolves. Exposes `{ language, setLanguage, theme, setTheme }` and
 * wraps `children` in `ThemeProvider` so the DOM `data-theme` attribute
 * effect stays a separate, focused concern (`../theme/ThemeContext`) --
 * no component below this needs to know persistence exists at all.
 *
 * `I18nProvider` (11-03-PLAN.md Task 1) nests INSIDE `ThemeProvider`,
 * receiving `locale={settings.language}`/`setLocale={setLanguage}` as the
 * same kind of controlled-component props -- `setLocale` is a thin
 * call-through to the already-wired `setLanguage`, adding no new
 * persistence logic. This provider's own `saveError` `ErrorBanner`
 * instance lives INSIDE `I18nProvider` (alongside `{children}`) so every
 * `ErrorBanner` in the tree, including this one, renders through a t()-aware
 * `describeError` retrofit (plan 11-04).
 *
 * `updateSettings` is the ONLY write path -- every setter below calls
 * through it. It uses React's functional `setState(prev => ...)` form and
 * issues the `save_settings` call from INSIDE that same updater, using the
 * `next` value the updater itself just computed -- never a second read of
 * `settings` taken outside the updater. React invokes each queued
 * functional updater in order, chaining off the PREVIOUS updater's
 * committed result (even before either has re-rendered), so a theme change
 * and a language change fired back-to-back in the same tick still merge
 * correctly here and neither field's value is ever dropped
 * (11-01-PLAN.md Task 1).
 *
 * A rejected save is surfaced (Core Value: a silently-dropped settings
 * choice is a silent loss of the user's choice) via this provider's OWN
 * `ErrorBanner` instance -- the SAME component/`describeError` mapping the
 * rest of the app already uses for `ErrorDto`, reused here rather than
 * inventing a second error-display mechanism. It never rolls back the UI
 * state and never crashes.
 */
export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [saveError, setSaveError] = useState<ErrorDto | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<AppSettings>("load_settings")
      .then((loaded) => {
        if (cancelled) return;
        setSettings(loaded);
      })
      .catch(() => {
        // `load_settings` never rejects in production -- it absorbs every
        // failure into `AppSettings::default()` on the Rust side (D11-04).
        // A rejected promise here (e.g. IPC unavailable, or a test double)
        // must ALSO degrade silently to the built-in defaults already
        // rendered -- never a startup-blocking error surface.
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const updateSettings = useCallback((patch: Partial<AppSettings>) => {
    setSettings((prev) => {
      const next: AppSettings = { ...prev, ...patch };
      void invoke("save_settings", { settings: next }).catch((err) => {
        setSaveError(err as ErrorDto);
      });
      return next;
    });
  }, []);

  const setTheme = useCallback((theme: Theme) => updateSettings({ theme }), [updateSettings]);
  const setLanguage = useCallback(
    (language: string) => updateSettings({ language }),
    [updateSettings],
  );

  return (
    <SettingsContext.Provider
      value={{
        language: settings.language,
        setLanguage,
        theme: settings.theme,
        setTheme,
      }}
    >
      <ThemeProvider theme={settings.theme} setTheme={setTheme}>
        <I18nProvider locale={settings.language} setLocale={setLanguage}>
          {saveError && <ErrorBanner error={saveError} />}
          {children}
        </I18nProvider>
      </ThemeProvider>
    </SettingsContext.Provider>
  );
}

export function useSettings(): SettingsContextValue {
  const ctx = useContext(SettingsContext);
  if (!ctx) {
    throw new Error("useSettings must be used within a SettingsProvider");
  }
  return ctx;
}
