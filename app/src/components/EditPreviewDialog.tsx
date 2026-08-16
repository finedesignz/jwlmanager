import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type { DryRunReport } from "../bindings/DryRunReport";
import { useI18n } from "../i18n/I18nContext";

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
  /** When set, gates BOTH the typed-confirm input AND the destructive
   * top-border accent (07-UI-SPEC.md) — no caller can get the stronger
   * visual without also requiring the typed match. Confirm stays disabled
   * until the input's value is an EXACT (case-sensitive, untrimmed) match
   * for this string; `mask`, `Mask`, and ` MASK ` are all still disabled for
   * `requireTypedConfirm="MASK"`. Mask is the only caller of this prop —
   * every other dialog stays as restrained as the shipped delete preview. */
  requireTypedConfirm?: string;
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
  title,
  ariaLabel,
  summary,
  confirmLabel,
  confirmPendingLabel,
  requireTypedConfirm,
}: EditPreviewDialogProps) {
  const { t } = useI18n();
  // Defaults resolve through the catalog (11-04-PLAN.md) -- they can't be
  // plain destructuring defaults since `t` isn't available until the hook
  // above runs.
  const resolvedTitle = title ?? t("editPreviewDialog.defaultTitle");
  const resolvedAriaLabel = ariaLabel ?? t("common.confirmDelete");
  const resolvedConfirmLabel = confirmLabel ?? t("common.delete");
  const resolvedConfirmPendingLabel = confirmPendingLabel ?? t("common.deleting");
  const [pending, setPending] = useState(false);
  const [typedValue, setTypedValue] = useState("");
  const busyRef = useRef(false);
  // Exact, case-sensitive, UNTRIMMED match — " MASK " must stay disabled.
  const typedConfirmSatisfied =
    requireTypedConfirm === undefined || typedValue === requireTypedConfirm;

  const handleConfirm = useCallback(async () => {
    if (busyRef.current) {
      return; // double-click guard: no-op, not a duplicate invoke
    }
    if (!typedConfirmSatisfied) {
      return; // typed-confirm gate: never fires apply on a non-exact match
    }
    busyRef.current = true;
    setPending(true);
    try {
      await onConfirm();
    } finally {
      busyRef.current = false;
      setPending(false);
    }
  }, [onConfirm, typedConfirmSatisfied]);

  const handleTypedInputKeyDown = useCallback((event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      // Enter must never bypass the typed-confirm gate — there is no <form>
      // here (so no implicit submit), but this makes the non-bypassability
      // explicit and directly testable rather than relying on that absence.
      event.preventDefault();
    }
  }, []);

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
      : t("editPreviewDialog.nothing");

  return (
    <div
      className="edit-preview-overlay"
      role="presentation"
      onClick={handleOverlayClick}
    >
      <div
        className={
          requireTypedConfirm !== undefined
            ? "edit-preview-dialog edit-preview-dialog-destructive"
            : "edit-preview-dialog"
        }
        role="dialog"
        aria-modal="true"
        aria-label={resolvedAriaLabel}
        data-testid="edit-preview-dialog"
      >
        <h2 className="edit-preview-title">{resolvedTitle}</h2>
        <p className="edit-preview-summary" data-testid="edit-preview-summary">
          {summary ??
            t("editPreviewDialog.defaultSummary", {
              items: defaultSummary,
              count: report.total_deleted,
              plural: report.total_deleted === 1 ? "" : "s",
            })}
        </p>
        {requireTypedConfirm !== undefined && (
          <input
            type="text"
            className="edit-preview-typed-confirm-input"
            value={typedValue}
            onChange={(event) => setTypedValue(event.target.value)}
            onKeyDown={handleTypedInputKeyDown}
            disabled={pending}
            aria-label={t("editPreviewDialog.typedConfirmAriaLabel", { value: requireTypedConfirm })}
            data-testid="edit-preview-typed-confirm-input"
          />
        )}
        <div className="edit-preview-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={handleCancel}
            disabled={pending}
            data-testid="edit-preview-cancel"
          >
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="toolbar-button edit-preview-confirm"
            onClick={handleConfirm}
            disabled={pending || !typedConfirmSatisfied}
            aria-busy={pending}
            data-testid="edit-preview-confirm"
          >
            {pending ? resolvedConfirmPendingLabel : resolvedConfirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
