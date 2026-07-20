//! Proves the `Category` enum round-trips through serde (the ts-rs binding
//! generation itself runs as ts-rs's own auto-generated `#[test]` inside
//! `src/category.rs` during `cargo test`).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

use jwlmanager_lib::category::Category;

#[test]
fn category_enum() {
    let json = serde_json::to_string(&Category::Notes).expect("serialize Category");
    assert_eq!(json, "\"Notes\"");

    let round_tripped: Category = serde_json::from_str(&json).expect("deserialize Category");
    assert_eq!(round_tripped, Category::Notes);

    // All six categories must be distinct, stable identifiers — never a
    // translated display string (DATA-08, D-11).
    let all = [
        Category::Notes,
        Category::Bookmarks,
        Category::Favorites,
        Category::Highlights,
        Category::Annotations,
        Category::Playlists,
    ];
    let mut seen = std::collections::HashSet::new();
    for cat in all {
        let s = serde_json::to_string(&cat).expect("serialize Category");
        assert!(seen.insert(s), "Category variants must serialize uniquely");
    }
}
