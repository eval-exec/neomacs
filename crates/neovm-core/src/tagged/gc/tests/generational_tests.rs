//! Generational heap invariants, built up with each stage of P3.1 G2.

use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::heap_types::LispString;

struct ScratchRoots(usize);
impl ScratchRoots {
    fn new() -> Self {
        Self(save_scratch_gc_roots())
    }
    fn keep(&self, value: TaggedValue) {
        push_scratch_gc_root(value);
    }
}
impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn heap_with_generations(on: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = on;
    heap.publish_barrier_window();
    heap
}

fn header(value: TaggedValue) -> &'static mut GcHeader {
    unsafe { &mut *(TaggedHeap::value_heap_addr(value).unwrap() as *mut GcHeader) }
}

#[test]
fn cons_trailer_layout_and_zeroed_generation_bitmaps() {
    assert_eq!(CONS_BLOCK_SIZE, 4001);
    assert_eq!(CONS_MARK_WORDS, 63);
    let mut block = ConsBlock::new();
    assert_eq!(block.base_addr() % CONS_BLOCK_BYTES, 0);
    let (first, count) = block.reserve_tail(CONS_BLOCK_SIZE);
    assert_eq!((first, count), (0, 4001));
    assert_eq!(block.reserve_tail(1).1, 0);
    for i in 0..CONS_BLOCK_SIZE {
        assert!(ConsBlock::ptr_is_cell_aligned(unsafe {
            block.cells_ptr().add(i)
        }));
    }
    assert!(!ConsBlock::ptr_is_cell_aligned(unsafe {
        block.cells_ptr().add(CONS_BLOCK_SIZE)
    }));
    block.mark_cell_offset((CONS_BLOCK_SIZE - 1) * size_of::<ConsCell>());
    assert_eq!(block.count_marked(), 1);
    // The mark bitmap ends before the two zeroed, unused generation maps.
    for i in 0..CONS_MARK_WORDS {
        assert_eq!(block.trailer().old_word(i), 0);
        assert_eq!(block.trailer().unlogged_word(i), 0);
    }
    block.clear_marks();
    assert_eq!(block.count_marked(), 0);
}

#[test]
fn generational_owned_owner_logging_and_immediate_filter() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_vector(vec![TaggedValue::NIL]);
    roots.keep(owner);
    header(owner).tenured = true;
    for immediate in [
        TaggedValue::fixnum(-1),
        TaggedValue::fixnum(17),
        TaggedValue::fixnum(65),
    ] {
        assert!(crate::tagged::mutate::set_vector_slot(owner, 0, immediate));
        assert!(heap.current_mutator_gc().remset.is_empty());
    }
    let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    for _ in 0..3 {
        assert!(crate::tagged::mutate::set_vector_slot(owner, 0, child));
    }
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
    assert!(header(owner).is_remembered());
    assert!(!heap.mapped_remembered.contains(&owner.bits()));
    heap.debug_assert_remembered_membership(owner);
}

#[test]
fn generational_cons_claim_and_logged_lifecycle() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(owner);
    let (trailer, index) = heap.old_cons_trailer(owner).unwrap();
    trailer.set_old(index);
    trailer.set_unlogged(index);
    assert!(crate::tagged::mutate::set_cons_car(
        owner,
        TaggedValue::fixnum(8)
    ));
    assert!(heap.current_mutator_gc().remset.is_empty());
    let child = heap.alloc_string(LispString::from_utf8("child"));
    assert!(crate::tagged::mutate::set_cons_car(owner, child));
    assert!(crate::tagged::mutate::set_cons_cdr(owner, child));
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
    assert!(!heap.old_cons_trailer(owner).unwrap().0.is_unlogged(index));
    heap.begin_stw_collection();
    assert!(heap.current_mutator_gc().remset.is_empty());
    assert_eq!(heap.generational.r_seed, [owner]);
    heap.seed_root(owner);
    heap.complete_collection();
    assert!(heap.generational.r_seed.is_empty());
    assert!(heap.old_cons_trailer(owner).unwrap().0.is_unlogged(index));
    assert!(heap.owns_string_object(child.as_string_ptr().unwrap().cast()));
    assert!(crate::tagged::mutate::set_cons_car(owner, child));
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
}

#[test]
fn generational_mapped_and_permanent_logs_are_per_cycle() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let image = fake_image::FakeImage::leak(false);
    let mapped = image.register_vector(&mut heap);
    roots.keep(mapped);
    let permanent = heap.alloc_vector(vec![TaggedValue::NIL]);
    roots.keep(permanent);
    heap.collect_exact(std::iter::once(permanent));
    assert!(header(permanent).generation.permanent());
    let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    for _ in 0..2 {
        assert!(crate::tagged::mutate::set_vector_slot(mapped, 0, child));
        assert!(crate::tagged::mutate::set_vector_slot(permanent, 0, child));
    }
    assert_eq!(heap.current_mutator_gc().remset.len(), 2);
    heap.current_mutator_gc_mut()
        .remembered_cache
        .fill(permanent.bits());
    heap.begin_stw_collection();
    assert!(
        heap.current_mutator_gc()
            .remembered_cache
            .iter()
            .all(|&bits| bits == 0)
    );
    assert_eq!(heap.generational.r_seed.len(), 2);
    heap.complete_collection();
    assert!(!header(permanent).is_remembered());
    assert!(heap.current_mutator_gc().r_mapped_seen.is_empty());
    assert!(heap.generational.r_seed.is_empty());
    assert!(crate::tagged::mutate::set_vector_slot(permanent, 0, child));
    assert_eq!(heap.current_mutator_gc().remset, [permanent]);
}

#[test]
fn generational_disabled_never_logs_r() {
    let mut heap = heap_with_generations(false);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let image = fake_image::FakeImage::leak(false);
    let mapped = image.register_vector(&mut heap);
    roots.keep(mapped);
    let permanent = heap.alloc_vector(vec![TaggedValue::NIL]);
    roots.keep(permanent);
    heap.collect_exact(std::iter::once(permanent));
    let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    crate::tagged::mutate::set_vector_slot(mapped, 0, child);
    crate::tagged::mutate::set_vector_slot(permanent, 0, child);
    assert!(heap.current_mutator_gc().remset.is_empty());
    assert!(heap.generational.r_seed.is_empty());
    assert!(heap.mapped_remembered.contains(&mapped.bits()));
    assert!(heap.mapped_remembered.contains(&permanent.bits()));
}

#[test]
fn generational_header_claim_is_unique_across_threads() {
    // The wrapper is shared only to call the production atomic claim. No
    // worker reads or changes the header's non-atomic fields or next link.
    struct SharedHeader(GcHeader);
    unsafe impl Send for SharedHeader {}
    unsafe impl Sync for SharedHeader {}
    let header = std::sync::Arc::new(SharedHeader(GcHeader::new(HeapObjectKind::Float)));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let header = header.clone();
            std::thread::spawn(move || header.0.claim_remembered())
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .map(|w| usize::from(w.join().unwrap()))
            .sum::<usize>(),
        1
    );
    assert!(header.0.is_remembered());
    assert!(!header.0.claim_remembered());
}
