//! Per-category `.txt` wire-format interchange (IO-01/IO-02/IO-03,
//! 08-01-PLAN.md) — export/import of the pipe-delimited `.txt` files Python
//! writes for Annotations/Bookmarks/Favorites/Highlights/Notes
//! (`JWLManager.py:1307-2123`). This module tree is pure Rust file I/O +
//! rusqlite; NO jwlCore FFI is used anywhere in Phase 8 (threat register
//! T-08-SC's prohibitions class this alongside "no new dependency").
//!
//! [`header`] builds the shared export-file preamble every category writes
//! verbatim (byte-for-byte, injected timestamp/version context for
//! deterministic golden-fixture comparison). [`export`] holds the
//! `'None'`-sentinel row-join helper plus each category's export function.
//! [`import`] holds the two-stage parse-then-apply shape (parse fully BEFORE
//! any transaction opens, per D8-04's fail-fast-whole-transaction contract).
//!
//! This plan (08-01) lands ONLY Favorites — the simplest of the five `.txt`
//! categories (6 flat pipe fields, no `¦` escaping, no `{END}` sentinel, no
//! range-merge) — to prove the whole spine end-to-end before Plans 02-05 add
//! Annotations/Bookmarks/Highlights/Notes on top of it.

pub mod export;
pub mod header;
pub mod import;
pub mod usermark;
