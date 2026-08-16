import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { Category } from "../bindings/Category";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { IncrementalExportSummary } from "../bindings/IncrementalExportSummary";
import type { NotesImportPreview } from "../bindings/NotesImportPreview";
import type { PlaylistDeleteReport } from "../bindings/PlaylistDeleteReport";
import type { PlaylistImportPreview } from "../bindings/PlaylistImportPreview";
import type { StringKey } from "../i18n/strings";
import { useI18n } from "../i18n/I18nContext";
import { categoryLabel } from "./CategorySwitcher";
import ColorMenu from "./ColorMenu";
import EditPreviewDialog from "./EditPreviewDialog";
import FavoriteAddDialog from "./FavoriteAddDialog";
import MediaAddDialog from "./MediaAddDialog";
import RecordEditor from "./RecordEditor";
import TagDialog from "./TagDialog";
import { operationSet, type Op } from "../lib/operations";

/**
 * Fixed, uniform row height (px) — the `useVirtualizer` `estimateSize` value.
 * DATA-07's 9,000-row story (like DATA-01 before it) depends on every row
 * being exactly this tall and never wrapping (01-04 finding 14, D6-07), or the
 * fixed-size virtualizer mismeasures. MANDATORY for EVERY category, including
 * "smaller" ones — one always-virtualized code path, no per-category opt-out
 * (Linux WebKitGTK DOM-heavy-grid cliff, 06-RESEARCH D6-07).
 */
const ROW_HEIGHT = 44;

/** Inline, single-line truncation guard applied to every row regardless of
 * external CSS load order (defense-in-depth for the 44px/no-wrap contract). */
const NO_WRAP_STYLE: React.CSSProperties = {
  whiteSpace: "nowrap",
  overflow: "hidden",
  textOverflow: "ellipsis",
};

/** Human-facing op label catalog keys for the operation bar (deferred
 * affordances) — 11-04-PLAN.md retrofit; `common.color`/`common.add` reuse
 * the SAME word rendered verbatim elsewhere in the app. */
const OP_LABEL_KEYS: Record<Op, StringKey> = {
  delete: "categoryList.op.delete",
  // Ellipsis signals "opens a file picker" (08-UI-SPEC.md), matching the
  // app's existing convention ("Save v14-compatible copy…", "Merge Archive…").
  export: "categoryList.op.export",
  // "Edit" (EDIT-07, 07-05-PLAN.md) — the `view` op's real name once the
  // field-constrained record editor lands. Safe to rename globally: both
  // `view`-carrying categories (Notes, Annotations) go LIVE simultaneously
  // this phase.
  view: "categoryList.op.view",
  color: "common.color",
  tag: "categoryList.op.tag",
  add: "common.add",
  import: "categoryList.op.import",
};

/**
 * The Tauri command backing the LIVE "export" op for each category
 * (08-01-PLAN.md Task 1) — a single command, no dry-run pair: export never
 * mutates the archive (D8-09).
 */
const EXPORT_COMMANDS: Partial<Record<Category, string>> = {
  Favorites: "export_favorites",
  Bookmarks: "export_bookmarks",
  Annotations: "export_annotations",
  Highlights: "export_highlights",
  Notes: "export_notes",
  Playlists: "export_playlist",
};

/**
 * The Tauri command backing "Export changed…" (IO-04, 09-01-PLAN.md) — a
 * prior-file-then-target-file flow distinct from [`EXPORT_COMMANDS`]'s
 * plain export. Covers all five .txt-wire categories (plans 01-03).
 * Playlists has NO entry and never will (D9-06): its export is a
 * whole-database copy inside a zip container, not per-row wire records —
 * there is nothing to diff. The render/dispatch logic below only asks
 * whether a category has an entry here, never which category it is.
 */
const INCREMENTAL_EXPORT_COMMANDS: Partial<Record<Category, string>> = {
  Notes: "export_notes_incremental",
  Favorites: "export_favorites_incremental",
  Bookmarks: "export_bookmarks_incremental",
  Annotations: "export_annotations_incremental",
  Highlights: "export_highlights_incremental",
};

/**
 * The `(dryRun, apply)` Tauri command pair backing the LIVE "import" op for
 * each category (08-01-PLAN.md Task 3, 08-02-PLAN.md Tasks 1-2). Notes'
 * `dryRun` returns a `NotesImportPreview` (report + the file's own detected
 * bucket-delete character) rather than a bare `DryRunReport` — handled
 * specially in `handleImportClick`/`handleImportConfirm` below (D8-09).
 */
const IMPORT_COMMANDS: Partial<Record<Category, { dryRun: string; apply: string }>> = {
  Favorites: { dryRun: "import_favorites_dry_run", apply: "import_favorites_apply" },
  Bookmarks: { dryRun: "import_bookmarks_dry_run", apply: "import_bookmarks_apply" },
  Annotations: { dryRun: "import_annotations_dry_run", apply: "import_annotations_apply" },
  Highlights: { dryRun: "import_highlights_dry_run", apply: "import_highlights_apply" },
  Notes: { dryRun: "import_notes_dry_run", apply: "import_notes_apply" },
  Playlists: { dryRun: "import_playlist_dry_run", apply: "import_playlist_apply" },
};

/** Sums a `Record<string, number>`-shaped `DryRunReport` field's values. */
function sumCounts(counts: Record<string, number>): number {
  return Object.values(counts).reduce((total, n) => total + n, 0);
}

