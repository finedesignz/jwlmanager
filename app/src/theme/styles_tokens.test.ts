import { describe, expect, it } from "vitest";
// Vite's built-in `?raw` import (see src/vite-env.d.ts) -- this project has
// no `@types/node`, so styles.css is read via Vite's own asset pipeline
// rather than `node:fs`, keeping the zero-new-dependency line.
import css from "../styles.css?raw";

/**
 * Token-parity proof (D11-03, 11-01-PLAN.md Task 3): parses `styles.css`
 * from disk and asserts the dark `:root` block and the light
 * `:root[data-theme="light"]` block declare the IDENTICAL set of colour
 * custom-property names. A token added to one theme and not the other must
 * fail this test.
 *
 * Only the 8 named colour tokens are compared -- the spacing tokens
 * (`--space-*`) are theme-independent and declared ONCE, in the dark block,
 * by design. Do NOT "fix" this by duplicating spacing tokens into the light
 * block; that would defeat the point of this exclusion.
 */

const COLOR_TOKEN_NAMES = [
  "--bg-primary",
  "--bg-secondary",
  "--bg-tertiary",
  "--brand-primary",
  "--destructive",
  "--text-primary",
  "--text-muted",
  "--border-hairline",
];

function extractBlock(css: string, openingSelector: RegExp): string {
  const match = openingSelector.exec(css);
  if (!match) {
    throw new Error(`styles.css: could not find a block matching ${openingSelector}`);
  }
  const openBraceIndex = css.indexOf("{", match.index);
  const closeBraceIndex = css.indexOf("}", openBraceIndex);
  if (openBraceIndex === -1 || closeBraceIndex === -1) {
    throw new Error(`styles.css: malformed block for ${openingSelector}`);
  }
  return css.slice(openBraceIndex + 1, closeBraceIndex);
}

function extractColorTokenNames(block: string): Set<string> {
  const declared = new Set<string>();
  const propertyPattern = /(--[a-z0-9-]+)\s*:/g;
  let match: RegExpExecArray | null;
  while ((match = propertyPattern.exec(block)) !== null) {
    if (COLOR_TOKEN_NAMES.includes(match[1])) {
      declared.add(match[1]);
    }
  }
  return declared;
}

describe("styles_tokens — dark/light colour token parity (D11-03)", () => {
  it("the dark :root block declares all 8 named colour tokens", () => {
    const darkBlock = extractBlock(css, /:root\s*\{/);
    const darkTokens = extractColorTokenNames(darkBlock);
    expect([...darkTokens].sort()).toEqual([...COLOR_TOKEN_NAMES].sort());
  });

  it("the light [data-theme='light'] block declares the SAME 8 colour tokens as the dark block -- no drift", () => {
    const darkBlock = extractBlock(css, /:root\s*\{/);
    const lightBlock = extractBlock(css, /:root\[data-theme=["']light["']\]\s*\{/);

    const darkTokens = extractColorTokenNames(darkBlock);
    const lightTokens = extractColorTokenNames(lightBlock);

    expect(lightTokens).toEqual(darkTokens);
  });
});
