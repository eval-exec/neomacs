//! Test support (post-split test-tree pass): only the upper-free helpers
//! shared through `neovm_value::test_utils`.  Tests that need a booted
//! `Context` live in `neovm-core`'s test tree.  Compiled for this crate's own
//! tests and, via `test-support`, for an upper crate's test build.

#[cfg(any(test, feature = "test-support"))]
pub use neovm_value::test_utils::{
    cargo_workspace_root, init_test_tracing, nextest_workspace_root, workspace_root,
};
