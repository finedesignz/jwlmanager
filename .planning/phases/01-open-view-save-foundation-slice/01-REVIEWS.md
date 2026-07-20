---
phase: 1
reviewers: [codex]
reviewed_at: 2026-07-19
plans_reviewed: [01-01-PLAN.md, 01-02-PLAN.md, 01-03-PLAN.md, 01-04-PLAN.md, 01-05-PLAN.md, 01-06-PLAN.md, 01-07-PLAN.md]
attempted_but_failed: [gemini]
gemini_failure: "gemini-cli auth error — ineligible/project-id (throwIneligibleOrProjectIdError). Not a plan defect."
overall_external_risk: HIGH
---

# Cross-AI Plan Review — Phase 1

## Codex Review

**Summary**
The plan set is strong on intent and risk awareness, but not yet safe to execute as written. The biggest gaps are around session state, save atomicity, fixture/schema fidelity, and verification commands that can falsely pass or fail. For a data-integrity tool, the plans still rely too much on "synthetic fixture opens in our new code" and not enough on "this archive remains valid to the two external consumers that matter" (JW Library + the Python app).

**Strengths**
- Clear wave split: scaffold, core open path, manifest/CI/jwlCore, real notes/save, then UI.
- Correctly treats Windows arm64 jwlCore absence as a first-class capability state, not a hidden failure.
- Good emphasis on synthetic fixtures and never committing real `.jwlibrary`.
- Zip-slip, SQL parameterization, typed errors, unwrap/expect bans explicitly called out.
- Phase scope respects deferred work (no merge, no trim, no downgrade, no other categories).
- Notes plan correctly identifies resources.db + independent notes as Phase 1 necessities.

**Concerns (external reviewer)**
- HIGH — No durable archive session model. `open_archive(path) -> Vec<NotesRow>` extracts to a temp dir, but save/save-as need the same working copy, manifest, DB path, media entries, source path, target path, and tempdir lifetime. If `TempDir` drops after `open_archive`, save is impossible or unsafe.
- HIGH — Phase 1 accepts schemas it cannot safely handle. Minimal check accepts `schemaVersion > 11`, but upgrade is deferred to Phase 3. Opening/saving v12–15 without upgrade risks invalid output. Phase 1 should accept v16 only, with a clear deferred-support error.
- HIGH — Save atomicity underspecified on Windows. `std::fs::rename` is not atomic-replace when the destination exists; a delete-then-rename fallback creates a corruption window.
- HIGH — Hand-built "minimum v16 schema" risks becoming a false oracle. New/save compatibility should seed from `res/blank` or exact schema extraction, not a hand-built minimal DB, unless proven against JW Library + the Python app.
- HIGH — RED test in 01-01 may break compilation. `cargo test fixtures_generate` compiles all integration tests; a RED test referencing a missing `open_archive` symbol can fail unrelated fixture tests before 01-07 lands.
- HIGH — Several verification commands invalid/weak. `cargo test a b c` is not a valid multi-filter; `grep -qv "error\["` can pass when errors exist; Windows CI shell conflicts with Unix `grep`/`test`.
- HIGH — CI may fail before frontend tests exist. 01-02 runs `npm test`, but vitest/test scripts/deps are not clearly established in 01-01.
- HIGH — Error serialization hand-waved. `thiserror` enums containing `io::Error`/`rusqlite::Error`/`libloading::Error` cannot simply derive `Serialize`; need a sanitized IPC error DTO.
- MEDIUM — `archive/manifest.rs` created in 01-02 but `archive/mod.rs` not listed for export/wrapping; strict validity path may not actually replace the 01-07 minimal check.
- MEDIUM — Zip-slip coverage misses variants: duplicate entries, Windows backslash traversal, symlink chain overwrite, absolute Windows paths, zip bombs/oversized archives.
- MEDIUM — Save tests do not prove loose media / unknown manifest entries survive. Rebuilding only `manifest.json`+`userData.db` would silently destroy media.
- MEDIUM — Python differential oracle too vague; launching `JWLManager.py <path>` may hang as a GUI or skip on missing PySide6, making ARCH-02 untested if skipped in CI.
- MEDIUM — jwlCore status shape inconsistent across plans (`Result<Status, Error>` vs `loaded=false + reason`); pick one, UI depends on it.
- LOW — `zip = ">=2.3"` is not a pin; permits future majors. Use a semver requirement + committed lockfile, or an exact version.
- LOW — UI virtualization assumes fixed 44px rows while tags/snippets may wrap; enforce no-wrap/truncation or the virtualizer mismeasures.

