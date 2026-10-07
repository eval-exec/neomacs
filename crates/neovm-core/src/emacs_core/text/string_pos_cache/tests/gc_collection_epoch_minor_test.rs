//! String-position root coverage through real generational minor collections.

use super::*;
use crate::emacs_core::eval::Context;

use super::gc_collection_epoch_tests::{assert_warm_entry, populate_cache};

fn generational_context() -> Context {
    // Nextest isolates tests in separate processes. Set the constructor knob
    // before creating a Context or starting a collector thread.
    unsafe {
        std::env::set_var("NEOVM_GC_GENERATIONAL", "1");
        std::env::set_var("NEOVM_GC_MAJOR_MAX_MINORS", usize::MAX.to_string());
        std::env::set_var("NEOVM_GC_MAJOR_GROWTH_PERCENT", usize::MAX.to_string());
    }
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context.gc_stress = false;
    assert!(context.tagged_heap.generational_enabled());
    assert!(!context.gc_stress);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.gc_collect_exact();
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());
    assert!(context.tagged_heap.should_run_minor(false, false));
    context
}

fn cache_young_string(context: &mut Context) -> Value {
    let string = populate_cache(context);
    assert!(context.tagged_heap.owns_heap_value_for_test(string));
    assert!(
        !context.tagged_heap.value_is_old_for_test(string),
        "the cached entry must start young after the bootstrap major"
    );
    string
}

fn start_minor(context: &mut Context) -> usize {
    assert!(context.tagged_heap.generational_enabled());
    assert!(!context.gc_stress);
    assert!(context.tagged_heap.should_run_minor(false, false));
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());
    let completed = context.tagged_heap.gc_collections();
    let runtime_completed = context.gc_count;
    context.tagged_heap.set_gc_threshold(1);
    context.tagged_heap.alloc_cons(Value::NIL, Value::NIL);
    context.gc_safe_point();
    assert!(
        context.tagged_heap.sweep_in_progress(),
        "the runtime minor must return with its sweep deferred"
    );
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.concurrent_mark_running());
    assert_eq!(context.tagged_heap.gc_collections(), completed);
    assert_eq!(context.gc_count, runtime_completed);
    completed
}

fn finish_minor(context: &mut Context, completed: usize) {
    assert!(context.tagged_heap.sweep_in_progress());
    let runtime_completed = context.gc_count;
    // Completion refreshes runtime pacing; reset this threshold at every
    // cycle so polling cannot start another collection after the sweep drains.
    context.tagged_heap.set_gc_threshold(usize::MAX);
    for _ in 0..10_000 {
        context.gc_safe_point();
        if !context.tagged_heap.sweep_in_progress() {
            assert!(!context.tagged_heap.mark_in_progress());
            assert!(!context.tagged_heap.concurrent_mark_running());
            assert_eq!(context.tagged_heap.gc_collections(), completed + 1);
            assert_eq!(context.gc_count, runtime_completed + 1);
            return;
        }
    }
    panic!("public safe points did not complete the minor sweep");
}

fn assert_owned_warm_entry(context: &Context, string: Value, before: Entry) {
    // A failed root-preservation test must assert before reading a reclaimed
    // string's payload. This predicate only checks the heap's allocation set.
    assert!(
        context.tagged_heap.owns_heap_value_for_test(string),
        "an owning-thread minor reclaimed the cached string"
    );
    assert_warm_entry(string, before);
    assert!(
        context.tagged_heap.value_is_old_for_test(string),
        "minor marking must promote the cached young string before sweeping"
    );
}

#[test]
fn gc_collection_epoch_string_pos_minor_collections_keep_warm_entry() {
    let mut context = generational_context();
    let string = cache_young_string(&mut context);
    let before = CACHE.with(Cell::get).unwrap();
    let initial_completed = context.tagged_heap.gc_collections();
    for _ in 0..3 {
        let completed = start_minor(&mut context);
        assert_owned_warm_entry(&context, string, before);
        context.setup_thread_locals();
        assert_owned_warm_entry(&context, string, before);
        finish_minor(&mut context, completed);
        // Check before activation or conversion could refill a missing entry.
        assert_owned_warm_entry(&context, string, before);
        context.setup_thread_locals();
        assert_owned_warm_entry(&context, string, before);
    }
    assert_eq!(context.tagged_heap.gc_collections(), initial_completed + 3);
}

#[test]
fn gc_collection_epoch_string_pos_minor_sweep_activation_keeps_warm_entry() {
    let mut context = generational_context();
    let string = cache_young_string(&mut context);
    let before = CACHE.with(Cell::get).unwrap();
    let completed = start_minor(&mut context);
    context.setup_thread_locals();
    assert!(context.tagged_heap.sweep_in_progress());
    assert_eq!(context.tagged_heap.gc_collections(), completed);
    assert_owned_warm_entry(&context, string, before);
    finish_minor(&mut context, completed);
    assert_owned_warm_entry(&context, string, before);
}

#[derive(Clone, Copy)]
enum ReturnAt {
    Completed,
    Sweeping,
}

fn collect_minor_on_another_thread(
    mut context: Context,
    string: Value,
    return_at: ReturnAt,
) -> (Context, usize) {
    let completed = context.tagged_heap.gc_collections();
    let context = std::thread::spawn(move || {
        context.setup_thread_locals();
        assert!(CACHE.with(Cell::get).is_none());
        assert_eq!(start_minor(&mut context), completed);
        if matches!(return_at, ReturnAt::Completed) {
            finish_minor(&mut context, completed);
            assert!(
                !context.tagged_heap.owns_heap_value_for_test(string),
                "the destination minor must reclaim the source's young string"
            );
        }
        context
    })
    .join()
    .unwrap();
    (context, completed)
}

#[test]
fn gc_collection_epoch_string_pos_minor_activation_discards_swept_source_entry() {
    let mut context = generational_context();
    let heap_identity = context.tagged_heap.identity();
    let string = cache_young_string(&mut context);
    let (mut context, completed) =
        collect_minor_on_another_thread(context, string, ReturnAt::Completed);
    assert_eq!(context.tagged_heap.identity(), heap_identity);
    assert_eq!(context.tagged_heap.gc_collections(), completed + 1);
    assert!(!context.tagged_heap.owns_heap_value_for_test(string));
    // Only compare identity bits after the destination reclaimed the string.
    assert_eq!(CACHE.with(Cell::get).unwrap().string.bits(), string.bits());
    context.setup_thread_locals();
    assert!(
        CACHE.with(Cell::get).is_none(),
        "activation retained a source entry reclaimed by a destination minor"
    );
}

#[test]
fn gc_collection_epoch_string_pos_minor_sweep_foreign_activation_discards_entry() {
    let mut context = generational_context();
    let string = cache_young_string(&mut context);
    let (mut context, completed) =
        collect_minor_on_another_thread(context, string, ReturnAt::Sweeping);
    assert!(context.tagged_heap.sweep_in_progress());
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.concurrent_mark_running());
    assert_eq!(context.tagged_heap.gc_collections(), completed);
    assert_eq!(CACHE.with(Cell::get).unwrap().string.bits(), string.bits());
    context.setup_thread_locals();
    assert!(
        CACHE.with(Cell::get).is_none(),
        "an uncovered destination minor sweep retained the source entry"
    );
    finish_minor(&mut context, completed);
    assert!(!context.tagged_heap.owns_heap_value_for_test(string));
}
