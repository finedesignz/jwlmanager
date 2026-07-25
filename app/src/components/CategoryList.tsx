import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { Category } from "../bindings/Category";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import EditPreviewDialog from "./EditPreviewDialog";
import { operationSet, type Op } from "../lib/operations";

/**
 * Fixed, uniform row height (px) — the `useVirtualizer` `estimateSize` value.
 * DATA-07's 9,000-row story (like DATA-01 before it) depends on every row
 * being exactly this tall and never wrapping (01-04 finding 14, D6-07), or the
 * fixed-size virtualizer mismeasures. MANDATORY for EVERY category, including
 * "smaller" ones — one always-virtualized code path, no per-category opt-out
 * (Linux WebKitGTK DOM-heavy-grid cliff, 06-RESEARCH D6-07).
 */
const ROW_HEIGHT = 44;

/** Inline, single-line truncation guard applied to every row regardless of
 * external CSS load order (defense-in-depth for the 44px/no-wrap contract). */
const NO_WRAP_STYLE: React.CSSProperties = {
  whiteSpace: "nowrap",
  overflow: "hidden",
  textOverflow: "ellipsis",
};

/** Human-facing op labels for the operation bar (deferred affordances). */
const OP_LABEL: Record<Op, string> = {
  delete: "Delete",
  export: "Export",
  view: "View",
  color: "Color",
  tag: "Tag",
  add: "Add",
  import: "Import",
};

/**
 * The `(dryRun, apply)` Tauri command pair backing the LIVE "delete" op for
 * each category, keyed by category. Notes shipped in Phase 2
 * (`delete_notes_dry_run`/`delete_notes_apply`); Favorites lands in
 * 07-01-PLAN.md Task 1 (`favorite_remove_dry_run`/`favorite_remove_apply`).
 * More categories are added here as their own delete backend lands (D7-10)
 * — the render/dispatch logic below never hardcodes a category name; it only
 * asks "is this (category, op) pair LIVE?" (`operations.ts`'s `deferred`
 * flag) and, if so, looks up which commands to invoke here.
 */
const DELETE_COMMANDS: Partial<Record<Category, { dryRun: string; apply: string }>> = {
  Notes: { dryRun: "delete_notes_dry_run", apply: "delete_notes_apply" },
  Favorites: { dryRun: "favorite_remove_dry_run", apply: "favorite_remove_apply" },
};

/**
 * Resolve the primary label for a `BrowseRow` given its category.
 *
 * W1 FIX (plan-check): Playlists set `full`/`short`/`symbol` to the
 * `"* OTHER *"` sentinel (`db::browse::query_playlists`), so the generic
 * `[full, detail1, detail2]` join would surface the sentinel as the primary
 * label and bury the real name. The meaningful label for a playlist item is
 * `detail1` (= `PlaylistItem.Label`), so Playlists is special-cased to prefer
 * it. The playlist *Name* still surfaces via the tags column. Every other
 * category keeps the established publication-title fallback verbatim.
 */
function resolveLabel(row: BrowseRow, category: Category): string {
  if (category === "Playlists") {
    return row.detail1 ?? row.full;
  }
  const parts = [row.full, row.detail1, row.detail2].filter(
    (part): part is string => Boolean(part),
  );
  return parts.length > 0 ? parts.join(" — ") : row.symbol;
}

interface CategoryListProps {
  rows: BrowseRow[];
  category: Category;
  /** Called after a successful delete apply with the post-delete list
   * (deleted rows filtered out locally — no full reload). */
  onRowsChanged?: (rows: BrowseRow[]) => void;
  /** Routes a delete-flow `ErrorDto` to the app's existing error banner. */
  onError?: (err: ErrorDto) => void;
}

/**
 * Windowed (TanStack Virtual) render of `BrowseRow[]` for ANY category
 * (D6-07) — the generalized successor to `NotesList`. Renders only the rows
 * in/near the visible viewport so a 9,000+ row archive stays responsive,
 * especially on Linux WebKitGTK where a naive full-DOM render is a perf cliff.
 * Every row is a fixed 44px, single-line truncated (no wrap) so the fixed-size
 * virtualizer can never desync — and this holds for every category, never
 * dropped for "smaller" ones.
 *
 * Selection (D6-05) is a `Set<bigint>` keyed by `row.id` — the category
 * identity PK the backend set in 06-02, which future Phase 7 mutations will
 * dispatch on — so it survives virtualization (a selected row scrolled out of
 * view stays selected) and is RESET to empty whenever the `category` prop
 * changes (an integer key means nothing across categories).
 *
 * The contextual operation bar is rendered from `operationSet(category,
 * selection.size)` (D6-08). Phase 6 shipped exactly one live mutation — Notes
 * delete; Phase 7 progressively flips more `(category, op)` pairs LIVE
 * (07-01-PLAN.md starts with Favorites delete). Every selection-scoped delete
 * shares ONE dispatch below, keyed by [`DELETE_COMMANDS`] — the render logic
 * never hardcodes a category name, it only asks `operationSet`'s `deferred`
 * flag; every op still not LIVE renders as a visibly-deferred affordance.
 */
