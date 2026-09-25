//! Allocation regions (`alloc_region.rs`): where a region's cells come from,
//! how its unused tail goes back, how the counters stay exact through the
//! views, how allocate-black follows the region across every collector
//! phase, and that every collector entry closes it.

use super::*;

fn cell_addr(value: TaggedValue) -> usize {
    value.as_cons_ptr().expect("a cons") as usize
}

fn is_black(heap: &TaggedHeap, value: TaggedValue) -> bool {
    let ptr = value.as_cons_ptr().expect("a cons");
    let base = ConsBlock::block_base_for_ptr(ptr);
    let index = heap.cons_block_index_by_base[&base];
    heap.cons_blocks[index].is_marked_ptr(ptr)
}

/// Is the cell at `addr` marked in its block's bitmap?
fn addr_marked(heap: &TaggedHeap, addr: usize) -> bool {
    let ptr = addr as *const ConsCell;
    let base = ConsBlock::block_base_for_ptr(ptr);
    let index = heap.cons_block_index_by_base[&base];
    heap.cons_blocks[index].is_marked_ptr(ptr)
}

fn cons(heap: &mut TaggedHeap, n: i64) -> TaggedValue {
    heap.alloc_cons(TaggedValue::fixnum(n), TaggedValue::NIL)
}

fn free_list(heap: &TaggedHeap) -> Vec<usize> {
    let mut out = Vec::new();
    let mut cell = heap.cons_free_list;
    while !cell.is_null() {
        out.push(cell as usize);
        cell = unsafe { (*cell).free_next() };
    }
    out
}

fn counts(heap: &TaggedHeap) -> (u64, usize, usize, usize) {
    (
        heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()],
        heap.allocated_count(),
        heap.bytes_since_gc_exact(),
        heap.cons_live_count_exact(),
    )
}

/// A fresh heap's first region is a fresh block's head, then its bump tail:
/// consecutive cells, one region per `CONS_REGION_MAX_CELLS`.
#[test]
fn regions_bump_a_block_tail_in_order() {
    let mut heap = TaggedHeap::new();
    let first = cons(&mut heap, 0);
    let (cur, lim) = heap.cons_region_for_test();
    assert_eq!(cur, cell_addr(first) + size_of::<ConsCell>());
    assert_eq!(
        lim - cell_addr(first),
        CONS_REGION_MAX_CELLS * size_of::<ConsCell>()
    );
    assert_eq!(
        heap.region_book.cons,
        ConsRegionSource::BlockTail { block: 0 }
    );
    let mut prev = cell_addr(first);
    for i in 1..(3 * CONS_REGION_MAX_CELLS) as i64 {
        let next = cell_addr(cons(&mut heap, i));
        assert_eq!(next, prev + size_of::<ConsCell>(), "cell {i} follows");
        prev = next;
    }
    assert_eq!(heap.region_stats.cons_refills, 3);
    assert_eq!(heap.cons_blocks.len(), 1);
    assert_eq!(
        heap.cons_blocks[0].next_index as usize,
        3 * CONS_REGION_MAX_CELLS,
        "the third region was used up exactly"
    );
}

/// A block's end caps the region; the next comes from a fresh block.
#[test]
fn a_full_block_rolls_over_into_a_fresh_block() {
    let mut heap = TaggedHeap::new();
    for i in 0..CONS_BLOCK_SIZE as i64 {
        cons(&mut heap, i);
    }
    assert_eq!(heap.cons_blocks.len(), 1);
    assert_eq!(heap.cons_blocks[0].next_index as usize, CONS_BLOCK_SIZE);
    let (cur, lim) = heap.cons_region_for_test();
    assert_eq!(cur, lim, "the region ended with the block");
    let next = cons(&mut heap, -1);
    assert_eq!(heap.cons_blocks.len(), 2);
    assert_eq!(cell_addr(next), heap.cons_blocks[1].base_addr());
    assert_eq!(
        heap.region_book.cons,
        ConsRegionSource::BlockTail { block: 1 }
    );
}

