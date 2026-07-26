//! `.txt` export (IO-01) — the `'None'`-sentinel row-join helper shared by
//! every category, plus Favorites' export function
//! (`export_favorites`, `JWLManager.py:1454-1468`).
//!
//! Byte-exactness is the point of this phase: `export_wireformat_tests.rs`
//! compares the written file's bytes against a hand-authored golden fixture,
//! never a normalized/parsed comparison.

use super::header::{build_export_header, ExportHeaderCtx};
use crate::db::favorites::NonEmptyTagMapIds;
use crate::error::ArchiveError;
use rusqlite::types::Value;
use rusqlite::Connection;
use std::io::Write;
use std::path::Path;

/// Favorites never writes an `==={END}===` sentinel — unlike Annotations,
/// which does (`JWLManager.py:1420`). Encoded as an explicit stated fact
/// (RESEARCH Pitfall 1) rather than a silent omission, so a future category
/// module can't "fix" this into a spurious consistency.
#[allow(dead_code)] // documented fact, referenced by module docs/tests rather than code
pub(crate) const FAVORITES_WRITES_END_SENTINEL: bool = false;

fn map_sqlite_err(err: rusqlite::Error, context: &str) -> ArchiveError {
    ArchiveError::ExportFailed {
        reason: format!("{context}: {err}"),
    }
}

/// Converts one SQLite cell to `Option<String>` matching Python's
/// `str(x) if x is not None else None` — an integer renders as its plain
/// decimal string (never e.g. `1.0`), text renders verbatim, `NULL` renders
/// as `None` (the Rust `Option`, NOT yet the wire literal — [`join_row`]
/// applies the `'None'` STRING sentinel at join time, keeping the two
/// concerns separate).
fn value_to_field(value: Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Integer(i) => Some(i.to_string()),
        Value::Real(f) => Some(f.to_string()),
        Value::Text(s) => Some(s),
        Value::Blob(_) => None,
    }
}

/// Joins one row's fields with `|`, rendering `None` as the literal
/// four-character string `None` — ports
/// `'|'.join(str(x) if x is not None else 'None' for x in row)`
/// (`JWLManager.py:1445`/`:1461`/`:1477`, identical across every `.txt`
/// category). This EXACT helper is reused by Task 3's import dup-check
/// (`db::io::import`) so the two paths format like-with-like.
pub(crate) fn join_row(values: &[Option<String>]) -> String {
    values
        .iter()
        .map(|v| v.as_deref().unwrap_or("None"))
        .collect::<Vec<_>>()
        .join("|")
}

/// Reads every current Favorite row (or, when `ids` is given, exactly the
/// selected `TagMapId`s) in `Position` order, formatted via [`join_row`].
/// Reused verbatim by Task 3's import dup-check (`get_current` port) so the
/// "already present" comparison is always like-with-like against THIS export
/// path's own formatting.
pub(crate) fn read_favorite_lines(
    conn: &Connection,
    ids: Option<&NonEmptyTagMapIds>,
) -> Result<Vec<String>, ArchiveError> {
    let base_sql = "SELECT DocumentId, Track, IssueTagNumber, KeySymbol, MepsLanguage, Type \
         FROM Location JOIN TagMap USING (LocationId) \
         WHERE TagId = (SELECT TagId FROM Tag WHERE Type = 0 AND Name = 'Favorite')";

    let (sql, bound): (String, Vec<i64>) = match ids {
        Some(ids) => {
            let placeholders: String = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            (
                format!("{base_sql} AND TagMapId IN ({placeholders}) ORDER BY Position"),
                ids.iter().copied().collect(),
            )
        }
        None => (format!("{base_sql} ORDER BY Position"), Vec::new()),
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| map_sqlite_err(e, "read_favorite_lines: prepare"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            let mut fields = Vec::with_capacity(6);
            for i in 0..6 {
                let value: Value = row.get(i)?;
                fields.push(value);
            }
            Ok(fields)
        })
        .map_err(|e| map_sqlite_err(e, "read_favorite_lines: query"))?;

    let mut lines = Vec::new();
    for row in rows {
        let fields = row.map_err(|e| map_sqlite_err(e, "read_favorite_lines: read row"))?;
        let fields: Vec<Option<String>> = fields.into_iter().map(value_to_field).collect();
        lines.push(join_row(&fields));
    }
    Ok(lines)
}

/// Exports Favorites (whole category when `ids` is `None`, D8-10
/// selection-optional) to `path` as a `.txt` file: [`build_export_header`]
/// tagged `{FAVORITES}`, then one `\n`-prefixed data row per favorite, in
/// `Position` order. Writes NO `==={END}===` sentinel
/// ([`FAVORITES_WRITES_END_SENTINEL`]). UTF-8, no BOM. Returns the row count.
/// Never mutates the archive (D8-09) — this is a pure read + file write.
pub fn export_favorites(
    conn: &Connection,
    ids: Option<&NonEmptyTagMapIds>,
    header: &ExportHeaderCtx,
    path: &Path,
) -> Result<usize, ArchiveError> {
    let lines = read_favorite_lines(conn, ids)?;

    let mut file = std::fs::File::create(path).map_err(ArchiveError::from)?;
    file.write_all(build_export_header(header).as_bytes())
        .map_err(ArchiveError::from)?;
    for line in &lines {
        file.write_all(b"\n").map_err(ArchiveError::from)?;
        file.write_all(line.as_bytes()).map_err(ArchiveError::from)?;
    }

    Ok(lines.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn join_row_renders_null_as_literal_none() {
        let values = vec![Some("1".to_string()), None, Some("nwt".to_string())];
        assert_eq!(join_row(&values), "1|None|nwt");
    }

    #[test]
    fn join_row_empty_slice_is_empty_string() {
        let values: Vec<Option<String>> = Vec::new();
        assert_eq!(join_row(&values), "");
    }
}
