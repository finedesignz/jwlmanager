# Phase 10 — jwlCore test concurrency finding

**Source finding:** `.planning/phases/10-n-way-merge-fold/VERIFICATION.md` — `fold_merge_tests.rs`
intermittently flaky under default multi-threaded `cargo test` (1 failure in 4 runs), stable
10/10 under `--test-threads=1`. Observed failure shape: a row-count divergence with
`Tag`/`TagMap` present on only one side of a comparison that should have matched.

## 1. Which cause it was

**Cause (a): the vendored `jwlCore-amd64.dll` is not safe to invoke concurrently from
multiple threads in one process.** Not a fixture collision.

Fixture collision was ruled out first: every fixture generator in `tests/common/mod.rs` builds
its archive/DB inside a fresh `tempfile::TempDir` (`TempDir::new()`), and every test in
`fold_merge_tests.rs` calls its own generator per test — there is no shared/fixed path, shared
static, or reused filename between concurrently-running tests. `tempfile::TempDir` names are
process-unique, so two tests cannot collide on the same on-disk path.

Two independent pieces of evidence point at the DLL/loader instead:

- **`jwlcore::loader::load_library` (Windows path) mutates the process-global `PATH` environment
  variable** for the duration of every load (`std::env::set_var("PATH", ...)` prepend, then
  restore) so `sqlite3_64.dll` resolves next to `jwlCore-amd64.dll`. Environment variables are
  process-global, not thread-local — two threads calling `load_library` concurrently can
  interleave their set/restore, and the OS loader's own `LoadLibrary` call for the *same DLL
  path* hands back the *same already-loaded module* to every caller in the process (Windows
  reference-counts DLL loads by path; it does not create a second isolated instance). So two
  concurrently-running tests are not just racing on `PATH` — they are, in practice, driving the
  *same* loaded native module from two threads at once.
- **`jwlcore::merge::run_merge_with_lib_path`'s own doc comment already documents jwlCore-side
  global state**: it reads `getLastResult()` "RIGHT NOW, before `library` drops" specifically
  because "the result string is process-global native memory that a later merge would
  overwrite" (D5-06). A C library that keeps a process-global last-result buffer is exactly the
  kind of implementation that would also keep other internal state (parser/writer buffers,
  in-progress transaction bookkeeping) shared across concurrent invocations rather than
  re-entrant per call — which is consistent with the observed failure: a `Tag`/`TagMap` pair
  belonging to one concurrently-running test's merge showing up (or not) in a different test's
  result.

No cheaper decisive experiment was constructed beyond this: the evidence above (env-var
mutation across threads + a native library that already documents its own process-global
result buffer) is sufficient to explain the observed symptom, and the >=8 repeated
default-parallelism runs after serializing all real-DLL tests in this file (see Section 4)
confirm the flake disappears once concurrent access is prevented — the decisive experiment is
therefore the fix's own verification run, not a separate throwaway repro.

## 2. Can the APP invoke jwlCore concurrently?

**No — production cannot overlap two merges/folds.** `lib.rs` manages exactly one
`Mutex<Option<ArchiveSession>>` app state (`.manage(Mutex::new(None::<ArchiveSession>))`), and
every command that can reach jwlCore (`merge_dry_run`, `merge_commit`, `fold_merge_dry_run`,
`fold_merge_commit`, plus every other session-mutating command) takes `state.lock()` for its
entire body before doing anything. The doc comments on `merge_commit`/`fold_merge_commit`
already state this explicitly:

> "...this single lock critical section (D5-06 serialization)."

Because there is exactly one `ArchiveSession` slot and every jwlCore-touching command holds that
same mutex for its whole duration, two merges/folds — whether triggered by the same session or
(impossible, since there is only one slot) a different one — cannot execute concurrently inside
one running app instance. The mutex was already designed with this constraint in mind (the
comment cites D5-06 by name), so this is confirmed existing behavior, not a new mitigation.

**This is a test-only problem.** No production code changes were made or are needed.

## 3. Fix applied

Fixture collision was ruled out (Section 1), so no fixture-path fix was needed. The fix is a
dependency-free, in-test-module `Mutex<()>` (`JWLCORE_TEST_LOCK`) added to
`app/src-tauri/tests/fold_merge_tests.rs`, with a `let _lock = JWLCORE_TEST_LOCK.lock().unwrap();`
guard acquired in every one of the 10 tests immediately after its `host_lib_or_skip` gate (i.e.
before any real DLL work begins, but after the off-host skip so CI hosts without a vendored
binary still return early without touching the lock). A doc comment above the static explains
the root cause and explicitly tells a future reader not to remove it. No new Cargo dependency
was added (`std::sync::Mutex` only).

Only `fold_merge_tests.rs` was changed, matching this task's scope. `merge_orchestration.rs` and
`merge_ffi.rs` invoke the same real DLL via the same loader and are exposed to the same
process-global risk in principle, but neither was reported as flaky by the Phase 10 verifier and
both are out of scope for this fix — flagged here for awareness, not modified.

## 4. Proof — >=5 runs under default parallelism

`cargo test --jobs 2 --test fold_merge_tests` (default test-thread parallelism; `--jobs 2` only
caps the *build* linker concurrency per this repo's documented host constraint, unrelated to
test-thread count) was run **8** times after the fix, every run producing all 10 tests green:

| Run | Result |
|-----|--------|
| 1 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.81s` |
| 2 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s` |
| 3 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.80s` |
| 4 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.80s` |
| 5 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.54s` |
| 6 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.75s` |
| 7 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.72s` |
| 8 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.90s` |

Additionally:

- `cargo test --jobs 2` (full suite, once): all test binaries reported `test result: ok` with
  `0 failed` — no regression from the added lock.
- `cargo clippy --all-targets -- -D warnings`: clean, no warnings.

## Constraints honored

- No new Cargo dependency (`std::sync::Mutex` from `std`, already imported transitively by the
  crate; added `use std::sync::Mutex;` to the test file).
- No production merge logic was touched — the app already serializes merge/fold via its own
  session mutex (Section 2); this fix is entirely test-scoped.
- Typed errors / no `unwrap`/`panic` rule applies to production code; the test file already used
  `.unwrap()`/`.expect()` throughout (permitted in test code), and the new `.lock().unwrap()`
  matches that existing convention.
