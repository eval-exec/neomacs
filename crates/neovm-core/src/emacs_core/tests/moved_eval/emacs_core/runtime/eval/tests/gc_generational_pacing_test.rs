//! Major limits, memory-full selection and synchronous automatic stress.
//! Context completion counts and hooks publish once after the entire sweep.

use super::*;
use strum::IntoEnumIterator;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompletionCounts {
    context: u64,
    gcs_done: i64,
    hooks: i64,
}

fn integer(context: &Context, name: &str) -> i64 {
    context
        .obarray()
        .symbol_value_copied(name)
        .expect(name)
        .as_int()
        .expect("integer counter")
}

fn counts(context: &Context) -> CompletionCounts {
    CompletionCounts {
        context: context.gc_count_for_test(),
        gcs_done: integer(context, "gcs-done"),
        hooks: integer(context, "u34p-hook-count"),
    }
}

fn assert_one_completion(context: &Context, before: CompletionCounts) {
    assert_eq!(
        counts(context),
        CompletionCounts {
            context: before.context + 1,
            gcs_done: before.gcs_done + 1,
            hooks: before.hooks + 1,
        }
    );
    assert_eq!(
        integer(context, "u34p-hook-saw-gcs"),
        integer(context, "gcs-done")
    );
}

fn context(max_minors: usize, stress_every: usize) -> Context {
    // Nextest executes each test in its own process. The heap constructor
    // reads the generation/pacing variables once. Avoid absolute boot counts.
    unsafe {
        std::env::set_var("NEOVM_GC_GENERATIONAL", "1");
        std::env::set_var("NEOVM_GC_MAJOR_MAX_MINORS", max_minors.to_string());
        std::env::set_var("NEOVM_GC_MAJOR_GROWTH_PERCENT", usize::MAX.to_string());
        std::env::set_var("NEOVM_GC_STRESS_MAJOR_EVERY", stress_every.to_string());
    }
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::new_context();
    // The stress-specific test enables it at the real automatic entry after
    // bounded, rooted fixture setup. Other tests control automatic selection.
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut context, false);
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq gc-cons-threshold most-positive-fixnum
                                 gc-cons-percentage 1000
                                 memory-full nil)",
            )
            .expect("controlled fixture pacing");
    });
    context.gc_collect_exact();
    assert!(context.tagged_heap().tagged_heap_generational_enabled());
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq u34p-hook-count 0 u34p-hook-saw-gcs 0
                                 post-gc-hook
                                 (list (lambda ()
                                         (setq u34p-hook-count (1+ u34p-hook-count)
                                               u34p-hook-saw-gcs gcs-done))))",
            )
            .expect("install one exact completion callback");
    });
    context
}

fn make_old_unreachable_sentinel(context: &mut Context) {
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq u34p-table (make-hash-table :test 'eq :weakness 'key)
                                 u34p-sentinel (cons 211 nil))
                          (puthash u34p-sentinel 211 u34p-table)",
            )
            .expect("root the weak table and sentinel before major");
    });
    let sentinel = context
        .obarray()
        .symbol_value_copied("u34p-sentinel")
        .unwrap();
    let before = counts(context);
    context.gc_collect_exact();
    assert_one_completion(context, before);
    assert!(context.tagged_heap().value_is_old_for_test(sentinel));
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq u34p-sentinel nil)")
            .expect("remove the only strong sentinel root");
    });
}

fn weak_entries(context: &Context) -> usize {
    // The table stays rooted in its real symbol cell. Its count records the
    // mark-time decision even if hook allocations reuse a freed key address.
    let table = context.obarray().symbol_value_copied("u34p-table").unwrap();
    assert!(context.tagged_heap().owns_heap_value_for_test(table));
    table.as_hash_table().unwrap().data.len()
}

