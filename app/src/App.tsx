import { useState } from "react";
import NotesList from "./components/NotesList";
import CommandBar from "./components/CommandBar";
import ErrorBanner from "./components/ErrorBanner";
import JwlCoreNotice from "./components/JwlCoreNotice";
import type { NotesRow } from "./bindings/NotesRow";
import type { ErrorDto } from "./bindings/ErrorDto";

/**
 * Walking Skeleton shell. `CommandBar` (01-06) wires Open/New/Save/Save As
 * to the IPC commands registered by 01-03/01-05/01-07, with pending/cancel/
 * double-click discipline. `ErrorBanner` renders every sanitized `ErrorDto`
 * as an actionable sentence (SAFE-05). `JwlCoreNotice` surfaces the
 * informational arm64 capability gap (D-13a) — never a red error.
 */
export default function App() {
  const [notes, setNotes] = useState<NotesRow[] | null>(null);
  const [error, setError] = useState<ErrorDto | null>(null);

  const archiveOpen = notes !== null;

  function handleOpened(result: NotesRow[]) {
    setError(null);
    setNotes(result);
  }

  function handleNewArchive() {
    setError(null);
    setNotes([]);
  }

  function handleSaved() {
    setError(null);
  }

  function handleError(err: ErrorDto) {
    setError(err);
  }

  function handleCancelled() {
    // A dismissed native dialog is a clean cancel — no error, no state change.
  }

  return (
    <div className="app-shell">
      <CommandBar
        archiveOpen={archiveOpen}
        onOpened={handleOpened}
        onNewArchive={handleNewArchive}
        onSaved={handleSaved}
        onError={handleError}
        onCancelled={handleCancelled}
      />

      <JwlCoreNotice />

      {error && <ErrorBanner error={error} />}

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
        </main>
      )}
    </div>
  );
}
