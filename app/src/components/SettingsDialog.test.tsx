import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SettingsDialog from "./SettingsDialog";
import { SettingsProvider } from "../settings/SettingsProvider";
// Vite's built-in `*?raw` import (vite/client.d.ts, already covers this
// suffix generically -- no new ambient declaration needed, see
// app/src/vite-env.d.ts's narrower `*.css?raw` override from 11-01) reads
// this file's OWN source at test time for the structural completeness scan
// below (11-03-PLAN.md Task 2).
import settingsDialogSource from "./SettingsDialog.tsx?raw";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "load_settings") return Promise.resolve({ language: "en", theme: "dark" });
    if (cmd === "save_settings") return Promise.resolve(undefined);
    if (cmd === "app_version") return Promise.resolve("0.9.3");
    return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
  });
});

function renderDialog() {
  return render(
    <SettingsProvider>
      <SettingsDialog onClose={vi.fn()} />
    </SettingsProvider>,
  );
}

describe("SettingsDialog (D11-03/D11-04, 11-01-PLAN.md Task 1)", () => {
  it("renders a theme control whose activation calls the theme setter (write-through to save_settings)", async () => {
    renderDialog();
    await screen.findByTestId("settings-dialog");

    fireEvent.click(screen.getByTestId("settings-dialog-theme-light"));

    expect(invokeMock).toHaveBeenCalledWith("save_settings", {
      settings: { language: "en", theme: "light" },
    });
    expect(screen.getByTestId("settings-dialog-theme-light")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("settings-dialog-theme-dark")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("renders a version string sourced from the runtime app_version command, never a literal", async () => {
    renderDialog();

    expect(await screen.findByTestId("settings-dialog-version")).toHaveTextContent("0.9.3");
    expect(invokeMock).toHaveBeenCalledWith("app_version");
  });

  it("SettingsDialog language switch: selecting a different locale calls setLocale (write-through to save_settings via 11-01's existing path) and the dialog's own text re-renders through the catalog on the same interaction", async () => {
    renderDialog();
    await screen.findByTestId("settings-dialog");

    fireEvent.change(screen.getByTestId("settings-dialog-language-select"), {
      target: { value: "de" },
    });

    expect(invokeMock).toHaveBeenCalledWith("save_settings", {
      settings: { language: "de", theme: "dark" },
    });
    expect(screen.getByTestId("settings-dialog-language-select")).toHaveValue("de");
    // "de" is an empty scaffolded catalog -- the Theme label still renders
    // (via the English fallback), proving the re-render didn't blank out.
    expect(screen.getByText("Theme")).toBeInTheDocument();
  });
});

/**
 * Structural completeness (D11-02, 11-03-PLAN.md Task 2): a plain
 * regex/brace-balance source scan of SettingsDialog.tsx's OWN JSX return
 * block -- NOT a behavioural render check, so it cannot be satisfied by
 * coincidence. Mirrors the technique app/src/theme/styles_tokens.test.ts
 * (11-01-PLAN.md Task 3) established for reading a source file at test time
 * via a `?raw` import and scanning it structurally.
 *
 * Scope is deliberately narrowed to the `return (...)` JSX block (not the
 * whole file) so TypeScript generic syntax elsewhere in the file (e.g.
 * `useState<string | null>`) can never be misread as a stray `>text<` JSX
 * text node by this line-oriented scan.
 */
describe("SettingsDialog structural completeness (D11-02, 11-03-PLAN.md Task 2)", () => {
  // Empty allowlist, deliberately: even the product name ("JWL Manager") is
  // referenced via the APP_NAME variable inside {}, not written as a
  // literal JSX text node, so nothing needs listing here today. Kept as an
  // explicit array (rather than inlining `[]` at the call site) so a later
  // reader who adds a real exception has an obvious place to add it.
  const ALLOWED_TEXT: string[] = [];
  const ALLOWED_ATTRS: string[] = [];

  it("contains zero user-facing string literals outside t() calls, except the allowlisted product-name/code-tag exceptions", () => {
    const found = findDisallowedLiterals(settingsDialogSource, ALLOWED_TEXT, ALLOWED_ATTRS);
    expect(found).toEqual([]);
  });
});

/**
 * Extracts the `return ( ... )` JSX block via paren-balance counting (same
 * technique as styles_tokens.test.ts's brace-balance `extractBlock`), then
 * scans it for (a) non-empty JSX text nodes and (b) `aria-label=`/`title=`/
 * `placeholder=` string-literal attributes -- after first stripping every
 * `{...}` JS/JSX expression slot (also via brace-balance counting) to
 * spaces, so `{t("...")}` calls, arrow-function handlers (`=>` contains a
 * bare `>`), and `{APP_NAME}` variable references never register as a
 * literal.
 */
function findDisallowedLiterals(
  source: string,
  allowedText: string[],
  allowedAttrs: string[],
): string[] {
  const returnIndex = source.indexOf("return (");
  if (returnIndex === -1) {
    throw new Error("could not find a `return (` JSX block in source");
  }
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
  const jsx = source.slice(openParenIndex + 1, endIndex);

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

  const found: string[] = [];

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

  return found;
}
