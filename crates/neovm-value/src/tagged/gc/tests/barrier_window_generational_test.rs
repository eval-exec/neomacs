//! The narrowed cons gate must preserve writes without an installed heap.

use super::barrier_window::published_cons_barrier_window;
use super::*;

fn promote_cons_until_old(heap: &mut TaggedHeap, owner: TaggedValue) {
    // Real promotion maintains canonical young-block logs and old counts.
    // Observe the selected age policy rather than assuming a single minor.
    for _ in 0..3 {
        if heap.cons_age_is_old_for_test(owner) {
            break;
        }
        heap.begin_minor_collection_for_test().unwrap();
        heap.seed_root(owner);
        heap.complete_minor_collection_for_test();
        heap.finish_incremental_sweep_now_for_test();
    }
    assert!(heap.cons_age_is_old_for_test(owner));
    assert_eq!(heap.cons_region_for_test(), (0, 0));
}

#[test]
fn symbol_cons_stores_do_not_install_a_heap() {
    let mut heap = TaggedHeap::new();
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { heap.set_generational_enabled(false) };
    set_tagged_heap(&mut heap);
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    // SAFETY: bare `TaggedHeap` unit test checking null-heap write recording.
    unsafe { clear_tagged_heap_if_installed(&heap) };
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
        // SAFETY: serial fixture heap; the caller holds the only heap view and no
        // collector or compiled leaf runs.
        unsafe { heap.set_generational_enabled(enabled) };
        // SAFETY: serial fixture; the heap is exclusively borrowed by the test
        // thread and no other mutator or compiled leaf runs across the publication.
        unsafe { heap.publish_barrier_window() };
        set_tagged_heap(&mut heap);
        assert_published_windows(&heap, BarrierWindow::NONE);
        heap.extend_dump_span(4096, 4096);
        let span = BarrierWindow::span(4096, 8192);
        assert_published_windows(&heap, span);
        // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
        unsafe { heap.set_concurrent_active_for_test(true) };
        assert_published_windows(&heap, BarrierWindow::ALL);
        // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
        unsafe { heap.set_concurrent_active_for_test(false) };
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
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { enabled.set_generational_enabled(true) };
    // SAFETY: serial fixture; the heap is exclusively borrowed by the test
    // thread and no other mutator or compiled leaf runs across the publication.
    unsafe { enabled.publish_barrier_window() };
    set_tagged_heap(&mut enabled);
    assert_published_windows(&enabled, BarrierWindow::NONE);
    let mut disabled = Box::new(TaggedHeap::new());
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { disabled.set_generational_enabled(false) };
    disabled.extend_dump_span(4096, 4096);
    // SAFETY: bare `TaggedHeap` unit test switching from `enabled` to `disabled`.
    unsafe { clear_tagged_heap_if_installed(&enabled) };
    set_tagged_heap(&mut disabled);
    assert_published_windows(&disabled, BarrierWindow::span(4096, 8192));
    // A displaced heap's teardown must not disarm the current heap.
    // SAFETY: bare `TaggedHeap` unit test verifying non-current clear is a no-op.
    unsafe { clear_tagged_heap_if_installed(&enabled) };
    assert_published_windows(&disabled, BarrierWindow::span(4096, 8192));
    // SAFETY: bare `TaggedHeap` unit test clearing `disabled` before restoring
    // `enabled`; seed a stale cons-barrier mirror so `set_tagged_heap` must
    // still re-derive `enabled`'s window.
    unsafe { clear_tagged_heap_if_installed(&disabled) };
    TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.set(BarrierWindow::span(8, 16)));
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
    // SAFETY: bare `TaggedHeap` unit test checking null-heap write recording.
    unsafe { clear_tagged_heap_if_installed(&heap) };
    // Do not rely on a fallback allocation's first collection to publish it.
    TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.set(BarrierWindow::span(8, 16)));
    // SAFETY: unit test checking fallback heap install when TLS is null; the
    // returned `fallback_root` is only used after `with_tagged_heap` returns so
    // no TLS-derived `&mut TaggedHeap` loan is live during the clear below.
    let fallback_root = unsafe {
        with_tagged_heap(|fallback| {
            assert_published_windows(fallback, fallback.barrier_window());
            assert_eq!(
                current_tagged_heap_identity(),
                Some(fallback.tagged_heap_identity())
            );
            std::ptr::NonNull::from(&*fallback)
        })
    };
    // `with_tagged_heap` has returned, so no TLS-derived `&mut TaggedHeap` loan
    // is live when we detach and clear the thread-local fallback heap.
    // SAFETY: `fallback_root` points to the process-lifetime test fallback heap
    // currently installed on this thread; no active `Context` or TLS heap loan
    // is live.
    unsafe { clear_tagged_heap_if_installed(fallback_root.as_ref()) };
    assert_eq!(published_cons_barrier_window(), BarrierWindow::NONE);
}

