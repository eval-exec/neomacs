//! One assertion surface for inline and file-backed snapshots.
//!
//! `expect!` and `expect_file!` expand to different types that share the same
//! assertion method.  Package suites take their expected screens through
//! [`Snapshot`] so a screen can move between an inline block and a file under
//! `snapshots/` without the helper that asserts it changing shape.

use expect_test::{Expect, ExpectFile};

/// Assert an observed screen against an expected one.
pub trait Snapshot {
    /// Panic with a diff when `actual` differs from the expectation.
    fn assert_snapshot(&self, actual: &str);
}

impl Snapshot for Expect {
    fn assert_snapshot(&self, actual: &str) {
        self.assert_eq(actual);
    }
}

impl Snapshot for ExpectFile {
    fn assert_snapshot(&self, actual: &str) {
        self.assert_eq(actual);
    }
}
