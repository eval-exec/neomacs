//! cfg(test)-only core wrapper for the value `keyboard_pure` module
//! (P1 value tree: pure_test re-home).
//!
//! Declaration-only: re-exports the value module and the private
//! split-era test prelude names that the historical parent module
//! imported privately, then mounts the relocated test file.

pub use neovm_value::keyboard_pure::*;

// Split-era test prelude: keyboard_pure.rs imported these privately, so the
// relocated `use super::*` glob needs them re-exported from the wrapper.
pub use neovm_value::tagged::value::SymbolMode;
pub use neovm_value::value::Value;

#[cfg(test)]
#[path = "../core_tests/keyboard_pure/tests/pure_test.rs"]
mod tests;
