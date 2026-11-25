//! Preserved cons-reserve behavior, driven by real stopped World fixtures.

use super::*;
use crate::tagged::gc::AllStopped;

fn young_blocks(heap: &mut TaggedHeap) -> usize {
    heap._satb_layout_reserve
        .current_young_log_mut()
        .map(|log| log.blocks().len())
        .unwrap_or(0)
}

pub(in crate::tagged::gc) fn handed_cons(addr: usize) -> TaggedValue {
    use crate::tagged::header::{ConsCdrWord, ConsWord};
    let cell = addr as *mut ConsCell;
    // SAFETY: the caller just took this exclusive cell from a live reserve.
    // Both fields are initialized before publishing or collecting the value.
    unsafe {
        cell.write(ConsCell {
            car: ConsWord::new(TaggedValue::NIL),
            cdr_or_next: ConsCdrWord::new(TaggedValue::NIL),
        });
        TaggedValue::from_cons_ptr(cell)
    }
}

pub(in crate::tagged::gc) fn two_idle_stops_hold_no_reserve(
    stopped: &AllStopped<'_>,
    heap: &mut TaggedHeap,
) {
    let bytes = heap.bytes_since_gc();
    let mut owner = OwnerEraReserve::new();
    owner
        .grant_at_stop(stopped, heap, true, MeasuredInterval::zero())
        .expect("floor grant");
    let block = super::lock_blocks(owner.cons.as_ref().unwrap())[0].block_index;
    let blocks = heap.cons_blocks.len();
    assert_eq!(owner.cons_blocks_held(), 1);
    assert_eq!(heap.bytes_since_gc(), bytes);
    assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
    assert_eq!(heap.jit_float_window_for_test(), (0, 0));
    owner.return_unused_before_sweep(stopped, heap).unwrap();
    assert!(!owner.holds_reserve());
    assert_eq!(heap.bytes_since_gc(), bytes);
    owner
        .grant_at_stop(stopped, heap, true, MeasuredInterval::zero())
        .expect("second floor grant");
    assert_eq!(owner.cons_blocks_held(), 1);
    assert_eq!(
        super::lock_blocks(owner.cons.as_ref().unwrap())[0].block_index,
        block
    );
    assert_eq!(
        heap.cons_blocks.len(),
        blocks,
        "idle grant reuses its block"
    );
    owner.return_unused_before_sweep(stopped, heap).unwrap();
    assert!(!owner.holds_reserve());
    assert_eq!(heap.cons_blocks[block].next_index(), 0);
}

pub(in crate::tagged::gc) fn pacing_charges_carved_cells_and_subtracts_the_open_tail(
    stopped: &AllStopped<'_>,
    heap: &mut TaggedHeap,
) {
    let mut owner = OwnerEraReserve::new();
    let mut region = OwnerAllocRegion::new();
    let bytes = heap.bytes_since_gc();
    let young = young_blocks(heap);
    owner
        .grant_at_stop(stopped, heap, true, MeasuredInterval::zero())
        .unwrap();
    let block = super::lock_blocks(owner.cons.as_ref().unwrap())[0].block_index;
    assert_eq!(heap.bytes_since_gc(), bytes, "reservation is not charged");
    assert_eq!(young_blocks(heap), young + 1);
    owner.carve_cons(heap, &mut region).unwrap();
    assert_eq!(young_blocks(heap), young + 1);
    let cell = size_of::<ConsCell>();
    assert_eq!(heap.bytes_since_gc() - bytes, CONS_REGION_MAX_CELLS * cell);
    assert_eq!(heap.bytes_since_gc() - owner.open_cons_tail_bytes(), bytes);
    for _ in 0..100 {
        handed_cons(region.take_cons_cell().expect("cell"));
    }
    owner.note_cons_taken(100);
    assert_eq!(
        heap.bytes_since_gc() - owner.open_cons_tail_bytes() - bytes,
        100 * cell
    );
    owner.close_for_stop(heap, &mut region).unwrap();
    assert_eq!(heap.bytes_since_gc() - bytes, 100 * cell);
    assert!(region.is_closed());
    owner.return_unused_before_sweep(stopped, heap).unwrap();
    assert!(!owner.holds_reserve());
    assert_eq!(heap.cons_blocks[block].next_index(), 100);
    assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
}

