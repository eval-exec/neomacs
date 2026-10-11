//! Direct Rust regressions for grouped root publication and explicit lifetime.

use super::*;
use crate::tagged::transport::root_table::collect_shared_root_gc_roots;

fn exclude_fresh(heap: &mut TaggedHeap) -> FacadeMarkExclusion {
    // SAFETY: each fixture calls this before allocating any external root.
    // It owns the fresh idle heap exclusively, with no obarray/TLS aliases,
    // callbacks, concurrent launcher or unfinished collection extent.
    unsafe { heap.drain_to_quiescent(|_| {}) }
        .unwrap()
        .exclude_concurrent_mark()
        .unwrap()
}

fn roots(heap: &TaggedHeap) -> Vec<TaggedValue> {
    let mut roots = Vec::new();
    collect_shared_root_gc_roots(heap, &mut roots);
    roots
}

fn publish(
    heap: &TaggedHeap,
    exclusion: &FacadeMarkExclusion,
    values: &[TaggedValue],
) -> RootBatchLease {
    let prepared = PreparedRootBatch::reserve(heap, exclusion, values.len()).unwrap();
    // SAFETY: callers pass only fresh same-heap objects or immediates, on
    // this fixture's original sole mutator. No collection or callback occurs
    // between allocation, fill and publication. Each heap remains owned for
    // all subsequent scans; tests collect from its registered table roots.
    unsafe { prepared.finalize(values) }
        .unwrap()
        .publish()
        .unwrap()
}

#[test]
fn reserved_storage_is_initialized_and_invisible_until_publication() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let value = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::NIL);
    let prepared = PreparedRootBatch::reserve(&heap, &exclusion, 4).unwrap();
    assert!(prepared.payload.words.iter().all(Option::is_none));
    let payload_address = Arc::as_ptr(&prepared.payload);
    let words_address = prepared.payload.words.as_ptr();
    let control_address = Arc::as_ptr(&prepared.reservation.ownership.as_ref().unwrap().control);
    assert!(roots(&heap).is_empty());

    // SAFETY: value is freshly allocated on this exclusive owning heap;
    // neither a callback nor collection occurs before publication below.
    let finalized = unsafe { prepared.finalize(&[value]) }.unwrap();
    assert_eq!(Arc::as_ptr(&finalized.payload), payload_address);
    assert_eq!(finalized.payload.words.as_ptr(), words_address);
    assert_eq!(finalized.payload.used, 1);
    assert!(finalized.payload.words[1..].iter().all(Option::is_none));
    assert!(roots(&heap).is_empty());
    let lease = finalized.publish().unwrap();
    let control = &lease.ownership.as_ref().unwrap().control;
    assert_eq!(Arc::as_ptr(control), control_address);
    assert_eq!(Arc::as_ptr(control.payload.get().unwrap()), payload_address);
    assert_eq!(control.payload.get().unwrap().words.as_ptr(), words_address);
    assert_eq!(roots(&heap), vec![value]);

    heap.collect_exact(roots(&heap).into_iter());
    assert!(heap.owns_heap_value_for_test(value));
    assert_eq!(value.cons_car(), TaggedValue::fixnum(7));
    lease.try_finish().unwrap();
    assert!(roots(&heap).is_empty());
    heap.collect_exact(roots(&heap).into_iter());
    // Membership checks never dereference the reclaimed cons.
    assert!(!heap.owns_heap_value_for_test(value));
}

