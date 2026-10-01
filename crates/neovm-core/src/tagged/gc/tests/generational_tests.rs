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
    heap.begin_minor_collection();
    assert!(heap.current_mutator_gc().remset.is_empty());
    assert_eq!(heap.generational.r_seed, [owner]);
    heap.seed_root(owner);
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
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
    heap.begin_minor_collection();
    assert!(
        heap.current_mutator_gc()
            .remembered_cache
            .iter()
            .all(|&bits| bits == 0)
    );
    assert_eq!(heap.generational.r_seed.len(), 2);
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
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

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
}

#[test]
fn minor_old_vector_and_cons_keep_new_children_and_drop_overwritten_young() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    roots.keep(vector);
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(cons);
    minor(&mut heap, &[vector, cons]);
    assert!(header(vector).tenured);
    assert!(!header(vector).generation.permanent());
    assert_eq!(header(vector).raw_mark(), UNMARKED_AT_REST);
    let discarded = heap.alloc_string(LispString::from_utf8("discarded"));
    crate::tagged::mutate::set_vector_slot(vector, 0, discarded);
    crate::tagged::mutate::set_vector_slot(vector, 0, TaggedValue::NIL);
    let string = heap.alloc_string(LispString::from_utf8("kept"));
    roots.keep(string);
    crate::tagged::mutate::set_cons_car(cons, string);
    let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    crate::tagged::mutate::set_vector_slot(vector, 0, child);
    assert_eq!(heap.remembered_log_len_for_test(), 2);
    minor(&mut heap, &[vector, cons]);
    assert!(!heap.owns_string_object(discarded.as_string_ptr().unwrap().cast()));
    assert!(heap.value_is_old_for_test(child));
    assert!(heap.value_is_old_for_test(string));
    assert_eq!(vector.as_vector_data().unwrap().as_slice()[0], child);
    assert_eq!(cons.cons_car(), string);
    assert_eq!(header(vector).raw_mark(), UNMARKED_AT_REST);
    assert_eq!(heap.remembered_log_len_for_test(), 0);
    // Eager promotion makes this child old; it is intentionally retained
    // after overwriting until C2.6's old-generation sweep exists.
    crate::tagged::mutate::set_vector_slot(vector, 0, TaggedValue::NIL);
    minor(&mut heap, &[]);
    assert!(heap.value_is_old_for_test(child));
}

#[test]
fn minor_mapped_and_permanent_edges_are_seeded_by_r() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let image = fake_image::FakeImage::leak(false);
    let mapped = image.register_vector(&mut heap);
    roots.keep(mapped);
    let permanent = heap.alloc_vector(vec![TaggedValue::NIL]);
    roots.keep(permanent);
    heap.collect_exact(std::iter::once(permanent));
    let child = heap.alloc_cons(TaggedValue::fixnum(29), TaggedValue::NIL);
    crate::tagged::mutate::set_vector_slot(mapped, 0, child);
    crate::tagged::mutate::set_vector_slot(permanent, 0, child);
    // Prove the minor does not depend on the full cycle's persistent seeds.
    heap.mapped_remembered.clear();
    minor(&mut heap, &[]);
    assert!(heap.value_is_old_for_test(child));
    assert_eq!(child.cons_car(), TaggedValue::fixnum(29));
    assert_eq!(heap.remembered_log_len_for_test(), 0);
}

#[test]
fn minor_eager_box_promotion_precedes_sweep_window_stores() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    roots.keep(owner);
    heap.begin_minor_collection();
    heap.seed_root(owner);
    heap.complete_minor_collection();
    assert!(heap.sweep_in_progress());
    assert!(header(owner).tenured);
    assert!(
        heap.generational.old_objects.is_null(),
        "Box movement is deferred"
    );
    let child = heap.alloc_string(LispString::from_utf8("sweep-born"));
    crate::tagged::mutate::with_hash_table_mut(owner, |table| {
        table.insert(
            TaggedValue::T.to_hash_key(&HashTableTest::Eq),
            TaggedValue::T,
            child,
        );
    });
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
    heap.finish_incremental_sweep_now();
    assert_eq!(
        heap.generational.old_objects,
        header(owner) as *mut GcHeader
    );
    assert_eq!(header(owner).raw_mark(), UNMARKED_AT_REST);
    assert!(
        !header(child).tenured,
        "sweep-born objects are not in promo"
    );
    minor(&mut heap, &[]);
    assert!(header(child).tenured);
    assert!(
        header(owner).gc_link().is_null(),
        "old Box list has no duplicate"
    );
    assert_eq!(
        heap.layout_stats()
            .boxed
            .iter()
            .map(|kind| kind.objects)
            .sum::<usize>(),
        1
    );
}

