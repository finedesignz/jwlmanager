import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type { DryRunReport } from "../bindings/DryRunReport";

interface EditPreviewDialogProps {
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
 * Reusable preview-then-confirm surface (D2-08, general per D2-07 so Phase 4's
 * downgrade preview and Phase 5's merge preview reuse it unchanged — and every
 * Phase 7 edit op reuses it again under THIS name, 07-01-PLAN.md D7-01/D7-13:
 * this component is Phase 2's original delete-preview dialog, renamed rather
 * than redesigned, since it was already fully generalized via the prop
 * surface below). Renders the `DryRunReport`'s per-table `deleted` counts in
 * plain language BEFORE any mutation happens (SAFE-01) — Confirm is the only
 * path to the caller's `*_apply` command; Cancel/Esc/click-outside are pure
 * no-ops that never invoke anything.
 *
 * Calm and trustworthy per 01-UI-SPEC, not an alarm: `--bg-secondary` card,
 * hairline border, `rounded-xl`; the destructive red accent is restrained to
 * the Confirm button only, never a full red-flooded modal.
 *
 * Confirm mirrors `CommandBar`'s synchronous busy-ref double-click guard — a
 * second click while the apply is in flight is a no-op, never a duplicate
 * concurrent invoke (T-02-10). Esc-to-cancel and click-outside-to-cancel
 * (07-UI-SPEC Component Inventory's recommended bundle-in fix) are gated
 * behind the SAME `busyRef` guard, so a dismiss can never race an in-flight
 * apply.
 */
export default function EditPreviewDialog({
  report,
  onConfirm,
  onCancel,
  title = "Delete these Notes?",
  ariaLabel = "Confirm delete",
  summary,
  confirmLabel = "Delete",
  confirmPendingLabel = "Deleting…",
}: EditPreviewDialogProps) {
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
    if (busyRef.current) {
      return; // dismiss guard: never cancel out from under an in-flight apply
    }
    onCancel();
  }, [onCancel]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        handleCancel();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleCancel]);

  const handleOverlayClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      // Only a click on the backdrop itself (never a bubbled click from
      // inside the dialog card) counts as "outside".
      if (event.target === event.currentTarget) {
        handleCancel();
      }
    },
    [handleCancel],
  );

  const entries = Object.entries(report.deleted).filter(([, count]) => count > 0);
  const defaultSummary =
    entries.length > 0
      ? entries.map(([table, count]) => `${count} ${table}`).join(", ")
      : "nothing";

  return (
    <div
      className="edit-preview-overlay"
      role="presentation"
      onClick={handleOverlayClick}
    >
      <div
        className="edit-preview-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel}
        data-testid="edit-preview-dialog"
      >
        <h2 className="edit-preview-title">{title}</h2>
        <p className="edit-preview-summary" data-testid="edit-preview-summary">
          {summary ?? (
            <>
              This will remove {defaultSummary} ({report.total_deleted} row
              {report.total_deleted === 1 ? "" : "s"} total). This can't be undone once
              you save.
            </>
          )}
        </p>
        <div className="edit-preview-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={handleCancel}
            disabled={pending}
            data-testid="edit-preview-cancel"
          >
            Cancel
          </button>
          <button
            type="button"
            className="toolbar-button edit-preview-confirm"
            onClick={handleConfirm}
            disabled={pending}
            aria-busy={pending}
            data-testid="edit-preview-confirm"
          >
            {pending ? confirmPendingLabel : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
