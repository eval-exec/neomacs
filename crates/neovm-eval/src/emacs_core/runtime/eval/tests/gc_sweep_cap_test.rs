//! An incremental sweep's slices follow allocation: a safe point runs one
//! only when the allocation since mark termination has caught up with the
//! sweep's progress. The overdue drain is the backstop: once a full
//! threshold's worth has been consed since the mark terminated, the next
//! cycle is due, and the sweep must finish at that safe point instead of
//! letting the garbage pile into the next cycle.

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

fn drive_until_sweeping(ev: &mut Context) {
    for _ in 0..20_000 {
        if ev.tagged_heap.sweep_in_progress() {
            return;
        }
        ev.gc_collect_from_current_roots();
        if ev.tagged_heap.mark_in_progress() {
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
    }
    panic!("no incremental sweep started");
}

#[test]
fn an_overdue_incremental_sweep_finishes_at_the_next_safe_point() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_gc_threshold(256 * 1024);
    // Unrooted non-cons garbage, swept object by object (cons blocks with
    // no survivors are released at termination and leave no sweep work).
    let garbage = |ev: &mut Context| {
        for i in 0..200_000u64 {
            let _ = ev
                .tagged_heap
                .alloc_bignum(malachite::integer::Integer::from(
                    (1u128 << 100) + i as u128,
                ));
        }
    };
    // The first cycle is the stop-the-world bootstrap, which sweeps
    // synchronously; later cycles mark concurrently and sweep in slices.
    ev.gc_collect_from_current_roots_impl(true);
    garbage(&mut ev);
    let mut attempts = 0;
    loop {
        drive_until_sweeping(&mut ev);
        // Just terminated: the consing counter was reset, so one safe point
        // is one slice.
        if ev.tagged_heap.bytes_since_gc() <= ev.tagged_heap.gc_threshold() {
            break;
        }
        // The mark window itself consed past the threshold (a busy test
        // runner): finish this cycle and try again.
        ev.tagged_heap.finish_incremental_sweep_now();
        attempts += 1;
        assert!(attempts < 20, "could not observe a fresh sweep");
        garbage(&mut ev);
    }
    ev.gc_collect_from_current_roots();
    assert!(
        ev.tagged_heap.sweep_in_progress(),
        "not overdue: a safe point sweeps one slice"
    );
    // Cons past the threshold with no safe point in between.
    while ev.tagged_heap.bytes_since_gc() <= ev.tagged_heap.gc_threshold() {
        let _ = ev.tagged_heap.alloc_cons(Value::fixnum(1), Value::NIL);
    }
    ev.gc_collect_from_current_roots();
    assert!(
        !ev.tagged_heap.sweep_in_progress(),
        "overdue: the sweep finishes so the next cycle can start"
    );
}

/// A fresh deferred sweep that needs well over a hundred slices, entered
/// with no allocation since mark termination and no slice run yet.
fn sweep_from_zero_allocation() -> Context {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_gc_threshold(256 * 1024);
    // Enough unrooted non-cons garbage that the sweep needs well over a
    // hundred slices.
    let garbage = |ev: &mut Context| {
        for i in 0..1_000_000u64 {
            let _ = ev
                .tagged_heap
                .alloc_bignum(malachite::integer::Integer::from(
                    (1u128 << 100) + i as u128,
                ));
        }
    };
    ev.gc_collect_from_current_roots_impl(true);
    garbage(&mut ev);
    let mut attempts = 0;
    loop {
        drive_until_sweeping(&mut ev);
        if ev.tagged_heap.bytes_since_gc() == 0 {
            break;
        }
        // The mark window consed: the sweep would not start from zero
        // allocation. Finish this cycle and try again.
        ev.tagged_heap.finish_incremental_sweep_now();
        attempts += 1;
        assert!(attempts < 20, "could not observe a fresh sweep");
        garbage(&mut ev);
    }
    assert_eq!(ev.tagged_heap.sweep_stats().slice_count, 0);
    ev
}

#[test]
fn an_incremental_sweep_is_paced_by_allocation_not_by_safe_points() {
    let mut ev = sweep_from_zero_allocation();
    // Safe points with no allocation between them: the sweep's first slice
    // runs, and nothing more is owed until the mutator allocates.
    for _ in 0..10_000 {
        ev.gc_safe_point_exact();
    }
    assert!(
        ev.tagged_heap.sweep_stats().slice_count <= 1,
        "{} slices ran across 10000 safe points with no allocation",
        ev.tagged_heap.sweep_stats().slice_count
    );
    assert!(ev.tagged_heap.sweep_in_progress());
    // With allocation, slices follow it, and the sweep completes before the
    // next cycle is due (so the overdue drain never fires).
    let mut slices = 0;
    while ev.tagged_heap.sweep_in_progress() {
        for _ in 0..16 {
            let _ = ev.tagged_heap.alloc_cons(Value::fixnum(1), Value::NIL);
        }
        assert!(
            ev.tagged_heap.bytes_since_gc() <= ev.tagged_heap.gc_threshold(),
            "the sweep fell behind allocation after {slices} slices"
        );
        slices = ev.tagged_heap.sweep_stats().slice_count;
        ev.gc_safe_point_exact();
    }
    assert!(slices > 100, "the sweep needed only {slices} slices");
}

#[test]
fn raising_the_threshold_mid_sweep_does_not_postpone_the_sweep() {
    let mut ev = sweep_from_zero_allocation();
    // The live count the cycle started from; the sweep updates it only when
    // it completes.
    let live_before = ev.tagged_heap.live_bytes();
    ev.gc_safe_point_exact();
    assert!(ev.tagged_heap.sweep_in_progress());
    // `(setq gc-cons-threshold most-positive-fixnum)` while the cycle sweeps:
    // the overdue drain can no longer fire.
    ev.set_gc_threshold(i64::MAX as usize / 2);
    // GNU's default `gc-cons-threshold`, the horizon's floor.
    let bound = live_before.max(100_000 * size_of::<usize>());
    while ev.tagged_heap.sweep_in_progress() && ev.tagged_heap.bytes_since_gc() <= bound {
        for _ in 0..16 {
            let _ = ev.tagged_heap.alloc_cons(Value::fixnum(1), Value::NIL);
        }
        ev.gc_safe_point_exact();
    }
    assert!(
        !ev.tagged_heap.sweep_in_progress(),
        "{} bytes allocated after the threshold was raised, sweep unfinished after {} slices",
        ev.tagged_heap.bytes_since_gc(),
        ev.tagged_heap.sweep_stats().slice_count
    );
}
