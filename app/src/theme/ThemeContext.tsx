import { createContext, useContext, useEffect, type ReactNode } from "react";
import type { Theme } from "../bindings/Theme";

interface ThemeContextValue {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

/**
 * D11-03: the ONLY DOM effect this context has is writing `theme` onto
 * `document.documentElement`'s `data-theme` dataset property -- the CSS
 * cascade (`:root[data-theme="light"]` in styles.css) is the ENTIRE
 * mechanism from there. No component reads or computes a colour value in
 * JS. Setting the attribute is a synchronous DOM write, so the whole window
 * repaints on the same frame with no reload.
 *
 * Deliberately a CONTROLLED component -- `theme`/`setTheme` are owned by
 * `SettingsProvider` (which write-throughs every change to
 * `save_settings`), not by this module. This context's only job is the DOM
 * attribute effect + exposing the pair to consumers below it, matching
 * 11-CONTEXT.md's key_links description of the two as separate concerns.
 */
export function ThemeProvider({
  theme,
  setTheme,
  children,
}: {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  children: ReactNode;
}) {
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  return <ThemeContext.Provider value={{ theme, setTheme }}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return ctx;
}
