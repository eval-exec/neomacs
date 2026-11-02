fn main() { let value = neovm_core::tagged::value::TaggedValue::NIL; let _ = value.as_subr_entry::<fn()>(); }