fn run_due_cycle(context: &mut Context, minor: bool) {
    let before = counts(context);
    // This is the real due-cycle body, with explicit=false. It deliberately
    // controls one due decision without changing the threshold arithmetic.
    context.gc_collect_from_current_roots_impl(false);
    if minor {
        assert!(context.tagged_heap().sweep_in_progress());
        assert!(!context.tagged_heap().concurrent_mark_running());
    } else {
        assert!(context.tagged_heap().concurrent_mark_running());
        assert!(!context.tagged_heap().sweep_in_progress());
    }
    assert_eq!(
        counts(context),
        before,
        "a start/termination is not completion"
    );
    if !minor {
        let wait = std::time::Instant::now();
        while !context.tagged_heap().concurrent_mark_done() {
            assert!(
                wait.elapsed() < std::time::Duration::from_secs(30),
                "worker did not drain"
            );
            std::thread::yield_now();
        }
        context.gc_collect_from_current_roots_impl(false);
        assert!(!context.tagged_heap().concurrent_mark_running());
        assert!(context.tagged_heap().sweep_in_progress());
        assert_eq!(
            counts(context),
            before,
            "major termination is not completion"
        );
    }
    for _ in 0..10_000 {
        if !context.tagged_heap().sweep_in_progress() {
            assert!(!context.tagged_heap().mark_in_progress());
            assert_one_completion(context, before);
            return;
        }
        context.gc_collect_from_current_roots_impl(false);
    }
    panic!("deferred sweep did not complete");
}

fn run_stress_cycle(context: &mut Context) {
    let before = counts(context);
    // C2.7 must preserve synchronous automatic stress while choosing the
    // generation: this wrapper is not the explicit-full entry.
    context.gc_collect_from_current_roots();
    assert!(!context.tagged_heap().concurrent_mark_running());
    assert!(!context.tagged_heap().mark_in_progress());
    assert!(!context.tagged_heap().sweep_in_progress());
    assert_one_completion(context, before);
}

#[test]
fn generational_minor_cap_selects_major_only_after_completed_minors() {
    let mut context = context(2, 8);
    make_old_unreachable_sentinel(&mut context);
    for _ in 0..2 {
        run_due_cycle(&mut context, true);
        assert_eq!(weak_entries(&context), 1);
    }
    run_due_cycle(&mut context, false);
    assert_eq!(weak_entries(&context), 0);
    // The completed major resets the cap. Continuation polls did not credit
    // extra minors, and two new minors are again permitted before a major.
    run_due_cycle(&mut context, true);
    run_due_cycle(&mut context, true);
    run_due_cycle(&mut context, false);
}

#[test]
fn generational_memory_full_uses_live_lisp_setting_to_select_major() {
    let mut context = context(usize::MAX, 8);
    make_old_unreachable_sentinel(&mut context);
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq memory-full t)")
            .expect("set the real runtime setting");
    });
    assert!(context.gc_runtime_memory_full_for_test());
    run_due_cycle(&mut context, false);
    assert_eq!(weak_entries(&context), 0);
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq memory-full nil)")
            .expect("clear the real runtime setting");
    });
    assert!(!context.gc_runtime_memory_full_for_test());
    run_due_cycle(&mut context, true);
}

#[test]
fn generational_automatic_stress_is_synchronous_seven_minors_then_eighth_major() {
    let mut context = context(usize::MAX, 8);
    make_old_unreachable_sentinel(&mut context);
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut context, true);
    for cycle in 1..=8 {
        run_stress_cycle(&mut context);
        assert_eq!(
            weak_entries(&context),
            usize::from(cycle < 8),
            "the first old sentinel is reclaimed only by cycle eight"
        );
    }
    // Establish another old sentinel without an intervening explicit major:
    // cycle nine promotes it, so the next major's reset is tested physically.
    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq u34p-sentinel (cons 223 nil))
                          (puthash u34p-sentinel 223 u34p-table)",
            )
            .expect("root the next young sentinel");
    });
    let second = context
        .obarray()
        .symbol_value_copied("u34p-sentinel")
        .unwrap();
    run_stress_cycle(&mut context); // cycle nine, first minor after major
    assert!(context.tagged_heap().value_is_old_for_test(second));
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq u34p-sentinel nil)")
            .expect("drop its only strong root");
    });
    for cycle in 10..=16 {
        run_stress_cycle(&mut context);
        assert_eq!(
            weak_entries(&context),
            usize::from(cycle < 16),
            "a completed automatic major resets the stress stride"
        );
    }
}

