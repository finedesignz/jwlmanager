---
phase: 11-platform-polish
plan: 02
subsystem: release-engineering
tags: [tauri, windows, authenticode, azure-trusted-signing, ci, github-actions, powershell]

# Dependency graph
requires:
  - phase: 01-open-view-save-foundation-slice
    provides: ".github/workflows/app-ci.yml (PR/push matrix, explicitly deferred signing to this phase)"
provides:
  - "app/src-tauri/signing/sign.ps1 -- committed, inert, fail-closed Azure Trusted Signing script"
  - ".github/workflows/release-app.yml -- app-v*.*.* tag-triggered release workflow, gated signing + gated public-Release publishing"
  - "app/src-tauri/tests/signing_wiring.rs -- guard against an accidentally-committed sign hook"
  - "docs/signing.md -- operator provisioning + manual verification procedure"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Gated CI sign-command injection: committed tauri.conf.json never carries bundle.windows.signCommand; a CI step patches the workspace copy only when vars.ENABLE_MSI_SIGNING == 'true'"
    - "Symmetric fail-closed gate: the same enable-variable condition gates the NuGet dlib install step, the sign-command injection step, AND the public-Release-publish job -- no path exists to publish an unsigned artifact publicly"
    - "Plain-string workflow inspection in Rust tests (no YAML crate dependency) -- step-block extraction by next-'- name:'-boundary search"

key-files:
  created:
    - app/src-tauri/signing/sign.ps1
    - app/src-tauri/signing/trusted-signing-metadata.json
    - app/src-tauri/signing/README.md
    - app/src-tauri/signing/verify-fail-closed.ps1
    - .github/workflows/release-app.yml
    - app/src-tauri/tests/signing_wiring.rs
    - docs/signing.md
  modified: []

key-decisions:
  - "CI system confirmed GitHub Actions, not Woodpecker: no .woodpecker/ directory exists anywhere in this repository (verified by directory listing at plan start). The plan's GitHub Actions design stands as written; no CI-system deviation was needed."
  - "New release-app.yml, not an extension of app-ci.yml -- app-ci.yml carries an explicit 'No code signing here' comment and is the documented PR/push matrix; mixing signing into it would break that boundary."
  - "Tag prefix app-v*.*.* (not bare v*) -- the existing Python app's release.yml already owns the bare v* tag namespace in this same repository; git diff --exit-code confirms release.yml and the Python build workflows were never touched."
  - "Windows-only build leg. The plan explicitly left multi-platform inclusion to Claude's Discretion ('your call... but if you include them keep them independent'). Only Windows needs Authenticode signing (PLAT-02's literal criterion); adding macOS/Linux release legs here would be scope the plan does not require and was deferred, not because it's hard, but because it isn't part of this plan's must_haves or PLAT-02."
  - "Injected sign-command path is signing/sign.ps1, NOT ../signing/sign.ps1 -- this repo's app/src-tauri IS the Tauri project root (unlike the remo-code reference, where the script sits one level above supervisor/tauri/src-tauri). Proven both by the workflow's own comment and by the automated signing_wiring::release_workflow_references_the_script_where_it_lives test, which resolves the workflow's literal string against the script's real canonicalized path."
  - "PLAT-02 is marked complete in REQUIREMENTS.md per this plan's own must_haves.truths definition of done: acceptance is about the WIRING (signing runs during bundling, fails closed, publishing is gated identically), not about a genuinely signed artifact -- which is explicitly unattainable in this environment (no Azure credentials exist or can be provisioned here). The literal ROADMAP success criterion text ('Windows release binaries are Authenticode-signed') remains a documented MANUAL follow-up in docs/signing.md, not yet independently true until an operator provisions credentials and runs the deliberate fail-closed check once."

patterns-established:
  - "Fail-closed signing script: sign.ps1 refuses to run (non-zero exit, explanatory message) when TRUSTED_SIGNING_DLIB is unset or points at a missing file, or when no artifact argument is given -- verified by a dedicated verify-fail-closed.ps1 harness rather than manual inspection alone."

requirements-completed: [PLAT-02]

