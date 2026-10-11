//! Major limits, memory-full selection and synchronous automatic stress.
//! Context completion counts and hooks publish once after the entire sweep.

use super::*;

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
        context: context.gc_count,
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
    let mut context = Context::new();
    // The stress-specific test enables it at the real automatic entry after
    // bounded, rooted fixture setup. Other tests control automatic selection.
    context.gc_stress = false;
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
    assert!(context.tagged_heap.generational_enabled());
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
    assert!(context.tagged_heap.value_is_old_for_test(sentinel));
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
    assert!(context.tagged_heap.owns_heap_value_for_test(table));
    table.as_hash_table().unwrap().data.len()
}

fn run_due_cycle(context: &mut Context, minor: bool) {
    let before = counts(context);
    // This is the real due-cycle body, with explicit=false. It deliberately
    // controls one due decision without changing the threshold arithmetic.
    context.gc_collect_from_current_roots_impl(false);
    if minor {
        assert!(context.tagged_heap.sweep_in_progress());
        assert!(!context.tagged_heap.concurrent_mark_running());
    } else {
        assert!(context.tagged_heap.concurrent_mark_running());
        assert!(!context.tagged_heap.sweep_in_progress());
    }
    assert_eq!(
        counts(context),
        before,
        "a start/termination is not completion"
    );
    if !minor {
        let wait = std::time::Instant::now();
        while !context.tagged_heap.concurrent_mark_done() {
            assert!(
                wait.elapsed() < std::time::Duration::from_secs(30),
                "worker did not drain"
            );
            std::thread::yield_now();
        }
        context.gc_collect_from_current_roots_impl(false);
        assert!(!context.tagged_heap.concurrent_mark_running());
        assert!(context.tagged_heap.sweep_in_progress());
        assert_eq!(
            counts(context),
            before,
            "major termination is not completion"
        );
    }
    for _ in 0..10_000 {
        if !context.tagged_heap.sweep_in_progress() {
            assert!(!context.tagged_heap.mark_in_progress());
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
    assert!(!context.tagged_heap.concurrent_mark_running());
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());
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
    assert!(context.gc_runtime_settings_cache.memory_full.is_full());
    run_due_cycle(&mut context, false);
    assert_eq!(weak_entries(&context), 0);
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq memory-full nil)")
            .expect("clear the real runtime setting");
    });
    assert!(!context.gc_runtime_settings_cache.memory_full.is_full());
    run_due_cycle(&mut context, true);
}

#[test]
fn generational_automatic_stress_is_synchronous_seven_minors_then_eighth_major() {
    let mut context = context(usize::MAX, 8);
    make_old_unreachable_sentinel(&mut context);
    context.gc_stress = true;
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
    assert!(context.tagged_heap.value_is_old_for_test(second));
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
    context.gc_stress = true;
    for _ in 0..3 {
        run_stress_cycle(&mut context);
        assert_eq!(weak_entries(&context), 1);
    }
    let before = counts(&context);
    context.gc_collect_exact();
    assert_one_completion(&context, before);
    assert_eq!(weak_entries(&context), 0);
    assert!(!context.tagged_heap.concurrent_mark_running());
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());

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
    assert!(context.tagged_heap.value_is_old_for_test(second));
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
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.with_gc_inhibited(|context| {
        context
            .eval_str("(setq memory-full t)")
            .expect("set memory-full through Lisp");
    });
    assert!(context.gc_runtime_settings_cache.memory_full.is_full());
    let before = counts(&context);
    context.gc_safe_point_exact();
    assert_eq!(
        counts(&context),
        before,
        "memory-full selects generation only when a cycle is due"
    );
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());
    assert_eq!(weak_entries(&context), 1);

    context.tagged_heap.set_gc_threshold(1);
    assert!(context.tagged_heap.should_collect());
    context.gc_safe_point_exact();
    assert!(context.tagged_heap.concurrent_mark_running());
    assert_eq!(counts(&context), before, "start is not completion");
    let wait = std::time::Instant::now();
    while context.gc_count == before.context {
        context.gc_safe_point_exact();
        assert!(
            wait.elapsed() < std::time::Duration::from_secs(30),
            "major did not complete"
        );
        std::thread::yield_now();
    }
    assert!(!context.tagged_heap.mark_in_progress());
    assert!(!context.tagged_heap.sweep_in_progress());
    assert_one_completion(&context, before);
    assert_eq!(weak_entries(&context), 0);
}
