//! GNU synchronization pseudovectors have ordinary reachability, not a native
//! all-ever registry: thread.c:319-322,434-438 allocate them; :410-413,592-595
//! destroy unreachable payloads. A successful mutex-lock clears event_object
//! on return (:344-368); owning a lock therefore adds no retained-object edge.
//! A condition's sole association retains its mutex (:434-438).

use super::{ThreadingObjectCount, threading_object_count};
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::threads::{MutexAcquisition, MutexRelease, ThreadingKind};
use neovm_value::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SynchronizationCounts {
    mutexes: ThreadingObjectCount,
    conditions: ThreadingObjectCount,
}

fn counts(context: &mut Context) -> SynchronizationCounts {
    SynchronizationCounts {
        mutexes: threading_object_count(context, ThreadingKind::Mutex),
        conditions: threading_object_count(context, ThreadingKind::CondVar),
    }
}

fn initialized_world() -> Context {
    let mut context = Context::new_with_threading(ThreadingMode::Sim);
    context.gc_stress = false;
    context.gc_collect_exact();
    context.tagged_heap.set_gc_threshold(usize::MAX);
    assert_eq!(
        counts(&mut context),
        SynchronizationCounts {
            mutexes: ThreadingObjectCount(0),
            conditions: ThreadingObjectCount(0),
        }
    );
    context
}

/// One actual minor in GEN1, one full in GEN0. Drive that single collection's
/// incremental work through Context, including its completion accounting.
/// This never retries a failed assertion or starts another collection until
/// something passes. The following full is a separate, fixed phase in each test.
fn collect_young_or_full(context: &mut Context) {
    let before = context.gc_count;
    if context.tagged_heap.tagged_heap_generational_enabled() {
        assert!(context.tagged_heap.should_run_minor(false, false));
        context.gc_pending = true;
        context.gc_safe_point_exact();
        assert!(context.tagged_heap.sweep_in_progress());
        assert!(!context.tagged_heap.concurrent_mark_running());
        assert_eq!(
            context.gc_count, before,
            "mark termination is not completion"
        );
        // command_loop.rs:2694 updates gc_count only when Context observes the
        // completed cycle. Draining TaggedHeap directly bypasses that entire
        // completion block (statistics, finalizers and post-gc-hook included).
        // Match gc_generational_test.rs::run_minor: bounded product work,
        // stopping as soon as this same sweep finishes, never an assertion poll.
        for _ in 0..10_000 {
            if !context.tagged_heap.sweep_in_progress() {
                break;
            }
            context.gc_safe_point_exact();
        }
        assert!(!context.tagged_heap.sweep_in_progress());
        assert!(!context.tagged_heap.mark_in_progress());
    } else {
        context.gc_collect_exact();
    }
    assert_eq!(context.gc_count, before + 1);
}

#[test]
fn p15_reclamation_mutex_churn_returns_exactly_to_baseline() {
    let mut context = initialized_world();
    let baseline = counts(&mut context);
    for _burst in 0..3 {
        for _object in 0..64 {
            let mutex = context.threads.create_mutex(None).unwrap();
            assert!(context.tagged_heap.owns_heap_value_for_test(mutex.value()));
        }
        assert_eq!(
            counts(&mut context),
            SynchronizationCounts {
                mutexes: ThreadingObjectCount(baseline.mutexes.0 + 64),
                conditions: baseline.conditions,
            }
        );
        collect_young_or_full(&mut context);
        assert_eq!(counts(&mut context), baseline);
        context.gc_collect_exact();
        assert_eq!(counts(&mut context), baseline);
    }
}

#[test]
fn p15_reclamation_condition_and_mutex_churn_returns_exactly_to_baseline() {
    let mut context = initialized_world();
    let baseline = counts(&mut context);
    for _burst in 0..3 {
        for _object in 0..64 {
            let mutex = context.threads.create_mutex(None).unwrap();
            let condition = context
                .threads
                .create_condition_variable(mutex, None)
                .unwrap();
            assert!(
                context
                    .tagged_heap
                    .owns_heap_value_for_test(condition.value())
            );
            assert!(context.tagged_heap.owns_heap_value_for_test(mutex.value()));
        }
        assert_eq!(
            counts(&mut context),
            SynchronizationCounts {
                mutexes: ThreadingObjectCount(baseline.mutexes.0 + 64),
                conditions: ThreadingObjectCount(baseline.conditions.0 + 64),
            }
        );
        collect_young_or_full(&mut context);
        assert_eq!(counts(&mut context), baseline);
        context.gc_collect_exact();
        assert_eq!(counts(&mut context), baseline);
    }
}

