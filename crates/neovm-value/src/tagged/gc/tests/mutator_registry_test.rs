//! Integration fixtures for actual K hot/cold MutatorGcState registrations.

use super::*;

#[test]
fn satb_registered_hot_and_cold_mutators_fold_retired_owner_and_union_dedup() {
    let mut heap = TaggedHeap::new();
    let handoff = heap.satb_handoff.clone();
    heap.current_mutator_gc_mut().bytes_since_gc = 7;
    heap.current_mutator_satb_mut()
        .satb
        .push(MarkWord::of(TaggedValue::fixnum(0)), &handoff);
    heap.current_mutator_satb_mut()
        .satb_snapshotted_owners
        .insert(17);
    // SAFETY: this deterministic fixture owns all registered mutators on one
    // thread, has no worker/TLS aliases/callbacks and retains its whole heap.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let ids: Vec<_> = (0..3)
        .map(|_| world.register_mutator_gc_for_test())
        .collect();
    for (ordinal, id) in ids.iter().copied().enumerate() {
        let entry = world.mutator_gc_for_test(id);
        entry.gc.bytes_since_gc = ordinal + 1;
        // Every mutator logs the same owner independently; only the stopped
        // union may dedup across owners. This catches accidental shared sets.
        assert!(entry.satb.satb_snapshotted_owners.insert(17));
        entry.satb.satb.push(
            MarkWord::of(TaggedValue::fixnum(((ordinal + 1) * 10) as i64)),
            &handoff,
        );
    }
    world
        .retire_mutator_gc_for_test(ids[1])
        .unwrap()
        .acknowledge();
    for (ordinal, id) in ids.iter().copied().enumerate() {
        if ordinal != 1 {
            world.mutator_gc_for_test(id).satb.satb.push(
                MarkWord::of(TaggedValue::fixnum(((ordinal + 1) * 10 + 1) as i64)),
                &handoff,
            );
        }
    }
    assert_eq!(world.heap().mutators().count(), 4);
    assert_eq!(world.heap().bytes_since_gc(), 13);
    assert_eq!(world.heap().mutator_satb_states().count(), 4);
    assert_eq!(world.fold_satb().unwrap(), 6);
    let owners = world.take_satb_written_owners();
    assert_eq!(owners.len(), 1);
    assert!(owners.contains(&17));
    world.ensure_satb_empty().unwrap();
    let mut words: Vec<_> = world
        .heap()
        .gray_queue
        .iter()
        .map(|v| v.as_fixnum().unwrap())
        .collect();
    words.sort_unstable();
    assert_eq!(words, vec![0, 10, 11, 20, 30, 31]);
}

#[test]
fn satb_begin_rejects_residual_in_any_registration_without_clearing_it() {
    let mut heap = TaggedHeap::new();
    let handoff = heap.satb_handoff.clone();
    // SAFETY: fixture owns all K registered mutator states and the heap.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let id = world.register_mutator_gc_for_test();
    world
        .mutator_gc_for_test(id)
        .satb
        .satb
        .push(MarkWord::of(TaggedValue::fixnum(42)), &handoff);
    assert!(matches!(
        world.begin_satb_cycle(),
        Err(scan_contract::SatbDrainError::Residual)
    ));
    assert_eq!(
        world
            .mutator_gc_for_test(id)
            .satb
            .satb
            .storage_usage()
            .words,
        1
    );
    assert_eq!(world.fold_satb().unwrap(), 1);
    world.begin_satb_cycle().unwrap();
}

#[test]
fn satb_termination_counts_partials_separately_from_published_and_retired_chunks() {
    let mut heap = TaggedHeap::new();
    let handoff = heap.satb_handoff.clone();
    for ordinal in 0..satb::SATB_CHUNK_WORDS + 1 {
        heap.current_mutator_satb_mut()
            .satb
            .push(MarkWord::of(TaggedValue::fixnum(ordinal as i64)), &handoff);
    }
    // SAFETY: this fixture owns every paired registration, with no worker,
    // TLS alias, callback or Lisp allocation while the world is admitted.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let active = world.register_mutator_gc_for_test();
    let retired = world.register_mutator_gc_for_test();
    for ordinal in 0..satb::SATB_CHUNK_WORDS {
        world.mutator_gc_for_test(active).satb.satb.push(
            MarkWord::of(TaggedValue::fixnum((100 + ordinal) as i64)),
            &handoff,
        );
    }
    world
        .mutator_gc_for_test(retired)
        .satb
        .satb
        .push(MarkWord::of(TaggedValue::fixnum(200)), &handoff);
    world
        .retire_mutator_gc_for_test(retired)
        .unwrap()
        .acknowledge();
    // The retired owner's partial has already become a published chunk.
    // Only the current owner's one word and the active owner's full resident
    // buffer count toward the stopped-world partial work.
    assert_eq!(
        world.fold_satb_for_termination().unwrap(),
        satb::SATB_CHUNK_WORDS * 2 + 2
    );
    assert_eq!(
        world.heap().sweep_stats().last_termination_partial_words,
        satb::SATB_CHUNK_WORDS + 1
    );
    world.ensure_satb_empty().unwrap();
    // Facade quiescence can perform a second generic fold after a join. It
    // must preserve the diagnostic of the actual termination it just drained.
    assert_eq!(world.fold_satb().unwrap(), 0);
    assert_eq!(
        world.heap().sweep_stats().last_termination_partial_words,
        satb::SATB_CHUNK_WORDS + 1
    );
    // A later successful termination with no partials resets the last count.
    world.begin_satb_cycle().unwrap();
    assert_eq!(world.fold_satb_for_termination().unwrap(), 0);
    assert_eq!(world.heap().sweep_stats().last_termination_partial_words, 0);
}

