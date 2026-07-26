import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import EditPreviewDialog from "./EditPreviewDialog";

interface UtilitiesMenuProps {
  /** Closes the WHOLE menu (Escape/click-outside, or a sub-item's own
   * dismissal) — no mutation. */
  onCancel: () => void;
  onError: (err: ErrorDto) => void;
  /** Called after a successful "Sort Tags…" apply — the caller re-fetches
   * whatever category is currently displayed (Sort Tags is archive-wide, so
   * a Notes/tags-column re-render is the only visible effect). */
  onSorted: () => void;
}

const TAG_ASSIGNMENT_NOOP_SUMMARY = "No tag assignments need renumbering.";

/**
 * "Utilities ▾" (new `CommandBar` entry, 07-03-PLAN.md Task 3) — the app's
 * first selection-independent operation surface. Popover mirroring
 * `ColorMenu`'s mechanics (8px radius, first-item focus on open, Escape
 * closes and returns focus to the trigger). Three items: "Clean Archive…"
 * and "Mask Archive…" render present-but-disabled (wired in a later plan,
 * per the deferred-affordance convention already used for op-bar buttons);
 * "Sort Tags…" is the one wired this plan — it fires `reorder_dry_run`
 * immediately and opens `EditPreviewDialog`.
 *
 * Sort Tags is deliberately ARCHIVE-WIDE, never selection-scoped — it does
 * NOT enter `operations.ts`'s `LIVE`/`CAPABILITY` descriptor (that table
 * only models per-category, selection-gated ops). It lives entirely here
 * and in `CommandBar`'s trigger button. Do not "fix" this by adding a
 * `Notes:sort`-style entry to `operations.ts` — there is no selection to
 * gate it on.
 */
export default function UtilitiesMenu({ onCancel, onError, onSorted }: UtilitiesMenuProps) {
  const [dryRunPending, setDryRunPending] = useState(false);
  const [preview, setPreview] = useState<DryRunReport | null>(null);
  const busyRef = useRef(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const sortRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    busyRef.current = dryRunPending;
  }, [dryRunPending]);

  useEffect(() => {
    sortRef.current?.focus();
  }, []);

  const handleCloseMenu = useCallback(() => {
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
        handleCloseMenu();
      }
    };
    const handleOutsideClick = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        handleCloseMenu();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handleOutsideClick);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handleOutsideClick);
    };
  }, [preview, handleCloseMenu]);

  const handleSortTagsClick = useCallback(async () => {
    if (dryRunPending) {
      return;
    }
    setDryRunPending(true);
    try {
      const report = await invoke<DryRunReport>("reorder_dry_run");
      setPreview(report);
    } catch (err) {
      onError(err as ErrorDto);
      onCancel();
    } finally {
      setDryRunPending(false);
    }
  }, [dryRunPending, onError, onCancel]);

  const handlePreviewConfirm = useCallback(async () => {
    try {
      await invoke("reorder_apply");
      onSorted();
    } catch (err) {
      onError(err as ErrorDto);
      setPreview(null);
    }
  }, [onSorted, onError]);

  const handlePreviewCancel = useCallback(() => {
    setPreview(null);
    onCancel();
  }, [onCancel]);

  if (preview) {
    const changed = preview.overwritten["TagMap"] ?? 0;
    return (
      <EditPreviewDialog
        report={preview}
        onConfirm={handlePreviewConfirm}
        onCancel={handlePreviewCancel}
        title="Sort tags?"
        ariaLabel="Confirm sort tags"
        confirmLabel="Sort Tags"
        confirmPendingLabel="Sorting…"
        summary={
          changed === 0 ? (
            <>{TAG_ASSIGNMENT_NOOP_SUMMARY}</>
          ) : (
            <>
              This renumbers tag order for every tagged note, sorted by note. {changed} tag
              assignment{changed === 1 ? "" : "s"} will be renumbered.
            </>
          )
        }
      />
    );
  }

  return (
    <div
      ref={menuRef}
      className="utilities-menu"
      role="menu"
      aria-label="Utilities"
      data-testid="utilities-menu"
    >
      <button
        type="button"
        role="menuitem"
        className="utilities-menu-item"
        disabled
        title="Coming soon"
        data-testid="utilities-menu-clean"
      >
        Clean Archive…
      </button>
      <button
        type="button"
        role="menuitem"
        className="utilities-menu-item"
        disabled
        title="Coming soon"
        data-testid="utilities-menu-mask"
      >
        Mask Archive…
      </button>
      <button
        ref={sortRef}
        type="button"
        role="menuitem"
        className="utilities-menu-item"
        onClick={handleSortTagsClick}
        disabled={dryRunPending}
        data-testid="utilities-menu-sort"
      >
        {dryRunPending ? "Preparing…" : "Sort Tags…"}
      </button>
    </div>
  );
}