#[test]
fn current_full_cycle_keeps_r_facts_until_next_minor() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_vector(vec![TaggedValue::NIL]);
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    heap.concurrent_begin();
    heap.seed_root(owner);
    heap.launch_concurrent_mark();
    let child = heap.alloc_cons(TaggedValue::fixnum(83), TaggedValue::NIL);
    crate::tagged::mutate::set_vector_slot(owner, 0, child);
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(owner);
    heap.incremental_drain_all();
    heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.value_is_old_for_test(child));
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
    minor(&mut heap, &[]);
    assert_eq!(child.cons_car(), TaggedValue::fixnum(83));
    assert!(heap.value_is_old_for_test(child));
}

#[test]
fn minor_weak_table_drops_young_keys_and_retains_old_keys() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let table = heap.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        Some(HashTableWeakness::Key),
        1.5,
        0.8,
    ));
    roots.keep(table);
    let key = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    crate::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(
            key.to_hash_key(&HashTableTest::Eq),
            key,
            TaggedValue::fixnum(7),
        );
    });
    minor(&mut heap, &[table]);
    let count = |table: TaggedValue| table.as_hash_table().unwrap().data.len();
    assert_eq!(count(table), 0);
    let old_key = heap.alloc_cons(TaggedValue::fixnum(5), TaggedValue::NIL);
    roots.keep(old_key);
    minor(&mut heap, &[old_key]);
    crate::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(
            old_key.to_hash_key(&HashTableTest::Eq),
            old_key,
            TaggedValue::T,
        );
    });
    assert_eq!(heap.current_mutator_gc().remset, [table]);
    minor(&mut heap, &[]);
    assert_eq!(count(table), 1);
}

#[test]
fn minor_finalizer_keeps_old_registry_members_and_queues_young_dead() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let function = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    roots.keep(function);
    let old = heap.alloc_finalizer(function);
    roots.keep(old);
    minor(&mut heap, &[old]);
    let young = heap.alloc_finalizer(function);
    minor(&mut heap, &[]);
    assert!(heap.value_is_old_for_test(old));
    assert!(!heap.owns_non_cons_object(young.as_veclike_ptr().unwrap().cast()));
    assert_eq!(heap.take_doomed_finalizer_functions(), [function]);
    assert_eq!(heap.finalizer_registry.len(), 1);
}

#[test]
fn allocation_counts_are_identical_with_generations_off_and_on() {
    let allocate = |on| {
        let mut heap = heap_with_generations(on);
        set_tagged_heap(&mut heap);
        for i in 0..257 {
            let _ = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::NIL);
        }
        let _ = heap.alloc_string(LispString::from_utf8("counts"));
        let _ = heap.alloc_vector(vec![TaggedValue::NIL; 5]);
        let _ = heap.alloc_record(vec![TaggedValue::NIL; 3]);
        heap.memory_use_counts_snapshot()
    };
    assert_eq!(allocate(false), allocate(true));
}

#[test]
fn current_full_before_first_minor_preserves_permanent_edge() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let image = fake_image::FakeImage::leak(false);
    let _ = image.register_vector(&mut heap);
    let owner = heap.alloc_vector(vec![TaggedValue::NIL]);
    roots.keep(owner);
    heap.collect_exact(std::iter::once(owner));
    assert!(header(owner).generation.permanent());
    let child = heap.alloc_cons(TaggedValue::fixnum(97), TaggedValue::NIL);
    crate::tagged::mutate::set_vector_slot(owner, 0, child);
    heap.collect_exact(std::iter::once(owner));
    assert_eq!(heap.generational.old_bytes, 0);
    assert_eq!(heap.current_mutator_gc().remset, [owner]);
    minor(&mut heap, &[owner]);
    assert!(heap.value_is_old_for_test(child));
    assert_eq!(child.cons_car(), TaggedValue::fixnum(97));
}

