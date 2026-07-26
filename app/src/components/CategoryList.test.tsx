import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import CategoryList from "./CategoryList";
import type { BrowseRow } from "../bindings/BrowseRow";

const invokeMock = vi.fn();
const openMock = vi.fn();
const saveMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
  save: (...args: unknown[]) => saveMock(...args),
}));

function makeRow(id: number, overrides: Partial<BrowseRow> = {}): BrowseRow {
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
    text_tag: null,
    ...overrides,
  };
}

/** A Playlists row: full/short/symbol are the "* OTHER *" sentinel; the real
 * label lives in detail1 (PlaylistItem.Label), the name in tags. */
function makePlaylistRow(id: number, label: string, name: string): BrowseRow {
  return makeRow(id, {
    language: null,
    symbol: "* OTHER *",
    color: null,
    modified: null,
    year: "",
    detail1: label,
    detail2: null,
    short: "* OTHER *",
    full: "* OTHER *",
    type_group: "Other",
    tags: name,
  });
}

beforeAll(() => {
  class ResizeObserverMock {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  // @ts-expect-error test-only global stub
  global.ResizeObserver = ResizeObserverMock;

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
  openMock.mockReset();
  saveMock.mockReset();
});

describe("CategoryList — virtualization + row contract (D6-07)", () => {
  it("virtualizes 9,000 rows: rendered DOM row nodes are far fewer than the count", () => {
    const rows = Array.from({ length: 9000 }, (_, i) => makeRow(i));
    render(<CategoryList rows={rows} category="Notes" />);

    const rendered = screen.getAllByTestId("category-list-row");
    expect(rendered.length).toBeGreaterThan(0);
    expect(rendered.length).toBeLessThan(100);
  });

  it("keeps every row at a fixed 44px height with an overlong label (single-line, no wrap)", () => {
    const overlong = "x".repeat(2000);
    const rows = [makeRow(1, { full: overlong, tags: overlong, detail1: null, detail2: null })];
    render(<CategoryList rows={rows} category="Notes" />);

    const row = screen.getByTestId("category-list-row");
    expect(row).toHaveStyle({ height: "44px", whiteSpace: "nowrap" });
  });

  it("renders the resolved label, tags, and modified date for a Notes row", () => {
    render(<CategoryList rows={[makeRow(1)]} category="Notes" />);
    expect(screen.getByText(/The Watchtower/)).toBeInTheDocument();
    expect(screen.getByText("Tag A | Tag B")).toBeInTheDocument();
    expect(screen.getByText("2026-01-01")).toBeInTheDocument();
  });

  it("shows a per-category empty state when there are no rows", () => {
    const { rerender } = render(<CategoryList rows={[]} category="Notes" />);
    expect(screen.getByText(/no Notes in this archive/i)).toBeInTheDocument();

    rerender(<CategoryList rows={[]} category="Bookmarks" />);
    expect(screen.getByText(/no Bookmarks in this archive/i)).toBeInTheDocument();
  });
});

describe("CategoryList — Playlists W1 label fix", () => {
  it("surfaces the playlist Label (detail1) as the primary label, not the '* OTHER *' sentinel", () => {
    render(
      <CategoryList
        rows={[makePlaylistRow(1, "My Favourite Songs", "Playlist One")]}
        category="Playlists"
      />,
    );
    const label = screen.getByTestId("category-list-row-label");
    expect(label).toHaveTextContent("My Favourite Songs");
    expect(label).not.toHaveTextContent("* OTHER *");
    // Playlist Name still surfaces via the tags column.
    expect(screen.getByText("Playlist One")).toBeInTheDocument();
  });
});

describe("CategoryList — selection model (D6-05)", () => {
  it("Delete is disabled with 0 selected and enabled once a Notes row is selected", () => {
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Notes" />);
    const deleteButton = screen.getByTestId("category-list-delete-button");
    expect(deleteButton).toBeDisabled();

    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    expect(deleteButton).not.toBeDisabled();
    expect(deleteButton).toHaveTextContent("Delete (1)");
  });

  it("selecting more rows updates the selection count", () => {
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Notes" />);
    const checkboxes = screen.getAllByTestId("category-list-row-checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    expect(screen.getByTestId("category-list-selection-count")).toHaveTextContent("2");
  });

  it("resets the selection to empty when the category prop changes (D6-05)", () => {
    const rows = [makeRow(1), makeRow(2)];
    const { rerender } = render(<CategoryList rows={rows} category="Notes" />);

    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    expect(screen.getByTestId("category-list-selection-count")).toHaveTextContent("1");

    // Switch away to another category, then back to Notes.
    rerender(<CategoryList rows={rows} category="Highlights" />);
    expect(screen.getByTestId("category-list-selection-count")).toHaveTextContent("0");

    rerender(<CategoryList rows={rows} category="Notes" />);
    expect(screen.getByTestId("category-list-delete-button")).toBeDisabled();
    expect(screen.getByTestId("category-list-selection-count")).toHaveTextContent("0");
  });

  it("multi-select works across a non-Notes category and updates the selection size", () => {
    render(<CategoryList rows={[makeRow(1), makeRow(2), makeRow(3)]} category="Highlights" />);
    const checkboxes = screen.getAllByTestId("category-list-row-checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[2]);
    expect(screen.getByTestId("category-list-selection-count")).toHaveTextContent("2");
  });
});

describe("CategoryList — contextual operation set (D6-08, DATA-07 criterion 3)", () => {
  it("Playlists renders live delete/add-media affordances, not the deferred placeholder (08-06-PLAN.md)", () => {
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Playlists" />);

    // Live delete affordance for Playlists — ref-counted media delete
    // (D8-07) landed in 08-06-PLAN.md, closing the last deferred slot.
    expect(screen.getByTestId("category-list-delete-button")).toBeInTheDocument();
    expect(screen.queryByTestId("category-list-op-delete")).not.toBeInTheDocument();

    // "Add Media…" renders as the live add-button, not the deferred op.
    expect(screen.getByTestId("category-list-add-button")).toHaveTextContent("Add Media…");
    expect(screen.queryByTestId("category-list-op-add")).not.toBeInTheDocument();
  });

  it("Notes renders the live delete op AND the live export/import affordances", () => {
    render(<CategoryList rows={[makeRow(1)]} category="Notes" />);
    // Live delete button exists.
    expect(screen.getByTestId("category-list-delete-button")).toBeInTheDocument();
    // export/import (08-04-PLAN.md) are LIVE for Notes — rendered as their
    // own dedicated buttons, never the generic deferred-op affordance.
    expect(screen.getByTestId("category-list-export-button")).toBeInTheDocument();
    expect(screen.getByTestId("category-list-import-button")).toBeInTheDocument();
    expect(screen.queryByTestId("category-list-op-export")).not.toBeInTheDocument();
  });
});

describe("CategoryList — live Notes delete flow", () => {
  it("clicking Delete invokes delete_notes_dry_run with the selected ids", async () => {
    invokeMock.mockResolvedValue({
      added: {},
      overwritten: {},
      deleted: { Note: 1 },
      total_deleted: 1,
    });
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Notes" />);

    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    fireEvent.click(screen.getByTestId("category-list-delete-button"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_notes_dry_run", { ids: [1n] }),
    );
  });

  it("Confirm applies the delete and removes the row locally", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "delete_notes_dry_run" || cmd === "delete_notes_apply") {
        return Promise.resolve({
          added: {},
          overwritten: {},
          deleted: { Note: 1 },
          total_deleted: 1,
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onRowsChanged = vi.fn();
    const rows = [makeRow(1), makeRow(2)];
    render(<CategoryList rows={rows} category="Notes" onRowsChanged={onRowsChanged} />);

    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    fireEvent.click(screen.getByTestId("category-list-delete-button"));

    await screen.findByTestId("edit-preview-dialog");
    fireEvent.click(screen.getByTestId("edit-preview-confirm"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_notes_apply", { ids: [1n] }),
    );
    await vi.waitFor(() => expect(onRowsChanged).toHaveBeenCalledWith([rows[1]]));
    await vi.waitFor(() =>
      expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument(),
    );
    await vi.waitFor(() =>
      expect(screen.getByTestId("category-list-delete-button")).toBeDisabled(),
    );
  });

  it("Cancel invokes no apply and leaves the list unchanged", async () => {
    invokeMock.mockResolvedValue({
      added: {},
      overwritten: {},
      deleted: { Note: 1 },
      total_deleted: 1,
    });
    const onRowsChanged = vi.fn();
    const rows = [makeRow(1), makeRow(2)];
    render(<CategoryList rows={rows} category="Notes" onRowsChanged={onRowsChanged} />);

    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    fireEvent.click(screen.getByTestId("category-list-delete-button"));

    await screen.findByTestId("edit-preview-dialog");
    invokeMock.mockClear();
    fireEvent.click(screen.getByTestId("edit-preview-cancel"));

    expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
    expect(onRowsChanged).not.toHaveBeenCalled();
  });
});

describe("CategoryList — Edit precondition (EDIT-07, 07-05-PLAN.md Task 2)", () => {
  it("Edit is disabled with title 'Select exactly one row to edit' at selection size 2", () => {
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Notes" />);
    const checkboxes = screen.getAllByTestId("category-list-row-checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const editButton = screen.getByTestId("category-list-edit-button");
    expect(editButton).toBeDisabled();
    expect(editButton).toHaveAttribute("title", "Select exactly one row to edit");
  });

  it("Edit is enabled at selection size 1", () => {
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Notes" />);
    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);

    const editButton = screen.getByTestId("category-list-edit-button");
    expect(editButton).not.toBeDisabled();
  });

  it("Edit is disabled with no title at selection size 0", () => {
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Notes" />);
    const editButton = screen.getByTestId("category-list-edit-button");
    expect(editButton).toBeDisabled();
    expect(editButton).not.toHaveAttribute("title");
  });

  it("clicking Edit at selection size 1 opens the RecordEditor for the selected row", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "record_fetch") {
        return Promise.resolve({ category: "Notes", title: "T", content: "C", color_index: null });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<CategoryList rows={[makeRow(1), makeRow(2)]} category="Notes" />);
    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    fireEvent.click(screen.getByTestId("category-list-edit-button"));

    expect(await screen.findByTestId("record-editor")).toBeInTheDocument();
  });
});

describe("CategoryList — Favorites import flow (IO-02, 08-01-PLAN.md)", () => {
  it("a malformed dry-run rejection renders the error banner and never mounts edit-preview-dialog", async () => {
    openMock.mockResolvedValue("/tmp/malformed.txt");
    const err = {
      code: "import_malformed",
      operation: "import_favorites_dry_run",
      safe_file_name: "malformed.txt",
      message_key: "error.archive.import_malformed",
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "import_favorites_dry_run") {
        return Promise.reject(err);
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onError = vi.fn();
    render(<CategoryList rows={[]} category="Favorites" onError={onError} />);

    fireEvent.click(screen.getByTestId("category-list-import-button"));

    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(err));
    expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument();
  });

  it("a successful dry-run opens edit-preview-dialog with the import preview", async () => {
    openMock.mockResolvedValue("/tmp/favorites.txt");
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "import_favorites_dry_run") {
        return Promise.resolve({
          added: { TagMap: 1 },
          overwritten: {},
          deleted: {},
          total_deleted: 0,
          skipped: {},
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<CategoryList rows={[]} category="Favorites" />);

    fireEvent.click(screen.getByTestId("category-list-import-button"));

    expect(await screen.findByTestId("edit-preview-dialog")).toBeInTheDocument();
    expect(screen.getByTestId("edit-preview-summary").textContent).toContain(
      "1 new record will be added.",
    );
  });

  it("clicking Export invokes export_favorites with the picked path and a null selection", async () => {
    saveMock.mockResolvedValue("/tmp/favorites-out.txt");
    invokeMock.mockResolvedValue(2);
    render(<CategoryList rows={[]} category="Favorites" />);

    fireEvent.click(screen.getByTestId("category-list-export-button"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("export_favorites", {
        path: "/tmp/favorites-out.txt",
        ids: null,
      }),
    );
  });
});
