use neovm_core::emacs_core::forward::{FwdDescriptor, LispFwd, alloc_objfwd};
use neovm_core::emacs_core::value::Value;
fn borrow_slot(header: &'static LispFwd) -> &'static Value {
    header.load().expect("object slot")
}
fn main() {
    let _ = borrow_slot(alloc_objfwd(Value::NIL).header());
}
