//! Admission changes allocator access after closing the sole owner's region.
//! Exercise the real Context boundary, including a refused reservation.

use super::{AdmissionError, Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::intern::intern;
use neovm_value::value::Value;

const ROOT: &str = "admission-allocator-root";
const AFTER: &str = "admission-allocator-after";

fn green_with_open_cons_region() -> (Context, Value, Value) {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.gc_stress = false;
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq gc-cons-threshold most-positive-fixnum
                       gc-cons-percentage 1000)
                 (defvar admission-allocator-root nil)
                 (defvar admission-allocator-after nil)",
            )
            .expect("prepare rooted fixture without automatic collection");
    });
    context.gc_collect_exact();
    assert_eq!(context.tagged_heap.cons_region_for_test(), (0, 0));
    assert!(!context.threads_admitted());
    assert!(!context.tagged_heap.threads_admitted());

    // Direct allocation cannot collect or run Lisp. Install the entire graph
    // in an obarray root before admission or any explicit collection.
    let leaf = context
        .tagged_heap
        .alloc_cons(Value::fixnum(41), Value::fixnum(42));
    let root = context.tagged_heap.alloc_cons(leaf, Value::fixnum(43));
    context.obarray.set_symbol_value(ROOT, root);
    // Ensure the final fixture allocation leaves unused cells even if the
    // graph happened to end exactly at a previous region's boundary.
    let (cursor, limit) = context.tagged_heap.cons_region_for_test();
    if cursor == limit {
        let extra = context
            .tagged_heap
            .alloc_cons(Value::fixnum(44), Value::NIL);
        context.obarray.set_symbol_value(AFTER, extra);
    }
    let (cursor, limit) = context.tagged_heap.cons_region_for_test();
    assert!(cursor != 0 && cursor < limit, "partially consumed region");
    (context, root, leaf)
}

fn assert_graph(context: &Context, root: Value, leaf: Value) {
    assert!(context.tagged_heap.owns_heap_value_for_test(root));
    assert!(context.tagged_heap.owns_heap_value_for_test(leaf));
    assert_eq!(print_value(&root), "((41 . 42) . 43)");
}

#[test]
fn admission_allocator_closes_partial_region_and_keeps_hidden_cons_roots() {
    let (mut context, root, leaf) = green_with_open_cons_region();
    let id = intern(ROOT);
    context
        .try_specbind(id, Value::NIL)
        .expect("hide the graph behind a live dynamic binding");
    let live = context.tagged_heap.cons_live_count_exact();
    let allocated = context.tagged_heap.allocated_count();
    let memory_counts = context.tagged_heap.memory_use_counts_snapshot();

    context.admit_threads().expect("commit Green admission");
    assert!(context.threads_admitted());
    assert!(context.tagged_heap.threads_admitted());
    assert_eq!(context.tagged_heap.cons_region_for_test(), (0, 0));
    assert_eq!(context.tagged_heap.cons_live_count_exact(), live);
    assert_eq!(context.tagged_heap.allocated_count(), allocated);
    assert_eq!(
        context.tagged_heap.memory_use_counts_snapshot(),
        memory_counts
    );

    // Allocate after the flip, then collect while the pre-admission graph is
    // hidden. Its sole canonical root must have survived binding projection.
    let after = context
        .tagged_heap
        .alloc_cons(Value::fixnum(99), Value::NIL);
    context.obarray.set_symbol_value(AFTER, after);
    context.gc_collect_exact();
    assert_graph(&context, root, leaf);
    assert!(context.tagged_heap.owns_heap_value_for_test(after));
    context.unbind_to(0);
    assert_eq!(
        context
            .eval_symbol(ROOT)
            .expect("projected root after unbind"),
        root
    );
    context.gc_collect_exact();
    assert_graph(&context, root, leaf);
    assert!(context.threads_admitted());
    assert!(context.tagged_heap.threads_admitted());
}

#[test]
fn admission_allocator_capacity_refusal_preserves_sole_mode_and_cons_inventory() {
    let (mut context, root, leaf) = green_with_open_cons_region();
    let live = context.tagged_heap.cons_live_count_exact();
    let allocated = context.tagged_heap.allocated_count();
    let memory_counts = context.tagged_heap.memory_use_counts_snapshot();
    let bytes = context.tagged_heap.bytes_since_gc_exact();
    let collections = context.tagged_heap.gc_collections();

    assert!(matches!(
        context.admit_threads_refusing_capacity_for_test(),
        Err(AdmissionError::Capacity)
    ));
    assert!(!context.threads_admitted());
    assert!(!context.tagged_heap.threads_admitted());
    // Entering the closed collector phase may refund an open region before
    // reservation refuses. Exact handed-out counts must stay unchanged.
    assert_eq!(context.tagged_heap.cons_region_for_test(), (0, 0));
    assert_eq!(context.tagged_heap.cons_live_count_exact(), live);
    assert_eq!(context.tagged_heap.allocated_count(), allocated);
    assert_eq!(
        context.tagged_heap.memory_use_counts_snapshot(),
        memory_counts
    );
    assert_eq!(context.tagged_heap.bytes_since_gc_exact(), bytes);
    assert_eq!(context.tagged_heap.gc_collections(), collections);
    assert_eq!(context.obarray.symbol_value_copied(ROOT), Some(root));
    assert_graph(&context, root, leaf);

    let after = context
        .tagged_heap
        .alloc_cons(Value::fixnum(99), Value::NIL);
    context.obarray.set_symbol_value(AFTER, after);
    assert_eq!(context.tagged_heap.cons_live_count_exact(), live + 1);
    assert!(!context.tagged_heap.threads_admitted());
    context
        .admit_threads()
        .expect("retry after refused reservation");
    assert!(context.threads_admitted());
    assert!(context.tagged_heap.threads_admitted());
    assert_eq!(context.tagged_heap.cons_region_for_test(), (0, 0));
    context.gc_collect_exact();
    assert_graph(&context, root, leaf);
    assert!(context.tagged_heap.owns_heap_value_for_test(after));
}
