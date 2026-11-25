#![deny(unsafe_code)]

use super::super::alloc_region::{ConsRegionSource, FloatRegionSource, RegionStats};
use super::super::arena_pages::ArenaRunSource;
use super::super::cons_allocator::ConsBlockBase;
use super::super::mark_word::MarkWord;
use super::super::owner_alloc_region::{ConsTailRefund, OwnerAllocRegion};
use super::super::pacing::GenerationPacingCounters;
use super::super::satb::SatbHandoff;
use super::super::young_registry::{
    YoungLogSlot, YoungRegistrationId, YoungRegistryControl, YoungRegistryError,
};
use super::super::{CONS_BLOCK_SIZE, ConsBlock, HeapIdentity, TaggedHeap};
use super::image::{ImageError, OwnerGcState, PublishedGcImage, PublishedGcWork};
use crate::intern::{NIL_SYM_ID, T_SYM_ID};
use crate::tagged::header::{ConsCell, GcHeader, HeapObjectKind};
use crate::tagged::value::TaggedValue;

#[allow(unsafe_code)]
fn restore_owner_for_test(
    state: &mut OwnerGcState,
    image: PublishedGcImage,
) -> Result<(), super::image::RestoreImageFailure> {
    // SAFETY: single-threaded unit test restoring an image published on this
    // same thread from immediate/live fixture words.
    unsafe { state.restore(image) }
}

#[allow(unsafe_code)]
fn restore_legacy_for_collection_for_test<'a>(
    image: PublishedGcImage,
    heap: &mut TaggedHeap,
    resolve_peer: impl FnOnce(
        YoungRegistrationId,
    ) -> Option<&'a mut super::super::young_registry::YoungBlockLog>,
) -> Result<(), super::image::RestoreImageFailure> {
    // SAFETY: single-threaded unit test restoring a frozen legacy image back
    // into its own live `&mut TaggedHeap` before any sweep.
    unsafe { image.restore_legacy_for_collection(heap, resolve_peer) }
}

#[allow(unsafe_code)]
fn push_satb_for_test(
    satb: &mut super::super::satb::SatbBuffer,
    value: TaggedValue,
    handoff: &SatbHandoff,
) {
    // SAFETY: unit test pushing an immediate into its own local SATB handoff.
    unsafe { satb.push(value, handoff) };
}

fn owner(heap: HeapIdentity) -> OwnerGcState {
    OwnerGcState::new(heap, YoungRegistrationId::issue_parallel(heap).unwrap(), 4).unwrap()
}

fn seed_scalar_facts(owner: &mut OwnerGcState) {
    owner.hot.allocated_count = 17;
    for (index, count) in owner.hot.memory_use_counts.iter_mut().enumerate() {
        *count = index as u64 + 1;
    }
    owner.hot.bytes_since_gc = 101;
    owner.hot.bytes_banked_at_resets = 202;
    owner.hot.pacing = GenerationPacingCounters {
        promoted_since_major: 303,
        minors_since_major: 4,
        stress_cycles_since_major: 5,
    };
    owner.hot.remset = vec![TaggedValue::fixnum(11), TaggedValue::fixnum(12)];
    owner.hot.major_cons_writes = vec![TaggedValue::fixnum(13)];
    owner.hot.major_symbol_preimages = vec![NIL_SYM_ID, T_SYM_ID];
    owner.hot.remembered_cache.fill(19);
    owner.clear_root_scratch();
    owner.push_immediate_root_slot_for_test(TaggedValue::fixnum(14));
    owner.push_immediate_root_slot_for_test(TaggedValue::fixnum(15));
}

fn assert_scalar_facts(owner: &OwnerGcState) {
    assert_eq!(owner.hot.allocated_count, 17);
    for (index, count) in owner.hot.memory_use_counts.iter().enumerate() {
        assert_eq!(*count, index as u64 + 1);
    }
    assert_eq!(owner.hot.bytes_since_gc, 101);
    assert_eq!(owner.hot.bytes_banked_at_resets, 202);
    assert_eq!(
        owner.hot.pacing,
        GenerationPacingCounters {
            promoted_since_major: 303,
            minors_since_major: 4,
            stress_cycles_since_major: 5,
        }
    );
    assert_eq!(
        owner.hot.remset,
        vec![TaggedValue::fixnum(11), TaggedValue::fixnum(12)]
    );
    assert_eq!(owner.hot.major_cons_writes, vec![TaggedValue::fixnum(13)]);
    assert_eq!(owner.hot.major_symbol_preimages, vec![NIL_SYM_ID, T_SYM_ID]);
    assert_eq!(
        owner.root_scratch_words_for_test(),
        &[
            MarkWord::from_immediate(TaggedValue::fixnum(14)).unwrap(),
            MarkWord::from_immediate(TaggedValue::fixnum(15)).unwrap(),
        ]
    );
}

