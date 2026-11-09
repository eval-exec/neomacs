//! Re-homed Tier-H user-test guard test.
//! Context/GC integration: the user-test guard must inhibit GC without a COW
//! entry and restore after every exit. Context fields are read through the
//! signed test-support accessors; the heap probes live in value's
//! `hash_guard_probe` module.
use crate::emacs_core::EvalError;
use crate::emacs_core::builtins::hash_user_callback::with_user_test_guard;
use crate::emacs_core::eval::Context;
use neovm_eval::test_support::authority::{set_gc_pending_for_test, set_gc_stress_for_test};
use neovm_value::tagged::gc::TaggedHeap;
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::set_generational_enabled_for_test;
use neovm_value::test_support::authority::{
    concurrent_hash_mutator_state_for_test, concurrent_hash_scan_policy_is_always_clone_for_test,
    concurrent_obarray_start_slots_present_for_test, hash_snapshot_entry_state_for_test,
    hash_snapshot_present_for_test, hash_snapshot_untouched_for_test,
    set_concurrent_claims_for_test, set_minors_since_major_for_test,
};

/// The Rev R2 nesting count as seen by assertions: 0 outside any user test,
/// one per in-flight activation on the table.
fn depth(table: TaggedValue) -> u8 {
    table
        .as_hash_table()
        .unwrap()
        .test_depth
        .load(std::sync::atomic::Ordering::Acquire)
}

fn wait_for_worker(heap: &TaggedHeap) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !heap.concurrent_mark_done() {
        assert!(
            std::time::Instant::now() < deadline,
            "Tier-H worker did not finish"
        );
        std::thread::yield_now();
    }
}

fn assert_user_guard_did_not_enter_tier_h(context: &Context, table: TaggedValue, epoch: u64) {
    let heap = context.tagged_heap();
    assert!(heap.concurrent_mark_running());
    let (cow, dirty, poisoned) =
        hash_snapshot_entry_state_for_test(heap, table.as_veclike_ptr().unwrap() as usize);
    assert_eq!(cow, hash_snapshot_untouched_for_test());
    assert!(!dirty);
    assert!(!poisoned);
    assert!(!heap.gc_locks_poisoned());
    let (retired, written) = concurrent_hash_mutator_state_for_test(heap);
    // retired_hash_buffers.is_empty()
    assert!(retired == 0);
    assert!(written.is_empty());
    let current = table.as_hash_table().unwrap();
    assert_eq!(current.data.switch_epoch(), epoch);
    assert_eq!(current.data.len(), 1);
}

