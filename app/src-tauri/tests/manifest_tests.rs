//! Byte-compat + field-order + v16-only assertions for `archive::manifest`
//! (ARCH-03, TDD RED for Task 1 of 01-02-PLAN.md).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

use jwlmanager_lib::archive::manifest::{check_validity, compute_hash, Manifest};
use jwlmanager_lib::error::ArchiveError;
use std::io::Write;

/// A known-good v16 manifest, hand-typed in the exact Python field order,
/// used as the fixture for the byte-exact serialization assertion below.
fn known_manifest_json() -> &'static str {
    r#"{"name":"JWL Manager","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0,"userDataBackup":{"lastModifiedDate":"2026-01-01T00:00:00Z","deviceName":"JWL Manager_0.1.0","databaseName":"userData.db","hash":"deadbeef","schemaVersion":16}}"#
}

#[test]
fn test_serialization_is_exact_compact_python_field_order() {
    let manifest = Manifest::parse(known_manifest_json().as_bytes()).expect("must parse");
    let serialized = manifest.to_compact_string().expect("must serialize");

    assert_eq!(
        serialized,
        known_manifest_json(),
        "serialized manifest must byte-match Python's compact, ordered form"
    );
    assert!(
        !serialized.contains(": ") && !serialized.contains(", "),
        "compact separators=(',', ':') form must have no structural whitespace after separators"
    );
}

#[test]
fn test_unknown_top_level_and_nested_keys_survive_round_trip() {
    let with_unknown = r#"{"name":"JWL Manager","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0,"userDataBackup":{"lastModifiedDate":"2026-01-01T00:00:00Z","deviceName":"JWL Manager_0.1.0","databaseName":"userData.db","hash":"deadbeef","schemaVersion":16,"futureNestedKey":"nested-value"},"futureTopLevelKey":"top-value"}"#;

    let manifest = Manifest::parse(with_unknown.as_bytes()).expect("must parse");
    assert_eq!(
        manifest
            .extra
            .get("futureTopLevelKey")
            .and_then(|v| v.as_str()),
        Some("top-value"),
        "unknown top-level key must survive parse"
    );
    assert_eq!(
        manifest
            .user_data_backup
            .extra
            .get("futureNestedKey")
            .and_then(|v| v.as_str()),
        Some("nested-value"),
        "unknown userDataBackup key must survive parse"
    );

    let round_tripped = manifest.to_compact_string().expect("must serialize");
    assert!(
        round_tripped.contains(r#""futureTopLevelKey":"top-value""#),
        "unknown top-level key must survive write: {round_tripped}"
    );
    assert!(
        round_tripped.contains(r#""futureNestedKey":"nested-value""#),
        "unknown nested key must survive write: {round_tripped}"
    );
}

#[test]
fn test_type_confused_schema_version_is_rejected_not_coerced() {
    let bad_type = r#"{"name":"JWL Manager","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0,"userDataBackup":{"lastModifiedDate":"2026-01-01T00:00:00Z","deviceName":"JWL Manager_0.1.0","databaseName":"userData.db","hash":"deadbeef","schemaVersion":"16"}}"#;

    let result = Manifest::parse(bad_type.as_bytes());
    assert!(
        result.is_err(),
        "a JSON string schemaVersion must be a parse error, not silently coerced to i64"
    );
    match result {
        Err(ArchiveError::Json(_)) => {}
        other => panic!("expected ArchiveError::Json, got {other:?}"),
    }
}

#[test]
fn test_check_validity_accepts_v14_and_v16_rejects_out_of_range() {
    let v14 = known_manifest_json().replace("\"schemaVersion\":16", "\"schemaVersion\":14");
    let v14_result = check_validity(v14.as_bytes());
    assert!(
        v14_result.is_ok(),
        "a v14 manifest must now be accepted (12-16 range, SCHEMA-01/02): {v14_result:?}"
    );

    let v16_result = check_validity(known_manifest_json().as_bytes());
    assert!(
        v16_result.is_ok(),
        "a v16 manifest must be accepted: {v16_result:?}"
    );

    let v11 = known_manifest_json().replace("\"schemaVersion\":16", "\"schemaVersion\":11");
    let v11_result = check_validity(v11.as_bytes());
    match v11_result {
        Err(ArchiveError::SchemaTooOld { version }) => assert_eq!(version, 11),
        other => panic!("expected SchemaTooOld {{ version: 11 }}, got {other:?}"),
    }

    let v17 = known_manifest_json().replace("\"schemaVersion\":16", "\"schemaVersion\":17");
    let v17_result = check_validity(v17.as_bytes());
    match v17_result {
        Err(ArchiveError::SchemaTooNew { version }) => assert_eq!(version, 17),
        other => panic!("expected SchemaTooNew {{ version: 17 }}, got {other:?}"),
    }
}

#[test]
fn test_check_validity_rejects_missing_user_data_backup() {
    let missing =
        r#"{"name":"JWL Manager","creationDate":"2026-01-01T00:00:00Z","version":1,"type":0}"#;
    let result = check_validity(missing.as_bytes());
    assert!(
        matches!(result, Err(ArchiveError::MissingUserDataBackup)),
        "a manifest with no userDataBackup key must be MissingUserDataBackup, got {result:?}"
    );
}

#[test]
fn test_compute_hash_matches_known_sha256_of_file_bytes() {
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    file.write_all(b"hello world").expect("write");
    file.flush().expect("flush");

    let hash = compute_hash(file.path()).expect("compute_hash must succeed");
    // sha256("hello world") — a well-known test vector.
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}
