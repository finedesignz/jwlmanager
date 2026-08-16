import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ThemeProvider, useTheme } from "./ThemeContext";
import type { Theme } from "../bindings/Theme";

function Consumer() {
  const { theme, setTheme } = useTheme();
  return (
    <div>
      <span data-testid="theme-value">{theme}</span>
      <button
        type="button"
        onClick={() => setTheme(theme === "light" ? "dark" : "light")}
        data-testid="toggle-theme"
      >
        Toggle
      </button>
    </div>
  );
}

function Harness({ initial }: { initial: Theme }) {
  const [theme, setTheme] = useState<Theme>(initial);
  return (
    <ThemeProvider theme={theme} setTheme={setTheme}>
      <Consumer />
    </ThemeProvider>
  );
}

describe("ThemeContext — DOM data-theme attribute effect (D11-03)", () => {
  it("setting the theme updates document.documentElement's data-theme dataset value, and setting it back updates it again", () => {
    render(<Harness initial="dark" />);
    expect(document.documentElement.dataset.theme).toBe("dark");

    fireEvent.click(screen.getByTestId("toggle-theme"));
    expect(document.documentElement.dataset.theme).toBe("light");

    fireEvent.click(screen.getByTestId("toggle-theme"));
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("mutates the attribute in place without unmounting/reloading the tree (no reload)", () => {
    render(<Harness initial="dark" />);
    const nodeBefore = screen.getByTestId("theme-value");

    fireEvent.click(screen.getByTestId("toggle-theme"));

    const nodeAfter = screen.getByTestId("theme-value");
    // Same DOM node reference across the theme change -- proves the switch
    // is a pure attribute mutation, never a remount/reload.
    expect(nodeAfter).toBe(nodeBefore);
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("useTheme throws when called outside a ThemeProvider", () => {
    function Bare() {
      useTheme();
      return null;
    }
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Bare />)).toThrow(/useTheme must be used within a ThemeProvider/);
    consoleSpy.mockRestore();
  });
});
