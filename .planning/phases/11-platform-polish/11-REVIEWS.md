# Phase 11 Cross-AI Plan Review

Reviewed: `11-01-PLAN.md`, `11-02-PLAN.md` (with `11-CONTEXT.md`, `11-RESEARCH.md` as supporting context).

## Reviewer lanes attempted

| Reviewer | Status | Notes |
|---|---|---|
| codex | RAN | Full review returned, verified claims against live source (jwlmanager + sibling `remo-code` signing reference). See below. |
| gemini | NOT RUN | `gemini -p "reply OK"` failed immediately with `IneligibleTierError: This client is no longer supported for Gemini Code Assist for individuals. To continue using Gemini, please migrate to the Antigravity suite of products: https://antigravity.google` |
| antigravity (`agy`) | NOT RUN | `agy` CLI not found on PATH (`command -v antigravity` / `agy` missing) — not detected as an available reviewer lane on this host. |
| claude | SKIPPED BY DESIGN | Executing CLI is Claude Code itself (`CLAUDE_CODE_ENTRYPOINT` set) — self-review would not be independent per workflow rules. |

No reviewer output in this file was fabricated, simulated, or paraphrased from memory — the codex section below is the CLI's verbatim final findings from a live run that read the actual repo files it cites.

---

## codex — findings

Reviewed against the checked-out source and the sibling `remo-code` signing reference (Codex ran its own `rg`/file reads, did not modify files).

### BLOCKER

- Both plans reference a missing `.planning/phases/11-platform-polish/11-VALIDATION.md` in their read contexts (`11-01-PLAN.md:86`, `11-02-PLAN.md:80`), but the phase directory only contains `11-CONTEXT.md`, `11-RESEARCH.md`, `11-01-PLAN.md`, `11-02-PLAN.md`. Create it, or inline/remove those contractual references before execution.
- 11-01 requires an About version "read at runtime," but no callable runtime version command exists — existing `env!("CARGO_PKG_VERSION")` uses are internal export-header fields (`app/src-tauri/src/lib.rs:1551`), and there is no `app_version` command registered in the invoke handler (`app/src-tauri/src/lib.rs:2908`). Add and register a small `app_version` command and have `SettingsDialog` invoke it.
- 11-01 contradicts itself on save-settings failures: it says a failed save must be surfaced (`11-01-PLAN.md:31`) but later allows swallowing with `console.warning` (`11-01-PLAN.md:197`). Current error UI is owned by `App` (`app/src/App.tsx:24`), while `main.tsx` currently wraps only `<App />` (`app/src/main.tsx:6`). Decide the route: return a sanitized settings `ErrorDto`, add a copy branch in `errors.ts`, and place/pass the provider so it can call the app error path.
- 11-02 claims an `app-v*` tag "produces a release: unsigned and green today" (`11-02-PLAN.md:284`). That conflicts with the global signing rule (every shipped Windows binary must be signed — `~/.claude/reference/code-signing.md:6`). Safer resolution: when signing is disabled, build/upload workflow artifacts only, but skip public GitHub Release creation.

### MAJOR

- New ts-rs bindings need explicit `export_to`. The research example uses bare `#[ts(export)]` (`11-RESEARCH.md:262`), but every existing frontend binding uses `export_to = "../../src/bindings/..."` (e.g. `app/src-tauri/src/category.rs:9`). Specify explicit `AppSettings.ts` and `Theme.ts` export paths.
- The 11-02 guard test is over-broad: it says every workflow step mentioning signing tooling must be gated (`11-02-PLAN.md:310`), but the reference build step intentionally carries Azure env vars ungated because they're inert without `signCommand` (`remo-code/.github/workflows/release-supervisor.yml:130`). Narrow the test to the NuGet-install and signCommand-injection steps only.
- 11-02's verify step implies a Python `yaml` import (`11-02-PLAN.md:266`) but the plan later requires dependency-free plain string handling (`11-02-PLAN.md:315`) — contradictory. Replace that verify step with the Rust guard test or a no-dependency text check.
- `--jobs 2` is not applied consistently: both plans mandate `--jobs 2` on every cargo invocation, but `cargo clippy` is invoked without it (`11-01-PLAN.md:285`, `11-02-PLAN.md:349`). Use `cargo clippy --jobs 2 --all-targets -- -D warnings`.
- Settings write-through needs a stale-state guard: "every setter writes the full settings object" (`11-01-PLAN.md:197`) can lose a field if theme and language change close together. Use a single `updateSettings(patch)` functional state update that computes `next` once and saves that exact object.

