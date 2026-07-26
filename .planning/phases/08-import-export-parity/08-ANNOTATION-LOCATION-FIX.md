# Annotation Location CHECK defect — investigation and fix

**Found during:** Phase 9 (incremental export) test-fixture work; the executor had to seed a
`DocumentId` on its Annotations fixture to work around this and correctly flagged it as
out of scope rather than silently patching around it.

**Pre-existing since:** Phase 8 (`find_or_insert_annotation_location`, `app/src-tauri/src/db/io/import.rs`).

## The defect

`find_or_insert_annotation_location` inserts a new `Location` row for an imported
annotation without ever setting `Track`:

```sql
INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, Type)
VALUES (?1, ?2, ?3, ?4, NULL, 0)
```

(and identically in the autoincrement branch just below it). The `Location` table's
`Type = 0` CHECK constraint (`CREATE_LOCATION_NEW`, `app/src-tauri/src/archive/upgrade.rs:47-70`,
a byte-exact port of `JWLManager.py:1026-1062`) requires ONE of:

- `DocumentId IS NOT NULL AND DocumentId != 0`, or
- `Track IS NOT NULL` AND (non-empty `KeySymbol` OR non-null/non-zero `DocumentId`), or
- a `BookNumber`/`ChapterNumber`-based scripture branch.

Since this INSERT never sets `Track`, `BookNumber`, or `ChapterNumber`, a **new** `Location`
row can only ever satisfy the CHECK when `DocumentId` is present and non-zero. Any Annotations
record with `{DOC=None}` whose `Location` does not already exist causes a raw
`SqliteFailure(ConstraintViolation, ...)` to surface instead of the intended import failure
path.

## What the Python oracle actually does (the authority)

`JWLManager.py:1909-1919`'s `add_location` (nested inside `import_annotations`) is:

```python
def add_location(attribs):
    existing_id = con.execute(
        'SELECT LocationId FROM Location WHERE DocumentId = ? AND IssueTagNumber = ? '
        'AND KeySymbol = ? AND MepsLanguage IS NULL AND Type = 0;',
        (attribs['DOC'], attribs['ISSUE'], attribs['PUB'])
    ).fetchone()
    if existing_id:
        location_id = existing_id[0]
    else:
        if available_ids.get('Location'):
            location_id = available_ids['Location'].pop()
            con.execute(
                'INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, '
                'MepsLanguage, Type) VALUES (?, ?, ?, ?, NULL, 0);',
                (location_id, attribs['DOC'], attribs['ISSUE'], attribs['PUB'])
            )
        else:
            location_id = con.execute(
                'INSERT INTO Location (DocumentId, IssueTagNumber, KeySymbol, MepsLanguage, '
                'Type) VALUES (?, ?, ?, NULL, 0);',
                (attribs['DOC'], attribs['ISSUE'], attribs['PUB'])
            ).lastrowid
    return location_id
```

**This is identical to the Rust port — Python never sets `Track` either.** The CHECK
constraint literal (`CREATE_LOCATION_NEW`) is Python's own DDL. So a `{DOC=None}` Annotations
record whose `Location` doesn't already exist would ALSO raise `sqlite3.IntegrityError` in
Python — caught by the bare `except:` at `JWLManager.py:1931`, surfaced to the user as a
generic `"Error on import!"` dialog, and rolled back (`con.execute('ROLLBACK;')`).

A second consequence, also present in Python: the existing-row `SELECT` binds
`DocumentId = ?` with `attribs['DOC']`. When `DOC` is `None`, SQL `DocumentId = NULL` is
never true (even against a genuinely-NULL stored value) — so the existing-Location lookup
can **never** find a match for a DOC-less record, even if one already exists (e.g. a
scripture-shaped `Type=0` Location created via Bookmarks/Highlights import, which legitimately
has `DocumentId` NULL and `BookNumber`/`ChapterNumber`/`KeySymbol` set instead). Every
DOC-less Annotations record therefore always falls through to the INSERT branch, and always
hits the same CHECK violation.

**Conclusion: importing a `{DOC=None}` Annotations record is impossible in JWLManager.py
itself, not just in this Rust port.** This is a genuine, permanent oracle limitation — not
something this app should "route around" with a different insert shape, since doing so would
produce archives Python cannot reproduce/accept (`Track`, `BookNumber`, `ChapterNumber` have
no wire representation for Annotations at all — the `.txt` format only carries `PUB`/`ISSUE`/
`DOC`/`LABEL`/`VALUE`).

