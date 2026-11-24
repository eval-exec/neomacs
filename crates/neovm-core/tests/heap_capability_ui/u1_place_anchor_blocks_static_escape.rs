//! P1.1: a place-anchored payload borrow cannot escape as `'static`.
use neovm_value::tagged::header::LispValueSlice;
use neovm_value::value::Value;

fn escapes(value: Value) -> Option<&'static LispValueSlice> {
    value.as_window_configuration_data()
}

fn main() {
    let _ = escapes;
}
