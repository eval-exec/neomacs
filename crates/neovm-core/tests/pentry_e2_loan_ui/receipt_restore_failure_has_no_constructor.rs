use neovm_value::heap_registry::detachment::RegistryRestoreFailure;

fn main() {
    let _failure = RegistryRestoreFailure::<String>::new();
}
