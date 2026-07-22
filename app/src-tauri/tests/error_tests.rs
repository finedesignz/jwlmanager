//! Proves the boundary `ErrorDto` never leaks raw filesystem paths or the
//! wrapped source error's `Display`, and that a malformed/non-archive input
//! reaches this DTO as a typed error, never a panic (D-14, SAFE-05,
//! finding 6 in 01-07-PLAN.md).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use jwlmanager_lib::error::ArchiveError;

#[test]
fn malformed_input_produces_sanitized_error_dto() {
    let err = ArchiveError::NotAZip;
    let dto = err.to_dto(
        "open_archive",
        Some(std::path::Path::new(
            "/home/user/secret-study-notes/archive.jwlibrary",
        )),
    );

    assert!(!dto.message_key.is_empty());
    assert_eq!(dto.code, "not_a_zip");
    assert_eq!(dto.safe_file_name.as_deref(), Some("archive.jwlibrary"));

    // The DTO must be serializable and must never contain the absolute path.
    let json = serde_json::to_string(&dto).expect("ErrorDto must serialize");
    assert!(!json.contains("/home/user/secret-study-notes"));
}

#[test]
fn schema_too_old_and_too_new_report_distinct_codes() {
    let too_old = ArchiveError::SchemaTooOld { version: 11 };
    let too_old_dto = too_old.to_dto("open_archive", None);
    assert_eq!(too_old_dto.code, "schema_too_old");
    assert!(!too_old_dto.message_key.is_empty());
    assert_eq!(too_old_dto.safe_file_name, None);

    let too_new = ArchiveError::SchemaTooNew { version: 17 };
    let too_new_dto = too_new.to_dto("open_archive", None);
    assert_eq!(too_new_dto.code, "schema_too_new");
    assert!(!too_new_dto.message_key.is_empty());
    assert_ne!(
        too_old_dto.code, too_new_dto.code,
        "too-old and too-new must be distinct codes"
    );
}

#[test]
fn schema_upgrade_failed_never_leaks_reason_into_dto() {
    let err = ArchiveError::SchemaUpgradeFailed {
        reason: "internal SQL detail that must not cross IPC".to_string(),
    };
    let dto = err.to_dto("open_archive", None);
    assert_eq!(dto.code, "schema_upgrade_failed");
    let json = serde_json::to_string(&dto).expect("ErrorDto must serialize");
    assert!(
        !json.contains("internal SQL detail"),
        "the internal reason must never leak into the DTO"
    );
}

#[test]
fn schema_downgrade_failed_never_leaks_reason_into_dto() {
    let err = ArchiveError::SchemaDowngradeFailed {
        reason: "internal SQL detail that must not cross IPC".to_string(),
    };
    let dto = err.to_dto("downgrade_archive", None);
    assert_eq!(dto.code, "schema_downgrade_failed");
    let json = serde_json::to_string(&dto).expect("ErrorDto must serialize");
    assert!(
        !json.contains("internal SQL detail"),
        "the internal reason must never leak into the DTO"
    );
}

/// End-to-end: opening a genuinely non-zip file must surface as a typed
/// `ArchiveError` (never panic), and that error must map cleanly to a
/// sanitized DTO with no path/source leakage.
#[test]
fn opening_a_non_archive_file_returns_a_typed_error_not_a_panic() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let not_a_zip = dir.path().join("not-an-archive.jwlibrary");
    std::fs::write(&not_a_zip, b"this is definitely not a zip file").expect("write bogus file");

    let result = open_and_validate(&not_a_zip, &dev_resources_db_path());
    let err = result.expect_err("a non-zip file must be rejected, not silently accepted");
    assert!(matches!(err, ArchiveError::NotAZip));

    let dto = err.to_dto("open_archive", Some(&not_a_zip));
    let json = serde_json::to_string(&dto).expect("ErrorDto must serialize");
    assert!(!json.contains(&dir.path().to_string_lossy().to_string()));
}
