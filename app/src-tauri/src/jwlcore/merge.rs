//! jwlCore `mergeDatabase` FFI invocation — the FIRST real call into the
//! native merge engine Phase 1 only loaded (D5-01..D5-10).
//!
//! This is the ONE `unsafe` FFI surface that actually INVOKES `mergeDatabase`.
//! It reuses Phase 1's hard-won load path verbatim (arch selection + the
//! Windows `sqlite3_64.dll` PATH-prepend in `loader::load_library`) — nothing
//! about DLL resolution is re-implemented here.
//!
//! Degradation contract (the Python `crash_box + sys.exit()` defect is NOT
//! ported): a missing/wrong-arch binary is `ArchiveError::MergeUnavailable`;
//! a non-zero `mergeDatabase` return is `ArchiveError::MergeFailed { reason }`
//! carrying `getLastResult()` detail INTERNALLY (never leaked over IPC — the
//! DTO exposes only a generic message_key, D-14). Nothing here ever panics on
//! the archive-data path.
//!
//! jwlCore takes two DIRECTORY paths — `path1` = destination, `path2` = source
//! — opens `<dir>/userData.db` in each, and merges source records INTO the
//! destination DB in place on disk, returning `0` on success
//! (JWLManager.py:2670-2672, jwlcore.py:64-68).

use crate::error::ArchiveError;
use crate::jwlcore::loader::{dev_libs_dir, load_library, resolve_lib_name, resolve_lib_path};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};

/// `int mergeDatabase(const char* path1_dest_dir, const char* path2_src_dir, bool downgrade);`
/// 0 = success. ABI verified against jwlcore.py:64-68 + loader symbol resolution.
type MergeFn = unsafe extern "C" fn(*const c_char, *const c_char, bool) -> c_int;

/// `const char* getLastResult();` — nullable, read-only, never freed by us.
type LastResultFn = unsafe extern "C" fn() -> *const c_char;

/// Fixed generic reason used when `getLastResult()` returns a null pointer, so
/// a failed merge always yields a non-empty typed reason (never an empty
/// string or a deref of null).
const UNKNOWN_MERGE_REASON: &str = "jwlCore reported a merge failure with no detail";

/// Builds the null-terminated C string jwlCore expects for a directory path.
///
/// KNOWN LIMIT: the path bytes come from `to_string_lossy()`, so a
/// `session.temp_dir` under a non-UTF-8-representable Windows username has its
/// un-representable bytes replaced with U+FFFD — the resulting CString then
/// points at a nonexistent directory and the merge returns non-zero, which
/// degrades to a clean `MergeFailed` (never UB). We DOCUMENT this as a known
/// MVP constraint rather than adding OS-string FFI plumbing. An interior NUL
/// byte (not possible in a real filesystem directory path, but guarded here)
/// maps to a typed error, never an unwrap/panic.
fn dir_cstring(path: &Path) -> Result<CString, ArchiveError> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|e| ArchiveError::MergeFailed {
        reason: format!("merge directory path contains an interior NUL byte: {e}"),
    })
}

/// Reads `getLastResult()`'s returned pointer into an owned `String`. A null
/// pointer yields the fixed generic reason.
///
/// SAFETY: the caller must pass either a null pointer or a valid,
/// null-terminated C string owned by jwlCore. The string is only READ here
/// (never freed) — jwlCore owns the storage (jwlcore.py:77-79 handling).
unsafe fn reason_from_ptr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        UNKNOWN_MERGE_REASON.to_string()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

/// Pure `(os, arch)` -> lib name step of [`merge_availability`], split out so
/// the "no binary for this host" branch is unit-testable without an
/// `AppHandle`. A host with no shipped binary (arm64-windows, unsupported
/// combos) maps `resolve_lib_name`'s `Err` to `MergeUnavailable` (Pitfall 3:
/// check availability BEFORE ever invoking into an unloaded lib).
fn availability_name(os: &str, arch: &str) -> Result<&'static str, ArchiveError> {
    resolve_lib_name(os, arch).map_err(|_| ArchiveError::MergeUnavailable)
}

/// Resolves the on-disk jwlCore lib path for the current host, or
/// `ArchiveError::MergeUnavailable` if this `(OS, ARCH)` has no shipped binary
/// or the path cannot be resolved. Checks `resolve_lib_name` FIRST (T-05-02) —
/// arm64-windows must yield "merge unavailable here", never a crash.
pub(crate) fn merge_availability(app: &tauri::AppHandle) -> Result<PathBuf, ArchiveError> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let name = availability_name(os, arch)?;
    resolve_lib_path(app, name).map_err(|_| ArchiveError::MergeUnavailable)
}

/// Dev-tree (`libs/`) path to the jwlCore binary for the CURRENT host, or
/// `None` when this `(OS, ARCH)` has no shipped binary. Lets the FFI
/// integration test (`tests/merge_ffi.rs`, a separate crate that cannot reach
/// the `pub(crate)` loader helpers) resolve the real DLL and skip-as-pass
/// off-host WITHOUT re-implementing arch selection — arch logic stays in the
/// one place (`loader::resolve_lib_name`). Existence is NOT checked here.
pub fn host_dev_lib_path() -> Option<PathBuf> {
    let name = resolve_lib_name(std::env::consts::OS, std::env::consts::ARCH).ok()?;
    Some(dev_libs_dir().join(name))
}

