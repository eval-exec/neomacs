use neovm_core::emacs_core::value::Value;

fn main() {
    let table = Value::make_sub_char_table(3, 0, vec![Value::NIL; 128]);
    table.with_sub_char_table_mut(|write| write.contents.ensure_owned().push(Value::T));
}
