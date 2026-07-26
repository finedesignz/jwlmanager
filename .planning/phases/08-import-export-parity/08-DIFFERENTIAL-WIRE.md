# 08-DIFFERENTIAL-WIRE: Real-Python oracle for `.txt` export byte-compatibility

## Goal

Phase 8's wire-format tests (`export_wireformat_tests.rs`) byte-compare Rust
export output against **hand-authored golden fixtures**, which prove only
self-consistency with our own reading of the format. This closes that gap:
run the real `JWLManager.py` export logic (PySide6 6.9.3 installed, Python
3.13.3, root-staged `jwlCore-amd64.dll`/`sqlite3_64.dll` all present on this
host — differential.rs's stated blockers do not apply here) and compare its
actual output to the Rust exporter's, byte for byte.

## Method

`export_items`'s per-category export logic (`JWLManager.py:1367-1668`) is a
set of nested closures inside `Window.export_items`, closing over
`self`/`con`/`items`/`form`/`current_archive`. Like `downgrade_schema`
(04-03, `PY_DOWNGRADE_SCHEMA` in `differential.rs`), it cannot be called
headlessly in isolation. Following that exact precedent, the closures'
logic was ported **verbatim** (line-for-line, same SQL, same string-building)
into a standalone stdlib-`sqlite3` script, seeded with the SAME synthetic
rows the Rust golden-fixture tests use (`export_wireformat_tests.rs`'s
`seed_*_golden_fixture_rows` functions), with the header's non-deterministic
fields (`current_archive`, `APP`, `VERSION`, `datetime.now()`) pinned to the
exact same values `ExportHeaderCtx` pins in the Rust tests
(`archive_name="MyArchive.jwlibrary"`, `app_version="0.1.0"`,
`timestamp="2026-01-01 @ 00:00:00"`) — isolating real format differences
from the timestamp, per the task brief.

Since the existing Rust wire-format tests already assert byte-exact equality
between the Rust exporter and `tests/fixtures/wire/*_golden.txt`, comparing
the Python replica's output to the SAME golden fixtures is equivalent to
comparing Rust vs. Python directly.

## Verdict per category

| Category | Verdict |
|---|---|
| Favorites | **BYTE-IDENTICAL** (content) — see line-ending note below |
| Bookmarks | **BYTE-IDENTICAL** (content) — see line-ending note below |
| Annotations | **BYTE-IDENTICAL** (content) — see line-ending note below |
| Highlights | **BYTE-IDENTICAL** (content) — see line-ending note below |
| Notes | **BYTE-IDENTICAL** (content) — see line-ending note below |

All five categories: every data field, field order, `None` sentinel,
`¦`-escaping, `{ISSUE}`/`{HEADING}`/`{RANGE}`/`{BLOCK}`/`{Reference}`
bracket logic, tag-pipe-joining, and `{END}` sentinel placement matched the
golden fixtures **exactly** after normalizing line endings (see below). No
content-level divergence was found in any category.

## The one real divergence found: line endings on Windows

`JWLManager.py`'s export closures open the output file with
`open(fname, 'w', encoding='utf-8')` — **no `newline=''`**. Python's default
text-mode write translates every `\n` in the string to `os.linesep`. On
Windows, `os.linesep == '\r\n'`, so **the real Python app, run on Windows,
writes CRLF line endings into every `.txt` export.**

The Rust exporter (`app/src-tauri/src/db/io/export.rs`) writes raw bytes via
`file.write_all(b"\n")` and `format!(...)` strings containing literal `\n` —
**always LF-only, on every platform**, never translated.

Verified directly on this host:
```python
>>> open('/tmp/t.txt','w',encoding='utf-8').write('a\nb')
>>> open('/tmp/t.txt','rb').read()
b'a\r\nb'
```

So: **on Windows**, a Python-exported `.txt` file and a Rust-exported
`.txt` file of the identical data are NOT byte-identical — they diverge
only in line-ending bytes (`\r\n` vs `\n`), never in content. On macOS/Linux,
`os.linesep == '\n'`, so Python's output would already be LF-only and
byte-identical to Rust's.

This is a real, confirmed platform-dependent divergence in Phase 8's
"byte-compatible" claim, not a fixture-authoring artifact — the hand-authored
golden fixtures were written LF-only (matching Rust's own always-LF
behavior), which is what Rust's own tests correctly assert, but it means
those goldens do NOT represent what the real Windows Python app emits.

**Assessment:** this is a genuine gap, not a bug to silently "fix" in either
implementation without a decision — flagging per the task brief rather than
papering over it. Two honest options: (a) accept CRLF-vs-LF as an
intentional, documented cross-platform normalization (Rust picks the
POSIX-portable LF unconditionally, which is arguably *more* correct/portable
than replicating Python's host-linesep quirk), or (b) match Python's
host-native behavior exactly (LF on macOS/Linux, CRLF on Windows) for
byte-for-byte parity with a same-host Python export. No existing project doc
records a decision either way for `.txt` exports specifically (unlike the
archive's internal SQLite bytes, which are documented as NOT
byte-preserving). Recommend surfacing this to the phase owner as a follow-up
decision; not fixed here per the constraint against modifying the export
implementation or the existing golden fixtures.

## Differential test added

Yes — `python_export_matches_rust_export_content` in
`app/src-tauri/tests/differential.rs`, `#[ignore]`d following the exact
convention of the other oracles in that file (RECORDED MANUAL GATE, requires
python3 + PySide6-free stdlib-only Python script + no DLLs needed since it
never touches `jwlcore`/PySide6, only stdlib `sqlite3`). It seeds the same
five golden-fixture datasets via `rusqlite`, calls the Rust `export_*`
functions, shells to the ported-verbatim Python replica script (embedded as
a string constant, mirroring `PY_DOWNGRADE_SCHEMA`), and asserts the two
outputs are equal after normalizing `\r\n` -> `\n` on both sides — documenting
the CRLF finding above in its doc comment so the normalization is never
mistaken for silently ignoring a real divergence.

Run explicitly with:
```
cd app/src-tauri && cargo test --jobs 2 --test differential -- --ignored python_export_matches_rust_export_content
```

STATUS: **VERIFIED PASSING** on 2026-07-26 (Windows x64, Python 3.13.3,
stdlib sqlite3 only — no PySide6/jwlCore dependency for this leg).

## Default suite

`cargo test --jobs 2` (non-ignored) confirmed still green after this change —
no existing test or fixture was modified.

## CRLF import interchange bug (found via this oracle, fixed 2026-07-26)

**Finding:** the oracle above established that `JWLManager.py` opens export
files with `open(fname, 'w', encoding='utf-8')` and no `newline=''`, so
Python's text-mode write translates `\n` -> `os.linesep` — on Windows the
real Python app writes **CRLF** into every `.txt` export. Chasing that
finding through the Rust *importer* (not just the exporter, which this
document otherwise covers) surfaced an asymmetric interchange failure:

- `parse_favorites_file`, the Bookmarks/Annotations line parser, and
  `parse_highlights_file` all incidentally tolerated CRLF (each strips a
  trailing `\r` per line before use — `trim_end_matches('\r')` /
  `trim_end()`).
- **`parse_notes_file` did not.** It locates each record's header/body
  boundary via `chunk.find("===\n")`; a CRLF file contains `===\r\n`, so the
  search failed and import aborted with `ImportMalformed { reason:
  "unterminated record header" }`. A Notes `.txt` file exported by the real
  Python app on Windows could not be re-imported by this app at all — a
  direct violation of Phase 8's bidirectional-interchange goal.
- Separately, `parse_annotations_file` uses the identical
  `chunk.find("===\n")` boundary search on bracket-tag records, so it carried
  the same latent defect even though it was not the file originally flagged.

**Fix:** added a single shared helper, `normalize_line_endings` (`app/src-
tauri/src/db/io/import.rs`), that converts `\r\n` -> `\n` (and a lone `\r` ->
`\n`, full universal-newlines) once on the WHOLE file text, applied at the
top of all five `parse_*_file` entry points (Favorites, Bookmarks,
Annotations, Highlights, Notes) — not just the one broken parser, so no
parser is left depending on incidental per-line `trim_end` behavior. This is
**parity, not a deviation**: Python's own reader
(`open(fname, encoding='utf-8')`, also no `newline=''`) applies universal-
newlines translation on READ, silently converting `\r\n`/`\r` back to `\n`
before any parsing logic runs, so Python never sees the `\r` at all —
reproducing that invisible read-time behavior in Rust is what makes the
round trip actually interchangeable. The existing per-line `\r` trims in the
other four parsers were left in place (now harmless belt-and-braces); the
export side is unchanged (Rust exporting LF unconditionally remains the
documented, separate decision — Python's reader accepts LF fine via the same
universal-newlines translation).

**Tests added** (`app/src-tauri/src/db/io/import.rs`, `mod tests`), all pure
`parse_*_file` unit tests — no DB fixture needed:
- `favorites_crlf_file_parses_identically_to_lf`
- `bookmarks_crlf_file_parses_identically_to_lf`
- `annotations_crlf_file_parses_identically_to_lf`
- `highlights_crlf_file_parses_identically_to_lf`
- `notes_crlf_file_parses_identically_to_lf` — the fixture that failed with
  `ImportMalformed { reason: "unterminated record header" }` before the fix
  (confirmed by inspection of the pre-fix `chunk.find("===\n")` boundary
  search against a `===\r\n` terminator)
- `notes_crlf_file_title_and_content_carry_no_stray_cr` — silent-corruption
  guard proving Title/Content contain no interior `\r` after a CRLF import,
  which a narrower fix limited to only the header-boundary match (without
  whole-file normalization) would NOT have caught, since `body.trim_end()`
  only strips the end of the whole body, not each interior line

**Verification:** `cargo test --jobs 2` (130 passed, `--lib` +
all integration suites, 0 failed), `cargo clippy --all-targets -- -D
warnings` (clean), `npx vitest run` (133 passed), `npx tsc --noEmit` (clean).
No existing fixture or test was modified; no new Cargo dependency added.
