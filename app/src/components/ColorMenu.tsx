import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { Category } from "../bindings/Category";
import type { ColorSelection } from "../bindings/ColorSelection";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import EditPreviewDialog from "./EditPreviewDialog";

/** Ported verbatim from `JWLManager.py:3220-3226` (`select_color`) — the
 * archive highlight-color palette is CONTENT data, not app-chrome color
 * (07-UI-SPEC.md), so it lives as a local module constant here (mirroring
 * how `db/labels.rs::COLOR_NAMES` is a Rust module const), never promoted to
 * a `--brand-*`-style CSS custom property. Index = the archive `ColorIndex`. */
const PALETTE: readonly { name: string; hex: string }[] = [
  { name: "Grey", hex: "#808080" },
  { name: "Yellow", hex: "#FAD929" },
  { name: "Green", hex: "#81BD4F" },
  { name: "Blue", hex: "#5EB4EF" },
  { name: "Red", hex: "#DB5D8D" },
  { name: "Orange", hex: "#FF862E" },
  { name: "Purple", hex: "#7B57A7" },
];

interface ColorMenuProps {
  /** Only `"Highlights"` or `"Notes"` are ever passed here — those are the
   * only two categories `operations.ts`'s `CAPABILITY` table offers `color`
   * for. */
  category: Category;
  selectedIds: bigint[];
  /** Called after a successful `color_apply` with the FRESH rows for this
   * category (re-fetched via `list_category` — colors changing is not a
   * deletion, so the caller's local-filter shortcut doesn't apply here). */
  onApplied: (rows: BrowseRow[]) => void;
  /** Closes the WHOLE menu (swatch-row Escape/click-outside, or the picker
   * stage's own dismissal) — no mutation. */
  onCancel: () => void;
  onError: (err: ErrorDto) => void;
}

interface ColorPreview {
  report: DryRunReport;
  colorIndex: number;
  colorName: string;
}

/**
 * "Color" (EDIT-02, 07-02-PLAN.md Task 3): a popover of 7 swatches, anchored
 * by the caller (`CategoryList.tsx`) below-left of the "Color" toolbar
 * button (mirrors Python's own `mapToGlobal(...bottomLeft())` anchor).
 * Self-contained dry-run-then-preview flow — like `FavoriteAddDialog`, this
 * component owns its OWN transition from popover to `EditPreviewDialog`
 * internally (never both at once). Picking a color closes the popover and
 * immediately fires `color_dry_run`; Confirm calls `color_apply`; Cancel/
 * Esc/click-outside at either stage writes nothing.
 *
 * The Grey swatch is `disabled` (not silently clicked-away) when
 * `category === "Highlights"` — Python's `set_color` treats Highlights+Grey
 * as a silent no-op (`JWLManager.py:3255-3256`), which would otherwise read
 * as a broken click. Grey stays enabled for Notes (it still synthesizes a
 * grey-colored UserMark there).
 */
export default function ColorMenu({
  category,
  selectedIds,
  onApplied,
  onCancel,
  onError,
}: ColorMenuProps) {
  const [preview, setPreview] = useState<ColorPreview | null>(null);
  const [dryRunPending, setDryRunPending] = useState(false);
  const busyRef = useRef(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const firstEnabledRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    busyRef.current = dryRunPending;
  }, [dryRunPending]);

  // Focus moves to the first enabled item on open (07-UI-SPEC.md Component
  // Inventory) — Yellow (index 1) when Grey is disabled for Highlights,
  // otherwise Grey (index 0) for Notes.
  useEffect(() => {
    firstEnabledRef.current?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const buildSelection = useCallback(
    (): ColorSelection =>
      category === "Highlights"
        ? { category: "Highlights", ids: selectedIds }
        : { category: "Notes", ids: selectedIds },
    [category, selectedIds],
  );

  const handlePickColor = useCallback(
    async (colorIndex: number, colorName: string) => {
      if (dryRunPending) {
        return;
      }
      setDryRunPending(true);
      try {
        const report = await invoke<DryRunReport>("color_dry_run", {
          selection: buildSelection(),
          colorIndex,
        });
        setPreview({ report, colorIndex, colorName });
      } catch (err) {
        onError(err as ErrorDto);
        onCancel();
      } finally {
        setDryRunPending(false);
      }
    },
    [dryRunPending, buildSelection, onError, onCancel],
  );

  const handlePreviewConfirm = useCallback(async () => {
    if (!preview) {
      return;
    }
    try {
      await invoke("color_apply", {
        selection: buildSelection(),
        colorIndex: preview.colorIndex,
      });
      const freshRows = await invoke<BrowseRow[]>("list_category", { category });
      onApplied(freshRows);
    } catch (err) {
      onError(err as ErrorDto);
      setPreview(null);
    }
  }, [preview, buildSelection, category, onApplied, onError]);

  const handlePreviewCancel = useCallback(() => {
    setPreview(null);
    onCancel();
  }, [onCancel]);

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

  if (preview) {
    const { report, colorName } = preview;
    const synthesized = report.added["UserMark"] ?? 0;
    const recolored = report.overwritten["UserMark"] ?? 0;
    return (
      <EditPreviewDialog
        report={report}
        onConfirm={handlePreviewConfirm}
        onCancel={handlePreviewCancel}
        title={`Change color to ${colorName}?`}
        ariaLabel="Confirm color change"
        confirmLabel="Change Color"
        confirmPendingLabel="Changing…"
        summary={
          synthesized > 0 ? (
            <>
              {recolored} highlight{recolored === 1 ? "" : "s"} will be recolored to{" "}
              {colorName}. {synthesized} note{synthesized === 1 ? "" : "s"} will become
              highlighted in {colorName} for the first time.
            </>
          ) : (
            <>
              {recolored} highlight{recolored === 1 ? "" : "s"} will be recolored to{" "}
              {colorName}.
            </>
          )
        }
      />
    );
  }

  const firstEnabledIndex = category === "Highlights" ? 1 : 0;

  return (
    <div ref={menuRef} className="color-menu" role="menu" aria-label="Color" data-testid="color-menu">
      {PALETTE.map((color, index) => {
        const disabled = category === "Highlights" && index === 0;
        return (
          <button
            key={color.name}
            ref={index === firstEnabledIndex ? firstEnabledRef : undefined}
            type="button"
            role="menuitem"
            className="color-menu-item"
            disabled={disabled}
            title={disabled ? "Grey has no effect on existing highlights" : color.name}
            onClick={() => handlePickColor(index, color.name)}
            data-testid={`color-menu-swatch-${index}`}
          >
            <span
              className="color-menu-swatch"
              style={{ backgroundColor: color.hex }}
              aria-hidden="true"
            />
            <span className="color-menu-item-label">{color.name}</span>
          </button>
        );
      })}
    </div>
  );
}