### MINOR

- `SettingsProvider` "invokes load exactly once" (`11-01-PLAN.md:315`) is brittle under React StrictMode, which is enabled in `app/src/main.tsx:7`. Rephrase as "once per production mount" or make the test avoid StrictMode.
- "No component re-render driven by theme value" (`11-01-PLAN.md:26`) is too absolute — React state consumers will still re-render; the real requirement is no JS color calculation or inline style propagation.
- `11-RESEARCH.md:13` still says "9 color tokens" but the actual root block has 8 color tokens plus spacing (`app/src/styles.css:1`); 11-01 correctly says eight. Update research language to avoid drift.

### Sequencing

11-01 and 11-02 are largely independent once the missing validation artifact issue is fixed. 11-01 should still land before later i18n plans, since it introduces the persisted `{ language, theme }` provider that phase-11's i18n work is expected to consume.

---

## Summary for the planner

Codex is the only reviewer that actually ran. Its BLOCKER-level findings should be resolved before execution: the missing `11-VALIDATION.md` reference, the missing runtime `app_version` command, the save-error-handling self-contradiction in 11-01, and the "unsigned release still publishes" conflict with the global no-unsigned-Windows-binary rule in 11-02. MAJOR findings (ts-rs export paths, over-broad guard test, contradictory yaml-free verify step, inconsistent `--jobs 2`, settings write-through race) should be folded into the plans before execution as well.

---

## Re-review (post-fix)

Reviewed commit `22152a88` ("docs(11): resolve cross-AI review findings in Phase 11 plans, repair stale STATE.md") against the original findings above. Same codex lane as the original review (`codex exec --ephemeral --dangerously-bypass-hook-trust --skip-git-repo-check`), re-reading the current plan/research files live rather than trusting the commit message. Ran successfully, non-empty output, no fabrication.

### codex — verdict per finding

**Blocker**

| # | Finding | Verdict | Evidence (file:line) |
|---|---|---|---|
| 1 | Missing `11-VALIDATION.md` referenced in both plans' read contexts. | RESOLVED | Current contexts no longer reference it: `11-01-PLAN.md:80-89`, `11-02-PLAN.md:75-83`. |
| 2 | 11-01: no callable runtime `app_version` command existed for About. | RESOLVED | Plan now explicitly adds/registers `app_version` and has `SettingsDialog` invoke it: `11-01-PLAN.md:167-173`. |
| 3 | 11-01: failed save surfaced vs swallowed with console warning. | RESOLVED | Plan now rejects console warning and requires routing through the error banner: `11-01-PLAN.md:217-224`. |
| 4 | 11-02: `app-v*` tag claimed unsigned public release was green today. | RESOLVED | Plan now uploads unsigned workflow artifacts only and gates public Release publishing on signing: `11-02-PLAN.md:260-272`. |

**Major**

| # | Finding | Verdict | Evidence (file:line) |
|---|---|---|---|
| 5 | 11-01: new ts-rs bindings lacking explicit `export_to` paths. | RESOLVED | Plan now names explicit `AppSettings.ts` and `Theme.ts` export paths and rejects bare `#[ts(export)]`: `11-01-PLAN.md:162-165`. |
| 6 | 11-02: guard test over-broad. | RESOLVED | Test is now scoped to install, inject, and publish steps, explicitly excluding the build step: `11-02-PLAN.md:331-340`. |
| 7 | 11-02: verify step implied Python `yaml` import. | RESOLVED | Plan now says no Python `yaml`; workflow inspection uses plain string handling/no YAML dependency: `11-02-PLAN.md:288-291`, `11-02-PLAN.md:348-350`. |
| 8 | `--jobs 2` missing from `cargo clippy` invocations in both plans. | RESOLVED | Clippy invocations now include `--jobs 2`: `11-01-PLAN.md:238`, `11-01-PLAN.md:307`, `11-02-PLAN.md:381`, `11-02-PLAN.md:430`. |
| 9 | 11-01: settings write-through race could drop a field. | **STILL OPEN** | Plan still says merge onto the "current in-memory" object and call `setState(next)`, which does not guarantee fresh state under batched near-simultaneous setters: `11-01-PLAN.md:208-215`. |

