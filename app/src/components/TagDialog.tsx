import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { TagState } from "../bindings/TagState";
import EditPreviewDialog from "./EditPreviewDialog";

interface TagDialogProps {
  /** The Notes selection to edit tags for — always non-empty (the "Tag" op
   * is gated on `selectionSize > 0` by `operations.ts`'s `NEEDS_SELECTION`). */
  selectedIds: bigint[];
  /** Called after a successful `tag_apply` with the FRESH Notes rows
   * (re-fetched via `list_category` — tags changing affects the tags
   * column, not a deletion, so the caller's local-filter shortcut doesn't
   * apply here). */
  onApplied: (rows: BrowseRow[]) => void;
  /** Cancel/dismiss the WHOLE dialog — no mutation. */
  onCancel: () => void;
  onError: (err: ErrorDto) => void;
}

/** A checklist row's effective tri-state, after applying any user override
 * on top of the fetched `count`. */
type RowState = "checked" | "unchecked" | "indeterminate";

function deriveState(tag: TagState, override: boolean | undefined, selectionSize: bigint): RowState {
  if (override !== undefined) {
    return override ? "checked" : "unchecked";
  }
  if (tag.count === 0n) {
    return "unchecked";
  }
  if (tag.count === selectionSize) {
    return "checked";
  }
  return "indeterminate";
}

/** Checkbox that also renders the native tri-state `indeterminate` visual
 * (not expressible as a plain JSX prop — must be set imperatively on the
 * DOM node, same reason `ColorMenu`'s focus-first-item uses a ref). */
function TriStateCheckbox({
  state,
  onToggle,
  label,
  testId,
}: {
  state: RowState;
  onToggle: () => void;
  label: string;
  testId: string;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) {
      ref.current.indeterminate = state === "indeterminate";
    }
  }, [state]);
  return (
    <input
      ref={ref}
      type="checkbox"
      checked={state === "checked"}
      onChange={onToggle}
      aria-label={label}
      data-testid={testId}
    />
  );
}

/**
 * "Tag" (EDIT-03, 07-03-PLAN.md Task 3): a tri-state checklist of every
 * `Tag WHERE Type = 1` row over the current Notes selection, plus a
 * new-tag-name input. Toggling a row only records a per-tag boolean
 * OVERRIDE (checked/unchecked) — an untouched row (including an
 * indeterminate one) is left completely alone and produces no delta, so
 * "Apply" only ever sends the tags the user actually interacted with.
 * "Apply" fires `tag_dry_run` once and hands off to `EditPreviewDialog`
 * (does not itself commit) — the same self-contained
 * picker-then-preview shape `FavoriteAddDialog`/`ColorMenu` already use.
 */
