//! The narrowed cons gate must preserve writes without an installed heap.

use super::barrier_window::published_cons_barrier_window;
use super::*;

#[test]
fn symbol_cons_stores_do_not_install_a_heap() {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = false;
    set_tagged_heap(&mut heap);
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    clear_tagged_heap_if_installed(&heap);
    assert!(!tagged_heap_is_installed());
    let before = RECORD_HEAP_WRITE_CALLS.with(Cell::get);

    // NIL and T are symbols, so neither may use the fixnum rejection.
    // The owner remains live, but its allocation heap is deliberately absent
    // from TLS. A barrier probe must not allocate a fallback or change identity.
    owner.set_car(TaggedValue::T);
    owner.set_cdr(TaggedValue::NIL);
    note_heap_write(owner, HeapWriteKind::ConsCar);

    assert_eq!(owner.cons_car(), TaggedValue::T);
    assert_eq!(owner.cons_cdr(), TaggedValue::NIL);
    assert_eq!(RECORD_HEAP_WRITE_CALLS.with(Cell::get), before);
    assert!(!tagged_heap_is_installed());
    assert_eq!(current_tagged_heap_identity(), None);
}

fn assert_published_windows(heap: &TaggedHeap, ordinary: BarrierWindow) {
    assert_eq!(published_barrier_window(), ordinary);
    assert_eq!(heap.jit_barrier_window_for_test(), ordinary);
    assert_eq!(
        published_cons_barrier_window(),
        if heap.generational.enabled {
            BarrierWindow::ALL
        } else {
            ordinary
        }
    );
}

#[test]
fn generational_cons_protocol_window_preserves_disabled_window_transitions() {
    for enabled in [false, true] {
        let mut heap = Box::new(TaggedHeap::new());
        heap.generational.enabled = enabled;
        heap.publish_barrier_window();
        set_tagged_heap(&mut heap);
        assert_published_windows(&heap, BarrierWindow::NONE);
        heap.extend_dump_span(4096, 4096);
        let span = BarrierWindow::span(4096, 8192);
        assert_published_windows(&heap, span);
        heap.set_concurrent_active_for_test(true);
        assert_published_windows(&heap, BarrierWindow::ALL);
        heap.set_concurrent_active_for_test(false);
        assert_published_windows(&heap, span);
        heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
        assert_published_windows(&heap, BarrierWindow::ALL);
        heap.set_write_tracking_mode(WriteTrackingMode::Disabled);
        assert_published_windows(&heap, span);
    }
}

#[test]
fn generational_cons_protocol_window_rederives_on_heap_switch_and_uninstall() {
    let mut enabled = Box::new(TaggedHeap::new());
    enabled.generational.enabled = true;
    enabled.publish_barrier_window();
    set_tagged_heap(&mut enabled);
    assert_published_windows(&enabled, BarrierWindow::NONE);
    let mut disabled = Box::new(TaggedHeap::new());
    disabled.generational.enabled = false;
    disabled.extend_dump_span(4096, 4096);
    set_tagged_heap(&mut disabled);
    assert_published_windows(&disabled, BarrierWindow::span(4096, 8192));
    // A displaced heap's teardown must not disarm the current heap.
    clear_tagged_heap_if_installed(&enabled);
    assert_published_windows(&disabled, BarrierWindow::span(4096, 8192));
    TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.set(BarrierWindow::NONE));
    set_tagged_heap(&mut enabled);
    assert_published_windows(&enabled, BarrierWindow::NONE);
    drop(disabled);
    assert_published_windows(&enabled, BarrierWindow::NONE);
    drop(enabled);
    assert_eq!(published_barrier_window(), BarrierWindow::NONE);
    assert_eq!(published_cons_barrier_window(), BarrierWindow::NONE);
    assert!(!tagged_heap_is_installed());
}

