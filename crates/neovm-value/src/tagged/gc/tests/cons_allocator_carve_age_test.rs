//! Canonical carve classification without a second young-block registry.
use super::super::scan_contract::SingleMutatorWorld;
use super::*;

fn set_mode(allocator: &mut ConsAllocator, mode: AllocatorAccessMode) {
    if matches!(mode, AllocatorAccessMode::Admitted) {
        // SAFETY: fixtures retain all storage and have no workers or regions.
        unsafe { allocator.stopped_world() }.admit_threads();
    }
}

fn registered_block(allocator: &mut ConsAllocator, old: bool) -> ConsBlock {
    let mut block = ConsBlock::new();
    block.reserve_tail(4);
    allocator.register(&block);
    if old {
        // Coherent stopped fixture: install one actual old cell and update the
        // canonical count through the same collector-owned count API.
        unsafe {
            (*block.cells_ptr()).car.set_plain(TaggedValue::T);
            (*block.cells_ptr()).set_cdr(TaggedValue::NIL);
        }
        block.trailer().set_old(0);
        // SAFETY: no worker, marker or allocation region exists in this fixture.
        unsafe { allocator.stopped_world() }.record_major_old_count(ConsBlockBase::of(&block), 1);
    }
    block
}

fn carve(
    world: &mut SingleMutatorWorld<'_>,
    allocator: &mut ConsAllocator,
    registration: YoungRegistrationId,
    base: ConsBlockBase,
) -> (ConsCarveAge, Vec<ConsBlockBase>) {
    let mut loan = world.take_young_log_loan(registration).unwrap();
    let result = allocator.note_carve(base, loan.log_mut()).unwrap();
    let listed = loan.log_mut().blocks().to_vec();
    world.return_young_log_loan(loan).unwrap();
    (result, listed)
}

fn unregister(
    world: &mut SingleMutatorWorld<'_>,
    allocator: &mut ConsAllocator,
    base: ConsBlockBase,
) {
    world
        .with_stopped_young_logs(|_, logs, _| {
            // SAFETY: all fixture loans are returned, no grant is active, and
            // the caller retains this block through canonical unlink/removal.
            unsafe { allocator.stopped_world() }
                .unregister(base, logs)
                .unwrap();
        })
        .unwrap();
}

#[test]
fn carve_age_owner_facade_certifies_fresh_block_and_keeps_one_log_entry() {
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocatorOwner::new(heap.heap_identity());
    let mut block = ConsBlock::new();
    block.reserve_tail(4);
    allocator.register(&block);
    let base = ConsBlockBase::of(&block);
    // SAFETY: this fixture owns the stopped heap, registry and external block.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let id = world.current_young_registration_id();
    let mut loan = world.take_young_log_loan(id).unwrap();
    for _ in 0..2 {
        let age = allocator.note_carve(base, loan.log_mut()).unwrap();
        assert_eq!(age, ConsCarveAge::NoOldOwners);
        assert!(age.no_old_owners());
        assert_eq!(loan.log_mut().blocks(), &[base]);
    }
    world.return_young_log_loan(loan).unwrap();
    allocator.with_state(|state, _| {
        let entry = state.block_for_base(base);
        assert!(entry.young.load(Ordering::Relaxed));
        assert_eq!(entry.old_count, 0);
        assert_eq!(entry.young_log_slot.unwrap().registration, id);
    });
    world
        .with_stopped_young_logs(|_, logs, _| {
            // SAFETY: no worker, region or loan retains this fixture block.
            unsafe { allocator.stopped_world() }
                .unregister(base, logs)
                .unwrap();
        })
        .unwrap();
}

#[test]
fn carve_age_sole_mixed_block_stays_conservative_after_becoming_young_work() {
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocator::new(heap.heap_identity());
    let block = registered_block(&mut allocator, true);
    let base = ConsBlockBase::of(&block);
    assert!(block.trailer().age_is_old(0));
    assert!(!block.trailer().age_is_old(1));
    // SAFETY: the fixture excludes workers, callbacks and collection.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let id = world.current_young_registration_id();
    for _ in 0..2 {
        let (age, listed) = carve(&mut world, &mut allocator, id, base);
        assert_eq!(age, ConsCarveAge::MayHaveOldOwners);
        assert!(!age.no_old_owners());
        assert_eq!(listed, vec![base]);
    }
    allocator.with_state(|state, _| {
        let entry = state.block_for_base(base);
        assert!(entry.young.load(Ordering::Relaxed));
        assert_eq!(entry.old_count, block.count_old());
    });
    unregister(&mut world, &mut allocator, base);
}

