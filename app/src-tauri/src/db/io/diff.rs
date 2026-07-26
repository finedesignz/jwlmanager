//! Incremental-export diff engine (IO-04, 09-01-PLAN.md) — shared by every
//! category plan 02-04 add on top of this Notes tracer.
//!
//! **The two-layer rule (state once, applied everywhere below):** the
//! exported set is decided PURELY by hash-set membership — a live record is
//! exported iff its hash is absent from the prior file's hash set. The
//! identity key (e.g. Notes' `{CREATED=}` value) is NEVER consulted to make
//! that decision; it only LABELS a record already in the exported set as
//! "added" (no matching prior key) versus "modified" (a matching prior key),
//! and separately names which prior keys are missing from the live side
//! ("deleted candidates"). Consequently every failure of the identity layer
//! — a collision, a churned key, a coincidence — biases toward EXPORTING a
//! record that didn't strictly need it, never toward OMITTING one that did.
//! Over-export is safe; under-export is the data gap this phase exists to
//! prevent (see `<the_one_invariant_that_matters>`, 09-01-PLAN.md).

use super::export::{export_notes, read_note_id_records};
use super::header::ExportHeaderCtx;
use super::import::parse_notes_file;
use crate::db::delete::NonEmptyNoteIds;
use crate::db::resources::ResourceCatalog;
use crate::error::ArchiveError;
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::hash::Hash;
use std::path::Path;
use ts_rs::TS;

/// The Tauri-facing result of an incremental export (IO-04) — export scope
/// counts, NOT a mutation preview (this phase never mutates the archive, so
/// it is a new DTO rather than a reuse of `db::edit::DryRunReport`, which
/// that shipped shape's own doc reserves for edit-op previews).
/// `deleted_candidates` is informational only (D9-04): the frontend must
/// render it with an explicit caveat that removals are never written to the
/// output file.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/IncrementalExportSummary.ts")]
pub struct IncrementalExportSummary {
    pub added: usize,
    pub modified: usize,
    pub deleted_candidates: usize,
    pub exported: usize,
}

/// Hashes `text` (a wire record's exact bytes, or a normalized slice of
/// them) to a hex-encoded SHA-256 digest. A single unit-separator byte
/// (`0x1F`, never a printable wire character) is appended before hashing so
/// that a caller who ever concatenates multiple segments cannot produce a
/// colliding hash from a different segment split of the same total bytes.
pub(crate) fn record_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher.update([0x1F_u8]);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The result of [`diff_records`]: `added` and `modified` together are the
/// exported set (D9-05/D9-04); `deleted_candidates` is informational only —
/// never written to the output file (no wire format encodes a deletion).
#[derive(Debug, Clone, Default)]
pub(crate) struct DiffResult<K> {
    pub added: Vec<K>,
    pub modified: Vec<K>,
    pub deleted_candidates: Vec<K>,
}

/// Computes the incremental-export diff between `prior` and `live`
/// `(identity_key, record_hash)` pairs.
///
/// The exported set — the union of `added` and `modified` — is taken
/// strictly from HASH SET membership: a live entry is exported iff its hash
/// is not present anywhere in `prior`'s hash multiset. That decision never
/// looks at `K` at all, so two live records that happen to share one key are
/// each independently exported when each carries a hash `prior` doesn't
/// have (a key collision can never suppress an export). Only AFTER an entry
/// is already in the exported set does its key get consulted, purely to
/// choose the added/modified label for the summary: a key present in
/// `prior` is `modified`, a key absent from `prior` is `added`.
///
/// `deleted_candidates` is every `prior` key absent from `live`'s key set —
/// independent of the hash comparison above, and independent of `added`/
/// `modified` membership.
pub(crate) fn diff_records<K: Eq + Hash + Clone>(
    prior: &[(K, String)],
    live: &[(K, String)],
) -> DiffResult<K> {
    let prior_hashes: HashSet<&str> = prior.iter().map(|(_, hash)| hash.as_str()).collect();
    let prior_keys: HashSet<&K> = prior.iter().map(|(key, _)| key).collect();

    let mut added = Vec::new();
    let mut modified = Vec::new();
    for (key, hash) in live {
        if !prior_hashes.contains(hash.as_str()) {
            if prior_keys.contains(key) {
                modified.push(key.clone());
            } else {
                added.push(key.clone());
            }
        }
    }

    let live_keys: HashSet<&K> = live.iter().map(|(key, _)| key).collect();
    let deleted_candidates: Vec<K> = prior
        .iter()
        .filter(|(key, _)| !live_keys.contains(key))
        .map(|(key, _)| key.clone())
        .collect();

    DiffResult {
        added,
        modified,
        deleted_candidates,
    }
}

