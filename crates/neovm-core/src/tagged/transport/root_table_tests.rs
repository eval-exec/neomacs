//! Counted table lifetimes, collector ownership, and concurrent retirement.

use super::*;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::transport::SharedRoot;
use std::cell::Cell;
use std::sync::Barrier;

#[test]
fn empty_count_skips_lazy_registry_initialization_and_locking() {
    let count = LiveTableCount::new();
    let initialized = Cell::new(0);
    let locked = Cell::new(0);
    let registry = LazyLock::new(|| {
        initialized.set(initialized.get() + 1);
        Mutex::new(())
    });
    let scan = || {
        let _lock = registry.lock().unwrap();
        locked.set(locked.get() + 1);
    };
    count.with_registered_tables(scan);
    assert_eq!(initialized.get(), 0);
    assert_eq!(locked.get(), 0);

    let registration = count.register();
    count.with_registered_tables(scan);
    assert_eq!(initialized.get(), 1);
    assert_eq!(locked.get(), 1);
    drop(registration);
    count.with_registered_tables(scan);
    assert_eq!(initialized.get(), 1);
    assert_eq!(locked.get(), 1);
}

#[test]
fn registrations_count_once_and_retire_on_the_receiving_thread() {
    let count = LiveTableCount::new();
    assert!(count.is_empty());
    let first = count.register();
    assert_eq!(count.0.load(Ordering::Acquire), 1);
    let second = count.register();
    assert_eq!(count.0.load(Ordering::Acquire), 2);
    drop(first);
    assert_eq!(count.0.load(Ordering::Acquire), 1);
    std::thread::scope(|scope| {
        scope.spawn(move || drop(second)).join().unwrap();
    });
    assert!(count.is_empty());
}

#[test]
fn a_collector_arc_keeps_the_table_registered_until_storage_is_released() {
    // This counter belongs only to this fixture, so assertions never depend
    // on whether an unrelated heap has live SharedRoots in another test.
    static COUNT: LiveTableCount = LiveTableCount::new();
    let heap = TaggedHeap::new();
    let table = Arc::new(RootTable::new(heap.heap_identity(), COUNT.register()));
    let collector = Arc::clone(&table);
    assert_eq!(COUNT.0.load(Ordering::Acquire), 1);
    drop(table);
    assert!(!COUNT.is_empty());
    std::thread::spawn(move || drop(collector)).join().unwrap();
    assert!(COUNT.is_empty());
}

#[test]
fn retirement_during_capture_and_readmission_preserve_the_new_root() {
    let mut heap = TaggedHeap::new();
    let identity = heap.heap_identity();
    for index in 0..64 {
        let old = heap.alloc_cons(TaggedValue::fixnum(index), TaggedValue::NIL);
        // SAFETY: `old` was just allocated by this heap's mutator. No
        // collection intervenes, and fixture collections seed this table.
        // The heap outlives the holder and every materialization.
        let old_root = unsafe { SharedRoot::new(&heap, old) };
        let collector = existing_table(identity).expect("admitted root has a table");
        let barrier = Arc::new(Barrier::new(2));
        let holder_barrier = Arc::clone(&barrier);
        let holder = std::thread::spawn(move || {
            holder_barrier.wait();
            drop(old_root);
        });
        barrier.wait();
        let mut snapshot = Vec::new();
        collect_shared_root_gc_roots(identity, &mut snapshot);
        holder.join().unwrap();
        // Retirement racing capture may leave the old root in this snapshot
        // for one collection, or may exclude it. It never fabricates a root.
        assert!(snapshot.iter().all(|value| value.bits() == old.bits()));
        assert!(!LIVE_TABLES.is_empty());
        assert_eq!(cell_census(identity), (0, 1));

        let new = heap.alloc_cons(TaggedValue::fixnum(index + 1), TaggedValue::NIL);
        // SAFETY: `new` is fresh and live on this same mutator. The retained
        // collector Arc keeps only table storage alive; registration gives
        // this new object its own lease before the collection below.
        let new_root = unsafe { SharedRoot::new(&heap, new) };
        let current = existing_table(identity).expect("re-admitted root has a table");
        assert!(Arc::ptr_eq(&collector, &current));
        drop(current);
        drop(collector);
        snapshot.clear();
        collect_shared_root_gc_roots(identity, &mut snapshot);
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].bits(), new.bits());
        heap.collect_exact(snapshot.into_iter());
        assert!(!heap.owns_heap_value_for_test(old));
        assert!(heap.owns_heap_value_for_test(new));
        assert_eq!(
            new_root.materialize(&heap).unwrap().value().bits(),
            new.bits()
        );
        drop(new_root);
        assert!(existing_table(identity).is_none());
        let mut no_roots = Vec::new();
        collect_shared_root_gc_roots(identity, &mut no_roots);
        assert!(no_roots.is_empty());
        heap.collect_exact(no_roots.into_iter());
        assert!(!heap.owns_heap_value_for_test(new));
    }
}
