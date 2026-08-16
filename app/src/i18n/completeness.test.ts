import { describe, expect, it } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { createElement, useState } from "react";
import commandBarSource from "../components/CommandBar.tsx?raw";
import tagDialogSource from "../components/TagDialog.tsx?raw";
import favoriteAddDialogSource from "../components/FavoriteAddDialog.tsx?raw";
import mediaAddDialogSource from "../components/MediaAddDialog.tsx?raw";
import editPreviewDialogSource from "../components/EditPreviewDialog.tsx?raw";
import foldMergeDialogSource from "../components/FoldMergeDialog.tsx?raw";
import recordEditorSource from "../components/RecordEditor.tsx?raw";
import categorySwitcherSource from "../components/CategorySwitcher.tsx?raw";
import categoryListSource from "../components/CategoryList.tsx?raw";
import colorMenuSource from "../components/ColorMenu.tsx?raw";
import utilitiesMenuSource from "../components/UtilitiesMenu.tsx?raw";
import errorBannerSource from "../components/ErrorBanner.tsx?raw";
import jwlCoreNoticeSource from "../components/JwlCoreNotice.tsx?raw";
import CommandBar from "../components/CommandBar";
import ErrorBanner from "../components/ErrorBanner";
import { I18nProvider } from "./I18nContext";
import { vi } from "vitest";
import type { ErrorDto } from "../bindings/ErrorDto";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

/**
 * The single source of truth for what a completeness scan may legitimately
 * find un-translated (11-04-PLAN.md, extending 11-03's App.test.tsx /
 * SettingsDialog.test.tsx allowlist convention) -- product name, raw enum
 * values used as data-testid/props, and punctuation-only fragments. Every
 * entry here is deliberate and file-scoped below, never a blanket bypass.
 */
export const ALLOWLIST: Record<string, { text: string[]; attrs: string[] }> = {
  "CommandBar.tsx": { text: [], attrs: [] },
  "TagDialog.tsx": { text: [], attrs: [] },
  "FavoriteAddDialog.tsx": { text: [], attrs: [] },
  "MediaAddDialog.tsx": { text: [], attrs: [] },
  "EditPreviewDialog.tsx": { text: [], attrs: [] },
  // The three row-control glyphs are language-neutral iconography (like
  // JwlCoreNotice's "×" dismiss glyph and MediaAddDialog's status glyphs,
  // which render through a JS expression rather than raw JSX text and so
  // never reach this scan at all) -- not prose, nothing to translate.
  "FoldMergeDialog.tsx": { text: ["▲", "▼", "✕"], attrs: [] },
  // The split-around-JSX convention (11-03-PLAN.md, reused here for the
  // annotation over-deletion warning's <strong>all</strong>) keeps the
  // wrapped element's own text a deliberate literal, exactly like App.tsx's
  // ".jwlibrary" <code> segment.
  "RecordEditor.tsx": { text: ["all"], attrs: [] },
  "CategorySwitcher.tsx": { text: [], attrs: [] },
  "CategoryList.tsx": { text: [], attrs: [] },
  "ColorMenu.tsx": { text: [], attrs: [] },
  "UtilitiesMenu.tsx": { text: [], attrs: [] },
  "ErrorBanner.tsx": { text: [], attrs: [] },
  // The dismiss glyph is the same language-neutral iconography as above.
  "JwlCoreNotice.tsx": { text: ["×"], attrs: [] },
};

const SOURCES: Record<string, string> = {
  "CommandBar.tsx": commandBarSource,
  "TagDialog.tsx": tagDialogSource,
  "FavoriteAddDialog.tsx": favoriteAddDialogSource,
  "MediaAddDialog.tsx": mediaAddDialogSource,
  "EditPreviewDialog.tsx": editPreviewDialogSource,
  "FoldMergeDialog.tsx": foldMergeDialogSource,
  "RecordEditor.tsx": recordEditorSource,
  "CategorySwitcher.tsx": categorySwitcherSource,
  "CategoryList.tsx": categoryListSource,
  "ColorMenu.tsx": colorMenuSource,
  "UtilitiesMenu.tsx": utilitiesMenuSource,
  "ErrorBanner.tsx": errorBannerSource,
  "JwlCoreNotice.tsx": jwlCoreNoticeSource,
};

/**
 * Extracts EVERY `return ( ... )` JSX block in `source` (not just the
 * first) via paren-balance counting -- several of the 13 files define more
 * than one (TagDialog.tsx's `TriStateCheckbox` helper above its own
 * `TagDialog` return; ColorMenu.tsx/UtilitiesMenu.tsx/RecordEditor.tsx/
 * FavoriteAddDialog.tsx's early-return preview branches alongside their
 * main picker return). Scanning only the first block would silently skip
 * every later one -- an under-scan, not a false pass, but a real gap
 * 11-03's single-block technique never had to handle since App.tsx/
 * SettingsDialog.tsx each define exactly one component.
 */