pub(in crate::tagged::gc) fn the_cap_is_a_reserve_miss(
    stopped: &AllStopped<'_>,
    heap: &mut TaggedHeap,
) {
    let mut owner = OwnerEraReserve::new();
    assert_eq!(uncapped_units(100_000, CONS_BLOCK_SIZE as u64), 53);
    assert!(53 <= OWNER_RESERVE_CAP);
    assert_eq!(
        reserve_block_misses(1_000_000, CONS_BLOCK_SIZE as u64, 1),
        263
    );
    assert_eq!(reserve_block_misses(100_000, CONS_BLOCK_SIZE as u64, 53), 0);
    let disclosure = disclose_refill_passes(26, 0);
    assert_eq!(disclosure.first_pass_misses, 26);
    assert_eq!(disclosure.counted_pass_misses, 0);
    assert_ne!(
        disclosure.counted_pass_misses,
        disclosure.first_pass_misses + disclosure.counted_pass_misses
    );
    let interval = MeasuredInterval {
        conses: (OWNER_RESERVE_CAP as u64 + 1) * CONS_BLOCK_SIZE as u64,
    };
    let blocks = heap.cons_blocks.len();
    assert_eq!(
        owner.grant_at_stop(stopped, heap, false, interval),
        Err(ReserveError::RegionOpen)
    );
    assert_eq!(heap.cons_blocks.len(), blocks);
    let outcome = owner.grant_at_stop(stopped, heap, true, interval).unwrap();
    assert!(outcome.reserve_miss);
    assert_eq!(outcome.cons_blocks, OWNER_RESERVE_CAP);
    assert_eq!(owner.cons_blocks_held(), OWNER_RESERVE_CAP);
    owner.return_unused_before_sweep(stopped, heap).unwrap();
    assert!(!owner.holds_reserve());
}

pub(in crate::tagged::gc) fn two_owners_carve_disjoint_runs_without_moving_the_heap_window(
    stopped: &AllStopped<'_>,
    heap: &mut TaggedHeap,
) {
    let mut a = OwnerEraReserve::new();
    let mut b = OwnerEraReserve::new();
    let mut region_a = OwnerAllocRegion::new();
    let mut region_b = OwnerAllocRegion::new();
    a.grant_at_stop(stopped, heap, true, MeasuredInterval::zero())
        .unwrap();
    b.grant_at_stop(stopped, heap, true, MeasuredInterval::zero())
        .unwrap();
    a.carve_cons(heap, &mut region_a).unwrap();
    b.carve_cons(heap, &mut region_b).unwrap();
    let cell_a = region_a.take_cons_cell().unwrap();
    let cell_b = region_b.take_cons_cell().unwrap();
    handed_cons(cell_a);
    handed_cons(cell_b);
    assert!(cell_a.abs_diff(cell_b) >= CONS_BLOCK_BYTES);
    assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
    assert_eq!(heap.jit_float_window_for_test(), (0, 0));
    let blocks = heap.cons_blocks.len();
    loop {
        while let Some(cell) = region_a.take_cons_cell() {
            handed_cons(cell);
        }
        a.close_for_stop(heap, &mut region_a).unwrap();
        match a.carve_cons(heap, &mut region_a) {
            Ok(()) => {}
            Err(ReserveError::ReserveMiss) => break,
            Err(error) => panic!("carve: {error:?}"),
        }
    }
    assert_eq!(heap.cons_blocks.len(), blocks);
    assert_eq!(
        a.carve_cons(heap, &mut region_a),
        Err(ReserveError::ReserveMiss)
    );
    b.close_for_stop(heap, &mut region_b).unwrap();
    a.return_unused_before_sweep(stopped, heap).unwrap();
    b.return_unused_before_sweep(stopped, heap).unwrap();
}

pub(in crate::tagged::gc) fn a_moved_block_tail_is_not_rewound(
    stopped: &AllStopped<'_>,
    heap: &mut TaggedHeap,
) {
    let mut owner = OwnerEraReserve::new();
    owner
        .grant_at_stop(stopped, heap, true, MeasuredInterval::zero())
        .unwrap();
    let block = super::lock_blocks(owner.cons.as_ref().unwrap())[0].block_index;
    // Model an extended tail, preserving the original release refusal check.
    let _ = heap.cons_blocks[block].reserve_tail(CONS_BLOCK_SIZE);
    super::lock_blocks(owner.cons.as_ref().unwrap())[0].limit = CONS_BLOCK_SIZE - 1;
    owner.return_unused_before_sweep(stopped, heap).unwrap();
    assert_eq!(heap.cons_blocks[block].next_index(), CONS_BLOCK_SIZE);
}
