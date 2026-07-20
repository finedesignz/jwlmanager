//! `Category` — a single-sourced Rust enum, exported to TypeScript via
//! `ts-rs` (D-11, DATA-08). Fixes the Python app's latent i18n control-flow
//! bug (`if category == _('Notes')`, `JWLManager.py:560-570`): translated
//! display strings never participate in matching, only this enum does.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/Category.ts")]
pub enum Category {
    Notes,
    Bookmarks,
    Favorites,
    Highlights,
    Annotations,
    Playlists,
}
