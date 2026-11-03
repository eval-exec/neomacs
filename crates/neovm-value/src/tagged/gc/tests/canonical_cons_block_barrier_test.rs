//! Canonical no-old-block certificates expire before collector age changes.

use super::*;
use crate::tagged::gc::alloc_region::RegionColor;
use crate::tagged::gc::knobs::set_chunk_map_for_test;
use crate::tagged::mutate::{set_cons_car, set_cons_cdr};
use crate::tagged::value::TAG_MASK;

fn installed_heap(chunk_map: bool) -> Box<TaggedHeap> {
    set_chunk_map_for_test(Some(chunk_map));
    let mut heap = Box::new(TaggedHeap::new());
    set_chunk_map_for_test(None);
    heap.set_generational_enabled(true);
    heap.gc_threshold = 1_000_000;
    set_tagged_heap(&mut heap);
    assert_eq!(heap.chunk_map.is_some(), chunk_map);
    heap
}

fn address(value: TaggedValue) -> usize {
    value.bits() & !TAG_MASK
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection().unwrap();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    heap.gc_threshold = 1_000_000;
}

fn old_owner(heap: &mut TaggedHeap) -> TaggedValue {
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    for _ in 0..3 {
        if heap.cons_age_is_old_for_test(owner) {
            break;
        }
        minor(heap, &[owner]);
    }
    assert!(heap.cons_age_is_old_for_test(owner));
    owner
}

#[test]
fn canonical_block_certificate_preserves_book_and_jit_layout() {
    assert_eq!(size_of::<RegionBook>(), 40);
    assert_eq!(std::mem::align_of::<RegionBook>(), 8);
    assert_eq!(std::mem::offset_of!(RegionBook, cons), 0);
    assert_eq!(std::mem::offset_of!(RegionBook, float), 16);
    assert_eq!(std::mem::offset_of!(RegionBook, cons_block_and_color), 32);
    assert_eq!(size_of::<JitHeapState>(), 48);
}

#[test]
fn canonical_block_certificate_keeps_all_address_bits_and_inactive_sentinel() {
    let mut book = RegionBook::new();
    let mask = CONS_BLOCK_ALIGN - 1;
    let mut bases = vec![0, CONS_BLOCK_ALIGN, usize::MAX & !mask];
    for shift in 16..usize::BITS {
        bases.push(1usize << shift);
    }
    for base in bases {
        for color in [RegionColor::White, RegionColor::Black] {
            book.set_cons_block_and_color(Some(base), color);
            for offset in 0..CONS_BLOCK_ALIGN {
                assert!(book.no_old_cons_block_contains(base | offset));
            }
            assert!(!book.no_old_cons_block_contains(base ^ CONS_BLOCK_ALIGN));
            assert_eq!(book.cons_color(), color);
            book.clear_cons_block();
            assert_eq!(book.cons_color(), color);
            for addr in [0, 1, 2, 3, base, base | mask, usize::MAX] {
                assert!(!book.no_old_cons_block_contains(addr));
            }
            book.set_cons_block_and_color(None, color);
            assert!(!book.no_old_cons_block_contains(base));
        }
    }
}

#[test]
fn canonical_block_certificate_covers_prior_grants_and_expires_on_close() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        let cells: Vec<_> = (0..3 * CONS_REGION_MAX_CELLS)
            .map(|_| heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL))
            .collect();
        let first = cells[0];
        let last = *cells.last().unwrap();
        assert_eq!(heap.cons_region_for_test().0, heap.cons_region_for_test().1);
        assert!(heap.region_book.no_old_cons_block_contains(address(first)));
        assert!(heap.region_book.no_old_cons_block_contains(address(last)));
        heap.close_alloc_regions();
        assert!(!heap.region_book.no_old_cons_block_contains(address(first)));
        heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        assert!(heap.region_book.no_old_cons_block_contains(address(first)));
        assert!(set_cons_cdr(first, last));
        assert!(!heap.owner_card_is_dirty_for_test(first));
        minor(&mut heap, &[first]);
        assert!(!heap.region_book.no_old_cons_block_contains(address(first)));
        assert_eq!(first.cons_cdr(), last);
        assert_eq!(last.cons_car(), TaggedValue::NIL);
    }
}

#[test]
fn canonical_mixed_block_uses_cell_age_and_remembers_without_rewriting() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        let owner = old_owner(&mut heap);
        let child = heap.alloc_cons(TaggedValue::fixnum(12345), TaggedValue::NIL);
        assert_eq!(
            address(owner) / CONS_BLOCK_ALIGN,
            address(child) / CONS_BLOCK_ALIGN
        );
        assert!(!heap.cons_age_is_old_for_test(child));
        assert!(!heap.region_book.no_old_cons_block_contains(address(owner)));
        assert!(!heap.region_book.no_old_cons_block_contains(address(child)));
        assert!(set_cons_cdr(child, TaggedValue::T));
        assert!(!heap.owner_card_is_dirty_for_test(child));
        assert!(set_cons_car(owner, child));
        assert!(heap.owner_card_is_dirty_for_test(owner));
        // Only the old owner is rooted. Two collections without another store
        // also exercise a child that survives once and remains young.
        for _ in 0..2 {
            minor(&mut heap, &[owner]);
            assert_eq!(owner.cons_car(), child);
            assert_eq!(child.cons_car(), TaggedValue::fixnum(12345));
            assert_eq!(child.cons_cdr(), TaggedValue::T);
        }
    }
}

