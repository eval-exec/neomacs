pub use neovm_edit::emacs_core::mode::*;
#[path = "../editing/mode/tests/custom_type_gc_test.rs"]
mod custom_type_gc;
#[path = "../editing/mode/tests/mode_test.rs"]
mod tests;
pub use neovm_value::gc_trace::GcTrace;
pub use neovm_value::heap_types::LispString;
pub use neovm_value::value::Value;
