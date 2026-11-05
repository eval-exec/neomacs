//! Text processing layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod code_conversion_workspace;
pub mod emacs_core;
pub mod encoding;

#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;
