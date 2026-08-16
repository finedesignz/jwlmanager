import type { StringKey } from "./strings";

// Deliberately empty pending real translation work (D11-02). Falls back to
// English at runtime (`I18nContext.t`'s `catalogs[locale]?.[key] ?? en[key]`
// fallback expression) for every key. Do NOT fill with machine-translated
// text -- see 11-CONTEXT.md D11-02 rationale (fabricated strings for a
// personal-data app are a trust regression, not parity).
export const es: Partial<Record<StringKey, string>> = {};