export default function CategoryList({
  rows,
  category,
  onRowsChanged,
  onError,
}: CategoryListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<Set<bigint>>(new Set());
  const [report, setReport] = useState<DryRunReport | null>(null);
  const [dryRunPending, setDryRunPending] = useState(false);

  // D6-05: switching categories clears stale integer keys that would collide
  // across categories (a BookmarkId means nothing in the Highlights list).
  useEffect(() => {
    setSelected(new Set());
    setReport(null);
  }, [category]);

  const virtualizer = useVirtualizer({
    count: rows.length,
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

  const deleteCommands = DELETE_COMMANDS[category];

  const handleDeleteClick = useCallback(async () => {
    if (selected.size === 0 || dryRunPending || !deleteCommands) {
      return;
    }
    setDryRunPending(true);
    try {
      const ids = Array.from(selected);
      const dryRunReport = await invoke<DryRunReport>(deleteCommands.dryRun, { ids });
      setReport(dryRunReport);
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setDryRunPending(false);
    }
  }, [selected, dryRunPending, deleteCommands, onError]);

  const handleConfirm = useCallback(async () => {
    if (!deleteCommands) {
      return;
    }
    const ids = Array.from(selected);
    try {
      await invoke(deleteCommands.apply, { ids });
      onRowsChanged?.(rows.filter((row) => !selected.has(row.id)));
      setSelected(new Set());
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setReport(null);
    }
  }, [selected, rows, deleteCommands, onRowsChanged, onError]);

  const handleCancel = useCallback(() => {
    setReport(null);
  }, []);

  const ops = operationSet(category, selected.size);

  if (rows.length === 0) {
    return (
      <p className="notes-list-empty" data-testid="category-list-empty">
        No {category} in this archive.
      </p>
    );
  }

  const virtualRows = virtualizer.getVirtualItems();

  return (
    <div className="notes-list-container" data-testid="category-list-container">
      <div className="notes-list-toolbar category-list-toolbar">
        <span
          className="category-list-selection-count"
          data-testid="category-list-selection-count"
        >
          {selected.size}
        </span>
        {ops.map((state) => {
          // Every LIVE selection-scoped delete shares one dispatch — the
          // command pair is looked up per-category via DELETE_COMMANDS, but
          // whether the button renders live at all is driven entirely by
          // `operations.ts`'s LIVE set (`state.deferred`), never a hardcoded
          // category name here.
          if (state.op === "delete" && !state.deferred) {
            return (
              <button
                key={state.op}
                type="button"
                className="toolbar-button category-list-delete-button"
                onClick={handleDeleteClick}
                disabled={!state.enabled || dryRunPending}
                data-testid="category-list-delete-button"
              >
                {dryRunPending ? "Preparing…" : `Delete (${selected.size})`}
              </button>
            );
          }
          // Every other op is surfaced-but-deferred (no backend mutation yet).
          return (
            <button
              key={state.op}
              type="button"
              className="toolbar-button category-list-op-deferred"
              disabled
              data-deferred="true"
              data-testid={`category-list-op-${state.op}`}
              title="Coming soon"
            >
              {OP_LABEL[state.op]} (soon)
            </button>
          );
        })}
      </div>
      <div
        ref={parentRef}
        className="notes-list-viewport"
        data-testid="category-list-viewport"
      >
        <ul
          className="notes-list"
          role="list"
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {virtualRows.map((virtualRow) => {
            const row = rows[virtualRow.index];
            const label = resolveLabel(row, category);
            return (
              <li
                key={Number(row.id)}
                className="notes-list-row"
                data-testid="category-list-row"
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
                  data-testid="category-list-row-checkbox"
                  aria-label={`Select ${label}`}
                  checked={selected.has(row.id)}
                  onChange={() => toggleSelected(row.id)}
                />
                <span
                  className="notes-list-row-label"
                  data-testid="category-list-row-label"
                  style={{ ...NO_WRAP_STYLE, flex: 1, minWidth: 0 }}
                >
                  {label}
                </span>
                {/* Color column only when the category produced one (Notes/
                    Highlights); absent columns render nothing and NEVER a
                    taller row. */}
                {row.color !== null && (
                  <span className="notes-list-row-color" style={NO_WRAP_STYLE}>
                    {row.color}
                  </span>
                )}
                {row.tags !== null && (
                  <span
                    className="notes-list-row-tags"
                    style={{ ...NO_WRAP_STYLE, flexShrink: 0, maxWidth: "30%" }}
                  >
                    {row.tags}
                  </span>
                )}
                {row.modified !== null && (
                  <span className="notes-list-row-modified">{row.modified}</span>
                )}
              </li>
            );
          })}
        </ul>
      </div>
      {report && (
        <EditPreviewDialog
          report={report}
          onConfirm={handleConfirm}
          onCancel={handleCancel}
        />
      )}
    </div>
  );
}
