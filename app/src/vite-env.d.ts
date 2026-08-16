/// <reference types="vite/client" />

// Vite's built-in `?raw` import suffix (core Vite feature, no plugin/
// dependency needed) returns a file's contents as a plain string. This
// project has no `@types/node`, so `styles_tokens.test.ts` (11-01-PLAN.md
// Task 3) reads `styles.css` at test time via this import form instead of
// `node:fs` -- keeping the zero-new-dependency line every prior phase held.
declare module "*.css?raw" {
  const content: string;
  export default content;
}
