use neovm_core::heap_types::LispString;

fn require_send<T: Send>() {}

fn main() {
    // A standalone string may own interval plists with mutator-local Values.
    require_send::<LispString>();
}
