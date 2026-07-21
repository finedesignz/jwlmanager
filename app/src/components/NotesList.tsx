import { useCallback, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import type { NotesRow } from "../bindings/NotesRow";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import DeletePreviewDialog from "./DeletePreviewDialog";

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

interface NotesListProps {
  notes: NotesRow[];
  /** Called after a successful delete apply with the post-delete list
   * (deleted rows filtered out locally — no full reload, finding 8). */
  onNotesChanged?: (notes: NotesRow[]) => void;
  /** Routes a delete-flow `ErrorDto` to the app's existing error banner. */
  onError?: (err: ErrorDto) => void;
}

/**
 * Windowed (TanStack Virtual) render of `NotesRow[]` from the `open_archive`
 * IPC command. Renders only the rows in/near the visible viewport so a
 * 9,000+ row archive stays responsive (DATA-01), especially on Linux
 * WebKitGTK (RESEARCH.md D-10) where a naive full-DOM render is a perf
 * cliff. Every row is a fixed 44px, single-line truncated (no wrap) so the
 * fixed-size virtualizer's measurements can never desync (finding 14).
 *
 * Also owns the destructive-delete slice (SAFE-01/SAFE-03): a per-row
 * checkbox selection (keyed by `NoteId`, so it survives virtualization —
 * selecting a row that scrolls out of view stays selected), a Delete
 * affordance disabled unless `selection.size >= 1` (UI-tier defense-in-depth;
 * the backend's `NonEmptyNoteIds` is the real enforcement boundary, D2-06),
 * and the `DeletePreviewDialog` wiring: Delete runs `delete_notes_dry_run`
 * and opens the preview; Confirm/Cancel live entirely inside the dialog.
 */
export default function NotesList({ notes, onNotesChanged, onError }: NotesListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<Set<bigint>>(new Set());
  const [report, setReport] = useState<DryRunReport | null>(null);
  const [dryRunPending, setDryRunPending] = useState(false);

  const virtualizer = useVirtualizer({
    count: notes.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  const toggleSelected = useCallback((id: bigint) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const handleDeleteClick = useCallback(async () => {
    if (selected.size === 0 || dryRunPending) {
      return;
    }
    setDryRunPending(true);
    try {
      const ids = Array.from(selected);
      const dryRunReport = await invoke<DryRunReport>("delete_notes_dry_run", { ids });
      setReport(dryRunReport);
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setDryRunPending(false);
    }
  }, [selected, dryRunPending, onError]);

  const handleConfirm = useCallback(async () => {
    const ids = Array.from(selected);
    try {
      await invoke("delete_notes_apply", { ids });
      onNotesChanged?.(notes.filter((note) => !selected.has(note.id)));
      setSelected(new Set());
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setReport(null);
    }
  }, [selected, notes, onNotesChanged, onError]);

  const handleCancel = useCallback(() => {
    setReport(null);
  }, []);

  if (notes.length === 0) {
    return <p className="notes-list-empty">No notes in this archive.</p>;
  }

  const virtualRows = virtualizer.getVirtualItems();

  return (
    <div className="notes-list-container">
      <div className="notes-list-toolbar">
        <button
          type="button"
          className="toolbar-button notes-list-delete-button"
          onClick={handleDeleteClick}
          disabled={selected.size === 0 || dryRunPending}
          data-testid="notes-list-delete-button"
        >
          {dryRunPending ? "Preparing…" : `Delete (${selected.size})`}
        </button>
      </div>
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
                <input
                  type="checkbox"
                  className="notes-list-row-checkbox"
                  data-testid="notes-list-row-checkbox"
                  aria-label={`Select ${resolveLabel(note)}`}
                  checked={selected.has(note.id)}
                  onChange={() => toggleSelected(note.id)}
                />
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
      {report && (
        <DeletePreviewDialog
          report={report}
          onConfirm={handleConfirm}
          onCancel={handleCancel}
        />
      )}
    </div>
  );
}
