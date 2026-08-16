import { describe, expect, it } from "vitest";
import { en } from "./en";
import { de } from "./de";
import { es } from "./es";
import { fr } from "./fr";
import { it as itLocale } from "./it";
import { pl } from "./pl";
import { pt } from "./pt";
import { ru } from "./ru";
import { uk } from "./uk";
import type { StringKey } from "./strings";

/**
 * Codex QC finding (2026-08-16): format/protocol literals -- `.jwlibrary`,
 * `manifest.json`, `user_data` -- are embedded INSIDE translatable
 * `errors.*` catalog values (en.ts). These are load-bearing format warts
 * (CLAUDE.md: "must stay byte-compatible with the existing Python app and
 * JW Library itself"), not prose. A future real translation could alter or
 * drop them from a non-English catalog and the error banner would then
 * display a wrong format token.
 *
 * Guard: every non-empty translation of a key whose English value contains
 * one of these literals must contain that literal VERBATIM. Fails loudly
 * the moment a translator (human or machine) breaks a protected token, and
 * works for locales that don't exist yet -- no restructuring of the string
 * catalog required (D11-02 keeps `errors.*` values as plain translatable
 * sentences with `{param}` interpolation only for genuinely variable data,
 * e.g. `errors.default`'s `{operation}`; `.jwlibrary`/`manifest.json`/
 * `user_data` are fixed format identifiers embedded in fixed sentences, so
 * there is no interpolation seam to extract them into without inventing a
 * markup-aware template language for these three cases alone).
 */
const PROTECTED_LITERALS = [".jwlibrary", "manifest.json", "user_data"] as const;

const LOCALES: Record<string, Partial<Record<StringKey, string>>> = {
  de,
  es,
  fr,
  it: itLocale,
  pl,
  pt,
  ru,
  uk,
};

describe("format-literal protection across locale catalogs", () => {
  const protectedKeys = (Object.keys(en) as StringKey[]).filter((key) =>
    PROTECTED_LITERALS.some((literal) => en[key].includes(literal)),
  );

  it("found the expected protected keys in en.ts (sanity check the scan itself still matches reality)", () => {
    expect(protectedKeys.sort()).toEqual(
      ["errors.missingManifest", "errors.missingUserDataBackup", "errors.notAZip"].sort(),
    );
  });

  for (const [locale, catalog] of Object.entries(LOCALES)) {
    for (const key of protectedKeys) {
      it(`${locale}: "${key}" preserves every protected literal verbatim when translated`, () => {
        const translated = catalog[key];
        if (translated === undefined) return; // untranslated key falls back to en -- nothing to check
        const englishLiterals = PROTECTED_LITERALS.filter((literal) => en[key].includes(literal));
        for (const literal of englishLiterals) {
          expect(translated).toContain(literal);
        }
      });
    }
  }
});
