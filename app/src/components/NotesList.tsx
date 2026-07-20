import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { NotesRow } from "../bindings/NotesRow";

/**
 * Fixed, uniform row height (px). This is the perf-relevant constant
 * `useVirtualizer`'s `estimateSize` returns — DATA-01's 9,000-row story
 * depends on every row being exactly this tall, never wrapping (finding 14,
 * 01-04-PLAN.md), or the fixed-size virtualizer mismeasures.
 */
const ROW_HEIGHT = 44;

/** Inline, single-line truncation guard applied to every row regardless of
 * external CSS load order (defense-in-depth for the 44px/no-wrap contract). */
const NO_WRAP_STYLE: React.CSSProperties = {
  whiteSpace: "nowrap",
  overflow: "hidden",
  textOverflow: "ellipsis",
};

/**
 * Resolves the human-readable Notes-list label from a `NotesRow` synthesized
 * by `db::notes::query_notes` (resources.db label synthesis, 01-04). Falls
 * back through publication full title -> Bible detail -> raw symbol, never
 * a raw ID.
 */
function resolveLabel(note: NotesRow): string {
  const parts = [note.full, note.detail1, note.detail2].filter(
    (part): part is string => Boolean(part),
  );
  return parts.length > 0 ? parts.join(" — ") : note.symbol;
}

/**
 * Windowed (TanStack Virtual) render of `NotesRow[]` from the `open_archive`
 * IPC command. Renders only the rows in/near the visible viewport so a
 * 9,000+ row archive stays responsive (DATA-01), especially on Linux
 * WebKitGTK (RESEARCH.md D-10) where a naive full-DOM render is a perf
 * cliff. Every row is a fixed 44px, single-line truncated (no wrap) so the
 * fixed-size virtualizer's measurements can never desync (finding 14).
 */
export default function NotesList({ notes }: { notes: NotesRow[] }) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: notes.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  if (notes.length === 0) {
    return <p className="notes-list-empty">No notes in this archive.</p>;
  }

  const virtualRows = virtualizer.getVirtualItems();

  return (
    <div
      ref={parentRef}
      className="notes-list-viewport"
      data-testid="notes-list-viewport"
    >
      <ul
        className="notes-list"
        role="list"
        style={{ height: virtualizer.getTotalSize(), position: "relative" }}
      >
        {virtualRows.map((virtualRow) => {
          const note = notes[virtualRow.index];
          return (
            <li
              key={Number(note.id)}
              className="notes-list-row"
              data-testid="notes-list-row"
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: `${ROW_HEIGHT}px`,
                transform: `translateY(${virtualRow.start}px)`,
                ...NO_WRAP_STYLE,
              }}
            >
              <span
                className="notes-list-row-label"
                style={{ ...NO_WRAP_STYLE, flex: 1, minWidth: 0 }}
              >
                {resolveLabel(note)}
              </span>
              <span
                className="notes-list-row-tags"
                style={{ ...NO_WRAP_STYLE, flexShrink: 0, maxWidth: "30%" }}
              >
                {note.tags}
              </span>
              <span className="notes-list-row-modified">{note.modified}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
