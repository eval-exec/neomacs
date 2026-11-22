use neovm_value::tagged::gc::heap_transfer::{StopScope, with_stop_scope};

fn unify<'s>(_left: &StopScope<'s>, _right: &StopScope<'s>) {}

fn main() {
    with_stop_scope(|outer| {
        with_stop_scope(|inner| unify(&outer, &inner));
    });
}