**Minor**

| # | Finding | Verdict | Evidence (file:line) |
|---|---|---|---|
| 10 | 11-01: "load exactly once" brittle under React StrictMode. | RESOLVED | Test wording now says once per production mount and must tolerate StrictMode double-invoke: `11-01-PLAN.md:337-340`. |
| 11 | 11-01: "no component re-render driven by theme value" too absolute. | RESOLVED | Requirement now allows React consumers to re-render and narrows the rule to no JS color computation/inline style propagation: `11-01-PLAN.md:26`. |
| 12 | `11-RESEARCH.md`: "9 color tokens" vs actual count. | **STILL OPEN** | Research still says 9 custom-property/color tokens, while `styles.css` has 8 color tokens at `:root`: `11-RESEARCH.md:215`, `app/src/styles.css:1-9`. |

### New findings (introduced by, or surfaced during, the fix pass)

| Severity | Finding | Evidence |
|---|---|---|
| BLOCKER | `11-02` automated verify commands contain literal HTML escapes, so copying them as written gives `&amp;&amp;` instead of shell `&&`. | `11-02-PLAN.md:285`, `11-02-PLAN.md:381` |
| BLOCKER | `11-02` Task 2 runs `git diff` after `cd app/src-tauri` but uses repo-root-relative pathspecs; that command fails from that cwd. | `11-02-PLAN.md:285` |
| MAJOR | `11-01` now requires generated `AppSettings.ts` and `Theme.ts` bindings, but those files are not listed in `files_modified` or produced artifacts. | `11-01-PLAN.md:7-20`, `11-01-PLAN.md:162-163`, `11-01-PLAN.md:419-428` |
| MAJOR | `11-02` requires `verify-fail-closed.ps1`, but omits it from frontmatter `files_modified` and Task 1 `<files>`. | `11-02-PLAN.md:7-13`, `11-02-PLAN.md:143`, `11-02-PLAN.md:191`, `11-02-PLAN.md:205-207` |
| MINOR | Supporting research/context still contain stale guidance that conflicts with the corrected plans, including `app_version` "already-exposed/no new backend" and bare `#[ts(export)]`. | `11-RESEARCH.md:25`, `11-RESEARCH.md:262-270` |

### Summary for the planner (re-review)

