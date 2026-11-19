use neovm_value::tagged::gc::heap_transfer::ReturnedHeapTransfer;

fn assert_send<T: Send>() {}

fn main() {
    assert_send::<ReturnedHeapTransfer>();
}