#[test]
fn generational_cons_protocol_window_fallback_install_derives_both_mirrors() {
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    clear_tagged_heap_if_installed(&heap);
    // Do not rely on a fallback allocation's first collection to publish it.
    TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.set(BarrierWindow::span(8, 16)));
    with_tagged_heap(|fallback| {
        assert_published_windows(fallback, fallback.barrier_window());
        assert_eq!(current_tagged_heap_identity(), Some(fallback.identity()));
        clear_tagged_heap_if_installed(fallback);
    });
    assert_eq!(published_cons_barrier_window(), BarrierWindow::NONE);
}

#[test]
fn generational_cons_protocol_window_logs_symbols_once_and_keeps_preimages() {
    let mut heap = Box::new(TaggedHeap::new());
    heap.generational.enabled = true;
    heap.publish_barrier_window();
    set_tagged_heap(&mut heap);
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let (trailer, index) = heap.old_cons_trailer(owner).unwrap();
    trailer.set_old(index);
    trailer.set_unlogged(index);
    owner.set_car(TaggedValue::fixnum(1));
    assert!(heap.current_mutator_gc().remset.is_empty());
    owner.set_car(TaggedValue::T);
    owner.set_cdr(TaggedValue::NIL);
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
    // Concurrent marking must still record an immediate overwrite's old
    // child, even though generation-only stores reject that new value.
    owner.set_car(owner);
    heap.set_concurrent_active_for_test(true);
    owner.set_car(TaggedValue::fixnum(2));
    assert!(heap.take_satb_shared_for_test().contains(&owner));
    heap.set_concurrent_active_for_test(false);
}

#[test]
fn generational_cold_cons_reject_skips_full_hook_for_immediates_and_ineligible_owners() {
    let mut heap = Box::new(TaggedHeap::new());
    heap.generational.enabled = true;
    heap.publish_barrier_window();
    set_tagged_heap(&mut heap);
    let young = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let old = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let (trailer, index) = heap.old_cons_trailer(old).unwrap();
    trailer.set_old(index);
    trailer.set_unlogged(index);
    let before = BARRIER_SLOW_CALLS.with(Cell::get);
    old.set_car(TaggedValue::fixnum(7));
    old.set_cdr(TaggedValue::char('x'));
    young.set_car(TaggedValue::T);
    young.set_cdr(old);
    assert_eq!(BARRIER_SLOW_CALLS.with(Cell::get), before);
    assert!(heap.current_mutator_gc().remset.is_empty());

    old.set_car(TaggedValue::T);
    assert_eq!(BARRIER_SLOW_CALLS.with(Cell::get), before + 1);
    old.set_cdr(TaggedValue::NIL);
    old.set_car(young);
    assert_eq!(BARRIER_SLOW_CALLS.with(Cell::get), before + 1);
    assert_eq!(heap.current_mutator_gc().remset, [old]);
}

#[test]
fn generational_cold_cons_reject_preserves_real_window_immediate_writes() {
    use super::fake_image::FakeImage;

    let mut heap = Box::new(TaggedHeap::new());
    heap.generational.enabled = true;
    heap.publish_barrier_window();
    set_tagged_heap(&mut heap);
    let young = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
    let before = BARRIER_SLOW_CALLS.with(Cell::get);
    young.set_car(TaggedValue::fixnum(9));
    assert_eq!(BARRIER_SLOW_CALLS.with(Cell::get), before + 1);
    assert!(heap.is_dirty_owner(young));
    heap.set_write_tracking_mode(WriteTrackingMode::Disabled);

    let image = FakeImage::leak(false);
    let mapped = image.register_cons(&mut heap);
    let before = BARRIER_SLOW_CALLS.with(Cell::get);
    mapped.set_car(TaggedValue::fixnum(10));
    assert_eq!(BARRIER_SLOW_CALLS.with(Cell::get), before + 1);
    mapped.set_car(young);
    heap.set_concurrent_active_for_test(true);
    mapped.set_car(TaggedValue::fixnum(11));
    assert!(heap.take_satb_shared_for_test().contains(&young));
    heap.set_concurrent_active_for_test(false);
}
