//! Display and window layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod display_evaluation;
pub mod emacs_core;
pub mod face;
pub mod image_identity;
/// Shared upper-free test helpers (re-export; see `test_utils.rs`).
#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;
pub mod window;
