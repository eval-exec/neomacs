//! Upper-free test infrastructure shared by value-side and core-side tests.
//!
//! Peer-signed R7c(a) (2026-11-03): these helpers have no upper dependency, so
//! they live once in the lower layer.  `neovm-core`'s `test_utils` re-exports
//! them, keeping the historical `crate::test_utils::...` paths for its test
//! files.  The module is compiled for value's own tests and for core's test
//! build (feature `test-support`); it is absent from every product profile.

use std::path::PathBuf;

/// Initialize the process-wide test support shared by every split crate's
/// test tree.
///
/// Arms the dedicated per-thread test fallback heap.  A unified workspace
/// test build (CI's `cargo nextest archive --workspace`) unifies
/// dev-dependency features, so `test-thread-fallback-heap` is compiled for
/// every dependent; the fallback must still stay unavailable to a crate that
/// never had it, so the feature only makes it available and this explicit
/// arming call enables it for the process.  Only the six split crates' test
/// trees reach this init (through their `test_utils` re-export);
/// `neomacs-layout-engine` never does and keeps the production panic.
///
/// Also initializes the tracing subscriber for test output. Thin wrapper
/// around [`crate::logging::init_for_tests`] kept for the many existing call
/// sites in this crate's test files. Tests never write to a log file
/// regardless of `NEOMACS_LOG_TO_FILE` — output is always routed through the
/// test harness's writer.
pub fn init_test_tracing() {
    #[cfg(any(test, feature = "test-thread-fallback-heap"))]
    crate::tagged::gc::enable_test_thread_fallback();
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

/// Construct a legacy-GC fixture with the knob selected before heap creation.
/// Nextest gives each test its own process. Restore the caller's environment
/// on return or unwind; an existing heap's generation mode is never changed.
///
/// This is an environment knob for the GC owned by this crate, so it lives
/// here rather than beside the boot-owning fixtures in core's test tree
/// (TEST-SUPPORT-DESIGN.md rev 2, "Where each item lives").  The body stays
/// `pub(crate)`: the single cross-crate route is the AUTHORITY fence
/// `test_support::authority::with_legacy_gc`, which core's and eval's
/// `test_utils` re-export (peer 10:07Z re-class: a store to process-global
/// state, even self-restoring, is AUTHORITY, not VIS).
pub(crate) fn with_legacy_gc<T>(create: impl FnOnce() -> T) -> T {
    struct RestoreGenerationalKnob(Option<std::ffi::OsString>);

    impl Drop for RestoreGenerationalKnob {
        fn drop(&mut self) {
            unsafe {
                match self.0.take() {
                    Some(previous) => std::env::set_var("NEOVM_GC_GENERATIONAL", previous),
                    None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
                }
            }
        }
    }

    let _restore = RestoreGenerationalKnob(std::env::var_os("NEOVM_GC_GENERATIONAL"));
    unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "0") };
    create()
}
