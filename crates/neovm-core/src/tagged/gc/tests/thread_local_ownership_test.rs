use super::*;

fn assert_empty_barrier_caches(heap: Option<&TaggedHeap>) {
    if let Some(heap) = heap {
        assert!(
            heap.current_mutator_gc()
                .remembered_cache
                .iter()
                .all(|&bits| bits == 0)
        );
    }
    TAGGED_HEAP_SATB_CACHE.with(|slots| assert!(slots.iter().all(|slot| slot.get() == 0)));
}

#[test]
fn gc_tls_ownership_activation_clears_weak_barrier_owner_words() {
    let mut first = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut first);
    let owner = first.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let slot = barrier_cache_slot(owner.bits());
    first.current_mutator_gc_mut().remembered_cache[slot] = owner.bits();
    TAGGED_HEAP_SATB_CACHE.with(|slots| slots[slot].set(owner.bits()));

    let mut second = Box::new(TaggedHeap::new());
    second.current_mutator_gc_mut().remembered_cache[slot] = owner.bits();
    set_tagged_heap(&mut second);
    assert_empty_barrier_caches(Some(&second));
    assert_eq!(current_tagged_heap_identity(), Some(second.identity()));
    drop(first);
    assert_eq!(current_tagged_heap_identity(), Some(second.identity()));
    drop(second);
    assert!(current_tagged_heap_identity().is_none());
    assert_empty_barrier_caches(None);
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
    TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.with(|active| active.set(true));
    TAGGED_HEAP_DUMP_SPAN.with(|span| span.set((4096, 8192)));
    let mut second = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut second);
    TAGGED_HEAP_PARTITION_ACTIVE.with(|active| assert!(!active.get()));
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|active| assert!(!active.get()));
    assert!(!concurrent_hash_mutation_active());
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

fn heap_with_hash_claims(claims: bool) -> Box<TaggedHeap> {
    knobs::set_concurrent_claims_for_test(Some(claims));
    let heap = Box::new(TaggedHeap::new());
    knobs::set_concurrent_claims_for_test(None);
    heap
}

#[test]
fn gc_tls_hash_activation_follows_real_launch_join_and_termination() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};
    for generational in [false, true] {
        for claims in [false, true] {
            let mut heap = heap_with_hash_claims(claims);
            heap.generational.enabled = generational;
            set_tagged_heap(&mut heap);
            let owner = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
            assert!(!concurrent_hash_mutation_active());
            heap.concurrent_begin();
            heap.seed_root(owner);
            assert!(
                !concurrent_hash_mutation_active(),
                "root seeding is not reader launch"
            );
            heap.launch_concurrent_mark();
            assert!(concurrent_mark_active());
            assert_eq!(concurrent_hash_mutation_active(), claims);
            assert_eq!(heap.concurrent_hash_snapshot().is_some(), claims);
            heap.join_concurrent_mark();
            assert!(!concurrent_hash_mutation_active());
            assert_eq!(
                heap.concurrent_hash_snapshot().is_some(),
                claims,
                "joined snapshot storage is retained until termination"
            );
            set_tagged_heap(&mut heap);
            assert!(
                !concurrent_hash_mutation_active(),
                "retained Arc cannot reactivate Tier-H"
            );
            heap.reseed_runtime_and_remembered_roots();
            heap.seed_root(owner);
            heap.incremental_drain_all();
            heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
            heap.finish_incremental_sweep_now();
            assert!(heap.concurrent_hash_snapshot().is_none());
            assert!(!concurrent_hash_mutation_active());
            drop(heap);
            assert!(!concurrent_hash_mutation_active());
        }
    }
}

#[test]
fn gc_tls_hash_activation_survives_displaced_heap_join_and_reinstallation() {
    let mut first = heap_with_hash_claims(true);
    set_tagged_heap(&mut first);
    first.concurrent_begin();
    first.launch_concurrent_mark();
    assert!(concurrent_hash_mutation_active());

    let mut second = heap_with_hash_claims(true);
    set_tagged_heap(&mut second);
    assert!(!concurrent_hash_mutation_active());
    second.concurrent_begin();
    second.launch_concurrent_mark();
    assert!(concurrent_hash_mutation_active());
    clear_tagged_heap_if_installed(&first);
    assert!(concurrent_hash_mutation_active());

    // The first heap's teardown calls join even though another heap owns TLS.
    // It must not clear the installed heap's dedicated Tier-H activation.
    drop(first);
    assert!(concurrent_hash_mutation_active());
    assert_eq!(current_tagged_heap_identity(), Some(second.identity()));
    TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.with(|active| active.set(false));
    set_tagged_heap(&mut second);
    assert!(
        concurrent_hash_mutation_active(),
        "installation repairs stale local protocol mirrors"
    );
    second.join_concurrent_mark();
    assert!(second.concurrent_hash_snapshot().is_some());
    assert!(!concurrent_hash_mutation_active());
    set_tagged_heap(&mut second);
    assert!(!concurrent_hash_mutation_active());
    second.reseed_runtime_and_remembered_roots();
    second.incremental_drain_all();
    second.incremental_finish(second.live_bytes(), std::time::Instant::now());
    second.finish_incremental_sweep_now();
    drop(second);
    assert!(!concurrent_hash_mutation_active());
}

#[test]
fn gc_tls_hash_activation_is_local_to_each_installed_mutator() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    let (ready, started) = mpsc::channel();
    let (resume, released) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut heap = heap_with_hash_claims(true);
        heap.set_concurrent_hash_snapshot(Some(Arc::new(
            concurrent_hash::HashTableScanSnapshot::new(),
        )));
        set_tagged_heap(&mut heap);
        heap.set_concurrent_active_for_test(true);
        assert!(concurrent_hash_mutation_active());
        ready.send(()).expect("hash activation parent dropped");
        released
            .recv_timeout(Duration::from_secs(10))
            .expect("hash activation parent did not release worker");
        assert!(concurrent_hash_mutation_active());
        heap.set_concurrent_active_for_test(false);
        drop(heap);
        assert!(!concurrent_hash_mutation_active());
    });
    let mut heap = heap_with_hash_claims(false);
    set_tagged_heap(&mut heap);
    heap.set_concurrent_active_for_test(true);
    started
        .recv_timeout(Duration::from_secs(10))
        .expect("hash activation worker did not start");
    assert!(concurrent_mark_active());
    assert!(
        !concurrent_hash_mutation_active(),
        "claims-OFF rejects during a concurrent cycle"
    );
    resume
        .send(())
        .expect("hash activation worker exited early");
    worker
        .join()
        .expect("mutator-local hash activation worker failed");
    assert!(!concurrent_hash_mutation_active());
    heap.set_concurrent_active_for_test(false);
    drop(heap);
}