fn stats_words(stats: &RegionStats) -> [u64; 11] {
    [
        stats.cons_refills,
        stats.cons_cells_granted,
        stats.cons_short_runs,
        stats.cons_from_free_list,
        stats.cons_from_block_tail,
        stats.cons_from_fresh_block,
        stats.cons_tail_closes,
        stats.float_refills,
        stats.float_slots_granted,
        stats.float_short_runs,
        stats.float_tail_closes,
    ]
}

#[test]
fn image_round_trip_preserves_facts_and_visits_every_work_family() {
    let mut heap = TaggedHeap::new();
    let mut state = owner(heap.heap_identity());
    seed_scalar_facts(&mut state);
    // Construct a real descriptor over a real initialized cons; no invented
    // tagged pointers or collector dereferences are used by this transport test.
    let cons = heap.alloc_cons(TaggedValue::fixnum(91), TaggedValue::NIL);
    heap.close_alloc_regions();
    let first = cons.as_cons_ptr().unwrap() as usize;
    heap.current_mutator_gc_mut().black_cons_region_start = Some(first);
    heap.close_black_cons_region(first + std::mem::size_of::<ConsCell>());
    state.hot.black_born_regions.push(
        heap.current_mutator_gc_mut()
            .black_born_regions
            .pop()
            .unwrap(),
    );

    let mut header = Box::new(GcHeader::new(HeapObjectKind::Float));
    let header_ptr = &mut *header as *mut GcHeader;
    let header_addr = header_ptr as usize;
    let block = ConsBlock::new();
    let base = ConsBlockBase::of(&block);
    state.hot.black_born.push(header_ptr);
    state.hot.r_mapped_seen.insert(header_addr);
    state.cold.satb_snapshotted_owners.insert(base.addr());
    state.cold.satb_string_preimage_addrs.insert(header_addr);
    let mut pending = Vec::new();
    let log = state.cold.young.resident_mut().unwrap();
    let _position = log.push(base);
    log.ensure_pending(&mut pending);
    let id = log.id();
    state.region_stats = RegionStats {
        cons_refills: 1,
        cons_cells_granted: 2,
        cons_short_runs: 3,
        cons_from_free_list: 4,
        cons_from_block_tail: 5,
        cons_from_fresh_block: 6,
        cons_tail_closes: 7,
        float_refills: 8,
        float_slots_granted: 9,
        float_short_runs: 10,
        float_tail_closes: 11,
    };

    let image = state.publish().unwrap();
    assert_eq!(image.young_id(), id);
    let summary = image.summary();
    assert_eq!(summary.allocated_count, 17);
    assert_eq!(
        summary.memory_use_counts,
        std::array::from_fn(|index| index as u64 + 1)
    );
    assert_eq!(summary.bytes_since_gc, 101);
    assert_eq!(summary.bytes_banked_at_resets, 202);
    assert_eq!(
        (
            summary.promoted_since_major,
            summary.minors_since_major,
            summary.stress_cycles_since_major
        ),
        (303, 4, 5)
    );
    let mut work = [0; 10];
    image.visit_work(|item| {
        let family = match item {
            PublishedGcWork::Remembered(_) => 0,
            PublishedGcWork::ConsWrite(_) => 1,
            PublishedGcWork::Root(_) => 2,
            PublishedGcWork::MappedOwner(addr) => {
                assert_eq!(addr, header_addr);
                3
            }
            PublishedGcWork::BlackHeader(addr) => {
                assert_eq!(addr, header_addr);
                4
            }
            PublishedGcWork::BlackRegion(region) => {
                assert_eq!(region.cons_owners().collect::<Vec<_>>(), vec![cons]);
                5
            }
            PublishedGcWork::SymbolPreimage(_) => 6,
            PublishedGcWork::SnapshottedOwner(addr) => {
                assert_eq!(addr, base.addr());
                7
            }
            PublishedGcWork::StringPreimageAddress(addr) => {
                assert_eq!(addr, header_addr);
                8
            }
            PublishedGcWork::YoungBlock(found) => {
                assert_eq!(found, base);
                9
            }
        };
        work[family] += 1;
    });
    assert_eq!(work, [2, 1, 2, 1, 1, 1, 2, 1, 1, 1]);
    assert_eq!(
        [
            summary.remembered_words,
            summary.cons_writes,
            summary.root_words,
            summary.mapped_owners,
            summary.black_headers,
            summary.black_regions,
            summary.symbol_preimages,
            summary.snapshotted_owners,
            summary.string_preimage_addrs,
            summary.young_blocks,
        ],
        work
    );
    let mut words = Vec::new();
    image.visit_logs(|word| words.push(word.bits()));
    assert_eq!(
        words,
        [11, 12, 13, 14, 15]
            .into_iter()
            .map(|n| TaggedValue::fixnum(n).bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(state.hot.allocated_count, 0);
    assert_eq!(state.hot.pacing, GenerationPacingCounters::default());
    assert!(state.hot.remset.is_empty() && state.root_scratch_is_empty());
    assert!(state.cold.young.resident().unwrap().blocks().is_empty());
    assert_eq!(stats_words(&state.region_stats), [0; 11]);

    restore_owner_for_test(&mut state, image).unwrap();
    assert_scalar_facts(&state);
    assert!(state.hot.remembered_cache.iter().all(|entry| *entry == 0));
    assert_eq!(state.hot.black_born, vec![header_ptr]);
    assert_eq!(state.hot.black_born_regions.len(), 1);
    assert!(state.hot.r_mapped_seen.contains(&header_addr));
    assert!(state.cold.satb_snapshotted_owners.contains(&base.addr()));
    assert!(state.cold.satb_string_preimage_addrs.contains(&header_addr));
    let log = state.cold.young.resident().unwrap();
    assert_eq!(log.id(), id);
    assert_eq!(log.blocks(), &[base]);
    assert_eq!(log.pending_index().unwrap().index(), 0);
    assert_eq!(pending, vec![id]);
    assert_eq!(
        stats_words(&state.region_stats),
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    );
}

#[test]
fn wrong_heap_or_young_id_restore_retains_image_for_correct_owner() {
    for wrong_heap in [false, true] {
        let heap = HeapIdentity::issue();
        let mut source = owner(heap);
        seed_scalar_facts(&mut source);
        let block = ConsBlock::new();
        let base = ConsBlockBase::of(&block);
        let mut pending = Vec::new();
        let log = source.cold.young.resident_mut().unwrap();
        let _position = log.push(base);
        log.ensure_pending(&mut pending);
        let mut destination = owner(if wrong_heap {
            HeapIdentity::issue()
        } else {
            heap
        });
        let destination_id = destination.cold.young.id();
        let image = source.publish().unwrap();
        let summary = image.summary();
        let original_id = image.young_id();
        let failure = restore_owner_for_test(&mut destination, image).unwrap_err();
        let expected = if wrong_heap {
            YoungRegistryError::WrongHeap
        } else {
            YoungRegistryError::UnknownRegistration
        };
        assert!(matches!(failure.error, ImageError::Young(error) if error == expected));
        assert_eq!(failure.image.summary(), summary);
        assert_eq!(failure.image.young_id(), original_id);
        assert_eq!(failure.image.young().blocks(), &[base]);
        assert_eq!(failure.image.young().pending_index().unwrap().index(), 0);
        assert_eq!(destination.cold.young.id(), destination_id);
        assert!(
            destination
                .cold
                .young
                .resident()
                .unwrap()
                .blocks()
                .is_empty()
        );
        assert!(
            destination
                .cold
                .young
                .resident()
                .unwrap()
                .pending_index()
                .is_none()
        );
        assert_eq!(destination.hot.allocated_count, 0);
        assert!(destination.root_scratch_is_empty());
        restore_owner_for_test(&mut source, failure.image).unwrap();
        assert_scalar_facts(&source);
        assert_eq!(source.cold.young.resident().unwrap().blocks(), &[base]);
        assert_eq!(pending, vec![original_id]);
    }
}

#[test]
fn every_open_region_fact_refuses_publication_before_moving_owner_facts() {
    for open in 0..8 {
        let mut state = owner(HeapIdentity::issue());
        seed_scalar_facts(&mut state);
        match open {
            0 => state.regions.cons = ConsRegionSource::FreeListRun,
            1 => {
                state.regions.float = FloatRegionSource::Run {
                    page: 0,
                    source: ArenaRunSource::PageTail,
                }
            }
            2 => state.jit.cons_cur.set(1),
            3 => state.jit.cons_lim.set(1),
            4 => state.jit.float_cur.set(1),
            5 => state.jit.float_lim.set(1),
            6 => state.hot.black_cons_region_start = Some(1),
            _ => state.hot.black_float_region_start = Some(1),
        }
        assert!(matches!(state.publish(), Err(ImageError::RegionsOpen)));
        assert_scalar_facts(&state);
        assert!(state.hot.remembered_cache.iter().all(|entry| *entry == 19));
        state.regions.cons = ConsRegionSource::Closed;
        state.regions.float = FloatRegionSource::Closed;
        state.jit.cons_cur.set(0);
        state.jit.cons_lim.set(0);
        state.jit.float_cur.set(0);
        state.jit.float_lim.set(0);
        state.hot.black_cons_region_start = None;
        state.hot.black_float_region_start = None;
        let image = state.publish().unwrap();
        restore_owner_for_test(&mut state, image).unwrap();
        assert_scalar_facts(&state);
    }
}

#[test]
fn an_open_owner_region_refuses_publication_without_moving_facts() {
    let mut state = owner(HeapIdentity::issue());
    seed_scalar_facts(&mut state);
    let mut region = OwnerAllocRegion::new();
    region.install_cons_free_list(0x1000, 2);
    assert!(matches!(
        state.validate_publication_with_owner(&region),
        Err(ImageError::RegionsOpen)
    ));
    assert_scalar_facts(&state);
    let refund = region.close_for_stop();
    assert_eq!(
        refund.cons,
        ConsTailRefund::FreeList {
            first: 0x1000,
            cells: 2,
        }
    );
    state.validate_publication_with_owner(&region).unwrap();
    assert_scalar_facts(&state);
}

#[test]
fn residual_satb_refuses_publication_without_moving_or_discarding_words() {
    let mut state = owner(HeapIdentity::issue());
    seed_scalar_facts(&mut state);
    let block = ConsBlock::new();
    let base = ConsBlockBase::of(&block);
    let _position = state.cold.young.resident_mut().unwrap().push(base);
    let handoff = SatbHandoff::new();
    let value = TaggedValue::fixnum(99);
    let word = MarkWord::from_immediate(value).unwrap();
    push_satb_for_test(&mut state.cold.satb, value, &handoff);
    assert!(matches!(state.publish(), Err(ImageError::SatbResidual)));
    assert_scalar_facts(&state);
    assert_eq!(state.cold.satb.words_for_test(), &[word]);
    assert_eq!(state.cold.young.resident().unwrap().blocks(), &[base]);
    state.cold.satb.publish_partial(&handoff).unwrap();
    let chunks = handoff.take_all().unwrap();
    assert_eq!(
        chunks
            .into_iter()
            .flat_map(|chunk| chunk.into_words())
            .collect::<Vec<_>>(),
        vec![word]
    );
    let image = state.publish().unwrap();
    restore_owner_for_test(&mut state, image).unwrap();
    assert_scalar_facts(&state);
}

#[test]
fn occupied_destination_refuses_restore_and_preserves_image_for_retry() {
    let mut state = owner(HeapIdentity::issue());
    seed_scalar_facts(&mut state);
    let image = state.publish().unwrap();
    let summary = image.summary();
    state.hot.bytes_since_gc = 1;
    let failure = restore_owner_for_test(&mut state, image).unwrap_err();
    assert!(matches!(failure.error, ImageError::DestinationOccupied));
    assert_eq!(failure.image.summary(), summary);
    assert_eq!(state.hot.bytes_since_gc, 1);
    assert!(state.root_scratch_is_empty());
    state.hot.bytes_since_gc = 0;
    restore_owner_for_test(&mut state, failure.image).unwrap();
    assert_scalar_facts(&state);
}

#[test]
fn young_payload_refuses_occupied_or_pending_shell_without_losing_payload() {
    for pending_only in [false, true] {
        let id = YoungRegistrationId::issue_parallel(HeapIdentity::issue()).unwrap();
        let mut slot = YoungLogSlot::new_parallel(id);
        let first = ConsBlock::new();
        let second = ConsBlock::new();
        let base = ConsBlockBase::of(&first);
        let other = ConsBlockBase::of(&second);
        let _position = slot.resident_mut().unwrap().push(base);
        let payload = slot.take_owner_payload().unwrap();
        let shell = slot.resident_mut().unwrap();
        let _position = shell.push(other);
        let mut pending = Vec::new();
        shell.ensure_pending(&mut pending);
        if pending_only {
            assert_eq!(shell.drain_blocks().collect::<Vec<_>>(), vec![other]);
        }
        let (error, payload) = slot.restore_owner_payload(payload).unwrap_err();
        assert_eq!(error, YoungRegistryError::LogNotLoaned);
        assert_eq!(payload.id(), id);
        assert_eq!(payload.blocks(), &[base]);
        let shell = slot.resident().unwrap();
        assert_eq!(shell.id(), id);
        assert_eq!(shell.pending_index().unwrap().index(), 0);
        assert_eq!(
            shell.blocks(),
            if pending_only {
                &[][..]
            } else {
                std::slice::from_ref(&other)
            }
        );
    }
}

#[test]
fn young_worker_loan_refuses_owner_transfer_until_payload_returns() {
    let (mut control, mut slot) = YoungRegistryControl::new(HeapIdentity::issue());
    let block = ConsBlock::new();
    let base = ConsBlockBase::of(&block);
    let _position = slot.resident_mut().unwrap().push(base);
    let payload = slot.take_owner_payload().unwrap();
    let loan = control.take_loan(&mut slot).unwrap();
    assert!(matches!(
        slot.take_owner_payload(),
        Err(YoungRegistryError::LogLoaned)
    ));
    let (error, payload) = slot.restore_owner_payload(payload).unwrap_err();
    assert_eq!(error, YoungRegistryError::LogLoaned);
    assert_eq!(payload.blocks(), &[base]);
    assert_eq!(
        control.ensure_stopped(),
        Err(YoungRegistryError::OutstandingLoans(1))
    );
    control.return_loan(&mut slot, loan).unwrap();
    slot.restore_owner_payload(payload).unwrap();
    assert_eq!(slot.resident().unwrap().blocks(), &[base]);
    control.ensure_stopped().unwrap();
}

#[test]
fn parallel_young_ids_are_unique_across_threads_and_heaps() {
    let heaps = [HeapIdentity::issue(), HeapIdentity::issue()];
    let workers: Vec<_> = (0..8)
        .map(|index| {
            let heap = heaps[index % heaps.len()];
            std::thread::spawn(move || {
                let id = YoungRegistrationId::issue_parallel(heap).unwrap();
                assert_eq!(id.heap_identity(), heap);
                assert!(id.is_parallel());
                id
            })
        })
        .collect();
    let ids: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    let ordinals: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(ordinals.len(), ids.len());
    let legacy = YoungRegistryControl::new(heaps[0]).1;
    assert!(!legacy.id().is_parallel());
}

#[test]
fn restore_legacy_for_collection_merges_shell_young_blocks_into_empty_frozen_log() {
    let mut heap = TaggedHeap::new();
    assert!(
        heap.tagged_heap_generational_enabled(),
        "young-log merge tests require generational GC enabled (NEOVM_GC_GENERATIONAL!=0)"
    );
    heap.close_alloc_regions();
    let legacy = PublishedGcImage::from_legacy(&mut heap).unwrap();
    assert!(legacy.young().blocks().is_empty());

    // Simulate a WorldTurn / serial-turn cons allocation while entry 0 is frozen.
    let _c = heap.alloc_cons(TaggedValue::fixnum(101), TaggedValue::fixnum(202));
    heap.close_alloc_regions();
    assert!(
        !heap
            .current_mutator_satb()
            .young
            .resident()
            .unwrap()
            .blocks()
            .is_empty(),
        "alloc_cons after freeze must populate the resident young shell"
    );

    let expected_blocks = heap
        .current_mutator_satb()
        .young
        .resident()
        .unwrap()
        .blocks()
        .to_vec();
    restore_legacy_for_collection_for_test(legacy, &mut heap, |moved_id| {
        panic!("unexpected peer pending lookup: {moved_id:?}");
    })
    .expect("restore_legacy_for_collection must fold resident shell young blocks");
    let restored = heap.current_mutator_satb().young.resident().unwrap();
    assert_eq!(restored.blocks(), expected_blocks.as_slice());
    assert!(restored.pending_index().is_some());
    heap.cons_allocator
        .verify_young_registry_with_extra(restored, &[]);
}

#[test]
fn restore_legacy_for_collection_merges_nonempty_frozen_and_shell_young_blocks() {
    let mut heap = TaggedHeap::new();
    assert!(
        heap.tagged_heap_generational_enabled(),
        "young-log merge tests require generational GC enabled (NEOVM_GC_GENERATIONAL!=0)"
    );
    // Allocate a cons so the frozen legacy image already owns a non-empty young
    // block and a pending entry.
    let _pre = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    heap.close_alloc_regions();
    let legacy = PublishedGcImage::from_legacy(&mut heap).unwrap();
    let frozen_blocks = legacy.young().blocks().to_vec();
    assert!(
        !frozen_blocks.is_empty(),
        "frozen legacy image must hold the pre-freeze young block"
    );

    // Fill the rest of the first ConsBlock so a fresh second ConsBlock is
    // allocated and recorded into the resident shell (giving both frozen and
    // shell distinct non-empty young logs + duplicate pending_mutators entries).
    for i in 0..=CONS_BLOCK_SIZE {
        let _ = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL);
    }
    heap.close_alloc_regions();
    let shell_blocks = heap
        .current_mutator_satb()
        .young
        .resident()
        .unwrap()
        .blocks()
        .to_vec();
    assert!(
        !shell_blocks.is_empty(),
        "filling past one ConsBlock must record a fresh young block in the resident shell"
    );

    restore_legacy_for_collection_for_test(legacy, &mut heap, |moved_id| {
        panic!("unexpected peer pending lookup: {moved_id:?}");
    })
    .expect("restore_legacy_for_collection must merge nonempty frozen + shell young logs");
    let restored = heap.current_mutator_satb().young.resident().unwrap();
    let mut expected = frozen_blocks;
    expected.extend_from_slice(&shell_blocks);
    assert_eq!(restored.blocks(), expected.as_slice());
    assert!(restored.pending_index().is_some());
    heap.cons_allocator
        .verify_young_registry_with_extra(restored, &[]);
}

#[test]
fn restore_legacy_for_collection_repairs_swapped_peer_pending_index() {
    let mut heap = TaggedHeap::new();
    assert!(
        heap.tagged_heap_generational_enabled(),
        "young-log merge tests require generational GC enabled (NEOVM_GC_GENERATIONAL!=0)"
    );
    // 1. Pre-freeze cons -> legacy id_0 sits at pending_mutators[0].
    let _pre = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    heap.close_alloc_regions();
    let legacy = PublishedGcImage::from_legacy(&mut heap).unwrap();
    let frozen_blocks = legacy.young().blocks().to_vec();

    // 2. Shell cons past CONS_BLOCK_SIZE -> duplicate id_0 sits at pending_mutators[1].
    for i in 0..=CONS_BLOCK_SIZE {
        let _ = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL);
    }
    heap.close_alloc_regions();
    let shell_blocks = heap
        .current_mutator_satb()
        .young
        .resident()
        .unwrap()
        .blocks()
        .to_vec();
    let shell_pending = heap
        .current_mutator_satb()
        .young
        .resident()
        .unwrap()
        .pending_index()
        .expect("shell is pending");

    // 3. Register a third ConsBlock owned by `heap` and record a carve on a
    //    parallel peer log so `peer_id` sits at the tail of `pending_mutators`
    //    (`index > shell_pending`).
    let peer_id = YoungRegistrationId::issue_parallel(heap.heap_identity()).unwrap();
    let mut peer_slot = YoungLogSlot::new_parallel(peer_id);
    let peer_block = ConsBlock::new();
    let peer_base = ConsBlockBase::of(&peer_block);
    heap.cons_allocator.register(&peer_block);
    heap.cons_blocks.push(peer_block);
    heap.cons_allocator
        .note_carve(peer_base, peer_slot.resident_mut().unwrap())
        .unwrap();
    let peer_pending_before = peer_slot
        .resident()
        .unwrap()
        .pending_index()
        .expect("peer is pending");
    assert!(
        peer_pending_before.index() > shell_pending.index(),
        "peer must sit at pending_mutators tail so swap_remove moves it"
    );

    // 4. Merging shell into frozen removes `shell_pending` via swap_remove and
    //    must preflight and repair the swapped peer log's pending index.
    let mut looked_up = None;
    let peer_log = peer_slot.resident_mut().unwrap();
    let mut peer_opt = Some(peer_log);
    restore_legacy_for_collection_for_test(legacy, &mut heap, |moved_id| {
        assert!(looked_up.is_none(), "at most one peer lookup occurs");
        looked_up = Some(moved_id);
        assert_eq!(moved_id, peer_id);
        peer_opt.take()
    })
    .expect("restore_legacy_for_collection repairs swapped peer pending index");
    assert_eq!(looked_up, Some(peer_id));
    assert_eq!(
        peer_slot.resident().unwrap().pending_index(),
        Some(shell_pending)
    );
    let restored = heap.current_mutator_satb().young.resident().unwrap();
    let mut expected = frozen_blocks;
    expected.extend_from_slice(&shell_blocks);
    assert_eq!(restored.blocks(), expected.as_slice());
    heap.cons_allocator
        .verify_young_registry_with_extra(restored, &[peer_slot.resident().unwrap()]);
}

