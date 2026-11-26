use neovm_value::heap_registry::detachment::RegistryDetachFailure;

fn main() {
    let _failure = RegistryDetachFailure::<String>::new();
}
