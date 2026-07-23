import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import CategorySwitcher from "./CategorySwitcher";
import type { Category } from "../bindings/Category";

const ALL: Category[] = [
  "Notes",
  "Bookmarks",
  "Favorites",
  "Highlights",
  "Annotations",
  "Playlists",
];

describe("CategorySwitcher (D6-06 enum-driven selector)", () => {
  it("renders one control per Category enum variant (six options)", () => {
    render(<CategorySwitcher active="Notes" onSelect={() => {}} />);
    for (const cat of ALL) {
      expect(
        screen.getByTestId(`category-switcher-option-${cat}`),
        `${cat} option should render`,
      ).toBeInTheDocument();
    }
    // exactly six, no more.
    expect(screen.getAllByRole("button")).toHaveLength(6);
  });

  it("marks the active variant as current (aria-pressed)", () => {
    render(<CategorySwitcher active="Highlights" onSelect={() => {}} />);
    expect(screen.getByTestId("category-switcher-option-Highlights")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("category-switcher-option-Notes")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("calls onSelect with the enum value when another option is chosen", () => {
    const onSelect = vi.fn();
    render(<CategorySwitcher active="Notes" onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId("category-switcher-option-Playlists"));
    expect(onSelect).toHaveBeenCalledWith("Playlists");
  });

  it("does not fire onSelect when the already-active option is clicked", () => {
    const onSelect = vi.fn();
    render(<CategorySwitcher active="Bookmarks" onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId("category-switcher-option-Bookmarks"));
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("disables all options when disabled prop is set", () => {
    const onSelect = vi.fn();
    render(<CategorySwitcher active="Notes" onSelect={onSelect} disabled />);
    const opt = screen.getByTestId("category-switcher-option-Favorites");
    expect(opt).toBeDisabled();
    fireEvent.click(opt);
    expect(onSelect).not.toHaveBeenCalled();
  });
});
