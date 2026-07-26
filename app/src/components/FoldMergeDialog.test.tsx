import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import FoldMergeDialog from "./FoldMergeDialog";

const PATHS = [
  "C:/archives/one.jwlibrary",
  "C:/archives/two.jwlibrary",
  "C:/archives/three.jwlibrary",
];

function renderDialog(paths: string[] = PATHS) {
  const onChange = vi.fn();
  const onContinue = vi.fn();
  const onCancel = vi.fn();
  render(
    <FoldMergeDialog
      paths={paths}
      onChange={onChange}
      onContinue={onContinue}
      onCancel={onCancel}
    />,
  );
  return { onChange, onContinue, onCancel };
}

describe("FoldMergeDialog", () => {
  it("renders one row per path, in order, numbered from 1, showing the base name", () => {
    renderDialog();
    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent("1");
    expect(rows[0]).toHaveTextContent("one.jwlibrary");
    expect(rows[1]).toHaveTextContent("2");
    expect(rows[1]).toHaveTextContent("two.jwlibrary");
    expect(rows[2]).toHaveTextContent("3");
    expect(rows[2]).toHaveTextContent("three.jwlibrary");
    // Full path available as a title (aria description) on the row.
    expect(rows[0]).toHaveAttribute("title", "C:/archives/one.jwlibrary");
  });

  it("move-up on row 2 swaps rows 1 and 2 and reports the new order", () => {
    const { onChange } = renderDialog();
    fireEvent.click(screen.getByTestId("fold-merge-row-1-up"));
    expect(onChange).toHaveBeenCalledWith([
      "C:/archives/two.jwlibrary",
      "C:/archives/one.jwlibrary",
      "C:/archives/three.jwlibrary",
    ]);
  });

  it("move-down on row 1 (0-index) swaps rows 1 and 2 and reports the new order", () => {
    const { onChange } = renderDialog();
    fireEvent.click(screen.getByTestId("fold-merge-row-0-down"));
    expect(onChange).toHaveBeenCalledWith([
      "C:/archives/two.jwlibrary",
      "C:/archives/one.jwlibrary",
      "C:/archives/three.jwlibrary",
    ]);
  });

  it("move-up is unavailable on the first row", () => {
    renderDialog();
    expect(screen.getByTestId("fold-merge-row-0-up")).toBeDisabled();
  });

  it("move-down is unavailable on the last row", () => {
    renderDialog();
    expect(screen.getByTestId("fold-merge-row-2-down")).toBeDisabled();
  });

  it("remove drops that row and renumbers the rest", () => {
    const { onChange } = renderDialog();
    fireEvent.click(screen.getByTestId("fold-merge-row-1-remove"));
    expect(onChange).toHaveBeenCalledWith([
      "C:/archives/one.jwlibrary",
      "C:/archives/three.jwlibrary",
    ]);
  });

  it("Continue is unavailable with fewer than 3 rows, with a visible reason", () => {
    renderDialog(PATHS.slice(0, 2));
    expect(screen.getByTestId("fold-merge-continue")).toBeDisabled();
    expect(screen.getByTestId("fold-merge-reason")).toHaveTextContent(
      /at least 3/i,
    );
  });

  it("Continue is available with 3 or more rows and no reason is shown", () => {
    renderDialog();
    expect(screen.getByTestId("fold-merge-continue")).not.toBeDisabled();
    expect(screen.queryByTestId("fold-merge-reason")).not.toBeInTheDocument();
  });

  it("Continue click invokes onContinue when enabled", () => {
    const { onContinue } = renderDialog();
    fireEvent.click(screen.getByTestId("fold-merge-continue"));
    expect(onContinue).toHaveBeenCalledTimes(1);
  });

  it("Cancel invokes the cancel callback and nothing else", () => {
    const { onCancel, onChange, onContinue } = renderDialog();
    fireEvent.click(screen.getByTestId("fold-merge-cancel"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onChange).not.toHaveBeenCalled();
    expect(onContinue).not.toHaveBeenCalled();
  });

  it("renders the order-explanation sentence", () => {
    renderDialog();
    expect(screen.getByTestId("fold-merge-order-note")).toHaveTextContent(
      /order shown, top to bottom/i,
    );
    expect(screen.getByTestId("fold-merge-order-note")).toHaveTextContent(
      /lower in the list wins/i,
    );
  });
});