#[test]
fn canonical_block_rollover_keeps_noncurrent_young_owner_fallback() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        let first = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        let mut last = first;
        while heap.cons_blocks.len() == 1 {
            last = heap.alloc_cons(TaggedValue::fixnum(67), TaggedValue::NIL);
        }
        assert!(!heap.region_book.no_old_cons_block_contains(address(first)));
        assert!(heap.region_book.no_old_cons_block_contains(address(last)));
        assert!(set_cons_cdr(first, last));
        assert!(!heap.owner_card_is_dirty_for_test(first));
        minor(&mut heap, &[first]);
        assert_eq!(first.cons_cdr(), last);
        assert_eq!(last.cons_car(), TaggedValue::fixnum(67));
    }
}

#[test]
fn canonical_valid_certificate_does_not_skip_satb_preimages() {
    for chunk_map in [false, true] {
        for immediate in [TaggedValue::fixnum(71), TaggedValue::char('λ')] {
            let mut heap = installed_heap(chunk_map);
            let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            heap.set_concurrent_active_for_test(true);
            let owner = heap.alloc_cons(child, TaggedValue::NIL);
            assert!(heap.region_book.no_old_cons_block_contains(address(owner)));
            assert!(set_cons_car(owner, immediate));
            assert!(heap.take_satb_shared_for_test().contains(&child));
            heap.set_concurrent_active_for_test(false);
        }
    }
}

#[test]
fn canonical_generation_zero_never_publishes_certificate() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        heap.set_generational_enabled(false);
        let owner = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
        assert!(!heap.region_book.no_old_cons_block_contains(address(owner)));
        assert!(set_cons_car(owner, TaggedValue::fixnum(4)));
        assert_eq!(owner.cons_car(), TaggedValue::fixnum(4));
    }
}

fn counts(heap: &TaggedHeap) -> (u64, usize, usize, usize) {
    (
        heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()],
        heap.allocated_count(),
        heap.bytes_since_gc_exact(),
        heap.cons_live_count_exact(),
    )
}

fn marked(heap: &TaggedHeap, addr: usize) -> bool {
    let ptr = addr as *const ConsCell;
    let block = heap.cons_block_index_by_base[&ConsBlock::block_base_for_ptr(ptr)];
    heap.cons_blocks[block].is_marked_ptr(ptr)
}

#[test]
fn canonical_block_black_partial_close_keeps_color_and_unmarks_only_refunded_tail() {
    for chunk_map in [false, true] {
        set_chunk_map_for_test(Some(chunk_map));
        let mut heap = Box::new(TaggedHeap::new());
        set_chunk_map_for_test(None);
        heap.set_generational_enabled(true);
        heap.gc_threshold = 1_000_000;
        set_tagged_heap(&mut heap);
        assert_eq!(heap.chunk_map.is_some(), chunk_map);
        heap.set_sweep_in_progress_for_test(true);
        let before = counts(&heap);
        let refills_before = heap.region_stats.cons_refills;
        let n = 2 * CONS_REGION_MAX_CELLS + 1;
        let handed: Vec<_> = (0..n)
            .map(|i| heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL))
            .collect();
        let first = handed[0].bits() & !TAG_MASK;
        assert!(heap.region_book.no_old_cons_block_contains(first));
        assert_eq!(heap.region_book.cons_color(), RegionColor::Black);
        assert_eq!(heap.region_stats.cons_refills - refills_before, 3);
        assert!(handed.iter().all(|v| marked(&heap, v.bits() & !TAG_MASK)));
        let (cur, lim) = heap.cons_region_for_test();
        assert!(cur < lim);
        let tail: Vec<_> = (cur..lim).step_by(size_of::<ConsCell>()).collect();
        assert_eq!(tail.len(), CONS_REGION_MAX_CELLS - 1);
        assert!(tail.iter().all(|&a| marked(&heap, a)));
        let exact = (
            before.0 + n as u64,
            before.1 + n,
            before.2 + n * size_of::<ConsCell>(),
            before.3 + n,
        );
        assert_eq!(counts(&heap), exact);

        // Close while the phase is STILL black. The retained color must drive
        // unmarking even after the no-old-block certificate has been cleared.
        heap.close_alloc_regions();
        assert!(heap.sweep_in_progress);
        assert_eq!(heap.cons_region_for_test(), (0, 0));
        assert_eq!(heap.region_book.cons, ConsRegionSource::Closed);
        assert!(!heap.region_book.no_old_cons_block_contains(first));
        assert_eq!(heap.region_book.cons_color(), RegionColor::Black);
        assert_eq!(heap.region_book.cons_block_and_color, 3);
        assert_eq!(counts(&heap), exact);
        assert_eq!(heap.cons_blocks[0].next_index as usize, n);
        assert!(handed.iter().all(|v| marked(&heap, v.bits() & !TAG_MASK)));
        assert!(tail.iter().all(|&a| !marked(&heap, a)));

        heap.set_sweep_in_progress_for_test(false);
        let white = heap.alloc_cons(TaggedValue::fixnum(9001), TaggedValue::NIL);
        let white_addr = white.bits() & !TAG_MASK;
        assert_eq!(white_addr, cur);
        assert!(heap.region_book.no_old_cons_block_contains(white_addr));
        assert_eq!(heap.region_book.cons_color(), RegionColor::White);
        assert_eq!(heap.region_book.cons_block_and_color & 1, 0);
        assert!(!marked(&heap, white_addr));
        assert_eq!(white.cons_car(), TaggedValue::fixnum(9001));
        assert_eq!(
            handed[n - 1].cons_car(),
            TaggedValue::fixnum((n - 1) as i64)
        );
    }
}