/// Free-list regions: the head plus every cell after it at the next address,
/// stopping at the first gap; the rest of the list stays.
#[test]
fn a_free_list_region_is_one_adjacent_run() {
    let mut heap = TaggedHeap::new();
    // 64 cells; keep 0..10, 20, 40..64. Dead: 10..20 and 21..40.
    let cells: Vec<TaggedValue> = (0..64).map(|i| cons(&mut heap, i)).collect();
    let kept: Vec<TaggedValue> = cells
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < 10 || *i == 20 || *i >= 40)
        .map(|(_, c)| *c)
        .collect();
    heap.collect_exact(kept.iter().copied());
    assert!(
        !heap.alloc_regions_open(),
        "the collection closed the region"
    );
    let list = free_list(&heap);
    assert!(list.windows(2).all(|w| w[0] < w[1]), "the list ascends");
    assert_eq!(list[0], cell_addr(cells[10]), "lowest dead cell first");
    assert_eq!(list.len(), 10 + 19, "every dead cell, nothing else");

    let got = cons(&mut heap, 100);
    assert_eq!(cell_addr(got), cell_addr(cells[10]));
    assert_eq!(heap.region_book.cons, ConsRegionSource::FreeListRun);
    let (cur, lim) = heap.cons_region_for_test();
    assert_eq!(cur, cell_addr(cells[11]));
    assert_eq!(lim, cell_addr(cells[20]), "the run stops at the gap at 20");
    assert_eq!(heap.cons_free_list as usize, cell_addr(cells[21]));
    // Use the run up; the next region is the next run.
    for i in 11..20 {
        assert_eq!(cell_addr(cons(&mut heap, i)), cell_addr(cells[i as usize]));
    }
    let next = cons(&mut heap, 21);
    assert_eq!(cell_addr(next), cell_addr(cells[21]));
    let (_, lim) = heap.cons_region_for_test();
    assert_eq!(lim, cell_addr(cells[40]));
}

/// Closing gives back exactly the unused cells: a block tail rewinds its
/// cursor, a free-list run goes back on the list in ascending order with
/// every car `DEAD` again.
#[test]
fn closing_gives_the_unused_tail_back_where_it_came_from() {
    let mut heap = TaggedHeap::new();
    let a = cons(&mut heap, 1);
    let _b = cons(&mut heap, 2);
    heap.close_alloc_regions();
    assert!(!heap.alloc_regions_open());
    assert_eq!(heap.cons_region_for_test(), (0, 0));
    assert_eq!(heap.cons_blocks[0].next_index, 2, "the bump tail rewound");
    let c = cons(&mut heap, 3);
    assert_eq!(cell_addr(c), cell_addr(a) + 2 * size_of::<ConsCell>());

    // A free-list run of 5, one used, four back.
    let mut heap = TaggedHeap::new();
    let cells: Vec<TaggedValue> = (0..16).map(|i| cons(&mut heap, i)).collect();
    let kept: Vec<TaggedValue> = cells
        .iter()
        .enumerate()
        .filter(|(i, _)| !(3..8).contains(i))
        .map(|(_, c)| *c)
        .collect();
    heap.collect_exact(kept.iter().copied());
    let before = free_list(&heap);
    let got = cons(&mut heap, 99);
    assert_eq!(cell_addr(got), cell_addr(cells[3]));
    heap.close_alloc_regions();
    let after = free_list(&heap);
    assert_eq!(after, before[1..].to_vec(), "cells 4..8 back, in order");
    for &addr in &after {
        let car = unsafe { (*(addr as *const ConsCell)).car };
        assert_eq!(car, TaggedValue::DEAD, "a free cell's car is DEAD");
    }
}

/// Every counter view is exact after 1, 127, 128 and 129 allocations, with
/// the region open and after it closes; the charged `bytes_since_gc` is
/// never below the exact count.
#[test]
fn the_counter_views_are_exact_with_the_region_open_or_closed() {
    for n in [1usize, 127, 128, 129, 300] {
        let mut heap = TaggedHeap::new();
        let before = counts(&heap);
        for i in 0..n {
            cons(&mut heap, i as i64);
        }
        let want = (
            before.0 + n as u64,
            before.1 + n,
            before.2 + n * size_of::<ConsCell>(),
            before.3 + n,
        );
        assert_eq!(counts(&heap), want, "{n} conses, region open");
        assert!(heap.bytes_since_gc() >= heap.bytes_since_gc_exact());
        assert_eq!(
            heap.total_allocated_bytes(),
            (n * size_of::<ConsCell>()) as u64
        );
        heap.close_alloc_regions();
        assert_eq!(counts(&heap), want, "{n} conses, region closed");
        assert_eq!(heap.bytes_since_gc(), heap.bytes_since_gc_exact());
        assert_eq!(heap.allocated_count, heap.allocated_count());
    }
}