#[cfg(feature = "gc-memory-telemetry")]
#[test]
fn satb_inventory_counts_aligned_control_chunks_partials_and_sidecar() {
    let mut heap = TaggedHeap::new();
    let handoff = heap.satb_handoff.clone();
    for ordinal in 0..satb::SATB_CHUNK_WORDS + 1 {
        heap.current_mutator_satb_mut()
            .satb
            .push(MarkWord::of(TaggedValue::fixnum(ordinal as i64)), &handoff);
    }
    let inventory = memory_inventory::snapshot(&heap, false);
    let find = |name| {
        inventory
            .buffers
            .iter()
            .find(|buffer| buffer.name == name)
            .unwrap()
    };
    assert_eq!(find("satb-handoff-control").capacity_bytes, 256);
    assert_eq!(find("satb-chunk-descriptors").elements, 1);
    assert_eq!(find("satb-chunk-payloads").elements, satb::SATB_CHUNK_WORDS);
    assert_eq!(find("mutator-satb-partial").elements, 1);
    assert_eq!(
        find("mutator-satb-partial").capacity_bytes,
        satb::SATB_CHUNK_WORDS * size_of::<MarkWord>()
    );
    assert_eq!(
        find("mutator-satb-sidecar").capacity_bytes,
        heap._satb_layout_reserve.allocation_bytes()
    );
    assert!(find("mutator-satb-sidecar").capacity_bytes > 0);
}

#[test]
fn satb_test_owner_switch_restores_hot_cold_state_and_tls_after_panic() {
    let mut heap = AliasableBox::new(TaggedHeap::new());
    set_tagged_heap_root(heap.root());
    heap.current_mutator_gc_mut().bytes_since_gc = 11;
    heap.current_mutator_satb_mut()
        .satb_snapshotted_owners
        .insert(23);
    // SAFETY: this thread owns the complete fixture; switching performs only
    // owner-local Rust state writes, with no Lisp allocation or safepoint.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let id = world.register_mutator_gc_for_test();
    drop(world);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), _> = heap.with_mutator_gc_for_test(id, |root| {
            // SAFETY: this callback does not cross any TLS barrier while using
            // the temporary selected-owner state loan.
            let heap = unsafe { &mut *root.as_ptr() };
            assert_eq!(heap.current_mutator_gc().bytes_since_gc, 0);
            assert!(
                heap.current_mutator_satb()
                    .satb_snapshotted_owners
                    .is_empty()
            );
            heap.current_mutator_gc_mut().bytes_since_gc = 31;
            heap.current_mutator_satb_mut()
                .satb_snapshotted_owners
                .insert(47);
            panic!("unwind from selected artificial mutator");
        });
    }));
    assert!(result.is_err());
    // SAFETY: the switch restored the owner and its callback loan ended.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    assert_eq!(world.heap().current_mutator_gc().bytes_since_gc, 11);
    assert!(
        world
            .heap()
            .current_mutator_satb()
            .satb_snapshotted_owners
            .contains(&23)
    );
    assert!(
        !world
            .heap()
            .current_mutator_satb()
            .satb_snapshotted_owners
            .contains(&47)
    );
    let selected = world.mutator_gc_for_test(id);
    assert_eq!(selected.gc.bytes_since_gc, 31);
    assert!(selected.satb.satb_snapshotted_owners.contains(&47));
    assert!(tagged_heap_is_current(world.heap()));
}

/// Heap-only reproduction of the artificial-owner helper crossing a real
/// barrier. The fixture owner and TLS start from the same allocation root.
#[test]
fn heapborrow_r6_registered_owner_barrier_keeps_root_provenance() {
    let mut heap = aliasable::AliasableBox::new(TaggedHeap::new());
    let root = heap.root();
    set_tagged_heap_root(root);
    let old = heap.alloc_cons(TaggedValue::fixnum(19), TaggedValue::NIL);
    let source = heap.alloc_cons(old, TaggedValue::NIL);
    let id = {
        // SAFETY: sole stopped fixture owner, before beginning the mark.
        let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
        world.register_mutator_gc_for_test()
    };
    heap.concurrent_begin();
    set_tagged_heap_root(root);
    {
        // SAFETY: all artificial owners are stopped on this fixture thread;
        // the selected writer changes only preallocated cons cells and logs.
        heap.with_mutator_gc_for_test(id, |_| {
            assert!(crate::tagged::mutate::set_cons_car(
                source,
                TaggedValue::NIL
            ));
        })
        .unwrap();
    }
    assert_eq!(source.cons_car(), TaggedValue::NIL);
    // SAFETY: complete external roots of this unpublished fixture mark.
    drop(unsafe { heap.drain_to_quiescent(|heap| heap.seed_root(source)) }.unwrap());
}
