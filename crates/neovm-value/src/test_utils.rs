//! Upper-free test infrastructure shared by value-side and core-side tests.
//!
//! Peer-signed R7c(a) (2026-11-03): these helpers have no upper dependency, so
//! they live once in the lower layer.  `neovm-core`'s `test_utils` re-exports
//! them, keeping the historical `crate::test_utils::...` paths for its test
//! files.  The module is compiled for value's own tests and for core's test
//! build (feature `test-support`); it is absent from every product profile.

use std::path::PathBuf;

/// Initialize the tracing subscriber for test output.
///
/// Thin wrapper around [`crate::logging::init_for_tests`] kept for the
/// many existing call sites in this crate's test files. Tests never
/// write to a log file regardless of `NEOMACS_LOG_TO_FILE` — output is
/// always routed through the test harness's writer.
pub fn init_test_tracing() {
    crate::logging::init_for_tests();
}

/// The workspace root baked in at compile time — the **build** machine's
/// path, wrong for archive-shipped binaries on every other runner.  Only
/// [`workspace_root`] reads it.
pub fn cargo_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

/// The workspace root nextest exports at runtime: the live workspace on
/// the running machine.  `None` outside nextest.
pub fn nextest_workspace_root() -> Option<PathBuf> {
    std::env::var_os("NEXTEST_WORKSPACE_ROOT").map(PathBuf::from)
}

/// The workspace root of the machine *running* the test: nextest's runtime
/// value when present, the compile-time constant otherwise.  The fallback
/// order lives here once, so no call site can get it wrong.
pub fn workspace_root() -> PathBuf {
    nextest_workspace_root().unwrap_or_else(cargo_workspace_root)
}
