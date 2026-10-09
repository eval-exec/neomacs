use neovm_core::heap_types::LispString;

fn main() {
    let mut string = LispString::from_utf8("abc");
    let properties = string.intervals();
    string.clear_intervals();
    assert!(properties.is_empty());
}
