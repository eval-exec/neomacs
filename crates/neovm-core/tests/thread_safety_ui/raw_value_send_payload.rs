use neovm_core::tagged::value::TaggedValue;

// A background payload carries plain data or rooted transport, never a raw
// thread-confined value.
struct WorkerPayload {
    _value: TaggedValue,
}

fn require_send<T: Send>() {}

fn main() {
    require_send::<WorkerPayload>();
}
