use neovm_core::emacs_core::value::Value;

fn main() {
    let table = Value::make_char_table(Value::NIL, Value::NIL, 1);
    table.with_char_table_mut(|write| write.defalt = Value::T);
}