/// `region_budget` never lets the charged counter cross the threshold
/// before the exact one would: with a small threshold, `should_collect`
/// turns true on exactly the allocation that makes the exact count reach
/// it.
#[test]
fn a_region_never_brings_a_collection_forward() {
    for threshold in [1usize, 16, 100, 1000, 4096, 5000] {
        let mut heap = TaggedHeap::new();
        heap.set_gc_threshold(threshold);
        for i in 0..400 {
            cons(&mut heap, i);
            let exact_over = heap.bytes_since_gc_exact() >= threshold;
            assert_eq!(
                heap.should_collect(),
                exact_over,
                "threshold {threshold}, cons {i}: charged {} exact {}",
                heap.bytes_since_gc(),
                heap.bytes_since_gc_exact()
            );
            if exact_over {
                break;
            }
        }
    }
}

/// `garbage-collect-maybe`'s FACTOR test and the profiler read the exact
/// count, which a reset banks exactly.
#[test]
fn a_reset_banks_exactly_what_was_handed_out() {
    let mut heap = TaggedHeap::new();
    for i in 0..10 {
        cons(&mut heap, i);
    }
    assert_eq!(heap.bytes_since_gc_exact(), 10 * size_of::<ConsCell>());
    heap.reset_bytes_since_gc();
    assert!(!heap.alloc_regions_open(), "a reset closes the region");
    assert_eq!(heap.bytes_since_gc(), 0);
    assert_eq!(
        heap.total_allocated_bytes(),
        (10 * size_of::<ConsCell>()) as u64
    );
}

/// A region granted while the heap allocates black is pre-marked; when the
/// phase ends the region closes, its unused tail unmarked, and a region
/// granted after is white.
#[test]
fn black_follows_the_region_across_the_phase_flags() {
    let mut heap = TaggedHeap::new();
    let white = cons(&mut heap, 0);
    assert!(!is_black(&heap, white));

    heap.set_sweep_in_progress_for_test(true);
    let during_sweep = cons(&mut heap, 1);
    assert!(is_black(&heap, during_sweep));
    let (cur, lim) = heap.cons_region_for_test();
    assert!(lim > cur, "the region has an unused tail");
    let tail: Vec<usize> = (cur..lim).step_by(size_of::<ConsCell>()).collect();
    assert!(tail.iter().all(|&a| addr_marked(&heap, a)), "pre-marked");
    heap.set_sweep_in_progress_for_test(false);
    assert!(
        is_black(&heap, during_sweep),
        "a handed-out cell stays black"
    );
    assert!(
        tail.iter().all(|&a| !addr_marked(&heap, a)),
        "the unused tail is unmarked at close"
    );

    heap.set_concurrent_active_for_test(true);
    let during_mark = heap.list_from_slice(&[TaggedValue::fixnum(1); 1000]);
    let mut cursor = during_mark;
    while !cursor.is_nil() {
        assert!(
            is_black(&heap, cursor),
            "a list built during a mark is black"
        );
        cursor = unsafe { (*cursor.as_cons_ptr().unwrap()).cdr() };
    }
    heap.set_concurrent_active_for_test(false);

    let after = cons(&mut heap, 2);
    assert!(!is_black(&heap, after));
    // Bits at or above a block's cursor are never set.
    for block in &heap.cons_blocks {
        let used = block.next_index as usize;
        let marked_at_or_above = (used..CONS_BLOCK_SIZE).any(|i| {
            let cell = unsafe { block.cells_ptr().add(i) };
            block.is_marked_ptr(cell)
        });
        assert!(!marked_at_or_above, "a mark past the bump cursor");
    }
}

