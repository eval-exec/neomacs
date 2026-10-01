//! The shared assertion surface: inline and file-backed expectations both work
//! through it, and a file is compared exactly as it lies on disk.

use expect_test::{expect, expect_file};

use crate::Snapshot;

/// A file-backed expectation asserts the file's bytes, with no normalization.
#[test]
fn file_backed_snapshot_compares_the_file_bytes() {
    expect_file!["snapshots/example.txt"].assert_snapshot("hello\n");
    let mismatch = std::panic::catch_unwind(|| {
        expect_file!["snapshots/example.txt"].assert_snapshot("other\n")
    });
    assert!(
        mismatch.is_err(),
        "a differing screen must fail the assertion"
    );
}

/// An inline expectation keeps working through the same call.
#[test]
fn inline_snapshot_asserts_through_the_same_call() {
    expect!["hello"].assert_snapshot("hello");
    let mismatch = std::panic::catch_unwind(|| expect!["hello"].assert_snapshot("other"));
    assert!(
        mismatch.is_err(),
        "a differing screen must fail the assertion"
    );
}
