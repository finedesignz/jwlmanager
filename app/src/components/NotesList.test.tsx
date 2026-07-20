import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import NotesList from "./NotesList";
import type { NotesRow } from "../bindings/NotesRow";

function makeNote(id: number, overrides: Partial<NotesRow> = {}): NotesRow {
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
});
