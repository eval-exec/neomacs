//! An incremental sweep takes one slice per safe point until allocation
//! outruns it: once a full threshold's worth has been consed since the mark
//! terminated, the next cycle is due, and the sweep must finish at that safe
//! point instead of letting the garbage pile into the next cycle.

use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

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
    let mut ev = Context::new();
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
