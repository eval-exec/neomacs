//! Re-homed display host/boot tests (P1 B1).
//!
//! These tests lived in neovm-display's cfg(test) modules but need the
//! system host-input publisher, core boot, or core display entries. They are
//! re-homed as an integration test target so they EXECUTE against the real
//! runtime without waiting for core's lib-test target to go green. Test
//! bodies are the display originals; only the constructor line changed
//! (`crate::test_utils::new_context()` / `runtime_startup_context()`).
//!
//! The facade modules below preserve the display-side path spellings
//! (`crate::host_input`, `crate::window`, `crate::emacs_core::...`) so the
//! moved bodies stay byte-identical apart from the constructor.

pub mod test_utils {
    pub use neovm_display::test_utils::mock_redisplay;
    pub use neovm_value::test_utils::*;

    /// Canonical bare constructor: core's one public boot facade.
    pub fn new_context() -> neovm_eval::emacs_core::eval::Context {
        neovm_core::emacs_core::eval_hook_installers::new_context()
    }

    /// Cached full GNU bootstrap, the same context core's test helper builds.
    pub fn runtime_startup_context() -> neovm_eval::emacs_core::eval::Context {
        neovm_core::emacs_core::load_boot::create_runtime_startup_evaluator_cached()
            .unwrap_or_else(|error| panic!("bootstrap: {error}"))
    }
}

pub mod host_input {
    pub use neovm_core::host_input::*;
}

pub use neovm_core::window;

pub mod emacs_core {
    pub use neovm_core::emacs_core::*;
}

#[path = "moved_display_host/display_suite.rs"]
pub mod display_suite;
