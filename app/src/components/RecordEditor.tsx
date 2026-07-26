import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { Category } from "../bindings/Category";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { RecordEditFields } from "../bindings/RecordEditFields";
import type { RecordEditPayload } from "../bindings/RecordEditPayload";
import type { RecordIdentity } from "../bindings/RecordIdentity";
import EditPreviewDialog from "./EditPreviewDialog";
import { PALETTE } from "./ColorMenu";

interface RecordEditorProps {
  /** Only `"Notes"` or `"Annotations"` are ever passed here — those are the
   * only two categories `operations.ts`'s `CAPABILITY` table offers `view`
   * (rendered as "Edit") for. */
  category: Category;
  /** The single selected `BrowseRow` (the caller — `CategoryList` — only
   * opens this component when exactly one row is selected). For Annotations
   * this row's `text_tag` disambiguates it from sibling rows sharing the
   * same `LocationId` (D7-09). */
  row: BrowseRow;
  /** Called after a successful save or delete with the FRESH rows for this
   * category (re-fetched via `list_category`, same pattern as
   * `ColorMenu`/`TagDialog`). */
  onApplied: (rows: BrowseRow[]) => void;
  /** Closes the editor with no mutation. */
  onCancel: () => void;
  onError: (err: ErrorDto) => void;
}

type PendingPreview = { kind: "save"; report: DryRunReport } | { kind: "delete"; report: DryRunReport };

/** Builds the record-scoped identity from the category + selected row —
 * shared by fetch, save, and delete so the three never drift apart. */
function buildIdentity(category: Category, row: BrowseRow): RecordIdentity {
  if (category === "Notes") {
    return { category: "Notes", note_id: row.id };
  }
  return { category: "Annotations", location_id: row.id, text_tag: row.text_tag ?? "" };
}

/**
 * "Edit" (EDIT-07, 07-05-PLAN.md Task 2): the field-constrained record
 * editor. Despite the toolbar label "Edit", the surface is intentionally
 * narrow (D7-09) — Notes: Title/Content/Color; Annotations: Value only —
 * never an arbitrary SQL grid. Opens by fetching the record's CURRENT field
 * values (`record_fetch`, since `BrowseRow` never carries a Note's own
 * Title/Content or an Annotation's own Value — those are publication-label
 * metadata, not editable content), then "Save Changes" and "Delete" each
 * route through their own dry-run -> `EditPreviewDialog` -> apply chain,
 * exactly like every other Phase 7 op group. "Delete" reuses the SAME
 * single-record identity as fetch/save, scoped to `(LocationId, TextTag)`
 * for Annotations — never the browse-list's over-deleting by-`LocationId`
 * delete (rule #10).
 */
