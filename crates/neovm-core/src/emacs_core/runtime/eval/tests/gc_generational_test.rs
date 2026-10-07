//! Generational collections through the evaluator's real collection entries.

use super::*;

fn generational_context() -> Context {
    // Nextest isolates each test in its own process. Set the constructor knob
    // before creating a Context or starting its background collector.
    unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "1") };
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    // This fixture observes manual deferred minors. Automatic synchronous
    // stress routing is covered separately by gc_generational_pacing.
    context.gc_stress = false;
    assert!(context.tagged_heap.generational_enabled());
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.gc_collect_exact();
    assert!(context.tagged_heap.should_run_minor(false, false));
    context
}

fn run_minor(context: &mut Context) {
    let completed = context.gc_count;
    context.gc_collect_from_current_roots_impl(false);
    assert!(context.tagged_heap.sweep_in_progress());
    assert!(!context.tagged_heap.concurrent_mark_running());
    assert_eq!(
        context.gc_count, completed,
        "mark termination is not completion"
    );
    for _ in 0..10_000 {
        if !context.tagged_heap.sweep_in_progress() {
            assert_eq!(context.gc_count, completed + 1);
            return;
        }
        context.gc_collect_from_current_roots_impl(false);
    }
    panic!("minor sweep did not complete");
}

fn global(context: &Context, name: &str) -> Value {
    context.obarray().symbol_value(name).copied().expect(name)
}

#[test]
fn minor_seeds_symbol_cells_and_buffer_local_slots() {
    let mut context = generational_context();
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(progn
                   (setq u34-symbol-root (cons 11 22))
                   (set (make-local-variable 'u34-local-root) (vector 33 44)))",
            )
            .expect("install roots");
    });
    let symbol_root = global(&context, "u34-symbol-root");
    let local_root = context
        .buffers
        .current_buffer()
        .and_then(|buffer| buffer.buffer_local_value("u34-local-root"))
        .expect("buffer-local root");

    run_minor(&mut context);
    assert_eq!(symbol_root.cons_car(), Value::fixnum(11));
    assert_eq!(symbol_root.cons_cdr(), Value::fixnum(22));
    assert_eq!(
        local_root.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(33), Value::fixnum(44)]
    );
    assert!(context.tagged_heap.value_is_old_for_test(symbol_root));
    assert!(context.tagged_heap.value_is_old_for_test(local_root));
}

#[test]
fn minor_seeds_vm_and_specpdl_root_frames() {
    let mut context = generational_context();
    let spec_roots = context.save_specpdl_roots();
    let spec_payload = Value::cons(Value::fixnum(41), Value::NIL);
    context.push_specpdl_root(spec_payload);
    context.push_vm_root_frame();
    let vm_payload = Value::vector(vec![Value::fixnum(37)]);
    context.push_vm_frame_root(vm_payload);

    run_minor(&mut context);
    assert_eq!(
        vm_payload.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(37)]
    );
    assert_eq!(spec_payload.cons_car(), Value::fixnum(41));
    assert!(context.tagged_heap.value_is_old_for_test(vm_payload));
    assert!(context.tagged_heap.value_is_old_for_test(spec_payload));
    context.pop_vm_root_frame();
    context.restore_specpdl_roots(spec_roots);
}

#[test]
fn minor_hooks_and_gcs_done_run_once_after_sweep_completion() {
    let mut context = generational_context();
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq u34-hook-count 0
                       post-gc-hook
                       (list (lambda ()
                               (setq u34-hook-count (1+ u34-hook-count)))))",
            )
            .expect("install hook");
    });
    let before = global(&context, "gcs-done").as_int().unwrap();
    let conses = context.tagged_heap.memory_use_counts_snapshot()
        [crate::tagged::gc::MemoryUseCountSlot::ConsCells.index()];
    context.gc_collect_from_current_roots_impl(false);
    assert!(context.tagged_heap.sweep_in_progress());
    assert_eq!(global(&context, "gcs-done").as_int(), Some(before));
    assert_eq!(global(&context, "u34-hook-count").as_int(), Some(0));
    while context.tagged_heap.sweep_in_progress() {
        context.gc_collect_from_current_roots_impl(false);
    }
    assert_eq!(global(&context, "gcs-done").as_int(), Some(before + 1));
    assert_eq!(global(&context, "u34-hook-count").as_int(), Some(1));
    assert_eq!(
        context.tagged_heap.memory_use_counts_snapshot()
            [crate::tagged::gc::MemoryUseCountSlot::ConsCells.index()],
        conses,
        "collection does not reset cumulative allocation counts"
    );
    context.gc_collect_exact();
    assert_eq!(global(&context, "gcs-done").as_int(), Some(before + 2));
    assert_eq!(global(&context, "u34-hook-count").as_int(), Some(2));
}

#[test]
fn explicit_collection_keeps_the_full_synchronous_entry() {
    let mut context = generational_context();
    let reachable = Value::cons(Value::fixnum(53), Value::NIL);
    context.assign("u34-explicit-root", reachable);
    for _ in 0..128 {
        let _ = Value::cons(Value::fixnum(61), Value::NIL);
    }
    let allocated = context.tagged_heap.allocated_count();
    let completed = context.gc_count;
    context.gc_collect_exact();
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());
    assert_eq!(context.gc_count, completed + 1);
    assert!(context.tagged_heap.allocated_count() < allocated);
    assert_eq!(reachable.cons_car(), Value::fixnum(53));
    assert!(
        context.tagged_heap.value_is_old_for_test(reachable),
        "the explicit major promotes surviving conses"
    );
}

#[cfg(feature = "jit")]
#[test]
fn compiled_old_cons_stores_remember_young_children_each_cycle() {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
    use crate::emacs_core::jit::compile::{NativeRun, compile_bytecode_function};
    use crate::emacs_core::value::LambdaParams;

    let mut context = generational_context();
    let owner = Value::cons(Value::NIL, Value::NIL);
    context.assign("u34-compiled-owner", owner);
    run_minor(&mut context);
    assert!(context.tagged_heap.value_is_old_for_test(owner));

    for (index, op) in [Op::Setcar, Op::Setcdr].into_iter().enumerate() {
        let mut function = ByteCodeFunction::new(LambdaParams {
            required: vec![
                crate::emacs_core::intern::SymId(1),
                crate::emacs_core::intern::SymId(2),
            ],
            optional: Vec::new(),
            rest: None,
        });
        function.lexical = true;
        function.ops = vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return];
        function.max_stack = 8;
        let leaf = compile_bytecode_function(&function).expect("cons store compiles");
        let child_roots = context.save_specpdl_roots();
        let child = Value::cons(Value::fixnum(71 + index as i64), Value::NIL);
        context.push_specpdl_root(child);
        let context_ptr = &mut context as *mut Context as *mut u8;
        let result = leaf.call(context_ptr, &[owner, child]);
        assert!(matches!(result, NativeRun::Ok(bits) if Value::from_bits(bits) == child));
        assert_eq!(context.tagged_heap.remembered_log_len_for_test(), 1);
        context.restore_specpdl_roots(child_roots);

        run_minor(&mut context);
        let stored = if index == 0 {
            owner.cons_car()
        } else {
            owner.cons_cdr()
        };
        assert_eq!(stored, child);
        assert_eq!(stored.cons_car(), Value::fixnum(71 + index as i64));
        assert!(context.tagged_heap.value_is_old_for_test(stored));
        assert_eq!(context.tagged_heap.remembered_log_len_for_test(), 0);
    }
}