#[test]
fn one_batch_keeps_transitive_children_and_filters_immediates() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let child = heap.alloc_float(2.5);
    let parent = heap.alloc_cons(child, TaggedValue::NIL);
    let garbage = heap.alloc_float(3.5);
    let lease = publish(
        &heap,
        &exclusion,
        &[
            TaggedValue::NIL,
            TaggedValue::T,
            TaggedValue::fixnum(9),
            parent,
        ],
    );
    assert_eq!(lease.root_count(), 1);
    assert_eq!(roots(&heap), vec![parent]);
    let reader = lease.reader(&heap).unwrap();
    let mut local = Vec::new();
    reader.append_roots(&mut local);
    assert_eq!(local, vec![parent]);
    reader.finish().unwrap();
    heap.collect_exact(roots(&heap).into_iter());
    assert!(heap.owns_heap_value_for_test(parent));
    assert!(heap.owns_heap_value_for_test(child));
    assert!(!heap.owns_heap_value_for_test(garbage));
    assert_eq!(parent.cons_car(), child);
    assert_eq!(child.xfloat(), 2.5);
    lease.try_finish().unwrap();
    heap.collect_exact(roots(&heap).into_iter());
    assert!(!heap.owns_heap_value_for_test(parent));
    assert!(!heap.owns_heap_value_for_test(child));
}

#[test]
fn cancellation_reuses_position_with_a_new_generation_and_control() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let prepared = PreparedRootBatch::reserve(&heap, &exclusion, 0).unwrap();
    let ownership = prepared.reservation.ownership.as_ref().unwrap();
    let table = Arc::clone(&ownership.table);
    let old = Arc::clone(&ownership.control);
    drop(prepared);
    assert_eq!(old.state().unwrap(), SlotState::Cancelled);
    let next = PreparedRootBatch::reserve(&heap, &exclusion, 0).unwrap();
    let current = &next.reservation.ownership.as_ref().unwrap().control;
    assert_eq!(old.key.slot, current.key.slot);
    assert_ne!(old.key.generation, current.key.generation);
    assert!(!Arc::ptr_eq(&old, current));
    assert!(matches!(
        table.lock().batches.as_ref().unwrap().validate(&old),
        Err(RootBatchError::StaleReservation)
    ));
    // An old control never mutates a later generation occupying the position.
    old.set_state(SlotState::Cancelled);
    assert_eq!(current.state().unwrap(), SlotState::Reserved);
    assert!(roots(&heap).is_empty());
    drop(next);
    drop(exclusion);
    assert!(!heap.facade_mark_is_excluded());
}

#[test]
fn admission_rejects_foreign_exclusion_and_foreign_reader() {
    let mut owner = TaggedHeap::new();
    let foreign = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut owner);
    assert!(matches!(
        PreparedRootBatch::reserve(&foreign, &exclusion, 1),
        Err(RootBatchError::Exclusion(
            FacadeMarkExclusionError::ForeignHeap { .. }
        ))
    ));
    let value = owner.alloc_float(4.5);
    let lease = publish(&owner, &exclusion, &[value]);
    assert!(matches!(
        lease.reader(&foreign),
        Err(RootBatchError::ForeignHeap { .. })
    ));
    assert!(roots(&foreign).is_empty());
    assert_eq!(
        lease
            .ownership
            .as_ref()
            .unwrap()
            .control
            .readers
            .load(Ordering::Acquire),
        0
    );
    lease.try_finish().unwrap();
}

#[test]
fn capacity_failure_preserves_unique_storage_for_original_owner_retry() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let first = heap.alloc_float(5.5);
    let second = heap.alloc_float(6.5);
    let prepared = PreparedRootBatch::reserve(&heap, &exclusion, 1).unwrap();
    let storage = Arc::as_ptr(&prepared.payload);
    // SAFETY: both objects are fresh on this sole mutator; this rejected fill
    // performs no collection or callback and preserves prepared ownership.
    let failure = unsafe { prepared.finalize(&[first, second]) }.unwrap_err();
    assert!(matches!(
        failure.error(),
        RootBatchError::Capacity {
            required: 2,
            capacity: 1
        }
    ));
    let (prepared, _) = failure.into_parts();
    assert_eq!(Arc::as_ptr(&prepared.payload), storage);
    assert_eq!(prepared.payload.used, 0);
    assert!(roots(&heap).is_empty());
    // SAFETY: the retained prepared stage has the same heap and live first
    // object; no collection, callback or ownership handoff intervened.
    let lease = unsafe { prepared.finalize(&[first]) }
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(roots(&heap), vec![first]);
    lease.try_finish().unwrap();
}

