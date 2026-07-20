//! Thin, raw located-Note query (skeleton). Analog: `JWLManager.py:751-757`
//! (`get_notes`'s main SQL). This is deliberately THIN: no resources.db
//! label synthesis and no independent-notes UNION yet — both thicken in
//! 01-04 (`db/notes.rs` finding, 01-07-PLAN.md non-negotiables).

use crate::error::ArchiveError;
use rusqlite::Connection;
use serde::Serialize;
use ts_rs::TS;

/// A single Notes-list row, over IPC to the frontend.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/NotesRow.ts")]
pub struct NotesRow {
    pub id: i64,
    pub color_index: i64,
    pub tags: Option<String>,
    pub modified: Option<String>,
}

/// Base located-Note query, matching `JWLManager.py:751-757`'s main SQL
/// shape (no `dupes` CTE, no `WHERE` filter — both are later phases).
const LOCATED_NOTES_SQL: &str = "SELECT NoteId Id, ColorIndex Color, \
    GROUP_CONCAT(Name, ' | ') Tags, substr(LastModified, 0, 11) Modified \
    FROM (SELECT * FROM Note n JOIN Location l USING (LocationId) \
        LEFT JOIN TagMap tm USING (NoteId) \
        LEFT JOIN Tag t USING (TagId) \
        LEFT JOIN UserMark u USING (UserMarkId) \
        ORDER BY t.Name) n \
    GROUP BY n.NoteId";

/// Queries the located-Note rows from an already-open `userData.db`
/// connection.
pub fn query_notes(conn: &Connection) -> Result<Vec<NotesRow>, ArchiveError> {
    let mut stmt = conn.prepare(LOCATED_NOTES_SQL)?;
    let mapped = stmt.query_map([], |row| {
        Ok(NotesRow {
            id: row.get(0)?,
            color_index: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            tags: row.get(2)?,
            modified: row.get(3)?,
        })
    })?;

    let mut rows = Vec::new();
    for row in mapped {
        rows.push(row?);
    }
    Ok(rows)
}
