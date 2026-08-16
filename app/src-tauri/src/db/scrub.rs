//! Archive-wide, selection-free text-scrub operations — Clean and Mask
//! (EDIT-06, `JWLManager.py:3698-3823`, D7-07/D7-08). Only user-authored
//! text fields are ever touched here (`InputField.Value`, `Bookmark.Title`,
//! `Bookmark.Snippet`, `Note.Title`, `Note.Content`, `Location.Title`) —
//! NEVER any publication body text, which this app never even loads into
//! these tables.
//!
//! **Clean** (`clean_items`, `:3698-3748`) normalizes Unicode separator
//! junk: every `\p{Zs}` except ASCII space becomes a single ASCII space,
//! every `\p{Zl}`/`\p{Zp}` is removed, and every `\r` becomes `\n`. Rust's
//! `regex` crate has no `regex.V1` `--` set-subtraction the Python's
//! `[\p{Zs}--\x20]` relies on, so [`clean_text`] special-cases ASCII space
//! explicitly in the per-char transform below rather than trying to express
//! the subtraction as a character class.
//!
//! **Mask** (`obscure_items`, `:3750-3823`) replaces every `\p{L}` character
//! with a letter cycled from a randomly-chosen word out of
//! `['obscured','yada','bla','gibberish','børk']`, preserving per-character
//! case, and leaves every non-letter untouched. Randomness makes exact
//! output non-reproducible byte-for-byte by design — [`SplitMix64`] is a
//! small hand-rolled, dependency-free PRNG (07-RESEARCH.md Correction 4:
//! `rand` is not a dependency and none is added for this) seeded explicitly
//! per call, following `src/time.rs`'s own dependency-free precedent. Tests
//! assert SHAPE invariants (length, case, non-letter identity) under a fixed
//! seed for determinism — never exact masked output beyond that.
//!
//! Both ops follow the shared dry-run/apply envelope every Phase 7 edit op
//! uses: `apply_*(tx, ...)` runs inside the caller's transaction; `dry_run_*`
//! runs the SAME real apply inside a never-committed `unchecked_transaction`
//! under [`PragmaGuard`] (SAFE-01), reporting a [`DryRunReport`] built from
//! per-table row-changed counts (`overwritten` — every touched row keeps its
//! PK, this is always an UPDATE-in-place, never an add/delete), mirroring
//! `reorder.rs`'s `reorder_report` shape rather than the generic PK-snapshot
//! diff (which can't express "this row's TEXT changed" — only presence).

use crate::db::edit::DryRunReport;
use crate::db::pragma_guard::PragmaGuard;
use crate::error::ArchiveError;
use regex::Regex;
use rusqlite::{params, Connection, Transaction};
use std::collections::BTreeMap;
use std::sync::LazyLock;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::CleanFailed {
        reason: format!("{context}: {err}"),
    }
}

fn map_mask_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::MaskFailed {
        reason: format!("{context}: {err}"),
    }
}

// ---------------------------------------------------------------------------
// Clean
// ---------------------------------------------------------------------------

/// `\p{Zs}` (any Unicode space separator, INCLUDING ASCII space) — matched
/// per-character below, with ASCII space special-cased out before this ever
/// runs (module docs: this is how the missing `--` set-subtraction is
/// expressed). A fixed, compile-time-known-valid literal — a compile failure
/// here is a programmer error caught by `scrub_regex_patterns_compile`, not
/// an archive-data-path panic (D-15), matching `labels.rs:20-31`'s pattern.
static ZS: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"^\p{Zs}$").expect("ZS regex must compile")
});
/// `[\p{Zl}\p{Zp}]` — line/paragraph separators, always removed (never
/// space-substituted), matching `joiners` (`JWLManager.py:3731`).
static ZL_ZP: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"^[\p{Zl}\p{Zp}]$").expect("ZL_ZP regex must compile")
});

/// Ports `clean(txt)` (`JWLManager.py:3700-3703`) as a single per-character
/// pass: `\r` -> `\n`; ASCII space (U+0020) is left alone; any other
/// `\p{Zs}` becomes ASCII space; `\p{Zl}`/`\p{Zp}` are removed; everything
/// else is copied through unchanged. Returns `None` when nothing in `input`
/// needed changing (SAME semantics as the row-count gate `clean_annotations`/
/// `clean_notes` use via `regex.search(combined, ...)`) so callers can both
/// detect "this row needs an UPDATE" and get the new value in one pass —
/// `\r` counts as a change too (the plan's behavior contract), a small,
/// deliberate widening of Python's `combined` detector, which omits `\r`
/// from ITS row-touch gate.
fn clean_text(input: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if c == '\r' {
            out.push('\n');
            changed = true;
        } else if c == ' ' {
            out.push(c);
        } else if ZS.is_match(&c.to_string()) {
            out.push(' ');
            changed = true;
        } else if ZL_ZP.is_match(&c.to_string()) {
            changed = true; // removed: nothing pushed
        } else {
            out.push(c);
        }
    }
    if changed {
        Some(out)
    } else {
        None
    }
}