#[test]
fn minor_unchains_young_dead_marker_and_keeps_old_chain_node() {
    use crate::heap_types::LispMarker;
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let marker_data = || LispMarker {
        buffer: None,
        insertion_type: false,
        marker_id: None,
        bytepos: 0,
        charpos: 0,
        last_position_valid: false,
        next_marker: std::ptr::null_mut(),
        chained: false,
    };
    let old = heap.alloc_marker(marker_data());
    roots.keep(old);
    minor(&mut heap, &[old]);
    let young = heap.alloc_marker(marker_data());
    let text = crate::buffer::buffer_text::BufferText::new();
    let old_ptr = old.as_veclike_ptr().unwrap().cast::<MarkerObj>() as *mut MarkerObj;
    let young_ptr = young.as_veclike_ptr().unwrap().cast::<MarkerObj>() as *mut MarkerObj;
    text.chain_splice_at_head(old_ptr);
    text.chain_splice_at_head(young_ptr);
    unsafe { heap.set_marker_chain_head_slots(vec![text.markers_head_slot_raw()]) };
    minor(&mut heap, &[]);
    assert_eq!(text.chain_walk_collect(), [old_ptr]);
    assert!(heap.marker_arena.owns(old_ptr.cast()));
    assert!(!heap.marker_arena.owns(young_ptr.cast()));
    assert_eq!(header(old).raw_mark(), UNMARKED_AT_REST);
    // Current full cycles still retain ordinary old nodes, and their raw
    // mark stays white even when a parity repeats (C2.6 supplies major reclaim).
    let mut parities = Vec::new();
    for _ in 0..3 {
        heap.collect_exact(std::iter::once(old));
        parities.push(heap.mark_parity);
        assert_eq!(header(old).raw_mark(), UNMARKED_AT_REST);
    }
    assert_eq!(parities[0], parities[2]);
}

#[test]
fn minor_uninterned_symbol_weak_key_survives_without_old_owner_retrace() {
    use crate::emacs_core::intern::{intern_uninterned, is_canonical_id};
    use crate::emacs_core::value::{HashTableTest, LispHashTable};

    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let symbol_id = intern_uninterned("u34-minor-old-owner-symbol");
    assert!(!is_canonical_id(symbol_id));
    let symbol = TaggedValue::from_sym_id(symbol_id);
    let owner = heap.alloc_vector(vec![symbol]);
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    assert!(heap.value_is_old_for_test(owner));

    let table = heap.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        Some(HashTableWeakness::Key),
        1.5,
        0.8,
    ));
    roots.keep(table);
    let payload = heap.alloc_cons(TaggedValue::fixnum(101), TaggedValue::NIL);
    crate::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(symbol.to_hash_key(&HashTableTest::Eq), symbol, payload);
    });
    let dead_key = heap.alloc_cons(TaggedValue::fixnum(103), TaggedValue::NIL);
    crate::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(
            dead_key.to_hash_key(&HashTableTest::Eq),
            dead_key,
            TaggedValue::fixnum(107),
        );
    });

    minor(&mut heap, &[owner, table]);
    assert!(
        !heap.marked_symbols.contains(symbol_id),
        "the minor skips the old owner's historical symbol-mark repair"
    );
    {
        let table = table.as_hash_table().unwrap();
        assert_eq!(table.data.len(), 1);
        let entry = table.data.entries_in_slot_order().next().unwrap();
        assert_eq!((entry.key, entry.value), (symbol, payload));
    }
    assert!(heap.value_is_old_for_test(payload));
    assert_eq!(payload.cons_car(), TaggedValue::fixnum(101));
    assert!(!heap.is_value_marked(dead_key));

    // The full path must still perform the skipped old-owner repair rather
    // than treating every symbol id as live, as a minor conservatively does.
    heap.collect_exact([owner, table].into_iter());
    assert!(heap.marked_symbols.contains(symbol_id));
    assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
}

#[test]
fn minor_then_major_old_owner_keeps_new_uninterned_weak_symbol_key() {
    use crate::emacs_core::intern::{intern_uninterned, is_canonical_id};
    use crate::emacs_core::value::{HashTableTest, LispHashTable};

    for cons_owner in [false, true] {
        for with_position in [false, true] {
            let mut heap = heap_with_generations(true);
            set_tagged_heap(&mut heap);
            let roots = ScratchRoots::new();
            let owner = if cons_owner {
                heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL)
            } else {
                heap.alloc_vector(vec![TaggedValue::NIL])
            };
            roots.keep(owner);
            minor(&mut heap, &[owner]);
            assert!(heap.value_is_old_for_test(owner));

            // Create the symbol after promotion: the old owner's earlier
            // trace cannot have installed this symbol's side-table mark.
            let id = intern_uninterned("u34-review-new-symbol-in-old-owner");
            assert!(!is_canonical_id(id));
            let symbol = TaggedValue::from_sym_id(id);
            let strong_value = if with_position {
                heap.alloc_symbol_with_pos(symbol, TaggedValue::fixnum(41))
            } else {
                symbol
            };
            if cons_owner {
                assert!(crate::tagged::mutate::set_cons_car(owner, strong_value));
            } else {
                assert!(crate::tagged::mutate::set_vector_slot(
                    owner,
                    0,
                    strong_value
                ));
            }
            assert_eq!(heap.current_mutator_gc().remset, [owner]);

            let table = heap.alloc_hash_table(LispHashTable::new_with_options(
                HashTableTest::Eq,
                4,
                Some(HashTableWeakness::Key),
                1.5,
                0.8,
            ));
            roots.keep(table);
            crate::tagged::mutate::with_hash_table_mut(table, |table| {
                table.insert(
                    symbol.to_hash_key(&HashTableTest::Eq),
                    symbol,
                    TaggedValue::fixnum(43),
                );
            });
            minor(&mut heap, &[owner, table]);
            assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
            assert!(heap.marked_symbols.contains(id));
            // Until C2.6, a requested major is today's synchronous full cycle.
            heap.collect_exact([owner, table].into_iter());
            assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
            assert!(heap.marked_symbols.contains(id));
        }
    }
}

