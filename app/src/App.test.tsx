import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({
    loaded: true,
    arch: "x86_64",
    version: "1.0.0",
    reason: null,
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

describe("App", () => {
  it("renders the empty-state shell with no archive open", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", { name: /no archive open/i }),
    ).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: /open archive/i }).length,
    ).toBeGreaterThan(0);
  });
});
