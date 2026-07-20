---
phase: 01-open-view-save-foundation-slice
plan: 02
subsystem: archive-core, ci
tags: [rust, serde, sha2, zip, security, ci, github-actions]

requires:
  - phase: 01-01
    provides: app/ scaffold, Cargo.toml/Cargo.lock, 6-variant zip-slip fixture generator, package-lock.json/vitest config
  - phase: 01-07
    provides: ArchiveError enum, extract::extract_zip_slip_safe, archive/mod.rs v16-only skeleton gate
provides:
  - Byte-compatible Manifest/UserDataBackup structs + compact serializer
  - compute_hash (sha256, hash-last discipline documented)
  - manifest::check_validity (v16-only, typed ArchiveError rejections)
  - 6-variant zip-slip rejection test proving archive::extract's containment property
  - .github/workflows/app-ci.yml four-leg matrix (windows-latest, windows-11-arm, ubuntu-latest, macos-latest)
  - app/src-tauri/clippy.toml
affects: [01-03, 01-04, 01-05]

tech-stack:
  added: []
  patterns:
    - "Ordered struct + #[serde(flatten)] catch-all map (serde_json preserve_order feature) for byte-compat manifest with unknown-key forward-compat"
    - "Two-pass validity check: probe as serde_json::Value for a specific missing-key error, then strict struct parse for type-confusion rejection"
    - "job-level defaults.run.shell: bash for cross-platform CI portability"

key-files:
  created:
    - app/src-tauri/src/archive/manifest.rs
    - app/src-tauri/tests/manifest_tests.rs
    - app/src-tauri/tests/archive_tests.rs
    - app/src-tauri/clippy.toml
    - .github/workflows/app-ci.yml
  modified:
    - app/src-tauri/src/archive/mod.rs
    - app/src-tauri/Cargo.toml
    - app/src-tauri/Cargo.lock

key-decisions:
  - "serde_json preserve_order feature added to Cargo.toml (Rule 3, blocking): #[serde(flatten)] catch-all maps need it to round-trip unknown keys in read order rather than re-sorted alphabetically"
  - "check_validity operates on raw manifest.json bytes only (not a zip-level open) -- file-level checks (NotAZip/MissingManifest) stay 01-07's archive/mod.rs responsibility; manifest.rs owns manifest-content validation only, avoiding any duplication of 01-07's gate"
  - "Test-verified zip 8.6.0's actual enclosed_name() behavior (path.rs) diverges from the plan's blanket 'all six variants literally error' assumption: absolute-path entries are deliberately stripped-and-contained (not rejected) 'similar to other ZIP tools', and this extractor never creates real filesystem symlinks from entries -- so only the two traversal variants (../, ..\\..\\) produce a typed ZipSlipRejected error. The security PROPERTY under test (nothing escapes the extraction root) holds for all six and is asserted unconditionally; the literal-rejection assumption for the other four was corrected in the test rather than the extractor"
  - "windows-11-arm CI leg only requires build+test green in this plan -- jwlCore-specific typed no-binary status is 01-03's concern, not asserted here"

metrics:
  duration: "~45 minutes"
  completed: "2026-07-19"
---

# Phase 1 Plan 2: Byte-Compatible Manifest, Zip-Slip Test Suite, Four-Leg CI Summary

Hardened the archive envelope 01-07 opened: a byte-compatible `manifest.json` (ordered struct, compact separators, hash-last, v16-only gate), a 6-variant zip-slip rejection test proving 01-07's extractor safely contains every malicious fixture, and a four-platform CI matrix (build + fmt + clippy + test) with Windows-safe shell selection.

## What Was Built

