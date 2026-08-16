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

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("save_settings", {
        settings: { language: "en", theme: "light" },
      });
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

    await vi.waitFor(() => {
      const calls = invokeMock.mock.calls.filter(([cmd]) => cmd === "save_settings");
      expect(calls.length).toBe(2);
    });
    const saveCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "save_settings");
    const lastCall = saveCalls[saveCalls.length - 1];
    expect(lastCall[1]).toEqual({ settings: { language: "de", theme: "light" } });
  });

  it("[F1] persists BOTH changes even when the FIRST save_settings IPC call resolves AFTER the second (out-of-order completion)", async () => {
    // `diskState` models the actual file on disk: it is overwritten
    // whenever an IPC call's promise SETTLES, not when it is issued --
    // exactly like the real `save_settings` Rust command running on a
    // blocking thread pool with no ordering guarantee. `resolvers` collects
    // one settle-function per `save_settings` invoke call, in the order
    // those calls are ISSUED (which may differ from settle order below).
    let diskState: Record<string, unknown> | undefined;
    const resolvers: Array<() => void> = [];

    invokeMock.mockImplementation((cmd: string, args?: { settings: AppSettings }) => {
      if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
      if (cmd === "save_settings") {
        const settings = args!.settings as unknown as Record<string, unknown>;
        return new Promise<void>((resolve) => {
          resolvers.push(() => {
            diskState = settings;
            resolve();
          });
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    renderWithConsumer();
    await screen.findByTestId("current-theme");
    invokeMock.mockClear();
    diskState = undefined;
    resolvers.length = 0;

    // Fire the theme change, then the language change back-to-back.
    fireEvent.click(screen.getByTestId("set-light"));
    fireEvent.click(screen.getByTestId("set-de"));

    // A single-flight (fixed) implementation issues the first save
    // asynchronously (chained off a resolved promise), so wait for at
    // least one `save_settings` call to actually be issued before deciding
    // which race scenario (serialized vs. fire-and-forget) applies.
    await vi.waitFor(() => {
      expect(resolvers.length).toBeGreaterThanOrEqual(1);
    });

    if (resolvers.length === 1) {
      // Serialized (fixed) implementation: the second save is only issued
      // after the first settles. Settle the first, let the chain issue the
      // second, then settle that.
      resolvers[0]();
      await vi.waitFor(() => {
        expect(resolvers.length).toBe(2);
      });
      resolvers[1]();
    } else {
      // Fire-and-forget (buggy) implementation: both saves are already
      // in flight. Settle OUT OF ORDER -- the newer (second/language) call
      // lands on disk first, then the older (first/theme) call lands LAST,
      // reproducing the write-ordering race.
      expect(resolvers.length).toBe(2);
      resolvers[1]();
      await Promise.resolve();
      resolvers[0]();
    }
    await Promise.resolve();
    await Promise.resolve();

    // Whatever landed on disk LAST must reflect BOTH changes -- theme AND
    // language. A correct (single-flight, order-preserving) save path never
    // lets a stale, older write land after a newer one.
    expect(diskState).toEqual({ language: "de", theme: "light" });
  });
});

describe("SettingsProvider — stale error banner (F4)", () => {
  it("clears the save-error banner after a subsequent successful save", async () => {
    let shouldFail = true;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
      if (cmd === "save_settings") {
        if (shouldFail) {
          return Promise.reject({
            code: "settings_write_failed",
            operation: "save_settings",
            safe_file_name: null,
            message_key: "error.settings.write_failed",
          });
        }
        return Promise.resolve(undefined);
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    renderWithConsumer();
    await screen.findByTestId("current-theme");

    fireEvent.click(screen.getByTestId("set-light"));
    await screen.findByTestId("error-banner");

    shouldFail = false;
    fireEvent.click(screen.getByTestId("set-de"));

    await vi.waitFor(() => {
      expect(screen.queryByTestId("error-banner")).not.toBeInTheDocument();
    });
  });
});
