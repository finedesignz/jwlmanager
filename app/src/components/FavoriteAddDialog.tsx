import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { FavoriteEdition } from "../bindings/FavoriteEdition";
import EditPreviewDialog from "./EditPreviewDialog";

interface FavoriteAddDialogProps {
  /** Called after a successful `favorite_add_apply` with the FRESH
   * Favorites row list (re-fetched via `list_category` — the apply's own
   * `DryRunReport` doesn't carry the new row's `BrowseRow` shape, unlike
   * delete's client-side filter). The caller is expected to close this
   * dialog in response. */
  onApplied: (rows: BrowseRow[]) => void;
  /** Cancel/dismiss the WHOLE dialog (picker stage Cancel button, Esc, or
   * click-outside while picking) — no mutation, no fetch left in flight
   * matters once unmounted. */
  onCancel: () => void;
  /** Routes any catalog-fetch/dry-run/apply failure to the app's existing
   * error banner (same `onError` prop `CategoryList`/`CommandBar` already
   * use) — per 07-UI-SPEC.md this dialog has no error UI of its own. */
  onError: (err: ErrorDto) => void;
}

/** Sensible default language so the dialog doesn't open to a guaranteed-
 * empty edition list — the bundled catalog only has favorite-eligible
 * editions in ~9 of ~1400 known languages (`ResourceCatalog::all_language_names`'s
 * doc comment), and English is always one of them. Python's own combo box
 * instead forces a blank first choice (`JWLManager.py:3407`); this is a
 * deliberate UX improvement, not a parity requirement. */
const DEFAULT_LANGUAGE = "English";

/**
 * "Add Favorite" (EDIT-05 mark, 07-01-PLAN.md Task 3): pick a language, pick
 * a Bible edition, preview, confirm. Self-contained two-stage flow — like
 * `CommandBar`'s v14-save/Merge preview pattern, this component owns its
 * OWN dry-run-then-preview transition internally rather than delegating
 * preview-state ownership to its caller: the picker (language `<select>` +
 * edition list) renders until `favorite_add_dry_run` succeeds, at which
 * point this component renders `EditPreviewDialog` INSTEAD of the picker
 * (never both at once). Confirm calls `favorite_add_apply`; Cancel/Esc/
 * click-outside at ANY stage writes nothing.
 *
 * `EditPreviewDialog`'s own Cancel/Esc/click-outside (while previewing)
 * falls back to the picker rather than closing this whole dialog — the
 * user can immediately try a different edition without restarting, which
 * matters most for the duplicate-rejection case ("Choose a different
 * edition"). Only the PICKER's own Cancel button (or Esc/click-outside
 * while picking) calls the outer `onCancel` prop and closes everything.
 */
