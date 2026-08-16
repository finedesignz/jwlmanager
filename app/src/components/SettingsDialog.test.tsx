import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SettingsDialog from "./SettingsDialog";
import { SettingsProvider } from "../settings/SettingsProvider";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
    if (cmd === "save_settings") return Promise.resolve(undefined);
    if (cmd === "app_version") return Promise.resolve("0.9.3");
    return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
  });
});

function renderDialog() {
  return render(
    <SettingsProvider>
      <SettingsDialog onClose={vi.fn()} />
    </SettingsProvider>,
  );
}

describe("SettingsDialog (D11-03/D11-04, 11-01-PLAN.md Task 1)", () => {
  it("renders a theme control whose activation calls the theme setter (write-through to save_settings)", async () => {
    renderDialog();
    await screen.findByTestId("settings-dialog");

    fireEvent.click(screen.getByTestId("settings-dialog-theme-light"));

    expect(invokeMock).toHaveBeenCalledWith("save_settings", {
      settings: { language: "en", theme: "light" },
    });
    expect(screen.getByTestId("settings-dialog-theme-light")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("settings-dialog-theme-dark")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("renders a version string sourced from the runtime app_version command, never a literal", async () => {
    renderDialog();

    expect(await screen.findByTestId("settings-dialog-version")).toHaveTextContent("0.9.3");
    expect(invokeMock).toHaveBeenCalledWith("app_version");
  });
});
