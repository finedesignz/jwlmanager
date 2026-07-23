import { useState } from "react";
import CategoryList from "./components/CategoryList";
import CategorySwitcher from "./components/CategorySwitcher";
import CommandBar from "./components/CommandBar";
import ErrorBanner from "./components/ErrorBanner";
import JwlCoreNotice from "./components/JwlCoreNotice";
import { invoke } from "@tauri-apps/api/core";
import type { BrowseRow } from "./bindings/BrowseRow";
import type { Category } from "./bindings/Category";
import type { ErrorDto } from "./bindings/ErrorDto";

/**
 * Walking Skeleton shell, now category-aware (DATA-07). `CommandBar` (01-06)
 * wires Open/New/Save/Save As to the IPC commands registered by 01-03/01-05/
 * 01-07. `open_archive` still yields the initial Notes view; thereafter the
 * `CategorySwitcher` (06-03) drives `list_category` (06-02) to swap the
 * rendered rows, and `CategoryList` (06-03) windows any of the six categories.
 * The app owns `{category, rows}`: switching category re-fetches its rows and
 * — because `CategoryList` resets its own selection on the `category` prop
 * change (D6-05) — no stale integer key ever crosses categories. `ErrorBanner`
 * renders every sanitized `ErrorDto` (SAFE-05); `JwlCoreNotice` surfaces the
 * informational arm64 capability gap (D-13a) — never a red error.
 */
export default function App() {
  const [rows, setRows] = useState<BrowseRow[] | null>(null);
  const [category, setCategory] = useState<Category>("Notes");
  const [error, setError] = useState<ErrorDto | null>(null);

  const archiveOpen = rows !== null;

  function handleOpened(result: BrowseRow[]) {
    setError(null);
    setCategory("Notes");
    setRows(result);
  }

  function handleNewArchive() {
    setError(null);
    setCategory("Notes");
    setRows([]);
  }

  function handleSaved() {
    setError(null);
  }

  function handleRowsChanged(result: BrowseRow[]) {
    setRows(result);
  }

  function handleError(err: ErrorDto) {
    setError(err);
  }

  function handleCancelled() {
    // A dismissed native dialog is a clean cancel — no error, no state change.
  }

  // Switching category re-queries `list_category` and swaps the rendered rows.
  // On failure the prior view stays intact (rows are only replaced on the
  // resolved result) and the error routes through the existing ErrorBanner —
  // a last-write-wins swap is acceptable at solo-desktop scale (T-06-09).
  async function handleSelectCategory(next: Category) {
    try {
      const result = await invoke<BrowseRow[]>("list_category", { category: next });
      setError(null);
      setCategory(next);
      setRows(result);
    } catch (err) {
      handleError(err as ErrorDto);
    }
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
          <CategorySwitcher active={category} onSelect={handleSelectCategory} />
          <CategoryList
            rows={rows}
            category={category}
            onRowsChanged={handleRowsChanged}
            onError={handleError}
          />
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
