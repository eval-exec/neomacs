//! Evaluator test support (post-split TEST-SUPPORT-DESIGN.md rev 2).
//!
//! Only fixtures that never construct a `Context` live here: a booted
//! Context belongs to the core boot facade, and tests that need one are
//! re-homed into `neovm-core/core_tests/moved_eval/`. This module is
//! compiled for the crate's own unit tests and, behind `test-support`, for
//! core's test build; the helpers are the single value-level definitions.

pub use neovm_value::test_utils::{
    cargo_workspace_root, init_test_tracing, nextest_workspace_root, workspace_root,
};
