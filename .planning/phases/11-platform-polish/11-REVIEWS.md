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
