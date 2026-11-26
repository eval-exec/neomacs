//! Phase 1c.0 compiling control: sequential `HeapRef` reads followed by
//! collection, `HeapMut` bulk mutation, sequential `with_hash_table_mut_in`,
//! and `TaggedHeap::shutdown` all compile cleanly.
use neovm_core::emacs_core::Context;
use neovm_value::tagged::gc::TaggedHeap;
use neovm_value::value::Value;

fn sequential_with_context(ctx: &mut Context, v: Value) {
    let len = v
        .vector_data_in(ctx.heap_ref())
        .map_or(0, |items| items.len());
    ctx.gc_collect_exact();
    let _ = len;
}

fn sequential_with_heap(mut heap: TaggedHeap, v: Value) {
    {
        let mut hm = heap.heap_mut();
        let len = v.vector_data_in(hm.as_ref()).map_or(0, |items| items.len());
        let _ = v.replace_vector_data_in(&mut hm, vec![Value::fixnum(len as i64)]);
        let _ = v.with_hash_table_mut_in(&mut hm, |_first| ());
        let _ = v.with_hash_table_mut_in(&mut hm, |_second| ());
    }
    let _ = heap.shutdown();
}

fn main() {
    let _ = (sequential_with_context, sequential_with_heap);
}