#[test]
fn carve_age_shared_facade_distinguishes_fresh_and_mixed_blocks() {
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocator::new(heap.heap_identity());
    let fresh = registered_block(&mut allocator, false);
    let mixed = registered_block(&mut allocator, true);
    set_mode(&mut allocator, AllocatorAccessMode::Admitted);
    // SAFETY: the admitted fixture is parked; no actual worker or region runs.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let id = world.current_young_registration_id();
    let mut loan = world.take_young_log_loan(id).unwrap();
    {
        let shared = allocator.shared();
        for (block, expected) in [
            (&fresh, ConsCarveAge::NoOldOwners),
            (&mixed, ConsCarveAge::MayHaveOldOwners),
        ] {
            let age = shared
                .note_carve(ConsBlockBase::of(block), loan.log_mut())
                .unwrap();
            assert_eq!(age, expected);
        }
        assert_eq!(loan.log_mut().blocks().len(), 2);
        shared.with_state(|state, mode| {
            assert!(matches!(mode, AllocatorAccessMode::Admitted));
            for block in [&fresh, &mixed] {
                let entry = state.block_for_base(ConsBlockBase::of(block));
                assert!(entry.young.load(Ordering::Relaxed));
                assert_eq!(entry.old_count, block.count_old());
            }
        });
    }
    world.return_young_log_loan(loan).unwrap();
    unregister(&mut world, &mut allocator, ConsBlockBase::of(&fresh));
    unregister(&mut world, &mut allocator, ConsBlockBase::of(&mixed));
}

#[test]
fn carve_age_wrong_heap_refusal_preserves_classification_inputs_and_logs() {
    for mode in [AllocatorAccessMode::Sole, AllocatorAccessMode::Admitted] {
        let owner_heap = TaggedHeap::new();
        let mut foreign_heap = TaggedHeap::new();
        let mut allocator = ConsAllocator::new(owner_heap.heap_identity());
        let block = registered_block(&mut allocator, true);
        let base = ConsBlockBase::of(&block);
        set_mode(&mut allocator, mode);
        // SAFETY: both domains and all registered storage remain stopped.
        let mut world = unsafe { SingleMutatorWorld::from_heap(&mut foreign_heap) };
        let id = world.current_young_registration_id();
        let mut loan = world.take_young_log_loan(id).unwrap();
        assert_eq!(
            allocator.note_carve(base, loan.log_mut()),
            Err(YoungRegistryError::WrongHeap)
        );
        assert!(loan.log_mut().blocks().is_empty());
        assert_eq!(loan.log_mut().pending_index(), None);
        allocator.with_state(|state, _| {
            let entry = state.block_for_base(base);
            assert_eq!(entry.old_count, 1);
            assert!(!entry.young.load(Ordering::Relaxed));
            assert_eq!(entry.young_log_slot, None);
            assert!(state.pending_mutators.is_empty());
        });
        world.return_young_log_loan(loan).unwrap();
        allocator.unregister(base);
    }
}

#[test]
fn carve_age_skipped_recording_fault_never_certifies_no_old_owners() {
    for mode in [AllocatorAccessMode::Sole, AllocatorAccessMode::Admitted] {
        let mut heap = TaggedHeap::new();
        let mut allocator = ConsAllocator::new(heap.heap_identity());
        let block = registered_block(&mut allocator, false);
        let base = ConsBlockBase::of(&block);
        set_mode(&mut allocator, mode);
        // SAFETY: no marker, worker, callback or region uses this fixture.
        let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
        let id = world.current_young_registration_id();
        let fault = ConsAllocatorFaultGuard::inject(ConsAllocatorFault::SkipCarveRecording);
        let (age, listed) = carve(&mut world, &mut allocator, id, base);
        drop(fault);
        assert_eq!(age, ConsCarveAge::MayHaveOldOwners);
        assert!(!age.no_old_owners());
        assert!(listed.is_empty());
        allocator.with_state(|state, _| {
            let entry = state.block_for_base(base);
            assert_eq!(entry.old_count, 0);
            assert!(!entry.young.load(Ordering::Relaxed));
            assert_eq!(entry.young_log_slot, None);
            assert!(state.pending_mutators.is_empty());
        });
        allocator.unregister(base);
    }
}

#[test]
fn carve_age_does_not_require_the_current_mutator_to_own_the_young_log_entry() {
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocator::new(heap.heap_identity());
    let block = registered_block(&mut allocator, false);
    let base = ConsBlockBase::of(&block);
    set_mode(&mut allocator, AllocatorAccessMode::Admitted);
    // SAFETY: both canonical registrations are parked in this stopped fixture.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let first = world.current_young_registration_id();
    let second_physical = world.register_mutator_gc_for_test();
    let second = world.registered_young_id_for_test(second_physical).unwrap();
    let (age, listed) = carve(&mut world, &mut allocator, first, base);
    assert!(age.no_old_owners());
    assert_eq!(listed, vec![base]);
    let (age, listed) = carve(&mut world, &mut allocator, second, base);
    assert!(age.no_old_owners());
    assert!(
        listed.is_empty(),
        "the first winner still owns the sole log entry"
    );
    allocator.with_state(|state, _| {
        let entry = state.block_for_base(base);
        assert_eq!(entry.young_log_slot.unwrap().registration, first);
        assert_eq!(state.pending_mutators.len(), 1);
    });
    unregister(&mut world, &mut allocator, base);
}
