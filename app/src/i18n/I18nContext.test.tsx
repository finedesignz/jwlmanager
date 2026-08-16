import { useState } from "react";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider, useI18n } from "./I18nContext";
import { en } from "./en";

function Consumer() {
  const { t } = useI18n();
  return (
    <div>
      <span data-testid="settings-button">{t("app.settingsButton")}</span>
      <span data-testid="version-with-param">
        {t("settings.versionLine", { version: "1.2.3" })}
      </span>
      <span data-testid="version-no-param">{t("settings.versionLine")}</span>
    </div>
  );
}

function Harness({ initialLocale }: { initialLocale: string }) {
  const [locale, setLocale] = useState(initialLocale);
  return (
    <I18nProvider locale={locale} setLocale={setLocale}>
      <Consumer />
    </I18nProvider>
  );
}

/**
 * D11-02, 11-03-PLAN.md Task 2. Task 1 (a `type="tracer"` task) already
 * built and wired the full `I18nProvider`/`useI18n` mechanism end to end
 * through App.tsx and SettingsDialog.tsx -- these tests prove the
 * mechanism, they do not drive new implementation (mirrors 11-01-PLAN.md
 * Task 3's precedent for a tracer-first plan: see TDD Gate Compliance in
 * the SUMMARY for why there is no separate RED commit here).
 */
describe("I18nContext (D11-02, 11-03-PLAN.md Task 1)", () => {
  it("I18nContext fallback: a key present in en but absent from a locale's (empty) catalog resolves to English when that locale is active", () => {
    render(<Harness initialLocale="de" />);
    expect(screen.getByTestId("settings-button")).toHaveTextContent(
      en["app.settingsButton"],
    );
  });

  it("I18nContext unknown locale: a wholly unregistered locale code resolves every key to English and does not throw", () => {
    expect(() => render(<Harness initialLocale="xx" />)).not.toThrow();
    expect(screen.getByTestId("settings-button")).toHaveTextContent(
      en["app.settingsButton"],
    );
  });

  it("I18nContext param substitution: a {token} is replaced when a matching param is given, and left literal when the template carries no params", () => {
    render(<Harness initialLocale="en" />);
    expect(screen.getByTestId("version-with-param")).toHaveTextContent("Version 1.2.3");
    expect(screen.getByTestId("version-no-param")).toHaveTextContent("Version {version}");
  });

  it("useI18n throws when called outside an I18nProvider", () => {
    function Bare() {
      useI18n();
      return null;
    }
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Bare />)).toThrow(/useI18n must be used within an I18nProvider/);
    consoleSpy.mockRestore();
  });
});
