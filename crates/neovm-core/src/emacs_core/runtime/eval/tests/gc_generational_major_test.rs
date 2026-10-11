//! Real Context minor/major entries and heap-attributed roots.

use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_root_slot, restore_scratch_gc_roots, save_scratch_gc_roots, set_scratch_gc_root,
};

struct ScratchRoots(usize);

impl ScratchRoots {
    fn new() -> Self {
        Self(save_scratch_gc_roots())
    }

    fn keep(&self, value: Value) -> usize {
        push_scratch_gc_root_slot(value)
    }

    fn clear(&self, slot: usize) {
        set_scratch_gc_root(slot, Value::NIL);
    }
}

impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn context() -> Context {
    unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "1") };
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    // This fixture observes manual deferred minors. Automatic synchronous
    // stress routing is covered separately by gc_generational_pacing.
    context.gc_stress = false;
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq gc-cons-threshold most-positive-fixnum gc-cons-percentage 1000)")
            .expect("set bounded test pacing");
    });
    context.gc_collect_exact();
    assert!(context.tagged_heap.generational_enabled());
    assert!(context.tagged_heap.should_run_minor(false, false));
    context
}

fn minor(context: &mut Context) {
    let before = context.gc_count;
    context.gc_collect_from_current_roots_impl(false);
    assert!(context.tagged_heap.sweep_in_progress());
    assert!(!context.tagged_heap.concurrent_mark_running());
    assert_eq!(
        context.gc_count, before,
        "mark termination is not completion"
    );
    for _ in 0..10_000 {
        if !context.tagged_heap.sweep_in_progress() {
            assert_eq!(context.gc_count, before + 1);
            return;
        }
        context.gc_collect_from_current_roots_impl(false);
    }
    panic!("minor sweep did not finish");
}

fn global(context: &Context, name: &str) -> Value {
    context.obarray().symbol_value_copied(name).expect(name)
}

fn integer(context: &Context, name: &str) -> i64 {
    global(context, name).as_int().expect("integer counter")
}

#[test]
fn explicit_major_reclaims_old_graph_and_runs_old_finalizer_before_hook() {
    let mut context = context();
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq u34b-major-graph (cons 151 nil)
                       u34b-finalizer-count 0
                       u34b-hook-count 0
                       u34b-hook-saw-finalizers 0
                       u34b-major-finalizer
                       (make-finalizer
                        (lambda ()
                          (setq u34b-finalizer-count
                                (1+ u34b-finalizer-count)))))",
            )
            .expect("create rooted session graph and finalizer");
    });
    let graph = global(&context, "u34b-major-graph");
    let finalizer = global(&context, "u34b-major-finalizer");
    minor(&mut context);
    assert!(context.tagged_heap.owns_heap_value_for_test(graph));
    assert!(context.tagged_heap.owns_heap_value_for_test(finalizer));
    assert!(context.tagged_heap.value_is_old_for_test(graph));
    assert!(context.tagged_heap.value_is_old_for_test(finalizer));
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq post-gc-hook
                       (list
                        (lambda ()
                          (setq u34b-hook-count (1+ u34b-hook-count)
                                u34b-hook-saw-finalizers u34b-finalizer-count)))
                       u34b-major-graph nil
                       u34b-major-finalizer nil)",
            )
            .expect("release owner frames before collection");
    });
    let completed = context.gc_count;
    let gcs_done = integer(&context, "gcs-done");

    minor(&mut context);
    assert!(context.tagged_heap.owns_heap_value_for_test(graph));
    assert!(context.tagged_heap.owns_heap_value_for_test(finalizer));
    assert_eq!(integer(&context, "u34b-finalizer-count"), 0);
    assert_eq!(integer(&context, "u34b-hook-count"), 1);
    assert_eq!(integer(&context, "u34b-hook-saw-finalizers"), 0);

    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(graph));
    assert!(!context.tagged_heap.owns_heap_value_for_test(finalizer));
    assert_eq!(integer(&context, "u34b-finalizer-count"), 1);
    assert_eq!(integer(&context, "u34b-hook-count"), 2);
    assert_eq!(integer(&context, "u34b-hook-saw-finalizers"), 1);
    assert_eq!(context.gc_count, completed + 2);
    assert_eq!(integer(&context, "gcs-done"), gcs_done + 2);
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());

    context.gc_collect_exact();
    assert_eq!(integer(&context, "u34b-finalizer-count"), 1);
    assert_eq!(integer(&context, "u34b-hook-count"), 3);
    assert_eq!(context.gc_count, completed + 3);
    assert_eq!(integer(&context, "gcs-done"), gcs_done + 3);
}

#[test]
fn explicit_major_drops_old_weak_key_after_real_minor_retention() {
    let mut context = context();
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq u34b-weak-table (make-hash-table :test 'eq :weakness 'key)
                       u34b-weak-key (cons 157 nil))
                 (puthash u34b-weak-key 163 u34b-weak-table)",
            )
            .expect("create rooted weak key");
    });
    let key = global(&context, "u34b-weak-key");
    let table = global(&context, "u34b-weak-table");
    minor(&mut context);
    assert!(context.tagged_heap.owns_heap_value_for_test(key));
    assert!(context.tagged_heap.value_is_old_for_test(key));
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq u34b-weak-key nil)")
            .expect("drop returned key frame");
    });
    minor(&mut context);
    assert!(context.tagged_heap.owns_heap_value_for_test(table));
    assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
    assert!(context.tagged_heap.owns_heap_value_for_test(key));
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(table));
    assert_eq!(table.as_hash_table().unwrap().data.len(), 0);
    assert!(!context.tagged_heap.owns_heap_value_for_test(key));
}