/// Applies [`clean_text`] to `Value = ?` (as a fresh literal string), else
/// keeps the ORIGINAL `Option<String>` (preserving a `NULL` value rather
/// than ever coercing it to an empty string when the field wasn't itself
/// the reason for the change).
fn clean_opt(input: &Option<String>) -> Option<String> {
    clean_text(input.as_deref().unwrap_or(""))
}

/// Normalizes Unicode separator junk archive-wide, per the module docs.
/// Touches ONLY `InputField.Value` (keyed by `TextTag`) and `Note.Title`/
/// `Note.Content` (keyed by `NoteId`) — ports `clean_annotations`/
/// `clean_notes` (`JWLManager.py:3705-3723`). Every `UPDATE` binds the
/// transformed value as a parameter (never SQL-interpolated). Returns
/// per-table ROW counts (not replacement counts) — a row with two
/// separators in it increments its table's count by exactly 1. Runs inside
/// the caller's transaction; a failure here rolls back with everything else
/// in that transaction.
pub fn apply_clean(tx: &Transaction) -> Result<BTreeMap<String, usize>, ArchiveError> {
    let mut counts = BTreeMap::new();

    let mut input_field_count = 0usize;
    {
        let mut stmt = tx
            .prepare("SELECT Value, TextTag FROM InputField")
            .map_err(|e| map_sqlite_err(e, "apply_clean: prepare InputField scan"))?;
        let rows: Vec<(Option<String>, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| map_sqlite_err(e, "apply_clean: scan InputField"))?
            .collect::<Result<_, _>>()
            .map_err(|e| map_sqlite_err(e, "apply_clean: read InputField row"))?;
        for (value, tag) in rows {
            if let Some(cleaned) = clean_opt(&value) {
                tx.execute(
                    "UPDATE InputField SET Value = ?1 WHERE TextTag = ?2",
                    params![cleaned, tag],
                )
                .map_err(|e| map_sqlite_err(e, "apply_clean: update InputField"))?;
                input_field_count += 1;
            }
        }
    }
    if input_field_count > 0 {
        counts.insert("InputField".to_string(), input_field_count);
    }

    let mut note_count = 0usize;
    {
        let mut stmt = tx
            .prepare("SELECT Title, Content, NoteId FROM Note")
            .map_err(|e| map_sqlite_err(e, "apply_clean: prepare Note scan"))?;
        let rows: Vec<(Option<String>, Option<String>, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| map_sqlite_err(e, "apply_clean: scan Note"))?
            .collect::<Result<_, _>>()
            .map_err(|e| map_sqlite_err(e, "apply_clean: read Note row"))?;
        for (title, content, id) in rows {
            let cleaned_title = clean_opt(&title);
            let cleaned_content = clean_opt(&content);
            if cleaned_title.is_some() || cleaned_content.is_some() {
                let new_title = cleaned_title.or(title);
                let new_content = cleaned_content.or(content);
                tx.execute(
                    "UPDATE Note SET Title = ?1, Content = ?2 WHERE NoteId = ?3",
                    params![new_title, new_content, id],
                )
                .map_err(|e| map_sqlite_err(e, "apply_clean: update Note"))?;
                note_count += 1;
            }
        }
    }
    if note_count > 0 {
        counts.insert("Note".to_string(), note_count);
    }

    Ok(counts)
}

/// Wraps [`apply_clean`]'s per-table ROW counts into a [`DryRunReport`] —
/// every touched row is an UPDATE-in-place (`overwritten`), matching
/// `reorder_report`'s shape (`reorder.rs:129-140`).
fn clean_report(counts: BTreeMap<String, usize>) -> DryRunReport {
    DryRunReport {
        added: BTreeMap::new(),
        overwritten: counts,
        deleted: BTreeMap::new(),
        total_deleted: 0,
        skipped: BTreeMap::new(),
    }
}

