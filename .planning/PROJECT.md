# JWL Manager (Tauri)

## What This Is

A cross-platform desktop app for managing `.jwlibrary` backup archives from JW Library — viewing, editing, exporting, importing, and merging personal study data (notes, highlights, bookmarks, tags, annotations, playlists, favorites). This is a from-scratch Tauri rewrite (Rust core + web frontend) replacing the existing PySide6 Python app, built to reach parity slice by slice and then go beyond it.

## Core Value

**Never lose or corrupt a user's archive.** These are years of irreplaceable personal study notes. If everything else fails, the data must survive intact.

## Requirements

### Validated

<!-- Shipped and confirmed valuable — inherited from the working Python app (v12.1.0), field-proven across 277 upstream issues and years of real use. These are proven-valuable capabilities the rewrite must re-earn, NOT proven-implemented in the new app. -->

- ✓ Open/save/save-as `.jwlibrary` archives — existing (Python)
- ✓ Merge two archives via `jwlCore` native lib — existing (Python); confirmed in real use by project owner
- ✓ View/edit/delete across 6 categories (Notes, Highlights, Bookmarks, Annotations, Favorites, Playlists) — existing
- ✓ Per-category import + export — existing
- ✓ Schema upgrade (v12–16) and optional v14 downgrade — existing
- ✓ Tagging, coloring, cleaning, masking, reordering — existing
- ✓ Localization via gettext — existing

### Active

<!-- Current scope. Building toward these. -->

**Parity (the rewrite must re-earn these):**
- [ ] Read/write the `.jwlibrary` archive envelope faithfully (zip + `manifest.json` + `userData.db`)
- [ ] Bind `jwlCore` from Rust via `libloading` for merge (do not reimplement merge)
- [ ] Reproduce the 7-table LocationId remap closure with explicit, tested ordering semantics
- [ ] Reproduce `trim_db` on save (orphan sweep, tag re-densify, VACUUM)
- [ ] Schema upgrade/downgrade with the v16↔v14 delta
- [ ] All 6 categories: view, edit, delete, import, export
- [ ] Localization

**New value (why the rewrite is worth doing):**
- [ ] Dry-run diff/preview before destructive operations ("will add 412, overwrite 6, delete 0" + cancel)
- [ ] Incremental export (export only what changed since last export)
- [ ] Signed binaries (Azure Trusted Signing — Titanium Labs LLC)
- [ ] Automated test suite (fixture archives, round-trip assertions) — built per-phase, not retrofitted
- [ ] Stable category enums (replacing translated-string control-flow keys)
- [ ] N-way merge fold (absorbed from jwlFusion's approach, not its code)
- [ ] Arch-aware native lib loading (fixes the arm64 selection bug)

### Out of Scope

- **Bundling or caching publication text** — copyrighted content, not user data. Bright line: crossing it reframes what this project legally is.
- **Cloud sync / AI features / telemetry** — religious-affiliation data is GDPR Art. 9 special-category. Single principle, decided once, not re-argued per feature.
- **Writing to JW Library's live database** — requires reverse-engineering undocumented vendor behavior; unacceptable risk to user data.
- **Merging sibling erykjj repos' code** — 5 of 7 publish no source; jwlFusion is Infiniti Noncommercial vs this project's MIT. Capability reuse via the MIT `jwlcore` path only.
- **Reimplementing merge logic** — `jwlCore` is the sanctioned prebuilt engine; both this app and jwlFusion already bind the byte-identical binaries.
- **Auto-update (deferred, not rejected)** — target users want a frozen known-good build for infrequent, high-stakes use. Revisit only with explicit opt-in.

## Context

- **Replaces:** `JWLManager.py` (~4077-line PySide6 monolith) + `jwlcore.py` (ctypes bridge). The Python app works and is in active real-world use by the project owner (Windows arm64 v12.1.0 build, used repeatedly for merges).
- **Domain knowledge is documented:** `.planning/research/FUNCTIONALITY-SPEC.md` (700 lines, line-cited) captures the archive format contract, ~26 operations, and non-obvious business rules. `.planning/codebase/` holds the map of the existing app.
- **Merge is a prebuilt dependency, not a port.** `jwlCore` binaries in `libs/` are hash-identical to those in sibling repo `jwlFusion`. Rust binds them via `libloading`.
- **Adversarial review completed.** A 5-persona council returned RESHAPE (Contrarian 2 · Expansionist 8 · Logician 2 · Researcher 3 · Buyer 2), recommending Python-first. Project owner reviewed and overrode in favor of the Tauri rewrite, accepting the risk knowingly, with "test as we go" replacing a retrofitted characterization harness. Council findings are retained in `.planning/research/` as standing risk context, not as a blocked decision.
- **Known upstream demand:** signing/Gatekeeper friction (#1 by support volume), incremental export (#188, 29 comments), dry-run preview ("what did the merge do to my data?" — #231, #198, #290, #186).

## Constraints

- **Tech stack**: Tauri v2 (Rust core + web frontend) — replaces PySide6. Rust binds `jwlCore` via `libloading`.
- **Compatibility**: Must read and write archives interchangeably with the existing Python app and JW Library itself. Format warts are load-bearing and must be preserved (`'None'` null sentinel, `|`→`¦` escaping, `==={END}===` parser sentinel, compact manifest JSON separators).
- **Data safety**: Save is not byte-preserving (`trim_db` + VACUUM). Parity must be verified semantically (normalized table state), never by byte-diffing outputs.
- **Security**: Fix zip-slip (`ZipFile.extractall` equivalent must validate paths). No f-string/format-string SQL interpolation — parameterize.
- **Licensing**: MIT. Do not ingest Infiniti Noncommercial (jwlFusion) or unlicensed (`NOASSERTION`) sibling code.
- **Platform**: Windows (incl. arm64), macOS, Linux. Linux WebKitGTK has documented performance issues with DOM-heavy grids — virtualize any large list.
- **Bandwidth**: Solo/hobby scale. Vertical MVP slices so every phase ships working value.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Tauri rewrite over incremental Python fixes | Owner's call, made against a RESHAPE verdict with full knowledge of the risk. Long-term maintainability + signed/small binaries + a clean core outweigh the parity slog for this owner. | — Pending |
| Bind `jwlCore` via `libloading`; never reimplement merge | The hardest logic is a prebuilt binary both apps already share (hash-verified). Removes the single largest rewrite risk. | — Pending |
| Test as we go, per-phase, in the new app | Owner declined a retrofitted characterization harness over the Python app. Tests are written alongside each Tauri slice instead. | — Pending |
| Vertical MVP slices | Every phase ships an end-to-end working capability — avoids the "months of no value" failure mode the council flagged. | — Pending |
| Dry-run diff prioritized early | Highest-conviction user ask; directly serves Core Value (data safety) and de-risks every destructive operation that follows. | — Pending |
| Absorb jwlFusion capability, not code | License conflict (Infiniti Noncommercial vs MIT) blocks code reuse; the MIT `jwlcore` path reaches the identical engine. | — Pending |
| Stable enums replace translated category strings | Existing app uses `if category == _('Notes')` — a latent i18n bug. Rewrite fixes it at the root. | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-07-16 after initialization*
