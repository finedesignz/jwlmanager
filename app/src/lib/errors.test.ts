import { describe, expect, it } from "vitest";
// Read as raw source text via Vite's `?raw` import (the SAME mechanism
// App.test.tsx/SettingsDialog.test.tsx already use to read component
// source at test time, 11-03-PLAN.md) -- not Node's `fs` module, since
// `@types/node` is not a dependency of this project and this plan adds
// zero new dependencies (D11-02). Vite's import-analysis resolves this
// relative path against the file on disk regardless of dev-server `fs`
// restrictions, which only gate HTTP-served requests, not static imports.
import errorRsSource from "../../src-tauri/src/error.rs?raw";
import settingsRsSource from "../../src-tauri/src/settings.rs?raw";
import { describeError } from "./errors";
import { en } from "../i18n/en";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { StringKey } from "../i18n/strings";

/** A real `t` built from the actual `en` catalog (11-04-PLAN.md task 3) --
 * NOT a mock that trivially echoes the key -- the exact algorithm
 * `I18nContext.tsx`'s own `t` uses, duplicated here (not imported) since
 * this is a pure lib test with no React tree to mount an `I18nProvider`
 * into. */
function realT(key: StringKey, params?: Record<string, string | number>): string {
  const template = en[key];
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, token: string) =>
    params[token] === undefined ? match : String(params[token]),
  );
}

/** Extracts the body of a named `fn` (brace-balance from its first `{`) so
 * the tuple regex below only ever scans the ACTUAL `to_dto` match arms --
 * never anything else in the file that happens to look like a 2-string
 * tuple. */
function extractFnBody(source: string, fnNameMarker: string): string {
  const startIdx = source.indexOf(fnNameMarker);
  if (startIdx === -1) {
    throw new Error(`could not find "${fnNameMarker}" in source`);
  }
  const braceStart = source.indexOf("{", startIdx);
  let depth = 0;
  let i = braceStart;
  for (; i < source.length; i++) {
    if (source[i] === "{") depth++;
    else if (source[i] === "}") {
      depth--;
      if (depth === 0) break;
    }
  }
  return source.slice(braceStart, i + 1);
}

/** Regex-extracts every `("snake_case_code", "dotted.message.key")` tuple
 * from a `to_dto` match-arm body -- the tuple is emitted either inline
 * (`ArchiveError::NotAZip => ("not_a_zip", "error.archive.not_a_zip"),`) or
 * split across lines inside a `{ ... }` arm body
 * (`ArchiveError::FavoriteFailed { .. } => {\n  ("favorite_failed", "...")\n}`)
 * -- this pattern matches both shapes since it only anchors on the
 * parenthesized pair itself, not the surrounding match-arm syntax. */
function extractCodeMessageKeyTuples(fnBody: string): Array<{ code: string; messageKey: string }> {
  // The trailing `,?` handles the multi-line arm shape (`(\n  "code",\n
  // "key",\n)`), where the second string is followed by a trailing comma
  // before the closing paren -- present for every tuple whose Rust source
  // formatter wrapped the arm across lines (`MissingUserDataBackup`,
  // `SchemaUpgradeFailed`, `SchemaDowngradeFailed`,
  // `MissingResourcesLanguage`), absent for the single-line ones.
  const pattern = /\(\s*"([a-z_]+)"\s*,\s*"([a-z_.]+)"\s*,?\s*\)/g;
  const tuples: Array<{ code: string; messageKey: string }> = [];
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(fnBody)) !== null) {
    tuples.push({ code: match[1], messageKey: match[2] });
  }
  return tuples;
}

function loadRealErrorCodes(): Array<{ code: string; messageKey: string }> {
  const errorRsBody = extractFnBody(errorRsSource, "pub fn to_dto(&self, operation: &str, file: Option<&Path>)");
  const settingsRsBody = extractFnBody(settingsRsSource, "pub fn to_dto(&self, operation: &str)");
  return [...extractCodeMessageKeyTuples(errorRsBody), ...extractCodeMessageKeyTuples(settingsRsBody)];
}

