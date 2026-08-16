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