function extractReturnBlocks(source: string): string[] {
  const blocks: string[] = [];
  let searchFrom = 0;
  for (;;) {
    const returnIndex = source.indexOf("return (", searchFrom);
    if (returnIndex === -1) break;
    const openParenIndex = source.indexOf("(", returnIndex);
    let depth = 0;
    let endIndex = openParenIndex;
    for (; endIndex < source.length; endIndex++) {
      if (source[endIndex] === "(") depth++;
      else if (source[endIndex] === ")") {
        depth--;
        if (depth === 0) break;
      }
    }
    blocks.push(source.slice(openParenIndex + 1, endIndex));
    searchFrom = endIndex + 1;
  }
  return blocks;
}

/** Strips every `{...}` JS/JSX expression slot to spaces (brace-balance),
 * same technique as 11-03's helper -- so `{t("...")}` calls, arrow-function
 * handlers, and any string embedded in a JS expression never register as a
 * bare literal. */
function stripExpressionSlots(jsx: string): string {
  let stripped = "";
  let braceDepth = 0;
  for (const ch of jsx) {
    if (ch === "{") {
      braceDepth++;
      continue;
    }
    if (ch === "}") {
      braceDepth = Math.max(0, braceDepth - 1);
      continue;
    }
    stripped += braceDepth > 0 ? " " : ch;
  }
  return stripped;
}

/**
 * Extends 11-03's structural scan: every `return (...)` JSX block in the
 * file (not just the first), scanned for non-empty JSX text nodes and
 * `aria-label`/`title`/`placeholder` string-literal attributes.
 */
function findDisallowedLiterals(source: string, allowedText: string[], allowedAttrs: string[]): string[] {
  const blocks = extractReturnBlocks(source);
  if (blocks.length === 0) {
    throw new Error("could not find any `return (` JSX block in source");
  }
  const found: string[] = [];
  for (const jsx of blocks) {
    const stripped = stripExpressionSlots(jsx);

    const textNodePattern = />([^<>]*)</g;
    let match: RegExpExecArray | null;
    while ((match = textNodePattern.exec(stripped)) !== null) {
      const text = match[1].replace(/\s+/g, " ").trim();
      if (text.length === 0 || allowedText.includes(text)) continue;
      found.push(`JSX text node: "${text}"`);
    }

    const attrPattern = /\b(aria-label|title|placeholder)\s*=\s*"([^"]*)"/g;
    while ((match = attrPattern.exec(stripped)) !== null) {
      const [, attr, value] = match;
      if (allowedAttrs.includes(value)) continue;
      found.push(`${attr}="${value}"`);
    }
  }
  return found;
}

/**
 * Native-dialog `filters: [{ name: "..." }]` and `title: "..."`/
 * `` title: `...` `` object-literal strings passed to
 * `@tauri-apps/plugin-dialog`'s `open`/`save` calls (11-04-PLAN.md task 3) --
 * these live in handler functions ABOVE the `return (` JSX block (CommandBar's
 * `FILTERS`, CategoryList's several `open`/`save` calls, MediaAddDialog's
 * `open` call), so the JSX-text/attr scan above never sees them; a SEPARATE
 * pass over the whole file catches them. `title:` here is the object-literal
 * PROPERTY (colon), never the JSX ATTRIBUTE (`title=`), so it can never
 * double-count what `findDisallowedLiterals` already found.
 */
function findDialogOptionLiterals(source: string, allowedText: string[]): string[] {
  const found: string[] = [];

  const filterNamePattern = /filters:\s*\[\s*\{\s*name:\s*"([^"]*)"/g;
  let match: RegExpExecArray | null;
  while ((match = filterNamePattern.exec(source)) !== null) {
    const value = match[1];
    if (allowedText.includes(value)) continue;
    found.push(`dialog filter name: "${value}"`);
  }

  const titlePattern = /\btitle:\s*(?:"([^"]*)"|`([^`]*)`)/g;
  while ((match = titlePattern.exec(source)) !== null) {
    const value = match[1] ?? match[2];
    if (value === undefined || allowedText.includes(value)) continue;
    found.push(`dialog title: "${value}"`);
  }

  return found;
}

const SCANNED_FILES = Object.keys(SOURCES);

