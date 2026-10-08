use neovm_core::emacs_core::Context;
use neovm_core::tagged::gc::TaggedHeap;
use neovm_core::tagged::transport::SharedRoot;
use neovm_core::tagged::value::TaggedValue;

fn main() {
    let heap = TaggedHeap::new();
    let context = Context::new();
    let _ = SharedRoot::new(&heap, TaggedValue::NIL);
    let _ = SharedRoot::from_current_heap(TaggedValue::NIL);
    let _ = context.share_value(TaggedValue::NIL);
    let _ = SharedRoot::batch_from_current_heap(&[TaggedValue::NIL]);
    let _ = context.share_values(&[TaggedValue::NIL]);
}
