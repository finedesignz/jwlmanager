//! Database access over the extracted `userData.db` — read-side browse
//! queries (`browse`, `notes`) plus the mutation/edit backends Phase 7 adds
//! on the shared `edit` safety spine (`favorites`, and the sibling modules
//! Plans 02-05 add on top of it).

pub mod browse;
pub mod color;
pub mod delete;
pub mod edit;
pub mod favorites;
pub mod highlights;
pub mod labels;
pub mod notes;
pub mod pragma_guard;
pub mod reorder;
pub mod resources;
pub mod scrub;
pub mod tags;
pub mod trim;
