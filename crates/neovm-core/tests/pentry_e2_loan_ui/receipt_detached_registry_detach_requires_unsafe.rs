use neovm_value::heap_registry::HeapRegistrySlot;

fn main() {
    let slot = HeapRegistrySlot::new(String::new());
    let handle = slot.current();
    let _receipt = slot.detach(&handle, String::new());
}