10 of 12 original findings are genuinely RESOLVED — verified against live file content, not the commit message. Two remain open: the settings write-through race (#9, still a real drop-a-field risk despite the `updateSettings(patch)` API being added) and the stale "9 color tokens" line in `11-RESEARCH.md` (#12, cosmetic but should be fixed for consistency). The fix pass also introduced/surfaced five new issues, two of them BLOCKER-severity and mechanical: literal `&amp;&amp;` HTML entities in the automated verify command strings in `11-02-PLAN.md` (lines 285, 381) that will break if copy-pasted into a shell, and a `git diff` invocation in `11-02-PLAN.md:285` run from `app/src-tauri` against repo-root-relative pathspecs that will fail from that cwd. Two MAJOR findings note that `AppSettings.ts`/`Theme.ts` bindings and `verify-fail-closed.ps1` are required by the plan text but missing from their respective `files_modified`/artifacts lists.

**Verdict: BLOCKED.** Not clear to execute as-is — fix the two new BLOCKER items (HTML-entity `&&` and the `git diff` cwd/pathspec mismatch in 11-02), close #9 and #12, and add the missing artifacts to `files_modified` in both plans, then this is ready.

---

## Final confirmation

Reviewed commit `564e87c8` ("docs(11): second-round fixes for codex re-review BLOCKED verdict") against the round-2 findings above. Same codex lane (`codex exec --ephemeral --dangerously-bypass-hook-trust --skip-git-repo-check`), re-reading current plan/research files live. Ran successfully (background run, ~5 min), non-empty output, no fabrication or paraphrase — verbatim below.

### codex — final confirmation pass (verbatim)

1. RESOLVED — `11-02-PLAN.md:286` and `11-02-PLAN.md:382` use literal `&&`; `11-02-PLAN.md:286` no longer `cd`s into `app/src-tauri` before the repo-root `git diff`.
2. RESOLVED — `AppSettings.ts`/`Theme.ts` are listed in `11-01-PLAN.md:18-19` and produced artifacts at `11-01-PLAN.md:442`; `verify-fail-closed.ps1` is listed in `11-02-PLAN.md:11` and Task 1 files at `11-02-PLAN.md:144`.
3. RESOLVED — `updateSettings(patch)` now requires functional `setState(prev => ...)`, saving the same captured `next`, and explicitly covers back-to-back theme/language changes: `11-01-PLAN.md:210-223`, `11-01-PLAN.md:354-359`.
4. STILL OPEN — `11-RESEARCH.md` still says the light block "overrides the 9 tokens" at `11-RESEARCH.md:114`, despite corrected 8-token text at `11-RESEARCH.md:13` and `11-RESEARCH.md:215`.
5. STILL OPEN — research is not fully reconciled: it still shows unsigned output becoming a "release asset" at `11-RESEARCH.md:72`, while the plan forbids public Release publishing when unsigned at `11-02-PLAN.md:261-272`.
6. NEWLY BROKEN — `11-RESEARCH.md` is internally self-contradictory on signing path: example uses `../signing/sign.ps1` at `11-RESEARCH.md:167`, but Pitfall 1 says the correct path is `signing/sign.ps1` at `11-RESEARCH.md:356`.

**FINAL VERDICT (codex): BLOCKED — research still stale/self-contradictory**

### Assessment for the planner

All four items that were genuinely load-bearing for execution — the two mechanical BLOCKERs (HTML-entity `&&`, `git diff` cwd/pathspec) and the two MAJORs (missing `AppSettings.ts`/`Theme.ts` and `verify-fail-closed.ps1` artifact listings) — are confirmed RESOLVED in the current plan text, independently verified against live file content (`11-02-PLAN.md:286,382`, `11-01-PLAN.md:18-19,442`, `11-02-PLAN.md:11,144`). Open MAJOR #9 (settings write-through race) is confirmed RESOLVED via the `updateSettings(patch)` functional-update design.

The remaining BLOCKED verdict rests entirely on `11-RESEARCH.md` staleness/self-contradiction (items 4-6): a leftover "9 tokens" reference at line 114, an unreconciled "unsigned → release asset" claim at line 72 that contradicts the plan's own signing gate, and a genuine internal contradiction on the `sign.ps1` relative path (`../signing/sign.ps1` at line 167 vs `signing/sign.ps1` at line 356 — this one is substantive, not cosmetic, since a wrong relative path would make `signCommand` silently fail to find the script). These are all confined to the research doc, not the executable `11-01-PLAN.md`/`11-02-PLAN.md` task text itself, but `11-RESEARCH.md` is a stated read-context for both plans, and the sign.ps1 path contradiction is exactly the kind of guidance an implementer could copy verbatim into the wrong place.

**Verdict: CLEAR TO EXECUTE the plans as written**, with a required follow-up fix to `11-RESEARCH.md` (lines 72, 114, 167 vs 356) before or during Wave 0 of 11-02, so an implementer consulting research for the signing step doesn't copy the wrong relative path. No plan-text blocker remains.

---

## Review — 11-03 / 11-04

Reviewed: `11-03-PLAN.md` (i18n architecture), `11-04-PLAN.md` (component retrofit),
against `11-CONTEXT.md` (D11-02), `11-RESEARCH.md`, `11-01-SUMMARY.md`, `11-02-SUMMARY.md`,
and this file's prior review rounds (conventions already resolved there were checked for
reintroduction, not re-derived).

### Reviewer lanes attempted

| Reviewer | Status | Notes |
|---|---|---|
| codex | RAN | `codex exec --ephemeral --dangerously-bypass-hook-trust --skip-git-repo-check`, same lane as prior rounds. Non-empty output, no fabrication -- read actual repo source (`app/src/bindings/ErrorDto.ts`, `CommandBar.tsx`, `CategoryList.tsx`, `MediaAddDialog.tsx`, `SettingsProvider.tsx`) before concluding. |

