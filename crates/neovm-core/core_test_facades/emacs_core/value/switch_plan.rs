//! cfg(test)-only core wrapper for the moved `value::switch_plan` test tree.
//!
//! Declaration-only: the historical module path re-exports the value module
//! and hosts the relocated switch-plan test declaration; never compiled in a
//! product build.

pub use neovm_value::value::switch_plan::*;

// Split-era prelude: the historical parent module imported these names
// privately, so the moved test's `use super::*` needs them re-exported.
pub use neovm_value::tagged::value::TAG_MASK;
pub use neovm_value::value::{HashTableTest, LispHashTable, Value};

#[cfg(test)]
#[path = "../../../core_tests/emacs_core/runtime/value/switch_plan/tests/switch_plan_test.rs"]
mod tests;
