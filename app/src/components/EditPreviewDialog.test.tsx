import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import EditPreviewDialog from "./EditPreviewDialog";
import type { DryRunReport } from "../bindings/DryRunReport";

function makeReport(overrides: Partial<DryRunReport> = {}): DryRunReport {
  return {
    added: {},
    overwritten: { TagMap: 2 },
    deleted: { Note: 1, TagMap: 1 },
    total_deleted: 2,
    ...overrides,
  };
}

describe("EditPreviewDialog", () => {
  it("renders the per-table deleted counts from the DryRunReport", () => {
    render(<EditPreviewDialog report={makeReport()} onConfirm={vi.fn()} onCancel={vi.fn()} />);

    const summary = screen.getByTestId("edit-preview-summary");
    expect(summary).toHaveTextContent("1 Note");
    expect(summary).toHaveTextContent("1 TagMap");
    expect(summary).toHaveTextContent("2 rows total");
  });

  it("Confirm invokes onConfirm exactly once even with a rapid double-click", async () => {
    let resolveConfirm: () => void = () => {};
    const onConfirm = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveConfirm = resolve;
        }),
    );
    render(<EditPreviewDialog report={makeReport()} onConfirm={onConfirm} onCancel={vi.fn()} />);

    const confirmButton = screen.getByTestId("edit-preview-confirm");
    fireEvent.click(confirmButton);
    fireEvent.click(confirmButton); // rapid second click while pending

    expect(onConfirm).toHaveBeenCalledTimes(1);
    resolveConfirm();
  });

  it("Confirm shows a pending state and disables the button while in flight", () => {
    const onConfirm = vi.fn(() => new Promise<void>(() => {}));
    render(<EditPreviewDialog report={makeReport()} onConfirm={onConfirm} onCancel={vi.fn()} />);

    fireEvent.click(screen.getByTestId("edit-preview-confirm"));
    expect(screen.getByTestId("edit-preview-confirm")).toBeDisabled();
  });

  it("Cancel invokes onCancel and never onConfirm", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<EditPreviewDialog report={makeReport()} onConfirm={onConfirm} onCancel={onCancel} />);

    fireEvent.click(screen.getByTestId("edit-preview-cancel"));

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("Escape key invokes onCancel", () => {
    const onCancel = vi.fn();
    render(<EditPreviewDialog report={makeReport()} onConfirm={vi.fn()} onCancel={onCancel} />);

    fireEvent.keyDown(document, { key: "Escape" });

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("Escape key is ignored while Confirm is pending (busyRef guard)", () => {
    const onConfirm = vi.fn(() => new Promise<void>(() => {}));
    const onCancel = vi.fn();
    render(<EditPreviewDialog report={makeReport()} onConfirm={onConfirm} onCancel={onCancel} />);

    fireEvent.click(screen.getByTestId("edit-preview-confirm"));
    fireEvent.keyDown(document, { key: "Escape" });

    expect(onCancel).not.toHaveBeenCalled();
  });

  it("clicking the overlay backdrop invokes onCancel", () => {
    const onCancel = vi.fn();
    render(<EditPreviewDialog report={makeReport()} onConfirm={vi.fn()} onCancel={onCancel} />);

    const overlay = screen.getByTestId("edit-preview-dialog").parentElement as HTMLElement;
    fireEvent.click(overlay);

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("clicking inside the dialog card does not invoke onCancel", () => {
    const onCancel = vi.fn();
    render(<EditPreviewDialog report={makeReport()} onConfirm={vi.fn()} onCancel={onCancel} />);

    fireEvent.click(screen.getByTestId("edit-preview-dialog"));

    expect(onCancel).not.toHaveBeenCalled();
  });

  it("clicking the overlay backdrop while Confirm is pending does not invoke onCancel", () => {
    const onConfirm = vi.fn(() => new Promise<void>(() => {}));
    const onCancel = vi.fn();
    render(<EditPreviewDialog report={makeReport()} onConfirm={onConfirm} onCancel={onCancel} />);

    fireEvent.click(screen.getByTestId("edit-preview-confirm"));
    const overlay = screen.getByTestId("edit-preview-dialog").parentElement as HTMLElement;
    fireEvent.click(overlay);

    expect(onCancel).not.toHaveBeenCalled();
  });
});