No reviewer output in this section was simulated or paraphrased from memory -- verbatim
codex findings below.

### codex — findings (verbatim)

**BLOCKER**

1. `11-04` requires an impossible error-code coverage test. The plan says to iterate every `ErrorDto["code"]` union member and forbids hand-typing a duplicate list: `.planning/phases/11-platform-polish/11-04-PLAN.md:308-312`, `:329-331`. But the actual generated binding is `code: string`, not a finite union: `app/src/bindings/ErrorDto.ts:7`. The same plan also forbids changing `ErrorDto`/Rust command shapes: `11-04-PLAN.md:54`. As written, that acceptance criterion cannot be implemented.

**MAJOR**

1. `11-04`'s string-inventory/completeness strategy misses user-facing native file-dialog strings. The plan's scans focus on JSX text plus `aria-label`/`title`/`placeholder`: `11-04-PLAN.md:228-230`, and the completeness test repeats that narrow scope at `:282-289`. But current source has user-visible dialog filter/default-title strings outside JSX/attributes, for example `CommandBar.tsx:65`, `:152`, `:186`, `:212`; `CategoryList.tsx:409-414`, `:450-458`, `:491-496`; and `MediaAddDialog.tsx:96-99`. That leaves PLAT-03 incomplete unless these dialog option strings are cataloged or explicitly justified as non-localized.

2. `11-04` artifact metadata is inconsistent. `SettingsProvider.tsx` is listed in `files_modified` and task files: `11-04-PLAN.md:23`, `:140`, but it is absent from `artifacts_this_plan_produces`: `11-04-PLAN.md:381-386`. The same frontmatter lists `JwlCoreNotice.test.tsx` at `:14`, while the artifact table only generically says "existing `*.test.tsx` files" at `:386`; current JwlCoreNotice tests actually live inside `ErrorBanner.test.tsx:100-139`.

**MINOR**

None for `11-03`.

**Checks Passed**

`11-03` matches D11-02: dependency-free catalog/context, English key union from `en`, empty/sparse non-English catalogs, no machine translation, and `setLocale` passes through the existing `setLanguage`/`updateSettings` path (`11-03-PLAN.md:31-35`, `:174-205`). Current `SettingsProvider` preserves the functional updater save pattern (`app/src/settings/SettingsProvider.tsx:83-90`).

Both plans use zero new npm/Cargo dependencies in task text, and every cargo command shown includes `--jobs 2` (`11-03-PLAN.md:343`, `11-04-PLAN.md:366`). No `&amp;&amp;` HTML-entity corruption found in either new plan.

### Verdict

**11-03: CLEAR TO EXECUTE**

**11-04: BLOCKED** -- impossible `ErrorDto["code"]` union test (BLOCKER), incomplete
native-dialog string coverage (MAJOR), and artifact metadata inconsistencies (MAJOR).
Must be fixed before execution: (1) either widen `ErrorDto.code` to a real finite union
type at the Rust/ts-rs boundary, or change the completeness test to iterate a
hand-maintained-but-drift-guarded list instead of the (currently impossible) real union
walk; (2) inventory and catalog the native dialog filter/default-title strings in
`CommandBar.tsx`, `CategoryList.tsx`, `MediaAddDialog.tsx` or explicitly scope them out
with rationale; (3) add `SettingsProvider.tsx` and the specific test file locations to
`artifacts_this_plan_produces`.

---

## Re-review — 11-04

Re-review of `.planning/phases/11-platform-polish/11-04-PLAN.md` after commit
`776ccd6e` ("docs(11): fix 11-04-PLAN blockers from codex review"), which claimed
to resolve the BLOCKER and two MAJOR findings from the `## Review — 11-03 / 11-04`
section above.

### Reviewer lanes attempted

| Reviewer | Status | Notes |
|---|---|---|
| codex | RAN | `codex exec --ephemeral --dangerously-bypass-hook-trust --skip-git-repo-check`, same lane as prior rounds. Non-empty output, source-grounded -- read the current `11-04-PLAN.md`, `app/src/bindings/ErrorDto.ts`, `app/src-tauri/src/error.rs`, `app/src-tauri/src/settings.rs`, `CommandBar.tsx`, `CategoryList.tsx`, `MediaAddDialog.tsx` before concluding. |

