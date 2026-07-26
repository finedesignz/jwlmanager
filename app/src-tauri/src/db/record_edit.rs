//! Field-constrained record editor backend (EDIT-07, D7-09, 07-05-PLAN.md
//! Task 1). Ports `update_notes`/`update_annotations`
//! (`JWLManager.py:2833-2876`, specifically `update_notes` `:2835-2849`
//! including the UserMark synthesis `:2840-2845`, `update_annotations`
//! `:2851-2855`, and the record-scoped single deletes `:2848-2849`,
//! `:2854-2855`).
//!
//! Despite the name "raw data editor", the ACTUAL Python write-back surface
//! is field-constrained, never arbitrary SQL: Notes -> `Title`, `Content`,
//! `ColorIndex` (via `UserMark`, synthesized exactly as `db::color` does);
//! Annotations -> `Value` only, keyed by `(LocationId, TextTag)`. No table
//! name, column name, or SQL fragment ever crosses the IPC boundary —
//! [`RecordEditPayload`]/[`RecordIdentity`] are tagged enums with named
//! fields only.
//!
//! The Note color path reuses [`crate::db::color::apply_color`]'s `Notes`
//! branch VERBATIM (one implementation, not two, per 07-05-PLAN.md
//! key_links) — this module never re-implements UserMark synthesis.
//!
//! The two Annotation delete paths must never be crossed (rule #10): the
//! browse-list delete (`db::delete::delete_annotations`) removes ALL
//! `InputField` rows at a `LocationId` by design (an intentional
//! over-deletion the preview must surface truthfully); [`apply_record_delete`]
//! here is scoped to exactly one `(LocationId, TextTag)` row.

use crate::db::color::{apply_color, ColorSelection};
use crate::db::delete::NonEmptyNoteIds;
use crate::db::edit::{diff_snapshots, snapshot_tables, DryRunReport};
use crate::db::pragma_guard::PragmaGuard;
use crate::db::trim::trim_sweep;
use crate::error::ArchiveError;
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::RecordEditFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Affected-table set for a record edit's [`DryRunReport`]: `Note` (Title/
/// Content/LastModified UPDATE), `UserMark` (color synthesis/update, shared
/// with `db::color`), `InputField` (Annotation Value UPDATE — `rowid`
/// because its natural key `(LocationId, TextTag)` has no single-column PK,
/// same precedent as `db::edit::TRACKED_TABLES`).
pub(crate) const RECORD_EDIT_SNAPSHOT_TABLES: &[(&str, &str)] = &[
    ("Note", "NoteId"),
    ("UserMark", "UserMarkId"),
    ("InputField", "rowid"),
];

/// Identifies exactly one record for [`fetch_record_fields`] and
/// [`apply_record_delete`] — the record-editor's own scoped identity, kept
/// distinct from the browse-list's per-category identity types in
/// `db::delete` (a Note's `NoteId` here is the SAME value as
/// `db::delete::NonEmptyNoteIds`, but this type carries exactly one id, never
/// a selection).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "category")]
#[ts(export, export_to = "../../src/bindings/RecordIdentity.ts")]
pub enum RecordIdentity {
    Notes { note_id: i64 },
    Annotations { location_id: i64, text_tag: String },
}

/// The current field values for one record, fetched fresh when the editor
/// opens. `BrowseRow` (the category list's row shape) never carries a Note's
/// `Title`/`Content` or an Annotation's `Value` — those are the browse list's
/// publication-LABEL metadata, not the record's own editable content —
/// mirroring the Python Data Viewer's own separate `get_notes()`/
/// `get_annotations()` fetch (`JWLManager.py:3041`, `:3125`) rather than
/// reusing the browse-list row.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "category")]
#[ts(export, export_to = "../../src/bindings/RecordEditFields.ts")]
pub enum RecordEditFields {
    Notes {
        title: String,
        content: String,
        /// `None` when the Note has no linked `UserMark` yet (`UserMarkId IS
        /// NULL`) — the editor's "No color" affordance (07-UI-SPEC.md
        /// partial-state).
        color_index: Option<i64>,
    },
    Annotations {
        value: String,
    },
}

