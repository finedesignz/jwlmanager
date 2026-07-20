//! Proves `archive::extract::extract_zip_slip_safe` rejects a `../`
//! traversal entry. The full 6-variant crafted-fixture proof (all
//! `ZipSlipVariant`s) lands in 01-02; this plan proves the primitive works
//! on the representative case (ARCH-05).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use common::ZipSlipVariant;
use jwlmanager_lib::archive::extract::extract_zip_slip_safe;

#[test]
fn extract_rejects_traversal() {
    let (_fixture_dir, archive_path) =
        common::generate_zip_slip_fixture(ZipSlipVariant::UnixTraversal);
    let dest = tempfile::TempDir::new().expect("dest tempdir");

    let result = extract_zip_slip_safe(&archive_path, dest.path());

    assert!(
        result.is_err(),
        "a '../' traversal entry must be refused, not silently extracted"
    );
}