#[test]
fn p15_reclamation_locked_mutex_is_a_weak_only_key() {
    let mut context = initialized_world();
    context
        .eval_str("(setq p15-held-weak (make-hash-table :test 'eq :weakness 'key))")
        .unwrap();
    let baseline = counts(&mut context);
    let mutex = context.threads.create_mutex(None).unwrap();
    assert_eq!(
        context.threads.mutex_lock(mutex),
        MutexAcquisition::Acquired
    );
    assert_eq!(
        context.threads.mutex_lock(mutex),
        MutexAcquisition::Acquired
    );
    context.set_variable("p15-held-key", mutex.value());
    context
        .eval_str("(puthash p15-held-key t p15-held-weak)")
        .unwrap();
    context.eval_str("(setq p15-held-key nil)").unwrap();
    // The only Lisp edge is a weak key; the owner's native id is not a root.
    // A Rust handle/Value local likewise registers no root lease.
    collect_young_or_full(&mut context);
    assert!(!context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    assert_eq!(counts(&mut context), baseline);
    assert_eq!(
        context
            .eval_str("(hash-table-count p15-held-weak)")
            .unwrap(),
        Value::fixnum(0)
    );
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    assert_eq!(counts(&mut context), baseline);
    // Never dereference or unlock the collected mutex through the old handle.
}

#[test]
fn p15_reclamation_reachable_locked_mutex_preserves_recursive_ownership() {
    let mut context = initialized_world();
    let baseline = counts(&mut context);
    let mutex = context.threads.create_mutex(None).unwrap();
    context.set_variable("p15-reachable-mutex", mutex.value());
    assert_eq!(
        context.threads.mutex_lock(mutex),
        MutexAcquisition::Acquired
    );
    assert_eq!(
        context.threads.mutex_lock(mutex),
        MutexAcquisition::Acquired
    );
    collect_young_or_full(&mut context);
    assert!(context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    assert!(context.threads.mutex_owned_by_current_thread(mutex));
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    assert!(context.threads.mutex_owned_by_current_thread(mutex));
    assert_eq!(
        context.threads.mutex_unlock(mutex),
        MutexRelease::StillLocked
    );
    assert_eq!(context.threads.mutex_unlock(mutex), MutexRelease::Released);
    assert_eq!(context.threads.mutex_unlock(mutex), MutexRelease::NotOwner);
    assert_eq!(
        counts(&mut context),
        SynchronizationCounts {
            mutexes: ThreadingObjectCount(baseline.mutexes.0 + 1),
            conditions: baseline.conditions,
        }
    );
    context.set_variable("p15-reachable-mutex", Value::NIL);
    // This object has already tenured; a full collection decides its death.
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    context.gc_collect_exact();
    assert_eq!(counts(&mut context), baseline);
}

#[test]
fn p15_reclamation_reachable_condition_is_the_only_mutex_edge() {
    let mut context = initialized_world();
    let baseline = counts(&mut context);
    let mutex = context.threads.create_mutex(None).unwrap();
    let condition = context
        .threads
        .create_condition_variable(mutex, None)
        .unwrap();
    // Retain only the condition. No mutex variable, VM root or transport lease
    // can mask an omitted association visitor in the canonical child schema.
    context.set_variable("p15-reachable-condition", condition.value());
    collect_young_or_full(&mut context);
    assert!(
        context
            .tagged_heap
            .owns_heap_value_for_test(condition.value())
    );
    assert!(context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    assert_eq!(
        context.threads.condition_variable_mutex(condition).value(),
        mutex.value()
    );
    context.gc_collect_exact();
    assert!(
        context
            .tagged_heap
            .owns_heap_value_for_test(condition.value())
    );
    assert!(context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    assert_eq!(
        context.threads.condition_variable_mutex(condition).value(),
        mutex.value()
    );
    assert_eq!(
        counts(&mut context),
        SynchronizationCounts {
            mutexes: ThreadingObjectCount(baseline.mutexes.0 + 1),
            conditions: ThreadingObjectCount(baseline.conditions.0 + 1),
        }
    );
    context.set_variable("p15-reachable-condition", Value::NIL);
    context.gc_collect_exact();
    assert!(
        !context
            .tagged_heap
            .owns_heap_value_for_test(condition.value())
    );
    assert!(!context.tagged_heap.owns_heap_value_for_test(mutex.value()));
    context.gc_collect_exact();
    assert_eq!(counts(&mut context), baseline);
}