/// The typed, field-constrained edit payload IPC accepts — Notes: `Title`/
/// `Content`/`ColorIndex`; Annotations: `Value` only. No table name, column
/// name, or SQL fragment ever crosses this boundary (D7-09).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "category")]
#[ts(export, export_to = "../../src/bindings/RecordEditPayload.ts")]
pub enum RecordEditPayload {
    Notes {
        note_id: i64,
        title: String,
        content: String,
        /// `None` leaves the Note's color untouched; `Some(idx)` sets it,
        /// synthesizing a `UserMark` first if the Note has none yet.
        color_index: Option<i64>,
    },
    Annotations {
        location_id: i64,
        text_tag: String,
        value: String,
    },
}

/// Fetches the current field values for one record so the editor can prefill
/// them (see module docs for why this can't reuse the browse-list row).
pub fn fetch_record_fields(
    conn: &Connection,
    identity: &RecordIdentity,
) -> Result<RecordEditFields, ArchiveError> {
    match identity {
        RecordIdentity::Notes { note_id } => {
            let (title, content, user_mark_id): (String, String, Option<i64>) = conn
                .query_row(
                    "SELECT Title, Content, UserMarkId FROM Note WHERE NoteId = ?1",
                    [note_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|e| map_sqlite_err(e, "fetch_record_fields: note"))?;
            let color_index: Option<i64> = match user_mark_id {
                Some(id) => conn
                    .query_row(
                        "SELECT ColorIndex FROM UserMark WHERE UserMarkId = ?1",
                        [id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| map_sqlite_err(e, "fetch_record_fields: usermark color"))?,
                None => None,
            };
            Ok(RecordEditFields::Notes {
                title,
                content,
                color_index,
            })
        }
        RecordIdentity::Annotations {
            location_id,
            text_tag,
        } => {
            let value: Option<String> = conn
                .query_row(
                    "SELECT Value FROM InputField WHERE LocationId = ?1 AND TextTag = ?2",
                    rusqlite::params![location_id, text_tag],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| map_sqlite_err(e, "fetch_record_fields: annotation"))?
                .flatten();
            Ok(RecordEditFields::Annotations {
                value: value.unwrap_or_default(),
            })
        }
    }
}

/// Applies a record edit — ports `update_notes`/`update_annotations` (module
/// docs). For Notes with `color_index: Some(idx)`, reuses
/// [`crate::db::color::apply_color`]'s `Notes` branch VERBATIM (synthesizes a
/// `UserMark` if the Note has none yet, exactly as the Color Menu path does)
/// before updating `Title`/`Content`/`LastModified`. `now` is injected (never
/// read internally, mirrors `lib.rs`'s `save_archive`/`save_as` pattern) so
/// callers/tests are deterministic. Runs inside the caller's transaction;
/// every SQL statement binds only typed params (SAFE-02).
pub fn apply_record_edit(
    tx: &Transaction,
    payload: &RecordEditPayload,
    now: &str,
    guid_seed: u64,
) -> Result<(), ArchiveError> {
    match payload {
        RecordEditPayload::Notes {
            note_id,
            title,
            content,
            color_index,
        } => {
            if let Some(color_index) = color_index {
                let ids = NonEmptyNoteIds::try_from(vec![*note_id]).map_err(|reason| {
                    ArchiveError::RecordEditFailed {
                        reason: format!("apply_record_edit: {reason}"),
                    }
                })?;
                let selection = ColorSelection::Notes { ids };
                apply_color(tx, &selection, *color_index, guid_seed)?;
            }
            tx.execute(
                "UPDATE Note SET Title = ?1, Content = ?2, LastModified = ?3 WHERE NoteId = ?4",
                rusqlite::params![title, content, now, note_id],
            )
            .map_err(|e| map_sqlite_err(e, "apply_record_edit: update note"))?;
        }
        RecordEditPayload::Annotations {
            location_id,
            text_tag,
            value,
        } => {
            tx.execute(
                "UPDATE InputField SET Value = ?1 WHERE LocationId = ?2 AND TextTag = ?3",
                rusqlite::params![value, location_id, text_tag],
            )
            .map_err(|e| map_sqlite_err(e, "apply_record_edit: update annotation"))?;
        }
    }
    Ok(())
}

/// Runs [`apply_record_edit`] and returns the resulting semantic
/// [`DryRunReport`], snapshotting [`RECORD_EDIT_SNAPSHOT_TABLES`] before/after
/// inside the caller's (already-open, real, committing) transaction — the
/// shape `db::color::apply_color_reporting` established.
pub fn apply_record_edit_reporting(
    tx: &Transaction,
    payload: &RecordEditPayload,
    now: &str,
    guid_seed: u64,
) -> Result<DryRunReport, ArchiveError> {
    let before = snapshot_tables(tx, RECORD_EDIT_SNAPSHOT_TABLES)?;
    apply_record_edit(tx, payload, now, guid_seed)?;
    let after = snapshot_tables(tx, RECORD_EDIT_SNAPSHOT_TABLES)?;
    Ok(diff_snapshots(&before, &after))
}

/// Runs the REAL `apply_record_edit` + `trim_sweep` inside a transaction that
/// is NEVER committed (SAFE-01) and returns a SEMANTIC [`DryRunReport`] —
/// copied verbatim in shape from `db::color::dry_run_color`.
pub fn dry_run_record_edit(
    conn: &mut Connection,
    payload: &RecordEditPayload,
    now: &str,
    guid_seed: u64,
) -> Result<DryRunReport, ArchiveError> {
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

    let before = snapshot_tables(&tx, RECORD_EDIT_SNAPSHOT_TABLES)?;
    apply_record_edit(&tx, payload, now, guid_seed)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, RECORD_EDIT_SNAPSHOT_TABLES)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
    drop(guard);

    Ok(report)
}

/// Deletes EXACTLY the one identified record — Notes: `DELETE FROM Note
/// WHERE NoteId = ?`; Annotations: `DELETE FROM InputField WHERE LocationId
/// = ? AND TextTag = ?`. NEVER the browse-list's over-deleting
/// `LocationId`-only delete (`db::delete::delete_annotations`) — the two
/// annotation delete paths must never be crossed (module docs, rule #10).
pub fn apply_record_delete(
    tx: &Transaction,
    identity: &RecordIdentity,
) -> Result<usize, ArchiveError> {
    match identity {
        RecordIdentity::Notes { note_id } => tx
            .execute("DELETE FROM Note WHERE NoteId = ?1", [note_id])
            .map_err(|e| map_sqlite_err(e, "apply_record_delete: note")),
        RecordIdentity::Annotations {
            location_id,
            text_tag,
        } => tx
            .execute(
                "DELETE FROM InputField WHERE LocationId = ?1 AND TextTag = ?2",
                rusqlite::params![location_id, text_tag],
            )
            .map_err(|e| map_sqlite_err(e, "apply_record_delete: annotation")),
    }
}

/// Runs the REAL `apply_record_delete` + `trim_sweep` inside a transaction
/// that is NEVER committed (SAFE-01) and returns a SEMANTIC [`DryRunReport`].
pub fn dry_run_record_delete(
    conn: &mut Connection,
    identity: &RecordIdentity,
) -> Result<DryRunReport, ArchiveError> {
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

    let before = snapshot_tables(&tx, RECORD_EDIT_SNAPSHOT_TABLES)?;
    apply_record_delete(&tx, identity)?;
    trim_sweep(&tx)?;
    let after = snapshot_tables(&tx, RECORD_EDIT_SNAPSHOT_TABLES)?;

    let report = diff_snapshots(&before, &after);

    drop(tx);
    drop(guard);

    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn record_identity_notes_round_trips_through_json() {
        let identity = RecordIdentity::Notes { note_id: 42 };
        let json = serde_json::to_string(&identity).unwrap();
        let round_tripped: RecordIdentity = serde_json::from_str(&json).unwrap();
        match round_tripped {
            RecordIdentity::Notes { note_id } => assert_eq!(note_id, 42),
            RecordIdentity::Annotations { .. } => panic!("expected Notes variant"),
        }
    }

    #[test]
    fn record_edit_payload_annotations_round_trips_through_json() {
        let payload = RecordEditPayload::Annotations {
            location_id: 10,
            text_tag: "t1".to_string(),
            value: "updated".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let round_tripped: RecordEditPayload = serde_json::from_str(&json).unwrap();
        match round_tripped {
            RecordEditPayload::Annotations {
                location_id,
                text_tag,
                value,
            } => {
                assert_eq!(location_id, 10);
                assert_eq!(text_tag, "t1");
                assert_eq!(value, "updated");
            }
            RecordEditPayload::Notes { .. } => panic!("expected Annotations variant"),
        }
    }
}