/**
 * The `(dryRun, apply)` Tauri command pair backing the LIVE "delete" op for
 * each category, keyed by category. Notes shipped in Phase 2
 * (`delete_notes_dry_run`/`delete_notes_apply`); Favorites lands in
 * 07-01-PLAN.md Task 1 (`favorite_remove_dry_run`/`favorite_remove_apply`);
 * Highlights lands in 07-02-PLAN.md Task 3
 * (`highlight_delete_dry_run`/`highlight_delete_apply`, D7-10 — targets
 * `BlockRange` only, never `UserMark`, rule #9). More categories are added
 * here as their own delete backend lands — the render/dispatch logic below
 * never hardcodes a category name; it only asks "is this (category, op)
 * pair LIVE?" (`operations.ts`'s `deferred` flag) and, if so, looks up which
 * commands to invoke here.
 */
const DELETE_COMMANDS: Partial<Record<Category, { dryRun: string; apply: string }>> = {
  Notes: { dryRun: "delete_notes_dry_run", apply: "delete_notes_apply" },
  Favorites: { dryRun: "favorite_remove_dry_run", apply: "favorite_remove_apply" },
  Highlights: { dryRun: "highlight_delete_dry_run", apply: "highlight_delete_apply" },
  // Bookmarks/Annotations (D7-10, 07-05-PLAN.md). Annotations delete is by
  // LocationId — an intentional over-deletion (rule #10): it removes ALL
  // InputField rows at that location, never just one TextTag. This is
  // distinct from the record editor's own (LocationId, TextTag)-scoped
  // single delete (`RecordEditor`'s "Delete" button), which must never be
  // crossed with this one.
  Bookmarks: { dryRun: "bookmark_delete_dry_run", apply: "bookmark_delete_apply" },
  Annotations: { dryRun: "annotation_delete_dry_run", apply: "annotation_delete_apply" },
  // Playlists (D8-07, 08-06-PLAN.md) — ref-counted media delete. Returns a
  // `PlaylistDeleteReport` (a `DryRunReport` PLUS media_removed/media_kept
  // counts), NOT a bare `DryRunReport` like every other category — handled
  // specially below (`playlistDeleteReport` state, distinct from `report`).
  Playlists: { dryRun: "playlist_delete_dry_run", apply: "playlist_delete_apply" },
};

/**
 * Resolve the primary label for a `BrowseRow` given its category.
 *
 * W1 FIX (plan-check): Playlists set `full`/`short`/`symbol` to the
 * `"* OTHER *"` sentinel (`db::browse::query_playlists`), so the generic
 * `[full, detail1, detail2]` join would surface the sentinel as the primary
 * label and bury the real name. The meaningful label for a playlist item is
 * `detail1` (= `PlaylistItem.Label`), so Playlists is special-cased to prefer
 * it. The playlist *Name* still surfaces via the tags column. Every other
 * category keeps the established publication-title fallback verbatim.
 */
function resolveLabel(row: BrowseRow, category: Category): string {
  if (category === "Playlists") {
    return row.detail1 ?? row.full;
  }
  const parts = [row.full, row.detail1, row.detail2].filter(
    (part): part is string => Boolean(part),
  );
  return parts.length > 0 ? parts.join(" — ") : row.symbol;
}

/**
 * Resolve the operation-bar button label for `op` given `category` — same
 * category-specific-override PATTERN `resolveLabel` demonstrates for row
 * labels (most ops render the generic {@link OP_LABEL} verbatim; Favorites'
 * `add` gets "Add Favorite" instead of the generic "Add", which reads as
 * ambiguous alone — 07-UI-SPEC.md). Playlists' `add` (still deferred, media
 * add is Phase 8) keeps the generic "Add" label unchanged.
 */
function resolveOpLabel(op: Op, category: Category, t: ReturnType<typeof useI18n>["t"]): string {
  if (op === "add" && category === "Favorites") {
    return t("categoryList.op.addFavorite");
  }
  // "Add Media…" (Playlists, IO-02, 08-06-PLAN.md) — the ellipsis signals
  // "opens a native multi-file picker, not an instant action" (08-UI-SPEC.md).
  if (op === "add" && category === "Playlists") {
    return t("categoryList.op.addMedia");
  }
  return t(OP_LABEL_KEYS[op]);
}

interface CategoryListProps {
  rows: BrowseRow[];
  category: Category;
  /** Called after a successful delete apply with the post-delete list
   * (deleted rows filtered out locally — no full reload). */
  onRowsChanged?: (rows: BrowseRow[]) => void;
  /** Routes a delete-flow `ErrorDto` to the app's existing error banner. */
  onError?: (err: ErrorDto) => void;
}

