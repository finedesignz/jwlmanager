import { fireEvent, render as rtlRender, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";
import ColorMenu from "./ColorMenu";
import { I18nProvider } from "../i18n/I18nContext";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";

function render(ui: ReactElement) {
  return rtlRender(
    <I18nProvider locale="en" setLocale={() => {}}>
      {ui}
    </I18nProvider>,
  );
}

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makeReport(overrides: Partial<DryRunReport> = {}): DryRunReport {
  return {
    added: {},
    overwritten: { UserMark: 2 },
    deleted: {},
    total_deleted: 0,
    skipped: {},
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("ColorMenu — swatch row (EDIT-02, 07-02-PLAN.md Task 3)", () => {
  it("renders 7 swatches with role=menuitem and the Grey row disabled+tooltipped for Highlights", () => {
    render(
      <ColorMenu category="Highlights" selectedIds={[1n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    const menu = screen.getByTestId("color-menu");
    expect(menu).toHaveAttribute("role", "menu");

    const grey = screen.getByTestId("color-menu-swatch-0");
    expect(grey).toBeDisabled();
    expect(grey).toHaveAttribute("title", "Grey has no effect on existing highlights");

    for (let i = 1; i < 7; i++) {
      expect(screen.getByTestId(`color-menu-swatch-${i}`)).not.toBeDisabled();
    }
  });

  it("Grey stays enabled for Notes (it still synthesizes a grey-colored UserMark there)", () => {
    render(
      <ColorMenu category="Notes" selectedIds={[1n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    const grey = screen.getByTestId("color-menu-swatch-0");
    expect(grey).not.toBeDisabled();
  });

  it("Escape closes the menu and invokes onCancel while no dry-run is pending", () => {
    const onCancel = vi.fn();
    render(
      <ColorMenu category="Notes" selectedIds={[1n]} onApplied={vi.fn()} onCancel={onCancel} onError={vi.fn()} />,
    );

    fireEvent.keyDown(document, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("clicking a swatch fires exactly one color_dry_run invocation with the ColorSelection + colorIndex", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "color_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <ColorMenu category="Highlights" selectedIds={[10n, 20n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    fireEvent.click(screen.getByTestId("color-menu-swatch-3")); // Blue

    await screen.findByTestId("edit-preview-dialog");

    const dryRunCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "color_dry_run");
    expect(dryRunCalls).toHaveLength(1);
    expect(dryRunCalls[0][1]).toEqual({
      selection: { category: "Highlights", ids: [10n, 20n] },
      colorIndex: 3,
    });
  });

  it("clicking the disabled Grey row on Highlights never invokes color_dry_run", () => {
    render(
      <ColorMenu category="Highlights" selectedIds={[1n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    fireEvent.click(screen.getByTestId("color-menu-swatch-0"));
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("ColorMenu — preview to apply", () => {
  it("shows both clauses when the report synthesized UserMarks (report.added.UserMark > 0)", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "color_dry_run") {
        return Promise.resolve(makeReport({ added: { UserMark: 1 }, overwritten: { UserMark: 2 } }));
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <ColorMenu category="Notes" selectedIds={[5n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    fireEvent.click(screen.getByTestId("color-menu-swatch-2")); // Green

    const summary = await screen.findByTestId("edit-preview-summary");
    expect(summary).toHaveTextContent("2 highlights will be recolored to Green");
    expect(summary).toHaveTextContent("1 note will become highlighted in Green for the first time");
  });

  it("shows only the recolor clause when nothing was synthesized", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "color_dry_run") return Promise.resolve(makeReport({ overwritten: { UserMark: 1 } }));
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <ColorMenu category="Highlights" selectedIds={[1n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    fireEvent.click(screen.getByTestId("color-menu-swatch-1")); // Yellow

    const summary = await screen.findByTestId("edit-preview-summary");
    expect(summary).toHaveTextContent("1 highlight will be recolored to Yellow");
    expect(summary).not.toHaveTextContent("will become highlighted");
  });

  it("Confirm applies, re-fetches the category, and calls onApplied with the fresh rows", async () => {
    const freshRows = [{ id: 950n }];
    invokeMock.mockImplementation((cmd: string, args?: { category?: string }) => {
      if (cmd === "color_dry_run") return Promise.resolve(makeReport());
      if (cmd === "color_apply") return Promise.resolve(makeReport());
      if (cmd === "list_category" && args?.category === "Highlights") {
        return Promise.resolve(freshRows);
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onApplied = vi.fn();
    render(
      <ColorMenu category="Highlights" selectedIds={[1n]} onApplied={onApplied} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    fireEvent.click(screen.getByTestId("color-menu-swatch-4")); // Red
    await screen.findByTestId("edit-preview-dialog");
    fireEvent.click(screen.getByTestId("edit-preview-confirm"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("color_apply", {
        selection: { category: "Highlights", ids: [1n] },
        colorIndex: 4,
      }),
    );
    await vi.waitFor(() => expect(onApplied).toHaveBeenCalledWith(freshRows));
  });

  it("a color_dry_run failure routes through onError and closes the menu (no preview opens)", async () => {
    const err: ErrorDto = {
      code: "color_failed",
      operation: "color_dry_run",
      safe_file_name: null,
      message_key: "error.archive.color_failed",
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "color_dry_run") return Promise.reject(err);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onError = vi.fn();
    const onCancel = vi.fn();
    render(
      <ColorMenu category="Notes" selectedIds={[1n]} onApplied={vi.fn()} onCancel={onCancel} onError={onError} />,
    );

    fireEvent.click(screen.getByTestId("color-menu-swatch-5")); // Orange

    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(err));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument();
  });
});