#[test]
fn impossible_host_capacity_is_rejected_before_reservation_or_epoch_retention() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    assert!(matches!(
        PreparedRootBatch::reserve(&heap, &exclusion, usize::MAX),
        Err(RootBatchError::StorageAllocation(_))
    ));
    assert!(roots(&heap).is_empty());
    drop(exclusion);
    assert!(!heap.facade_mark_is_excluded());
}

#[test]
fn contended_finish_returns_busy_without_unrooting_or_waiting_for_the_lock() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let value = heap.alloc_float(6.75);
    let lease = publish(&heap, &exclusion, &[value]);
    let ownership = lease.ownership.as_ref().unwrap();
    let table = Arc::clone(&ownership.table);
    let control = Arc::clone(&ownership.control);
    let guard = table.lock();
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || send.send(lease.try_finish()).unwrap());
    let result = receive.recv_timeout(std::time::Duration::from_secs(2));
    // Release before joining even on failure: the regression must not strand
    // its own child if a future implementation accidentally blocks here.
    drop(guard);
    worker.join().unwrap();
    let failure = result
        .expect("finish must return while another thread holds the table lock")
        .unwrap_err();
    assert!(matches!(failure.error(), RootBatchError::Busy));
    assert_eq!(control.state().unwrap(), SlotState::Live);
    assert_eq!(roots(&heap), vec![value]);
    assert!(heap.facade_mark_is_excluded());
    let (lease, _) = failure.into_parts();
    lease.try_finish().unwrap();
    assert!(roots(&heap).is_empty());
}

#[test]
fn closing_keeps_scannable_roots_until_the_explicit_reader_finishes() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let value = heap.alloc_float(7.5);
    let lease = publish(&heap, &exclusion, &[value]);
    // Model a registry-owned logical read below the public lease-borrow seam.
    // Its owner retains independent table/control storage and an epoch token,
    // which the private constructor now borrows for the whole reader extent.
    // Public reader() borrows the actual lease: consuming that borrowed lease
    // is statically prohibited and is not a public API interleaving here.
    let ownership = lease.ownership.as_ref().unwrap();
    let reader_owner = BatchOwnership {
        table: Arc::clone(&ownership.table),
        control: Arc::clone(&ownership.control),
        epoch: ownership.epoch.retain().unwrap(),
    };
    let reader = RootBatchReader::admit(&reader_owner, &heap).unwrap();
    let failure = lease.try_finish().unwrap_err();
    assert!(matches!(
        failure.error(),
        RootBatchError::ReadersOutstanding {
            readers: 1,
            abandoned: 0
        }
    ));
    let (lease, _) = failure.into_parts();
    assert!(matches!(lease.reader(&heap), Err(RootBatchError::Closing)));
    // Closing stops escaping-reader admission, while a synchronous locked
    // common-table scan continues to retain and seed its registered roots.
    assert_eq!(roots(&heap), vec![value]);
    let mut local = Vec::new();
    reader.append_roots(&mut local);
    assert_eq!(local, vec![value]);
    reader.finish().unwrap();
    drop(reader_owner);
    let retired = lease.try_finish().unwrap();
    assert_eq!(retired.heap_identity(), heap.heap_identity());
    assert_eq!(retired.root_count(), 1);
    assert!(roots(&heap).is_empty());
}

#[test]
fn abandoned_reader_cannot_be_mistaken_for_explicit_completion() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let value = heap.alloc_float(8.5);
    let lease = publish(&heap, &exclusion, &[value]);
    drop(lease.reader(&heap).unwrap());
    let failure = lease.try_finish().unwrap_err();
    assert!(matches!(
        failure.error(),
        RootBatchError::ReadersOutstanding {
            readers: 0,
            abandoned: 1
        }
    ));
    let (lease, _) = failure.into_parts();
    drop(exclusion);
    assert!(heap.facade_mark_is_excluded());
    heap.collect_exact(roots(&heap).into_iter());
    assert!(heap.owns_heap_value_for_test(value));
    // Deliberately exercise the conservative incomplete Drop fallback. The
    // retained table and exclusion are intentional; there is no marker child
    // or native callback, and the heap's fallback retains its backing storage.
    drop(lease);
    assert!(heap.facade_mark_is_excluded());
    assert_eq!(roots(&heap), vec![value]);
}

