import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import NotesList from "./NotesList";
import type { BrowseRow } from "../bindings/BrowseRow";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makeNote(id: number, overrides: Partial<BrowseRow> = {}): BrowseRow {
  return {
    id: BigInt(id),
    language: "English",
    symbol: "w",
    color: "Blue",
    tags: "Tag A | Tag B",
    modified: "2026-01-01",
    year: "2026",
    detail1: "01: Genesis",
    detail2: "Chap.   1",
    short: "w",
    full: "The Watchtower",
    type_group: "Magazines",
    independent: false,
    ...overrides,
  };
}

beforeAll(() => {
  class ResizeObserverMock {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  // jsdom has no ResizeObserver; TanStack Virtual only needs observe/
  // unobserve/disconnect to exist, not to actually fire.
  // @ts-expect-error test-only global stub
  global.ResizeObserver = ResizeObserverMock;

  // jsdom gives every element a zero layout box by default. TanStack
  // Virtual needs a non-zero scroll-viewport height to compute which rows
  // fall in the visible range — without this, either zero or every row
  // "counts" as in-range, defeating the test's purpose either direction.
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    value: 600,
  });
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    value: 600,
  });
});

beforeEach(() => {
  invokeMock.mockReset();
});

describe("NotesList", () => {
  it("virtualizes 9,000 rows: rendered row DOM nodes are far fewer than the row count", () => {
    const notes = Array.from({ length: 9000 }, (_, i) => makeNote(i));
    render(<NotesList notes={notes} />);

    const rows = screen.getAllByTestId("notes-list-row");
    expect(rows.length).toBeGreaterThan(0);
    // 600px viewport / 44px rows ~= 14 visible rows + overscan(8*2) ~= 30;
    // generous ceiling well under the 9,000 row count proves windowing.
    expect(rows.length).toBeLessThan(100);
  });

  it("keeps every row at a fixed 44px height even with an overlong snippet (single-line, no wrap)", () => {
    const overlong = "x".repeat(2000);
    const notes = [
      makeNote(1, { full: overlong, tags: overlong, detail1: null, detail2: null }),
    ];
    render(<NotesList notes={notes} />);

    const row = screen.getByTestId("notes-list-row");
    expect(row).toHaveStyle({ height: "44px", whiteSpace: "nowrap" });
  });

  it("renders the resolved label, tags, and modified date for a located note", () => {
    render(<NotesList notes={[makeNote(1)]} />);
    expect(screen.getByText(/The Watchtower/)).toBeInTheDocument();
    expect(screen.getByText("Tag A | Tag B")).toBeInTheDocument();
    expect(screen.getByText("2026-01-01")).toBeInTheDocument();
  });

  it("shows the empty state when there are no notes", () => {
    render(<NotesList notes={[]} />);
    expect(screen.getByText(/no notes in this archive/i)).toBeInTheDocument();
  });

  it("Delete is disabled with 0 selected and enabled once a row is selected", () => {
    render(<NotesList notes={[makeNote(1), makeNote(2)]} />);

    const deleteButton = screen.getByTestId("notes-list-delete-button");
    expect(deleteButton).toBeDisabled();

    const checkboxes = screen.getAllByTestId("notes-list-row-checkbox");
    fireEvent.click(checkboxes[0]);

    expect(deleteButton).not.toBeDisabled();
    expect(deleteButton).toHaveTextContent("Delete (1)");
  });

  it("selecting more rows updates the delete count", () => {
    render(<NotesList notes={[makeNote(1), makeNote(2)]} />);
    const checkboxes = screen.getAllByTestId("notes-list-row-checkbox");

    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    expect(screen.getByTestId("notes-list-delete-button")).toHaveTextContent("Delete (2)");
  });

  it("clicking Delete invokes delete_notes_dry_run with the selected NoteIds", async () => {
    invokeMock.mockResolvedValue({
      added: {},
      overwritten: {},
      deleted: { Note: 1 },
      total_deleted: 1,
    });
    render(<NotesList notes={[makeNote(1), makeNote(2)]} />);

    const checkboxes = screen.getAllByTestId("notes-list-row-checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(screen.getByTestId("notes-list-delete-button"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_notes_dry_run", { ids: [1n] }),
    );
  });

  it("Confirm in the preview dialog applies the delete and removes the row locally", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "delete_notes_dry_run") {
        return Promise.resolve({
          added: {},
          overwritten: {},
          deleted: { Note: 1 },
          total_deleted: 1,
        });
      }
      if (cmd === "delete_notes_apply") {
        return Promise.resolve({
          added: {},
          overwritten: {},
          deleted: { Note: 1 },
          total_deleted: 1,
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onNotesChanged = vi.fn();
    const notes = [makeNote(1), makeNote(2)];
    render(<NotesList notes={notes} onNotesChanged={onNotesChanged} />);

    fireEvent.click(screen.getAllByTestId("notes-list-row-checkbox")[0]);
    fireEvent.click(screen.getByTestId("notes-list-delete-button"));

    await screen.findByTestId("delete-preview-dialog");
    fireEvent.click(screen.getByTestId("delete-preview-confirm"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_notes_apply", { ids: [1n] }),
    );
    await vi.waitFor(() => expect(onNotesChanged).toHaveBeenCalledWith([notes[1]]));
    await vi.waitFor(() =>
      expect(screen.queryByTestId("delete-preview-dialog")).not.toBeInTheDocument(),
    );
    // Selection is cleared: delete button reverts to disabled with count 0.
    await vi.waitFor(() =>
      expect(screen.getByTestId("notes-list-delete-button")).toBeDisabled(),
    );
  });

  it("Cancel in the preview dialog invokes no apply and leaves the list unchanged", async () => {
    invokeMock.mockResolvedValue({
      added: {},
      overwritten: {},
      deleted: { Note: 1 },
      total_deleted: 1,
    });
    const onNotesChanged = vi.fn();
    const notes = [makeNote(1), makeNote(2)];
    render(<NotesList notes={notes} onNotesChanged={onNotesChanged} />);

    fireEvent.click(screen.getAllByTestId("notes-list-row-checkbox")[0]);
    fireEvent.click(screen.getByTestId("notes-list-delete-button"));

    await screen.findByTestId("delete-preview-dialog");
    invokeMock.mockClear(); // clear the dry-run call to isolate the cancel assertion
    fireEvent.click(screen.getByTestId("delete-preview-cancel"));

    expect(screen.queryByTestId("delete-preview-dialog")).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
    expect(onNotesChanged).not.toHaveBeenCalled();
  });
});
