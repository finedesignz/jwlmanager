import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../settings/SettingsProvider";

interface SettingsDialogProps {
  /** Closes the dialog -- no mutation of its own beyond what the theme
   * control already wrote through immediately on click. */
  onClose: () => void;
}

const APP_NAME = "JWL Manager";

/**
 * Settings / About dialog (D11-03/D11-04, 11-01-PLAN.md Task 1) -- hosts the
 * theme switcher and a minimal About region. Follows the established
 * `TagDialog`/`FavoriteAddDialog` overlay + card conventions (reuses
 * `.edit-preview-overlay` for its backdrop).
 *
 * The Language field is a labelled, non-functional SLOT -- plan 11-03 fills
 * it with a real `<select>` wired to `I18nContext`. It deliberately does
 * NOT render a dropdown here: a dropdown with no working options would
 * appear live but do nothing, which is worse than an honest placeholder.
 */
export default function SettingsDialog({ onClose }: SettingsDialogProps) {
  const { theme, setTheme, language } = useSettings();
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
        aria-label="Settings"
        data-testid="settings-dialog"
      >
        <h2 className="settings-dialog-title">Settings</h2>

        <div className="settings-dialog-field">
          <label id="settings-dialog-theme-label">Theme</label>
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
              Light
            </button>
            <button
              type="button"
              className="toolbar-button settings-dialog-theme-option"
              aria-pressed={theme === "dark"}
              onClick={() => setTheme("dark")}
              data-testid="settings-dialog-theme-dark"
            >
              Dark
            </button>
          </div>
        </div>

        <div className="settings-dialog-field" data-testid="settings-dialog-language-slot">
          <label id="settings-dialog-language-label">Language</label>
          <p className="settings-dialog-coming-soon" aria-labelledby="settings-dialog-language-label">
            Currently: {language === "en" ? "English" : language}. More languages coming soon.
          </p>
        </div>

        <div className="settings-dialog-about" data-testid="settings-dialog-about">
          <h3 className="settings-dialog-about-title">About</h3>
          <p className="settings-dialog-about-line">{APP_NAME}</p>
          <p className="settings-dialog-about-line" data-testid="settings-dialog-version">
            Version {version ?? "..."}
          </p>
        </div>

        <div className="settings-dialog-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={onClose}
            data-testid="settings-dialog-close"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