/// Runs the REAL [`apply_clean`] inside a transaction that is NEVER
/// committed and returns the resulting [`DryRunReport`] — leaves the DB
/// unchanged (SAFE-01). Matches `dry_run_reorder`'s envelope shape exactly.
pub fn dry_run_clean(conn: &mut Connection) -> Result<DryRunReport, ArchiveError> {
    let guard = PragmaGuard::new(conn).map_err(|e| map_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_sqlite_err(e, "setting dry-run pragmas"))?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_sqlite_err(e, "opening dry-run transaction"))?;

    let counts = apply_clean(&tx)?;
    let report = clean_report(counts);

    drop(tx);
    drop(guard);

    Ok(report)
}

// ---------------------------------------------------------------------------
// Mask
// ---------------------------------------------------------------------------

/// `\p{L}` (any Unicode letter) — matched per-character, mirroring `m =
/// regex.compile(r'\p{L}')` (`JWLManager.py:3806`).
static LETTER: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"^\p{L}$").expect("LETTER regex must compile")
});

/// `words = ['obscured', 'yada', 'bla', 'gibberish', 'børk']`
/// (`JWLManager.py:3805`) — every word's letters are simple, single-`char`
/// case-fold targets (verified: `ø` uppercases to the single char `Ø`), so
/// [`obscure_text`]'s per-character replacement never breaks the
/// length-preservation shape invariant.
const MASK_WORDS: [&str; 5] = ["obscured", "yada", "bla", "gibberish", "børk"];

/// A source of bounded random integers, threaded explicitly rather than read
/// from a global — the SAME pattern `guid_seed: u64` uses at the `db::color`
/// boundary (07-RESEARCH.md Shared Pattern 6), generalized to a trait so
/// [`obscure_text`] stays pure with respect to randomness and tests can
/// supply a scripted/deterministic implementation if ever needed beyond
/// [`SplitMix64`].
pub trait SeedRng {
    /// Returns a value in `0..bound`. `bound` is always `> 0` at every call
    /// site in this module (`MASK_WORDS.len()`, non-empty word `.len()`).
    fn next_range(&mut self, bound: usize) -> usize;
}

