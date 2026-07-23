import type { Category } from "../bindings/Category";

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
 */
export default function CategorySwitcher({
  active,
  onSelect,
  disabled = false,
}: CategorySwitcherProps) {
  return (
    <div
      className="category-switcher"
      role="group"
      aria-label="Category"
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
            {category}
          </button>
        );
      })}
    </div>
  );
}
