import { useCallback } from "react";

interface FoldMergeDialogProps {
  /** The chosen source archive paths, in fold order (top = first, applied
   * first; bottom = last, applied last and wins any contested record). The
   * component holds no copy of this — the parent's array IS the array the
   * user sees, so what later goes to the backend is unambiguously what was
   * shown here. */
  paths: string[];
  /** Called with the FULL reordered/filtered array on any Move up / Move
   * down / Remove — never a delta. */
  onChange: (paths: string[]) => void;
  /** Below 3 entries this is not called; Continue renders disabled with a
   * visible reason instead. */
  onContinue: () => void;
  onCancel: () => void;
}

const MIN_SOURCES = 3;

/** Base file name (no directory) of a source path — same convention as
 * CommandBar's `baseName` helper for the two-archive merge summary. */
function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/**
 * "Merge Multiple Archives…" source list (MERGE-03, D10-01): shows the
 * chosen archives in fold order with per-row Move up / Move down / Remove
 * controls, and states in plain language that the order shown is the fold
 * order and that a record changed in two archives keeps the version from
 * the one lower in the list. Never implies the fold is order-independent.
 *
 * Controlled component, no internal path-list state (07-UI-SPEC/TagDialog
 * convention extended here) — reordering/removal is index-swap/filter over
 * a copy of the parent's array, reported back in full via `onChange`.
 * Continue is unavailable below MIN_SOURCES entries, with the reason shown
 * rather than a silently-dead control (CLAUDE.md/UI-SPEC "title-only" rule
 * does not apply to a disabled-affordance explanation).
 */
export default function FoldMergeDialog({
  paths,
  onChange,
  onContinue,
  onCancel,
}: FoldMergeDialogProps) {
  const handleMoveUp = useCallback(
    (index: number) => {
      if (index <= 0) {
        return;
      }
      const next = [...paths];
      [next[index - 1], next[index]] = [next[index], next[index - 1]];
      onChange(next);
    },
    [paths, onChange],
  );

  const handleMoveDown = useCallback(
    (index: number) => {
      if (index >= paths.length - 1) {
        return;
      }
      const next = [...paths];
      [next[index], next[index + 1]] = [next[index + 1], next[index]];
      onChange(next);
    },
    [paths, onChange],
  );

  const handleRemove = useCallback(
    (index: number) => {
      onChange(paths.filter((_, i) => i !== index));
    },
    [paths, onChange],
  );

  const canContinue = paths.length >= MIN_SOURCES;

  return (
    <div className="edit-preview-overlay" role="presentation">
      <div
        className="fold-merge-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Merge multiple archives"
        data-testid="fold-merge-dialog"
      >
        <h2 className="fold-merge-title">Merge Multiple Archives</h2>
        <p className="fold-merge-order-note" data-testid="fold-merge-order-note">
          Archives merge in the order shown, top to bottom. When two archives
          change the same record, the one lower in the list wins.
        </p>
        <ul className="fold-merge-list" role="list" data-testid="fold-merge-list">
          {paths.map((path, index) => (
            <li
              key={`${index}-${path}`}
              className="fold-merge-row"
              data-testid={`fold-merge-row-${index}`}
              title={path}
            >
              <span className="fold-merge-row-position">{index + 1}</span>
              <span className="fold-merge-row-label">{baseName(path)}</span>
              <button
                type="button"
                className="toolbar-button"
                onClick={() => handleMoveUp(index)}
                disabled={index === 0}
                aria-label={`Move ${baseName(path)} up`}
                data-testid={`fold-merge-row-${index}-up`}
              >
                ▲
              </button>
              <button
                type="button"
                className="toolbar-button"
                onClick={() => handleMoveDown(index)}
                disabled={index === paths.length - 1}
                aria-label={`Move ${baseName(path)} down`}
                data-testid={`fold-merge-row-${index}-down`}
              >
                ▼
              </button>
              <button
                type="button"
                className="toolbar-button"
                onClick={() => handleRemove(index)}
                aria-label={`Remove ${baseName(path)}`}
                data-testid={`fold-merge-row-${index}-remove`}
              >
                ✕
              </button>
            </li>
          ))}
        </ul>
        {!canContinue && (
          <p className="fold-merge-reason" data-testid="fold-merge-reason">
            Pick at least {MIN_SOURCES} archives to merge.
          </p>
        )}
        <div className="fold-merge-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={onCancel}
            data-testid="fold-merge-cancel"
          >
            Cancel
          </button>
          <button
            type="button"
            className="toolbar-button"
            onClick={onContinue}
            disabled={!canContinue}
            data-testid="fold-merge-continue"
          >
            Continue
          </button>
        </div>
      </div>
    </div>
  );
}
