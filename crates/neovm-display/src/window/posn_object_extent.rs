//! Window posn-extent data facade (stage-4 W2/E5 FM-6b1).
//!
//! The posn-object extent policy, snapshot accessor and retained-extent
//! reader moved to emacs_core::eval::data::posn_object_extent; this module
//! re-exports them at the old paths and keeps the test modules.

#[cfg(any(test, feature = "redisplay-test-policy"))]
pub use neovm_eval::emacs_core::eval::data::posn_object_extent::force_posn_object_extent_for_test;
pub use neovm_eval::emacs_core::eval::data::posn_object_extent::{
    PosnObjectExtentMode, posn_object_extent_mode, retained_posn_extent,
};

#[cfg(test)]
#[path = "tests/posn_object_extent_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/posn_policy_injection_test.rs"]
mod injection_tests;