function makeError(overrides: Partial<ErrorDto> = {}): ErrorDto {
  return {
    code: "not_a_zip",
    operation: "open_archive",
    safe_file_name: null,
    message_key: "error.archive.not_a_zip",
    ...overrides,
  };
}

describe("describeError full coverage (11-04-PLAN.md task 3)", () => {
  const realCodes = loadRealErrorCodes();

  it("derives at least the known 39 codes from src-tauri/src/error.rs + settings.rs at test time (sanity floor, not a hand-typed duplicate list)", () => {
    // A floor, not an exact count -- the whole POINT of deriving this list
    // from the Rust source is that it grows/shrinks with the source without
    // this test needing an edit. A floor still catches the regex silently
    // matching nothing (extraction broke) or matching far too little.
    expect(realCodes.length).toBeGreaterThanOrEqual(39);
    const codes = realCodes.map((c) => c.code);
    expect(new Set(codes).size).toBe(codes.length); // every code appears exactly once
  });

  it("every code the Rust source actually emits resolves, through describeError, to a distinct, non-empty, catalog-sourced string that is NOT the generic default fallback", () => {
    const operation = "open_archive";
    const genericFallback = realT("errors.default", { operation });
    const seen = new Map<string, string>();
    for (const { code } of realCodes) {
      const sentence = describeError(makeError({ code, operation }), realT);
      expect(sentence.length, `code "${code}" resolved to an empty string`).toBeGreaterThan(0);
      expect(sentence, `code "${code}" resolved to the bare code itself`).not.toBe(code);
      // The load-bearing assertion: every REAL Rust-emitted code has its
      // OWN describeError branch, per this test's premise -- none of them
      // are meant to fall through to the generic `default` case. A future
      // `to_dto` match arm added without a matching describeError branch
      // WOULD fall through to `default` and produce exactly this generic
      // text, failing this assertion for that code (task 3's coverage
      // contract).
      expect(sentence, `code "${code}" fell through to the generic default fallback -- missing branch`).not.toBe(
        genericFallback,
      );
      // Distinct per code -- catches a code accidentally sharing another
      // code's catalog key (copy-paste error), not just a missing branch.
      if (seen.has(sentence)) {
        throw new Error(
          `code "${code}" resolves to the SAME sentence as code "${seen.get(sentence)}" -- ` +
            `each code must have its own describeError branch/catalog key`,
        );
      }
      seen.set(sentence, code);
    }
  });

  it("the missing-branch guard genuinely fires: an unmapped code DOES fall through to the generic default fallback (red), while every real code does not (green)", () => {
    const operation = "open_archive";
    const genericFallback = realT("errors.default", { operation });

    // RED: a code with no describeError branch (not in the Rust source,
    // hence not in `realCodes` either) falls through to `default` and
    // equals the generic fallback -- this is exactly the failure state the
    // assertion above would catch for a REAL code missing its branch.
    const unmappedSentence = describeError(makeError({ code: "a_future_code_not_yet_handled", operation }), realT);
    expect(unmappedSentence).toBe(genericFallback);

    // GREEN: every code the Rust source actually emits (e.g. the two
    // branches this plan newly added) resolves to its OWN specific text,
    // never the generic fallback -- confirming the guard discriminates.
    expect(describeError(makeError({ code: "trim_failed", operation }), realT)).not.toBe(genericFallback);
    expect(describeError(makeError({ code: "record_edit_failed", operation }), realT)).not.toBe(genericFallback);
  });

  it("the default branch's {operation} substitution works", () => {
    const sentence = describeError(makeError({ code: "totally_unknown_code", operation: "save_archive" }), realT);
    expect(sentence).toContain("save_archive");
    expect(sentence).not.toContain("{operation}");
  });
});
