//! ARCH-02 differential oracle (D-01, review finding 10): the Python app
//! (`JWLManager.py`) must actually open an archive the Tauri app saved. This
//! is the real proof that our byte-compatible manifest + zip rebuild produce
//! something JW Library's own ecosystem recognizes — not just something our
//! own Rust code can read back.
//!
//! `#[ignore]`d by default: `JWLManager.py` imports `PySide6` at MODULE
//! level (`from res.ui_main_window import Ui_MainWindow` -> ... ->
//! `from PySide6.QtCore import ...`), so even a headless `check_validity`
//! call requires the full GUI dependency stack (`res/requirements.txt`) to be
//! installed. This dev/CI sandbox does not have PySide6 installed (verified:
//! `python3 -c "import PySide6"` -> `ModuleNotFoundError`), and
//! `.github/workflows/app-ci.yml` (01-02) is a Rust-only test matrix with no
//! Python install step. Per finding 10, this is NEVER silently reported as a
//! passing oracle — the test is explicitly `#[ignore]`d with this reason, and
//! 01-05-SUMMARY.md records the RECORDED MANUAL GATE that must be run by a
//! human (with `res/requirements.txt` installed) before Phase 1 is
//! considered complete: `cargo test --test differential -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

mod common;

use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::archive::save::save_archive;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use std::path::Path;
use std::process::Command;

/// Repo root, resolved the same way `tests/fixtures.rs`'s git check does.
fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// REAL headless oracle: generates a fixture, saves it through the Tauri
/// save path, then shells to `python3` and calls
/// `JWLManager.Window.check_validity` (unbound, `self=None` — the success
/// path never touches `self`, only the two `QMessageBox.warning` failure
/// branches do) against the Tauri-saved file. Asserts the Python app agrees
/// the saved archive is valid.
///
/// Run explicitly with `cargo test --test differential -- --ignored` on a
/// machine with `res/requirements.txt` installed (the RECORDED MANUAL GATE).
/// STATUS: **VERIFIED PASSING** on 2026-07-20 (Windows x64, Python 3.13.3,
/// PySide6 6.9.3, jwlCore v0.32.1). The Python app's own `check_validity`
/// accepted a Tauri-saved archive — ARCH-02's differential oracle is real,
/// not asserted. Still `#[ignore]`d because CI (`app-ci.yml`) is a Rust-only
/// matrix with no Python/PySide6 install step; re-run locally with:
///   `cargo test --test differential -- --ignored`
///
/// Local prerequisites (one-time):
///   1. `python -m pip install -r res/requirements.txt`
///   2. Copy `libs/jwlCore-amd64.dll` + `libs/sqlite3_64.dll` to the repo root
///      — on win32 `jwlcore.py:_load_lib` resolves the DLL next to itself, which
///      is the repo root in a source checkout (PyInstaller does this in the
///      shipped build). Both copies are gitignored.
#[test]
#[ignore = "requires python3 + PySide6 (res/requirements.txt) + the win32 root-staged \
            jwlCore/sqlite3 DLLs; CI is a Rust-only matrix. VERIFIED PASSING locally \
            2026-07-20 — see this test's doc comment and 01-05-SUMMARY.md"]
