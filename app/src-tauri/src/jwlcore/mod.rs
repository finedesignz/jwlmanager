//! jwlCore native library binding surface (D-12, D-13, D-13a).
//!
//! Phase 1 scope is strictly LOAD + RESOLVE SYMBOLS + read version. The
//! `mergeDatabase` symbol is resolved (to prove ABI compatibility) but is
//! NEVER called here — that is Phase 5's job.

pub mod loader;

pub use loader::{check_jwlcore, JwlCoreStatus};
