use super::*;

fn assert_empty_barrier_caches() {
    for cache in [&TAGGED_HEAP_REMEMBERED_CACHE, &TAGGED_HEAP_SATB_CACHE] {
        cache.with(|slots| assert!(slots.iter().all(|slot| slot.get() == 0)));
    }
}

#[test]
fn gc_tls_ownership_activation_clears_weak_barrier_owner_words() {
    let mut first = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut first);
    let owner = first.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let slot = barrier_cache_slot(owner.bits());
    TAGGED_HEAP_REMEMBERED_CACHE.with(|slots| slots[slot].set(owner.bits()));
    TAGGED_HEAP_SATB_CACHE.with(|slots| slots[slot].set(owner.bits()));

    let mut second = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut second);
    assert_empty_barrier_caches();
    assert_eq!(current_tagged_heap_identity(), Some(second.identity()));
    drop(first);
    assert_eq!(current_tagged_heap_identity(), Some(second.identity()));
    drop(second);
    assert!(current_tagged_heap_identity().is_none());
    assert_empty_barrier_caches();
    TAGGED_HEAP_DUMP_SPAN.with(|span| assert_eq!(span.get(), (usize::MAX, 0)));
    TAGGED_HEAP_BARRIER_WINDOW.with(|window| assert_eq!(window.get(), BarrierWindow::NONE));
}

#[test]
fn gc_tls_ownership_heap_activation_rederives_barrier_mirrors() {
    let mut first = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut first);
    // Simulate previously published mirrors without launching a background
    // collector. Activation must derive them from the new heap's state.
    TAGGED_HEAP_PARTITION_ACTIVE.with(|active| active.set(true));
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|active| active.set(true));
    TAGGED_HEAP_DUMP_SPAN.with(|span| span.set((4096, 8192)));
    let mut second = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut second);
    TAGGED_HEAP_PARTITION_ACTIVE.with(|active| assert!(!active.get()));
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|active| assert!(!active.get()));
    TAGGED_HEAP_DUMP_SPAN.with(|span| assert_eq!(span.get(), (usize::MAX, 0)));
    TAGGED_HEAP_WRITE_TRACKING_MODE
        .with(|mode| assert_eq!(mode.get(), second.write_tracking_mode()));
    TAGGED_HEAP_BARRIER_WINDOW.with(|window| assert_eq!(window.get(), second.barrier_window()));
}

#[test]
fn gc_tls_ownership_allocation_view_metadata_rejects_pointer_reuse() {
    let mut first = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut first);
    let original_identity = first.identity();
    let mut second = Box::new(TaggedHeap::new());
    // Model an inactive source-thread pointer whose storage was reused by a
    // moved Context. Pointer equality alone must not accept its old mirrors.
    TAGGED_HEAP.with(|pointer| pointer.set(&mut *second));
    assert!(!tagged_heap_is_current(&second));
    assert_eq!(current_tagged_heap_identity(), Some(original_identity));
    // Ownership queries must not dereference an inactive raw pointer either.
    TAGGED_HEAP.with(|pointer| pointer.set(std::ptr::dangling_mut()));
    assert_eq!(current_tagged_heap_identity(), Some(original_identity));
    assert!(!tagged_heap_is_current(&second));
    set_tagged_heap(&mut second);
    assert!(tagged_heap_is_current(&second));
    drop(first);
    assert_eq!(current_tagged_heap_identity(), Some(second.identity()));
    drop(second);
    assert!(current_tagged_heap_identity().is_none());
}
