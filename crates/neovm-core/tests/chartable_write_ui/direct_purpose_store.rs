use neovm_core::emacs_core::value::Value;

fn main() {
    let table = Value::make_char_table(Value::NIL, Value::NIL, 1);
    table
        .as_char_table_obj()
        .unwrap()
        .purpose
        .store_atomic(Value::T);
}