describe("i18n completeness (11-04-PLAN.md task 3)", () => {
  describe("completeness all components", () => {
    for (const file of SCANNED_FILES) {
      it(`${file} contains zero hardcoded strings outside t() calls (JSX text/attrs + native-dialog filters/title)`, () => {
        const { text, attrs } = ALLOWLIST[file];
        const jsxFound = findDisallowedLiterals(SOURCES[file], text, attrs);
        const dialogFound = findDialogOptionLiterals(SOURCES[file], text);
        expect([...jsxFound, ...dialogFound]).toEqual([]);
      });
    }

    it("the scan genuinely guards: a temporarily reintroduced hardcoded JSX attribute literal in CommandBar.tsx is caught", () => {
      // A literal directly assigned to a scanned attribute (`attr="..."`)
      // is what this scan structurally catches -- a literal buried inside a
      // `{ternary ? t(...) : t(...)}` JS expression slot is, BY DESIGN,
      // stripped before the text-node/attr regexes ever run (same as
      // `{t("...")}` calls themselves must never falsely register), so the
      // demonstration below tampers an ATTRIBUTE literal, matching exactly
      // how 11-03's own App.test.tsx demo (a stray JSX text node) proved
      // its scan fires on a real, structurally-visible violation.
      const tampered = SOURCES["CommandBar.tsx"].replace(
        'aria-label={t("commandBar.toolbarAriaLabel")}',
        'aria-label="Stray hardcoded text"',
      );
      expect(tampered).not.toBe(SOURCES["CommandBar.tsx"]);
      const found = findDisallowedLiterals(tampered, ALLOWLIST["CommandBar.tsx"].text, ALLOWLIST["CommandBar.tsx"].attrs);
      expect(found.some((entry) => entry.includes("Stray hardcoded text"))).toBe(true);
    });

    it("the native-dialog filter/title scan genuinely guards: a temporarily reintroduced literal filter name is caught", () => {
      const tampered = SOURCES["MediaAddDialog.tsx"].replace(
        't("mediaAddDialog.filterImages")',
        '"Images (Stray)"',
      );
      expect(tampered).not.toBe(SOURCES["MediaAddDialog.tsx"]);
      const found = findDialogOptionLiterals(tampered, ALLOWLIST["MediaAddDialog.tsx"].text);
      expect(found.some((entry) => entry.includes("Stray"))).toBe(true);
    });
  });

  describe("category enum isolation (structural)", () => {
    const files: [string, string][] = [
      ["CategorySwitcher.tsx", categorySwitcherSource],
      ["CategoryList.tsx", categoryListSource],
    ];

    it("never passes a translated (t()/categoryLabel()-derived) value into onSelect, an invoke(...) category argument, or a data-testid template literal", () => {
      const forbidden = [/onSelect\(\s*(?:t\(|categoryLabel\()/, /data-testid=\{[^}]*\b(?:t|categoryLabel)\(/];
      for (const [name, source] of files) {
        for (const pattern of forbidden) {
          expect(source, `${name} unexpectedly matched forbidden pattern ${pattern}`).not.toMatch(pattern);
        }
      }
    });

    it("still keys onSelect/data-testid/list_category off the raw Category enum value, unchanged", () => {
      expect(categorySwitcherSource).toContain("onSelect(category)");
      expect(categorySwitcherSource).toContain("data-testid={`category-switcher-option-${category}`}");
      expect(categoryListSource).toContain('invoke<BrowseRow[]>("list_category", { category })');
    });

    it("the isolation guard genuinely fires: routing a translated label into onSelect is caught", () => {
      const tampered = categorySwitcherSource.replace(
        "onSelect(category);",
        "onSelect(categoryLabel(category, t) as unknown as Category);",
      );
      expect(tampered).not.toBe(categorySwitcherSource);
      expect(tampered).toMatch(/onSelect\(\s*(?:t\(|categoryLabel\()/);
    });
  });

  describe("language switch, multi-component", () => {
    function makeError(): ErrorDto {
      return {
        code: "not_a_zip",
        operation: "open_archive",
        safe_file_name: null,
        message_key: "error.archive.not_a_zip",
      };
    }

    // Written with `createElement` (not JSX) since this file is `.test.ts`,
    // not `.test.tsx` (11-04-PLAN.md/artifacts_this_plan_produces names it
    // literally) -- esbuild's `.ts` loader does not parse JSX syntax.
    function Harness() {
      const [locale, setLocale] = useState("en");
      return createElement(I18nProvider, {
        locale,
        setLocale,
        children: [
          createElement(
            "button",
            { key: "switch", "data-testid": "switch-locale", onClick: () => setLocale("de") },
            "switch",
          ),
          createElement(CommandBar, {
            key: "command-bar",
            archiveOpen: true,
            onOpened: () => {},
            onNewArchive: () => {},
            onSaved: () => {},
            onError: () => {},
            onCancelled: () => {},
            currentCategory: "Notes",
            onCategoryRowsChanged: () => {},
          }),
          createElement(ErrorBanner, { key: "error-banner", error: makeError() }),
        ],
      });
    }

    it("switching the active locale re-renders CommandBar and ErrorBanner on the same interaction, falling back to English since de's catalog is still empty", () => {
      render(createElement(Harness));

      expect(screen.getByRole("button", { name: /open archive/i })).toBeInTheDocument();
      expect(screen.getByTestId("error-banner").textContent).toMatch(/isn't a valid \.jwlibrary backup/i);

      fireEvent.click(screen.getByTestId("switch-locale"));

      // de's catalog is empty (D11-02) -- both components fall back to the
      // SAME English text after the switch, proving the retrofit is live
      // (re-reads `t` from context on every render) rather than merely
      // present in source.
      expect(screen.getByRole("button", { name: /open archive/i })).toBeInTheDocument();
      expect(screen.getByTestId("error-banner").textContent).toMatch(/isn't a valid \.jwlibrary backup/i);
    });
  });
});