#[test]
fn restore_legacy_for_collection_refuses_missing_swapped_peer_without_mutating() {
    let mut heap = TaggedHeap::new();
    assert!(
        heap.tagged_heap_generational_enabled(),
        "young-log merge tests require generational GC enabled (NEOVM_GC_GENERATIONAL!=0)"
    );
    let _pre = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    heap.close_alloc_regions();
    let legacy = PublishedGcImage::from_legacy(&mut heap).unwrap();
    let frozen_blocks = legacy.young().blocks().to_vec();
    let frozen_pending = legacy.young().pending_index();

    for i in 0..=CONS_BLOCK_SIZE {
        let _ = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL);
    }
    heap.close_alloc_regions();
    let shell_blocks = heap
        .current_mutator_satb()
        .young
        .resident()
        .unwrap()
        .blocks()
        .to_vec();
    let shell_pending = heap
        .current_mutator_satb()
        .young
        .resident()
        .unwrap()
        .pending_index();

    let peer_id = YoungRegistrationId::issue_parallel(heap.heap_identity()).unwrap();
    let mut peer_slot = YoungLogSlot::new_parallel(peer_id);
    let peer_block = ConsBlock::new();
    let peer_base = ConsBlockBase::of(&peer_block);
    heap.cons_allocator.register(&peer_block);
    heap.cons_blocks.push(peer_block);
    heap.cons_allocator
        .note_carve(peer_base, peer_slot.resident_mut().unwrap())
        .unwrap();
    let peer_pending = peer_slot.resident().unwrap().pending_index();

    let failure = restore_legacy_for_collection_for_test(legacy, &mut heap, |_moved_id| None)
        .expect_err("unresolvable swapped peer must refuse before any mutation");
    assert!(
        matches!(
            failure.error,
            ImageError::Young(YoungRegistryError::UnknownRegistration)
        ),
        "expected UnknownRegistration, got {:?}",
        failure.error
    );
    assert_eq!(
        failure.image.young().blocks(),
        frozen_blocks.as_slice(),
        "refusal must leave frozen young blocks untouched"
    );
    assert_eq!(
        failure.image.young().pending_index(),
        frozen_pending,
        "refusal must leave frozen pending index untouched"
    );
    let shell = heap.current_mutator_satb().young.resident().unwrap();
    assert_eq!(
        shell.blocks(),
        shell_blocks.as_slice(),
        "refusal must leave resident shell young blocks untouched"
    );
    assert_eq!(
        shell.pending_index(),
        shell_pending,
        "refusal must leave resident shell pending index untouched"
    );
    assert_eq!(
        peer_slot.resident().unwrap().pending_index(),
        peer_pending,
        "refusal must leave peer pending index untouched"
    );

    // Providing the peer afterward must still succeed cleanly with full registry invariants.
    let mut peer_opt = Some(peer_slot.resident_mut().unwrap());
    restore_legacy_for_collection_for_test(failure.image, &mut heap, |moved_id| {
        (moved_id == peer_id).then(|| peer_opt.take()).flatten()
    })
    .expect("retry with valid peer succeeds after fail-closed refusal");
    let restored = heap.current_mutator_satb().young.resident().unwrap();
    heap.cons_allocator
        .verify_young_registry_with_extra(restored, &[peer_slot.resident().unwrap()]);
}
