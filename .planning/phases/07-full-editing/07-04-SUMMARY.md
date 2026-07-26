---
phase: 07-full-editing
plan: "04"
subsystem: editing
tags: [scrub, clean, mask, unicode, prng, react]
dependency-graph:
  requires: ["07-01 db/edit.rs safety spine", "07-01 EditPreviewDialog", "07-03 UtilitiesMenu"]
  provides: ["Clean Archive (EDIT-06)", "Mask Archive (EDIT-06)", "EditPreviewDialog requireTypedConfirm mode"]
  affects: [app/src-tauri/src/db, app/src-tauri/src/error.rs, app/src-tauri/src/lib.rs, app/src-tauri/Cargo.toml, app/src/components/EditPreviewDialog.tsx, app/src/components/UtilitiesMenu.tsx, app/src/lib/errors.ts, app/src/styles.css]
tech-stack:
  added: []
  patterns:
    - "hand-rolled SplitMix64 PRNG (src/db/scrub.rs), following src/time.rs's own dependency-free precedent — no rand crate added, seed threaded explicitly as a plain u64 parameter exactly like now: &str is threaded at save_archive"
    - "per-character regex.is_match against single-char strings to classify Unicode General Category (\\p{Zs}/\\p{Zl}/\\p{Zp}/\\p{L}) — sidesteps Rust regex's lack of the Python regex crate's -- set-subtraction operator by special-casing ASCII space in the transform loop instead of the character class"
    - "row-count dry-run envelope (apply_*(tx) -> BTreeMap<String, usize>, wrapped into DryRunReport.overwritten) reused from reorder.rs's reorder_report shape rather than the generic PK-snapshot diff_snapshots — Clean/Mask always keep every row's PK, so diff_snapshots could never distinguish a touched row from an untouched one"
key-files:
  created:
    - app/src-tauri/src/db/scrub.rs
    - app/src-tauri/tests/scrub_tests.rs
  modified:
    - app/src-tauri/src/db/mod.rs
    - app/src-tauri/src/error.rs
    - app/src-tauri/src/lib.rs
    - app/src-tauri/Cargo.toml
    - app/src/components/EditPreviewDialog.tsx
    - app/src/components/EditPreviewDialog.test.tsx
    - app/src/components/UtilitiesMenu.tsx
    - app/src/components/UtilitiesMenu.test.tsx
    - app/src/lib/errors.ts
    - app/src/styles.css
decisions:
  - "Clean's row-touch gate counts a \\r-only row as changed, a deliberate small widening beyond Python's own `combined` detector (JWLManager.py:3732), which omits \\r from its own regex.search gate even though clean() always converts \\r to \\n when a row IS touched by another reason. The plan's behavior bullets explicitly list \\r->\\n as one of 'the above' behaviors that gate row-touch counting, so this port follows the plan's stated contract over the Python's narrower literal gate."
  - "Mask's apply_mask only UPDATEs and counts a row when at least one relevant field is non-empty, unlike Python's obscure_bookmarks/obscure_notes which always issue an UPDATE per row regardless of content (an unconditional if/else always executes one UPDATE branch). This makes the DryRunReport's row counts semantically meaningful (rows actually mutated) rather than 'every row in the table' — acceptable per the plan's own instruction that Mask parity is asserted on shape invariants, never a byte-diff or literal-UPDATE-count oracle."
  - "Tasks 1 and 2 (Clean, Mask) landed as a single commit rather than two — both live in the exact same files (db/scrub.rs, error.rs, lib.rs, Cargo.toml, scrub_tests.rs) since they share the module's dry-run/apply envelope; a task-boundary split would only fragment one cohesive module, not reflect an independently revertable unit."
metrics:
  duration: "single session"
  completed: 2026-07-26
status: complete
---

# Phase 7 Plan 4: Clean + Mask (EDIT-06) Summary

Shipped Clean (Unicode separator normalization) and Mask (privacy-scrub with a hand-rolled seeded PRNG) — the two archive-wide, selection-free text operations — and gave Mask the strongest confirmation friction in the app: a dry-run preview plus a non-bypassable case-sensitive typed `MASK` confirm plus a restrained destructive accent, gated behind `EditPreviewDialog`'s new `requireTypedConfirm` mode.

## What Was Built

