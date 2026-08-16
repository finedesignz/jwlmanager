import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SettingsProvider, useSettings } from "./SettingsProvider";
import type { AppSettings } from "../bindings/AppSettings";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function Consumer() {
  const { theme, setTheme, language, setLanguage } = useSettings();
  return (
    <div>
      <span data-testid="current-theme">{theme}</span>
      <span data-testid="current-language">{language}</span>
      <button type="button" data-testid="set-light" onClick={() => setTheme("light")}>
        Light
      </button>
      <button type="button" data-testid="set-de" onClick={() => setLanguage("de")}>
        German
      </button>
      <button
        type="button"
        data-testid="set-both"
        onClick={() => {
          setTheme("light");
          setLanguage("de");
        }}
      >
        Both
      </button>
    </div>
  );
}

function renderWithConsumer() {
  return render(
    <SettingsProvider>
      <Consumer />
    </SettingsProvider>,
  );
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("SettingsProvider — load (D11-04)", () => {
  it("invokes load_settings on mount and adopts the returned theme/language (tolerates StrictMode double-invoke in dev)", async () => {
    const loaded: AppSettings = { language: "fr", theme: "light" };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_settings") return Promise.resolve(loaded);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    renderWithConsumer();

    await screen.findByText("fr");
    expect(screen.getByTestId("current-theme")).toHaveTextContent("light");

    // Not asserting a raw call count of exactly one -- React StrictMode's
    // intentional double-invoke in development is tolerated here, only the
    // ADOPTED values matter for this behaviour.
    const loadCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "load_settings");
    expect(loadCalls.length).toBeGreaterThanOrEqual(1);
    expect(loadCalls.length).toBeLessThanOrEqual(2);
  });
});

describe("SettingsProvider — defaults on load rejection (D11-04)", () => {
  it("renders with the built-in defaults and shows NO error surface when load_settings rejects", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_settings") return Promise.reject(new Error("boom"));
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    renderWithConsumer();

    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("load_settings"));

    expect(screen.getByTestId("current-language")).toHaveTextContent("en");
    expect(screen.getByTestId("current-theme")).toHaveTextContent("dark");
    expect(screen.queryByTestId("error-banner")).not.toBeInTheDocument();
  });
});

describe("SettingsProvider — write-through (D11-04)", () => {
  it("changing the theme invokes save_settings once with both fields present", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
      if (cmd === "save_settings") return Promise.resolve(undefined);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    renderWithConsumer();
    await screen.findByTestId("current-theme");
    invokeMock.mockClear();

    fireEvent.click(screen.getByTestId("set-light"));

    expect(invokeMock).toHaveBeenCalledWith("save_settings", {
      settings: { language: "en", theme: "light" },
    });
    const saveCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "save_settings");
    expect(saveCalls.length).toBe(1);
  });

  it("a rejected save surfaces via the shared ErrorBanner without rolling back the UI state (Core Value: never silently drop a user choice)", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
      if (cmd === "save_settings") {
        return Promise.reject({
          code: "settings_write_failed",
          operation: "save_settings",
          safe_file_name: null,
          message_key: "error.settings.write_failed",
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    renderWithConsumer();
    await screen.findByTestId("current-theme");

    fireEvent.click(screen.getByTestId("set-light"));

    await screen.findByTestId("error-banner");
    expect(screen.getByTestId("current-theme")).toHaveTextContent("light");
  });
});

describe("SettingsProvider — concurrent write-through (D11-04)", () => {
  it("firing a theme change and a language change in the same tick merges both into the LAST save_settings call", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
      if (cmd === "save_settings") return Promise.resolve(undefined);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    renderWithConsumer();
    await screen.findByTestId("current-theme");
    invokeMock.mockClear();

    fireEvent.click(screen.getByTestId("set-both"));

    const saveCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "save_settings");
    expect(saveCalls.length).toBe(2);
    const lastCall = saveCalls[saveCalls.length - 1];
    expect(lastCall[1]).toEqual({ settings: { language: "de", theme: "light" } });
  });
});
