import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { DryRunReport } from "../bindings/DryRunReport";
import DeletePreviewDialog from "./DeletePreviewDialog";

type ActionName = "open" | "new" | "save" | "saveAs" | "saveV14" | "merge";

/** In-flight v14 export: the dry-run preview + the chosen target path,
 * held until the user confirms or cancels the reused preview dialog. */
interface V14Preview {
  report: DryRunReport;
  target: string;
}

/** In-flight merge: the dry-run preview + the chosen SOURCE archive path,
 * held until the user confirms or cancels the reused preview dialog. The
 * source archive is only ever READ (MERGE-02) — nothing is written until
 * Confirm calls `merge_commit`. */
interface MergePreview {
  report: DryRunReport;
  sourcePath: string;
}

/** Sums every per-table count in a `DryRunReport` bucket (added/overwritten). */
function sumCounts(bucket: { [key in string]: number }): number {
  return Object.values(bucket).reduce((total, n) => total + n, 0);
}

/** Base file name (no directory) of a source path, for the merge summary. */
function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

interface CommandBarProps {
  /** Disables Save/Save As per UI-SPEC ("disable Save/Save As when no archive is open"). */
  archiveOpen: boolean;
  onOpened: (notes: BrowseRow[]) => void;
  onNewArchive: () => void;
  onSaved: () => void;
  onError: (err: ErrorDto) => void;
  /** Native dialog dismissed — a clean cancel, never an error (finding 15). */
  onCancelled: () => void;
}

const FILTERS = [{ name: "JW Library Backup", extensions: ["jwlibrary"] }];

/**
 * Open / New / Save / Save As command bar (ARCH-06/07 UI). Every file action
 * carries: an explicit per-action pending state (disables ALL file actions
 * while any invoke is in flight), a double-click guard via a synchronous
 * ref check (a second click while pending is a no-op, never a duplicate
 * concurrent invoke — finding 15/T-06-03), and treats a dismissed native
 * open/save dialog as a clean cancel (no error banner).
 */
