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
use rusqlite::Connection;
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
/// STATUS: **VERIFIED PASSING** on 2026-07-20 (Windows x64, Python 3.13.3,
/// PySide6 6.9.3, jwlCore v0.32.1) — same environment as the v16 oracle
/// above. A synthetic v14 fixture, upgraded to v16 by `open_and_validate`
/// and saved through the Tauri save path, was accepted by the Python app's
/// `check_validity`.
#[test]
#[ignore = "requires python3 + PySide6 (res/requirements.txt) + the win32 root-staged \
            jwlCore/sqlite3 DLLs; CI is a Rust-only matrix. VERIFIED PASSING locally \
            2026-07-20 — see this test's doc comment."]
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
///
/// D3-11 ACCEPTANCE GATE (03-03): this is the recorded manual gate for the
/// owner's real v14 archives — when `JWLM_REAL_ARCHIVE` is set, the round
/// trip above is followed by a Python `check_validity` acceptance assertion
/// (skipped, not failed, if python3 isn't on PATH). Run explicitly with:
///   `JWLM_REAL_ARCHIVE=<path to real .jwlibrary> cargo test --test differential`
/// or via the standalone helper:
///   `cargo run --example roundtrip -- <in> <out>` then manually invoke
///   `JWLManager.Window.check_validity(None, '<out>')`.
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

    // D3-11 acceptance gate: the owner's real archive, opened (upgrading
    // v14->v16 in-place per 03-02 if needed) and saved through the Tauri
    // save path, must be accepted by the Python app's own `check_validity` —
    // this is the actual proof the JW Library ecosystem takes what we wrote
    // back, not just that our own Rust code can reopen it. Guarded by
    // python3 availability (matching Task 1's tolerance): if python3 isn't
    // on PATH in this environment, skip visibly rather than fail the run —
    // never a silent pass.
    match Command::new("python3").arg("--version").output() {
        Ok(v) if v.status.success() => {
            let (ok, stdout, stderr) = run_python_check_validity(&scratch_target);
            assert!(
                ok,
                "Python app (JWLManager.check_validity) did not accept the owner's real \
                 archive after Tauri open+save.\nstdout: {stdout}\nstderr: {stderr}"
            );
            println!(
                "D3-11 acceptance gate: Python check_validity ACCEPTED the round-tripped \
                 real archive"
            );
        }
        _ => {
            eprintln!(
                "python3 not available on PATH — skipping the D3-11 Python check_validity \
                 acceptance assertion (Rust-only round trip above still ran and passed). \
                 Run `cargo test --test differential -- --ignored` (with res/requirements.txt \
                 installed) for the full manual gate, or see \
                 `cargo run --example roundtrip -- <in> <out>` + manual check_validity."
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 04-03: v16 -> v14 schema-downgrade differential oracle (D4-10)
// ---------------------------------------------------------------------------

/// Normalized state query (A2): a single line per surviving `Location`, keyed
/// by its v14-distinguishing columns (NOT its LocationId — the survivor id is
/// implementation-defined: Rust keeps the lowest id, Python keeps `ids[0]` in
/// query order, so literal ids legitimately differ), carrying the TOTAL count
/// of rows across all seven FK-bearing dependent tables that point at it.
///
/// Two downgraded databases are semantically equivalent iff this query returns
/// byte-identical text on both — same set of surviving v14 keys, same
/// dependent fan-in per key — regardless of which physical LocationId won each
/// merge. Runs unchanged in both rusqlite and Python's stdlib `sqlite3`.
const NORMALIZED_STATE_SQL: &str = "\
SELECT group_concat(line, char(10)) FROM ( \
  SELECT printf('%s|%s|%s|%s|%s|%s|%s|%s=%d', \
    ifnull(BookNumber,'~'), ifnull(ChapterNumber,'~'), ifnull(DocumentId,'~'), \
    ifnull(Track,'~'), ifnull(IssueTagNumber,'~'), ifnull(KeySymbol,'~'), \
    ifnull(MepsLanguage,'~'), ifnull(Type,'~'), \
      (SELECT count(*) FROM Bookmark b WHERE b.LocationId = l.LocationId) \
    + (SELECT count(*) FROM Bookmark b WHERE b.PublicationLocationId = l.LocationId) \
    + (SELECT count(*) FROM Note n WHERE n.LocationId = l.LocationId) \
    + (SELECT count(*) FROM UserMark u WHERE u.LocationId = l.LocationId) \
    + (SELECT count(*) FROM InputField i WHERE i.LocationId = l.LocationId) \
    + (SELECT count(*) FROM TagMap t WHERE t.LocationId = l.LocationId) \
    + (SELECT count(*) FROM PlaylistItemLocationMap p WHERE p.LocationId = l.LocationId) \
  ) AS line \
  FROM Location l ORDER BY line \
)";

/// Python replication of `JWLManager.py`'s `downgrade_schema` MERGE + Location
/// table rebuild (JWLManager.py:1172-1236). The real `downgrade_schema` is a
/// nested closure inside the Qt `save_file` method operating on the global
/// `TMP_PATH/DB_NAME` — it is NOT headlessly callable in isolation, and
/// importing `JWLManager` pulls in the full PySide6 stack (see the
/// module-level `#[ignore]` rationale). So this leg replicates that closure's
/// SQL VERBATIM against a caller-supplied db path using only stdlib `sqlite3`
/// (no PySide6, no jwlCore), which is exactly the algorithm under test for
/// semantic parity (A2). Kept byte-for-byte aligned with the app source.
const PY_DOWNGRADE_SCHEMA: &str = r#"
import sys, sqlite3
db = sys.argv[1]
con = sqlite3.connect(db)
groups = {}
for row in con.execute("SELECT LocationId, KeySymbol, IssueTagNumber, MepsLanguage, DocumentId, Track, Type FROM Location WHERE BookNumber IS NULL AND ChapterNumber IS NULL").fetchall():
    key = f"{row[1]}|{row[2]}|{row[3]}|{row[4]}|{row[5]}|{row[6]}"
    groups.setdefault(key, []).append(row[0])
for key, ids in groups.items():
    if len(ids) > 1:
        keep_id = ids[0]
        for old_id in ids[1:]:
            con.execute("UPDATE Bookmark SET LocationId = ? WHERE LocationId = ?", (keep_id, old_id))
            con.execute("UPDATE Bookmark SET PublicationLocationId = ? WHERE PublicationLocationId = ?", (keep_id, old_id))
            con.execute("UPDATE Note SET LocationId = ? WHERE LocationId = ?", (keep_id, old_id))
            con.execute("UPDATE UserMark SET LocationId = ? WHERE LocationId = ?", (keep_id, old_id))
            con.execute("UPDATE InputField SET LocationId = ? WHERE LocationId = ?", (keep_id, old_id))
            con.execute("UPDATE TagMap SET LocationId = ? WHERE LocationId = ?", (keep_id, old_id))
            con.execute("UPDATE PlaylistItemLocationMap SET LocationId = ? WHERE LocationId = ?", (keep_id, old_id))
            con.execute("DELETE FROM Location WHERE LocationId = ?", (old_id,))
con.commit()
con.close()
"#;

/// Runs the given SQL that returns a single (possibly NULL) text column and
/// yields the string (empty if NULL).
fn query_single_text(conn: &Connection, sql: &str) -> String {
    conn.query_row(sql, [], |r| r.get::<_, Option<String>>(0))
        .expect("normalized-state query must succeed")
        .unwrap_or_default()
}

/// D4-10 differential oracle A: a Rust-downgraded v14 archive is accepted by
/// the Python app's own `check_validity`. Mirrors
/// [`python_app_opens_upgraded_v14_archive`] but exercises the DOWNGRADE path
/// (`save_v14_copy`) instead of the upgrade path.
///
/// `#[ignore]`d for the same RECORDED-MANUAL-GATE reason as the other two
/// oracles: `check_validity` requires the full PySide6 stack + the win32
/// root-staged jwlCore/sqlite3 DLLs, and CI (`app-ci.yml`) is a Rust-only
/// matrix. Run explicitly with `cargo test --test differential -- --ignored`.
///
/// STATUS: **NOT-YET-VERIFIED** in the 04-03 execution environment — PySide6
/// is not installed here (`python3 -c "import PySide6"` -> ModuleNotFoundError),
/// so the `check_validity` leg cannot be exercised. The normalized-equivalence
/// leg ([`rust_downgrade_matches_python_downgrade_normalized`]) DID run and
/// passed (stdlib sqlite3 only). Re-run this gate locally with
/// `res/requirements.txt` installed to flip this to VERIFIED PASSING.
#[test]
#[ignore = "requires python3 + PySide6 (res/requirements.txt) + the win32 root-staged \
            jwlCore/sqlite3 DLLs; CI is a Rust-only matrix. NOT-YET-VERIFIED in the \
            04-03 env (PySide6 absent) — see this test's doc comment."]
fn python_app_opens_downgraded_v14_archive() {
    let (_fixture_dir, archive_path) = common::generate_v16_collision_fixture();
    let (session, _notes) = open_and_validate(&archive_path, &dev_resources_db_path())
        .expect("open_and_validate must succeed on the v16 collision fixture");

    let scratch = tempfile::TempDir::new().expect("scratch tempdir");
    let target = scratch.path().join("downgraded_v14.jwlibrary");
    jwlmanager_lib::archive::downgrade::save_v14_copy(
        &session,
        &target,
        "JWL Manager",
        "JWL Manager_test",
        "2026-01-02T00:00:00Z",
    )
    .expect("save_v14_copy must succeed before handing off to the Python oracle");

    // The on-disk downgraded userData.db must actually be v14 before hand-off.
    let extract = tempfile::TempDir::new().expect("extract tempdir");
    let db_path = extract_userdata_db(&target, extract.path());
    let conn = Connection::open(&db_path).expect("open downgraded userData.db");
    let user_version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .expect("read user_version");
    assert_eq!(
        user_version, 14,
        "save_v14_copy output must be at user_version 14 before the Python oracle"
    );
    drop(conn);

    let (ok, stdout, stderr) = run_python_check_validity(&target);
    assert!(
        ok,
        "Python app (JWLManager.check_validity) did not accept the Rust-downgraded v14 \
         archive.\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// D4-10 differential oracle B (A2 normalized equivalence): the Rust downgrade
/// (`downgrade_to_v14`) and the Python app's own `downgrade_schema` MERGE,
/// applied to the SAME v16 collision fixture, produce SEMANTICALLY equivalent
/// v14 state — identical set of surviving v14 Location keys with identical
/// dependent fan-in per key ([`NORMALIZED_STATE_SQL`]) — even though each
/// implementation keeps a different physical survivor LocationId (Rust: lowest
/// id; Python: `ids[0]` in query order). We NEVER compare literal survivor ids
/// and NEVER byte-diff the databases.
///
/// This leg needs ONLY stdlib `sqlite3` on both sides (no PySide6, no
/// jwlCore), so it runs in this environment. It is python3-gated (skips
/// visibly, never fails, when python3 is absent) exactly like
/// [`real_archive_round_trip_env_gated`], so a Rust-only CI runner without
/// python3 does not spuriously fail.
///
/// STATUS: **VERIFIED PASSING** on 2026-07-22 (Python 3.13.3, stdlib sqlite3).
/// Rust `downgrade_to_v14` and the replicated Python `downgrade_schema` merge
/// produced identical normalized state on the 3-way collision fixture.
#[test]
fn rust_downgrade_matches_python_downgrade_normalized() {
    if !python3_available() {
        eprintln!(
            "python3 not on PATH — skipping the Rust/Python normalized-downgrade equivalence \
             leg (expected on a Rust-only CI runner)."
        );
        return;
    }

    // Rust leg: downgrade a fresh collision db in-process.
    let (_rust_dir, rust_db) = common::generate_v16_collision_db();
    {
        let mut conn = Connection::open(&rust_db).expect("open rust collision db");
        jwlmanager_lib::archive::downgrade::downgrade_to_v14(&mut conn)
            .expect("Rust downgrade_to_v14 must succeed");
    }
    let rust_conn = Connection::open(&rust_db).expect("reopen rust downgraded db");
    let rust_normalized = query_single_text(&rust_conn, NORMALIZED_STATE_SQL);

    // Python leg: run the replicated app downgrade_schema merge on an
    // independently-generated identical collision db, then read the SAME
    // normalized query back in-process.
    let (_py_dir, py_db) = common::generate_v16_collision_db();
    let out = Command::new("python3")
        .arg("-c")
        .arg(PY_DOWNGRADE_SCHEMA)
        .arg(py_db.to_string_lossy().as_ref())
        .output()
        .expect("failed to invoke python3");
    assert!(
        out.status.success(),
        "python downgrade_schema replication failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let py_conn = Connection::open(&py_db).expect("reopen python downgraded db");
    let py_normalized = query_single_text(&py_conn, NORMALIZED_STATE_SQL);

    assert!(
        !rust_normalized.is_empty(),
        "normalized state must be non-empty (fixture has Locations)"
    );
    assert_eq!(
        rust_normalized, py_normalized,
        "Rust and Python downgrades must be normalized-equivalent (same surviving v14 keys + \
         dependent fan-in), independent of which physical survivor LocationId each kept.\n\
         Rust:\n{rust_normalized}\nPython:\n{py_normalized}"
    );
}

/// True if `python3 --version` runs successfully.
fn python3_available() -> bool {
    matches!(Command::new("python3").arg("--version").output(), Ok(o) if o.status.success())
}

/// Extracts `userData.db` from a `.jwlibrary` archive into `dest_dir` and
/// returns its path (test helper for the v14 PRAGMA assertion).
fn extract_userdata_db(archive: &Path, dest_dir: &Path) -> std::path::PathBuf {
    let file = std::fs::File::open(archive).expect("open archive");
    let mut zip = zip::ZipArchive::new(file).expect("read archive as zip");
    let mut entry = zip.by_name("userData.db").expect("archive has userData.db");
    let out = dest_dir.join("userData.db");
    let mut writer = std::fs::File::create(&out).expect("create extracted db");
    std::io::copy(&mut entry, &mut writer).expect("extract userData.db");
    out
}
