use neovm_value::tagged::gc::heap_transfer::{StopScope, with_stop_scope};

fn main() {
    let _escaped: StopScope<'static> = with_stop_scope(|scope| scope);
}