#[test]
fn generational_explicit_collection_is_full_and_resets_automatic_stress_stride() {
    let mut context = context(usize::MAX, 8);
    make_old_unreachable_sentinel(&mut context);
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut context, true);
    for _ in 0..3 {
        run_stress_cycle(&mut context);
        assert_eq!(weak_entries(&context), 1);
    }
    let before = counts(&context);
    context.gc_collect_exact();
    assert_one_completion(&context, before);
    assert_eq!(weak_entries(&context), 0);
    assert!(!context.tagged_heap().concurrent_mark_running());
    assert!(!context.tagged_heap().mark_in_progress());
    assert!(!context.tagged_heap().sweep_in_progress());

    context.with_gc_inhibited(|context| {
        context
            .eval_str(
                "(setq u34p-sentinel (cons 227 nil))
                          (puthash u34p-sentinel 227 u34p-table)",
            )
            .expect("root another young sentinel");
    });
    let second = context
        .obarray()
        .symbol_value_copied("u34p-sentinel")
        .unwrap();
    run_stress_cycle(&mut context); // first new minor promotes it
    assert!(context.tagged_heap().value_is_old_for_test(second));
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq u34p-sentinel nil)")
            .expect("drop its only strong root");
    });
    for cycle in 2..=8 {
        run_stress_cycle(&mut context);
        assert_eq!(weak_entries(&context), usize::from(cycle < 8));
    }
}

#[test]
fn generational_memory_full_waits_for_due_then_selects_major_at_real_safe_point() {
    let mut context = context(usize::MAX, 8);
    make_old_unreachable_sentinel(&mut context);
    // SAFETY: test-only owner-thread heap policy setup through the non-swappable mutable accessor contract; the view is used in place and never swapped or moved across Worlds.
    unsafe { context.tagged_heap_mut() }.set_gc_threshold(usize::MAX);
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq memory-full t)")
            .expect("set memory-full through Lisp");
    });
    assert!(context.gc_runtime_memory_full_for_test());
    let before = counts(&context);
    context.gc_safe_point_exact();
    assert_eq!(
        counts(&context),
        before,
        "memory-full selects generation only when a cycle is due"
    );
    assert!(!context.tagged_heap().mark_in_progress());
    assert!(!context.tagged_heap().sweep_in_progress());
    assert_eq!(weak_entries(&context), 1);

    // SAFETY: test-only owner-thread heap policy setup through the non-swappable mutable accessor contract; the view is used in place and never swapped or moved across Worlds.
    unsafe { context.tagged_heap_mut() }.set_gc_threshold(1);
    assert!(context.tagged_heap().should_collect());
    context.gc_safe_point_exact();
    assert!(context.tagged_heap().concurrent_mark_running());
    assert_eq!(counts(&context), before, "start is not completion");
    let wait = std::time::Instant::now();
    while context.gc_count_for_test() == before.context {
        context.gc_safe_point_exact();
        assert!(
            wait.elapsed() < std::time::Duration::from_secs(30),
            "major did not complete"
        );
        std::thread::yield_now();
    }
    assert!(!context.tagged_heap().mark_in_progress());
    assert!(!context.tagged_heap().sweep_in_progress());
    assert_one_completion(&context, before);
    assert_eq!(weak_entries(&context), 0);
}

#[test]
fn generational_done_poll_terminates_immediately_and_folds_resident_satb_partial() {
    let mut context = context(usize::MAX, 8);
    context.with_gc_inhibited(|context| {
        context.eval_str("(setq memory-full t)").unwrap();
    });
    // This preexisting orphan is absent from the Context root set and the
    // seeded root closure. Its overwritten root-cell preimage is its only mark edge.
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation and is never swapped or moved across Worlds.
    let orphan = unsafe { context.tagged_heap_mut() }.alloc_cons(Value::fixnum(317), Value::NIL);
    context.gc_collect_from_current_roots_impl(false);
    assert!(context.tagged_heap().concurrent_mark_running());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !context.tagged_heap().concurrent_mark_done() {
        assert!(std::time::Instant::now() < deadline, "worker did not drain");
        std::thread::yield_now();
    }
    assert!(
        context.tagged_heap().bytes_since_gc()
            <= context.tagged_heap().gc_threshold().saturating_mul(4)
    );
    assert!(!context.tagged_heap().value_is_marked_for_test(orphan));
    neovm_value::tagged::gc::note_root_overwrite(orphan);
    assert!(context.tagged_heap().concurrent_mark_done());
    // A single real safe point must stop and fold now; the previous defer-once
    // path would publish this word and leave the concurrent mark running.
    context.gc_safe_point_exact();
    assert!(!context.tagged_heap().concurrent_mark_running());
    assert!(context.tagged_heap().sweep_in_progress());
    assert_eq!(
        context
            .tagged_heap()
            .sweep_stats()
            .last_termination_partial_words,
        1
    );
    assert!(context.tagged_heap().value_is_marked_for_test(orphan));
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    unsafe { context.tagged_heap_mut() }.finish_incremental_sweep_now();
    assert!(context.tagged_heap().owns_heap_value_for_test(orphan));
}

