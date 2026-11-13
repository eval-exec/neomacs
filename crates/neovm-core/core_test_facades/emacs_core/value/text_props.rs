//! Core-owned text-property tests at their historical module path.
pub use neovm_value::value::text_props::*;

// Public-path restoration for the relocated core_tests files.
pub use neovm_edit::buffer::buffer_text::BufferText;

#[cfg(test)]
#[path = "../../../core_tests/emacs_core/runtime/value/text_props/tests/bounded_absent_property_test.rs"]
mod bounded_absent_property_test;
