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

type PreviewKind = "clean" | "mask" | "sort";

const MASK_IRREVERSIBILITY_WARNING =
  "This permanently replaces all text in this archive with randomized characters — note titles and content, annotation values, bookmark titles and snippets, and location titles. This cannot be undone.";

/**
 * "Utilities ▾" (new `CommandBar` entry, 07-03-PLAN.md Task 3) — the app's
 * first selection-independent operation surface. Popover mirroring
 * `ColorMenu`'s mechanics (8px radius, first-item focus on open, Escape
 * closes and returns focus to the trigger). All three items are LIVE
 * (07-04-PLAN.md Task 3): "Clean Archive…", "Mask Archive…", and
 * "Sort Tags…" each fire their own `*_dry_run` command immediately on click
 * and open `EditPreviewDialog` with their own copy — Mask alone in
 * `requireTypedConfirm="MASK"` mode (D7-08), the strongest confirmation
 * friction in the app, proportional to it being the single most
 * irreversible operation this app has (archive-wide, random, destroys ALL
 * user-authored text, no undo).
 *
 * Clean/Mask/Sort Tags are deliberately ARCHIVE-WIDE, never selection-scoped
 * — none of them enter `operations.ts`'s `LIVE`/`CAPABILITY` descriptor
 * (that table only models per-category, selection-gated ops). They live
 * entirely here and in `CommandBar`'s trigger button. Do not "fix" this by
 * adding a `Notes:clean`/`Notes:mask`/`Notes:sort`-style entry to
 * `operations.ts` — there is no selection to gate any of them on.
 */
export default function UtilitiesMenu({ onCancel, onError, onSorted }: UtilitiesMenuProps) {
  const [dryRunPending, setDryRunPending] = useState(false);
  const [preview, setPreview] = useState<DryRunReport | null>(null);
  const [previewKind, setPreviewKind] = useState<PreviewKind | null>(null);
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
      setPreviewKind("sort");
    } catch (err) {
      onError(err as ErrorDto);
      onCancel();
    } finally {
      setDryRunPending(false);
    }
  }, [dryRunPending, onError, onCancel]);

  const handleCleanClick = useCallback(async () => {
    if (dryRunPending) {
      return;
    }
    setDryRunPending(true);
    try {
      const report = await invoke<DryRunReport>("clean_dry_run");
      setPreview(report);
      setPreviewKind("clean");
    } catch (err) {
      onError(err as ErrorDto);
      onCancel();
    } finally {
      setDryRunPending(false);
    }
  }, [dryRunPending, onError, onCancel]);

  const handleMaskClick = useCallback(async () => {
    if (dryRunPending) {
      return;
    }
    setDryRunPending(true);
    try {
      const report = await invoke<DryRunReport>("mask_dry_run");
      setPreview(report);
      setPreviewKind("mask");
    } catch (err) {
      onError(err as ErrorDto);
      onCancel();
    } finally {
      setDryRunPending(false);
    }
  }, [dryRunPending, onError, onCancel]);

  const handlePreviewConfirm = useCallback(async () => {
    try {
      const command =
        previewKind === "clean" ? "clean_apply" : previewKind === "mask" ? "mask_apply" : "reorder_apply";
      await invoke(command);
      // `onSorted` re-fetches the current category and closes the menu —
      // the SAME refresh Clean/Mask need after mutating archive-wide text,
      // reused here rather than adding a second, functionally-identical
      // callback prop (CommandBar.tsx is not in this plan's file scope).
      onSorted();
    } catch (err) {
      onError(err as ErrorDto);
      setPreview(null);
      setPreviewKind(null);
    }
  }, [onSorted, onError, previewKind]);

  const handlePreviewCancel = useCallback(() => {
    setPreview(null);
    setPreviewKind(null);
    onCancel();
  }, [onCancel]);

  if (preview && previewKind === "sort") {
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

  if (preview && previewKind === "clean") {
    const rowCount = Object.values(preview.overwritten).reduce((sum, n) => sum + n, 0);
    return (
      <EditPreviewDialog
        report={preview}
        onConfirm={handlePreviewConfirm}
        onCancel={handlePreviewCancel}
        title="Clean this archive?"
        ariaLabel="Confirm clean archive"
        confirmLabel="Clean Archive"
        confirmPendingLabel="Cleaning…"
        summary={
          rowCount === 0 ? undefined : (
            <>
              This normalizes hidden separator characters (like non-breaking spaces) in note
              titles, note content, and annotations. {rowCount} row{rowCount === 1 ? "" : "s"}{" "}
              will be updated.
            </>
          )
        }
      />
    );
  }

  if (preview && previewKind === "mask") {
    const rowCount = Object.values(preview.overwritten).reduce((sum, n) => sum + n, 0);
    const tableList = Object.keys(preview.overwritten).join(", ");
    return (
      <EditPreviewDialog
        report={preview}
        onConfirm={handlePreviewConfirm}
        onCancel={handlePreviewCancel}
        title="Mask this archive?"
        ariaLabel="Confirm mask archive"
        confirmLabel="Mask Archive"
        confirmPendingLabel="Masking…"
        requireTypedConfirm="MASK"
        summary={
          <>
            {MASK_IRREVERSIBILITY_WARNING}
            {rowCount > 0 && (
              <>
                <br />
                <br />
                {rowCount} record{rowCount === 1 ? "" : "s"} across {tableList} will be masked.
              </>
            )}
          </>
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
        ref={sortRef}
        type="button"
        role="menuitem"
        className="utilities-menu-item"
        onClick={handleCleanClick}
        disabled={dryRunPending}
        data-testid="utilities-menu-clean"
      >
        {dryRunPending ? "Preparing…" : "Clean Archive…"}
      </button>
      <button
        type="button"
        role="menuitem"
        className="utilities-menu-item"
        onClick={handleMaskClick}
        disabled={dryRunPending}
        data-testid="utilities-menu-mask"
      >
        {dryRunPending ? "Preparing…" : "Mask Archive…"}
      </button>
      <button
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
