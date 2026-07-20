import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import NotesList from "./components/NotesList";
import type { NotesRow } from "./bindings/NotesRow";
import type { ErrorDto } from "./bindings/ErrorDto";

/**
 * Walking Skeleton shell. `open_archive` is wired here (01-07) through the
 * Tauri native file-open dialog — never a raw JS path string (T-07-05).
 * `Save` / `Save As` / `New Archive` remain disabled; 01-05 wires those.
 */
export default function App() {
  const [notes, setNotes] = useState<NotesRow[] | null>(null);
  const [error, setError] = useState<ErrorDto | null>(null);

  async function handleOpenArchive() {
    setError(null);
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "JW Library Backup", extensions: ["jwlibrary"] }],
    });
    if (typeof selected !== "string") {
      return; // user cancelled the dialog
    }

    try {
      const result = await invoke<NotesRow[]>("open_archive", {
        path: selected,
      });
      setNotes(result);
    } catch (err) {
      setNotes(null);
      setError(err as ErrorDto);
    }
  }

  const archiveOpen = notes !== null;

  return (
    <div className="app-shell">
      <div className="toolbar">
        <button
          type="button"
          className="toolbar-button"
          onClick={handleOpenArchive}
        >
          Open Archive
        </button>
        <button type="button" className="toolbar-button" disabled>
          New Archive
        </button>
        <button type="button" className="toolbar-button" disabled>
          Save
        </button>
        <button type="button" className="toolbar-button" disabled>
          Save As
        </button>
      </div>

      {error && (
        <div className="error-banner" role="alert">
          {error.message_key}
          {error.safe_file_name ? ` (${error.safe_file_name})` : ""}
        </div>
      )}

      {archiveOpen ? (
        <main className="notes-main">
          <NotesList notes={notes} />
        </main>
      ) : (
        <main className="empty-state">
          <h1>No archive open</h1>
          <p>
            Open a <code>.jwlibrary</code> file to view your Notes, or create
            a new archive.
          </p>
          <div className="empty-state-actions">
            <button
              type="button"
              className="toolbar-button"
              onClick={handleOpenArchive}
            >
              Open Archive
            </button>
            <button type="button" className="toolbar-button" disabled>
              New Archive
            </button>
          </div>
        </main>
      )}
    </div>
  );
}
