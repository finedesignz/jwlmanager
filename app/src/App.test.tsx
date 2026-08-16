import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { SettingsProvider } from "./settings/SettingsProvider";
import { en } from "./i18n/en";
// Vite's built-in `*?raw` import (vite/client.d.ts) reads this file's OWN
// source at test time for the structural completeness scan below
// (11-03-PLAN.md Task 2) -- no new ambient declaration needed.
import appSource from "./App.tsx?raw";
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
    if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
    if (cmd === "save_settings") return Promise.resolve(undefined);
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
  render(
    <SettingsProvider>
      <App />
    </SettingsProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: /open archive/i }));
  await screen.findByText(/Watchtower Note One/);
}

describe("App — empty-state shell", () => {
  it("renders the empty-state shell with no archive open", () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "check_jwlcore") return Promise.resolve(CORE_STATUS);
      if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <SettingsProvider>
        <App />
      </SettingsProvider>,
    );
    expect(
      screen.getByRole("heading", { name: /no archive open/i }),
    ).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: /open archive/i }).length,
    ).toBeGreaterThan(0);
  });

  it("App shell retrofit: the empty-state heading and the .jwlibrary sentence render via the catalog, not a hardcoded literal duplicated in the test (D11-02, 11-03-PLAN.md Task 2)", () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "check_jwlcore") return Promise.resolve(CORE_STATUS);
      if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(
      <SettingsProvider>
        <App />
      </SettingsProvider>,
    );

    expect(
      screen.getByRole("heading", { name: en["app.emptyState.title"] }),
    ).toBeInTheDocument();

    const expectedSentence = `${en["app.emptyState.bodyBefore"]}.jwlibrary${en["app.emptyState.bodyAfter"]}`;
    expect(
      screen.getByText(
        (_, element) =>
          element?.tagName.toLowerCase() === "p" && element.textContent === expectedSentence,
      ),
    ).toBeInTheDocument();
  });
});

/**
 * Structural completeness (D11-02, 11-03-PLAN.md Task 2): a plain
 * regex/brace-balance source scan of App.tsx's OWN JSX return block -- NOT
 * a behavioural render check, so it cannot be satisfied by coincidence.
 * Mirrors app/src/theme/styles_tokens.test.ts's (11-01-PLAN.md Task 3)
 * technique of reading a source file at test time via a `?raw` import and
 * scanning it structurally; the identical scan (with its own allowlist) is
 * also applied to SettingsDialog.tsx in
 * app/src/components/SettingsDialog.test.tsx.
 *
 * Scope is deliberately narrowed to the `return (...)` JSX block (not the
 * whole file) so TypeScript generic syntax elsewhere in the file (e.g.
 * `useState<BrowseRow[] | null>`) can never be misread as a stray `>text<`
 * JSX text node by this line-oriented scan.
 */
describe("App structural completeness (D11-02, 11-03-PLAN.md Task 2)", () => {
  // The ONLY literal allowed inside the scanned JSX: the <code> element's
  // own text (".jwlibrary") -- everything else in App.tsx's JSX renders
  // through t(). The product name is not referenced anywhere in App.tsx.
  const ALLOWED_TEXT = [".jwlibrary"];
  const ALLOWED_ATTRS: string[] = [];

  it("contains zero user-facing string literals outside t() calls, except the allowlisted <code> element text", () => {
    const found = findDisallowedLiterals(appSource, ALLOWED_TEXT, ALLOWED_ATTRS);
    expect(found).toEqual([]);
  });
});

/**
 * Extracts the `return ( ... )` JSX block via paren-balance counting, then
 * scans it for (a) non-empty JSX text nodes and (b) `aria-label=`/`title=`/
 * `placeholder=` string-literal attributes -- after first stripping every
 * `{...}` JS/JSX expression slot (also via brace-balance counting) to
 * spaces, so `{t("...")}` calls and arrow-function handlers (`=>` contains
 * a bare `>`) never register as a literal. Duplicated (not imported) from
 * SettingsDialog.test.tsx's identical helper -- this task's file list is
 * these three test files only, and the two call sites scan different
 * source strings with different allowlists, so a tiny shared scanner
 * function is kept local to each rather than introducing a new shared test
 * util module.
 */
function findDisallowedLiterals(
  source: string,
  allowedText: string[],
  allowedAttrs: string[],
): string[] {
  const returnIndex = source.indexOf("return (");
  if (returnIndex === -1) {
    throw new Error("could not find a `return (` JSX block in source");
  }
  const openParenIndex = source.indexOf("(", returnIndex);
  let depth = 0;
  let endIndex = openParenIndex;
  for (; endIndex < source.length; endIndex++) {
    if (source[endIndex] === "(") depth++;
    else if (source[endIndex] === ")") {
      depth--;
      if (depth === 0) break;
    }
  }
  const jsx = source.slice(openParenIndex + 1, endIndex);

  let stripped = "";
  let braceDepth = 0;
  for (const ch of jsx) {
    if (ch === "{") {
      braceDepth++;
      continue;
    }
    if (ch === "}") {
      braceDepth = Math.max(0, braceDepth - 1);
      continue;
    }
    stripped += braceDepth > 0 ? " " : ch;
  }

  const found: string[] = [];

  const textNodePattern = />([^<>]*)</g;
  let match: RegExpExecArray | null;
  while ((match = textNodePattern.exec(stripped)) !== null) {
    const text = match[1].replace(/\s+/g, " ").trim();
    if (text.length === 0 || allowedText.includes(text)) continue;
    found.push(`JSX text node: "${text}"`);
  }

  const attrPattern = /\b(aria-label|title|placeholder)\s*=\s*"([^"]*)"/g;
  while ((match = attrPattern.exec(stripped)) !== null) {
    const [, attr, value] = match;
    if (allowedAttrs.includes(value)) continue;
    found.push(`${attr}="${value}"`);
  }

  return found;
}

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

    // Notes + a selection: the live Delete op is present and enabled, and
    // Notes:export/import (08-04-PLAN.md) render as their own live buttons.
    fireEvent.click(screen.getAllByTestId("category-list-row-checkbox")[0]);
    const deleteButton = screen.getByTestId("category-list-delete-button");
    expect(deleteButton).toBeEnabled();
    expect(deleteButton).toHaveTextContent("Delete (1)");

    expect(screen.getByTestId("category-list-export-button")).toBeInTheDocument();
    expect(screen.getByTestId("category-list-import-button")).toBeInTheDocument();

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