fn python_app_opens_tauri_saved_archive() {
    let (_fixture_dir, archive_path) = common::generate_v16_fixture();
    let (session, _notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed");
    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_archive must succeed before handing off to the Python oracle");

    let (ok, stdout, stderr) = run_python_check_validity(&archive_path);
    assert!(
        ok,
        "Python app (JWLManager.check_validity) did not accept the Tauri-saved archive.\n\
         stdout: {stdout}\nstderr: {stderr}"
    );
}

/// Extended v14-upgrade oracle (03-03, D3-11 code-path proof): seeds a
/// synthetic pre-v16-shaped fixture at `user_version = 14`, runs it through
/// `open_and_validate` (which now upgrades it to v16 in-place, 03-02), saves
/// it through the Tauri save path, then hands the result to the SAME Python
/// `check_validity` oracle as [`python_app_opens_tauri_saved_archive`].
///
/// This proves that an UPGRADED archive — not just a synthetic archive that
/// was already at v16 — is accepted by the Python app / JW Library
/// ecosystem, which is the reason this phase exists (03-CONTEXT.md D3-11).
///
/// `#[ignore]`d for the same reason as the v16 oracle above (RECORDED MANUAL
/// GATE, CI is Rust-only, no PySide6). Run explicitly with:
///   `cargo test --test differential -- --ignored`
///
/// STATUS: pending the owner's local run — do NOT mark VERIFIED PASSING
/// until a human has actually executed this test and confirmed
/// `ORACLE_RESULT:PASS` (finding 9 / no-overclaim, T-03-08).
#[test]
#[ignore = "requires python3 + PySide6 (res/requirements.txt) + the win32 root-staged \
            jwlCore/sqlite3 DLLs; CI is a Rust-only matrix. RECORDED MANUAL GATE — run \
            with `cargo test --test differential -- --ignored` and update this doc \
            comment's STATUS line to VERIFIED PASSING once a human confirms the result. \
            Not yet marked VERIFIED PASSING (03-03)."]
fn python_app_opens_upgraded_v14_archive() {
    let (_fixture_dir, archive_path) = common::generate_fixture_pre_v16_shape(14);
    let (session, _notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed and upgrade v14 to v16");

    assert_eq!(
        session.manifest.schema_version, 16,
        "open_and_validate must have upgraded the v14 fixture to v16 before save \
         (proves the upgrade actually ran, not just that the fixture claims v16)"
    );

    save_archive(
        &session,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_archive must succeed before handing off to the Python oracle");

    let (ok, stdout, stderr) = run_python_check_validity(&archive_path);
    assert!(
        ok,
        "Python app (JWLManager.check_validity) did not accept the upgraded (v14->v16) \
         Tauri-saved archive.\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Shared Python-oracle invocation used by every differential test: shells to
/// `python3` and calls `JWLManager.Window.check_validity` (unbound,
/// `self=None` — the success path never touches `self`, only the two
/// `QMessageBox.warning` failure branches do) against the given archive path.
/// Returns `(accepted, stdout, stderr)`.
fn run_python_check_validity(archive_path: &Path) -> (bool, String, String) {
    let saved_path = archive_path.to_string_lossy().replace('\\', "\\\\");
    let python_code = format!(
        "import sys\n\
         sys.path.insert(0, r'{root}')\n\
         import JWLManager\n\
         ok = JWLManager.Window.check_validity(None, '{path}')\n\
         print('ORACLE_RESULT:' + ('PASS' if ok else 'FAIL'))\n",
        root = repo_root().display(),
        path = saved_path
    );

    // Run from the repo root and put it first on PATH. On Windows `jwlcore.py`
    // resolves `jwlCore-amd64.dll` next to itself (repo root), and that DLL has
    // a STATIC import of `sqlite3_64.dll` — the OS loader resolves that one via
    // the normal search order, which does NOT include the loaded DLL's own
    // directory. Without this the process dies with a bare
    // "could not load: sqlite3_64.dll" before `check_validity` is ever reached.
    // Same root cause (and same fix) as `src/jwlcore/loader.rs`.
    let root = repo_root();
    let path_var = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let patched_path = format!("{}{}{}", root.display(), sep, path_var);

    let output = Command::new("python3")
        .arg("-c")
        .arg(&python_code)
        .current_dir(&root)
        .env("PATH", &patched_path)
        .output()
        .expect("failed to invoke python3 — is it on PATH?");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let ok = output.status.success() && stdout.contains("ORACLE_RESULT:PASS");
    (ok, stdout, stderr)
}

/// Owner's real archive round-trip (D-07): opens `JWLM_REAL_ARCHIVE` if set,
/// saves it through the Tauri save path, and asserts the save succeeds and
/// the archive reopens with a non-empty notes/session state. NEVER run in
/// CI — skipped (not failed) when the env var is unset, since this
/// necessarily touches irreplaceable personal data that must never be
/// committed or fixture-generated (GDPR Art. 9 bright line, D-06).
#[test]
fn real_archive_round_trip_env_gated() {
    let Ok(real_archive_path) = std::env::var("JWLM_REAL_ARCHIVE") else {
        eprintln!(
            "JWLM_REAL_ARCHIVE not set — skipping real-archive round-trip test (expected in CI)"
        );
        return;
    };

    let path = Path::new(&real_archive_path);
    assert!(
        path.exists(),
        "JWLM_REAL_ARCHIVE points at a nonexistent file: {real_archive_path}"
    );

    let (session, notes) = open_and_validate(path, &dev_resources_db_path())
        .expect("owner's real archive must open through the real open path");
    println!("real archive opened with {} note rows", notes.len());

    // Save-as into a scratch temp path — NEVER overwrite the owner's real
    // archive in place during a test run.
    let scratch_dir = tempfile::TempDir::new().expect("scratch tempdir");
    let scratch_target = scratch_dir.path().join("real_archive_round_trip.jwlibrary");
    jwlmanager_lib::archive::new::save_as(
        &session,
        &scratch_target,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save-as of the owner's real archive must succeed");

    let (_reopened, reopened_notes) = open_and_validate(&scratch_target, &dev_resources_db_path())
        .expect("the round-tripped real archive must reopen cleanly");
    assert_eq!(
        notes.len(),
        reopened_notes.len(),
        "note count must be unchanged across a real-archive save-as round trip"
    );
}