export default function CommandBar({
  archiveOpen,
  onOpened,
  onNewArchive,
  onSaved,
  onError,
  onCancelled,
}: CommandBarProps) {
  const [pending, setPending] = useState<ActionName | null>(null);
  // The v14 export is preview-then-write (D4-07): the dry-run report + chosen
  // path live here between the dialog opening and Confirm/Cancel. `null` = no
  // preview open.
  const [v14Preview, setV14Preview] = useState<V14Preview | null>(null);
  // The merge is preview-then-write too (D5-05): the dry-run report + chosen
  // SOURCE archive path live here between the dialog opening and Confirm/Cancel.
  // `null` = no preview open.
  const [mergePreview, setMergePreview] = useState<MergePreview | null>(null);
  // Ref (not state) so the guard check is synchronous: two rapid clicks
  // dispatched before React re-renders must still see the first click's
  // "busy" flag, which `setState` alone cannot guarantee.
  const busyRef = useRef(false);

  const runAction = useCallback(async (name: ActionName, action: () => Promise<void>) => {
    if (busyRef.current) {
      return; // double-click guard: no-op, not a duplicate invoke
    }
    busyRef.current = true;
    setPending(name);
    try {
      await action();
    } finally {
      busyRef.current = false;
      setPending(null);
    }
  }, []);

  const handleOpen = useCallback(
    () =>
      runAction("open", async () => {
        const selected = await open({
          multiple: false,
          directory: false,
          filters: FILTERS,
        });
        if (typeof selected !== "string") {
          onCancelled();
          return;
        }
        try {
          const notes = await invoke<BrowseRow[]>("open_archive", { path: selected });
          onOpened(notes);
        } catch (err) {
          onError(err as ErrorDto);
        }
      }),
    [runAction, onOpened, onError, onCancelled],
  );

  const handleNew = useCallback(
    () =>
      runAction("new", async () => {
        const target = await save({
          filters: FILTERS,
          defaultPath: "New Archive.jwlibrary",
        });
        if (typeof target !== "string") {
          onCancelled();
          return;
        }
        try {
          await invoke("new_archive", { path: target });
          onNewArchive();
        } catch (err) {
          onError(err as ErrorDto);
        }
      }),
    [runAction, onNewArchive, onError, onCancelled],
  );

  const handleSave = useCallback(
    () =>
      runAction("save", async () => {
        try {
          await invoke("save_archive");
          onSaved();
        } catch (err) {
          onError(err as ErrorDto);
        }
      }),
    [runAction, onSaved, onError],
  );

  const handleSaveAs = useCallback(
    () =>
      runAction("saveAs", async () => {
        const target = await save({
          filters: FILTERS,
          defaultPath: "Archive.jwlibrary",
        });
        if (typeof target !== "string") {
          onCancelled();
          return;
        }
        try {
          await invoke("save_as", { path: target });
          onSaved();
        } catch (err) {
          onError(err as ErrorDto);
        }
      }),
    [runAction, onSaved, onError, onCancelled],
  );

  // "Save v14-compatible copy…" (D4-07/SCHEMA-03): an EXPLICIT export, never
  // the default Save. Preview-then-write — pick a target, run the dry-run to
  // learn how many Locations will merge, and open the reused
  // `DeletePreviewDialog`. Only Confirm calls `save_v14_copy`; Cancel/dismiss
  // write nothing.
  const handleSaveV14 = useCallback(
    () =>
      runAction("saveV14", async () => {
        const target = await save({
          filters: FILTERS,
          defaultPath: "Archive (v14).jwlibrary",
        });
        if (typeof target !== "string") {
          onCancelled();
          return;
        }
        try {
          const report = await invoke<DryRunReport>("downgrade_dry_run");
          setV14Preview({ report, target });
        } catch (err) {
          onError(err as ErrorDto);
        }
      }),
    [runAction, onError, onCancelled],
  );

  const handleV14Confirm = useCallback(async () => {
    if (!v14Preview) {
      return;
    }
    const { target } = v14Preview;
    try {
      await invoke("save_v14_copy", { path: target });
      onSaved();
    } catch (err) {
      onError(err as ErrorDto);
    } finally {
      setV14Preview(null);
    }
  }, [v14Preview, onSaved, onError]);

  const handleV14Cancel = useCallback(() => {
    setV14Preview(null);
  }, []);

  // "Merge Archive…" (MERGE-02): pick a SECOND `.jwlibrary` as the merge
  // source, run the REAL merge on a throwaway copy via `merge_dry_run` to learn
  // the add/overwrite preview, and open the reused `DeletePreviewDialog`. Only
  // Confirm calls `merge_commit`; Cancel/dismiss writes nothing. A missing or
  // wrong-arch jwlCore binary surfaces as a `merge_unavailable` ErrorDto through
  // the existing ErrorBanner copy — an actionable message, never a crash.
  const handleMerge = useCallback(
    () =>
      runAction("merge", async () => {
        const selected = await open({
          multiple: false,
          directory: false,
          filters: FILTERS,
        });
        if (typeof selected !== "string") {
          onCancelled();
          return;
        }
        try {
          const report = await invoke<DryRunReport>("merge_dry_run", {
            sourcePath: selected,
          });
          setMergePreview({ report, sourcePath: selected });
        } catch (err) {
          onError(err as ErrorDto);
        }
      }),
    [runAction, onError, onCancelled],
  );

  const handleMergeConfirm = useCallback(async () => {
    if (!mergePreview) {
      return;
    }
    const { sourcePath } = mergePreview;
    try {
      await invoke("merge_commit", { sourcePath });
      // merge_commit mutates the open session in place (returns void), so
      // re-query the merged Notes and push them through the existing open
      // refresh path (onOpened sets notes + clears any error) — this is the
      // "and refreshes" half of the merge slice.
      const notes = await invoke<BrowseRow[]>("list_notes");
      onOpened(notes);
    } catch (err) {
      onError(err as ErrorDto);
    } finally {
      setMergePreview(null);
    }
  }, [mergePreview, onOpened, onError]);

  const handleMergeCancel = useCallback(() => {
    setMergePreview(null);
  }, []);

  const anyPending = pending !== null;
  const v14LocationsMerged = v14Preview?.report.deleted["Location"] ?? 0;

  return (
    <>
    <div className="toolbar" role="toolbar" aria-label="Archive commands">
      <button
        type="button"
        className="toolbar-button"
        onClick={handleOpen}
        disabled={anyPending}
        aria-busy={pending === "open"}
      >
        {pending === "open" ? "Opening…" : "Open Archive"}
      </button>
      <button
        type="button"
        className="toolbar-button"
        onClick={handleNew}
        disabled={anyPending}
        aria-busy={pending === "new"}
      >
        {pending === "new" ? "Creating…" : "New Archive"}
      </button>
      <button
        type="button"
        className="toolbar-button"
        onClick={handleSave}
        disabled={anyPending || !archiveOpen}
        aria-busy={pending === "save"}
      >
        {pending === "save" ? "Saving…" : "Save"}
      </button>
      <button
        type="button"
        className="toolbar-button"
        onClick={handleSaveAs}
        disabled={anyPending || !archiveOpen}
        aria-busy={pending === "saveAs"}
      >
        {pending === "saveAs" ? "Saving…" : "Save As"}
      </button>
      <button
        type="button"
        className="toolbar-button toolbar-button-secondary"
        onClick={handleSaveV14}
        disabled={anyPending || !archiveOpen}
        aria-busy={pending === "saveV14"}
        data-testid="save-v14-button"
      >
        {pending === "saveV14" ? "Preparing…" : "Save v14-compatible copy…"}
      </button>
      <button
        type="button"
        className="toolbar-button toolbar-button-secondary"
        onClick={handleMerge}
        disabled={anyPending || !archiveOpen}
        aria-busy={pending === "merge"}
        data-testid="merge-button"
      >
        {pending === "merge" ? "Preparing…" : "Merge Archive…"}
      </button>
    </div>
    {v14Preview && (
      <DeletePreviewDialog
        report={v14Preview.report}
        onConfirm={handleV14Confirm}
        onCancel={handleV14Cancel}
        title="Save v14-compatible copy?"
        ariaLabel="Confirm v14-compatible save"
        confirmLabel="Save v14 copy"
        confirmPendingLabel="Saving…"
        summary={
          <>
            {v14LocationsMerged} Location{v14LocationsMerged === 1 ? "" : "s"} will be
            merged for v14 compatibility. This writes a separate copy — your open
            archive is left unchanged.
          </>
        }
      />
    )}
    {mergePreview && (
      <DeletePreviewDialog
        report={mergePreview.report}
        onConfirm={handleMergeConfirm}
        onCancel={handleMergeCancel}
        title="Merge this archive?"
        ariaLabel="Confirm merge"
        confirmLabel="Merge"
        confirmPendingLabel="Merging…"
        summary={
          <>
            {sumCounts(mergePreview.report.added)} record
            {sumCounts(mergePreview.report.added) === 1 ? "" : "s"} added,{" "}
            {sumCounts(mergePreview.report.overwritten)} updated from{" "}
            {baseName(mergePreview.sourcePath)}. This merges into your open
            archive — nothing is written until you save.
          </>
        }
      />
    )}
    </>
  );
}
