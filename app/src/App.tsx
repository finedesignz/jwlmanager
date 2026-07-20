/**
 * Walking Skeleton empty-state shell.
 *
 * Command surface (Open/New/Save/Save As) and IPC wiring are NOT implemented
 * here — this plan (01-01) only stands up the booting shell. `open_archive`
 * and friends are registered by later plans (01-07, 01-05).
 */
export default function App() {
  return (
    <div className="app-shell">
      <div className="toolbar">
        <button type="button" className="toolbar-button" disabled>
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
      <main className="empty-state">
        <h1>No archive open</h1>
        <p>
          Open a <code>.jwlibrary</code> file to view your Notes, or create a
          new archive.
        </p>
        <div className="empty-state-actions">
          <button type="button" className="toolbar-button" disabled>
            Open Archive
          </button>
          <button type="button" className="toolbar-button" disabled>
            New Archive
          </button>
        </div>
      </main>
    </div>
  );
}
