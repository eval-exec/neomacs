use neovm_core::tagged::value::TaggedValue;
use std::sync::OnceLock;

// A process-wide cache is shared by every thread; raw values cannot live in it.
static CACHE: OnceLock<TaggedValue> = OnceLock::new();

fn main() {
    let _ = CACHE.get();
}
