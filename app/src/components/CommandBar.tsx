import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { NotesRow } from "../bindings/NotesRow";
import type { ErrorDto } from "../bindings/ErrorDto";

type ActionName = "open" | "new" | "save" | "saveAs";

interface CommandBarProps {
  /** Disables Save/Save As per UI-SPEC ("disable Save/Save As when no archive is open"). */
  archiveOpen: boolean;
  onOpened: (notes: NotesRow[]) => void;
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
          const notes = await invoke<NotesRow[]>("open_archive", { path: selected });
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

  const anyPending = pending !== null;

  return (
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
    </div>
  );
}
