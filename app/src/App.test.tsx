import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

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
