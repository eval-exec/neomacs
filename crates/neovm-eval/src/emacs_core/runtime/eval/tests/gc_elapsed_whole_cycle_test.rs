//! `gc-elapsed` covers the whole collection. GNU `garbage_collect` adds its
//! entire duration (alloc.c, "Accumulate statistics"). A Neomacs cycle is
//! spread over several safe points (concurrent start, termination, sweep
//! slices), so every call of the cycle must count, not only the last one.

use crate::emacs_core::eval::Context;
use std::time::Duration;

fn gc_elapsed(ev: &Context) -> f64 {
    ev.builtin_var_value_by_name("gc-elapsed")
        .and_then(|value| value.as_number_f64())
        .unwrap_or(0.0)
}

fn gcs_done(ev: &Context) -> i64 {
    ev.builtin_var_value_by_name("gcs-done")
        .and_then(|value| value.as_fixnum())
        .unwrap_or(0)
}

fn unrooted_garbage(ev: &mut Context) {
    // Unrooted non-cons garbage is swept object by object, so the sweep
    // takes many bounded slices (cons blocks with no survivors are released
    // at termination and leave no sweep work).
    for i in 0..400_000u64 {
        let _ = ev
            .tagged_heap
            .alloc_bignum(malachite::integer::Integer::from(
                (1u128 << 100) + i as u128,
            ));
    }
}

/// Start a cycle from safe points and stop at the first sweep slice, with the
/// consing counter still under the threshold so each later safe point sweeps
/// exactly one slice.
fn start_sliced_cycle(ev: &mut Context) {
    ev.set_gc_threshold(256 * 1024);
    // The first cycle is the stop-the-world bootstrap; later cycles mark
    // concurrently and sweep in slices.
    ev.gc_collect_from_current_roots_impl(true);
    for _ in 0..20 {
        unrooted_garbage(ev);
        let mut sweeping = false;
        for _ in 0..20_000 {
            if ev.tagged_heap.sweep_in_progress() {
                sweeping = true;
                break;
            }
            ev.gc_collect_from_current_roots();
            if ev.tagged_heap.mark_in_progress() {
                std::thread::sleep(Duration::from_micros(200));
            }
        }
        assert!(sweeping, "no incremental sweep started");
        if ev.tagged_heap.bytes_since_gc() <= ev.tagged_heap.gc_threshold() {
            return;
        }
        // The mark window consed past the threshold (a busy test runner), so
        // the next safe point would finish the sweep at once. Complete this
        // cycle through the explicit path and try again.
        ev.gc_collect_from_current_roots_impl(true);
    }
    panic!("could not observe a fresh incremental sweep");
}

#[test]
fn gc_elapsed_covers_every_slice_of_an_incremental_cycle() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    start_sliced_cycle(&mut ev);
    assert!(
        ev.gc_cycle_elapsed > Duration::ZERO,
        "the start and termination calls of the cycle were not counted"
    );
    let done_before = gcs_done(&ev);
    let mut unfinished_slices = 0;
    while ev.tagged_heap.sweep_in_progress() {
        let banked = ev.gc_cycle_elapsed;
        ev.gc_collect_from_current_roots();
        if !ev.tagged_heap.sweep_in_progress() {
            break;
        }
        unfinished_slices += 1;
        assert!(
            ev.gc_cycle_elapsed > banked,
            "sweep slice {unfinished_slices} added no time to the cycle"
        );
        assert_eq!(
            gcs_done(&ev),
            done_before,
            "an unfinished slice completed a cycle"
        );
        assert!(unfinished_slices < 100_000, "the sweep never drained");
    }
    assert!(
        unfinished_slices > 3,
        "the cycle needs more than 3 sweep slices, took {unfinished_slices}"
    );
    assert_eq!(gcs_done(&ev), done_before + 1);
    assert_eq!(
        ev.gc_cycle_elapsed,
        Duration::ZERO,
        "a cycle completed by the collector entry leaves no banked time"
    );
}

#[test]
fn gc_elapsed_publishes_the_banked_time_when_the_cycle_completes() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    start_sliced_cycle(&mut ev);
    let elapsed_before = gc_elapsed(&ev);
    let mut banked = ev.gc_cycle_elapsed;
    while ev.tagged_heap.sweep_in_progress() {
        banked = ev.gc_cycle_elapsed;
        ev.gc_collect_from_current_roots();
    }
    // The completing call publishes the earlier calls' time plus its own.
    assert!(banked > Duration::ZERO);
    assert!(
        gc_elapsed(&ev) - elapsed_before >= banked.as_secs_f64() * (1.0 - 1e-9),
        "gc-elapsed grew {} s, the cycle's earlier calls took {} s",
        gc_elapsed(&ev) - elapsed_before,
        banked.as_secs_f64()
    );
}

#[test]
fn gc_elapsed_counts_an_in_flight_cycle_drained_by_garbage_collect() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    start_sliced_cycle(&mut ev);
    let banked = ev.gc_cycle_elapsed;
    assert!(banked > Duration::ZERO);
    let done_before = gcs_done(&ev);
    let elapsed_before = gc_elapsed(&ev);
    // Explicit `garbage-collect` drains the cycle in flight, then collects.
    ev.gc_collect_from_current_roots_impl(true);
    assert_eq!(gcs_done(&ev), done_before + 1);
    assert_eq!(ev.gc_cycle_elapsed, Duration::ZERO);
    assert!(
        gc_elapsed(&ev) - elapsed_before >= banked.as_secs_f64() * (1.0 - 1e-9),
        "the drained cycle's earlier calls are missing from gc-elapsed"
    );
}