**Task 1 — `archive/manifest.rs`:** Ordered `Manifest`/`UserDataBackup` structs (never `HashMap`/`Value`) reproduce Python's exact field order via `serde_json::to_string`'s inherently compact form (no `separators=(',',':' )` config needed — `to_string` never adds whitespace). `#[serde(flatten)]` catch-all maps (backed by `serde_json`'s `preserve_order` feature, added to `Cargo.toml`) preserve unknown top-level and `userDataBackup` keys read-to-write. `compute_hash(db_path)` sha256-hashes the final on-disk DB bytes, documented as the last DB-touching step (mirrors `JWLManager.py:1162-1168`). `check_validity(bytes)` strictly parses (a JSON-string `schemaVersion` is a parse error, not a coercion) then applies the v16-ONLY gate — reusing `ArchiveError::{MissingUserDataBackup, UnsupportedSchema}` with zero new enum variants, keeping 01-07's gate intact rather than loosening it to `> 11` (the legacy Python behavior, correctly NOT followed here per this plan's explicit non-negotiable).

**Task 2 — `tests/archive_tests.rs::zip_slip_rejected`:** Drives all 6 of 01-01's `generate_zip_slip_fixture` variants against 01-07's `extract_zip_slip_safe` (no changes to `extract.rs`). Verified against zip `8.6.0`'s actual `enclosed_name()` implementation that only the two traversal variants (`../`, `..\..\`) literally return `Err(ArchiveError::ZipSlipRejected)` — the crate deliberately strips-and-contains absolute-path entries "similar to other ZIP tools" rather than rejecting them, and this extractor already never materializes a real filesystem symlink from an entry (closing the CVE-2025-29787 symlink class independently of name rejection). The test asserts the actual security property — **nothing escapes the extraction root** — unconditionally for all six variants via a before/after parent-directory snapshot diff, plus an explicit symlink-metadata check that the symlink-mode entry never becomes a real symlink. A separate test documents the zip-bomb/oversized-entry guard as a named, forward-looking gap (no decompressed-size cap exists yet).

**Task 3 — `.github/workflows/app-ci.yml`:** Four-leg matrix (`windows-latest`, `windows-11-arm`, `ubuntu-latest`, `macos-latest`), each running `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test` → `npm ci` → `npm run build` → `npm test`. Job-level `defaults.run.shell: bash` makes every step portable across PowerShell-default Windows runners. Linux leg installs `libwebkit2gtk-4.1-dev` + Tauri build deps. No signing steps (Phase 11). `app/src-tauri/clippy.toml` added as a home for future `disallowed-methods`-style config (the actual unwrap/expect ban already lives as a crate attribute in `lib.rs`, verified still intact).

## Verification Evidence

- `cargo test --test manifest_tests` — 6/6 pass (byte-exact serialization, unknown-key round-trip, type-confused schemaVersion rejected, v14 rejected/v16 accepted, missing-userDataBackup rejected, sha256 test vector).
- `cargo test --test archive_tests` — 2/2 pass (all 6 zip-slip variants: containment property holds; zip-bomb gap documented).
- `cargo test` (full suite) — all binaries green, 0 failed, 0 ignored regressions.
- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — zero warnings.
- `.github/workflows/app-ci.yml` exists; contains `windows-11-arm`, `clippy --all-targets -- -D warnings`, `shell: bash`; parses as valid YAML (`python3 -c "import yaml; yaml.safe_load(...)"` → `YAML_OK`).

**Not run:** actually pushing the branch and confirming all four GitHub Actions legs go green (the plan's `<human-check>`) — this executor has no network access to trigger a live Actions run. `windows-11-arm` free-tier availability additionally assumes this repo is public; visibility was not independently confirmed in this session (no `gh` CLI available). The owner should push and watch the Actions run once, and correct `runs-on: windows-11-arm` to a fallback if the repo turns out to be private.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `serde_json` needed the `preserve_order` feature for flatten catch-all maps**
- **Found during:** Task 1, initial `cargo build` after adding `#[serde(flatten)] extra: serde_json::Map<...>` fields.
- **Issue:** `serde_json::Map` defaults to a `BTreeMap` backing (alphabetically re-sorted) unless the `preserve_order` feature is enabled, which would silently reorder unknown keys rather than preserving their read order.
- **Fix:** Added `features = ["preserve_order"]` to the `serde_json` dependency in `Cargo.toml`.
- **Files modified:** `app/src-tauri/Cargo.toml`, `app/src-tauri/Cargo.lock`
- **Commit:** `70921421`

**2. [Rule 1 - Bug] `Sha256::digest`'s `GenericArray` doesn't implement `LowerHex`**
- **Found during:** Task 1, `cargo test --test manifest_tests`.
- **Issue:** `format!("{digest:x}")` on the raw `sha2::Sha256::digest` output failed to compile (`LowerHex` not implemented for the underlying `generic-array` type without an extra crate feature).
- **Fix:** Replaced with a manual byte-to-hex fold (`digest.iter().map(|b| format!("{b:02x}")).collect()`), avoiding an extra dependency/feature for one call site.
- **Files modified:** `app/src-tauri/src/archive/manifest.rs`
- **Commit:** `70921421`

**3. [Rule 1 - Bug] Test assumption error: plan's "all six zip-slip variants literally error" did not match zip `8.6.0`'s real `enclosed_name()` behavior**
- **Found during:** Task 2, first `cargo test --test archive_tests` run (`AbsoluteUnix must be rejected ... got Ok`).
- **Issue:** The plan's acceptance criteria assumed every one of the 6 crafted variants returns `Err(ArchiveError::ZipSlipRejected)`. Reading zip `8.6.0`'s `src/path.rs` showed `enclosed_name()` deliberately strips a leading absolute-path root/prefix and returns a CONTAINED relative path ("similar to other ZIP tools") rather than `None`/rejecting — so absolute-unix, absolute-Windows, duplicate-entry, and symlink-chain all extract successfully but stay confined under `dest`; only the two traversal variants underflow the depth counter and error.
- **Fix:** Corrected the test's per-variant expectation (`Err` for the 2 traversal variants only; `Ok` + containment-assert for the other 4) rather than touching `extract.rs` (out of scope per the plan's explicit partition with 01-07) — the security property under test (nothing escapes `dest`, no real symlink materialized) is asserted unconditionally for all six regardless of branch.
- **Files modified:** `app/src-tauri/tests/archive_tests.rs`
- **Commit:** `e346b0db`

## Known Stubs

None — this plan added only tests, a hardening module, and CI config; no UI-facing stubs introduced.

## Threat Flags

None — all new surface (manifest parsing, zip-slip test coverage, CI) was already registered in this plan's own `<threat_model>` (T-02-01 through T-02-SC).

## Self-Check: PASSED

- `app/src-tauri/src/archive/manifest.rs` — FOUND
- `app/src-tauri/tests/manifest_tests.rs` — FOUND
- `app/src-tauri/tests/archive_tests.rs` — FOUND
- `app/src-tauri/clippy.toml` — FOUND
- `.github/workflows/app-ci.yml` — FOUND
- Commit `70921421` (feat: manifest.rs) — FOUND in `git log`
- Commit `e346b0db` (test: archive_tests.rs) — FOUND in `git log`
- Commit `5aa2497a` (ci: app-ci.yml) — FOUND in `git log`