/**
 * Windowed (TanStack Virtual) render of `BrowseRow[]` for ANY category
 * (D6-07) — the generalized successor to `NotesList`. Renders only the rows
 * in/near the visible viewport so a 9,000+ row archive stays responsive,
 * especially on Linux WebKitGTK where a naive full-DOM render is a perf cliff.
 * Every row is a fixed 44px, single-line truncated (no wrap) so the fixed-size
 * virtualizer can never desync — and this holds for every category, never
 * dropped for "smaller" ones.
 *
 * Selection (D6-05) is a `Set<bigint>` keyed by `row.id` — the category
 * identity PK the backend set in 06-02, which future Phase 7 mutations will
 * dispatch on — so it survives virtualization (a selected row scrolled out of
 * view stays selected) and is RESET to empty whenever the `category` prop
 * changes (an integer key means nothing across categories).
 *
 * The contextual operation bar is rendered from `operationSet(category,
 * selection.size)` (D6-08). Phase 6 shipped exactly one live mutation — Notes
 * delete; Phase 7 progressively flips more `(category, op)` pairs LIVE
 * (07-01-PLAN.md starts with Favorites delete). Every selection-scoped delete
 * shares ONE dispatch below, keyed by [`DELETE_COMMANDS`] — the render logic
 * never hardcodes a category name, it only asks `operationSet`'s `deferred`
 * flag; every op still not LIVE renders as a visibly-deferred affordance.
 */