#[derive(Clone, Copy, Debug, strum::EnumIter)]
enum ResidualMajorEntry {
    DroppedCapture,
    JoinedWorker,
}

#[test]
fn scheduler_finishes_residual_major_and_skips_minor() {
    for entry in ResidualMajorEntry::iter() {
        let mut context = context(usize::MAX, usize::MAX);
        context.with_gc_inhibited(|context| {
            context
                .eval_str("(setq u34p-retained (cons (vector 239) 241))")
                .expect("root a young graph in the real obarray");
        });
        let root = context
            .obarray()
            .symbol_value_copied("u34p-retained")
            .unwrap();
        let child = root.cons_car();
        assert!(!context.tagged_heap().value_is_old_for_test(root));
        assert!(context.tagged_heap().should_run_minor(false, false));
        let before = counts(&context);
        let heap_collections = context.tagged_heap().gc_collections();
        let heap_ptr: *mut neovm_value::tagged::gc::TaggedHeap =
            neovm_eval::test_support::authority::world_view_heap_root(context.tagged_heap())
                .as_ptr();
        // SAFETY: this Context owns the installed heap and every runtime root.
        // Its mutator is stopped through admission, root capture and optional
        // worker publication. Root enumeration only seeds collector work and
        // cannot run Lisp callbacks or allocation safepoints.
        unsafe {
            let mut capture = (*heap_ptr).permit_concurrent_mark().unwrap().begin();
            let captured_heap = capture.heap_mut() as *mut neovm_value::tagged::gc::TaggedHeap;
            context.seed_registered_mutator_roots_world_stopped(captured_heap);
            match entry {
                ResidualMajorEntry::DroppedCapture => drop(capture),
                ResidualMajorEntry::JoinedWorker => {
                    capture.launch().unwrap();
                    (*heap_ptr).finish_concurrent_mark().unwrap();
                }
            }
        }
        assert!(context.tagged_heap().mark_in_progress(), "{entry:?}");
        assert!(
            !context.tagged_heap().concurrent_mark_running(),
            "{entry:?}"
        );
        assert!(!context.tagged_heap().sweep_in_progress(), "{entry:?}");
        assert_eq!(
            counts(&context),
            before,
            "starting or joining is not completion"
        );
        assert_eq!(context.tagged_heap().gc_collections(), heap_collections);

        // A fresh due cycle would select a minor with these pacing limits.
        // This invocation instead finishes the pending major and accounts it
        // exactly once; that completed major already includes young garbage.
        context.gc_collect_from_current_roots_impl(false);
        assert_one_completion(&context, before);
        assert_eq!(context.tagged_heap().gc_collections(), heap_collections + 1);
        assert!(!context.tagged_heap().mark_in_progress(), "{entry:?}");
        assert!(
            !context.tagged_heap().concurrent_mark_running(),
            "{entry:?}"
        );
        assert!(!context.tagged_heap().sweep_in_progress(), "{entry:?}");
        assert_eq!(
            context.obarray().symbol_value_copied("u34p-retained"),
            Some(root)
        );
        assert!(context.tagged_heap().owns_heap_value_for_test(root));
        assert!(context.tagged_heap().owns_heap_value_for_test(child));
        assert!(context.tagged_heap().value_is_old_for_test(root));
        assert!(context.tagged_heap().value_is_old_for_test(child));
        assert_eq!(root.cons_car(), child);
        assert_eq!(
            root.cons_cdr(),
            neovm_value::tagged::value::TaggedValue::fixnum(241)
        );
        assert_eq!(
            child.as_vector_data().unwrap()[0],
            neovm_value::tagged::value::TaggedValue::fixnum(239)
        );
    }
}
