import { fireEvent, render as rtlRender, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";
import TagDialog from "./TagDialog";
import { I18nProvider } from "../i18n/I18nContext";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { TagState } from "../bindings/TagState";

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
    overwritten: {},
    deleted: {},
    total_deleted: 0,
    skipped: {},
    ...overrides,
  };
}

const TAGS: TagState[] = [
  { tag_id: 1n, name: "Alpha", count: 1n },
  { tag_id: 2n, name: "Beta", count: 2n },
  { tag_id: 3n, name: "Gamma", count: 0n },
];

beforeEach(() => {
  invokeMock.mockReset();
});

describe("TagDialog — checklist (EDIT-03, 07-03-PLAN.md Task 3)", () => {
  it("zero-tags state renders the empty sentence with the add input still present", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "tag_states") return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<TagDialog selectedIds={[10n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);

    expect(await screen.findByTestId("tag-dialog-empty")).toHaveTextContent(
      "No tags yet — type a name below to create one.",
    );
    expect(screen.getByTestId("tag-dialog-add-input")).toBeInTheDocument();
  });

  it("a partially-tagged tag renders indeterminate", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "tag_states") return Promise.resolve(TAGS);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <TagDialog selectedIds={[10n, 20n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    const alphaCheckbox = (await screen.findByTestId(
      "tag-dialog-item-1-checkbox",
    )) as HTMLInputElement;
    expect(alphaCheckbox.indeterminate).toBe(true);

    const betaCheckbox = screen.getByTestId("tag-dialog-item-2-checkbox") as HTMLInputElement;
    expect(betaCheckbox.checked).toBe(true);
    expect(betaCheckbox.indeterminate).toBe(false);

    const gammaCheckbox = screen.getByTestId("tag-dialog-item-3-checkbox") as HTMLInputElement;
    expect(gammaCheckbox.checked).toBe(false);
    expect(gammaCheckbox.indeterminate).toBe(false);
  });

  it("Apply fires exactly one tag_dry_run after toggling a row", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "tag_states") return Promise.resolve(TAGS);
      if (cmd === "tag_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <TagDialog selectedIds={[10n, 20n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />,
    );

    fireEvent.click(await screen.findByTestId("tag-dialog-item-3-checkbox"));
    fireEvent.click(screen.getByTestId("tag-dialog-apply"));

    await screen.findByTestId("edit-preview-dialog");

    const dryRunCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "tag_dry_run");
    expect(dryRunCalls).toHaveLength(1);
    expect(dryRunCalls[0][1]).toEqual({
      ids: [10n, 20n],
      removedTagIds: [],
      addedTagIds: [3n],
      newTagNames: [],
    });
  });

  it("Cancel fires no apply command", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "tag_states") return Promise.resolve(TAGS);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onCancel = vi.fn();
    render(
      <TagDialog selectedIds={[10n]} onApplied={vi.fn()} onCancel={onCancel} onError={vi.fn()} />,
    );

    await screen.findByTestId("tag-dialog");
    fireEvent.click(screen.getByTestId("tag-dialog-cancel"));

    expect(onCancel).toHaveBeenCalledTimes(1);
    const applyCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "tag_apply");
    expect(applyCalls).toHaveLength(0);
  });

  it("typing a new tag name and adding it includes it in newTagNames on Apply", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "tag_states") return Promise.resolve([]);
      if (cmd === "tag_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<TagDialog selectedIds={[10n]} onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);

    await screen.findByTestId("tag-dialog-empty");
    fireEvent.change(screen.getByTestId("tag-dialog-add-input"), { target: { value: "Delta" } });
    fireEvent.click(screen.getByTestId("tag-dialog-add-button"));
    fireEvent.click(screen.getByTestId("tag-dialog-apply"));

    await screen.findByTestId("edit-preview-dialog");

    const dryRunCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "tag_dry_run");
    expect(dryRunCalls).toHaveLength(1);
    expect(dryRunCalls[0][1]).toMatchObject({ newTagNames: ["Delta"] });
  });
});