export default function CategoryList({
  rows,
  category,
  onRowsChanged,
  onError,
}: CategoryListProps) {
  const { t } = useI18n();
  const parentRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<Set<bigint>>(new Set());
  const [report, setReport] = useState<DryRunReport | null>(null);
  // Playlists delete (D8-07, 08-06-PLAN.md) returns a `PlaylistDeleteReport`
  // (media_removed/media_kept counts alongside the standard report) instead
  // of a bare `DryRunReport` — kept in its OWN state so every other
  // category's delete flow (`report` above) stays untouched.
  const [playlistDeleteReport, setPlaylistDeleteReport] = useState<PlaylistDeleteReport | null>(
    null,
  );
  const [dryRunPending, setDryRunPending] = useState(false);
  // "Add Media…" (Playlists, IO-02, 08-06-PLAN.md Task 2) — its own show/hide
  // flag; MediaAddDialog owns its OWN picker/precheck/apply flow internally,
  // same shape as FavoriteAddDialog/TagDialog.
  const [showMediaAddDialog, setShowMediaAddDialog] = useState(false);
  // Favorites "Add Favorite" (EDIT-05 mark) — a selection-INDEPENDENT op, so
  // it gets its own show/hide flag rather than reusing `report` (which is
  // scoped to the selection-scoped delete flow above).
  const [showFavoriteDialog, setShowFavoriteDialog] = useState(false);
  // "Color" (EDIT-02) — its own popover show/hide flag; ColorMenu owns its
  // OWN dry-run/preview/apply flow internally once open, same shape as
  // FavoriteAddDialog.
  const [showColorMenu, setShowColorMenu] = useState(false);
  const colorButtonRef = useRef<HTMLButtonElement>(null);
  // "Tag" (EDIT-03) — its own show/hide flag; TagDialog owns its OWN
  // fetch/dry-run/preview/apply flow internally once open.
  const [showTagDialog, setShowTagDialog] = useState(false);
  // "Edit" (EDIT-07, the `view` op) -- its own show/hide flag; RecordEditor
  // owns its OWN fetch/dry-run/preview/apply flow internally, same shape as
  // TagDialog/FavoriteAddDialog. Only ever opened at selection size 1
  // (07-UI-SPEC.md's stricter-than-NEEDS_SELECTION precondition).
  const [showRecordEditor, setShowRecordEditor] = useState(false);
  // "Export…" (IO-01, 08-01-PLAN.md) — no preview dialog (D8-09, export
  // never mutates the archive); a transient post-success button-label flash
  // instead of a toast (this app has never shipped a toast system).
  const [exportPending, setExportPending] = useState(false);
  const [exportFlash, setExportFlash] = useState(false);
  const exportFlashTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // "Export changed…" (IO-04, 09-01-PLAN.md) — a prior-file picker THEN a
  // target-file save, then the returned added/modified/deleted-candidate
  // counts rendered in a small dismissible summary (no preview dialog: like
  // "Export…", this never mutates the archive, so there is nothing to
  // confirm — only a result to show).
  const [incrementalExportPending, setIncrementalExportPending] = useState(false);
  const [incrementalExportSummary, setIncrementalExportSummary] =
    useState<IncrementalExportSummary | null>(null);
  // "Import…" (IO-02, 08-01-PLAN.md) — dry-run parses the picked file and
  // returns the would-be result; a malformed file surfaces through
  // `onError` BEFORE this ever opens (D8-04 fail-fast — `importPreview`
  // stays `null`). Separate from `report` (the selection-scoped delete
  // preview above) since import is never selection-scoped.
  const [importPreview, setImportPreview] = useState<{
    report: DryRunReport;
    path: string;
    /** Notes-only (D8-09): the file's own detected bucket-delete character,
     * or `null` when the file names no bucket / the category isn't Notes. */
    bucket: string | null;
    /** Playlists-only: the playlist's name and media-file count, for the
     * UI-SPEC leading clause. `null` for every other category. */
    playlist: { name: string; mediaCount: number } | null;
  } | null>(null);
  const [importPending, setImportPending] = useState(false);
  // Notes-only (D8-09): the explicit user opt-in for the bucket delete the
  // preview surfaced. Reset alongside `importPreview` — never persists past
  // one import flow, and NEVER defaults to true.
  const [bucketDeleteOptIn, setBucketDeleteOptIn] = useState(false);

  useEffect(() => {
    return () => {
      if (exportFlashTimeoutRef.current !== null) {
        clearTimeout(exportFlashTimeoutRef.current);
      }
    };
  }, []);

  // D6-05: switching categories clears stale integer keys that would collide
  // across categories (a BookmarkId means nothing in the Highlights list).
  // Also closes any in-flight Favorite-add dialog — its own overlay blocks
  // category-switching in practice, but this is the same belt-and-suspenders
  // reset the rest of this effect already applies.
  //
  // Reset via the "adjust state during render" pattern (not useEffect):
  // `category` and `rows` change together, in the SAME commit, once
  // `App.handleSelectCategory`'s `list_category` await resolves — a
  // useEffect-based reset lands one commit LATER, which is a real race a
  // caller reading `selected` immediately after the rows swap (e.g. this
  // component's own tests) can observe as stale. Resetting synchronously
  // during render, in the render where `category` itself changes, closes
  // that window entirely.
  const [renderedCategory, setRenderedCategory] = useState(category);
  if (category !== renderedCategory) {
    setRenderedCategory(category);
    setSelected(new Set());
    setReport(null);
    setPlaylistDeleteReport(null);
    setShowFavoriteDialog(false);
    setShowColorMenu(false);
    setShowTagDialog(false);
    setShowRecordEditor(false);
    setShowMediaAddDialog(false);
    setImportPreview(null);
    setBucketDeleteOptIn(false);
    setIncrementalExportSummary(null);
  }

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  const toggleSelected = useCallback((id: bigint) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const deleteCommands = DELETE_COMMANDS[category];

  const handleDeleteClick = useCallback(async () => {
    if (selected.size === 0 || dryRunPending || !deleteCommands) {
      return;
    }
    setDryRunPending(true);
    try {
      const ids = Array.from(selected);
      if (category === "Playlists") {
        const dryRunReport = await invoke<PlaylistDeleteReport>(deleteCommands.dryRun, { ids });
        setPlaylistDeleteReport(dryRunReport);
      } else {
        const dryRunReport = await invoke<DryRunReport>(deleteCommands.dryRun, { ids });
        setReport(dryRunReport);
      }
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setDryRunPending(false);
    }
  }, [selected, dryRunPending, deleteCommands, category, onError]);

  const handleConfirm = useCallback(async () => {
    if (!deleteCommands) {
      return;
    }
    const ids = Array.from(selected);
    try {
      await invoke(deleteCommands.apply, { ids });
      onRowsChanged?.(rows.filter((row) => !selected.has(row.id)));
      setSelected(new Set());
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setReport(null);
      setPlaylistDeleteReport(null);
    }
  }, [selected, rows, deleteCommands, onRowsChanged, onError]);

  const handleCancel = useCallback(() => {
    setReport(null);
    setPlaylistDeleteReport(null);
  }, []);

  const exportCommand = EXPORT_COMMANDS[category];

  // "Export…" (D8-09): native save dialog -> direct file write (no dry-run,
  // no `EditPreviewDialog`) -> transient "Exported" flash or `ErrorBanner`.
  // Selection-optional (D8-10): with a selection, exports exactly the
  // selected rows; with none, exports the whole category.
  const handleExportClick = useCallback(async () => {
    if (exportPending || !exportCommand) {
      return;
    }
    // Playlists' export requires a concrete `PlaylistItemId` selection
    // (re-keying a `.jwlplaylist` needs a real row set, unlike the txt
    // categories' whole-category fallback) — never invoked with an empty
    // selection.
    if (category === "Playlists" && selected.size === 0) {
      return;
    }
    const target =
      category === "Playlists"
        ? await save({
            filters: [{ name: t("categoryList.filterPlaylists"), extensions: ["jwlplaylist"] }],
            defaultPath: `${category}.jwlplaylist`,
          })
        : await save({
            filters: [{ name: t("categoryList.filterTextFiles"), extensions: ["txt"] }],
            defaultPath: `${category}.txt`,
          });
    if (typeof target !== "string") {
      return; // cancelled — no backend call
    }
    setExportPending(true);
    try {
      const ids = selected.size > 0 ? Array.from(selected) : null;
      await invoke(exportCommand, { path: target, ids });
      if (exportFlashTimeoutRef.current !== null) {
        clearTimeout(exportFlashTimeoutRef.current);
      }
      setExportFlash(true);
      exportFlashTimeoutRef.current = setTimeout(() => setExportFlash(false), 2000);
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setExportPending(false);
    }
  }, [exportPending, exportCommand, category, selected, onError, t]);

  const incrementalExportCommand = INCREMENTAL_EXPORT_COMMANDS[category];

  // "Export changed…" (IO-04): a prior-file picker FIRST — cancelling it
  // aborts the whole action (never "no prior file", which is a distinct,
  // explicit choice `export_notes_incremental` only makes when `prior_path`
  // is truly absent) — then the usual target-file save dialog, then the
  // command, then the counts summary. No dry-run/preview step: like plain
  // "Export…", this never mutates the archive.
  const handleIncrementalExportClick = useCallback(async () => {
    if (incrementalExportPending || !incrementalExportCommand) {
      return;
    }
    const priorFile = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t("categoryList.filterTextFiles"), extensions: ["txt"] }],
      title: t("categoryList.priorExportTitle", { category: categoryLabel(category, t) }),
    });
    if (typeof priorFile !== "string") {
      return; // cancelled — no backend call
    }
    const target = await save({
      filters: [{ name: t("categoryList.filterTextFiles"), extensions: ["txt"] }],
      defaultPath: `${category}-changed.txt`,
    });
    if (typeof target !== "string") {
      return; // cancelled — no backend call
    }
    setIncrementalExportPending(true);
    try {
      const summary = await invoke<IncrementalExportSummary>(incrementalExportCommand, {
        path: target,
        priorPath: priorFile,
      });
      setIncrementalExportSummary(summary);
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setIncrementalExportPending(false);
    }
  }, [incrementalExportPending, incrementalExportCommand, category, onError, t]);

  const importCommands = IMPORT_COMMANDS[category];

  // "Import…": native open dialog -> dry-run (fail-fast `ErrorBanner` on a
  // malformed file, D8-04 — `EditPreviewDialog` never opens for it) ->
  // `EditPreviewDialog` with the added/updated/skipped summary.
  const handleImportClick = useCallback(async () => {
    if (importPending || !importCommands) {
      return;
    }
    const selectedFile =
      category === "Playlists"
        ? await open({
            multiple: false,
            directory: false,
            filters: [{ name: t("categoryList.filterPlaylists"), extensions: ["jwlplaylist"] }],
          })
        : await open({
            multiple: false,
            directory: false,
            filters: [{ name: t("categoryList.filterTextFiles"), extensions: ["txt"] }],
          });
    if (typeof selectedFile !== "string") {
      return; // cancelled — no backend call
    }
    setImportPending(true);
    try {
      if (category === "Notes") {
        const preview = await invoke<NotesImportPreview>(importCommands.dryRun, {
          path: selectedFile,
        });
        setImportPreview({
          report: preview.report,
          path: selectedFile,
          bucket: preview.bucket ?? null,
          playlist: null,
        });
      } else if (category === "Playlists") {
        const preview = await invoke<PlaylistImportPreview>(importCommands.dryRun, {
          path: selectedFile,
        });
        setImportPreview({
          report: preview.report,
          path: selectedFile,
          bucket: null,
          playlist: { name: preview.playlist_name, mediaCount: preview.media_count },
        });
      } else {
        const dryRunReport = await invoke<DryRunReport>(importCommands.dryRun, {
          path: selectedFile,
        });
        setImportPreview({ report: dryRunReport, path: selectedFile, bucket: null, playlist: null });
      }
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setImportPending(false);
    }
  }, [importPending, importCommands, category, onError, t]);

  const handleImportConfirm = useCallback(async () => {
    if (!importCommands || !importPreview) {
      return;
    }
    try {
      const args: Record<string, unknown> = { path: importPreview.path };
      // Notes-only (D8-09): the bucket delete NEVER runs unless the user
      // explicitly checked the opt-in — the default `null` matches the
      // command's own "never infer from the file" contract.
      if (category === "Notes") {
        args.bucket = bucketDeleteOptIn ? importPreview.bucket : null;
      }
      await invoke(importCommands.apply, args);
      const freshRows = await invoke<BrowseRow[]>("list_category", { category });
      onRowsChanged?.(freshRows);
    } catch (err) {
      onError?.(err as ErrorDto);
    } finally {
      setImportPreview(null);
      setBucketDeleteOptIn(false);
    }
  }, [importCommands, importPreview, category, bucketDeleteOptIn, onRowsChanged, onError]);

  const handleImportCancel = useCallback(() => {
    setImportPreview(null);
    setBucketDeleteOptIn(false);
  }, []);

  const ops = operationSet(category, selected.size);

  // The single selected row for the record editor (EDIT-07) — only ever
  // read when `selected.size === 1`, so the first (only) match is exact.
  const singleSelectedRow =
    selected.size === 1 ? rows.find((row) => selected.has(row.id)) : undefined;

  // NOTE: unlike the pre-Phase-7 shape, the empty-rows case no longer
  // early-returns before the toolbar — Favorites' "Add Favorite" (EDIT-05
  // mark) is selection-INDEPENDENT and must stay reachable even when the
  // category currently has zero rows (a very common starting state: a
  // fresh archive's Favorites list starts empty, and without this the op
  // would be permanently unreachable). Only the ROW-LIST portion below
  // conditionally renders the empty message instead of the virtualized list.

  const virtualRows = virtualizer.getVirtualItems();

  return (
    <div className="notes-list-container" data-testid="category-list-container">
      <div className="notes-list-toolbar category-list-toolbar">
        <span
          className="category-list-selection-count"
          data-testid="category-list-selection-count"
        >
          {selected.size}
        </span>
        {ops.map((state) => {
          // Every LIVE selection-scoped delete shares one dispatch — the
          // command pair is looked up per-category via DELETE_COMMANDS, but
          // whether the button renders live at all is driven entirely by
          // `operations.ts`'s LIVE set (`state.deferred`), never a hardcoded
          // category name here.
          if (state.op === "delete" && !state.deferred) {
            return (
              <button
                key={state.op}
                type="button"
                className="toolbar-button category-list-delete-button"
                onClick={handleDeleteClick}
                disabled={!state.enabled || dryRunPending}
                data-testid="category-list-delete-button"
              >
                {dryRunPending ? t("common.preparing") : t("categoryList.deleteButton", { count: selected.size })}
              </button>
            );
          }
          // Favorites' "Add Favorite" (EDIT-05 mark) — selection-independent
          // (state.enabled is always true once LIVE, since `add` is not in
          // operations.ts's NEEDS_SELECTION set), so this never gates on
          // `selected.size`. Opens FavoriteAddDialog, which owns its own
          // dry-run/preview/apply flow internally.
          if (state.op === "add" && !state.deferred) {
            return (
              <button
                key={state.op}
                type="button"
                className="toolbar-button category-list-add-button"
                onClick={() =>
                  category === "Playlists" ? setShowMediaAddDialog(true) : setShowFavoriteDialog(true)
                }
                disabled={!state.enabled}
                data-testid="category-list-add-button"
              >
                {resolveOpLabel(state.op, category, t)}
              </button>
            );
          }
          // "Export…" (IO-01, D8-09) — no dry-run, no `EditPreviewDialog`;
          // selection-optional (D8-10), so `state.enabled` is always true
          // once LIVE. The tooltip surfaces the "exports everything" hint
          // only when nothing is selected (08-UI-SPEC.md: title= only, no
          // permanent helper text).
          if (state.op === "export" && !state.deferred) {
            return (
              <span key={state.op} style={{ display: "contents" }}>
                <button
                  type="button"
                  className="toolbar-button category-list-export-button"
                  onClick={handleExportClick}
                  disabled={!state.enabled || exportPending}
                  title={
                    selected.size === 0
                      ? t("categoryList.export.tooltipNoSelection", { category: categoryLabel(category, t) })
                      : undefined
                  }
                  data-testid="category-list-export-button"
                >
                  {exportPending
                    ? t("categoryList.export.pending")
                    : exportFlash
                      ? t("categoryList.export.done")
                      : resolveOpLabel(state.op, category, t)}
                </button>
                {/* "Export changed…" (IO-04, 09-01-PLAN.md) — sits next to
                    plain "Export…", present only for a category in
                    INCREMENTAL_EXPORT_COMMANDS (Notes only this plan). No
                    dry-run; the prior-file picker (cancel = abort) then the
                    save-target picker (cancel = abort) gate the invoke. */}
                {incrementalExportCommand && (
                  <button
                    type="button"
                    className="toolbar-button category-list-export-incremental-button"
                    onClick={handleIncrementalExportClick}
                    disabled={incrementalExportPending}
                    data-testid="category-list-export-incremental-button"
                  >
                    {incrementalExportPending ? t("categoryList.export.pending") : t("categoryList.export.changedButton")}
                  </button>
                )}
              </span>
            );
          }
          // "Import…" (IO-02) — archive-wide-into-category, never
          // selection-gated (D8-10).
          if (state.op === "import" && !state.deferred) {
            return (
              <button
                key={state.op}
                type="button"
                className="toolbar-button category-list-import-button"
                onClick={handleImportClick}
                disabled={!state.enabled || importPending}
                data-testid="category-list-import-button"
              >
                {importPending ? t("categoryList.import.pending") : resolveOpLabel(state.op, category, t)}
              </button>
            );
          }
          // "Color" (EDIT-02, Notes + Highlights) — a relatively-positioned
          // wrapper so ColorMenu's popover can anchor below-left of the
          // trigger button (07-UI-SPEC.md: mirrors Python's own
          // `mapToGlobal(...bottomLeft())` anchor point) without a portal.
          if (state.op === "color" && !state.deferred) {
            return (
              <span key={state.op} style={{ position: "relative", display: "inline-block" }}>
                <button
                  ref={colorButtonRef}
                  type="button"
                  className="toolbar-button category-list-color-button"
                  onClick={() => setShowColorMenu(true)}
                  disabled={!state.enabled}
                  data-testid="category-list-color-button"
                >
                  {resolveOpLabel(state.op, category, t)}
                </button>
                {showColorMenu && (
                  <ColorMenu
                    category={category}
                    selectedIds={Array.from(selected)}
                    onApplied={(freshRows) => {
                      setShowColorMenu(false);
                      setSelected(new Set());
                      onRowsChanged?.(freshRows);
                    }}
                    onCancel={() => {
                      setShowColorMenu(false);
                      colorButtonRef.current?.focus();
                    }}
                    onError={(err) => onError?.(err)}
                  />
                )}
              </span>
            );
          }
          // "Tag" (EDIT-03, Notes only) — opens the modal TagDialog, which
          // owns its own fetch/dry-run/preview/apply flow internally.
          if (state.op === "tag" && !state.deferred) {
            return (
              <button
                key={state.op}
                type="button"
                className="toolbar-button category-list-tag-button"
                onClick={() => setShowTagDialog(true)}
                disabled={!state.enabled}
                data-testid="category-list-tag-button"
              >
                {resolveOpLabel(state.op, category, t)}
              </button>
            );
          }
          // "Edit" (EDIT-07, the `view` op, Notes + Annotations) — opens the
          // modal RecordEditor, which owns its own fetch/dry-run/preview/
          // apply flow internally. Stricter precondition than
          // `operationSet`'s generic `NEEDS_SELECTION` (>0): 07-UI-SPEC.md
          // requires EXACTLY one row selected, present-but-disabled with an
          // explicit tooltip for 2+ — reusing the same disabled+title
          // convention already used for deferred ops.
          if (state.op === "view" && !state.deferred) {
            const tooManySelected = selected.size > 1;
            return (
              <button
                key={state.op}
                type="button"
                className="toolbar-button category-list-edit-button"
                onClick={() => setShowRecordEditor(true)}
                disabled={!state.enabled || selected.size !== 1}
                title={tooManySelected ? t("categoryList.edit.tooManySelected") : undefined}
                data-testid="category-list-edit-button"
              >
                {resolveOpLabel(state.op, category, t)}
              </button>
            );
          }
          // Every other op is surfaced-but-deferred (no backend mutation yet).
          return (
            <button
              key={state.op}
              type="button"
              className="toolbar-button category-list-op-deferred"
              disabled
              data-deferred="true"
              data-testid={`category-list-op-${state.op}`}
              title={t("categoryList.op.comingSoonTitle")}
            >
              {t("categoryList.op.deferredLabel", { label: resolveOpLabel(state.op, category, t) })}
            </button>
          );
        })}
      </div>
      {rows.length === 0 ? (
        <p className="notes-list-empty" data-testid="category-list-empty">
          {t("categoryList.emptyState", { category: categoryLabel(category, t) })}
        </p>
      ) : (
      <div
        ref={parentRef}
        className="notes-list-viewport"
        data-testid="category-list-viewport"
      >
        <ul
          className="notes-list"
          role="list"
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {virtualRows.map((virtualRow) => {
            const row = rows[virtualRow.index];
            const label = resolveLabel(row, category);
            return (
              <li
                key={Number(row.id)}
                className="notes-list-row"
                data-testid="category-list-row"
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: `${ROW_HEIGHT}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                  ...NO_WRAP_STYLE,
                }}
              >
                <input
                  type="checkbox"
                  className="notes-list-row-checkbox"
                  data-testid="category-list-row-checkbox"
                  aria-label={t("categoryList.selectionCheckboxAriaLabel", { label })}
                  checked={selected.has(row.id)}
                  onChange={() => toggleSelected(row.id)}
                />
                <span
                  className="notes-list-row-label"
                  data-testid="category-list-row-label"
                  style={{ ...NO_WRAP_STYLE, flex: 1, minWidth: 0 }}
                >
                  {label}
                </span>
                {/* Color column only when the category produced one (Notes/
                    Highlights); absent columns render nothing and NEVER a
                    taller row. */}
                {row.color !== null && (
                  <span className="notes-list-row-color" style={NO_WRAP_STYLE}>
                    {row.color}
                  </span>
                )}
                {row.tags !== null && (
                  <span
                    className="notes-list-row-tags"
                    style={{ ...NO_WRAP_STYLE, flexShrink: 0, maxWidth: "30%" }}
                  >
                    {row.tags}
                  </span>
                )}
                {row.modified !== null && (
                  <span className="notes-list-row-modified">{row.modified}</span>
                )}
              </li>
            );
          })}
        </ul>
      </div>
      )}
      {report && (
        <EditPreviewDialog
          report={report}
          onConfirm={handleConfirm}
          onCancel={handleCancel}
        />
      )}
      {playlistDeleteReport && (
        <EditPreviewDialog
          report={playlistDeleteReport.report}
          onConfirm={handleConfirm}
          onCancel={handleCancel}
          title={t("categoryList.playlistDelete.title", {
            count: selected.size,
            plural: selected.size === 1 ? "" : "s",
          })}
          ariaLabel={t("common.confirmDelete")}
          summary={
            <>
              {t("categoryList.playlistDelete.summaryDeletes", {
                count: selected.size,
                plural: selected.size === 1 ? "" : "s",
              })}{" "}
              {t("categoryList.playlistDelete.summaryMediaRemoved", {
                count: playlistDeleteReport.media_removed,
                plural: playlistDeleteReport.media_removed === 1 ? "" : "s",
              })}
              {playlistDeleteReport.media_kept > 0 && (
                <>
                  {" "}
                  {t("categoryList.playlistDelete.summaryMediaKept", {
                    count: playlistDeleteReport.media_kept,
                    plural: playlistDeleteReport.media_kept === 1 ? "" : "s",
                    pronoun: playlistDeleteReport.media_kept === 1 ? "it's" : "they're",
                  })}
                </>
              )}
            </>
          }
        />
      )}
      {importPreview && (
        <EditPreviewDialog
          report={importPreview.report}
          onConfirm={handleImportConfirm}
          onCancel={handleImportCancel}
          title={
            category === "Playlists"
              ? t("categoryList.import.titlePlaylist")
              : t("categoryList.import.title", { category: categoryLabel(category, t) })
          }
          ariaLabel={
            category === "Playlists"
              ? t("categoryList.import.labelPlaylist")
              : t("categoryList.import.label", { category: categoryLabel(category, t) })
          }
          confirmLabel={
            category === "Playlists"
              ? t("categoryList.import.labelPlaylist")
              : t("categoryList.import.label", { category: categoryLabel(category, t) })
          }
          confirmPendingLabel={t("categoryList.import.pending")}
          summary={
            <>
              {/* Playlists-only leading clause (UI-SPEC): names the playlist
                  and its media-file count ahead of the standard
                  added/updated/skipped lines. */}
              {importPreview.playlist !== null && (
                <>
                  {t("categoryList.import.playlistLeadingClause", {
                    name: importPreview.playlist.name,
                    count: importPreview.playlist.mediaCount,
                    plural: importPreview.playlist.mediaCount === 1 ? "" : "s",
                  })}
                  <br />
                </>
              )}
              {/* Notes-only extra clause (D8-09): only when the file names a
                  title-character bucket to bulk-delete first. Rendered from
                  `report.deleted` — never silently applied; requires this
                  explicit opt-in checkbox before `handleImportConfirm` sends
                  the bucket through. */}
              {category === "Notes" && importPreview.bucket !== null && (
                <label
                  style={{ display: "block", marginBottom: "0.75rem" }}
                  className="notes-import-bucket-delete-optin"
                >
                  <input
                    type="checkbox"
                    checked={bucketDeleteOptIn}
                    onChange={(e) => setBucketDeleteOptIn(e.target.checked)}
                  />{" "}
                  {t("categoryList.import.bucketDeleteOptIn", {
                    count: sumCounts(importPreview.report.deleted),
                    plural: sumCounts(importPreview.report.deleted) === 1 ? "" : "s",
                    bucket: importPreview.bucket,
                  })}
                </label>
              )}
              {sumCounts(importPreview.report.added) === 0 &&
              sumCounts(importPreview.report.overwritten) === 0 &&
              sumCounts(importPreview.report.skipped) === 0 ? (
                <>{t("categoryList.import.nothingNew")}</>
              ) : (
                <>
                  {sumCounts(importPreview.report.added) > 0 && (
                  <>
                    {t("categoryList.import.added", {
                      count: sumCounts(importPreview.report.added),
                      plural: sumCounts(importPreview.report.added) === 1 ? "" : "s",
                    })}
                    <br />
                  </>
                )}
                {sumCounts(importPreview.report.overwritten) > 0 && (
                  <>
                    {t("categoryList.import.updated", {
                      count: sumCounts(importPreview.report.overwritten),
                      plural: sumCounts(importPreview.report.overwritten) === 1 ? "" : "s",
                    })}
                    <br />
                  </>
                )}
                  {sumCounts(importPreview.report.skipped) > 0 && (
                    <>
                      {t("categoryList.import.skipped", {
                        count: sumCounts(importPreview.report.skipped),
                        plural: sumCounts(importPreview.report.skipped) === 1 ? "" : "s",
                      })}
                    </>
                  )}
                </>
              )}
            </>
          }
        />
      )}
      {incrementalExportSummary && (
        <div
          className="edit-preview-overlay"
          role="presentation"
          onClick={(event) => {
            if (event.target === event.currentTarget) {
              setIncrementalExportSummary(null);
            }
          }}
        >
          <div
            className="edit-preview-dialog"
            role="dialog"
            aria-modal="true"
            aria-label={t("categoryList.incrementalExport.label", { category: categoryLabel(category, t) })}
            data-testid="incremental-export-summary"
          >
            <h2 className="edit-preview-title">
              {t("categoryList.incrementalExport.label", { category: categoryLabel(category, t) })}
            </h2>
            <p className="edit-preview-summary" data-testid="incremental-export-summary-counts">
              {t("categoryList.incrementalExport.summary", {
                added: incrementalExportSummary.added,
                addedPlural: incrementalExportSummary.added === 1 ? "" : "s",
                modified: incrementalExportSummary.modified,
                modifiedPlural: incrementalExportSummary.modified === 1 ? "" : "s",
              })}
              {incrementalExportSummary.deleted_candidates > 0 && (
                <>
                  {" "}
                  {t("categoryList.incrementalExport.deletedCandidates", {
                    count: incrementalExportSummary.deleted_candidates,
                    plural: incrementalExportSummary.deleted_candidates === 1 ? "" : "s",
                    isAre: incrementalExportSummary.deleted_candidates === 1 ? "is" : "are",
                  })}
                </>
              )}
            </p>
            <div className="edit-preview-actions">
              <button
                type="button"
                className="toolbar-button edit-preview-confirm"
                onClick={() => setIncrementalExportSummary(null)}
                data-testid="incremental-export-summary-dismiss"
              >
                {t("common.ok")}
              </button>
            </div>
          </div>
        </div>
      )}
      {showFavoriteDialog && (
        <FavoriteAddDialog
          onApplied={(freshRows) => {
            setShowFavoriteDialog(false);
            onRowsChanged?.(freshRows);
          }}
          onCancel={() => setShowFavoriteDialog(false)}
          onError={(err) => onError?.(err)}
        />
      )}
      {showMediaAddDialog && (
        <MediaAddDialog
          onApplied={() => {
            setShowMediaAddDialog(false);
            invoke<BrowseRow[]>("list_category", { category: "Playlists" })
              .then((freshRows) => onRowsChanged?.(freshRows))
              .catch((err) => onError?.(err as ErrorDto));
          }}
          onCancel={() => setShowMediaAddDialog(false)}
          onError={(err) => onError?.(err)}
        />
      )}
      {showTagDialog && (
        <TagDialog
          selectedIds={Array.from(selected)}
          onApplied={(freshRows) => {
            setShowTagDialog(false);
            setSelected(new Set());
            onRowsChanged?.(freshRows);
          }}
          onCancel={() => setShowTagDialog(false)}
          onError={(err) => onError?.(err)}
        />
      )}
      {showRecordEditor && singleSelectedRow && (
        <RecordEditor
          category={category}
          row={singleSelectedRow}
          onApplied={(freshRows) => {
            setShowRecordEditor(false);
            setSelected(new Set());
            onRowsChanged?.(freshRows);
          }}
          onCancel={() => setShowRecordEditor(false)}
          onError={(err) => onError?.(err)}
        />
      )}
    </div>
  );
}
