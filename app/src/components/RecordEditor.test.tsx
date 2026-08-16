import { fireEvent, render as rtlRender, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";
import RecordEditor from "./RecordEditor";
import { I18nProvider } from "../i18n/I18nContext";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { RecordEditFields } from "../bindings/RecordEditFields";

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

function makeRow(overrides: Partial<BrowseRow> = {}): BrowseRow {
  return {
    id: 500n,
    language: "English",
    symbol: "nwt",
    color: null,
    tags: null,
    modified: null,
    year: "2026",
    detail1: "01: Genesis",
    detail2: null,
    short: "Genesis",
    full: "Genesis 1:1",
    type_group: "Bible",
    independent: false,
    text_tag: null,
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("RecordEditor (EDIT-07, 07-05-PLAN.md Task 2)", () => {
  it("Notes render Title, Content, and the color row", async () => {
    const fields: RecordEditFields = {
      category: "Notes",
      title: "My title",
      content: "My content",
      color_index: null,
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "record_fetch") return Promise.resolve(fields);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <RecordEditor
        category="Notes"
        row={makeRow()}
        onApplied={vi.fn()}
        onCancel={vi.fn()}
        onError={vi.fn()}
      />,
    );

    expect(await screen.findByTestId("record-editor-content")).toHaveValue("My content");
    expect(screen.getByLabelText("Title")).toHaveValue("My title");
    expect(screen.getByTestId("record-editor-color-0")).toBeInTheDocument();
    expect(screen.getByTestId("record-editor-no-color")).toBeInTheDocument();
  });

  it("Annotations render only the Value field, no Title or color row", async () => {
    const fields: RecordEditFields = { category: "Annotations", value: "annotation value" };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "record_fetch") return Promise.resolve(fields);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <RecordEditor
        category="Annotations"
        row={makeRow({ text_tag: "tag-a" })}
        onApplied={vi.fn()}
        onCancel={vi.fn()}
        onError={vi.fn()}
      />,
    );

    expect(await screen.findByTestId("record-editor-content")).toHaveValue("annotation value");
    expect(screen.queryByLabelText("Title")).not.toBeInTheDocument();
    expect(screen.queryByTestId("record-editor-color-0")).not.toBeInTheDocument();
  });

  it("a Note WITH a linked UserMark renders the matching swatch selected, no 'No color' state", async () => {
    const fields: RecordEditFields = {
      category: "Notes",
      title: "Colored note",
      content: "content",
      color_index: 2n,
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "record_fetch") return Promise.resolve(fields);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <RecordEditor
        category="Notes"
        row={makeRow()}
        onApplied={vi.fn()}
        onCancel={vi.fn()}
        onError={vi.fn()}
      />,
    );

    await screen.findByTestId("record-editor-content");
    expect(screen.queryByTestId("record-editor-no-color")).not.toBeInTheDocument();
    expect(screen.getByTestId("record-editor-color-2")).toHaveAttribute("aria-pressed", "true");
  });

  it("Cancel triggers zero backend invocations beyond the initial fetch", async () => {
    const fields: RecordEditFields = {
      category: "Notes",
      title: "T",
      content: "C",
      color_index: null,
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "record_fetch") return Promise.resolve(fields);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onCancel = vi.fn();
    render(
      <RecordEditor
        category="Notes"
        row={makeRow()}
        onApplied={vi.fn()}
        onCancel={onCancel}
        onError={vi.fn()}
      />,
    );

    await screen.findByTestId("record-editor");
    fireEvent.click(screen.getByTestId("record-editor-cancel"));

    expect(onCancel).toHaveBeenCalledTimes(1);
    const nonFetchCalls = invokeMock.mock.calls.filter(([cmd]) => cmd !== "record_fetch");
    expect(nonFetchCalls).toHaveLength(0);
  });

  it("Save fires exactly one record_edit_dry_run and opens the preview dialog", async () => {
    const fields: RecordEditFields = {
      category: "Notes",
      title: "T",
      content: "C",
      color_index: null,
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "record_fetch") return Promise.resolve(fields);
      if (cmd === "record_edit_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <RecordEditor
        category="Notes"
        row={makeRow()}
        onApplied={vi.fn()}
        onCancel={vi.fn()}
        onError={vi.fn()}
      />,
    );

    await screen.findByTestId("record-editor-save");
    fireEvent.click(screen.getByTestId("record-editor-save"));

    await screen.findByTestId("edit-preview-dialog");

    const dryRunCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "record_edit_dry_run");
    expect(dryRunCalls).toHaveLength(1);
  });

  it("Delete on an Annotation with InputField.deleted = 2 renders the over-deletion summary", async () => {
    const fields: RecordEditFields = { category: "Annotations", value: "v" };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "record_fetch") return Promise.resolve(fields);
      if (cmd === "record_delete_dry_run") {
        return Promise.resolve(makeReport({ deleted: { InputField: 2 } }));
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <RecordEditor
        category="Annotations"
        row={makeRow({ text_tag: "tag-a" })}
        onApplied={vi.fn()}
        onCancel={vi.fn()}
        onError={vi.fn()}
      />,
    );

    await screen.findByTestId("record-editor-delete");
    fireEvent.click(screen.getByTestId("record-editor-delete"));

    const dialog = await screen.findByTestId("edit-preview-dialog");
    expect(dialog).toHaveTextContent(
      "Deleting this annotation removes all annotation fields at this location",
    );
  });
});
