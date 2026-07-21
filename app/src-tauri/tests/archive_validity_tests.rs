//! Proves the widened 12-16 validity gate (SCHEMA-01/02, 03-02-PLAN.md
//! finding 3): v16 and v14 fixtures are both accepted (v14 upgraded to 16
//! on open); v11 and v17 are rejected with distinct typed errors. v12-15
//! acceptance/upgrade mechanics beyond this accept/reject boundary are
//! covered in depth by `schema_upgrade_tests.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::error::ArchiveError;

#[test]
fn schema_gate_accepts_v16_and_v14_rejects_v11() {
    let (_dir16, archive_path16) = common::generate_v16_fixture();
    let accepted16 = open_and_validate(&archive_path16, &dev_resources_db_path());
    assert!(
        accepted16.is_ok(),
        "a v16 fixture must be accepted: {:?}",
        accepted16.err()
    );

    let (_dir14, archive_path14) = common::generate_v14_fixture();
    let accepted14 = open_and_validate(&archive_path14, &dev_resources_db_path());
    assert!(
        accepted14.is_ok(),
        "a v14 fixture must now be accepted and upgraded (12-16 range): {:?}",
        accepted14.err()
    );
    let (session14, _notes) = accepted14.unwrap();
    assert_eq!(
        session14.manifest.schema_version, 16,
        "v14 fixture must be upgraded to 16 on open"
    );

    let (_dir11, archive_path11) = common::generate_v11_fixture();
    let rejected11 = open_and_validate(&archive_path11, &dev_resources_db_path());
    match rejected11 {
        Err(ArchiveError::SchemaTooOld { version }) => {
            assert_eq!(
                version, 11,
                "rejection must report the actual declared version"
            )
        }
        other => panic!("expected SchemaTooOld for a v11 fixture, got {other:?}"),
    }

    let (_dir17, archive_path17) = common::generate_fixture_v17_shape();
    let rejected17 = open_and_validate(&archive_path17, &dev_resources_db_path());
    match rejected17 {
        Err(ArchiveError::SchemaTooNew { version }) => {
            assert_eq!(
                version, 17,
                "rejection must report the actual declared version"
            )
        }
        other => panic!("expected SchemaTooNew for a v17 fixture, got {other:?}"),
    }
}