#[test]
fn major_and_minor_keep_only_the_matching_heaps_scratch_root_graph() {
    let roots = ScratchRoots::new();
    let mut first = context();
    let first_child = Value::cons(Value::fixnum(167), Value::NIL);
    let first_child_slot = roots.keep(first_child);
    let first_owner = Value::vector(vec![first_child]);
    let first_owner_slot = roots.keep(first_owner);
    roots.clear(first_child_slot);
    minor(&mut first);
    assert!(first.tagged_heap.owns_heap_value_for_test(first_child));
    assert!(first.tagged_heap.value_is_old_for_test(first_child));
    first.gc_collect_exact();
    assert!(first.tagged_heap.owns_heap_value_for_test(first_owner));
    assert!(first.tagged_heap.owns_heap_value_for_test(first_child));
    assert_eq!(first_owner.as_vector_data().unwrap()[0], first_child);
    assert_eq!(first_child.cons_car(), Value::fixnum(167));

    let mut second = context();
    let second_child = Value::cons(Value::fixnum(173), Value::NIL);
    let second_child_slot = roots.keep(second_child);
    let second_owner = Value::vector(vec![second_child]);
    let second_owner_slot = roots.keep(second_owner);
    roots.clear(second_child_slot);
    minor(&mut second);
    second.gc_collect_exact();
    assert!(second.tagged_heap.owns_heap_value_for_test(second_owner));
    assert!(second.tagged_heap.owns_heap_value_for_test(second_child));
    let mut discovered = Vec::new();
    collect_thread_local_gc_roots(
        &mut discovered,
        second.tagged_heap.identity(),
        second.tagged_heap.gc_collections(),
        crate::tagged::gc::CacheRootScan::Snapshot {
            collection_in_progress: second.tagged_heap.mark_in_progress()
                || second.tagged_heap.sweep_in_progress(),
        },
        &mut Vec::new(),
    );
    assert!(
        discovered
            .iter()
            .any(|(value, _)| value.bits() == second_owner.bits())
    );
    assert!(
        discovered
            .iter()
            .all(|(value, _)| value.bits() != first_owner.bits())
    );
    drop(discovered);

    first.setup_thread_locals();
    minor(&mut first);
    first.gc_collect_exact();
    assert!(first.tagged_heap.owns_heap_value_for_test(first_child));
    assert_eq!(first_child.cons_car(), Value::fixnum(167));
    roots.clear(first_owner_slot);
    first.gc_collect_exact();
    assert!(!first.tagged_heap.owns_heap_value_for_test(first_owner));
    assert!(!first.tagged_heap.owns_heap_value_for_test(first_child));

    second.setup_thread_locals();
    second.gc_collect_exact();
    assert!(second.tagged_heap.owns_heap_value_for_test(second_owner));
    assert!(second.tagged_heap.owns_heap_value_for_test(second_child));
    assert_eq!(second_owner.as_vector_data().unwrap()[0], second_child);
    assert_eq!(second_child.cons_car(), Value::fixnum(173));
    roots.clear(second_owner_slot);
    second.gc_collect_exact();
    assert!(!second.tagged_heap.owns_heap_value_for_test(second_owner));
    assert!(!second.tagged_heap.owns_heap_value_for_test(second_child));
}

#[test]
fn major_and_minor_trace_an_old_graph_rooted_only_by_terminal_parameters() {
    use crate::emacs_core::terminal::pure::{
        builtin_set_terminal_parameter, builtin_terminal_parameter,
    };

    let mut context = context();
    let roots = ScratchRoots::new();
    let key = Value::symbol("u34b-terminal-major-root");
    let child = Value::cons(Value::fixnum(179), Value::NIL);
    let child_slot = roots.keep(child);
    let owner = Value::vector(vec![child]);
    let owner_slot = roots.keep(owner);
    builtin_set_terminal_parameter(&mut context, vec![Value::NIL, key, owner])
        .expect("install sole terminal root");
    roots.clear(child_slot);
    roots.clear(owner_slot);
    minor(&mut context);
    assert!(context.tagged_heap.owns_heap_value_for_test(owner));
    assert!(context.tagged_heap.owns_heap_value_for_test(child));
    assert!(context.tagged_heap.value_is_old_for_test(owner));
    assert!(context.tagged_heap.value_is_old_for_test(child));
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(owner));
    assert!(context.tagged_heap.owns_heap_value_for_test(child));
    assert_eq!(
        builtin_terminal_parameter(&mut context, vec![Value::NIL, key])
            .expect("terminal root remains installed"),
        owner
    );
    assert_eq!(owner.as_vector_data().unwrap()[0], child);
    assert_eq!(child.cons_car(), Value::fixnum(179));

    builtin_set_terminal_parameter(&mut context, vec![Value::NIL, key, Value::NIL])
        .expect("remove sole terminal root");
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(owner));
    assert!(!context.tagged_heap.owns_heap_value_for_test(child));
}
