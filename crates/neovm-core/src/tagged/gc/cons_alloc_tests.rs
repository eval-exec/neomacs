//! Cons allocation: the three cell sources (free list, block bump, fresh
//! block), the counters they advance, the allocate-black rule while a sweep
//! or a concurrent mark is in flight, and `list_from_slice` (GNU `Flist`).

use super::*;

fn car_cdr(value: TaggedValue) -> (TaggedValue, TaggedValue) {
    let ptr = value.as_cons_ptr().expect("a cons");
    // SAFETY: the pointer comes from a cons this test just allocated.
    unsafe { ((*ptr).load_car(), (*ptr).cdr()) }
}

/// Is this cons black, asked of the block that owns it.
fn is_black(heap: &TaggedHeap, value: TaggedValue) -> bool {
    let ptr = value.as_cons_ptr().expect("a cons");
    let base = ConsBlock::block_base_for_ptr(ptr);
    let index = heap.cons_block_index_by_base[&base];
    heap.cons_blocks[index].is_marked_ptr(ptr)
}

/// Walk a proper list into a Vec of its cars.
fn list_cars(mut value: TaggedValue) -> Vec<TaggedValue> {
    let mut out = Vec::new();
    while !value.is_nil() {
        let (car, cdr) = car_cdr(value);
        out.push(car);
        value = cdr;
    }
    out
}

#[test]
fn cons_allocation_advances_every_counter_by_one_cell() {
    let mut heap = TaggedHeap::new();
    let before_counts = heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()];
    let before_bytes = heap.bytes_since_gc();
    let before_live = heap.live_bytes();
    let before_allocated = heap.allocated_count;
    let before_cons_live = heap.cons_live_count;

    let cell = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);

    assert_eq!(car_cdr(cell), (TaggedValue::T, TaggedValue::NIL));
    assert_eq!(
        heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()],
        before_counts + 1
    );
    assert_eq!(heap.bytes_since_gc(), before_bytes + size_of::<ConsCell>());
    assert_eq!(heap.live_bytes(), before_live + size_of::<ConsCell>());
    assert_eq!(heap.allocated_count, before_allocated + 1);
    assert_eq!(heap.cons_live_count, before_cons_live + 1);
}

#[test]
fn cons_allocation_rolls_over_into_a_fresh_block() {
    let mut heap = TaggedHeap::new();
    // The first cons opens the first block; a full block's worth after it
    // must open exactly one more (GNU's `cons_block_index` rollover).
    let mut cells = vec![heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL)];
    let blocks_before = heap.cons_blocks.len();
    for _ in 0..CONS_BLOCK_SIZE {
        cells.push(heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL));
    }

    assert_eq!(heap.cons_blocks.len(), blocks_before + 1);
    for block in &heap.cons_blocks {
        assert!(
            heap.cons_block_index_by_base
                .contains_key(&block.base_addr()),
            "every block base must be registered for mark_cons"
        );
    }
    for cell in &cells {
        let ptr = cell.as_cons_ptr().expect("a cons");
        assert!(ConsBlock::ptr_is_cell_aligned(ptr));
    }
    let addrs: std::collections::HashSet<usize> = cells
        .iter()
        .map(|c| c.as_cons_ptr().unwrap() as usize)
        .collect();
    assert_eq!(addrs.len(), cells.len(), "every cell is distinct");
}

/// GNU asserts a fresh cons is unmarked (`eassert (!XCONS_MARKED_P)`), except
/// that neomacs allocates black while a deferred sweep or a concurrent mark
/// is in flight so the newborn survives that cycle.
#[test]
fn cons_allocation_is_black_only_while_sweeping_or_marking() {
    let mut heap = TaggedHeap::new();
    let quiet = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    assert!(!is_black(&heap, quiet), "a quiet-heap cons is white");

    heap.sweep_in_progress = true;
    let during_sweep = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    assert!(is_black(&heap, during_sweep));
    heap.sweep_in_progress = false;

    heap.concurrent_mark_running = true;
    let during_mark = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    assert!(is_black(&heap, during_mark));
    heap.concurrent_mark_running = false;

    let after = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    assert!(!is_black(&heap, after));
}

/// A reclaimed cell returns through the free list, with the counters still
/// counting the reuse as one allocation (GNU `Fcons`'s first arm).
#[test]
fn reclaimed_cells_come_back_through_the_free_list() {
    let mut heap = TaggedHeap::new();
    // Keep half the conses live so the block survives the sweep; the other
    // half becomes the free list.
    let mut kept = Vec::new();
    let mut doomed = Vec::new();
    for i in 0..16 {
        let cell = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        if i % 2 == 0 {
            kept.push(cell)
        } else {
            doomed.push(cell)
        }
    }
    let doomed_addrs: std::collections::HashSet<usize> = doomed
        .iter()
        .map(|c| c.as_cons_ptr().unwrap() as usize)
        .collect();

    heap.collect_exact(kept.iter().copied());
    assert!(
        !heap.cons_free_list.is_null(),
        "the collection must reclaim the unrooted conses"
    );

    let before_allocated = heap.allocated_count;
    let before_live = heap.cons_live_count;
    let reused = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    assert!(
        doomed_addrs.contains(&(reused.as_cons_ptr().unwrap() as usize)),
        "the free list must hand back a reclaimed cell"
    );
    assert_eq!(car_cdr(reused), (TaggedValue::T, TaggedValue::NIL));
    assert_eq!(heap.allocated_count, before_allocated + 1);
    assert_eq!(heap.cons_live_count, before_live + 1);
}

/// GNU `Flist`: cons from the end, so the cars keep their order.
#[test]
fn list_from_slice_builds_a_proper_list_in_order() {
    let mut heap = TaggedHeap::new();
    let a = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    let elements = [TaggedValue::T, a, TaggedValue::NIL];

    let before = heap.cons_live_count;
    let list = heap.list_from_slice(&elements);
    assert_eq!(heap.cons_live_count, before + elements.len());

    let cars = list_cars(list);
    assert_eq!(cars.len(), elements.len());
    for (got, want) in cars.iter().zip(elements.iter()) {
        assert_eq!(
            got.bits(),
            want.bits(),
            "cars keep their order and identity"
        );
    }

    let empty = heap.list_from_slice(&[]);
    assert!(empty.is_nil());
    assert_eq!(heap.cons_live_count, before + elements.len());
}