#[test]
fn generational_cons_protocol_window_marks_symbols_and_keeps_preimages() {
    let mut heap = Box::new(TaggedHeap::new());
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { heap.set_generational_enabled(true) };
    // SAFETY: serial fixture; the heap is exclusively borrowed by the test
    // thread and no other mutator or compiled leaf runs across the publication.
    unsafe { heap.publish_barrier_window() };
    set_tagged_heap(&mut heap);
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    promote_cons_until_old(&mut heap, owner);
    owner.set_car(TaggedValue::fixnum(1));
    assert!(!heap.owner_card_is_dirty_for_test(owner));
    owner.set_car(TaggedValue::T);
    owner.set_cdr(TaggedValue::NIL);
    // A symbol's liveness rides on the card: S3 cons stores mark, never log.
    assert!(heap.owner_card_is_dirty_for_test(owner));
    assert!(heap.current_mutator_gc().remset.is_empty());
    // Concurrent marking must still record an immediate overwrite's old
    // child, even though generation-only stores reject that new value.
    owner.set_car(owner);
    // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
    unsafe { heap.set_concurrent_active_for_test(true) };
    owner.set_car(TaggedValue::fixnum(2));
    // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
    assert!(unsafe { heap.take_satb_shared_for_test() }.contains(&owner));
    // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
    unsafe { heap.set_concurrent_active_for_test(false) };
}

#[test]
fn generational_cold_cons_reject_skips_full_hook_for_immediates_and_ineligible_owners() {
    let mut heap = Box::new(TaggedHeap::new());
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { heap.set_generational_enabled(true) };
    // SAFETY: serial fixture; the heap is exclusively borrowed by the test
    // thread and no other mutator or compiled leaf runs across the publication.
    unsafe { heap.publish_barrier_window() };
    set_tagged_heap(&mut heap);
    let old = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    promote_cons_until_old(&mut heap, old);
    let before = BARRIER_SLOW_CALLS.with(Cell::get);
    old.set_car(TaggedValue::fixnum(7));
    old.set_cdr(TaggedValue::char('x'));
    assert!(
        !heap.owner_card_is_dirty_for_test(old),
        "immediates mark nothing"
    );

    // Outside the window a heap value only marks its owner's card: the
    // full hook, its record and the remembered set are all skipped.
    let young = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    old.set_car(TaggedValue::T);
    old.set_cdr(TaggedValue::NIL);
    old.set_car(young);
    young.set_cdr(old);
    assert_eq!(BARRIER_SLOW_CALLS.with(Cell::get), before);
    assert!(heap.owner_card_is_dirty_for_test(old));
    assert!(heap.current_mutator_gc().remset.is_empty());
}

#[test]
fn generational_cold_cons_reject_preserves_real_window_immediate_writes() {
    use super::fake_image::FakeImage;

    let mut heap = Box::new(TaggedHeap::new());
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { heap.set_generational_enabled(true) };
    // SAFETY: serial fixture; the heap is exclusively borrowed by the test
    // thread and no other mutator or compiled leaf runs across the publication.
    unsafe { heap.publish_barrier_window() };
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
    // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
    unsafe { heap.set_concurrent_active_for_test(true) };
    mapped.set_car(TaggedValue::fixnum(11));
    // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
    assert!(unsafe { heap.take_satb_shared_for_test() }.contains(&young));
    // SAFETY: serial test heap; no concurrent marker has been launched and none can run during the call.
    unsafe { heap.set_concurrent_active_for_test(false) };
}