**Tasks 1+2 (commit `a5c5171e`) — `db/scrub.rs`.** `clean_text(input) -> Option<String>` ports `clean(txt)` (`JWLManager.py:3700-3703`) as a single per-character pass: `\r` → `\n`, ASCII space (U+0020) left alone, every other `\p{Zs}` → ASCII space, `\p{Zl}`/`\p{Zp}` removed. Since Rust's `regex` crate has no `regex.V1` `--` set-subtraction the Python's `[\p{Zs}--\x20]` relies on, ASCII space is special-cased directly in the transform rather than expressed as a character class. `apply_clean(tx)` scans `InputField` (keyed by `TextTag`) and `Note` (keyed by `NoteId`), binds every changed value as a parameter, and returns per-table ROW counts (not replacement counts — a row with two separators still counts once). `dry_run_clean(conn)` wraps it in the established rolled-back-`unchecked_transaction` + `PragmaGuard` envelope.

`obscure_text(input, rng: &mut impl SeedRng) -> String` ports `obscure_text` (`JWLManager.py:3752-3768`): picks one random word per call from `['obscured','yada','bla','gibberish','børk']` and cycles its letters (case-preserved per character) over every `\p{L}` character; every non-letter is copied through byte-identical. `SplitMix64` is a small, cited, dependency-free PRNG (`rand` stays absent from `Cargo.lock` per 07-RESEARCH.md Correction 4) following `src/time.rs`'s own hand-rolled-algorithm precedent — `seed: u64` is threaded explicitly through `apply_mask(tx, seed)`/`dry_run_mask(conn, seed)`, the SAME pattern `now: &str` uses at `save_archive`. `apply_mask` covers `InputField.Value`, `Bookmark.Title`/`Snippet`, `Note.Title`/`Content`, and `Location.Title` — exactly the six columns EDIT-06 specifies, nothing else, and never publication body text (this app never loads any into these tables).

