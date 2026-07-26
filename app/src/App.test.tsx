import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { BrowseRow } from "./bindings/BrowseRow";

/**
 * DATA-07 end-to-end integration proof over MOCKED IPC (no real archive). Drives
 * the real `CommandBar` open path so the archive-open branch renders, then
 * exercises the full wired flow: switching category invokes `list_category` and
 * swaps the rendered rows (criterion 1), the selection is reset on switch so no
 * stale key survives (D6-05), the contextual operation set updates with
 * (category, selection) — live Notes-delete vs deferred elsewhere (criterion 3)
 * — and the shipped Notes delete flow still works end-to-end through the shell.
 */

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

const CORE_STATUS = { loaded: true, arch: "x86_64", version: "1.0.0", reason: null };

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

// Distinct, per-category rows so a row swap is observable in the DOM.
const NOTES_ROWS = [
  makeRow(1, { full: "Watchtower Note One" }),
  makeRow(2, { full: "Watchtower Note Two" }),
];
const HIGHLIGHT_ROWS = [
  makeRow(10, { full: "Highlighted Passage Alpha", tags: null, modified: null }),
  makeRow(11, { full: "Highlighted Passage Beta", tags: null, modified: null }),
];

const DRY_RUN_REPORT = {
  added: {},
  overwritten: {},
  deleted: { Note: 1 },
  total_deleted: 1,
};

beforeAll(() => {
  class ResizeObserverMock {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  // @ts-expect-error test-only global stub — TanStack Virtual only needs the
  // methods to exist, jsdom has no ResizeObserver.
  global.ResizeObserver = ResizeObserverMock;

  // jsdom hands every element a zero layout box; the virtualizer needs a
  // non-zero scroll-viewport height to compute the in-range rows.
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

/** Default IPC dispatch: open_archive → Notes, list_category → per-category,
 * check_jwlcore → loaded (JwlCoreNotice mounts and queries it). */
function wireHappyPathInvoke() {
  invokeMock.mockImplementation((cmd: string, args?: { category?: string }) => {
    if (cmd === "check_jwlcore") return Promise.resolve(CORE_STATUS);
    if (cmd === "open_archive") return Promise.resolve(NOTES_ROWS);
    if (cmd === "list_category") {
      if (args?.category === "Highlights") return Promise.resolve(HIGHLIGHT_ROWS);
      return Promise.resolve([]);
    }
    if (cmd === "delete_notes_dry_run" || cmd === "delete_notes_apply") {
      return Promise.resolve(DRY_RUN_REPORT);
    }
    return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
  });
}

/** Render App and drive the real CommandBar open path to reach the Notes view. */
async function openArchive() {
  openMock.mockResolvedValue("C:/archives/study.jwlibrary");
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /open archive/i }));
  await screen.findByText(/Watchtower Note One/);
}

describe("App — empty-state shell", () => {
  it("renders the empty-state shell with no archive open", () => {
    invokeMock.mockResolvedValue(CORE_STATUS);
    render(<App />);
    expect(
      screen.getByRole("heading", { name: /no archive open/i }),
    ).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: /open archive/i }).length,
    ).toBeGreaterThan(0);
  });
});

describe("App — DATA-07 end-to-end (mocked IPC)", () => {
  it("open_archive yields the initial Notes view (category defaults to Notes)", async () => {
    wireHappyPathInvoke();
    await openArchive();

    expect(invokeMock).toHaveBeenCalledWith("open_archive", {
      path: "C:/archives/study.jwlibrary",
    });
    // The Notes option is the active/pressed category on open.
    expect(screen.getByTestId("category-switcher-option-Notes")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByText(/Watchtower Note One/)).toBeInTheDocument();
  });

  it("selecting Highlights invokes list_category and swaps the rendered rows (criterion 1)", async () => {
    wireHappyPathInvoke();
    await openArchive();

    fireEvent.click(screen.getByTestId("category-switcher-option-Highlights"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_category", {
        category: "Highlights",
      }),
    );
    // Rows swapped: the highlight rows are shown, the Notes rows are gone.
    await screen.findByText(/Highlighted Passage Alpha/);
    expect(screen.queryByText(/Watchtower Note One/)).not.toBeInTheDocument();
    expect(screen.getByTestId("category-switcher-option-Highlights")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("switching category resets the prior selection — no stale key survives (D6-05)", async () => {
    wireHappyPathInvoke();
    await openArchive();

    // Select a Notes row: selection count reflects it.
    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    expect(screen.getByTestId("category-list-selection-count")).toHaveTextContent("1");

    // Switch to Highlights — the selection must be empty for the new category.
    fireEvent.click(screen.getByTestId("category-switcher-option-Highlights"));
    await screen.findByText(/Highlighted Passage Alpha/);

    expect(screen.getByTestId("category-list-selection-count")).toHaveTextContent("0");
  });

  it("the operation set updates with (category, selection): live Notes-delete vs deferred (criterion 3)", async () => {
    wireHappyPathInvoke();
    await openArchive();

    // Notes + a selection: the live Delete op is present and enabled, while
    // Notes:export (Notes' own export/import lands in a later 08-xx plan,
    // not this one) stays a deferred affordance — proving the op-bar still
    // renders the deferred branch for whatever ISN'T live.
    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    const deleteButton = screen.getByTestId("category-list-delete-button");
    expect(deleteButton).toBeEnabled();
    expect(deleteButton).toHaveTextContent("Delete (1)");

    const deferredExport = screen.getByTestId("category-list-op-export");
    expect(deferredExport).toHaveAttribute("data-deferred", "true");
    expect(deferredExport).toBeDisabled();

    // Switch to Highlights: delete is ALSO live here (07-02-PLAN.md D7-10),
    // routed through the same shared delete-button dispatch.
    fireEvent.click(screen.getByTestId("category-switcher-option-Highlights"));
    await screen.findByText(/Highlighted Passage Alpha/);

    expect(screen.getByTestId("category-list-delete-button")).toBeInTheDocument();
  });

  it("Notes delete still works end-to-end through the shell (dry-run → confirm → row removed)", async () => {
    wireHappyPathInvoke();
    await openArchive();

    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    fireEvent.click(screen.getByTestId("category-list-delete-button"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_notes_dry_run", { ids: [1n] }),
    );
    await screen.findByTestId("edit-preview-dialog");
    fireEvent.click(screen.getByTestId("edit-preview-confirm"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_notes_apply", { ids: [1n] }),
    );
    // The deleted row is gone; the surviving row remains (local filter, no reload).
    await vi.waitFor(() =>
      expect(screen.queryByText(/Watchtower Note One/)).not.toBeInTheDocument(),
    );
    expect(screen.getByText(/Watchtower Note Two/)).toBeInTheDocument();
  });

  it("a list_category failure leaves the prior view intact and surfaces the error (T-06-09)", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "check_jwlcore") return Promise.resolve(CORE_STATUS);
      if (cmd === "open_archive") return Promise.resolve(NOTES_ROWS);
      if (cmd === "list_category") {
        return Promise.reject({
          code: "query_failed",
          operation: "list_category",
          safe_file_name: "study.jwlibrary",
          message_key: "error.browse.query_failed",
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    await openArchive();

    fireEvent.click(screen.getByTestId("category-switcher-option-Highlights"));

    // Error banner surfaces; the Notes view is untouched (rows not swapped).
    await screen.findByTestId("error-banner");
    expect(screen.getByText(/Watchtower Note One/)).toBeInTheDocument();
    expect(screen.getByTestId("category-switcher-option-Notes")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
});
