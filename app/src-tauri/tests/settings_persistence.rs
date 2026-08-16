//! Settings persistence tests (D11-04, 11-01-PLAN.md Task 2): round trip,
//! missing/corrupt/truncated-file degradation to defaults, directory
//! creation on write, an unwritable-target typed error, and a structural
//! proof that `settings.rs` never references archive/session state
//! (T-11-02).
//!
//! Drives the directory-taking helpers directly -- integration tests cannot
//! obtain a `tauri::AppHandle` / packaged resource dir, so `settings.rs`
//! factors its load/save bodies into `load_settings_from_dir`/
//! `save_settings_to_dir`, the same reason the Phase 5/10 merge cores are
//! `pub`. Every test drives a `tempfile::TempDir` -- none touches a real OS
//! app-data directory (Established Patterns).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

use jwlmanager_lib::settings::{
    load_settings_from_dir, save_settings_to_dir, AppSettings, SettingsError, Theme,
    SETTINGS_FILE_NAME,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Writing a settings value then reading it back from the SAME directory
/// yields the identical language and theme.
#[test]
fn settings_round_trip() {
    let dir = TempDir::new().expect("tempdir");
    let original = AppSettings {
        language: "de".to_string(),
        theme: Theme::Light,
    };
    save_settings_to_dir(dir.path(), &original).expect("save must succeed");

    let loaded = load_settings_from_dir(dir.path());
    assert_eq!(loaded, original);
}

/// Reading from a directory with no settings file yields English + Dark and
/// does not error or panic.
#[test]
fn settings_missing_file_returns_defaults() {
    let dir = TempDir::new().expect("tempdir");

    let loaded = load_settings_from_dir(dir.path());

    assert_eq!(loaded, AppSettings::default());
    assert_eq!(loaded.language, "en");
    assert_eq!(loaded.theme, Theme::Dark);
}

/// A settings file containing invalid JSON, and separately one containing
/// valid JSON of the wrong shape, both yield the defaults without panicking.
#[test]
fn settings_corrupt_file_returns_defaults() {
    let invalid_json_dir = TempDir::new().expect("tempdir");
    fs::write(
        invalid_json_dir.path().join(SETTINGS_FILE_NAME),
        "{ not valid json !!!",
    )
    .expect("write invalid json");
    assert_eq!(
        load_settings_from_dir(invalid_json_dir.path()),
        AppSettings::default()
    );

    let wrong_shape_dir = TempDir::new().expect("tempdir");
    fs::write(
        wrong_shape_dir.path().join(SETTINGS_FILE_NAME),
        r#"{"unexpected":"shape"}"#,
    )
    .expect("write wrong-shape json");
    assert_eq!(
        load_settings_from_dir(wrong_shape_dir.path()),
        AppSettings::default()
    );
}

/// A settings file truncated mid-token yields the defaults.
#[test]
fn settings_truncated_file_returns_defaults() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(
        dir.path().join(SETTINGS_FILE_NAME),
        r#"{"language":"en","theme":"dar"#,
    )
    .expect("write truncated json");

    assert_eq!(load_settings_from_dir(dir.path()), AppSettings::default());
}

/// Saving into a directory tree that does not yet exist creates it and
/// succeeds.
#[test]
fn settings_write_creates_directory() {
    let base = TempDir::new().expect("tempdir");
    let target_dir = base.path().join("nested").join("settings-dir");
    assert!(!target_dir.exists());

    save_settings_to_dir(&target_dir, &AppSettings::default())
        .expect("save must create the directory tree");

    assert!(target_dir.exists());
    assert!(target_dir.join(SETTINGS_FILE_NAME).exists());
}

/// A save that cannot write returns the typed write-failure error rather
/// than panicking. Deterministic on every platform (no permission bits
/// required, so no skip-as-pass needed here): a regular FILE occupies the
/// path component the save needs to treat as a directory, so
/// `create_dir_all` must fail.
#[test]
fn settings_unwritable_target_returns_typed_error() {
    let base = TempDir::new().expect("tempdir");
    let blocking_file = base.path().join("blocked");
    fs::write(&blocking_file, b"not a directory").expect("write blocking file");
    let target_dir = blocking_file.join("subdir");

    let result = save_settings_to_dir(&target_dir, &AppSettings::default());

    assert!(
        matches!(result, Err(SettingsError::WriteFailed { .. })),
        "expected a typed WriteFailed error, got {result:?}"
    );
}

/// Structural proof of T-11-02 (settings domain -> archive domain boundary):
/// the settings module must name no archive/session type or field.
/// Forbidden identifiers are listed ONLY here, never in settings.rs itself,
/// so this assertion cannot be defeated by the words merely appearing in
/// that module's own prose.
#[test]
fn settings_source_has_no_archive_references() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/settings.rs");
    let source = fs::read_to_string(&source_path).expect("read settings.rs source");

    let forbidden = [
        "ArchiveSession",
        "archive::save",
        "session::",
        "target_path",
        "db_path",
        "temp_dir",
        "ArchiveError",
    ];
    for needle in forbidden {
        assert!(
            !source.contains(needle),
            "settings.rs must not reference `{needle}` -- settings must stay disjoint from archive/session state (T-11-02)"
        );
    }
}
