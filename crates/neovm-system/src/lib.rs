//! Host system services layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod emacs_core;

#[cfg(feature = "test-support")]
pub mod test_support;

/// Test-only fixtures for this crate's tests and for upper test builds.
///
/// Compiled for the crate's own tests and under the `test-support` feature;
/// absent from every product profile (TEST-SUPPORT-DESIGN rules 1 and 5).
#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;