/// Dependency-free SplitMix64 PRNG (reference:
/// <https://prng.di.unimi.it/splitmix64.c>), following `src/time.rs`'s own
/// precedent of hand-rolling a small, well-known, cited algorithm rather
/// than adding a crate for one narrow need (`rand` is absent from
/// `Cargo.lock`, 07-RESEARCH.md Correction 4). NOT cryptographically
/// secure — Mask is a privacy convenience matching the Python's behavior,
/// never a cryptographic guarantee (T-07-23).
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl SeedRng for SplitMix64 {
    fn next_range(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

/// Ports `obscure_text` (`JWLManager.py:3752-3768`): picks ONE random word
/// from [`MASK_WORDS`] per call (matching Python's per-field word choice,
/// not a single archive-wide word) and cycles its letters — uppercased when
/// the source character is uppercase — over every `\p{L}` character in
/// `input`; every non-letter is copied through byte-identical. Output
/// `chars().count()` always equals `input.chars().count()`.
pub fn obscure_text(input: &str, rng: &mut impl SeedRng) -> String {
    let word: Vec<char> = MASK_WORDS[rng.next_range(MASK_WORDS.len())]
        .chars()
        .collect();
    let word_len = word.len();
    let mut idx = 0usize;
    let mut out = String::with_capacity(input.len());

    for c in input.chars() {
        if LETTER.is_match(&c.to_string()) {
            let replacement = word[idx];
            if c.is_uppercase() {
                for u in replacement.to_uppercase() {
                    out.push(u);
                }
            } else {
                out.push(replacement);
            }
            idx += 1;
            if idx == word_len {
                idx = 0;
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `obscure_text` over an `Option<&str>`, treating `NULL`/absent as `""`
/// (mirrors Python's falsy-guard `if title: title = obscure_text(title)`) —
/// returns `None` when there is nothing to mask (empty/absent), so callers
/// can gate the row-touched count on "at least one field actually changed",
/// same as [`clean_opt`].
fn obscure_opt(input: &Option<String>, rng: &mut impl SeedRng) -> Option<String> {
    let s = input.as_deref().unwrap_or("");
    if s.is_empty() {
        None
    } else {
        Some(obscure_text(s, rng))
    }
}

/// Masks every user-authored text field archive-wide — ports
/// `obscure_annotations`/`obscure_bookmarks`/`obscure_notes`/
/// `obscure_locations` (`JWLManager.py:3770-3798`), in the SAME order.
/// Touches ONLY `InputField.Value`, `Bookmark.Title`/`Snippet`,
/// `Note.Title`/`Content`, and `Location.Title` — never publication body
/// text (this app never loads any). A row with nothing to mask (every
/// relevant field empty/absent) is left untouched and NOT counted — a
/// semantically meaningful row-count for the dry-run preview, rather than
/// Python's literal always-`UPDATE`-every-row behavior (acceptable per
/// module docs: shape invariants only, never a byte-diff oracle). `seed`
/// is threaded explicitly (module docs) so the SAME seed always produces
/// the SAME masked output for the SAME input across `dry_run_mask` and
/// `apply_mask`. Runs inside the caller's transaction; a failure here
/// rolls back with everything else in that transaction.
pub fn apply_mask(tx: &Transaction, seed: u64) -> Result<BTreeMap<String, usize>, ArchiveError> {
    let mut rng = SplitMix64::new(seed);
    let mut counts = BTreeMap::new();

    let mut input_field_count = 0usize;
    {
        let mut stmt = tx
            .prepare("SELECT Value, TextTag FROM InputField")
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: prepare InputField scan"))?;
        let rows: Vec<(Option<String>, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: scan InputField"))?
            .collect::<Result<_, _>>()
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: read InputField row"))?;
        for (value, tag) in rows {
            if let Some(masked) = obscure_opt(&value, &mut rng) {
                tx.execute(
                    "UPDATE InputField SET Value = ?1 WHERE TextTag = ?2",
                    params![masked, tag],
                )
                .map_err(|e| map_mask_sqlite_err(e, "apply_mask: update InputField"))?;
                input_field_count += 1;
            }
        }
    }
    if input_field_count > 0 {
        counts.insert("InputField".to_string(), input_field_count);
    }

    let mut bookmark_count = 0usize;
    {
        let mut stmt = tx
            .prepare("SELECT Title, Snippet, BookmarkId FROM Bookmark")
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: prepare Bookmark scan"))?;
        let rows: Vec<(Option<String>, Option<String>, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: scan Bookmark"))?
            .collect::<Result<_, _>>()
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: read Bookmark row"))?;
        for (title, snippet, id) in rows {
            let masked_title = obscure_opt(&title, &mut rng);
            let masked_snippet = obscure_opt(&snippet, &mut rng);
            if masked_title.is_some() || masked_snippet.is_some() {
                let new_title = masked_title.or(title);
                let new_snippet = masked_snippet.or(snippet);
                tx.execute(
                    "UPDATE Bookmark SET Title = ?1, Snippet = ?2 WHERE BookmarkId = ?3",
                    params![new_title, new_snippet, id],
                )
                .map_err(|e| map_mask_sqlite_err(e, "apply_mask: update Bookmark"))?;
                bookmark_count += 1;
            }
        }
    }
    if bookmark_count > 0 {
        counts.insert("Bookmark".to_string(), bookmark_count);
    }

    let mut note_count = 0usize;
    {
        let mut stmt = tx
            .prepare("SELECT Title, Content, NoteId FROM Note")
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: prepare Note scan"))?;
        let rows: Vec<(Option<String>, Option<String>, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: scan Note"))?
            .collect::<Result<_, _>>()
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: read Note row"))?;
        for (title, content, id) in rows {
            let masked_title = obscure_opt(&title, &mut rng);
            let masked_content = obscure_opt(&content, &mut rng);
            if masked_title.is_some() || masked_content.is_some() {
                let new_title = masked_title.or(title);
                let new_content = masked_content.or(content);
                tx.execute(
                    "UPDATE Note SET Title = ?1, Content = ?2 WHERE NoteId = ?3",
                    params![new_title, new_content, id],
                )
                .map_err(|e| map_mask_sqlite_err(e, "apply_mask: update Note"))?;
                note_count += 1;
            }
        }
    }
    if note_count > 0 {
        counts.insert("Note".to_string(), note_count);
    }

    let mut location_count = 0usize;
    {
        let mut stmt = tx
            .prepare("SELECT Title, LocationId FROM Location")
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: prepare Location scan"))?;
        let rows: Vec<(Option<String>, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: scan Location"))?
            .collect::<Result<_, _>>()
            .map_err(|e| map_mask_sqlite_err(e, "apply_mask: read Location row"))?;
        for (title, id) in rows {
            if let Some(masked) = obscure_opt(&title, &mut rng) {
                tx.execute(
                    "UPDATE Location SET Title = ?1 WHERE LocationId = ?2",
                    params![masked, id],
                )
                .map_err(|e| map_mask_sqlite_err(e, "apply_mask: update Location"))?;
                location_count += 1;
            }
        }
    }
    if location_count > 0 {
        counts.insert("Location".to_string(), location_count);
    }

    Ok(counts)
}

/// Wraps [`apply_mask`]'s per-table ROW counts into a [`DryRunReport`],
/// matching [`clean_report`]'s shape.
fn mask_report(counts: BTreeMap<String, usize>) -> DryRunReport {
    DryRunReport {
        added: BTreeMap::new(),
        overwritten: counts,
        deleted: BTreeMap::new(),
        total_deleted: 0,
        skipped: BTreeMap::new(),
    }
}

/// Runs the REAL [`apply_mask`] inside a transaction that is NEVER
/// committed and returns the resulting [`DryRunReport`] — leaves the DB
/// unchanged (SAFE-01). `seed` is supplied by the caller (the `mask_apply`/
/// `mask_dry_run` commands in `lib.rs` share ONE seed per user action) so a
/// preview's counts and shape are reproducible against the eventual apply
/// for testing, even though the masked TEXT itself is not meant to be
/// previewed verbatim (the UI shows counts, never the masked strings).
pub fn dry_run_mask(conn: &mut Connection, seed: u64) -> Result<DryRunReport, ArchiveError> {
    let guard =
        PragmaGuard::new(conn).map_err(|e| map_mask_sqlite_err(e, "snapshotting pragmas"))?;

    conn.execute_batch(
        "PRAGMA temp_store = 'MEMORY'; \
         PRAGMA synchronous = 'OFF'; \
         PRAGMA journal_mode = 'MEMORY'; \
         PRAGMA foreign_keys = 'OFF';",
    )
    .map_err(|e| map_mask_sqlite_err(e, "setting dry-run pragmas"))?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| map_mask_sqlite_err(e, "opening dry-run transaction"))?;

    let counts = apply_mask(&tx, seed)?;
    let report = mask_report(counts);

    drop(tx);
    drop(guard);

    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn scrub_regex_patterns_compile() {
        assert!(ZS.is_match(" "));
        assert!(ZL_ZP.is_match("\u{2028}"));
        assert!(LETTER.is_match("a"));
    }

    #[test]
    fn clean_text_ascii_space_untouched() {
        assert_eq!(clean_text("a b"), None);
    }

    #[test]
    fn clean_text_nbsp_and_ideographic_space_become_ascii_space() {
        assert_eq!(clean_text("a\u{00A0}b\u{3000}c"), Some("a b c".to_string()));
    }

    #[test]
    fn clean_text_line_and_paragraph_separators_removed() {
        assert_eq!(clean_text("a\u{2028}b\u{2029}c"), Some("abc".to_string()));
    }

    #[test]
    fn clean_text_cr_becomes_lf() {
        assert_eq!(clean_text("a\r\nb"), Some("a\n\nb".to_string()));
    }

    #[test]
    fn clean_text_multiple_separators_still_one_change() {
        // Row-level "did this string change" is a single Option, not a count
        // of replacements — the row-count-not-replacement-count contract is
        // enforced at the caller (apply_clean increments by 1 per row).
        assert!(clean_text("a\u{00A0}b\u{00A0}c").is_some());
    }

    #[test]
    fn obscure_text_preserves_length() {
        let mut rng = SplitMix64::new(42);
        let input = "Héllo Мир 123 !@# 😀";
        let output = obscure_text(input, &mut rng);
        assert_eq!(input.chars().count(), output.chars().count());
    }

    #[test]
    fn obscure_text_non_letters_byte_identical() {
        let mut rng = SplitMix64::new(7);
        let input = "a1 b2! c3😀";
        let output = obscure_text(input, &mut rng);
        for (i, o) in input.chars().zip(output.chars()) {
            if !LETTER.is_match(&i.to_string()) {
                assert_eq!(i, o, "non-letter char must be byte-identical");
            }
        }
    }

    #[test]
    fn obscure_text_preserves_case_per_character() {
        let mut rng = SplitMix64::new(99);
        let input = "AbCdEf";
        let output = obscure_text(input, &mut rng);
        for (i, o) in input.chars().zip(output.chars()) {
            assert_eq!(i.is_uppercase(), o.is_uppercase());
        }
    }

    #[test]
    fn obscure_text_same_seed_same_input_is_deterministic() {
        let mut rng1 = SplitMix64::new(12345);
        let mut rng2 = SplitMix64::new(12345);
        let a = obscure_text("Deterministic Input", &mut rng1);
        let b = obscure_text("Deterministic Input", &mut rng2);
        assert_eq!(a, b);
    }

    #[test]
    fn split_mix64_next_range_stays_in_bounds() {
        let mut rng = SplitMix64::new(1);
        for _ in 0..100 {
            assert!(rng.next_range(5) < 5);
        }
    }
}
