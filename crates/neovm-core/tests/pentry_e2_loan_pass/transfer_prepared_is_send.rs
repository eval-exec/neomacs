use neovm_value::tagged::gc::heap_transfer::PreparedHeapTransfer;

fn assert_send<T: Send>() {}

fn main() {
    assert_send::<PreparedHeapTransfer>();
}
