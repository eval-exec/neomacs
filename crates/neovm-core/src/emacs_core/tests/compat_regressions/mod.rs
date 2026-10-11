//! Compatibility regression tests.
//!
//! These tests lock behavior for GNU Emacs compatibility paths that were
//! historically implemented in `compat_internal`.
#[cfg(test)]
#[path = "tests/compat_regressions_test.rs"]
mod tests;
