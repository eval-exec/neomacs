//! These check registry transport/retirement, not the complete minor algorithm.
use super::young_registry::{StoppedYoungLogs, YoungRegistryError};
use super::*;

#[test]
fn canonical_young_wrong_heap_return_retains_loan_for_correct_retry() {
    let proof = scan_contract::StoppedMutatorsProof::for_test();
    let mut first = mutator_gc::MutatorSatbSlot::new(HeapIdentity::issue());
    let mut other = mutator_gc::MutatorSatbSlot::new(HeapIdentity::issue());
    let id = first.current_young_id();
    let block = ConsBlock::new();
    let base = ConsBlockBase::of(&block);
    let mut pending = Vec::new();
    let mut loan = first.take_young_loan(id, &proof).unwrap();
    let index = loan.log_mut().push(base);
    assert_eq!(index.index(), 0);
    loan.log_mut().ensure_pending(&mut pending);
    let rejected = other.return_young_loan(loan, &proof).unwrap_err();
    assert_eq!(rejected.reason(), YoungRegistryError::WrongHeap);
    other.ensure_young_logs_stopped(&proof).unwrap();
    assert_eq!(
        first.ensure_young_logs_stopped(&proof),
        Err(YoungRegistryError::OutstandingLoans(1))
    );
    first
        .return_young_loan(rejected.into_loan(), &proof)
        .unwrap();
    let mut stopped = StoppedYoungLogs::new(&mut first, &proof).unwrap();
    let log = stopped.resolve_mut(id).unwrap();
    assert_eq!(log.blocks(), &[base]);
    assert_eq!(pending, vec![id]);
    assert_eq!(log.pending_index().unwrap().index(), 0);
}

#[test]
fn canonical_young_dropped_worker_loan_blocks_capture() {
    let proof = scan_contract::StoppedMutatorsProof::for_test();
    let mut registry = mutator_gc::MutatorSatbSlot::new(HeapIdentity::issue());
    let id = registry.current_young_id();
    let loan = registry.take_young_loan(id, &proof).unwrap();
    drop(loan);
    assert_eq!(
        StoppedYoungLogs::new(&mut registry, &proof).unwrap_err(),
        // Compare the typed error, not a lossy textual/boolean admission result.
        YoungRegistryError::OutstandingLoans(1)
    );
}

#[test]
fn canonical_young_only_loan_moves_to_worker_without_satb_send() {
    let proof = scan_contract::StoppedMutatorsProof::for_test();
    let mut registry = mutator_gc::MutatorSatbSlot::new(HeapIdentity::issue());
    let id = registry.current_young_id();
    let block = ConsBlock::new();
    let base = ConsBlockBase::of(&block);
    let loan = registry.take_young_loan(id, &proof).unwrap();
    // Actual thread transfer exercises Send of ONLY the loan; registry/SATB
    // remain on this thread. The block lives here through worker join.
    let loan = std::thread::spawn(move || {
        let mut loan = loan;
        let _position = loan.log_mut().push(base);
        loan
    })
    .join()
    .unwrap();
    registry.return_young_loan(loan, &proof).unwrap();
    let mut stopped = StoppedYoungLogs::new(&mut registry, &proof).unwrap();
    assert_eq!(stopped.resolve_mut(id).unwrap().blocks(), &[base]);
}

#[test]
fn canonical_young_retire_rejects_loan_before_satb_take_then_drains() {
    let mut heap = TaggedHeap::new();
    let handoff = heap.satb_handoff.clone();
    // SAFETY: no marker/worker/TLS aliases/callbacks exist; fixture owns heap.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let physical = world.register_mutator_gc_for_test();
    let id = world.registered_young_id_for_test(physical).unwrap();
    world
        .mutator_gc_for_test(physical)
        .satb
        .satb
        .push(MarkWord::of(TaggedValue::fixnum(73)), &handoff);
    let loan = world.take_young_log_loan(id).unwrap();
    assert!(matches!(
        world.retire_young_registration_for_test(id),
        Err(mutator_gc::MutatorRetireError::Young(
            YoungRegistryError::LogLoaned
        ))
    ));
    assert_eq!(
        world
            .mutator_gc_for_test(physical)
            .satb
            .satb
            .storage_usage()
            .words,
        1
    );
    assert_eq!(
        world.ensure_young_logs_stopped(),
        Err(YoungRegistryError::OutstandingLoans(1))
    );
    world.return_young_log_loan(loan).unwrap();
    world
        .retire_young_registration_for_test(id)
        .unwrap()
        .acknowledge();
    // The receipt did not remove the retired canonical log.
    world
        .with_stopped_young_logs(|_, logs, _| {
            assert_eq!(logs.resolve_mut(id).unwrap().id(), id);
        })
        .unwrap();
    assert_eq!(world.fold_satb().unwrap(), 1);
    world.ensure_satb_empty().unwrap();
    assert!(matches!(
        world.take_young_log_loan(id),
        Err(YoungRegistryError::Retired)
    ));
}