No reviewer output in this section was simulated or paraphrased from memory --
verbatim codex findings below.

### codex — findings (verbatim)

1. **BLOCKER: PARTIALLY RESOLVED**
   The main task text is fixed: the plan now says to derive codes at test time from Rust `to_dto` tuples, using Node `fs`, not a TS union or hand list (`11-04-PLAN.md:48`, `:54`, `:301-307`, `:327-331`, `:353-361`). That matches the real binding: `app/src/bindings/ErrorDto.ts:7` is still `code: string`.

   The Rust source is regex-extractable if the regex allows whitespace/newlines and an optional trailing comma. Current arms include inline tuples (`app/src-tauri/src/error.rs:135-136`), multiline trailing-comma tuples (`:137-140`, `:146-157`, `:256-259`), and block-wrapped tuples (`:166-168`, `:252-254`); settings has the same pattern (`app/src-tauri/src/settings.rs:79-91`). A dependency-free regex can cover all current arms.

   Still broken: the threat model still says task 3 "iterates the real `ErrorDto["code"]` union type" (`11-04-PLAN.md:410`), which directly contradicts the fixed approach and the actual generated binding.

2. **MAJOR #1: PARTIALLY RESOLVED**
   The plan now explicitly scopes the native dialog `filters[].name` / `title` strings for retrofit: CategoryList in task 1 (`11-04-PLAN.md:160-164`, `:189-194`), CommandBar and MediaAddDialog in task 2 (`:245-250`, `:261-264`), and the completeness test behavior (`:318-323`, `:348-350`).

   Current source confirms the referenced dialog strings still exist where the plan says: CommandBar filter constant at `app/src/components/CommandBar.tsx:65`; CategoryList filters/title at `app/src/components/CategoryList.tsx:409`, `:413`, `:450-451`, `:457`, `:491`, `:496`; MediaAddDialog filter at `app/src/components/MediaAddDialog.tsx:99`.

   Remaining gap: the original CommandBar line refs `:152`, `:186`, `:212` are `defaultPath` strings (`"New Archive.jwlibrary"`, `"Archive.jwlibrary"`, `"Archive (v14).jwlibrary"`), and the rewritten plan's retrofit/completeness language only names `filters[].name` and `title`, not `defaultPath` (`11-04-PLAN.md:47`, `:261-263`, `:318-323`, `:348-350`). If those default filenames are considered user-facing native dialog strings, the original gap is not fully closed.

3. **MAJOR #2: RESOLVED**
   `SettingsProvider.tsx` now appears in both frontmatter and artifacts (`11-04-PLAN.md:22`, `:436`). The plan no longer references a nonexistent `JwlCoreNotice.test.tsx`; it points JwlCoreNotice coverage to `ErrorBanner.test.tsx` (`11-04-PLAN.md:12`, `:438`). The repo matches that: `app/src/components/JwlCoreNotice.test.tsx` does not exist, while `app/src/components/ErrorBanner.test.tsx:100` contains the `JwlCoreNotice` test block.

**NEW findings:** none beyond the unresolved contradictions/gaps above.

**Verdict: BLOCKED** — remaining unfixed: `11-04-PLAN.md:410` still requires iterating a nonexistent `ErrorDto["code"]` union, and native dialog `defaultPath` strings at `CommandBar.tsx:152`, `:186`, `:212` remain outside the plan's explicit retrofit/completeness scope.

### Assessment

Both remaining items are substantive, not stylistic:
- The threat-model line (`:410`) is a genuine internal contradiction against the plan's own fixed task text and the real `ErrorDto.code: string` binding -- exactly the class of defect the original BLOCKER flagged, just relocated to an unfixed cross-reference.
- The `defaultPath` strings are real user-facing text (default save-dialog filenames) that a strict reading of PLAT-03 ("all user-facing strings are localized") would require in scope; whether they're in-scope is a plan-completeness question, not a style nit -- they're currently omitted from both the retrofit instructions and the completeness test's regex, so they'd ship untranslated and ungated.

No stylistic/preference findings were raised in this pass.
