//! P4 preconditions M1, M3, M4 and batch-attach generation invariants on
//! `OverlayIndex`.

use super::*;
use crate::buffer::BufferId;
use neovm_value::heap_types::{OverlayDataInit, OverlaySerialSource};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Test-only scoped handle to probe a `parking_lot::RwLock` from a helper
/// thread when the inner tree contains `!Send` `Value` handles.
///
/// Callers in this test module only touch the lock state and plain integer
/// metadata (`generation`, `EmacsBytePos` keys) and never dereference a Lisp
/// `Value` or thread-local heap state off the owning thread.
struct LockProbe<'a, T>(&'a RwLock<T>);

// SAFETY: `parking_lot::RwLock`'s synchronization state is thread-safe, and
// test helper closures only inspect lock availability or integer metadata
// within `std::thread::scope` while the owner stays alive.
unsafe impl<T> Send for LockProbe<'_, T> {}

fn overlay(start: usize, end: usize) -> Value {
    Value::make_overlay(OverlayDataInit {
        serial: OverlaySerialSource::Mint,
        plist: Value::NIL,
        buffer: Some(BufferId(1)),
        start,
        end,
        front_advance: false,
        rear_advance: false,
    })
}

fn range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::from_usize(start, end)
}

#[test]
fn text_edit_does_not_publish_new_generation_before_tree_edit_finishes() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let ov = overlay(10, 20);
    assert!(index.attach(ov, range(10, 20)));
    // Publish endpoints so a reader can inspect them alongside generation.
    index.publish_endpoints();
    let before = index.generation();

    let intervals = Arc::clone(&index.intervals);
    let core = Arc::clone(&index.core);
    let intervals_probe = LockProbe(intervals.as_ref());
    let core_probe = LockProbe(core.as_ref());
    let holding = AtomicBool::new(false);

    let torn = std::thread::scope(|scope| {
        let observer = scope.spawn(|| {
            let intervals_probe = intervals_probe;
            let core_probe = core_probe;
            let guard = intervals_probe.0.read();
            holding.store(true, Ordering::Release);
            let deadline = Instant::now() + Duration::from_millis(50);
            let mut torn = None;
            while Instant::now() < deadline {
                let core_guard = core_probe.0.read();
                let observed_gen = core_guard.generation;
                let first_boundary = core_guard.endpoints.get().and_then(|endpoints| {
                    endpoints.next_after(EmacsBytePos::new(0), EmacsBytePos::new(100))
                });
                drop(core_guard);
                if observed_gen != before && first_boundary == Some(EmacsBytePos::new(10)) {
                    torn = Some((observed_gen, first_boundary));
                    break;
                }
                std::thread::yield_now();
            }
            drop(guard);
            torn
        });

        while !holding.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        index.adjust_for_text_edit(OverlayTextEdit::Insert {
            position: EmacsBytePos::new(5),
            length: EmacsByteLen::new(5),
            before_markers: false,
        });
        observer.join().expect("observer thread finished")
    });

    assert!(
        torn.is_none(),
        "M1: text edit published a new generation before updating endpoints: {torn:?}"
    );
    assert_eq!(index.generation(), before + 1);
    assert_eq!(
        index.next_boundary_after_no_alloc(EmacsBytePos::new(0), EmacsBytePos::new(100)),
        Some(EmacsBytePos::new(15))
    );
}

#[test]
fn property_mask_writers_advance_generation() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let ov = overlay(10, 20);
    assert!(index.attach(ov, range(10, 20)));
    index.publish_endpoints();

    let face = Value::symbol("face");
    let before_add = index.generation();
    index.overlay_property_key_added_to_member(ov, face);
    let after_add = index.generation();
    assert!(
        after_add > before_add,
        "M3: overlay_property_key_added_to_member must advance generation: before={before_add} after={after_add}"
    );

    let previous = index.property_keys_before_write(ov);
    let _ = ov.with_overlay_data_mut(|data| {
        data.plist = Value::list(vec![face, Value::symbol("bold")]);
    });
    let before_refresh = index.generation();
    index.overlay_properties_changed(ov, previous);
    let after_refresh = index.generation();
    assert!(
        after_refresh > before_refresh,
        "M3: overlay_properties_changed must advance generation: before={before_refresh} after={after_refresh}"
    );
}

