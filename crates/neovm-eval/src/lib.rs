//! Evaluator and Lisp runtime layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod emacs_core;
pub mod host_effect;

/// Test support shared by this crate's own unit tests and, behind
/// `test-support`, by core's test build (a dependency crate never has
/// `cfg(test)` on). See TEST-SUPPORT-DESIGN.md rev 2.
#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;

// GNU seeds emacs-version from configure.ac's PACKAGE_VERSION; version.el
// derives its numeric components from that same value. The evaluator seeds
// the bare variable, so the constants live here (stage-4 W5/E13); the core
// crate keeps the historical crate-root paths as re-exports.
macro_rules! gnu_emacs_version {
    ($major:literal, $minor:literal $(, $micro:literal)?) => {
        /// The GNU Emacs version whose Lisp tree and compatibility surface we track.
        pub const GNU_EMACS_VERSION: &str =
            concat!(stringify!($major), ".", stringify!($minor) $(, ".", stringify!($micro))?);
        #[doc(hidden)]
        pub const GNU_EMACS_MAJOR_VERSION: u32 = $major;
        #[doc(hidden)]
        pub const GNU_EMACS_MINOR_VERSION: u32 = $minor;
        $(const _: u32 = $micro;)?
    };
}

gnu_emacs_version!(31, 1);