#[test]
fn canonical_young_nested_selected_owner_capture_retire_and_panic_restore() {
    let mut heap = AliasableBox::new(TaggedHeap::new());
    set_tagged_heap_root(heap.root());
    let handoff = heap.satb_handoff.clone();
    // SAFETY: all three artificial owners are confined to this one fixture;
    // selections/captures only touch Rust logs, with no Lisp/safepoint/marker.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let a = world.current_young_registration_id();
    let pb = world.register_mutator_gc_for_test();
    let pc = world.register_mutator_gc_for_test();
    let b = world.registered_young_id_for_test(pb).unwrap();
    let c = world.registered_young_id_for_test(pc).unwrap();
    world
        .mutator_gc_for_test(pb)
        .satb
        .satb
        .push(MarkWord::of(TaggedValue::fixnum(91)), &handoff);
    drop(world);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        heap.with_mutator_gc_for_test(pb, |root| {
            // SAFETY: outer selection retains exclusive heap ownership.
            let selected =
                unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut *root.as_ptr()) };
            assert_eq!(selected.current_young_registration_id(), b);
            drop(selected);
            // SAFETY: no outer heap loan crosses the nested root switch.
            unsafe {
                TaggedHeap::with_registered_mutator_gc_for_test(
                    root,
                    pc,
                    &scan_contract::StoppedMutatorsProof::for_test(),
                    |root| {
                        // SAFETY: nested selection retains the same exclusion.
                        let mut nested = unsafe {
                            scan_contract::SingleMutatorWorld::from_heap(&mut *root.as_ptr())
                        };
                        assert_eq!(nested.current_young_registration_id(), c);
                        nested
                            .with_stopped_young_logs(|_, logs, _| {
                                for id in [a, b, c] {
                                    assert_eq!(logs.resolve_mut(id).unwrap().id(), id);
                                }
                            })
                            .unwrap();
                        // b is physically in pc's registered slot while c is current.
                        nested
                            .retire_young_registration_for_test(b)
                            .unwrap()
                            .acknowledge();
                        panic!("nested selected owner unwind");
                    },
                )
            }
            .unwrap();
        })
        .unwrap();
    }));
    assert!(outcome.is_err());
    // SAFETY: both selection guards and all nested callback loans ended.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    assert_eq!(world.current_young_registration_id(), a);
    world
        .with_stopped_young_logs(|_, logs, _| {
            for id in [a, b, c] {
                assert_eq!(logs.resolve_mut(id).unwrap().id(), id);
            }
        })
        .unwrap();
    assert!(matches!(
        world.take_young_log_loan(b),
        Err(YoungRegistryError::Retired)
    ));
    assert_eq!(world.fold_satb().unwrap(), 1);
    assert!(tagged_heap_is_current(world.heap()));
}

#[cfg(feature = "gc-memory-telemetry")]
#[test]
fn canonical_young_inventory_counts_resident_capacity_without_satb_double_count() {
    let proof = scan_contract::StoppedMutatorsProof::for_test();
    let mut registry = mutator_gc::MutatorSatbSlot::new(HeapIdentity::issue());
    let before = registry.allocation_bytes();
    let block = ConsBlock::new();
    let payload_bytes = {
        let log = registry.current_young_log_mut().unwrap();
        let _position = log.push(ConsBlockBase::of(&block));
        log.allocation_bytes()
    };
    assert!(payload_bytes >= std::mem::size_of::<ConsBlockBase>());
    assert_eq!(registry.allocation_bytes(), before + payload_bytes);
    let id = registry.current_young_id();
    let loan = registry.take_young_loan(id, &proof).unwrap();
    // Owned by the loan, not a second payload still resident in the registry.
    assert_eq!(registry.allocation_bytes(), before);
    assert_eq!(
        registry.ensure_young_logs_stopped(&proof),
        Err(YoungRegistryError::OutstandingLoans(1))
    );
    registry.return_young_loan(loan, &proof).unwrap();
    assert_eq!(registry.allocation_bytes(), before + payload_bytes);
}

#[test]
fn canonical_young_drain_retains_owner_storage_and_removes_last_pending() {
    let mut registry = mutator_gc::MutatorSatbSlot::new(HeapIdentity::issue());
    let first = ConsBlock::new();
    let second = ConsBlock::new();
    let log = registry.current_young_log_mut().unwrap();
    let _first_position = log.push(ConsBlockBase::of(&first));
    let _second_position = log.push(ConsBlockBase::of(&second));
    let mut pending = Vec::new();
    log.ensure_pending(&mut pending);
    let storage = log.blocks().as_ptr();
    let drained: Vec<_> = log.drain_blocks().collect();
    assert_eq!(
        drained,
        [ConsBlockBase::of(&first), ConsBlockBase::of(&second)]
    );
    assert!(log.blocks().is_empty());
    assert_eq!(log.blocks().as_ptr(), storage);
    assert_eq!(log.remove_pending(&mut pending), None);
    assert_eq!(log.pending_index(), None);
    assert!(pending.is_empty());
}
