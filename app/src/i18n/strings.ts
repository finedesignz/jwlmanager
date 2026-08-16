import type { en } from "./en";

/**
 * Derived key union (D11-02) -- NEVER a separately hand-maintained list.
 * Adding a key to `en.ts` automatically extends `StringKey`; referencing a
 * key that doesn't exist in `en` is a compile error, and every non-English
 * locale file type-checks its own catalog against this same union.
 */
export type StringKey = keyof typeof en;
