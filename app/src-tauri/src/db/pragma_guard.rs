//! RAII PRAGMA snapshot/restore guard (D2-03, 02-01-PLAN.md finding 4).
//!
//! SQLite PRAGMAs are connection-level session state, NOT transactional —
//! `ROLLBACK` never restores a PRAGMA to its pre-transaction value. `trim_db`
//! forces `foreign_keys`/`journal_mode`/`synchronous`/`temp_store` to sweep-
//! friendly values (matching `JWLManager.py:3862-3865`) before opening its
//! transaction; this guard snapshots the PRIOR values on construction and
//! restores them on `Drop`, so BOTH the commit path and any early-return
//! failure/rollback path leave the connection's PRAGMAs exactly as they were
//! found — never hardcoded back to `ON`/`FULL`/`DELETE`/`DEFAULT` (Python's
//! literal restore values), because a caller may have deliberately configured
//! different session PRAGMAs before calling trim.

use rusqlite::Connection;

/// Snapshots `foreign_keys`, `journal_mode`, `synchronous`, and `temp_store`
/// on construction; restores each to its snapshotted value when dropped.
///
/// Holds a shared `&Connection` (not `&mut`), so callers can still open an
/// [`Connection::unchecked_transaction`] while a `PragmaGuard` is alive —
/// `rusqlite::Connection::transaction` requires `&mut self` and would
/// conflict with this guard's borrow; `unchecked_transaction` takes `&self`
/// specifically to support this pattern.
pub struct PragmaGuard<'c> {
    conn: &'c Connection,
    foreign_keys: i64,
    journal_mode: String,
    synchronous: i64,
    temp_store: i64,
}

impl<'c> PragmaGuard<'c> {
    /// Reads and stores the connection's current PRAGMA values. Does NOT
    /// change anything yet — callers set sweep-friendly PRAGMA values
    /// AFTER constructing the guard.
    pub fn new(conn: &'c Connection) -> Result<Self, rusqlite::Error> {
        let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
        let journal_mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        let temp_store: i64 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0))?;
        Ok(Self {
            conn,
            foreign_keys,
            journal_mode,
            synchronous,
            temp_store,
        })
    }
}

impl Drop for PragmaGuard<'_> {
    /// Restores every snapshotted PRAGMA. Errors here cannot propagate from
    /// `Drop` — they are deliberately swallowed (there is no typed-error
    /// channel out of a destructor); a restore failure would already be
    /// preceded by a broken connection, which the caller's own operation
    /// result already surfaces.
    fn drop(&mut self) {
        let _ = self
            .conn
            .pragma_update(None, "foreign_keys", self.foreign_keys);
        let _ = self
            .conn
            .pragma_update(None, "journal_mode", &self.journal_mode);
        let _ = self
            .conn
            .pragma_update(None, "synchronous", self.synchronous);
        let _ = self.conn.pragma_update(None, "temp_store", self.temp_store);
    }
}