Both ops' row counts wrap into `DryRunReport.overwritten` (every touched row keeps its PK — always an UPDATE-in-place), matching `reorder.rs`'s `reorder_report` shape rather than the shared PK-snapshot `diff_snapshots` (which can't express "this row's TEXT changed," only presence). `error.rs` gains `ArchiveError::CleanFailed`/`MaskFailed` → `clean_failed`/`mask_failed` DTO codes, `reason` kept internal-only per the module's established convention. `lib.rs` registers `clean_dry_run`/`clean_apply`/`mask_dry_run`/`mask_apply`, each command drawing its own wall-clock seed via a dedicated `mask_seed_now()` (a distinct function from `guid_seed_now()` so one seed source can't silently perturb the other). `Cargo.toml`'s `regex` why-comment is widened to note the new Unicode-character-class use over user-authored archive text. 14 tests in `tests/scrub_tests.rs` cover every behavior bullet: NBSP/ideographic-space→ASCII-space, ASCII-space-untouched, Zl/Zp removal, CR→LF, the two-separators-one-count contract, mask's length/case/non-letter-identity/determinism-under-seed invariants on a mixed-script fixture (Latin, Cyrillic accented, digit, punctuation, emoji), full six-column coverage, publication-content-table (`Tag`) untouched, empty-row skip, and a `no_rand_or_fancy_regex_dependency_declared` guard reading `Cargo.toml` directly.

**Task 3 (commit `acba7c91`) — `EditPreviewDialog.tsx` typed-confirm mode + `UtilitiesMenu.tsx` wiring.** `EditPreviewDialog` gains an optional `requireTypedConfirm?: string` prop that renders a `data-testid="edit-preview-typed-confirm-input"` text input (44px min-height, `--bg-tertiary`, `--brand-primary` focus ring — same box metrics as `.toolbar-button`) and keeps Confirm `disabled` until the input's value is an EXACT, case-sensitive, UNTRIMMED match (`mask`/`Mask`/` MASK ` all stay disabled). The same prop gates a `.edit-preview-dialog-destructive` 2px top-border accent class — no other caller opts into the stronger visual. `handleConfirm` double-guards on `typedConfirmSatisfied` (not just the `disabled` attribute), and the input's `onKeyDown` explicitly `preventDefault()`s on Enter so the non-bypassability is directly testable, not merely incidental to there being no `<form>`. Cancel/Esc/click-outside stay unaffected by the gate.

`UtilitiesMenu` fires "Clean Archive…" → `clean_dry_run` → `EditPreviewDialog` (title "Clean this archive?", summary from `DryRunReport.overwritten` row totals, confirm "Clean Archive"/"Cleaning…") → `clean_apply`; "Mask Archive…" → `mask_dry_run` → `EditPreviewDialog` with `requireTypedConfirm="MASK"` (title "Mask this archive?", the unconditional irreversibility warning leading the summary followed by the dry-run row/table-list line, confirm "Mask Archive" disabled-until-match, pending "Masking…") → `mask_apply`. Both reuse the existing `onSorted` refresh callback (re-fetches the current category, closes the menu) rather than adding a second, functionally-identical prop — `CommandBar.tsx` stays untouched, out of this plan's file scope. `errors.ts` gains `clean_failed`/`mask_failed` copy sentences. `styles.css` adds the destructive-accent modifier and typed-confirm input styling using only existing tokens.

## Verification

- `cargo test --jobs 2` (full suite): **83 + 2 + 5 + 1 + 7 + 14 + 4 + 3 + 4 + 4 + 5 + 1 + 12 + 6 + 2 + 6 + 1 + 5 + 2 + 1 + 1 + 5 + 4 + 16 + 17 + 14 + 7 + 14 = all green, 0 failed** (per-binary breakdown from the run; `scrub_tests` itself: 14 passed, 0 failed).
- `cargo clippy --all-targets -- -D warnings`: clean, zero warnings (after fixing two `clippy::unnecessary_get_then_check` findings in the test file, converted to `!counts.contains_key(...)`).
- `cd app && npx vitest run`: **110 tests, 11 files, all passed.**
- `cd app && npx tsc --noEmit`: clean, no output.
- `grep -n "UPDATE " app/src-tauri/src/db/scrub.rs`: six matches — `InputField.Value` (×2, Clean+Mask), `Note.Title/Content` (×2), `Bookmark.Title/Snippet`, `Location.Title` — exactly the allowed column set, no other columns.
- `grep -n "requireTypedConfirm" app/src/components`: referenced only by `EditPreviewDialog.tsx` (definition/prop) and the Mask call site in `UtilitiesMenu.tsx` (plus their own test files).
- `grep -n "^rand\|^fancy-regex" app/src-tauri/Cargo.toml`: no matches — no new dependency was added; the module docs explain why the missing `--` set-subtraction is worked around by special-casing ASCII space instead.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Clippy `unnecessary_get_then_check` in test fixtures**
- **Found during:** Task 1/2, `cargo clippy --all-targets -- -D warnings`
- **Issue:** `counts.get("InputField").is_none()` triggers a clippy lint preferring `!counts.contains_key(...)`.
- **Fix:** Replaced both occurrences in `tests/scrub_tests.rs`.
- **Files modified:** `app/src-tauri/tests/scrub_tests.rs`
- **Commit:** `a5c5171e`

**2. [Rule 1 - Bug] Location `CHECK` constraint violation in test fixture helpers**
- **Found during:** Task 1/2, first `cargo test --test scrub_tests` run
- **Issue:** `insert_input_field`/`insert_bookmark`/`insert_location_with_title` initially seeded `Location.Type = 1` (Bible) with `KeySymbol = NULL`, tripping the v16 schema's `Type=1` CHECK constraint (`KeySymbol IS NOT NULL AND length(KeySymbol) > 0`).
- **Fix:** Switched the fixture helpers to `Type = 2` (the least-constrained CHECK branch, matching `common::insert_representative_locations`'s established Type-2 shape) with `DocumentId = 0`.
- **Files modified:** `app/src-tauri/tests/scrub_tests.rs`
- **Commit:** `a5c5171e`

No architectural deviations (Rule 4) — no new Cargo dependency was added, no schema change beyond the plan's UPDATE-only scope, and no package-legitimacy checkpoint was needed.

## Known Stubs

None.

## Threat Flags

None beyond what `07-04-PLAN.md`'s own `<threat_model>` already registers (T-07-19 through T-07-23, T-07-SC) — all mitigated as planned: dry-run preview + non-bypassable typed confirm + destructive accent + `busyRef` guard for Mask; values transformed in Rust and bound as parameters, never SQL-interpolated; column allowlist enforced in source and asserted by the publication-content-untouched test; `.expect` confined to `LazyLock` static regex compiles; no new dependency.

## Self-Check: PASSED

- `app/src-tauri/src/db/scrub.rs` — FOUND
- `app/src-tauri/tests/scrub_tests.rs` — FOUND
- Commit `a5c5171e` — FOUND in `git log --oneline --all`
- Commit `acba7c91` — FOUND in `git log --oneline --all`