#[test]
fn live_lease_drop_retains_registration_and_exclusion_without_retiring() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let value = heap.alloc_float(9.5);
    let lease = publish(&heap, &exclusion, &[value]);
    let control = Arc::clone(&lease.ownership.as_ref().unwrap().control);
    drop(exclusion);
    drop(lease);
    assert_eq!(control.state().unwrap(), SlotState::Live);
    assert!(heap.facade_mark_is_excluded());
    heap.collect_exact(roots(&heap).into_iter());
    assert!(heap.owns_heap_value_for_test(value));
    // SAFETY: this fixture is the exclusive heap owner, with no callbacks,
    // obarray/TLS aliases or unfinished synchronous collection extent.
    assert!(matches!(
        unsafe { heap.permit_concurrent_mark() },
        Err(crate::tagged::gc::ConcurrentMarkAdmissionError::Excluded(_))
    ));
}

#[test]
fn explicit_finish_on_foreign_thread_releases_epoch_but_inert_storage_does_not_hold_it() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let value = heap.alloc_float(10.5);
    let lease = publish(&heap, &exclusion, &[value]);
    let table = Arc::clone(&lease.ownership.as_ref().unwrap().table);
    let control = Arc::clone(&lease.ownership.as_ref().unwrap().control);
    drop(exclusion);
    assert!(heap.facade_mark_is_excluded());
    // Only opaque metadata/storage crosses this boundary. The owner retains
    // its local value without a collection, and no reader or collector is
    // active; retirement may safely release this supplemental registration.
    let retired = std::thread::spawn(move || lease.try_finish().unwrap())
        .join()
        .unwrap();
    assert_eq!(retired.heap_identity(), heap.heap_identity());
    assert_eq!(control.state().unwrap(), SlotState::Retired);
    assert!(control.payload.get().is_some());
    assert!(!heap.facade_mark_is_excluded());
    assert!(roots(&heap).is_empty());
    // SAFETY: this sole-owner fixture has completed all reads and collections;
    // the retained inert host table has no heap access or exclusion authority.
    drop(unsafe { heap.permit_concurrent_mark() }.unwrap());
    drop(control);
    drop(table);
}

#[test]
fn generation_and_slot_conversion_exhaustion_return_typed_errors() {
    let mut slots = BatchSlots::default();
    let first = slots.reserve().unwrap();
    first.set_state(SlotState::Cancelled);
    slots.next_generation = u64::MAX;
    assert!(matches!(
        slots.reserve(),
        Err(RootBatchError::GenerationExhausted)
    ));
    slots.validate(&first).unwrap();
    assert_eq!(first.state().unwrap(), SlotState::Cancelled);
    assert!(matches!(
        BatchSlotId::try_from(usize::MAX),
        Err(RootBatchError::SlotSpaceExhausted)
    ));
    assert_eq!(usize::from(BatchSlotId::try_from(0).unwrap()), 0);
}

#[test]
fn exhausted_abandonment_never_wraps_into_a_retirable_zero() {
    let mut heap = TaggedHeap::new();
    let exclusion = exclude_fresh(&mut heap);
    let lease = publish(&heap, &exclusion, &[]);
    let control = Arc::clone(&lease.ownership.as_ref().unwrap().control);
    control.abandoned.store(usize::MAX, Ordering::Relaxed);
    drop(lease.reader(&heap).unwrap());
    assert_eq!(control.readers.load(Ordering::Acquire), 0);
    assert_eq!(control.abandoned.load(Ordering::Acquire), usize::MAX);
    let failure = lease.try_finish().unwrap_err();
    assert!(matches!(
        failure.error(),
        RootBatchError::ReadersOutstanding {
            readers: 0,
            abandoned: usize::MAX
        }
    ));
    // This deliberately exhausted obligation has no recovery path: Drop must
    // retain the epoch and table rather than silently claim completion.
    drop(failure);
    drop(exclusion);
    assert!(heap.facade_mark_is_excluded());
}
