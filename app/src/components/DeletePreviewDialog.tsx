import { useCallback, useRef, useState, type ReactNode } from "react";
import type { DryRunReport } from "../bindings/DryRunReport";

interface DeletePreviewDialogProps {
  report: DryRunReport;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
  /** Dialog heading. Defaults to the Notes-delete copy. */
  title?: string;
  /** `aria-label` for the dialog role. Defaults to "Confirm delete". */
  ariaLabel?: string;
  /** Overrides the summary body entirely (caller-driven copy, e.g. the v14
   * "N Locations will be merged" framing). Defaults to the Notes-delete
   * summary derived from `report.deleted`. */
  summary?: ReactNode;
  /** Confirm button label at rest. Defaults to "Delete". */
  confirmLabel?: string;
  /** Confirm button label while the confirm handler is in flight. Defaults to
   * "Deleting…". */
  confirmPendingLabel?: string;
}

/**
 * Reusable destructive-confirm surface (D2-08, general per D2-07 so Phase 4's
 * downgrade preview and Phase 5's merge preview can reuse it unchanged).
 * Renders the `DryRunReport`'s per-table `deleted` counts in plain language
 * BEFORE any mutation happens (SAFE-01) — Confirm is the only path to
 * `delete_notes_apply`; Cancel is a pure no-op that never invokes anything.
 *
 * Calm and trustworthy per 01-UI-SPEC, not an alarm: `--bg-secondary` card,
 * hairline border, `rounded-xl`; the destructive red accent is restrained to
 * the Confirm button only, never a full red-flooded modal.
 *
 * Confirm mirrors `CommandBar`'s synchronous busy-ref double-click guard —
 * a second click while the apply is in flight is a no-op, never a duplicate
 * concurrent invoke (T-02-10).
 */
export default function DeletePreviewDialog({
  report,
  onConfirm,
  onCancel,
  title = "Delete these Notes?",
  ariaLabel = "Confirm delete",
  summary,
  confirmLabel = "Delete",
  confirmPendingLabel = "Deleting…",
}: DeletePreviewDialogProps) {
  const [pending, setPending] = useState(false);
  const busyRef = useRef(false);

  const handleConfirm = useCallback(async () => {
    if (busyRef.current) {
      return; // double-click guard: no-op, not a duplicate invoke
    }
    busyRef.current = true;
    setPending(true);
    try {
      await onConfirm();
    } finally {
      busyRef.current = false;
      setPending(false);
    }
  }, [onConfirm]);

  const handleCancel = useCallback(() => {
    if (pending) {
      return;
    }
    onCancel();
  }, [pending, onCancel]);

  const entries = Object.entries(report.deleted).filter(([, count]) => count > 0);
  const defaultSummary =
    entries.length > 0
      ? entries.map(([table, count]) => `${count} ${table}`).join(", ")
      : "nothing";

  return (
    <div className="delete-preview-overlay" role="presentation">
      <div
        className="delete-preview-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel}
        data-testid="delete-preview-dialog"
      >
        <h2 className="delete-preview-title">{title}</h2>
        <p className="delete-preview-summary" data-testid="delete-preview-summary">
          {summary ?? (
            <>
              This will remove {defaultSummary} ({report.total_deleted} row
              {report.total_deleted === 1 ? "" : "s"} total). This can't be undone once
              you save.
            </>
          )}
        </p>
        <div className="delete-preview-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={handleCancel}
            disabled={pending}
            data-testid="delete-preview-cancel"
          >
            Cancel
          </button>
          <button
            type="button"
            className="toolbar-button delete-preview-confirm"
            onClick={handleConfirm}
            disabled={pending}
            aria-busy={pending}
            data-testid="delete-preview-confirm"
          >
            {pending ? confirmPendingLabel : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
