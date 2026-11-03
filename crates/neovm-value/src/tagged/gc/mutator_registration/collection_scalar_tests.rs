#![forbid(unsafe_code)]
//! Redistribute existing real credits, then prove sweeping and serial merge do
//! not resurrect the pre-sweep peer balance or duplicate cumulative histories.
use super::super::TaggedHeap;
use super::super::alloc_region::RegionStats;
use super::super::pacing::GenerationPacingCounters;
use super::image::{CollectionHistory, OwnerGcState, PublishedGcImage, ResponsePacing};
use super::*;
use crate::tagged::value::TaggedValue;

#[test]
fn stopped_scalar_fold_preserves_history_and_rebases_real_live_credits_once() {
    let mut heap = TaggedHeap::new();
    let mut live = TaggedValue::NIL;
    for i in 0..32 {
        let value = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::NIL);
        if i == 0 {
            live = value;
        }
        if i == 15 {
            // A real prior reset gives the fold both banked and pending bytes.
            heap.reset_bytes_since_gc();
        }
    }
    heap.close_alloc_regions();
    let before_count = heap.current_mutator_gc().allocated_count;
    let before_memory = heap.memory_use_counts_snapshot();
    let before_bytes = heap.total_allocated_bytes();
    assert_eq!(before_count, 32);
    assert!(heap.current_mutator_gc().bytes_banked_at_resets > 0);
    assert!(heap.current_mutator_gc().bytes_since_gc > 0);
    let mut owner = OwnerGcState::new(
        heap.heap_identity(),
        YoungRegistrationId::issue_parallel(heap.heap_identity()).unwrap(),
        0,
    )
    .unwrap();
    // Split existing physical credits, rather than inventing live objects.
    let h = heap.current_mutator_gc_mut();
    owner.hot.allocated_count = h.allocated_count / 2;
    h.allocated_count -= owner.hot.allocated_count;
    for (local, peer) in h
        .memory_use_counts
        .iter_mut()
        .zip(&mut owner.hot.memory_use_counts)
    {
        *peer = *local / 2;
        *local -= *peer;
    }
    owner.hot.bytes_since_gc = h.bytes_since_gc / 2;
    h.bytes_since_gc -= owner.hot.bytes_since_gc;
    owner.hot.bytes_banked_at_resets = h.bytes_banked_at_resets / 2;
    h.bytes_banked_at_resets -= owner.hot.bytes_banked_at_resets;
    let local_pacing = GenerationPacingCounters {
        promoted_since_major: 3,
        minors_since_major: 2,
        stress_cycles_since_major: 1,
    };
    let peer_pacing = GenerationPacingCounters {
        promoted_since_major: 7,
        minors_since_major: 5,
        stress_cycles_since_major: 4,
    };
    h.pacing = local_pacing;
    owner.hot.pacing = peer_pacing;
    owner.region_stats = std::mem::take(&mut heap.region_stats);
    let mut peer = owner.publish().unwrap();
    let own_memory = heap.current_mutator_gc().memory_use_counts;
    let own_bytes = heap.total_allocated_bytes();
    let history = CollectionHistory::save(&heap);
    peer.add_collection_scalars(&mut heap);
    assert_eq!(heap.current_mutator_gc().allocated_count, before_count);
    assert_eq!(heap.memory_use_counts_snapshot(), before_memory);
    assert_eq!(heap.total_allocated_bytes(), before_bytes);
    assert_eq!(heap.current_mutator_gc().pacing.promoted_since_major, 10);
    let bytes = heap.live_bytes();
    let started = std::time::Instant::now();
    heap.begin_stw_collection();
    heap.seed_root(live);
    heap.mark_all();
    heap.finalize_collection(0, bytes, started);
    let after_count = heap.current_mutator_gc().allocated_count;
    assert_eq!(after_count, 1, "dead physical objects must debit the union");
    assert_eq!(heap.total_allocated_bytes(), before_bytes);
    let gen1 = heap.generational_enabled();
    history.restore(&mut heap, gen1);
    peer.finish_collection_response(if gen1 {
        ResponsePacing::Reset
    } else {
        ResponsePacing::Preserve
    });
    assert_eq!(heap.current_mutator_gc().memory_use_counts, own_memory);
    assert_eq!(heap.total_allocated_bytes(), own_bytes);
    assert_eq!(heap.current_mutator_gc().allocated_count, after_count);
    assert_eq!(peer.summary().allocated_count, 0);
    assert_eq!(peer.summary().bytes_since_gc, 0);
    assert_eq!(
        heap.current_mutator_gc().pacing,
        if gen1 {
            GenerationPacingCounters::default()
        } else {
            local_pacing
        }
    );
    assert_eq!(
        peer.pacing(),
        if gen1 {
            GenerationPacingCounters::default()
        } else {
            peer_pacing
        }
    );
    // Use the actual era-finish fold to check a second accounting boundary.
    let mut serial = PublishedGcImage::from_legacy(&mut heap).unwrap();
    serial.merge_non_young(peer).unwrap();
    serial.restore_legacy(&mut heap).unwrap();
    assert_eq!(heap.current_mutator_gc().allocated_count, after_count);
    assert_eq!(heap.memory_use_counts_snapshot(), before_memory);
    assert_eq!(heap.total_allocated_bytes(), before_bytes);
    assert_eq!(heap.current_mutator_gc().bytes_since_gc, 0);
    assert_eq!(
        heap.region_stats.cons_refills,
        RegionStats::default().cons_refills
    );
    // A later stopped cycle with no new allocations must not bank twice.
    let started = std::time::Instant::now();
    let bytes = heap.live_bytes();
    heap.begin_stw_collection();
    heap.seed_root(live);
    heap.mark_all();
    heap.finalize_collection(0, bytes, started);
    assert_eq!(heap.total_allocated_bytes(), before_bytes);
    assert_eq!(heap.current_mutator_gc().allocated_count, after_count);
}
