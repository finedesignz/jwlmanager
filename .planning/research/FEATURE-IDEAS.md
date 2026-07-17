# JWLManager (Tauri rewrite) — Candidate Feature Ideas

*Research date: 2026-07-16. Scope: NEW features for a from-scratch Tauri (Rust core + web frontend) rewrite replacing the PySide6 app.*

## Evidence base

- **277 issues** across the full history of [`erykjj/JWLManager`](https://github.com/erykjj/jwlmanager) (all states, fetched via GitHub API). The repo does not use labels meaningfully (only 4 labels applied across 277 issues), so demand was ranked by **comment volume** — the only reliable engagement proxy available.
- `res/HELP.md` (upstream feature reference) and the local `.planning/codebase/{ARCHITECTURE,STRUCTURE,CONCERNS}.md`.
- Sibling project [`erykjj/jwlFusion`](https://github.com/erykjj/jwlFusion) — the CLI merge utility sharing the same vendored `jwlCore` native library (currently v0.32.1, schema v16 default, `--downgrade` to v14).

**Top threads by engagement** (all closed, but closure ≠ demand satisfied — several were closed as "won't do" or partially done):

| # | Title | Comments |
|---|---|---|
| 95 | Excel Export Feature | 68 |
| 180 | Exporting notes to markdown | 47 |
| 188 | Selective and **Incremental** Export for sync with external tools (Obsidian) | 29 |
| 194 | Enhanced Markdown Export with **Linked References** for Obsidian | 23 |
| 237 | Can't open v8 | 27 |
| 1 | **Unsecure Windows App** | 21 |
| 159 | Exporting to excel error | 21 |
| 277 | Could not find shared library jwlCore-arm64.dll | 19 |
| 254 | Feature request: change note color via UI | 11 |
| 177 | Sharing annotations with iOS devices | 11 |
| 231 | Links lost after merge | 12 |
| 196 | JW Library structured links in exported notes | 8 |
| 195 | Optional Selection of Properties for Markdown Export | 8 |
| 187 | Archive name not visible in dark mode | 8 |
| 153 | Screen size and resolution | 8 |

**The single loudest signal in the whole tracker is the export→external-knowledge-tool pipeline** (#95 + #180 + #188 + #194 + #195 + #196 = 173 comments combined, one clearly identifiable power-user cohort). **The second loudest is distribution trust** (#1, #287, #271, #237, #277, #258, #230 — SmartScreen/AV false positives, Gatekeeper quarantine, missing arch binaries). Both are directly addressable by the Tauri rewrite.

---

## Format & legal risk key

Used per-feature below:

- **Format dependency: LOW** — touches only `user_data.db` tables whose shape is already exercised by the current app.
- **Format dependency: MED** — needs stable interpretation of existing columns (e.g. `Location`, `BlockRange`, `UserMarkGuid`) across schema versions.
- **Format dependency: HIGH** — requires **reverse-engineering undocumented JW Library behavior**. Flagged explicitly.
- **Licensing/ToS: FLAG** — touches copyrighted publication content (not user-authored data). Called out inline.

> **Standing constraint for the whole rewrite:** the `.jwlibrary` backup format is undocumented and vendor-controlled. Every schema assumption is a maintenance liability (the current app already carries `upgrade_schema`/`downgrade_schema` for v14↔v16). **User-authored data (notes, tags, highlights, bookmarks) is the safe surface. Publication text is not.**

---

## TABLE STAKES — parity, not proposed

Noted for completeness only; these are re-implementation obligations, not new features. Anything shipped without these is a regression:

- Open/validate `.jwlibrary` (zip + `user_data.db` + manifest), save/re-zip, schema upgrade/downgrade (v14↔v16).
- Tree view of Annotations, Bookmarks, Favorites, Highlights, Notes, Playlists; grouping by title/type/language/color/tag.
- Merge archives (currently via the `jwlCore` native lib).
- Export/import: XLSX, custom `{NOTES=}`/`{ANNOTATIONS}`/`{BOOKMARKS}`/`{HIGHLIGHTS}` text format, markdown (one-way), `.jwlplaylist`.
- Bulk tag; bulk color change (Notes + Highlights); delete; **Clean** (strip invisible chars); **Mask** (obfuscate for sharing); **Sort** (restore natural note order); cleanup of orphan records.
- Data Viewer; drag-and-drop; dark mode; i18n (Weblate-driven); single-instance lockfile; `JWLManager.conf` state.

---

## HIGH-CONVICTION

### 1. Signed, auto-updating binaries

**Pitch:** Ship a code-signed, notarized, delta-auto-updating app so users stop being told their Bible study tool is malware.

**Problem:** The PyInstaller bundle trips SmartScreen and AV heuristics on every release. This is not a papercut — it is the app's single most repeated support burden and it directly costs trust with a non-technical, trust-sensitive user base.

**Evidence:** #1 "Unsecure Windows App" (21 comments, upstream had to write a whole `.github/SECURITY.md` to defend against it); #287 "Virus detected"; #271 `"JWLManager.app" is damaged...`; #237 "Can't open v8" (27 comments); README instructs users to run `xattr -cr JWLManager.app` and `chmod +x` — i.e. *teaching users to bypass OS security warnings*, which is exactly the habit that makes them phishable.

**Tauri unlock:** Yes, decisively. Tauri produces a native signed MSI/NSIS/`.app`/AppImage instead of a self-extracting Python interpreter (the actual heuristic trigger). `tauri-plugin-updater` gives signed delta updates, which PySide6+PyInstaller never had — today users manually re-download a ~100MB bundle per release. Bundle drops from PyInstaller-scale to single-digit MB.

**Complexity:** M (signing infra + CI, mostly one-time). **Format dependency:** NONE. **Risk:** Apple Developer cert needed for macOS notarization (annual cost); updater signing key must not leak.

---

### 2. Incremental / differential export

**Pitch:** Export only what changed since the last export, so a 9,000-note vault syncs in seconds instead of being rewritten wholesale.

**Problem:** Power users re-export their entire archive on every sync, clobbering an external vault and destroying anything downstream (Obsidian graph state, git history, file mtimes). Full re-export does not scale past a few thousand notes.

**Evidence:** **#188 (29 comments)** — the requester explicitly states *"With over 9000 notes, it has become increasingly difficult..."* and describes a real Obsidian-vault sync workflow. This is the highest-value unmet request in the tracker.

**Comparable tools:** Zotero's `Better BibTeX` auto-export-on-change; Anki's incremental sync; Obsidian's own file-level sync. All treat "export the delta, not the world" as baseline.

**Tauri unlock:** Partial — this is mostly core logic, but Rust makes hashing/diffing 9k notes cheap, and `tauri-plugin-fs` + a sidecar-free watcher makes safe incremental writes practical.

**Complexity:** M. **Format dependency:** MED — needs a stable per-note identity. `LastModified` exists but its update semantics are vendor-controlled; safest design is a **local content-hash manifest** (hash of note body+title+location) stored app-side, not a trust of any DB timestamp. **Risk:** Deleted-note detection is the hard part — a naive delta silently leaves tombstones in the user's vault. Needs an explicit reconcile mode.

---

### 3. Round-trip markdown (import markdown back into an archive)

**Pitch:** Make markdown export bidirectional — edit notes in your real editor, write them back to `.jwlibrary`.

**Problem:** Markdown export is **one-way today** (HELP.md: *"exported (but not imported) as separate markdown files"*). Users get their notes out into a good editor, edit them there, and then cannot get them back. The whole external-tool workflow is a dead end.

**Evidence:** #180 (47 comments) established markdown export; #188/#194/#195 are all the *same cohort* pushing toward a real two-way workflow. The XLSX path is already round-trippable — markdown being export-only is an arbitrary asymmetry users keep bumping into.

**Comparable tools:** Obsidian (files are the source of truth), Zotero note round-trip, Anki's CrowdAnki import/export.

**Tauri unlock:** Neutral (core logic), though a Rust markdown parser (`pulldown-cmark`) is faster and stricter than ad-hoc Python string handling.

**Complexity:** L. **Format dependency:** MED — must reconstruct valid `Location`/`UserMark` rows from frontmatter. **Risk:** **High blast radius.** This writes user-authored data back into an irreplaceable archive. Requires the existing `{NOTES=}` dedup discipline, a mandatory dry-run diff, and non-negotiable backup-before-write. Do not ship without the Dry-Run Diff (#5).

---

### 4. Full-text search across the archive

**Pitch:** Instant search over every note, annotation, tag, and highlight — with filters — instead of hunting a tree.

**Problem:** The only way to find anything today is to expand a grouping tree. Upstream's own README concedes *"the more items there are to be sorted into a tree structure, the longer it will take."* At 9k notes (#188) the tree is unusable as a finding aid.

**Evidence:** Partly **speculative as a filed request** — no single issue says "add search". But it's an inference from #188's scale complaint plus #153 (screen size/resolution) plus the README's own performance caveat. **Marked honestly: the *need* is evidenced, the *feature* was never explicitly requested.**

**Comparable tools:** Universal. Obsidian, Zotero, DEVONthink, and Anki all treat instant search as the primary navigation surface; a tree-only archive manager is a category outlier.

**Tauri unlock:** Yes. SQLite **FTS5 is already available** in the same SQLite the archive ships in — build a transient FTS index in the temp DB on load, discard on save so the user's archive is never polluted. Rust does this at a speed the current polars+QTreeWidget path cannot approach.

**Complexity:** M. **Format dependency:** LOW (read-only over known text columns). **Risk:** Low — must guarantee the FTS index is *never* written into the saved `.jwlibrary` (would produce an archive JW Library may reject).

---

### 5. Dry-run diff / preview before write

**Pitch:** Show exactly what a merge, import, or bulk operation will change — before it touches the archive.

**Problem:** Every destructive op today is faith-based. Merge conflicts resolve invisibly, imports silently create near-duplicates when a title changed "even slightly" (HELP.md), and Clean/Mask/Sort are documented as **"dangerous, destructive, one-way"** with the mitigation being *"keep a backup"*. That's a docs-shaped fix for a UX-shaped problem.

**Evidence:** #231 "Links lost after merge" (12 comments — user lost data and couldn't tell what happened or when); #198 "Overwriting and adding highlights from two different backups" (confusion about merge semantics); #290 "Unable to highlight after merge and restore"; #289 "Use of Duplicate note feature"; #186 "Error when importing highlights" (12). Repeated post-hoc "what did it do to my data?" threads are the signature of a missing preview.

**Comparable tools:** git's `--dry-run`/diff; Zotero's duplicate-merge preview pane; Anki's import preview showing added/updated/skipped counts.

**Tauri unlock:** Neutral in principle — but a webview renders a real side-by-side diff trivially, where Qt widgets made it painful enough that it never got built.

**Complexity:** M. **Format dependency:** LOW–MED. **Risk:** Low; this is a *risk-reducing* feature and a prerequisite for #3.

---

### 6. Structured JW Library deep links in exports

**Pitch:** Preserve working `jwpub://`/wol.jw.org links so an exported note can jump back to its source paragraph.

**Problem:** Exported notes lose their tether to the publication they came from, making an exported vault a pile of orphaned text.

**Evidence:** #196 "JW Library structured links in exported notes" (8); #194 "Enhanced Markdown Export with Linked References" (23); #195 "Optional Selection of Properties" (8) — users want to *choose* which properties get `[[wikilink]]`-wrapped rather than the current fixed behavior (HELP.md: only `document` is bracketed, and getting `color`/`language` linked requires the absurd workaround of *switching the Grouping before exporting*).

**Tauri unlock:** Partial — `tauri-plugin-deep-link` additionally lets JWLManager *register as a handler*, so links can point back into JWLManager itself, not just the web.

**Complexity:** S–M. **Format dependency:** MED — link construction from `Location` (BBCCCVVV / docId) is already done for the XLSX `Link` column, so the mapping exists; extending it is incremental.

**Licensing/ToS: FLAG (low).** Emitting a *URL* to jw.org/wol content is linking, not redistribution — that's fine. It becomes a problem only if it slides into caching the linked content (see Anti-Features).

---

### 7. Note color + tag editing in the main UI

**Pitch:** Change a note's color and tags inline, where you're already looking at it.

**Problem:** Bulk-only editing forces users into a selection→dialog dance for a one-note change.

**Evidence:** #254 "Feature request: change note color via UI" (11 comments) — a direct, explicit, unambiguous ask.

**Tauri unlock:** Yes, meaningfully. This is exactly the kind of interaction where a webview's inline editing/color-picker affordances are near-free, versus custom `QTreeWidgetItem` delegates. Related: #187 "Archive name not visible in dark mode" and #153 "screen size and resolution" are both Qt-styling/layout bugs that a CSS-based UI makes structurally less likely.

**Complexity:** S. **Format dependency:** LOW. **Risk:** Minimal.

---

### 8. First-class CLI (one binary, both faces)

**Pitch:** Ship the GUI and a scriptable CLI from the same Rust core.

**Problem:** Automation users currently need a *separate project* (`jwlFusion`) that merges but cannot export, tag, clean, or mask. Two codebases, two release cadences, one shared `jwlCore` — and `jwlFusion`'s changelog is mostly "updated jwlCore libs", i.e. pure integration overhead.

**Evidence:** The existence and active maintenance of `jwlFusion` **is** the evidence — someone built and maintains a whole CLI because JWLManager has none. Its README covers exactly the merge subset. #188's incremental-sync workflow is inherently a cron-job/scriptable use case.

**Comparable tools:** `anki-connect`, `zotero-cli`, `pandoc` — every mature archive tool has a scriptable face.

**Tauri unlock:** Yes. With a Rust core crate, the CLI is a second binary target over the same library — no FFI, no vendored `.dll`/`.so`/`.dylib` per platform, no `ctypes` bridge. **This also retires the entire `libs/` vendoring problem** (CONCERNS.md flags these as unreproducible binary blobs; #277 "Could not find shared library jwlCore-arm64.dll" is that debt failing in the field).

**Complexity:** M (L if it means porting `jwlCore`'s merge logic into the Rust core rather than continuing to FFI it). **Format dependency:** LOW. **Risk:** Porting merge logic risks regressing the most data-critical path in the app — and CONCERNS.md notes **there are zero automated tests today**. Gate this behind a fixture-based round-trip test suite.

---

## SPECULATIVE

### 9. Selective, shareable export bundle (`.jwlibrary` subset)

**Pitch:** Export a checked subset as a small valid `.jwlibrary` that anyone can restore directly on iOS/Android.

**Problem:** JW Library's restore **replaces** the entire database rather than merging (confirmed by the maintainer in #177). So sharing "just my notes on this one publication" with a friend is impossible without them nuking their own data — unless they also install JWLManager.

**Evidence:** #177 "Sharing annotations with iOS devices" (11 comments) — the maintainer's own answer is essentially *"that's why JWLManager exists"*, i.e. an acknowledged workaround, not a solution. Also #232 "Merging from laptop to ipad and iphone", #235 "JWPUB Convention Notebook".

**Why speculative:** It does not actually solve the problem it's aimed at. The recipient still needs JWLManager to merge the subset in, because the *app's* restore semantics are the real constraint and we cannot change those. It's a nicer bundle, not a fixed workflow. Ship only if the merge-on-receive UX is genuinely one-click.

**Complexity:** M. **Format dependency:** MED. **Risk:** Producing a subset archive that JW Library rejects or that silently wipes a recipient's data is a catastrophic-and-plausible failure mode.

### 10. Archive history / local versioned snapshots

**Pitch:** Auto-snapshot the archive before every destructive op; browse and roll back.

**Problem:** The documented safety net is literally *"do keep a backup until you're convinced that all is well"* (README) — the user is the version control system.

**Evidence:** Speculative as a request; inferred from the recurring damage threads (#231, #290, #279 "Lost publication", #280 "Missing notes"). No one asked for snapshots; several people asked for their data back.

**Comparable tools:** DEVONthink's versioned database; Zotero's automatic DB backups; Obsidian's File Recovery core plugin.

**Complexity:** M. **Format dependency:** LOW (opaque blob snapshots — no schema knowledge needed). **Risk:** Disk bloat (archives with playlist media can be large); needs retention limits and content-addressed dedup.

### 11. Statistics / study dashboard

**Pitch:** Highlight and note activity over time, by publication, by tag.

**Problem:** No aggregate view of one's own study data exists.

**Evidence:** **None. Fully speculative.** No issue requests this. Included only because the data is sitting right there and the webview makes charts cheap.

**Comparable tools:** Anki's review heatmap; Obsidian graph view; Zotero's library stats.

**Complexity:** S–M. **Format dependency:** LOW. **Risk:** Scope creep and vanity metrics. Cut first if the roadmap tightens. Note the tone constraint: this is devotional data, and a gamified "study streak" would read as tasteless to much of this user base.

### 12. Plugin / user-scriptable export templates

**Pitch:** Let users define their own export format via templates instead of filing an issue per variation.

**Problem:** Every export tweak is currently a maintainer round-trip.

**Evidence:** Inferred, moderately well: #195 (property selection), #194 (link format), #196 (structured links), #173 (playlist export language) are four separate issues that are all really *"I want the output shaped slightly differently."* A template engine collapses that class.

**Complexity:** M. **Format dependency:** LOW. **Risk:** A template DSL is a permanent API surface. Start with a sandboxed, data-in/text-out template engine (e.g. Tera/Handlebars) over a fixed context object — **never** arbitrary code execution, and never a plugin system with filesystem access.

---

## ANTI-FEATURES — deliberately NOT built

### A1. Bundling, caching, or rendering publication text

**Why it looks attractive:** Notes would be so much more useful shown next to the verse or paragraph they annotate. #235 ("JWPUB Convention Notebook") gestures at this.

**Why not:** **Licensing/ToS: FLAG (HIGH).** Publication content is copyrighted and is *not* the user's data. The entire legitimacy of this project rests on a clean line: **JWLManager touches only user-authored data inside a backup the user created.** Extracting/caching/redistributing publication text crosses into redistribution of copyrighted material and would reframe the project as a content pirate. Link out (#6); never inline. This line should be written into the project README as an explicit non-goal, not just held informally.

### A2. Cloud sync / hosted account

**Why it looks attractive:** "Sync my notes across devices" is the subtext of #177, #232, and #188.

**Why not:** Puts religious-affiliation data — among the most sensitive categories there is, and explicitly special-category data under GDPR Art. 9 — on someone's server. It creates a breach-liability surface and an operating cost for a free tool, and it invites the misreading that this is an official JW service. Local-first, file-based, zero-account. Users who want sync already have Obsidian/iCloud/Syncthing under their own control; #188's author had *already built exactly that* — he just needed a better export.

### A3. Writing directly to JW Library's live app database

**Why it looks attractive:** Skips the backup→edit→restore dance entirely.

**Why not:** **Format dependency: HIGH** — requires reverse-engineering undocumented, unsupported, per-platform internal storage (and on iOS it's not reachable at all without jailbreak). It would break on any app update, risks corrupting the user's live data with no backup by construction, and is far more plausibly read as tampering with the vendor's app than the current "operate on an export the user made" posture. The backup file is the *contract*. Stay behind it.

### A4. AI features (summarize notes, auto-tag, semantic search)

**Why it looks attractive:** It's 2026, and the notes are right there.

**Why not:** No one asked — zero of 277 issues. It means shipping either an API key (sending devotional notes to a third-party LLM — see A2's sensitivity argument) or a multi-GB local model into an app whose headline win is *"finally a small, signed, trusted binary."* An LLM confidently paraphrasing someone's religious study notes is a bad failure mode with no upside over FTS5 (#4), which solves the actual finding problem deterministically and instantly.

### A5. Mobile (iOS/Android) via Tauri v2

**Why it looks attractive:** Tauri v2 genuinely supports mobile, and #177/#232 are mobile-shaped complaints.

**Why not:** The mobile pain in those threads is *JW Library's restore-replaces-merge semantics* — which a JWLManager mobile app cannot change, because it can't reach JW Library's sandboxed DB (see A3). You'd ship a mobile app that can only operate on files the user manually shuttles in and out of a share sheet — worse than the desktop flow, for triple the platform surface (App Store review of a third-party app operating on a religious publisher's data files is its own adventure). Tauri *enables* this; that's not a reason to do it.

### A6. Telemetry / analytics

**Why it looks attractive:** CONCERNS.md correctly flags that 29 bare `except:` blocks make field failures invisible, so "we need visibility" is a real, correctly-diagnosed problem.

**Why not:** The fix is *structured local logging plus a user-initiated, user-reviewable crash report* — not a phone-home. Same sensitivity argument as A2: usage telemetry from an app tied to religious practice is a category of data that shouldn't leave the device. (The current app's optional crash-report POST is already the right shape — keep it opt-in and keep it reviewable.)

---

## Top 5 by conviction

| Rank | Feature | Why |
|------|---------|-----|
| **1** | **Signed, auto-updating binaries (#1)** | Highest evidence-to-effort ratio in the document. It's the app's most-repeated support burden (#1, #237, #271, #287, #277), it's ~pure infrastructure with **zero format dependency and zero data risk**, and it's the one thing the Tauri rewrite delivers almost as a side effect. It also removes the current README's genuinely harmful advice to disable OS security checks. Ship it first — it derisks every later release. |
| **2** | **Incremental export (#2)** | The clearest unmet, explicitly-articulated, at-scale user need in the tracker (#188, 29 comments, a named user with 9,000 notes and a real workflow). No comparable tool considers full-re-export acceptable. Bounded scope, and the local content-hash manifest design sidesteps trusting vendor timestamps. |
| **3** | **Dry-run diff / preview (#5)** | Ranked above the flashier round-trip because it is the **prerequisite** for safely shipping it, and because the repeated "what did the merge do to my data?" threads (#231, #198, #290, #186) show the current faith-based model already failing. It converts the README's "keep a backup ;-)" from a disclaimer into an actual guarantee. In an app whose entire job is mutating irreplaceable personal data, this is table stakes that the category simply never built. |
| **4** | **Full-text search (#4)** | The honest caveat: nobody filed this. But every comparable tool treats it as the primary navigation surface, upstream's own README concedes the tree doesn't scale, and FTS5 is *already in the SQLite the archive ships in*. Highest capability-per-line-of-code in the list, and it obviates the AI-search temptation (A4) entirely. |
| **5** | **First-class CLI / unified Rust core (#8)** | Conviction is high on the **architecture**, moderate on the feature. Collapsing JWLManager + jwlFusion onto one core retires the vendored-binary debt that CONCERNS.md flags and that #277 shows breaking in the field, and it's what makes #2's cron-driven sync real. Ranked 5th only because it's the largest, riskiest piece of work (porting merge logic with **zero existing test coverage**) — so it must be gated behind fixture-based round-trip tests, not attempted alongside them. |

**Deliberately just below the line:** Round-trip markdown (#3) is arguably the most *wanted* thing here, but it writes user-authored data back into irreplaceable archives and should not ship until #5 exists to make it safe. Sequence it immediately after the top 5, not within them.

## Risk flags summary

- **Reverse-engineering required:** #2 (note identity semantics — mitigated by app-side hashing), #3 and #9 (reconstructing valid rows), and A3 (rejected outright on these grounds).
- **Licensing/ToS:** A1 is the bright line — no publication text, ever. #6 is safe (links, not content) provided it never caches what it links to. Recommend an explicit non-goals section in the rewrite's README.
- **Data-loss blast radius:** #3, #8, #9 all mutate irreplaceable user data with **zero test coverage today** (CONCERNS.md). A fixture-based round-trip test suite is a hard prerequisite for all three.
- **Sensitive-data posture:** religious-affiliation data is GDPR Art. 9 special-category. It motivates rejecting A2, A4, and A6, and should be recorded as an architectural principle (local-first, no account, no phone-home), not re-litigated per feature.