fn assert_cold_query_drops_core_before_intervals<R: Eq + std::fmt::Debug>(
    label: &str,
    query: impl FnOnce(&OverlayIndex) -> R,
    expected: R,
    expect_published_after: bool,
) {
    let mut index = OverlayIndex::new();
    assert!(index.attach(overlay(10, 20), range(10, 20)));
    assert!(
        !index.is_endpoints_published(),
        "test requires a cold endpoint index"
    );

    // Deterministically probe `core.try_write()` at the exact cold-path
    // `intervals.read()` acquisition on this private single-threaded index.
    // Because no other thread touches `index`, `try_write()` fails if and only
    // if `query` still holds `core.read()` when acquiring `intervals.read()`.
    index.cold_intervals_probe.arm();
    let result = query(&index);
    let (intervals_reads, saw_core_held) = index.cold_intervals_probe.snapshot();
    index.cold_intervals_probe.disarm();

    assert_eq!(result, expected, "{label}: unexpected query result");
    assert_eq!(
        intervals_reads, 1,
        "M4: {label} must execute its cold intervals.read() path exactly once"
    );
    assert!(
        !saw_core_held,
        "M4: {label} held core.read() while acquiring intervals.read() (lock-order inversion)"
    );
    assert_eq!(
        index.is_endpoints_published(),
        expect_published_after,
        "{label}: unexpected endpoint publication state"
    );
}

#[test]
fn cold_boundary_queries_drop_core_lock_before_reading_intervals() {
    crate::test_utils::init_test_tracing();
    assert_cold_query_drops_core_before_intervals(
        "next_boundary_after_no_alloc",
        |index| index.next_boundary_after_no_alloc(EmacsBytePos::new(0), EmacsBytePos::new(100)),
        Some(EmacsBytePos::new(10)),
        false,
    );
    assert_cold_query_drops_core_before_intervals(
        "previous_boundary_before",
        |index| index.previous_boundary_before(EmacsBytePos::new(100), EmacsBytePos::new(0)),
        Some(EmacsBytePos::new(20)),
        false,
    );
    assert_cold_query_drops_core_before_intervals(
        "LiveOverlayIndex::next_boundary_after",
        |index| {
            let live = index.live_handle();
            let stale = Cell::new(false);
            let next =
                live.next_boundary_after(EmacsBytePos::new(0), EmacsBytePos::new(100), &stale);
            assert!(!stale.get(), "live handle must stay fresh");
            next
        },
        Some(EmacsBytePos::new(10)),
        false,
    );
    assert_cold_query_drops_core_before_intervals(
        "publish_endpoints",
        |index| index.publish_endpoints(),
        (),
        true,
    );
    assert_cold_query_drops_core_before_intervals(
        "LiveOverlayIndex::endpoint_records_strictly_within",
        |index| {
            let live = index.live_handle();
            let stale = Cell::new(false);
            let count = live
                .endpoint_records_strictly_within(
                    range(0, 100),
                    OverlayPropertyFilter::unfiltered(),
                    &stale,
                )
                .count();
            assert!(!stale.get(), "live handle must stay fresh");
            count
        },
        2,
        true,
    );
}

#[test]
fn attach_batch_advances_generation_and_rejects_nonempty_without_bump() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let initial = index.generation();
    let first = overlay(10, 20);
    let second = overlay(30, 40);
    assert!(index.attach_batch(
        &[(first, range(10, 20)), (second, range(30, 40))],
        OverlayBatchOrder::AscendingQueryOrder,
    ));
    let after_batch = index.generation();
    assert!(
        after_batch > initial,
        "attach_batch must advance generation: before={initial} after={after_batch}"
    );

    let third = overlay(50, 60);
    assert!(!index.attach_batch(
        &[(third, range(50, 60))],
        OverlayBatchOrder::AscendingQueryOrder,
    ));
    assert_eq!(
        index.generation(),
        after_batch,
        "rejected attach_batch must not advance generation"
    );
}
