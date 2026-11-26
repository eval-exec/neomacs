//! Phase 1c.0 R1: safe code cannot call `with_tagged_heap` to mint a `HeapMut`
//! that bypasses the `Context` borrow (E0133).
use neovm_core::emacs_core::Context;
use neovm_value::tagged::gc::with_tagged_heap;
use neovm_value::value::Value;

fn bypass_via_tls_heap(ctx: &Context, v: Value) {
    let items = v.vector_data_in(ctx.heap_ref()).unwrap();
    with_tagged_heap(|heap| {
        let mut hm = heap.heap_mut();
        let _ = v.replace_vector_data_in(&mut hm, Vec::new());
    });
    let _ = items.len();
}

fn main() {
    let _ = bypass_via_tls_heap;
}