/// Every collector entry closes the open region before it looks at the
/// heap, and each phase flip closes before it flips.
#[test]
fn every_collector_entry_closes_the_region() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let root = cons(&mut heap, 0);
    let open = |heap: &mut TaggedHeap| {
        cons(heap, 1);
        assert!(heap.alloc_regions_open());
    };

    open(&mut heap);
    heap.collect_exact(std::iter::once(root));
    assert!(!heap.alloc_regions_open(), "collect_exact");

    open(&mut heap);
    heap.reset_bytes_since_gc();
    assert!(!heap.alloc_regions_open(), "reset_bytes_since_gc");

    // A concurrent cycle, a region open at each step.
    open(&mut heap);
    heap.concurrent_begin();
    assert!(!heap.alloc_regions_open(), "concurrent_begin (parity flip)");
    heap.seed_root(root);
    open(&mut heap);
    heap.launch_concurrent_mark();
    assert!(!heap.alloc_regions_open(), "launch_concurrent_mark");
    open(&mut heap);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    assert!(!heap.alloc_regions_open(), "join_concurrent_mark");
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    heap.incremental_drain_all();
    open(&mut heap);
    let bytes_before = heap.live_bytes();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    assert!(!heap.alloc_regions_open(), "incremental_finish");
    assert!(heap.sweep_in_progress());
    open(&mut heap);
    heap.incremental_sweep_slice(0);
    assert!(!heap.alloc_regions_open(), "incremental_sweep_slice");
    open(&mut heap);
    heap.finish_incremental_sweep_now();
    assert!(!heap.alloc_regions_open(), "finish_incremental_sweep");
    assert!(!heap.sweep_in_progress());
    assert_eq!(
        unsafe { (*root.as_cons_ptr().unwrap()).load_car() },
        TaggedValue::fixnum(0)
    );
}

/// Conses allocated across a concurrent mark, its termination and every
/// slice of the deferred sweep: each live cons survives with its contents,
/// no cell is handed out twice while live, and the sweep's live recount is
/// exact.
#[test]
fn allocation_across_a_concurrent_cycle_keeps_every_live_cons() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    // Enough blocks that the deferred sweep takes several slices.
    let mut root = TaggedValue::NIL;
    for i in 0..(3 * CONS_BLOCK_SIZE) as i64 {
        let garbage = cons(&mut heap, -i);
        let _ = garbage;
        if i % 3 == 0 {
            root = heap.alloc_cons(TaggedValue::fixnum(i), root);
        }
    }
    heap.collect_exact(std::iter::once(root)); // the bootstrap cycle
    let mut live: Vec<TaggedValue> = Vec::new();
    let keep = |heap: &mut TaggedHeap, live: &mut Vec<TaggedValue>, n: i64| {
        for i in 0..n {
            let c = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(-i));
            live.push(c);
            cons(heap, 7); // garbage between the live ones
        }
    };
    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    keep(&mut heap, &mut live, 500);
    while !heap.concurrent_mark_done() {
        keep(&mut heap, &mut live, 5);
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    for &c in &live {
        heap.seed_root(c);
    }
    heap.incremental_drain_all();
    let bytes_before = heap.live_bytes();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    while heap.sweep_in_progress() {
        keep(&mut heap, &mut live, 50);
        heap.incremental_sweep_slice(1);
    }
    let addrs: std::collections::HashSet<usize> = live.iter().map(|c| cell_addr(*c)).collect();
    assert_eq!(addrs.len(), live.len(), "a live cell was handed out twice");
    let mut per_n: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for &c in &live {
        let (car, cdr) = unsafe {
            let p = c.as_cons_ptr().unwrap();
            ((*p).load_car(), (*p).cdr())
        };
        let n = car.as_fixnum().expect("car intact");
        assert_eq!(cdr, TaggedValue::fixnum(-n), "cdr intact");
        *per_n.entry(n).or_default() += 1;
    }
    // The recount the sweep finished with matches the marks.
    heap.close_alloc_regions();
    let recount: usize = heap.cons_blocks.iter().map(ConsBlock::count_marked).sum();
    assert_eq!(heap.cons_live_count, recount);
    // Everything live survives a full collection too.
    let mut roots = live.clone();
    roots.push(root);
    heap.collect_exact(roots.into_iter());
    for &c in &live {
        let car = unsafe { (*c.as_cons_ptr().unwrap()).load_car() };
        assert!(car.is_fixnum(), "survived the stop-the-world cycle");
    }
}

/// The layout census closes the region, then counts it exactly.
#[test]
fn the_layout_census_is_exact_after_closing() {
    let mut heap = TaggedHeap::new();
    for i in 0..5 {
        cons(&mut heap, i);
    }
    heap.close_alloc_regions();
    let stats = heap.layout_stats();
    assert_eq!(stats.cons.live_slots, 5);
    assert_eq!(stats.cons.bumped_slots, 5);
}
