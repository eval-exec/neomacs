//! Small Miri regression for the obarray's GC-visible function/plist words.

use super::*;

#[test]
fn miriub3_gc_collect_exact_traces_function_and_plist() {
    // Deliberately omit builtin/bootstrap registration: only the reduced VM
    // roots and this symbol are needed to reach Obarray::trace_roots.
    let mut context = Context::new_minimal_vm_harness();
    let symbol = intern("miriub3-function-and-plist");
    let function = Value::cons(Value::fixnum(71), Value::NIL);
    let plist = Value::cons(Value::fixnum(83), Value::NIL);
    context.obarray.set_symbol_function_id(symbol, function);
    context.obarray.set_symbol_plist_id(symbol, plist);

    context.gc_collect_exact();

    let function = context.obarray.symbol_function_id(symbol).unwrap();
    let plist = context.obarray.symbol_plist_id(symbol);
    assert_eq!(function.cons_car(), Value::fixnum(71));
    assert_eq!(plist.cons_car(), Value::fixnum(83));
}

/// The real root walk must inherit capture's loan rather than re-entering
/// through the Context's root-derived heap view. No marker thread is needed.
#[test]
fn heapborrow_r1_context_roots_during_capture() {
    let mut context = Context::new_minimal_vm_harness();
    let value = Value::cons(Value::fixnum(17), Value::NIL);
    context.lexenv = value;
    let root = context.tagged_heap.heap_root().as_ptr();
    // SAFETY: this stopped minimal Context is the sole owner; the capture
    // stays unpublished and the callback only enumerates and seeds roots.
    unsafe {
        let mut capture = (*root).permit_concurrent_mark().unwrap().begin();
        let heap = capture.heap_mut() as *mut neovm_value::tagged::gc::TaggedHeap;
        (*heap).seed_root(value);
        context.seed_registered_mutator_roots_world_stopped(heap);
        capture.heap_mut().seed_root(value);
        drop(capture);
        drop(
            (*root)
                .drain_to_quiescent(|heap| {
                    context.seed_registered_mutator_roots_world_stopped(heap);
                })
                .unwrap(),
        );
    }
    assert_eq!(context.lexenv.cons_car(), Value::fixnum(17));
}

/// Covers the shared reseed seam reached by exact collection and the Green
/// admission/retirement callers, with an active but unpublished mark.
#[test]
fn heapborrow_r2_r3_context_roots_during_drain() {
    let mut context = Context::new_minimal_vm_harness();
    context.lexenv = Value::cons(Value::fixnum(29), Value::NIL);
    let root = context.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole stopped owner; no worker, callback, or allocation safepoint.
    unsafe {
        (*root).concurrent_begin();
        drop(
            (*root)
                .drain_to_quiescent(|heap| {
                    context.seed_registered_mutator_roots_world_stopped(heap);
                })
                .unwrap(),
        );
    }
    assert_eq!(context.lexenv.cons_car(), Value::fixnum(29));
}
