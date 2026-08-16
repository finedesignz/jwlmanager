import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../settings/SettingsProvider";
import { useI18n } from "../i18n/I18nContext";
import { SUPPORTED_LOCALES } from "../i18n/locales";

interface SettingsDialogProps {
  /** Closes the dialog -- no mutation of its own beyond what the theme
   * control already wrote through immediately on click. */
  onClose: () => void;
}

const APP_NAME = "JWL Manager";

/**
 * Settings / About dialog (D11-03/D11-04, 11-01-PLAN.md Task 1; language
 * select D11-02, 11-03-PLAN.md Task 1) -- hosts the theme switcher, the
 * language switcher, and a minimal About region. Follows the established
 * `TagDialog`/`FavoriteAddDialog` overlay + card conventions (reuses
 * `.edit-preview-overlay` for its backdrop).
 *
 * Every string in this component renders through `t()` -- the product name
 * ("JWL Manager") is the only literal, since proper nouns are not catalog
 * entries.
 */
export default function SettingsDialog({ onClose }: SettingsDialogProps) {
  const { theme, setTheme, language } = useSettings();
  const { t, setLocale } = useI18n();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<string>("app_version")
      .then((v) => {
        if (!cancelled) setVersion(v);
      })
      .catch(() => {
        // Non-critical display value -- leave the loading placeholder if
        // the command is unavailable rather than surfacing an error.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const handleOverlayClick = (event: React.MouseEvent<HTMLDivElement>) => {
    if (event.target === event.currentTarget) {
      onClose();
    }
  };

  return (
    <div className="edit-preview-overlay" role="presentation" onClick={handleOverlayClick}>
      <div
        className="settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={t("settings.title")}
        data-testid="settings-dialog"
      >
        <h2 className="settings-dialog-title">{t("settings.title")}</h2>

        <div className="settings-dialog-field">
          <label id="settings-dialog-theme-label">{t("settings.themeLabel")}</label>
          <div
            className="settings-dialog-theme-control"
            role="group"
            aria-labelledby="settings-dialog-theme-label"
          >
            <button
              type="button"
              className="toolbar-button settings-dialog-theme-option"
              aria-pressed={theme === "light"}
              onClick={() => setTheme("light")}
              data-testid="settings-dialog-theme-light"
            >
              {t("settings.themeLight")}
            </button>
            <button
              type="button"
              className="toolbar-button settings-dialog-theme-option"
              aria-pressed={theme === "dark"}
              onClick={() => setTheme("dark")}
              data-testid="settings-dialog-theme-dark"
            >
              {t("settings.themeDark")}
            </button>
          </div>
        </div>

        <div className="settings-dialog-field" data-testid="settings-dialog-language-slot">
          <label id="settings-dialog-language-label" htmlFor="settings-dialog-language-select">
            {t("settings.languageLabel")}
          </label>
          <select
            id="settings-dialog-language-select"
            className="settings-dialog-language-select"
            value={language}
            onChange={(event) => setLocale(event.target.value)}
            data-testid="settings-dialog-language-select"
          >
            {SUPPORTED_LOCALES.map(({ code, nativeName }) => (
              <option key={code} value={code}>
                {nativeName}
              </option>
            ))}
          </select>
        </div>

        <div className="settings-dialog-about" data-testid="settings-dialog-about">
          <h3 className="settings-dialog-about-title">{t("settings.aboutTitle")}</h3>
          <p className="settings-dialog-about-line">{APP_NAME}</p>
          <p className="settings-dialog-about-line" data-testid="settings-dialog-version">
            {t("settings.versionLine", { version: version ?? "…" })}
          </p>
        </div>

        <div className="settings-dialog-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={onClose}
            data-testid="settings-dialog-close"
          >
            {t("settings.closeButton")}
          </button>
        </div>
      </div>
    </div>
  );
}
