//! Upper-free test fixtures for the host-system crate.
//!
//! The value-level helpers live once in `neovm_value::test_utils`; this
//! module re-exports the subset the system tests use, keeping the
//! historical `crate::test_utils::...` call sites.  It is compiled for the
//! crate's own tests and for upper test builds under the `test-support`
//! feature, never in a product profile.
//!
//! Fixtures that need an evaluator `Context` do not live here: they are
//! re-homed into core's test tree (TEST-SUPPORT-DESIGN §1a option A), so
//! this surface stays free of upward edges.

pub use neovm_value::test_utils::{
    cargo_workspace_root, init_test_tracing, nextest_workspace_root, workspace_root,
};
