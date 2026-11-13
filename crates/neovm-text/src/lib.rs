//! Text processing layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod code_conversion_workspace;
pub mod emacs_core;
pub mod encoding;

/// Test-only authority fence for core-owned tests of moved text fixtures:
/// the signed test_support::authority delegates for the regex/DFA/parse-cache
/// TLS resets and the regex perf probes.
#[cfg(feature = "test-support")]
pub mod test_support;

#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;
