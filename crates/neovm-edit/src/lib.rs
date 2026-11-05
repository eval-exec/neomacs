//! Editor and buffer state layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod buffer;
pub mod emacs_core;
#[cfg(feature = "test-support")]
pub mod test_support;

#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;