**Suggestions** (as given)
- Add a Phase 1 `ArchiveSession` state object before save work: owns `TempDir`, source path, current target path, DB path, manifest metadata, dirty flag, media-entry inventory, behind Tauri managed state.
- Change schema gate to `schemaVersion == 16` && `PRAGMA user_version == 16`; reject others with "schema upgrade arrives in Phase 3."
- Define platform-safe replacement explicitly: same-directory temp file, flush, atomic replace per OS, no delete-then-rename window.
- Base `new_archive()` on `res/blank` unless the generated schema is proven externally compatible.
- Make the RED test compile safely: ignored test / expected-failure harness / stub returning `NotImplemented` until 01-07.
- Replace filtered test commands with reliable ones (`cargo test --test manifest_tests` or full `cargo test`); set CI shells explicitly.
- Add frontend test tooling, `npm test`, vitest config, lockfiles, UI deps in 01-01 if 01-02 CI runs them.
- Use an IPC-safe error type `{ code, operation, safeFileName?, actionableMessageKey? }`; keep raw source errors internal.
- Add save-preservation tests for loose media, unknown manifest keys, zip entry duplicates, original archive byte hash after save-as.
- Make the Python oracle a real headless helper; if unavailable in CI, require a recorded local/manual gate before Phase 1 completes.
- Add explicit cancel/pending/double-click behavior for Open/Save/Save As.

**Overall Risk: HIGH** — right priorities, but the current version can still produce a misleading green build while missing the core guarantee: preserving and rewriting archives safely.

---

## Gemini Review

Not available — gemini-cli failed to authenticate (ineligible / project-id error). No plan defect; reviewer environment issue.

---

## Consensus Summary

Single external reviewer (codex) this pass; gemini unavailable. Because codex returned a HIGH-risk verdict with concrete, well-founded data-integrity findings on a tool whose Core Value is "never lose or corrupt an archive," the orchestrator elected to fold the actionable findings back through a planner `--reviews` revision **before** execution rather than proceed.

### Highest-priority concerns to action (orchestrator triage)
1. **ArchiveSession state model** (HIGH) — ACCEPT. Add a managed `ArchiveSession` owning TempDir + paths + manifest + media inventory + dirty flag; open returns a session handle, not a bare `Vec<NotesRow>`. Spans 01-07 (open) and 01-05 (save).
2. **v16-only schema gate for Phase 1** (HIGH) — ACCEPT. Accept `schemaVersion == 16` only; reject others with "schema support arrives in Phase 3." Keeps Phase 1 honest; upgrade is genuinely Phase 3 (SCHEMA-01/02).
3. **Windows atomic replace** (HIGH) — ACCEPT. Specify `ReplaceFileW`/`std::fs` semantics per-OS with no delete-then-rename window. Direct Core-Value risk.
4. **Loose-media / unknown-entry preservation on save** (HIGH-effective) — ACCEPT. Save must rebuild the FULL zip (all original entries), not just manifest+db; add a preservation test. Silent media loss is exactly the corruption Core Value forbids.
5. **Seed new/fixtures from `res/blank`** (HIGH) — ACCEPT. Use the real JW Library empty-archive seed as the generation basis, not a hand-built minimal DB. Directly answers the roast council's "circular spec / false oracle" finding.
6. **IPC-safe error DTO** (HIGH) — ACCEPT. thiserror internal + a `Serialize`-able sanitized DTO across the boundary. Required for SAFE-05 to actually compile and not leak paths.
7. **RED test must not break the test crate compile** (HIGH) — ACCEPT. Use `#[ignore]`/stub until 01-07.
8. **Valid, portable verification commands** (HIGH) — ACCEPT. No invalid multi-filter `cargo test a b c`; no weak `grep -qv`; CI shell set explicitly per-runner.
9. **Frontend test tooling exists before CI runs npm test** (HIGH) — ACCEPT. 01-01 establishes vitest + deps + lockfile.
10. **Python oracle headless-or-gated** (MEDIUM) — ACCEPT. Make it a real headless check or an explicit recorded manual gate; never a silent skip that fakes ARCH-02.
11. **Extra zip-slip variants** (MEDIUM) — ACCEPT. Add backslash traversal, absolute Windows paths, duplicate entries, symlink chains; note zip-bomb as a bounded check.
12. **jwlCore status shape unified** (MEDIUM) — ACCEPT. One shape: `Result<JwlCoreStatus, _>` where `JwlCoreStatus { loaded: bool, reason: Option<...> }`.
13. **Exact zip pin + committed lockfile** (LOW) — ACCEPT. Pin `zip` and commit `Cargo.lock`.
14. **Row-height/wrap discipline** (LOW) — ACCEPT. Enforce single-line truncation for the fixed-height virtual row.
15. **Open/Save cancel/pending/double-click** (LOW) — ACCEPT into 01-06.

None rejected — all findings are legitimate for this tool. Folded into a `--reviews` planner revision.
</content>