export default function RecordEditor({ category, row, onApplied, onCancel, onError }: RecordEditorProps) {
  const [loading, setLoading] = useState(true);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [colorIndex, setColorIndex] = useState<bigint | null>(null);
  const [value, setValue] = useState("");
  const [dryRunPending, setDryRunPending] = useState(false);
  const [preview, setPreview] = useState<PendingPreview | null>(null);
  const busyRef = useRef(false);

  const identity = buildIdentity(category, row);

  useEffect(() => {
    let cancelled = false;
    invoke<RecordEditFields>("record_fetch", { identity: buildIdentity(category, row) })
      .then((fields) => {
        if (cancelled) return;
        if (fields.category === "Notes") {
          setTitle(fields.title);
          setContent(fields.content);
          setColorIndex(fields.color_index);
        } else {
          setValue(fields.value);
        }
      })
      .catch((err) => onError(err as ErrorDto))
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    busyRef.current = dryRunPending;
  }, [dryRunPending]);

  const buildPayload = useCallback((): RecordEditPayload => {
    if (category === "Notes") {
      return {
        category: "Notes",
        note_id: row.id,
        title,
        content,
        color_index: colorIndex,
      };
    }
    return {
      category: "Annotations",
      location_id: row.id,
      text_tag: row.text_tag ?? "",
      value,
    };
  }, [category, row, title, content, colorIndex, value]);

  const handleSaveClick = useCallback(async () => {
    if (dryRunPending) {
      return;
    }
    setDryRunPending(true);
    try {
      const report = await invoke<DryRunReport>("record_edit_dry_run", {
        payload: buildPayload(),
      });
      setPreview({ kind: "save", report });
    } catch (err) {
      onError(err as ErrorDto);
    } finally {
      setDryRunPending(false);
    }
  }, [dryRunPending, buildPayload, onError]);

  const handleDeleteClick = useCallback(async () => {
    if (dryRunPending) {
      return;
    }
    setDryRunPending(true);
    try {
      const report = await invoke<DryRunReport>("record_delete_dry_run", { identity });
      setPreview({ kind: "delete", report });
    } catch (err) {
      onError(err as ErrorDto);
    } finally {
      setDryRunPending(false);
    }
  }, [dryRunPending, identity, onError]);

  const handlePreviewConfirm = useCallback(async () => {
    if (!preview) {
      return;
    }
    try {
      if (preview.kind === "save") {
        await invoke("record_edit_apply", { payload: buildPayload() });
      } else {
        await invoke("record_delete_apply", { identity });
      }
      const freshRows = await invoke<BrowseRow[]>("list_category", { category });
      onApplied(freshRows);
    } catch (err) {
      onError(err as ErrorDto);
      setPreview(null);
    }
  }, [preview, buildPayload, identity, category, onApplied, onError]);

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
    if (preview.kind === "save") {
      return (
        <EditPreviewDialog
          report={preview.report}
          onConfirm={handlePreviewConfirm}
          onCancel={handlePreviewCancel}
          title="Save these changes?"
          ariaLabel="Confirm save"
          confirmLabel="Save Changes"
          confirmPendingLabel="Saving…"
        />
      );
    }
    // Annotation delete's over-deletion wart (rule #10, 07-UI-SPEC.md):
    // truthful, explicit copy when more than one InputField row would go.
    // The record editor's OWN delete is keyed by (LocationId, TextTag) and
    // never over-deletes on its own — this branch is a defensive backstop,
    // never expected to actually fire for this path, kept per spec anyway.
    const inputFieldDeleted = preview.report.deleted["InputField"] ?? 0;
    const summary =
      category === "Annotations" && inputFieldDeleted > 1 ? (
        <>
          Deleting this annotation removes <strong>all</strong> annotation fields at this
          location, not just this one — {inputFieldDeleted} annotation field
          {inputFieldDeleted === 1 ? "" : "s"} total will be deleted.
        </>
      ) : undefined;
    return (
      <EditPreviewDialog
        report={preview.report}
        onConfirm={handlePreviewConfirm}
        onCancel={handlePreviewCancel}
        title={category === "Notes" ? "Delete this Note?" : "Delete this Annotation?"}
        ariaLabel="Confirm delete"
        confirmLabel="Delete"
        confirmPendingLabel="Deleting…"
        summary={summary}
      />
    );
  }

  return (
    <div className="edit-preview-overlay" role="presentation" onClick={handleOverlayClick}>
      <div
        className="record-editor"
        role="dialog"
        aria-modal="true"
        aria-label={category === "Notes" ? "Edit Note" : "Edit Annotation"}
        data-testid="record-editor"
      >
        <h2 className="record-editor-title" data-testid="record-editor-title">
          {category === "Notes" ? "Edit Note" : "Edit Annotation"}
        </h2>

        {loading ? (
          <p className="record-editor-loading">Loading…</p>
        ) : category === "Notes" ? (
          <>
            <div className="record-editor-field">
              <label htmlFor="record-editor-title-input">Title</label>
              <input
                id="record-editor-title-input"
                type="text"
                className="record-editor-title-input"
                value={title}
                onChange={(event) => setTitle(event.target.value)}
              />
            </div>
            <div className="record-editor-field">
              <label htmlFor="record-editor-content-input">Content</label>
              <textarea
                id="record-editor-content-input"
                className="record-editor-textarea"
                data-testid="record-editor-content"
                value={content}
                onChange={(event) => setContent(event.target.value)}
              />
            </div>
            <div className="record-editor-field">
              <span>Color</span>
              {colorIndex === null && (
                <p className="record-editor-no-color" data-testid="record-editor-no-color">
                  No color
                </p>
              )}
              <div className="record-editor-colors" role="group" aria-label="Color">
                {PALETTE.map((color, index) => {
                  const selected = colorIndex !== null && Number(colorIndex) === index;
                  return (
                    <button
                      key={color.name}
                      type="button"
                      className={
                        selected
                          ? "record-editor-color-swatch record-editor-color-swatch-selected"
                          : "record-editor-color-swatch"
                      }
                      style={{ backgroundColor: color.hex }}
                      title={color.name}
                      aria-label={color.name}
                      aria-pressed={selected}
                      onClick={() => setColorIndex(BigInt(index))}
                      data-testid={`record-editor-color-${index}`}
                    />
                  );
                })}
              </div>
            </div>
          </>
        ) : (
          <div className="record-editor-field">
            <label htmlFor="record-editor-value-input">Value</label>
            <textarea
              id="record-editor-value-input"
              className="record-editor-textarea"
              data-testid="record-editor-content"
              value={value}
              onChange={(event) => setValue(event.target.value)}
            />
          </div>
        )}

        <div className="record-editor-actions">
          <button
            type="button"
            className="toolbar-button record-editor-delete-button"
            onClick={handleDeleteClick}
            disabled={loading || dryRunPending}
            data-testid="record-editor-delete"
          >
            Delete
          </button>
          <div className="record-editor-actions-right">
            <button
              type="button"
              className="toolbar-button"
              onClick={handleDialogCancel}
              disabled={dryRunPending}
              data-testid="record-editor-cancel"
            >
              Cancel
            </button>
            <button
              type="button"
              className="toolbar-button"
              onClick={handleSaveClick}
              disabled={loading || dryRunPending}
              data-testid="record-editor-save"
            >
              {dryRunPending ? "Preparing…" : "Save Changes"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
