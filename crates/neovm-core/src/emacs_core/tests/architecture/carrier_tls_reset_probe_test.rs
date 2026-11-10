//! C1 real-provider carrier reset regressions, hosted by the core test target.

#[test]
fn deferred_gc_reset_rejects_a_live_heap_mutation_extent_without_clearing_it() {
    std::thread::spawn(|| {
        use neovm_value::tagged::mutate::{HeapMutClosureGuard, in_heap_mut_closure_for_test};
        assert!(!in_heap_mut_closure_for_test());
        // Hold a real retired-job guard across the next job's boundary.
        let leaked_guard = HeapMutClosureGuard::enter();
        let reset = std::panic::catch_unwind(neovm_value::tagged::gc::reset_carrier_tls);
        let still_active = in_heap_mut_closure_for_test();
        drop(leaked_guard);
        assert!(!in_heap_mut_closure_for_test());
        assert!(
            reset.is_err(),
            "a live mutation extent crossed the job boundary"
        );
        assert!(
            still_active,
            "the failing reset erased the mutation invariant"
        );
        neovm_value::tagged::gc::reset_carrier_tls();
        assert!(!in_heap_mut_closure_for_test());
    })
    .join()
    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

#[cfg(feature = "vm-profile")]
#[test]
fn variable_cache_events_survive_park_but_reset_between_jobs() {
    let observed = neovm_eval::test_support::authority::var_cache_carrier_reset_probe_for_test();
    assert_eq!(observed.seeded_count(), 1);
    assert_eq!(
        observed.parked_count(),
        1,
        "an ordinary park must preserve the measured interval"
    );
    for count in observed.reset_counts() {
        assert_eq!(count, 0);
    }
}