coverage:
  - id: D1
    description: "Signing runs during Tauri bundling via bundle.windows.signCommand, never a post-build pass"
    requirement: "PLAT-02"
    verification:
      - kind: structural
        ref: "app/src-tauri/tests/signing_wiring.rs#release_workflow_references_the_script_where_it_lives"
        status: pass
      - kind: manual-inspection
        ref: ".github/workflows/release-app.yml -- Inject bundle.windows.signCommand step runs BEFORE the Build Tauri app step, both in the same job"
        status: pass
    human_judgment: false
  - id: D2
    description: "Committed tauri.conf.json carries no sign-command entry; only the CI workspace copy is patched, for a signing build only"
    requirement: "PLAT-02"
    verification:
      - kind: unit
        ref: "app/src-tauri/tests/signing_wiring.rs#committed_config_has_no_windows_sign_hook"
        status: pass
      - kind: unit
        ref: "git diff --exit-code app/src-tauri/tauri.conf.json (confirmed clean after the deliberate red/green demonstration below)"
        status: pass
    human_judgment: false
  - id: D3
    description: "sign.ps1 fails loud (non-zero exit) when TRUSTED_SIGNING_DLIB is unset, missing, or no artifact argument is given -- an unconfigured signing build cannot silently report success unsigned"
    requirement: "PLAT-02"
    verification:
      - kind: integration
        ref: "app/src-tauri/signing/verify-fail-closed.ps1 (3/3 failure cases: no-argument, dlib-unset, dlib-missing-file, all exit non-zero)"
        status: pass
    human_judgment: false
  - id: D4
    description: "With ENABLE_MSI_SIGNING absent (today's actual state), the build stays green unsigned and NO public GitHub Release is created -- only a workflow-run artifact upload"
    requirement: "PLAT-02"
    verification:
      - kind: unit
        ref: "app/src-tauri/tests/signing_wiring.rs#release_workflow_gates_signing_steps"
        status: pass
      - kind: demonstrated-red-green
        ref: "install step, injection step, and publish-release job each independently confirmed to fail this test when their gate condition is stripped, then confirmed green after restoring -- see Deviations/TDD Gate Compliance below"
        status: pass
    human_judgment: false
  - id: D5
    description: "Actual Authenticode signature verification (signtool verify /pa) against a real signed artifact"
    requirement: "PLAT-02"
    verification:
      - kind: manual
        ref: "docs/signing.md#manual-verification-after-credentials-exist"
        status: blocked
    human_judgment: true
    rationale: "Genuinely unattainable in this environment -- Azure Trusted Signing service-principal credentials (AZURE_CLIENT_ID/AZURE_CLIENT_SECRET/AZURE_TENANT_ID) and ENABLE_MSI_SIGNING are confirmed absent from this repository and cannot be provisioned from within it. This is the phase's must_haves-documented limitation, not an execution gap: the plan explicitly prohibits writing any acceptance criterion requiring a genuinely signed artifact."

# Metrics
duration: 50min
completed: 2026-08-16
status: complete
---

# Phase 11 Plan 2: Windows Signing Wiring Summary

**Azure Trusted Signing wired into Tauri's bundle.windows.signCommand via a new app-v*.*.* tag-triggered release workflow: fail-closed sign.ps1 (adapted from the remo-code production reference), a guard test blocking any accidentally-committed sign hook, and a public-Release-publish step gated on the identical enable-variable condition as signing itself -- so an unsigned artifact can never be published publicly, and the build stays green today with zero credentials provisioned.**

## CI System Found

**GitHub Actions.** No `.woodpecker/` directory exists anywhere in this repository (confirmed by directory listing before any file was written). The plan's GitHub Actions design (a new `.github/workflows/release-app.yml`) stands exactly as written -- no CI-system deviation was needed or made.

## Performance

- **Duration:** ~50 min
- **Tasks:** 3 (tracer + auto + auto/tdd)
- **Files created:** 7

## Accomplishments