## The fix

`find_or_insert_annotation_location` now checks, immediately after the existing-Location
lookup misses and before either INSERT branch, whether the record's `DOC` is present and
non-zero. If not, it returns a typed `ArchiveError::ImportFailed { reason }` naming the
record's `PUB`/`LABEL` and explaining that a new `Location` cannot satisfy the CHECK without a
`DocumentId` — matching Python's own failure on this input, but as a typed, explanatory error
instead of a raw constraint violation reaching the caller.

Applied identically to **both** the recycled-id branch and the autoincrement branch (they
shared the omission).

The existing-lookup `SELECT`'s predicate and column list were left unchanged — no `Track`
term was added, since `Track` still isn't part of what this INSERT ever writes; changing the
`SELECT` without changing the `INSERT` would only create a mismatch, and changing the `INSERT`
to write `Track` would diverge from the oracle's own (also-broken) behavior.

## Tests added

Both in `app/src-tauri/tests/import_wireformat_tests.rs`:

1. `annotation_without_doc_and_no_existing_location_rejected_with_typed_error` — imports a
   `{DOC=None}` Annotations record into an archive with no matching `Location`, asserts the
   import returns `ArchiveError::ImportFailed` (not a raw SQLite error), asserts the reason
   names `DOC`, and asserts no `Location`/`InputField` row was created after rollback.
2. `doc_less_annotation_export_is_unchanged_by_a_rejected_reimport` — seeds a legitimate
   scripture-shaped `Type=0` Location (`BookNumber`/`ChapterNumber`/`KeySymbol` set,
   `DocumentId` NULL) directly (not via import), exports it (confirms it round-trips through
   export as `{DOC=None}`), re-imports that exact exported text (confirms the re-import is
   rejected with the typed error, per the oracle-parity conclusion above), then exports again
   and asserts the two export outputs are byte-identical — proving the rejected import left the
   database untouched.

## Phase 9 fixture workaround — still load-bearing, unrelated to this fix

`app/src-tauri/tests/incremental_export_tests.rs`'s `seed_one_annotation` seeds its Annotation
`Location` with a concrete `DocumentId = 1001` (not NULL) specifically so that
`annotations_incremental_converges`'s **re-import** step can find the SAME `LocationId` again
via the existing-row `SELECT` (which, as shown above, can never match a NULL `DocumentId`).
This fix does not touch that `SELECT`, so **the workaround remains necessary and was not
removed** — it exists to make the existing-Location lookup succeed for re-import
convergence testing, a different concern from the CHECK-violation defect this fix addresses.

## Verification

- `cargo test --jobs 2` (mandatory `--jobs 2` — default parallelism OOMs the linker on this
  host, an environment limit unrelated to code). All new/existing tests in `import.rs` and
  `import_wireformat_tests.rs` pass. Two unrelated pre-existing failures were observed in
  `incremental_export_tests.rs`
  (`bookmarks_invariant_identity_collision_and_new_record_all_exported`,
  `highlights_invariant_identity_collision_and_new_record_all_exported`) — confirmed via
  `git status` to originate from another agent's concurrent, uncommitted edits to that shared
  test file (Phase 9 wave 4 `diff.rs`/`export.rs` work), not from this change. This file
  (`import.rs`) and its own test file were not touched by that concurrent work.
- `cargo clippy --all-targets -- -D warnings` — clean (only pre-existing `ts-rs` macro
  attribute-parsing warnings, not `-D warnings`-triggered failures).
- `npx vitest run` — 143/143 passed, 13/13 files.
- `npx tsc --noEmit` — clean, no output.

## Constraints honored

- No new Cargo dependency.
- Typed error (`ArchiveError::ImportFailed`), no `unwrap`/`panic`.
- All SQL parameterized (unchanged from before).
- Synthetic fixtures only.
- Edits confined to `find_or_insert_annotation_location` in
  `app/src-tauri/src/db/io/import.rs` and new tests in
  `app/src-tauri/tests/import_wireformat_tests.rs` — `db/io/diff.rs`, `db/io/export.rs`,
  `CategoryList.tsx`, and `docs/` were not touched.
