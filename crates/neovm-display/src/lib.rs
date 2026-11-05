//! Display and window layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod display_evaluation;
pub mod emacs_core;
pub mod face;
pub mod image_identity;
/// Test-support authority fence for the re-homed oracles (peer 19:36Z
/// follow-up): the three DisplayFrameExt test fixtures are signed rows and
/// this module is their only fence path. Compiled only for tests and the
/// upper test-support build.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
/// Shared upper-free test helpers (re-export; see `test_utils.rs`).
#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;
pub mod window;
