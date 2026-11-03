//! The cons age-byte mirror that GEN1 compiled stores test: only the two
//! stop-all promotions write it, GEN1 verify catches a dropped write, a
//! freed old slot reads young again, and a false-dirty card stays harmless.

use super::mirror_fault::{self, MirrorFault};
use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::heap_types::LispString;
use std::panic::{AssertUnwindSafe, catch_unwind};

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

/// Clears the injected fault even when the expected panic unwinds.
struct Fault;

impl Fault {
    fn inject(fault: MirrorFault) -> Self {
        mirror_fault::set(fault);
        Self
    }
}

impl Drop for Fault {
    fn drop(&mut self) {
        mirror_fault::set(MirrorFault::None);
    }
}

fn verified_heap() -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.set_generational_enabled(true);
    heap.generational.verify = true;
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.publish_barrier_window();
    heap
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection().unwrap();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn verify_rejects_a_minor_promotion_that_drops_an_age_write() {
    let mut heap = verified_heap();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(owner);
    let fault = Fault::inject(MirrorFault::SkipPromotionLanes);
    let panic = catch_unwind(AssertUnwindSafe(|| minor(&mut heap, &[owner])))
        .expect_err("a promotion that skips the mirror must fail verification");
    drop(fault);
    let message = panic_message(panic);
    assert!(message.contains("age bytes differ"), "{message}");
}

#[test]
fn verify_rejects_a_major_that_leaves_a_dead_cell_old_in_the_mirror() {
    let mut heap = verified_heap();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    assert!(heap.cons_age_is_old_for_test(owner));
    drop(roots);
    let fault = Fault::inject(MirrorFault::SkipMajorClear);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        heap.collect_exact(std::iter::empty());
    }))
    .expect_err("a major that keeps a dead cell old in the mirror must fail verification");
    drop(fault);
    let message = panic_message(panic);
    assert!(message.contains("age bytes differ"), "{message}");
}

#[test]
fn a_freed_old_slot_reused_by_a_new_cons_reads_young_at_the_barrier() {
    let mut heap = verified_heap();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    // A live neighbour keeps the block, so the sweep frees just the slot.
    let keeper = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(owner);
    roots.keep(keeper);
    minor(&mut heap, &[owner, keeper]);
    assert!(heap.cons_age_is_old_for_test(owner));
    let slot = owner.bits();
    drop(roots);
    let roots = ScratchRoots::new();
    roots.keep(keeper);
    heap.collect_exact(std::iter::once(keeper));
    heap.finish_incremental_sweep_now();
    // The sweep hands freed cells back to allocation; find the old slot.
    let reused = (0..4 * CONS_BLOCK_SIZE)
        .map(|_| {
            let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
            roots.keep(cons);
            cons
        })
        .find(|cons| cons.bits() == slot)
        .expect("the freed old slot is reused");
    assert!(!heap.value_is_old_for_test(reused));
    assert!(!heap.cons_age_is_old_for_test(reused));
    let child = heap.alloc_string(LispString::from_utf8("young-child"));
    assert!(crate::tagged::mutate::set_cons_car(reused, child));
    assert!(
        !heap.owner_card_is_dirty_for_test(reused),
        "a young owner's store marks no card"
    );
    minor(&mut heap, &[reused]);
}

#[test]
fn a_false_dirty_aliased_card_passes_verification() {
    let mut heap = verified_heap();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    assert!(!heap.owner_card_is_dirty_for_test(owner));
    // A store to an address 4 GiB away aliases onto the owner's card.
    let alias = (owner.xcons_ptr() as usize).wrapping_add(1 << 32);
    heap.mark_owner_card(alias);
    assert!(heap.owner_card_is_dirty_for_test(owner));
    // The false dirty only makes the minor trace the old cells of the card.
    minor(&mut heap, &[owner]);
    assert!(!heap.owner_card_is_dirty_for_test(owner));
    // A false clean is the failing case:
    // card_table_tests::verify_mode_rejects_an_old_cons_edge_with_a_clean_card.
}
