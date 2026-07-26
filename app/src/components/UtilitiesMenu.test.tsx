import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import UtilitiesMenu from "./UtilitiesMenu";
import type { DryRunReport } from "../bindings/DryRunReport";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makeReport(overrides: Partial<DryRunReport> = {}): DryRunReport {
  return {
    added: {},
    overwritten: {},
    deleted: {},
    total_deleted: 0,
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("UtilitiesMenu (07-03-PLAN.md Task 3)", () => {
  it("renders role=menu with Clean/Mask disabled and Sort Tags enabled", () => {
    render(<UtilitiesMenu onCancel={vi.fn()} onError={vi.fn()} onSorted={vi.fn()} />);

    expect(screen.getByTestId("utilities-menu")).toHaveAttribute("role", "menu");
    expect(screen.getByTestId("utilities-menu-clean")).toBeDisabled();
    expect(screen.getByTestId("utilities-menu-mask")).toBeDisabled();
    expect(screen.getByTestId("utilities-menu-sort")).not.toBeDisabled();
  });

  it("Escape closes the menu and returns focus to the trigger", () => {
    const onCancel = vi.fn();
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();

    render(<UtilitiesMenu onCancel={onCancel} onError={vi.fn()} onSorted={vi.fn()} />);
    fireEvent.keyDown(document, { key: "Escape" });

    expect(onCancel).toHaveBeenCalledTimes(1);
    document.body.removeChild(trigger);
  });

  it("Sort Tags fires exactly one reorder_dry_run", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "reorder_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<UtilitiesMenu onCancel={vi.fn()} onError={vi.fn()} onSorted={vi.fn()} />);

    fireEvent.click(screen.getByTestId("utilities-menu-sort"));

    await screen.findByTestId("edit-preview-dialog");

    const calls = invokeMock.mock.calls.filter(([cmd]) => cmd === "reorder_dry_run");
    expect(calls).toHaveLength(1);
  });

  it("a zero-change Sort Tags report renders the no-op summary", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "reorder_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<UtilitiesMenu onCancel={vi.fn()} onError={vi.fn()} onSorted={vi.fn()} />);

    fireEvent.click(screen.getByTestId("utilities-menu-sort"));

    const summary = await screen.findByTestId("edit-preview-summary");
    expect(summary).toHaveTextContent("No tag assignments need renumbering.");
  });

  it("a non-zero Sort Tags report renders the renumbered-count summary", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "reorder_dry_run") return Promise.resolve(makeReport({ overwritten: { TagMap: 4 } }));
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<UtilitiesMenu onCancel={vi.fn()} onError={vi.fn()} onSorted={vi.fn()} />);

    fireEvent.click(screen.getByTestId("utilities-menu-sort"));

    const summary = await screen.findByTestId("edit-preview-summary");
    expect(summary).toHaveTextContent("4 tag assignments will be renumbered");
  });

  it("Confirm applies reorder_apply and calls onSorted", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "reorder_dry_run") return Promise.resolve(makeReport({ overwritten: { TagMap: 1 } }));
      if (cmd === "reorder_apply") return Promise.resolve(makeReport({ overwritten: { TagMap: 1 } }));
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onSorted = vi.fn();
    render(<UtilitiesMenu onCancel={vi.fn()} onError={vi.fn()} onSorted={onSorted} />);

    fireEvent.click(screen.getByTestId("utilities-menu-sort"));
    await screen.findByTestId("edit-preview-dialog");
    fireEvent.click(screen.getByTestId("edit-preview-confirm"));

    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("reorder_apply"));
    await vi.waitFor(() => expect(onSorted).toHaveBeenCalledTimes(1));
  });
});
