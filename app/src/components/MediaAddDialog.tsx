import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { MediaAddApplyReport } from "../bindings/MediaAddApplyReport";
import type { MediaPrecheckResult } from "../bindings/MediaPrecheckResult";

interface MediaAddDialogProps {
  /** Called after a successful `media_add_apply` — the caller re-fetches the
   * fresh Playlists row list via `list_category` and closes this dialog. */
  onApplied: () => void;
  /** Cancel/dismiss the WHOLE dialog (Cancel button, Esc, or click-outside
   * BEFORE Confirm) — no backend call. */
  onCancel: () => void;
  /** Routes a precheck/apply failure to the app's existing error banner
   * (same `onError` prop every other dialog in this app uses). */
  onError: (err: ErrorDto) => void;
}

/** One row's live state — starts at its precheck classification, and (only
 * on a SUCCESSFUL apply) transitions `"new"` rows to `"added"`. A FAILED
 * apply is a whole-batch error (PD-3) — routed to `onError`, never rendered
 * by flipping any row to a false "added" state (08-UI-SPEC.md). */
type RowStatus = "new" | "duplicate" | "unsupported" | "added";

interface Row {
  path: string;
  status: RowStatus;
  reason: string | null;
}

function fileNameOf(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function rowGlyph(status: RowStatus): { glyph: string; className: string } {
  switch (status) {
    case "added":
      return { glyph: "✓", className: "media-add-status-glyph media-add-status-added" };
    case "unsupported":
      return { glyph: "✕", className: "media-add-status-glyph media-add-status-failed" };
    default:
      return { glyph: "–", className: "media-add-status-glyph media-add-status-muted" };
  }
}

function rowLabel(row: Row): string {
  switch (row.status) {
    case "new":
      return "new";
    case "duplicate":
      return "already added";
    case "unsupported":
      return row.reason ? `unreadable — ${row.reason}` : "unreadable — not a supported image";
    case "added":
      return "added";
  }
}

/**
 * "Add Media…" (Playlists, IO-02, 08-06-PLAN.md Task 2): native multi-file
 * picker -> SHA-256 pre-check (the confirm surface, D8-06) -> Confirm ->
 * a single atomic `media_add_apply` invoke -> post-completion result list.
 * NOT an `EditPreviewDialog` instance — the per-file granularity here has no
 * `DryRunReport`-shaped analog (08-UI-SPEC.md's documented deviation).
 *
 * [Claude's Discretion, documented deviation from 08-UI-SPEC.md]: the
 * UI-SPEC's Add Media copy never specifies how the destination PLAYLIST
 * (its Tag `Name`) is chosen — Python's own dialog has a "select existing
 * playlist or type a new name" combo box with no UI-SPEC equivalent
 * described for this port. Rule 2 (auto-add missing critical
 * functionality): without SOME way to name the destination playlist, this
 * command cannot function at all, so a single text input is added ahead of
 * the file-result list, defaulting to empty (Confirm additionally requires
 * a non-empty trimmed name). No new color/spacing token — reuses the
 * app's existing text-input box metrics (44px, `--bg-tertiary`).
 */
export default function MediaAddDialog({ onApplied, onCancel, onError }: MediaAddDialogProps) {
  const [playlistName, setPlaylistName] = useState("");
  const [rows, setRows] = useState<Row[] | null>(null);
  const [prechecking, setPrechecking] = useState(false);
  const [applying, setApplying] = useState(false);
  const [completed, setCompleted] = useState(false);
  const [addedCount, setAddedCount] = useState(0);
  const busyRef = useRef(false);

  useEffect(() => {
    busyRef.current = prechecking || applying;
  }, [prechecking, applying]);

  const handlePickFiles = useCallback(async () => {
    if (busyRef.current) {
      return;
    }
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: "Images", extensions: ["bmp", "gif", "heic", "jpg", "jpeg", "png"] }],
    });
    const paths = Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : null;
    if (paths === null || paths.length === 0) {
      return; // cancelled — no backend call
    }
    setPrechecking(true);
    try {
      const results = await invoke<MediaPrecheckResult[]>("media_add_precheck", { paths });
      setRows(
        results.map((r) => ({
          path: r.path,
          status: r.status as RowStatus,
          reason: r.reason,
        })),
      );
    } catch (err) {
      onError(err as ErrorDto);
    } finally {
      setPrechecking(false);
    }
  }, [onError]);

  const newCount = rows?.filter((r) => r.status === "new").length ?? 0;
  const allDuplicates = rows !== null && rows.length > 0 && rows.every((r) => r.status === "duplicate");
  const trimmedName = playlistName.trim();

  const handleConfirm = useCallback(async () => {
    if (busyRef.current || rows === null || newCount === 0 || trimmedName === "") {
      return;
    }
    busyRef.current = true;
    setApplying(true);
    try {
      const paths = rows.filter((r) => r.status === "new").map((r) => r.path);
      const report = await invoke<MediaAddApplyReport>("media_add_apply", {
        paths,
        playlistName: trimmedName,
      });
      setRows((prev) =>
        (prev ?? []).map((r) => (r.status === "new" ? { ...r, status: "added" } : r)),
      );
      setAddedCount(report.added);
      setCompleted(true);
    } catch (err) {
      // Whole-batch failure (PD-3): NO row transitions to "added" — the
      // archive is unchanged, so the row list stays exactly as the
      // pre-check left it and the user can retry Confirm.
      onError(err as ErrorDto);
    } finally {
      busyRef.current = false;
      setApplying(false);
    }
  }, [rows, newCount, trimmedName, onError]);

  const handleCancel = useCallback(() => {
    if (busyRef.current) {
      return; // dismiss guard: never cancel out from under an in-flight call
    }
    onCancel();
  }, [onCancel]);

  const handleDone = useCallback(() => {
    onApplied();
  }, [onApplied]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !completed) {
        handleCancel();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleCancel, completed]);

  const handleOverlayClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      if (event.target === event.currentTarget && !completed) {
        handleCancel();
      }
    },
    [handleCancel, completed],
  );

  const failedCount = rows?.filter((r) => r.status === "unsupported").length ?? 0;

  return (
    <div className="edit-preview-overlay" role="presentation" onClick={handleOverlayClick}>
      <div
        className="media-add-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Add Media"
        data-testid="media-add-dialog"
      >
        <h2 className="edit-preview-title">Add media?</h2>

        {!completed && (
          <div className="media-add-playlist-field">
            <label htmlFor="media-add-playlist-name">Playlist</label>
            <input
              id="media-add-playlist-name"
              type="text"
              className="media-add-playlist-input"
              data-testid="media-add-playlist-name"
              value={playlistName}
              onChange={(e) => setPlaylistName(e.target.value)}
              placeholder="Select existing or type a new playlist name"
              disabled={applying}
            />
          </div>
        )}

        {rows === null ? (
          <div className="media-add-picker">
            <button
              type="button"
              className="toolbar-button"
              onClick={handlePickFiles}
              disabled={prechecking}
              aria-busy={prechecking}
              data-testid="media-add-pick-files"
            >
              {prechecking ? "Checking files…" : "Choose files…"}
            </button>
          </div>
        ) : allDuplicates ? (
          <p className="media-add-empty" data-testid="media-add-all-duplicates">
            All selected files are already in this archive.
          </p>
        ) : (
          <>
            {(applying || completed) && (
              <p className="media-add-list-header" data-testid="media-add-list-header">
                {applying
                  ? `Copying files… 0 of ${newCount}`
                  : `${addedCount} added${failedCount > 0 ? `, ${failedCount} failed` : ""}.`}
              </p>
            )}
            <ul className="media-add-file-list" role="list">
              {rows.map((row, index) => {
                const { glyph, className } = rowGlyph(row.status);
                return (
                  <li
                    key={row.path}
                    className="media-add-file-row"
                    data-testid={`media-add-file-row-${index}`}
                  >
                    <span className={className}>{glyph}</span>
                    <span className="media-add-file-name" title={row.path}>
                      {fileNameOf(row.path)}
                    </span>
                    <span className="media-add-file-status">{rowLabel(row)}</span>
                  </li>
                );
              })}
            </ul>
          </>
        )}

        <div className="edit-preview-actions">
          {completed ? (
            <button
              type="button"
              className="toolbar-button"
              onClick={handleDone}
              data-testid="media-add-done"
            >
              Done
            </button>
          ) : (
            <>
              <button
                type="button"
                className="toolbar-button"
                onClick={handleCancel}
                disabled={applying}
                data-testid="media-add-cancel"
              >
                Cancel
              </button>
              {rows !== null && !allDuplicates && (
                <button
                  type="button"
                  className="toolbar-button"
                  onClick={handleConfirm}
                  disabled={applying || newCount === 0 || trimmedName === ""}
                  aria-busy={applying}
                  data-testid="media-add-confirm"
                >
                  {applying ? `Copying files… 0 of ${newCount}` : `Add Media (${newCount})`}
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