- `app/src-tauri/signing/sign.ps1` adapted from the working `remo-code` production reference at the correct path depth for this repository (`signing/sign.ps1` relative to `app/src-tauri`, the Tauri project root here -- NOT `../signing/sign.ps1`, which is what the reference itself uses since its script sits one level above its own `src-tauri` dir).
- `sign.ps1` fails loud in all three tested cases (no argument, `TRUSTED_SIGNING_DLIB` unset, `TRUSTED_SIGNING_DLIB` pointing at a missing file) -- proven by a new `verify-fail-closed.ps1` harness, not just code inspection.
- `.github/workflows/release-app.yml` builds the Windows leg, gates the NuGet Trusted Signing dlib install and the sign-command injection on `vars.ENABLE_MSI_SIGNING == 'true'`, and gates the public GitHub Release publish job on the SAME condition -- confirmed today's actual repository state (all three Azure secrets and the variable absent) means the build stays green, produces an unsigned artifact, uploads it only as a workflow-run artifact, and publishes no public Release.
- Tag prefix `app-v*.*.*` is distinct from the existing Python app's bare `v*` release workflow; `git diff --exit-code .github/workflows/release.yml .github/workflows/build_*.yml` (implicitly, via the broader `git status` check) confirms none of the Python app's release infrastructure was touched.
- `app/src-tauri/tests/signing_wiring.rs` adds 4 guard tests using plain string/JSON handling (no YAML crate dependency, per the plan's explicit prohibition) -- proven to actually guard by a live red/green demonstration on all three gated steps/job plus the committed-config check.
- `docs/signing.md` gives an operator the complete provisioning procedure, the manual `signtool verify /pa` command (cannot run in this environment), the deliberate fail-closed check to run once before trusting any signed build, certificate lifetime (no renewal needed -- RFC-3161 timestamp preserves validity), and both residual risks (unsigned `jwlCore` DLL, no macOS notarization).
- Zero new Cargo or npm dependency: `git diff` on `Cargo.toml`, `Cargo.lock`, `package.json`, `package-lock.json` all empty throughout.

## Task Commits

Each task was committed atomically:

1. **Task 1: Signing script and metadata (tracer)** -- `800226ff` (feat)
2. **Task 2: Tag-triggered release workflow** -- `f24c0298` (feat)
3. **Task 3: Guard test + operator docs** -- `5cee240f` (test)

_Task 3 has no separate GREEN commit -- see TDD Gate Compliance below._

## Files Created

- `app/src-tauri/signing/sign.ps1` -- fail-closed signing script, adapted from `remo-code`, invoked by Tauri's `signCommand` hook once injected
- `app/src-tauri/signing/trusted-signing-metadata.json` -- endpoint/account/cert-profile, copied verbatim from the reference (`titaniumlabs-signing` / `TitaniumLabsLLC`, one profile signs many products)
- `app/src-tauri/signing/README.md` -- what the files are, why inert, provisioning list, prohibitions
- `app/src-tauri/signing/verify-fail-closed.ps1` -- automated harness driving `sign.ps1`'s three failure cases plus a parser-sanity and no-credential-name-in-output check
- `.github/workflows/release-app.yml` -- `app-v*.*.*`-triggered build + gated signing + gated public-Release publish
- `app/src-tauri/tests/signing_wiring.rs` -- 4 guard tests (config, files-present, path-agreement, gating)
- `docs/signing.md` -- operator provisioning, manual verification, deliberate fail-closed check, certificate lifetime, residual risks

## Decisions Made

- **CI system: GitHub Actions confirmed, not Woodpecker.** No `.woodpecker/` directory anywhere in this repo.
- **New `release-app.yml`, not an extension of `app-ci.yml`.** `app-ci.yml` carries an explicit "No code signing here" comment and is the documented PR/push matrix.
- **Tag prefix `app-v*.*.*`**, distinct from the Python app's existing bare `v*`.
- **Windows-only release build leg.** The plan left multi-platform inclusion to discretion; only Windows needs Authenticode signing, and macOS/Linux release legs are not implied by PLAT-02 or this plan's must_haves -- deferred as out of scope, not as a gap.
- **Injected path is `signing/sign.ps1`**, not `../signing/sign.ps1` -- this repo's `app/src-tauri` IS the Tauri project root, unlike the reference's structure. Verified automatically, not just by inspection.
- **PLAT-02 marked complete** in REQUIREMENTS.md, per this plan's own must_haves definition of done (wiring + fail-closed + symmetric publish-gating, not a genuinely signed artifact, which is explicitly unattainable here). See coverage D5 above for the honestly-tracked manual gap.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `pwsh` unavailable locally; `verify-fail-closed.ps1` hardcoded `pwsh` invocations**
- **Found during:** Task 1 verification
- **Issue:** This machine has Windows PowerShell (`powershell.exe`, PSEdition Desktop) but not PowerShell 7/Core (`pwsh`), which is what `windows-latest` GitHub Actions runners ship and what the harness's own doc-comment example assumed. The three `Test-Case` invocations inside `verify-fail-closed.ps1` called `pwsh` directly, which is not on PATH here.
- **Fix:** Added a `$ShellExe` resolution (`pwsh` if `$PSVersionTable.PSEdition -eq 'Core'`, else `powershell`) and used it in place of the hardcoded `pwsh` literal in all three test-case invocations, so the harness runs unmodified under both PowerShell 7 (CI) and Windows PowerShell 5.1 (this local environment).
- **Files modified:** `app/src-tauri/signing/verify-fail-closed.ps1`
- **Verification:** `powershell.exe -NoProfile -NonInteractive -Command "& 'app/src-tauri/signing/verify-fail-closed.ps1'"` -- all 5 checks (parser sanity, no-credential-names, no-argument, dlib-unset, dlib-missing-file) PASS.
- **Committed in:** `800226ff` (Task 1 commit; discovered and fixed before the commit was made, so no separate fix commit was needed)

**2. [Rule 3 - Blocking] `$ErrorActionPreference = 'Stop'` in the harness turned expected child-process stderr into a terminating exception**
- **Found during:** Task 1 verification (same debugging pass as deviation 1)
- **Issue:** `verify-fail-closed.ps1` deliberately drives `sign.ps1` into its Write-Error failure paths. Under Windows PowerShell with the harness's own `$ErrorActionPreference = 'Stop'` still in scope, any stderr text from the child `powershell -File` invocation was wrapped into a terminating `ErrorRecord` and thrown in the PARENT process -- even though the child's stream was redirected with `*>$null` -- aborting the harness before it could read `$LASTEXITCODE`.
- **Fix:** Each of the three `Test-Case` invocations now sets `$ErrorActionPreference = 'Continue'` locally around the child-process call, restoring the prior value immediately after, and relies on `$LASTEXITCODE` (which the child process sets correctly regardless of how PowerShell handles its stderr stream) rather than on an exception never being thrown.
- **Files modified:** `app/src-tauri/signing/verify-fail-closed.ps1`
- **Verification:** Same run as deviation 1 -- all cases now report their real exit codes and PASS.
- **Committed in:** `800226ff` (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 3 - blocking, both confined to the local verification harness, neither touches `sign.ps1` itself or any CI-facing file)
**Impact on plan:** Necessary to actually run the plan's own required verify command (`pwsh -NoProfile -File app/src-tauri/signing/verify-fail-closed.ps1`) in an environment without `pwsh`. `windows-latest` CI runners ship `pwsh`, so `$ShellExe` resolves to `'pwsh'` there and the harness's CI behavior is unchanged from what the plan specified.

## TDD Gate Compliance

- **Task 3** has NO separate RED commit. Tasks 1 and 2 (a tracer and an `auto` task) already implemented the entire signing script, metadata, and gated release workflow completely and correctly BEFORE Task 3's test file was written. When `signing_wiring.rs`'s 4 tests were written against that already-complete implementation, all 4 passed on the first run -- there was no failing state to commit separately from the passing one. This mirrors the exact, already-accepted precedent set by `11-01-SUMMARY.md`'s Task 3 (frontend theme tests written against a tracer's complete implementation).
- The fail-fast rule ("a test that passes unexpectedly during RED means the test may be wrong") does not apply here for the same reason it did not apply in 11-01: the reason for the pass is fully understood (tasks 1/2 built the real thing) and is independently confirmed by a genuine red/green demonstration rather than trusting the first green run alone.
- **The demonstration, performed and reverted with zero residual diff:**
  1. `committed_config_has_no_windows_sign_hook` -- temporarily added a `bundle.windows.signCommand` entry to `tauri.conf.json`, ran the single test, confirmed FAILED with the expected assertion message, reverted the file, confirmed `git diff --exit-code app/src-tauri/tauri.conf.json` clean, re-ran the full 4-test suite, confirmed all green.
  2. `release_workflow_gates_signing_steps` -- in turn, stripped the `if: vars.ENABLE_MSI_SIGNING == 'true'` condition from (a) the "Install Trusted Signing dlib" step, (b) the "Inject bundle.windows.signCommand" step, and (c) the `publish-release` job's own `if:` line, running the single test after each strip and confirming FAILED with the specific assertion message naming that exact step/job, then restoring the full original file from a backup and confirming `git diff --exit-code .github/workflows/release-app.yml` clean, then re-running the full 4-test suite green.