/// Invokes `mergeDatabase` from the vendored lib at `lib_path`, merging the
/// `userData.db` under `source_root` INTO the one under `dest_root` in place.
///
/// Reuses `loader::load_library` (the Windows PATH-prepend for
/// `sqlite3_64.dll` is load-bearing and must not be bypassed, D5-01). A
/// non-zero return reads `getLastResult()` IMMEDIATELY, in the same scope,
/// before the library handle drops (D5-06: the result string is process-global
/// native memory that a later merge would overwrite).
// `pub` (not `pub(crate)`): the Task 3 FFI integration test in
// `tests/merge_ffi.rs` links this crate as an external crate and drives the
// real DLL through this entry point, matching the repo convention for
// integration-tested core routines (e.g. `archive::downgrade::save_v14_copy`).
pub fn run_merge_with_lib_path(
    lib_path: &Path,
    dest_root: &Path,
    source_root: &Path,
    downgrade: bool,
) -> Result<(), ArchiveError> {
    // Build both CStrings up front; keep them bound to locals so their backing
    // buffers outlive the `mergeDatabase` call (the raw pointers below borrow
    // from these).
    let dest = dir_cstring(dest_root)?;
    let src = dir_cstring(source_root)?;

    // A load failure after availability was checked is still "unavailable"
    // (a present-but-unloadable binary is not a merge-data failure).
    let library = load_library(lib_path).map_err(|_| ArchiveError::MergeUnavailable)?;

    // SAFETY: resolving (by name) the two FFI entry points from the just-loaded
    // vendored library; the names are the fixed jwlCore symbols and the bound
    // types match the declared ABI (jwlcore.py:64-68). A missing symbol means
    // the loaded binary is not a usable jwlCore -> MergeUnavailable.
    let merge: libloading::Symbol<MergeFn> =
        unsafe { library.get(b"mergeDatabase\0") }.map_err(|_| ArchiveError::MergeUnavailable)?;
    let last_result: libloading::Symbol<LastResultFn> =
        unsafe { library.get(b"getLastResult\0") }.map_err(|_| ArchiveError::MergeUnavailable)?;

    // SAFETY: calling `mergeDatabase` with two valid, null-terminated C string
    // pointers (their `CString` owners `dest`/`src` are still in scope) and a
    // bool, exactly per the declared ABI. jwlCore mutates
    // `<dest_root>/userData.db` in place and returns 0 on success.
    let rc = unsafe { merge(dest.as_ptr(), src.as_ptr(), downgrade) };
    if rc == 0 {
        return Ok(());
    }

    // D5-06: read the failure detail RIGHT NOW, before `library` drops.
    // SAFETY: `last_result` was resolved above; its ABI is `() -> *const
    // c_char` (nullable). `reason_from_ptr` null-checks before any deref and
    // only reads the (jwlCore-owned) string.
    let reason = unsafe { reason_from_ptr(last_result()) };
    Err(ArchiveError::MergeFailed { reason })
}

// Wave 1 shipped a `run_merge(app, ...)` convenience wrapper (merge_availability
// + run_merge_with_lib_path). Wave 2 SPLIT those two steps at the call site
// (`archive::merge` resolves `merge_availability(app)` once, then drives the
// lib-path core `run_merge_with_lib_path` — which the orchestration integration
// test can call without an AppHandle). The combined wrapper is therefore no
// longer used and was removed; `merge_availability` + `run_merge_with_lib_path`
// remain the two building blocks.

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn dir_cstring_ok_for_normal_path() {
        let c = dir_cstring(Path::new("/tmp/some/dir")).expect("normal path builds a CString");
        assert_eq!(c.to_str().unwrap(), "/tmp/some/dir");
    }

    #[test]
    fn dir_cstring_interior_nul_maps_to_typed_error_never_panics() {
        // An interior NUL cannot appear in a real dir path, but the guard must
        // return a typed MergeFailed rather than unwrap/panic.
        let bad = Path::new("bad\0dir");
        match dir_cstring(bad) {
            Err(ArchiveError::MergeFailed { reason }) => {
                assert!(
                    reason.contains("NUL"),
                    "reason should mention the NUL: {reason}"
                );
            }
            other => panic!("expected MergeFailed, got {other:?}"),
        }
    }

    #[test]
    fn reason_from_null_ptr_is_fixed_generic() {
        // SAFETY: passing a null pointer is the documented null branch.
        let reason = unsafe { reason_from_ptr(std::ptr::null()) };
        assert_eq!(reason, UNKNOWN_MERGE_REASON);
    }

    #[test]
    fn reason_from_valid_ptr_reads_the_string() {
        let owned = CString::new("merge failed: schema mismatch").unwrap();
        // SAFETY: `owned` is a valid null-terminated C string alive for the
        // duration of this call.
        let reason = unsafe { reason_from_ptr(owned.as_ptr()) };
        assert_eq!(reason, "merge failed: schema mismatch");
    }

    #[test]
    fn availability_name_arm64_windows_is_merge_unavailable() {
        match availability_name("windows", "aarch64") {
            Err(ArchiveError::MergeUnavailable) => {}
            other => panic!("expected MergeUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn availability_name_unsupported_combo_is_merge_unavailable() {
        match availability_name("freebsd", "x86_64") {
            Err(ArchiveError::MergeUnavailable) => {}
            other => panic!("expected MergeUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn availability_name_supported_host_resolves_a_name() {
        assert_eq!(
            availability_name("linux", "x86_64").unwrap(),
            "libjwlCore-x86_64.so"
        );
        assert_eq!(
            availability_name("windows", "x86_64").unwrap(),
            "jwlCore-amd64.dll"
        );
    }
}