export default function FavoriteAddDialog({
  onApplied,
  onCancel,
  onError,
}: FavoriteAddDialogProps) {
  const [languages, setLanguages] = useState<string[]>([]);
  const [selectedLanguage, setSelectedLanguage] = useState<string | null>(null);
  const [editions, setEditions] = useState<FavoriteEdition[]>([]);
  const [editionsLoading, setEditionsLoading] = useState(false);
  const [selectedEdition, setSelectedEdition] = useState<FavoriteEdition | null>(null);
  const [dryRunPending, setDryRunPending] = useState(false);
  const [preview, setPreview] = useState<DryRunReport | null>(null);
  const busyRef = useRef(false);

  // Fetch the full language catalog once on mount, then default-select
  // English (falling back to the first alphabetical entry on the
  // near-impossible chance the bundled catalog ever lacks it).
  useEffect(() => {
    let cancelled = false;
    invoke<string[]>("list_favorite_languages")
      .then((names) => {
        if (cancelled) return;
        setLanguages(names);
        setSelectedLanguage(
          names.includes(DEFAULT_LANGUAGE) ? DEFAULT_LANGUAGE : (names[0] ?? null),
        );
      })
      .catch((err) => onError(err as ErrorDto));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Re-fetch editions whenever the selected language changes.
  useEffect(() => {
    if (selectedLanguage === null) {
      setEditions([]);
      return;
    }
    let cancelled = false;
    setEditionsLoading(true);
    setSelectedEdition(null);
    invoke<FavoriteEdition[]>("list_favorite_editions", { language: selectedLanguage })
      .then((rows) => {
        if (cancelled) return;
        setEditions(rows);
      })
      .catch((err) => {
        if (cancelled) return;
        onError(err as ErrorDto);
      })
      .finally(() => {
        if (cancelled) return;
        setEditionsLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedLanguage]);

  const handleLanguageChange = useCallback(
    (event: React.ChangeEvent<HTMLSelectElement>) => {
      setSelectedLanguage(event.target.value);
    },
    [],
  );

  const handleAddFavoriteClick = useCallback(async () => {
    if (!selectedEdition || dryRunPending) {
      return;
    }
    setDryRunPending(true);
    try {
      const edition = { symbol: selectedEdition.symbol, language: selectedEdition.language };
      const report = await invoke<DryRunReport>("favorite_add_dry_run", { edition });
      setPreview(report);
    } catch (err) {
      onError(err as ErrorDto);
    } finally {
      setDryRunPending(false);
    }
  }, [selectedEdition, dryRunPending, onError]);

  const handlePreviewConfirm = useCallback(async () => {
    if (!selectedEdition) {
      return;
    }
    const edition = { symbol: selectedEdition.symbol, language: selectedEdition.language };
    try {
      await invoke("favorite_add_apply", { edition });
      const freshRows = await invoke<BrowseRow[]>("list_category", { category: "Favorites" });
      onApplied(freshRows);
    } catch (err) {
      onError(err as ErrorDto);
      // Fall back to the picker (not a full close) so the user can
      // immediately try a different edition — matters most for the
      // duplicate-rejection race between dry-run and apply.
      setPreview(null);
    }
  }, [selectedEdition, onApplied, onError]);

  const handlePreviewCancel = useCallback(() => {
    setPreview(null);
  }, []);

  const handlePickerCancel = useCallback(() => {
    if (busyRef.current) {
      return; // dismiss guard: never cancel out from under an in-flight dry-run
    }
    onCancel();
  }, [onCancel]);

  useEffect(() => {
    busyRef.current = dryRunPending;
  }, [dryRunPending]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        handlePickerCancel();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handlePickerCancel]);

  const handleOverlayClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      if (event.target === event.currentTarget) {
        handlePickerCancel();
      }
    },
    [handlePickerCancel],
  );

  if (preview) {
    return (
      <EditPreviewDialog
        report={preview}
        onConfirm={handlePreviewConfirm}
        onCancel={handlePreviewCancel}
        title="Add this favorite?"
        ariaLabel="Confirm add favorite"
        confirmLabel="Add Favorite"
        confirmPendingLabel="Adding…"
      />
    );
  }

  // Sorted by display title, matching the Python original's own edition
  // combo box (`sorted(filtered['Short'].to_list())`, JWLManager.py:3399).
  const sortedEditions = [...editions].sort((a, b) => a.short.localeCompare(b.short));

  return (
    <div className="edit-preview-overlay" role="presentation" onClick={handleOverlayClick}>
      <div
        className="favorite-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Add Favorite"
        data-testid="favorite-dialog"
      >
        <h2 className="favorite-dialog-title">Add Favorite</h2>

        <div className="favorite-dialog-field">
          <label htmlFor="favorite-dialog-language-select">Language</label>
          <select
            id="favorite-dialog-language-select"
            className="favorite-dialog-select"
            data-testid="favorite-dialog-language"
            value={selectedLanguage ?? ""}
            onChange={handleLanguageChange}
          >
            {languages.map((lang) => (
              <option key={lang} value={lang}>
                {lang}
              </option>
            ))}
          </select>
        </div>

        <div className="favorite-dialog-editions" data-testid="favorite-dialog-editions">
          {editionsLoading ? (
            <p className="favorite-dialog-loading">Loading editions…</p>
          ) : sortedEditions.length === 0 ? (
            <p className="favorite-dialog-empty" data-testid="favorite-dialog-empty">
              No editions found for {selectedLanguage}. Try a different language.
            </p>
          ) : (
            <ul className="favorite-dialog-edition-list" role="list">
              {sortedEditions.map((edition) => {
                const isSelected = selectedEdition?.symbol === edition.symbol;
                return (
                  <li key={edition.symbol}>
                    <button
                      type="button"
                      className={
                        isSelected
                          ? "favorite-dialog-edition-row favorite-dialog-edition-row-selected"
                          : "favorite-dialog-edition-row"
                      }
                      aria-pressed={isSelected}
                      onClick={() => setSelectedEdition(edition)}
                      data-testid={`favorite-dialog-edition-${edition.symbol}`}
                      title={edition.short}
                    >
                      {edition.short}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="favorite-dialog-actions">
          <button
            type="button"
            className="toolbar-button"
            onClick={handlePickerCancel}
            data-testid="favorite-dialog-cancel"
          >
            Cancel
          </button>
          <button
            type="button"
            className="toolbar-button"
            onClick={handleAddFavoriteClick}
            disabled={!selectedEdition || dryRunPending}
            aria-busy={dryRunPending}
            data-testid="favorite-dialog-add"
          >
            {dryRunPending ? "Preparing…" : "Add Favorite"}
          </button>
        </div>
      </div>
    </div>
  );
}