- All Rust tests pass (`cargo test --jobs 2`, full suite, 0 failed), `cargo clippy --jobs 2 --all-targets -- -D warnings` clean.

## Issues Encountered

None beyond the two auto-fixed local-environment deviations documented above. No pre-existing formatting/lint drift was touched (per the SCOPE BOUNDARY rule, same as 11-01).

## User Setup Required

**Yes -- this is the phase's documented, expected, non-blocking gap.** Azure Trusted Signing service-principal credentials for this repository are NOT YET PROVISIONED:

- Repository secrets `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`
- Repository variable `ENABLE_MSI_SIGNING` set to `true`

No code change is required to switch signing (and public Release publishing) on -- provisioning these four values is a pure operations action. Full procedure, including the required post-provisioning deliberate fail-closed check, is in `docs/signing.md`.

## Next Phase Readiness

- This was the final plan of Phase 11 (Platform Polish) and the final phase of the v1 milestone's plan queue -- ROADMAP Phase 11 now shows 2/2 plans executed.
- PLAT-02's literal ROADMAP text ("Windows release binaries are Authenticode-signed") remains genuinely unverifiable until an operator provisions the Azure credentials; `docs/signing.md` is the durable record of exactly what to do and how to prove it worked once that happens. This is not new-code work -- it is an operational follow-up outside this plan's execution scope.
- macOS notarization and a full macOS/Linux release-build leg for the Tauri app remain out of scope, consistent with 11-CONTEXT.md's explicit phase-boundary exclusions.

---
*Phase: 11-platform-polish*
*Completed: 2026-08-16*

## Self-Check: PASSED
All 7 created files verified present on disk; all 3 task commit hashes (`800226ff`, `f24c0298`, `5cee240f`) verified present in git log.