#[test]
fn minor_lazy_hydration_keys_survive_from_old_and_permanent_owners() {
    use crate::emacs_core::value::{HashKey, HashTableTest, LispHashTable};

    for permanent in [false, true] {
        let mut heap = heap_with_generations(true);
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let mut table = LispHashTable::new(HashTableTest::Equal);
        table.set_pending_dump_entries(vec![(
            HashKey::EqualCons(Box::new(HashKey::Int(7)), Box::new(HashKey::Nil)),
            TaggedValue::fixnum(11),
            None,
        )]);
        let owner = heap.alloc_hash_table(table);
        roots.keep(owner);
        if permanent {
            let image = fake_image::FakeImage::leak(false);
            roots.keep(image.register_vector(&mut heap));
            heap.collect_exact(std::iter::once(owner));
        } else {
            minor(&mut heap, &[owner]);
        }
        assert!(header(owner).tenured);
        assert_eq!(header(owner).generation.permanent(), permanent);
        let owner_ptr = owner.as_veclike_ptr().unwrap() as *const HashTableObj;
        assert!(unsafe { (*owner_ptr).table.needs_hydration() });

        // This real access reconstructs the structural key as a fresh heap
        // cons. Keep only its address for comparison, and end the table borrow
        // before collecting. The key has no scratch or explicit root.
        let key_address = {
            let hydrated = owner.as_hash_table().unwrap();
            assert!(!hydrated.needs_hydration());
            let key = *hydrated.key_snapshots().next().unwrap();
            assert!(key.is_cons());
            assert_eq!(key.cons_car(), TaggedValue::fixnum(7));
            assert!(!heap.value_is_old_for_test(key));
            key.xcons_ptr() as usize
        };
        assert_eq!(heap.current_mutator_gc().remset, [owner]);
        // A minor must find this edge through R, including for a permanent.
        heap.mapped_remembered.clear();
        minor(&mut heap, &[owner]);
        let key = *owner
            .as_hash_table()
            .unwrap()
            .key_snapshots()
            .next()
            .unwrap();
        assert_eq!(key.xcons_ptr() as usize, key_address);
        assert_eq!(key.cons_car(), TaggedValue::fixnum(7));
        assert_eq!(key.cons_cdr(), TaggedValue::NIL);
        assert!(heap.value_is_old_for_test(key));
        assert!(heap.current_mutator_gc().remset.is_empty());
    }
}

#[test]
#[should_panic(expected = "dump-partition verification: 1 unmarked heap children")]
fn generational_partition_verifier_checks_old_box_children() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};

    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    let child = heap.alloc_cons(TaggedValue::fixnum(109), TaggedValue::NIL);
    let ptr = owner.as_veclike_ptr().unwrap() as *mut HashTableObj;
    // Deliberately bypass the store barrier to check that the verifier sees
    // the collector's old Box list, rather than only permanent/arena owners.
    unsafe {
        (*ptr).table.insert(
            TaggedValue::T.to_hash_key(&HashTableTest::Eq),
            TaggedValue::T,
            child,
        );
    }
    heap.begin_minor_collection();
    heap.seed_root(owner);
    heap.incremental_drain_all();
    heap.verify_dump_partition();
}

#[test]
#[should_panic(expected = "dump-partition verification: 1 unmarked heap children")]
fn generational_partition_verifier_checks_unmarked_old_cons_children() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    let child = heap.alloc_string(LispString::from_utf8("unlogged-young-child"));
    // Deliberately bypass the barrier. The old cons will stay unmarked in the
    // next minor, so an ordinary mark-only owner walk cannot find this edge.
    unsafe { (*(owner.xcons_ptr() as *mut ConsCell)).set_car(child) };
    heap.begin_minor_collection();
    heap.seed_root(owner);
    heap.incremental_drain_all();
    heap.verify_dump_partition();
}
