import type { Category } from "../bindings/Category";
import type { StringKey } from "../i18n/strings";
import { useI18n } from "../i18n/I18nContext";

/**
 * The six browse categories, single-sourced as a typed `Category[]` so the
 * switcher is driven off the enum union itself — never translated display
 * strings (D6-06). Control flow (which list to fetch, which selection to
 * reset) keys off these exact enum values.
 */
const CATEGORIES: Category[] = [
  "Notes",
  "Bookmarks",
  "Favorites",
  "Highlights",
  "Annotations",
  "Playlists",
];

/** Maps every `Category` enum value to its `category.*` catalog key
 * (11-04-PLAN.md). Exported so `CategoryList.tsx` reuses the SAME mapping
 * for its own category-name-bearing sentences ("No {category} in this
 * archive.", the import/export dialog titles) rather than duplicating it. */
const CATEGORY_LABEL_KEYS: Record<Category, StringKey> = {
  Notes: "category.Notes",
  Bookmarks: "category.Bookmarks",
  Favorites: "category.Favorites",
  Highlights: "category.Highlights",
  Annotations: "category.Annotations",
  Playlists: "category.Playlists",
};

/** Translated DISPLAY label for a `Category` enum value (D6-06/DATA-08) —
 * presentation only. Every caller MUST keep using the raw `category` value
 * itself for control flow, `onSelect` payloads, IPC arguments, and
 * `data-testid`s — never this return value. */
export function categoryLabel(category: Category, t: ReturnType<typeof useI18n>["t"]): string {
  return t(CATEGORY_LABEL_KEYS[category]);
}

interface CategorySwitcherProps {
  /** The currently-shown category (marked as pressed/current). */
  active: Category;
  /** Fired with the chosen `Category` when a non-active option is clicked. */
  onSelect: (category: Category) => void;
  /** Disables the whole control (e.g. while an archive load is in flight). */
  disabled?: boolean;
}

/**
 * Enum-driven segmented control over the six `Category` variants (D6-06).
 * Reuses the existing `toolbar-button` class + dark tokens — no new design
 * system. Each option carries a stable `category-switcher-option-<Category>`
 * testid, marks the active variant via `aria-pressed`, and emits the enum
 * value (not a label) through `onSelect`. Clicking the already-active option
 * is a no-op.
 *
 * The RENDERED label is the only thing 11-04-PLAN.md's retrofit touches —
 * `category`, `data-testid`, and `onSelect`'s payload are byte-identical to
 * before (D6-06/DATA-08's structural isolation guard).
 */
export default function CategorySwitcher({
  active,
  onSelect,
  disabled = false,
}: CategorySwitcherProps) {
  const { t } = useI18n();
  return (
    <div
      className="category-switcher"
      role="group"
      aria-label={t("category.groupAriaLabel")}
      data-testid="category-switcher"
    >
      {CATEGORIES.map((category) => {
        const isActive = category === active;
        return (
          <button
            key={category}
            type="button"
            className="toolbar-button category-switcher-option"
            data-testid={`category-switcher-option-${category}`}
            aria-pressed={isActive}
            disabled={disabled}
            onClick={() => {
              if (!isActive) {
                onSelect(category);
              }
            }}
          >
            {categoryLabel(category, t)}
          </button>
        );
      })}
    </div>
  );
}