export default function TagDialog({
  selectedIds,
  onApplied,
  onCancel,
  onError,
}: TagDialogProps) {
  const [tags, setTags] = useState<TagState[]>([]);
  const [loading, setLoading] = useState(true);
  const [overrides, setOverrides] = useState<Map<bigint, boolean>>(new Map());
  const [newTagNames, setNewTagNames] = useState<string[]>([]);
  const [newTagInput, setNewTagInput] = useState("");
  const [dryRunPending, setDryRunPending] = useState(false);
  const [preview, setPreview] = useState<DryRunReport | null>(null);
  const busyRef = useRef(false);

  const selectionSize = BigInt(selectedIds.length);

  useEffect(() => {
    let cancelled = false;
    invoke<TagState[]>("tag_states", { ids: selectedIds })
      .then((rows) => {
        if (cancelled) return;
        setTags(rows);
      })
      .catch((err) => onError(err as ErrorDto))
      .finally(() => {
        if (cancelled) return;
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    busyRef.current = dryRunPending;
  }, [dryRunPending]);

  const handleToggleExisting = useCallback(
    (tag: TagState) => {
      const current = deriveState(tag, overrides.get(tag.tag_id), selectionSize);
      const next = current !== "checked";
      setOverrides((prev) => {
        const copy = new Map(prev);
        copy.set(tag.tag_id, next);
        return copy;
      });
    },
    [overrides, selectionSize],
  );

  const handleToggleNewTag = useCallback((name: string) => {
    setNewTagNames((prev) => prev.filter((n) => n !== name));
  }, []);

  const handleAddNewTag = useCallback(() => {
    const trimmed = newTagInput.trim();
    if (!trimmed) {
      return;
    }
    setNewTagNames((prev) => (prev.includes(trimmed) ? prev : [...prev, trimmed]));
    setNewTagInput("");
  }, [newTagInput]);

  const handleApplyClick = useCallback(async () => {
    if (dryRunPending) {
      return;
    }
    const removedTagIds: bigint[] = [];
    const addedTagIds: bigint[] = [];
    for (const [tagId, checked] of overrides) {
      (checked ? addedTagIds : removedTagIds).push(tagId);
    }
    if (removedTagIds.length === 0 && addedTagIds.length === 0 && newTagNames.length === 0) {
      return; // nothing to preview
    }
    setDryRunPending(true);
    try {
      const report = await invoke<DryRunReport>("tag_dry_run", {
        ids: selectedIds,
        removedTagIds,
        addedTagIds,
        newTagNames,
      });
      setPreview(report);
    } catch (err) {
      onError(err as ErrorDto);
    } finally {
      setDryRunPending(false);
    }
  }, [dryRunPending, overrides, newTagNames, selectedIds, onError]);

  const handlePreviewConfirm = useCallback(async () => {
    const removedTagIds: bigint[] = [];
    const addedTagIds: bigint[] = [];
    for (const [tagId, checked] of overrides) {
      (checked ? addedTagIds : removedTagIds).push(tagId);
    }
    try {
      await invoke("tag_apply", {
        ids: selectedIds,
        removedTagIds,
        addedTagIds,
        newTagNames,
      });
      const freshRows = await invoke<BrowseRow[]>("list_category", { category: "Notes" });
      onApplied(freshRows);
    } catch (err) {
      onError(err as ErrorDto);
      setPreview(null);
    }
  }, [overrides, newTagNames, selectedIds, onApplied, onError]);

  const handlePreviewCancel = useCallback(() => {
    setPreview(null);
  }, []);

  const handleDialogCancel = useCallback(() => {
    if (busyRef.current) {
      return; // dismiss guard: never cancel out from under an in-flight dry-run
    }
    onCancel();
  }, [onCancel]);

  useEffect(() => {
    if (preview) {
      return; // EditPreviewDialog owns Esc/click-outside once open
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        handleDialogCancel();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [preview, handleDialogCancel]);

  const handleOverlayClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      if (event.target === event.currentTarget) {
        handleDialogCancel();
      }
    },
    [handleDialogCancel],
  );

  if (preview) {
    const itemCount = selectedIds.length;
    return (
      <EditPreviewDialog
        report={preview}
        onConfirm={handlePreviewConfirm}
        onCancel={handlePreviewCancel}
        title={`Update tags for ${itemCount} item${itemCount === 1 ? "" : "s"}?`}
        ariaLabel="Confirm tag update"
        confirmLabel="Update Tags"
        confirmPendingLabel="Updating…"
      />
    );
  }

  const hasNothingToApply =
    overrides.size === 0 && newTagNames.length === 0;

  return (
    <div className="edit-preview-overlay" role="presentation" onClick={handleOverlayClick}>
      <div
        className="tag-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Edit Tags"
        data-testid="tag-dialog"
      >
        <h2 className="tag-dialog-title">Edit Tags</h2>

        <div className="tag-dialog-checklist" data-testid="tag-dialog-checklist">
          {loading ? (
            <p className="tag-dialog-loading">Loading tags…</p>
          ) : tags.length === 0 && newTagNames.length === 0 ? (
            <p className="tag-dialog-empty" data-testid="tag-dialog-empty">
              No tags yet — type a name below to create one.
            </p>
          ) : (
            <ul className="tag-dialog-list" role="list">
              {tags.map((tag) => {
                const state = deriveState(tag, overrides.get(tag.tag_id), selectionSize);
                const label = `${tag.name} (${tag.count})`;
                return (
                  <li
                    key={String(tag.tag_id)}
                    className="tag-dialog-row"
                    data-testid={`tag-dialog-item-${tag.tag_id}`}
                    title={label}
                  >
                    <TriStateCheckbox
                      state={state}
                      onToggle={() => handleToggleExisting(tag)}
                      label={label}
                      testId={`tag-dialog-item-${tag.tag_id}-checkbox`}
                    />
                    <span className="tag-dialog-row-label">{label}</span>
                  </li>
                );
              })}
              {newTagNames.map((name) => (
                <li
                  key={`new-${name}`}
                  className="tag-dialog-row"
                  data-testid={`tag-dialog-item-new-${name}`}
                  title={`${name} (new)`}
                >
                  <TriStateCheckbox
                    state="checked"
                    onToggle={() => handleToggleNewTag(name)}
                    label={`${name} (new)`}
                    testId={`tag-dialog-item-new-${name}-checkbox`}
                  />
                  <span className="tag-dialog-row-label">{name} (new)</span>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="tag-dialog-add-row">
          <input
            type="text"
            className="tag-dialog-add-input"
            data-testid="tag-dialog-add-input"
            placeholder="New tag name…"
            value={newTagInput}
            onChange={(event) => setNewTagInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                handleAddNewTag();
              }
            }}
          />
          <button
            type="button"
            className="toolbar-button"
            onClick={handleAddNewTag}
            disabled={newTagInput.trim().length === 0}
            data-testid="tag-dialog-add-button"
          >
            Add
          </button>
        </div>

        <div className="tag-dialog-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={handleDialogCancel}
            data-testid="tag-dialog-cancel"
          >
            Cancel
          </button>
          <button
            type="button"
            className="toolbar-button"
            onClick={handleApplyClick}
            disabled={dryRunPending || hasNothingToApply}
            aria-busy={dryRunPending}
            data-testid="tag-dialog-apply"
          >
            {dryRunPending ? "Preparing…" : "Apply"}
          </button>
        </div>
      </div>
    </div>
  );
}
