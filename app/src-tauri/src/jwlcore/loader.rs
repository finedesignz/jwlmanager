//! Arch-aware `(OS, ARCH)` jwlCore binary selection + lazy libloading bind
//! (D-12, D-13, D-13a, finding 12, T-03-01).
//!
//! Fixes the Python bridge's arch-blind bug (`jwlcore.py:29-38` selects on
//! `sys.platform` alone, silently assuming x86_64 everywhere). This module
//! selects by BOTH OS and CPU architecture via `std::env::consts::{OS,ARCH}`.
//!
//! Scope is strictly LOAD + RESOLVE SYMBOLS + read version (D-12). The
//! `mergeDatabase` symbol is resolved to prove ABI compatibility but is
//! NEVER invoked here.

use crate::error::JwlCoreError;
use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

/// Unified status shape returned by `check_jwlcore` (finding 12). A missing
/// or wrong-arch binary is represented as `loaded: false` with a `reason` —
/// this is an `Ok` value, never an `Err`. `Err(JwlCoreError)` is reserved for
/// a genuinely unexpected load fault (e.g. a present-but-corrupt binary).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/JwlCoreStatus.ts")]
pub struct JwlCoreStatus {
    pub loaded: bool,
    pub arch: String,
    pub version: Option<String>,
    pub reason: Option<String>,
}

/// Reasons a `(OS, ARCH)` combination has no loadable binary. Distinct from
/// `JwlCoreError`: these feed a non-loaded `JwlCoreStatus`, not an `Err`.
enum NoBinaryReason {
    Arm64Windows,
    Unsupported,
}

impl NoBinaryReason {
    fn message(&self) -> &'static str {
        match self {
            NoBinaryReason::Arm64Windows => "no binary for aarch64-windows",
            NoBinaryReason::Unsupported => "unsupported platform/architecture",
        }
    }
}

/// Selects the expected jwlCore binary FILE NAME for the current
/// `(env::consts::OS, env::consts::ARCH)`. Returns `Err(NoBinaryReason)` for
/// combinations with no shipped binary — the arm64-windows gap (D-13a) is a
/// first-class case here, not an afterthought.
fn resolve_lib_name(os: &str, arch: &str) -> Result<&'static str, NoBinaryReason> {
    match (os, arch) {
        ("windows", "x86_64") => Ok("jwlCore-amd64.dll"),
        ("windows", "aarch64") => Err(NoBinaryReason::Arm64Windows),
        ("linux", "x86_64") => Ok("libjwlCore-x86_64.so"),
        ("linux", "aarch64") => Ok("libjwlCore-arm64.so"),
        ("macos", _) => Ok("libjwlCore.dylib"),
        _ => Err(NoBinaryReason::Unsupported),
    }
}

/// Repo-root `libs/` dir when running from source (dev / `cargo test`).
/// `CARGO_MANIFEST_DIR` is `<repo>/app/src-tauri` at compile time, so `libs/`
/// lives two levels up.
fn dev_libs_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../libs"))
}

