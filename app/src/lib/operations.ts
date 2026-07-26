import type { Category } from "../bindings/Category";

/**
 * Operation-capability descriptor (D6-08) — the frontend port of the Python
 * `disable_options`/`tree_selection` tables (`JWLManager.py:510-547`, `:497-505`;
 * FUNCTIONALITY-SPEC §1.2). It answers ONE question the UI asks constantly:
 * given a category and how many rows are selected, which operations should the
 * contextual operation bar surface, and which of those are actually actionable
 * right now?
 *
 * Phase 6 scope was the descriptor + the selection-gated presentation, with
 * exactly one operation wired to a real backend mutation — Notes delete (the
 * shipped `delete_notes_dry_run`/`delete_notes_apply`). Phase 7
 * progressively flips more `(category, op)` pairs into [`LIVE`] as each
 * edit-op backend lands (07-01-PLAN.md starts with `Favorites:delete`).
 * Every pair not yet in [`LIVE`] renders surfaced-but-deferred.
 */

/** Every operation any category can, in principle, offer. */
export type Op = "delete" | "export" | "view" | "color" | "tag" | "add" | "import";

/**
 * Per-category capability set — which operations that category exposes at all
 * (ported verbatim from the Python per-category option tables). Order here is
 * the render order of the operation bar.
 */
const CAPABILITY: Record<Category, Op[]> = {
  Notes: ["delete", "export", "import", "view", "color", "tag"],
  Highlights: ["delete", "export", "import", "color"],
  Bookmarks: ["delete", "export", "import"],
  Annotations: ["delete", "export", "import", "view"],
  Favorites: ["delete", "export", "import", "add"],
  Playlists: ["delete", "export", "import", "add"],
};

/**
 * Operations that require at least one selected row to be actionable
 * (delete/export/view/color/tag act ON the selection). `add`/`import` are
 * selection-independent (they act on the whole category), so they are never
 * gated on selection size.
 */
const NEEDS_SELECTION: ReadonlySet<Op> = new Set<Op>([
  "delete",
  "export",
  "view",
  "color",
  "tag",
]);

/**
 * The (category, op) pairs wired to a real backend mutation. Keyed as
 * `${Category}:${Op}`. Everything not in this set renders deferred.
 * `Notes:delete` shipped in Phase 2; `Favorites:delete` (EDIT-05 unmark)
 * and `Favorites:add` (EDIT-05 mark) land in 07-01-PLAN.md. `Notes:color` /
 * `Highlights:color` (EDIT-02 recolor) and `Highlights:delete` (D7-10,
 * BlockRange-only, rule #9) land in 07-02-PLAN.md. `Notes:tag` (EDIT-03)
 * lands in 07-03-PLAN.md. `Playlists:add` (media add) stays deferred —
 * Phase 8 (D7-06/D7-10).
 *
 * Sort Tags (EDIT-04) is ARCHIVE-WIDE, never selection-scoped, so it
 * deliberately does NOT enter this descriptor at all — it lives entirely
 * on the "Utilities ▾" `CommandBar` button / `UtilitiesMenu`
 * (07-03-PLAN.md Task 3). Do not "fix" this by adding a `Notes:sort`-style
 * entry here; there is no selection for `NEEDS_SELECTION`/`CAPABILITY` to
 * gate it on.
 */
const LIVE: ReadonlySet<string> = new Set<string>([
  "Notes:delete",
  "Notes:color",
  "Notes:tag",
  "Favorites:delete",
  "Favorites:add",
  "Highlights:color",
  "Highlights:delete",
]);

/** A single entry in the contextual operation bar. */
export interface OperationState {
  op: Op;
  /** Actionable right now: the op is live AND its selection precondition (if
   * any) is met. A deferred op is never enabled. */
  enabled: boolean;
  /** True when this (category, op) pair has no backend mutation yet — the
   * affordance is surfaced but visibly parked ("coming soon"). */
  deferred: boolean;
}

/**
 * Compute the contextual operation set for `(category, selectionSize)`.
 *
 * For each op the category exposes: it is `enabled` only when it is live for
 * this category AND its selection precondition is satisfied; it is `deferred`
 * exactly when the `(category, op)` pair is not in the LIVE set. In Phase 6
 * that means `Notes:delete` is the sole op that can ever be enabled — and only
 * once at least one row is selected.
 */
export function operationSet(cat: Category, selectionSize: number): OperationState[] {
  return CAPABILITY[cat].map((op) => {
    const live = LIVE.has(`${cat}:${op}`);
    const selectionOk = !NEEDS_SELECTION.has(op) || selectionSize > 0;
    return {
      op,
      enabled: selectionOk && live,
      deferred: !live,
    };
  });
}
