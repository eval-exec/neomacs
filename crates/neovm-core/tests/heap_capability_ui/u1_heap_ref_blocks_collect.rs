//! Phase 1c.0 U1: holding a `HeapRef` payload borrow across
//! `Context::gc_collect_exact` fails with E0502.
use neovm_core::emacs_core::Context;
use neovm_value::value::Value;

fn borrow_across_collect(ctx: &mut Context, v: Value) {
    let items = v.vector_data_in(ctx.heap_ref()).unwrap();
    ctx.gc_collect_exact();
    let _ = items.len();
}

fn main() {
    let _ = borrow_across_collect;
}