/// Notes' hash input excludes the record's leading `{CREATED=...}
/// {MODIFIED=...}` bracket pair (D9-03 refinement, 09-01-PLAN.md
/// `<design_resolutions>`): a Note whose only change is a bumped timestamp
/// must hash identically to its prior version. Scans forward past the
/// SECOND closing brace of `record_text` (the first two bracket groups are
/// always `{CREATED=...}` then `{MODIFIED=...}` — [`super::export::
/// format_note_record`]'s first two writes, unconditionally, for every
/// Notes shape). Falls back to the whole text if fewer than two closing
/// braces are found (defensive only — a real record always has both).
pub(crate) fn notes_hash_input(record_text: &str) -> &str {
    let mut closes = 0;
    for (idx, ch) in record_text.char_indices() {
        if ch == '}' {
            closes += 1;
            if closes == 2 {
                return &record_text[idx + 1..];
            }
        }
    }
    record_text
}

/// Normalizes `\r\n`/`\r` -> `\n`, matching `import::normalize_line_endings`
/// exactly (duplicated per this codebase's established per-module-copy
/// convention for small helpers rather than a cross-module `pub` — see
/// `db::edit`'s `map_sqlite_err` module doc). The prior file is untrusted
/// user input, very likely a real Windows export with CRLF line endings.
fn normalize_line_endings(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains('\r') {
        std::borrow::Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

/// Extracts the `{CREATED=...}` value from a Notes record header — Notes'
/// wire-recoverable identity key (D9-02 refinement: no wire format encodes
/// its category's DB primary key, so identity must be a natural key already
/// on the wire; `CREATED` is stable for a note's life and is always the
/// first bracket, per [`super::export::format_note_record`]).
fn extract_created(header: &str) -> String {
    header
        .find("{CREATED=")
        .and_then(|idx| {
            let after = &header[idx + "{CREATED=".len()..];
            after.find('}').map(|end| after[..end].to_string())
        })
        .unwrap_or_default()
}

/// Splits a CRLF-normalized prior Notes export's TEXT into
/// `(created_value, record_text)` pairs, sharing the exact `\n===`
/// forward-scan boundary discipline `parse_notes_file` uses (line 1 is the
/// `{NOTES=}` tag, a boundary is any `\n===` immediately followed by `{`,
/// and the trailing `==={END}===` sentinel is consumed only as the FINAL
/// boundary — never emitted as a record of its own, since `windows(2)`
/// never pairs it with a boundary after it).
///
/// Assumes `text` already passed [`super::import::parse_notes_file`] as a
/// fail-fast validation gate (09-01-PLAN.md Task 2) — a chunk this function
/// can't find a `===\n` header terminator in is silently skipped rather than
/// panicking, but that path is not expected to be reached in practice since
/// the caller never calls this on a file `parse_notes_file` rejected.
pub(crate) fn split_prior_note_records(text: &str) -> Vec<(String, String)> {
    let normalized = normalize_line_endings(text);
    let text: &str = &normalized;

    let first_line_end = text.find('\n').unwrap_or(text.len());
    let rest = if first_line_end < text.len() {
        &text[first_line_end + 1..]
    } else {
        ""
    };

    let boundaries: Vec<usize> = rest
        .match_indices("\n===")
        .filter(|(idx, _)| rest[*idx + 4..].starts_with('{'))
        .map(|(idx, _)| idx)
        .collect();

    let mut records = Vec::new();
    if boundaries.len() < 2 {
        return records;
    }

    for window in boundaries.windows(2) {
        let (start, end) = (window[0], window[1]);
        let chunk = &rest[start + 4..end];
        let Some(header_end) = chunk.find("===\n") else {
            continue;
        };
        let header = &chunk[..header_end];
        let created = extract_created(header);
        let record_text = format!("\n==={chunk}");
        records.push((created, record_text));
    }

    records
}

/// Exports Notes changed since `prior_text` (IO-04, 09-01-PLAN.md Task 2) —
/// the `export_notes_incremental` Tauri command's pure body, callable
/// directly by tests (this codebase's established `*_impl` shape: a thin
/// command wrapper over a pure, directly-testable function).
///
/// `prior_text` is `None` for "no prior file supplied" (D9-05): the whole
/// category is exported, identical to a plain [`export_notes`] run, and
/// every note reports as `added`. When `Some`, the text is run through
/// [`parse_notes_file`] FIRST as a fail-fast validation gate — its typed
/// `ImportMalformed` propagates via `?` before ANY output file is written;
/// a malformed prior file is never silently degraded to an empty prior set.
///
/// The exported id set is decided PURELY by hash-set membership (the module
/// doc's two-layer rule) — [`diff_records`] is run SEPARATELY, keyed by the
/// `{CREATED=}` identity, only to label the summary counts; its result never
/// feeds back into which ids get exported. When the exported set is empty,
/// [`export_notes`] still runs — with a selection that matches no live
/// `NoteId` when [`NonEmptyNoteIds`] can't represent "empty" — so the caller
/// always gets a valid, well-formed (header + end sentinel, zero records)
/// output file rather than a silent no-op or fabricated bytes.
pub fn export_notes_incremental(
    conn: &Connection,
    prior_text: Option<&str>,
    catalog: &ResourceCatalog,
    header: &ExportHeaderCtx,
    now: &str,
    out_path: &Path,
) -> Result<IncrementalExportSummary, ArchiveError> {
    let prior_hashed: Vec<(String, String)> = match prior_text {
        Some(text) => {
            // Fail-fast validation gate (D9-05): a malformed prior file
            // aborts here, before any output file is created.
            parse_notes_file(text)?;
            split_prior_note_records(text)
                .into_iter()
                .map(|(created, record_text)| {
                    (created, record_hash(notes_hash_input(&record_text)))
                })
                .collect()
        }
        None => Vec::new(),
    };
    let prior_hash_set: HashSet<&str> =
        prior_hashed.iter().map(|(_, hash)| hash.as_str()).collect();

    let live_records = read_note_id_records(conn, None, catalog, now)?;
    let live_annotated: Vec<(i64, String, String)> = live_records
        .iter()
        .map(|(note_id, text)| {
            let created = extract_created(text);
            let hash = record_hash(notes_hash_input(text));
            (*note_id, created, hash)
        })
        .collect();

    // Exported set: hash-set membership ONLY, never the identity key — the
    // invariant that makes an identity failure bias toward over-export,
    // never under-export.
    let selected_ids: Vec<i64> = live_annotated
        .iter()
        .filter(|(_, _, hash)| !prior_hash_set.contains(hash.as_str()))
        .map(|(note_id, _, _)| *note_id)
        .collect();

    // Summary counts: identity-keyed, computed independently, never fed
    // back into `selected_ids` above.
    let live_hashed: Vec<(String, String)> = live_annotated
        .into_iter()
        .map(|(_, created, hash)| (created, hash))
        .collect();
    let diff = diff_records(&prior_hashed, &live_hashed);

    match NonEmptyNoteIds::try_from(selected_ids.clone()) {
        Ok(ids) => {
            export_notes(conn, Some(&ids), catalog, header, now, out_path)?;
        }
        Err(_) => {
            // Empty selection is unrepresentable by `NonEmptyNoteIds` by
            // construction (D9-01/D9-04) — still write a valid, well-formed
            // empty export via the SAME exporter, never a silent no-op nor
            // locally fabricated bytes. `-1` never matches a real `NoteId`
            // (SQLite `INTEGER PRIMARY KEY` rowids here are always positive),
            // so this selects zero rows through the same SQL path a real
            // empty selection would.
            let Ok(sentinel) = NonEmptyNoteIds::try_from(vec![-1_i64]) else {
                unreachable!("a single-element Vec is always a valid NonEmptyNoteIds");
            };
            export_notes(conn, Some(&sentinel), catalog, header, now, out_path)?;
        }
    }

    Ok(IncrementalExportSummary {
        added: diff.added.len(),
        modified: diff.modified.len(),
        deleted_candidates: diff.deleted_candidates.len(),
        exported: selected_ids.len(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn identical_prior_and_live_yields_empty_added_and_modified() {
        let prior = vec![(1_i64, record_hash("a")), (2_i64, record_hash("b"))];
        let live = prior.clone();
        let result = diff_records(&prior, &live);
        assert!(result.added.is_empty());
        assert!(result.modified.is_empty());
        assert!(result.deleted_candidates.is_empty());
    }

    #[test]
    fn live_only_key_is_added() {
        let prior: Vec<(i64, String)> = vec![(1, record_hash("a"))];
        let live = vec![(1, record_hash("a")), (2, record_hash("b"))];
        let result = diff_records(&prior, &live);
        assert_eq!(result.added, vec![2]);
        assert!(result.modified.is_empty());
        assert!(result.deleted_candidates.is_empty());
    }

    #[test]
    fn same_key_different_hash_is_modified() {
        let prior = vec![(1_i64, record_hash("a"))];
        let live = vec![(1_i64, record_hash("a-changed"))];
        let result = diff_records(&prior, &live);
        assert!(result.added.is_empty());
        assert_eq!(result.modified, vec![1]);
        assert!(result.deleted_candidates.is_empty());
    }

    #[test]
    fn prior_only_key_is_deleted_candidate_never_added_or_modified() {
        let prior = vec![(1_i64, record_hash("a")), (2_i64, record_hash("b"))];
        let live = vec![(1_i64, record_hash("a"))];
        let result = diff_records(&prior, &live);
        assert!(result.added.is_empty());
        assert!(result.modified.is_empty());
        assert_eq!(result.deleted_candidates, vec![2]);
    }

    #[test]
    fn two_live_records_sharing_one_key_are_both_exported_when_both_hashes_are_new() {
        let prior: Vec<(i64, String)> = vec![];
        let live = vec![(1_i64, record_hash("a")), (1_i64, record_hash("b"))];
        let result = diff_records(&prior, &live);
        // Decision is per-hash, not per-key: both entries independently pass
        // the "hash absent from prior" test, so both are exported (as
        // `added`, since neither prior side has key 1) — a key collision can
        // never suppress an export.
        assert_eq!(result.added.len(), 2);
        assert!(result.modified.is_empty());
    }

    #[test]
    fn notes_hash_input_strips_leading_created_modified_pair() {
        let record = "\n==={CREATED=2026-01-01T00:00:00}{MODIFIED=2026-06-01T00:00:00}{TAGS=}===\ntitle\nnote";
        let stripped = notes_hash_input(record);
        assert_eq!(stripped, "{TAGS=}===\ntitle\nnote");
    }

    #[test]
    fn notes_hash_input_is_stable_across_a_timestamp_only_change() {
        let a = "\n==={CREATED=2026-01-01T00:00:00}{MODIFIED=2026-06-01T00:00:00}{TAGS=}===\ntitle\nnote";
        let b = "\n==={CREATED=2026-01-01T00:00:00}{MODIFIED=2026-06-02T00:00:00}{TAGS=}===\ntitle\nnote";
        assert_eq!(notes_hash_input(a), notes_hash_input(b));
        assert_eq!(record_hash(notes_hash_input(a)), record_hash(notes_hash_input(b)));
    }

    #[test]
    fn split_prior_note_records_finds_created_and_record_text() {
        let text = "{NOTES=}\n\n\
            \n==={CREATED=2026-01-01T00:00:00}{MODIFIED=2026-01-01T00:00:00}{TAGS=}===\nTitle One\nNote body one\
            \n==={CREATED=2026-02-01T00:00:00}{MODIFIED=2026-02-01T00:00:00}{TAGS=}===\nTitle Two\nNote body two\
            \n==={END}===";
        let records = split_prior_note_records(text);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "2026-01-01T00:00:00");
        assert!(records[0].1.contains("Title One"));
        assert_eq!(records[1].0, "2026-02-01T00:00:00");
        assert!(records[1].1.contains("Title Two"));
    }

    #[test]
    fn split_prior_note_records_handles_crlf() {
        let text = "{NOTES=}\r\n\r\n\
            \r\n==={CREATED=2026-01-01T00:00:00}{MODIFIED=2026-01-01T00:00:00}{TAGS=}===\r\nTitle\r\nNote\
            \r\n==={END}===";
        let records = split_prior_note_records(text);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "2026-01-01T00:00:00");
    }
}