#[test]
fn tier_h_user_test_guards_inhibit_gc_without_cow_and_restore_after_all_exits() {
    crate::test_utils::init_test_tracing();
    assert!(neovm_value::hashtab::hash_test_parity_enabled());
    for generational in [false, true] {
        // Freeze the policy before Context bootstrap allocates any tables, so
        // the real table enters the chosen Tier-H ownership inventory.
        set_concurrent_claims_for_test(Some(true));
        let mut context = crate::test_utils::new_context();
        set_concurrent_claims_for_test(None);
        // SAFETY: serial fixture heap; the caller holds the only heap view and no
        // collector or compiled leaf runs.
        unsafe { set_generational_enabled_for_test(context.tagged_heap_mut(), generational) };
        // Bootstrap through the actual Context driver before requesting an
        // automatic concurrent major. This fixture deliberately tests that
        // path even when the process-wide suite requests GC stress elsewhere.
        set_gc_stress_for_test(&mut context, false);
        context.set_gc_threshold(usize::MAX);
        context.gc_collect_exact();
        assert!(context.tagged_heap().should_run_concurrent());
        let table = context
            .eval_str(
                "(progn
                   (setq neovm-u35-guard-table (make-hash-table :test 'eq))
                   (puthash 1 7 neovm-u35-guard-table)
                   neovm-u35-guard-table)",
            )
            .expect("create the Lisp-bound table through the real builtins");
        let gcs_done = context.eval_str("gcs-done").unwrap();
        let gc_count = context.gc_count_for_test();
        let epoch = table.as_hash_table().unwrap().data.switch_epoch();
        // SAFETY: the serial fixture owns the only heap view here; no collector
        // or compiled leaf runs across this allocation.
        let inserted =
            unsafe { context.tagged_heap_mut() }.alloc_record(vec![TaggedValue::fixnum(8091)]);
        // A Context termination skips the old obarray cells because the real
        // start handshake scans their snapshot concurrently. A table-only
        // heap start omits that snapshot and would sweep live Context state.
        if generational {
            // SAFETY: same serial exclusive heap view as above.
            set_minors_since_major_for_test(unsafe { context.tagged_heap_mut() }, usize::MAX);
        }
        set_gc_pending_for_test(&mut context, true);
        context.gc_safe_point_exact();
        assert!(context.tagged_heap().concurrent_mark_running());
        assert!(concurrent_obarray_start_slots_present_for_test(
            context.tagged_heap()
        ));
        wait_for_worker(context.tagged_heap());
        assert!(context.tagged_heap().value_is_marked_for_test(table));
        assert!(!context.tagged_heap().value_is_marked_for_test(inserted));

        with_user_test_guard(&mut context, table, |context| {
            assert_eq!(context.gc_inhibit_depth_for_test(), 1);
            assert_eq!(depth(table), 1);
            with_user_test_guard(context, table, |context| {
                assert_eq!(context.gc_inhibit_depth_for_test(), 1);
                assert_eq!(
                    context
                        .eval_str("(gethash 1 neovm-u35-guard-table)")
                        .unwrap()
                        .as_fixnum(),
                    Some(7),
                );
                assert_user_guard_did_not_enter_tier_h(context, table, epoch);
            });
            assert!(
                context
                    .eval_str("(garbage-collect)")
                    .expect("explicit GC obeys the active user-test inhibition")
                    .is_nil()
            );
            assert_eq!(context.eval_str("gcs-done").unwrap(), gcs_done);
            assert_eq!(context.gc_count_for_test(), gc_count);
            assert_user_guard_did_not_enter_tier_h(context, table, epoch);
        });

        for operation in [
            "(puthash 1 7 neovm-u35-guard-table)",
            "(puthash 2 9 neovm-u35-guard-table)",
            "(remhash 1 neovm-u35-guard-table)",
            "(remhash 2 neovm-u35-guard-table)",
            "(clrhash neovm-u35-guard-table)",
        ] {
            let result = with_user_test_guard(&mut context, table, |context| {
                assert_eq!(context.gc_inhibit_depth_for_test(), 1);
                context.eval_str(operation)
            });
            let EvalError::Signal { symbol, data, .. } =
                result.expect_err("the guard rejects writes before the mutation wrapper")
            else {
                panic!("expected the GNU mutation signal");
            };
            assert_eq!(symbol, crate::intern::intern("error"));
            assert_eq!(
                data[0].as_str_owned().as_deref(),
                Some("hash table test modifies table")
            );
            assert_eq!(data[1], table);
            assert_eq!(depth(table), 0);
            assert_eq!(context.gc_inhibit_depth_for_test(), 0);
            assert!(context.user_test_gc_accounting_pointer().is_none());
            assert_user_guard_did_not_enter_tier_h(&context, table, epoch);
        }

        for exit in [
            "(signal 'error '(u35-guard-signal))",
            "(throw 'u35-guard-exit 23)",
        ] {
            let result =
                with_user_test_guard(&mut context, table, |context| context.eval_str(exit));
            assert!(result.is_err());
            assert_eq!(depth(table), 0);
            assert_eq!(context.gc_inhibit_depth_for_test(), 0);
            assert!(context.user_test_gc_accounting_pointer().is_none());
            assert_user_guard_did_not_enter_tier_h(&context, table, epoch);
        }
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_user_test_guard(&mut context, table, |context| {
                assert_eq!(context.gc_inhibit_depth_for_test(), 1);
                panic!("user callback unwinds without entering the Tier-H wrapper");
            });
        }));
        assert!(unwind.is_err());
        assert_eq!(depth(table), 0);
        assert_eq!(context.gc_inhibit_depth_for_test(), 0);
        assert!(context.user_test_gc_accounting_pointer().is_none());
        assert_eq!(context.gc_count_for_test(), gc_count);
        assert_user_guard_did_not_enter_tier_h(&context, table, epoch);

        // Enter the same native puthash used by Lisp without an evaluator safe
        // point first: that would normally terminate this already-done worker.
        assert_eq!(
            crate::emacs_core::builtins::builtin_puthash_3(
                &mut context,
                TaggedValue::fixnum(2),
                inserted,
                table,
            )
            .expect("legal puthash after every guard exit"),
            inserted,
        );
        let (retired, written) = concurrent_hash_mutator_state_for_test(context.tagged_heap());
        assert_eq!(
            retired,
            usize::from(concurrent_hash_scan_policy_is_always_clone_for_test())
        );
        assert_eq!(written, [table]);
        assert_eq!(
            table.as_hash_table().unwrap().data.switch_epoch(),
            epoch.wrapping_add(1)
        );
        assert!(!context.tagged_heap().gc_locks_poisoned());
        // The Context driver re-seeds its complete roots before termination;
        // a hand-written table-only sweep would discard bootstrap state.
        context.gc_collect_exact();
        assert!(!hash_snapshot_present_for_test(context.tagged_heap()));
        let (retired, written) = concurrent_hash_mutator_state_for_test(context.tagged_heap());
        assert!(retired == 0);
        assert!(written.is_empty());
        assert!(context.tagged_heap().owns_heap_value_for_test(inserted));
        assert_eq!(
            context
                .eval_str("(gethash 2 neovm-u35-guard-table)")
                .unwrap(),
            inserted,
        );
        let record = inserted.as_record_data().unwrap();
        assert_eq!(record.len(), 1);
        assert_eq!(record[0], TaggedValue::fixnum(8091));
    }
}