/// Resolves the on-disk path to `name`. Prefers the dev-tree `libs/` (source
/// checkout, `cargo test`/`cargo tauri dev`); falls back to the Tauri
/// bundled resource directory (`libs/<name>`, declared in `tauri.conf.json`)
/// when running from a packaged build and the dev path doesn't exist.
fn resolve_lib_path(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, JwlCoreError> {
    let dev_path = dev_libs_dir().join(name);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    use tauri::Manager;
    app.path()
        .resolve(format!("libs/{name}"), tauri::path::BaseDirectory::Resource)
        .map_err(|_| JwlCoreError::PathResolutionFailed)
}

/// Loads the library at `path`, altering the DLL search path on Windows so
/// dependent DLLs co-located with jwlCore (namely `sqlite3_64.dll`, shipped
/// alongside it in `libs/`) resolve correctly. Plain `libloading::Library::new`
/// on Windows uses the default DLL search order, which does NOT include the
/// loaded DLL's own directory — without this flag, loading
/// `jwlCore-amd64.dll` fails with "could not load: sqlite3_64.dll" even
/// though that file sits right next to it.
/// `LOAD_WITH_ALTERED_SEARCH_PATH` alone was tried first and did NOT resolve
/// `sqlite3_64.dll` in practice on this host (jwlCore-amd64.dll statically
/// imports it, and the flag's directory-search semantics did not cover it
/// here) — the OS loader hard-terminates the whole process with
/// "could not load: sqlite3_64.dll" printed to stdout, bypassing normal
/// Rust `Result` error handling entirely. The reliable fix is to prepend the
/// binary's own directory to `PATH` for the duration of the load, since PATH
/// directories are part of the standard DLL search order and this covers
/// jwlCore's statically-imported dependency correctly.
#[cfg(target_os = "windows")]
fn load_library(path: &std::path::Path) -> Result<libloading::Library, libloading::Error> {
    let dir = path.parent().map(|p| p.to_path_buf());
    let original_path = std::env::var_os("PATH");

    if let Some(dir) = &dir {
        let mut new_path = std::ffi::OsString::from(dir);
        if let Some(existing) = &original_path {
            new_path.push(";");
            new_path.push(existing);
        }
        std::env::set_var("PATH", &new_path);
    }

    // SAFETY: loading a vendored, trusted binary shipped in this repo's
    // libs/ (see the top-level SAFETY note on `check_jwlcore`'s call site).
    let result = unsafe { libloading::Library::new(path) };

    if let Some(original) = original_path {
        std::env::set_var("PATH", original);
    }

    result
}

#[cfg(not(target_os = "windows"))]
fn load_library(path: &std::path::Path) -> Result<libloading::Library, libloading::Error> {
    // SAFETY: loading a vendored, trusted binary shipped in this repo's
    // libs/ (see the top-level SAFETY note on `check_jwlcore`'s call site).
    unsafe { libloading::Library::new(path) }
}

/// Four jwlCore FFI symbols the loader resolves to prove ABI compatibility.
/// Only `getCoreVersion` is ever CALLED here (D-12) — `mergeDatabase` is
/// resolved-but-not-invoked, mirroring `jwlcore.py:64-71`'s declared surface.
const EXPECTED_SYMBOLS: [&str; 4] = [
    "setProgressCallback",
    "mergeDatabase",
    "getLastResult",
    "getCoreVersion",
];

/// Lazily loads jwlCore for the current `(OS, ARCH)`, resolves its expected
/// symbols, and reads its version via `getCoreVersion` ONLY (never
/// `mergeDatabase` — D-12). Must be invoked from a command called by the
/// frontend after mount, NOT from Tauri `setup()` (Pitfall 4) — a
/// missing/wrong-arch binary must render a status, never crash launch.
#[tauri::command]
pub fn check_jwlcore(app: tauri::AppHandle) -> Result<JwlCoreStatus, JwlCoreError> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let name = match resolve_lib_name(os, arch) {
        Ok(name) => name,
        Err(reason) => {
            return Ok(JwlCoreStatus {
                loaded: false,
                arch: arch.to_string(),
                version: None,
                reason: Some(reason.message().to_string()),
            });
        }
    };

    let lib_path = resolve_lib_path(&app, name)?;

    // `lib_path` is a vendored jwlCore binary shipped in this repo's `libs/`
    // (or the equivalent bundled Tauri resource) — not attacker-controlled
    // input. Loading it is the entire purpose of this module (T-03-02
    // accepts vendored-binary trust; see 01-03-PLAN.md threat model).
    let library = load_library(&lib_path).map_err(|e| JwlCoreError::LoadFailed {
        reason: e.to_string(),
    })?;

    for symbol in EXPECTED_SYMBOLS {
        // SAFETY: resolving (not calling) a symbol by name from the library
        // just loaded above; symbol names are the four fixed FFI entry
        // points documented in `jwlcore.py:61-71`.
        let result: Result<libloading::Symbol<'_, unsafe extern "C" fn()>, _> =
            unsafe { library.get(format!("{symbol}\0").as_bytes()) };
        result.map_err(|_| JwlCoreError::MissingSymbol {
            symbol: symbol.to_string(),
        })?;
    }

    // SAFETY: `getCoreVersion` was just resolved above; its declared ABI
    // (`jwlcore.py:70-71`) is `() -> *const c_char` (nullable), matched here
    // exactly. This is the ONLY jwlCore function this loader ever calls
    // (D-12) — `mergeDatabase` is deliberately resolved-but-not-invoked.
    let version: Result<
        libloading::Symbol<'_, unsafe extern "C" fn() -> *const std::os::raw::c_char>,
        _,
    > = unsafe { library.get(b"getCoreVersion\0") };
    let version_str = match version {
        Ok(get_core_version) => {
            // SAFETY: calling the resolved `getCoreVersion` with no
            // arguments, per its declared zero-arg ABI. The returned pointer
            // is a `char*` owned by the native library; it is only read
            // (never freed) here, matching `jwlcore.py:77-79`'s handling.
            let ptr = unsafe { get_core_version() };
            if ptr.is_null() {
                None
            } else {
                // SAFETY: `ptr` was just checked non-null and originates
                // from `getCoreVersion`'s documented null-terminated C
                // string return.
                let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };
                Some(c_str.to_string_lossy().into_owned())
            }
        }
        Err(_) => {
            return Err(JwlCoreError::MissingSymbol {
                symbol: "getCoreVersion".to_string(),
            })
        }
    };

    Ok(JwlCoreStatus {
        loaded: true,
        arch: arch.to_string(),
        version: version_str,
        reason: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn jwlcore_resolve_windows_x86_64() {
        assert_eq!(
            resolve_lib_name("windows", "x86_64").ok(),
            Some("jwlCore-amd64.dll")
        );
    }

    #[test]
    fn jwlcore_resolve_windows_aarch64_no_binary() {
        let err = resolve_lib_name("windows", "aarch64").expect_err("must have no binary");
        assert_eq!(err.message(), "no binary for aarch64-windows");
    }

    #[test]
    fn jwlcore_resolve_linux_x86_64() {
        assert_eq!(
            resolve_lib_name("linux", "x86_64").ok(),
            Some("libjwlCore-x86_64.so")
        );
    }

    #[test]
    fn jwlcore_resolve_linux_aarch64() {
        assert_eq!(
            resolve_lib_name("linux", "aarch64").ok(),
            Some("libjwlCore-arm64.so")
        );
    }

    #[test]
    fn jwlcore_resolve_macos_x86_64() {
        assert_eq!(
            resolve_lib_name("macos", "x86_64").ok(),
            Some("libjwlCore.dylib")
        );
    }

    #[test]
    fn jwlcore_resolve_macos_aarch64() {
        assert_eq!(
            resolve_lib_name("macos", "aarch64").ok(),
            Some("libjwlCore.dylib")
        );
    }

    #[test]
    fn jwlcore_resolve_unsupported_combo() {
        let err = resolve_lib_name("freebsd", "x86_64").expect_err("must be unsupported");
        assert_eq!(err.message(), "unsupported platform/architecture");
    }

    /// Real, non-mocked load of the on-host jwlCore binary for the CURRENT
    /// `(OS, ARCH)` this test suite runs under (D-12 scope: load + resolve
    /// symbols + version only, never `mergeDatabase`). Skips itself as a
    /// pass when the current target has no binary (e.g. would only fire on
    /// an aarch64-windows CI runner).
    #[test]
    fn jwlcore_status_real_load_current_host() {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let name = match resolve_lib_name(os, arch) {
            Ok(name) => name,
            Err(_) => return, // no binary for this host target — nothing to load
        };
        let lib_path = dev_libs_dir().join(name);
        assert!(
            lib_path.exists(),
            "expected jwlCore binary at {lib_path:?} for {os}/{arch}"
        );

        // Same vendored-binary trust as `check_jwlcore` above.
        let library =
            load_library(&lib_path).unwrap_or_else(|e| panic!("failed to load {lib_path:?}: {e}"));
        for symbol in EXPECTED_SYMBOLS {
            let result: Result<libloading::Symbol<'_, unsafe extern "C" fn()>, _> =
                unsafe { library.get(format!("{symbol}\0").as_bytes()) };
            assert!(result.is_ok(), "missing expected symbol {symbol}");
        }
        let version: libloading::Symbol<'_, unsafe extern "C" fn() -> *const std::os::raw::c_char> =
            unsafe { library.get(b"getCoreVersion\0") }.expect("getCoreVersion must resolve");
        let ptr = unsafe { version() };
        assert!(!ptr.is_null(), "getCoreVersion returned a null pointer");
        let version_str = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        assert!(
            !version_str.is_empty(),
            "getCoreVersion returned an empty string"
        );
    }
}
