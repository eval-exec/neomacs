//! True majors, ordinary old reclamation, and P-all promotion.

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::{
    push_scratch_gc_root_slot, restore_scratch_gc_roots, save_scratch_gc_roots, set_scratch_gc_root,
};
use crate::emacs_core::value::{HashTableTest, LambdaParams, LispHashTable};
use crate::heap_types::{LispMarker, LispString};
use crate::tagged::header::LispByteVec;

struct Roots(usize);

impl Roots {
    fn new() -> Self {
        Self(save_scratch_gc_roots())
    }

    fn keep(&self, value: TaggedValue) -> usize {
        push_scratch_gc_root_slot(value)
    }

    fn clear(&self, slot: usize) {
        set_scratch_gc_root(slot, TaggedValue::NIL);
    }
}

impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn heap(on: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = on;
    heap.publish_barrier_window();
    heap
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
}

fn major(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.collect_exact(roots.iter().copied());
    assert!(!heap.mark_in_progress());
    assert!(!heap.sweep_in_progress());
}

fn header_ptr(value: TaggedValue) -> *const GcHeader {
    TaggedHeap::value_heap_addr(value).unwrap() as *const GcHeader
}

fn assert_ordinary_old(heap: &TaggedHeap, value: TaggedValue) {
    assert!(heap.owns_heap_value_for_test(value));
    assert!(heap.value_is_old_for_test(value));
    if !value.is_cons() {
        // Fresh borrow, after ownership is established; no borrow spans GC.
        let header = unsafe { &*header_ptr(value) };
        assert!(header.tenured);
        assert!(!header.generation.permanent());
    }
}

fn assert_header_at_rest(heap: &TaggedHeap, value: TaggedValue) {
    assert!(heap.owns_heap_value_for_test(value));
    assert_eq!(unsafe { (*header_ptr(value)).raw_mark() }, UNMARKED_AT_REST);
}

fn bytecode(constants: Vec<TaggedValue>) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.constants = constants.into();
    function.ops = vec![Op::Nil];
    function.gnu_bytecode_bytes = Some(LispByteVec::owned(vec![0xAA; 32]));
    function
}

fn retained_bytes(value: TaggedValue) -> LispByteVec {
    let object = unsafe { &*(value.as_veclike_ptr().unwrap() as *const ByteCodeObj) };
    object.data.gnu_bytecode_bytes.as_ref().unwrap().clone()
}

fn weak_key_table(heap: &mut TaggedHeap) -> TaggedValue {
    heap.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        Some(HashTableWeakness::Key),
        1.5,
        0.8,
    ))
}

fn insert(owner: TaggedValue, key: TaggedValue, value: TaggedValue) {
    crate::tagged::mutate::with_hash_table_mut(owner, |table| {
        table.insert(key.to_hash_key(&HashTableTest::Eq), key, value);
    });
}

#[test]
fn major_reclaims_dead_old_cons_box_and_bytecode_payload() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let cons = heap.alloc_cons(TaggedValue::fixnum(101), TaggedValue::NIL);
    let cons_root = roots.keep(cons);
    let boxed = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    let boxed_root = roots.keep(boxed);
    let code = heap.alloc_bytecode(bytecode(vec![]));
    let code_root = roots.keep(code);
    let bytes = retained_bytes(code);
    assert_eq!(bytes.owned_storage_ref_count_for_test(), Some(2));

    minor(&mut heap, &[cons, boxed, code]);
    for value in [cons, boxed, code] {
        assert_ordinary_old(&heap, value);
    }
    assert!(!heap.generational.old_objects.is_null());
    roots.clear(cons_root);
    roots.clear(boxed_root);
    roots.clear(code_root);
    minor(&mut heap, &[]);
    for value in [cons, boxed, code] {
        assert!(heap.owns_heap_value_for_test(value));
    }
    assert_eq!(bytes.owned_storage_ref_count_for_test(), Some(2));

    major(&mut heap, &[]);
    // No allocation or payload/header read occurs between reclamation and
    // ownership assertions, so slot reuse cannot obscure reclamation.
    for value in [cons, boxed, code] {
        assert!(!heap.owns_heap_value_for_test(value));
    }
    assert!(heap.generational.old_objects.is_null());
    assert_eq!(heap.generational.old_cons_count, 0);
    assert_eq!(heap.generational.old_bytes, 0);
    assert_eq!(bytes.owned_storage_ref_count_for_test(), Some(1));
    heap.assert_object_arenas_coherent();
}

#[test]
fn major_keeps_permanent_payload_and_its_ordinary_old_child() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let image = fake_image::FakeImage::leak(false);
    let mapped = image.register_vector(&mut heap);
    let child = heap.alloc_cons(TaggedValue::fixnum(103), TaggedValue::NIL);
    let child_root = roots.keep(child);
    let code = heap.alloc_bytecode(bytecode(vec![child]));
    let code_root = roots.keep(code);
    let bytes = retained_bytes(code);
    crate::tagged::mutate::set_vector_slot(mapped, 0, code);
    major(&mut heap, &[]);
    assert!(unsafe { (*header_ptr(code)).generation.permanent() });
    roots.clear(child_root);
    roots.clear(code_root);

    for _ in 0..3 {
        major(&mut heap, &[]);
        assert!(heap.owns_heap_value_for_test(code));
        assert!(unsafe { (*header_ptr(code)).generation.permanent() });
        assert_eq!(bytes.owned_storage_ref_count_for_test(), Some(2));
        assert!(heap.owns_heap_value_for_test(child));
        assert_eq!(child.cons_car(), TaggedValue::fixnum(103));
    }
    minor(&mut heap, &[]);
    assert!(heap.owns_heap_value_for_test(child));
    assert_eq!(heap.generational.old_cons_count, 1);
    assert_eq!(heap.generational.old_bytes, size_of::<ConsCell>());
}

#[test]
fn major_drops_an_unrooted_old_weak_key_after_minor_retention() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let table = weak_key_table(&mut heap);
    roots.keep(table);
    let key = heap.alloc_cons(TaggedValue::fixnum(107), TaggedValue::NIL);
    let key_root = roots.keep(key);
    insert(table, key, TaggedValue::fixnum(109));
    minor(&mut heap, &[table, key]);
    assert_ordinary_old(&heap, key);
    roots.clear(key_root);

    minor(&mut heap, &[table]);
    assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
    assert!(heap.owns_heap_value_for_test(key));
    major(&mut heap, &[table]);
    assert_eq!(table.as_hash_table().unwrap().data.len(), 0);
    assert!(!heap.owns_heap_value_for_test(key));
    assert_ordinary_old(&heap, table);
    major(&mut heap, &[table]);
    assert_eq!(table.as_hash_table().unwrap().data.len(), 0);
}

#[test]
fn major_queues_an_unrooted_old_finalizer_once() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let function = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    roots.keep(function);
    let finalizer = heap.alloc_finalizer(function);
    let finalizer_root = roots.keep(finalizer);
    minor(&mut heap, &[function, finalizer]);
    assert_ordinary_old(&heap, finalizer);
    roots.clear(finalizer_root);

    minor(&mut heap, &[function]);
    assert_eq!(heap.live_finalizer_count(), 1);
    assert!(heap.take_doomed_finalizer_functions().is_empty());
    major(&mut heap, &[function]);
    assert!(!heap.owns_heap_value_for_test(finalizer));
    assert_eq!(heap.live_finalizer_count(), 0);
    assert_eq!(heap.take_doomed_finalizer_functions(), [function]);
    assert!(heap.owns_heap_value_for_test(function));
    major(&mut heap, &[function]);
    assert!(heap.take_doomed_finalizer_functions().is_empty());
}

#[test]
fn major_unchains_an_unrooted_old_marker() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let marker = heap.alloc_marker(LispMarker {
        buffer: None,
        insertion_type: false,
        marker_id: None,
        bytepos: 0,
        charpos: 0,
        last_position_valid: false,
        next_marker: std::ptr::null_mut(),
        chained: false,
    });
    let marker_root = roots.keep(marker);
    minor(&mut heap, &[marker]);
    let text = crate::buffer::buffer_text::BufferText::new();
    let pointer = marker.as_veclike_ptr().unwrap().cast::<MarkerObj>() as *mut MarkerObj;
    text.chain_splice_at_head(pointer);
    unsafe { heap.set_marker_chain_head_slots(vec![text.markers_head_slot_raw()]) };
    roots.clear(marker_root);
    minor(&mut heap, &[]);
    assert_eq!(text.chain_walk_collect(), [pointer]);
    assert!(heap.marker_arena.owns(pointer.cast()));
    // Head slots are consumed by each collection, as the real Context root
    // walk reinstalls them before every cycle.
    unsafe { heap.set_marker_chain_head_slots(vec![text.markers_head_slot_raw()]) };
    major(&mut heap, &[]);
    assert!(text.chain_walk_collect().is_empty());
    assert!(!heap.marker_arena.owns(pointer.cast()));
}

#[test]
fn major_retraces_old_children_when_mark_parity_repeats() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let child = heap.alloc_string(LispString::from_utf8("reachable then dead"));
    let child_root = roots.keep(child);
    let boxed = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    let boxed_root = roots.keep(boxed);
    insert(boxed, TaggedValue::T, child);
    let owner = heap.alloc_cons(boxed, TaggedValue::NIL);
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    roots.clear(child_root);
    roots.clear(boxed_root);

    major(&mut heap, &[owner]);
    let first_parity = heap.mark_parity;
    assert_header_at_rest(&heap, child);
    assert_header_at_rest(&heap, boxed);
    major(&mut heap, &[owner]);
    assert_ne!(heap.mark_parity, first_parity);
    assert_eq!(owner.cons_car(), boxed);
    assert!(heap.owns_heap_value_for_test(child));
    assert_header_at_rest(&heap, child);
    assert_header_at_rest(&heap, boxed);

    crate::tagged::mutate::with_hash_table_mut(boxed, |table| {
        table.data.clear();
    });
    major(&mut heap, &[owner]);
    assert_eq!(heap.mark_parity, first_parity);
    assert!(!heap.owns_heap_value_for_test(child));
    assert!(heap.owns_heap_value_for_test(boxed));
    assert_eq!(owner.cons_car(), boxed);
    assert_header_at_rest(&heap, boxed);
}

#[test]
fn major_retraces_old_uninterned_symbol_owner_without_recent_r() {
    use crate::emacs_core::intern::{intern_uninterned, is_canonical_id};

    for positioned in [false, true] {
        let mut heap = heap(true);
        set_tagged_heap(&mut heap);
        let roots = Roots::new();
        let id = intern_uninterned("u34b-major-old-owner-symbol");
        assert!(!is_canonical_id(id));
        let symbol = TaggedValue::from_sym_id(id);
        let value = if positioned {
            heap.alloc_symbol_with_pos(symbol, TaggedValue::fixnum(113))
        } else {
            symbol
        };
        let value_root = roots.keep(value);
        let owner = heap.alloc_vector(vec![value]);
        let owner_root = roots.keep(owner);
        let table = weak_key_table(&mut heap);
        roots.keep(table);
        insert(table, symbol, TaggedValue::fixnum(127));
        minor(&mut heap, &[owner, table]);
        roots.clear(value_root);
        assert!(heap.current_mutator_gc().remset.is_empty());
        major(&mut heap, &[owner, table]);
        assert!(heap.marked_symbols.contains(id));
        assert_eq!(table.as_hash_table().unwrap().data.len(), 1);

        roots.clear(owner_root);
        major(&mut heap, &[table]);
        assert!(!heap.marked_symbols.contains(id));
        assert_eq!(table.as_hash_table().unwrap().data.len(), 0);
        assert!(!heap.owns_heap_value_for_test(owner));
    }
}

#[test]
fn late_dump_partition_visits_old_objects_and_consumes_stale_r_safely() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let live = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    let live_root = roots.keep(live);
    let dead = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    let dead_root = roots.keep(dead);
    let vector = heap.alloc_vector(vec![live]);
    let vector_root = roots.keep(vector);
    minor(&mut heap, &[vector, dead]);
    assert_ordinary_old(&heap, live);
    assert_ordinary_old(&heap, dead);
    assert_ordinary_old(&heap, vector);

    let live_child = heap.alloc_cons(TaggedValue::fixnum(131), TaggedValue::NIL);
    let live_child_root = roots.keep(live_child);
    insert(live, TaggedValue::T, live_child);
    let dead_child = heap.alloc_cons(TaggedValue::fixnum(137), TaggedValue::NIL);
    let dead_child_root = roots.keep(dead_child);
    insert(dead, TaggedValue::T, dead_child);
    assert_eq!(heap.current_mutator_gc().remset.len(), 2);

    // One contiguous image allocation, registered only after ordinary old
    // Box nodes already exist. Only its slot retains the live graph.
    let image = fake_image::FakeImage::leak(false);
    let mapped = image.register_vector(&mut heap);
    crate::tagged::mutate::set_vector_slot(mapped, 0, vector);
    for slot in [
        live_root,
        dead_root,
        vector_root,
        live_child_root,
        dead_child_root,
    ] {
        roots.clear(slot);
    }
    assert!(heap.is_partition_first_cycle());
    major(&mut heap, &[]);
    assert!(!heap.is_partition_first_cycle());
    assert!(!heap.owns_heap_value_for_test(dead));
    assert!(!heap.owns_heap_value_for_test(dead_child));
    assert!(heap.owns_heap_value_for_test(live));
    assert!(heap.owns_heap_value_for_test(vector));
    assert!(heap.owns_heap_value_for_test(live_child));
    assert!(unsafe { (*header_ptr(live)).generation.permanent() });
    assert!(unsafe { (*header_ptr(vector)).generation.permanent() });
    assert!(heap.generational.old_objects.is_null());
    assert!(
        !heap
            .current_mutator_gc()
            .remset
            .iter()
            .any(|value| value.bits() == dead.bits())
    );
    assert!(
        !heap
            .generational
            .r_seed
            .iter()
            .any(|value| value.bits() == dead.bits())
    );

    // A later minor/major cannot touch the stale freed owner's logged byte.
    minor(&mut heap, &[]);
    major(&mut heap, &[]);
    assert_eq!(live_child.cons_car(), TaggedValue::fixnum(131));
    assert!(heap.owns_heap_value_for_test(live));
    assert_eq!(heap.generational.old_cons_count, 1);
    assert_eq!(heap.generational.old_bytes, size_of::<ConsCell>());
}

#[test]
fn concurrent_major_promotes_worker_claims_black_births_before_deferred_sweep() {
    use super::knobs::{VecScanMode, set_vec_scan_mode_for_test};

    set_vec_scan_mode_for_test(Some(VecScanMode::Snapshot));
    let mut heap = heap(true);
    set_vec_scan_mode_for_test(None);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let target = heap.alloc_vector(vec![TaggedValue::NIL]);
    let target_root = roots.keep(target);
    let owner = heap.alloc_cons(target, TaggedValue::NIL);
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    roots.clear(target_root);
    assert!(heap.current_mutator_gc().remset.is_empty());
    let start_string = heap.alloc_string(LispString::from_utf8("worker-claimed young"));
    let start_string_root = roots.keep(start_string);
    let start_vector = heap.alloc_vector(vec![start_string]);
    let start_vector_root = roots.keep(start_vector);
    crate::tagged::mutate::set_cons_cdr(owner, start_vector);
    roots.clear(start_string_root);
    roots.clear(start_vector_root);
    assert!(!heap.value_is_old_for_test(start_string));
    assert!(!heap.value_is_old_for_test(start_vector));

    heap.concurrent_begin();
    heap.seed_root(owner);
    heap.launch_concurrent_mark();
    assert!(heap.concurrent_mark_running());
    let child = heap.alloc_cons(TaggedValue::fixnum(139), TaggedValue::NIL);
    let child_root = roots.keep(child);
    crate::tagged::mutate::set_vector_slot(target, 0, child);
    roots.clear(child_root);
    let string = heap.alloc_string(LispString::from_utf8("major-born"));
    roots.keep(string);
    let vector = heap.alloc_vector(vec![TaggedValue::fixnum(149)]);
    roots.keep(vector);
    let first_float = heap.alloc_float(1.5);
    roots.keep(first_float);
    let second_float = heap.alloc_float(2.5);
    roots.keep(second_float);
    heap.close_alloc_regions();
    let next_region_float = heap.alloc_float(3.5);
    roots.keep(next_region_float);
    let code = heap.alloc_bytecode(bytecode(vec![]));
    roots.keep(code);
    let boxed = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    roots.keep(boxed);
    // Rooted construction is independent of the tested allocate-black rule.
    // These births are not explicitly reseeded; P-all must promote them.
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    assert!(heap.sweep_stats().last_concurrent_vec_claimed >= 2);
    assert!(heap.sweep_stats().last_concurrent_str_claimed > 0);
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(owner);
    heap.incremental_drain_all();
    heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
    assert!(heap.sweep_in_progress());
    for value in [
        owner,
        target,
        start_string,
        start_vector,
        child,
        string,
        vector,
        first_float,
        second_float,
        next_region_float,
        code,
        boxed,
    ] {
        assert_ordinary_old(&heap, value);
    }
    // Marks still carry this cycle's parity until their sweep visit. GEN-3
    // zero is asserted only after the whole deferred sweep completes.
    assert_eq!(
        unsafe { (*header_ptr(target)).raw_mark() },
        heap.mark_parity.byte()
    );
    assert!(heap.current_mutator_gc().remset.is_empty());
    assert!(heap.generational.r_seed.is_empty());

    let sweep_born = heap.alloc_float(4.5);
    roots.keep(sweep_born);
    heap.finish_incremental_sweep_now();
    assert!(!heap.value_is_old_for_test(sweep_born));
    for value in [
        target,
        start_string,
        start_vector,
        string,
        vector,
        first_float,
        second_float,
        next_region_float,
        code,
        boxed,
    ] {
        assert_header_at_rest(&heap, value);
    }
    minor(
        &mut heap,
        &[
            owner,
            string,
            vector,
            first_float,
            second_float,
            next_region_float,
            code,
            boxed,
            sweep_born,
        ],
    );
    assert_eq!(target.as_vector_data().unwrap()[0], child);
    assert_eq!(child.cons_car(), TaggedValue::fixnum(139));
    assert!(heap.owns_heap_value_for_test(start_string));
    assert_eq!(owner.cons_cdr(), start_vector);
    assert_ordinary_old(&heap, sweep_born);
}

#[test]
fn major_and_minor_do_not_change_exact_allocation_counter_snapshots() {
    fn script(on: bool) -> Vec<[u64; MEMORY_USE_COUNT_LEN]> {
        let mut heap = heap(on);
        set_tagged_heap(&mut heap);
        let roots = Roots::new();
        let accumulator = roots.keep(TaggedValue::NIL);
        let mut tail = TaggedValue::NIL;
        let mut checkpoints = Vec::new();
        for index in 0..=CONS_BLOCK_SIZE {
            tail = heap.alloc_cons(TaggedValue::fixnum(index as i64), tail);
            set_scratch_gc_root(accumulator, tail);
            if index == 0 || index == CONS_BLOCK_SIZE - 1 {
                checkpoints.push(heap.memory_use_counts_snapshot());
            }
        }
        let float = heap.alloc_float(5.5);
        roots.keep(float);
        let other_float = heap.alloc_float(6.5);
        roots.keep(other_float);
        let string = heap.alloc_string(LispString::from_utf8("counter script"));
        roots.keep(string);
        let vector = heap.alloc_vector(vec![TaggedValue::NIL; 3]);
        roots.keep(vector);
        let record = heap.alloc_record(vec![TaggedValue::NIL; 2]);
        roots.keep(record);
        let before_close = heap.memory_use_counts_snapshot();
        checkpoints.push(before_close);
        heap.close_alloc_regions();
        assert_eq!(heap.memory_use_counts_snapshot(), before_close);
        major(
            &mut heap,
            &[tail, float, other_float, string, vector, record],
        );
        assert_eq!(heap.memory_use_counts_snapshot(), before_close);
        checkpoints.push(heap.memory_use_counts_snapshot());
        if on {
            minor(
                &mut heap,
                &[tail, float, other_float, string, vector, record],
            );
        } else {
            major(
                &mut heap,
                &[tail, float, other_float, string, vector, record],
            );
        }
        assert_eq!(heap.memory_use_counts_snapshot(), before_close);
        let final_float = heap.alloc_float(7.5);
        roots.keep(final_float);
        checkpoints.push(heap.memory_use_counts_snapshot());
        heap.close_alloc_regions();
        checkpoints.push(heap.memory_use_counts_snapshot());
        checkpoints
    }
    assert_eq!(script(false), script(true));
}

/// An assertion unwind must not leave a live worker borrowing the stack heap.
/// The test starts a real job, so its receiver exists whenever this joins.
struct FirstPartitionCancelCleanup(*mut TaggedHeap);
impl Drop for FirstPartitionCancelCleanup {
    fn drop(&mut self) {
        // SAFETY: the stack heap was declared before this local guard.
        let heap = unsafe { &mut *self.0 };
        if heap.concurrent_mark_running {
            heap.join_concurrent_mark();
        }
        heap.close_alloc_regions();
        heap.concurrent_mark_running = false;
        heap.mark_in_progress = false;
        heap.generational.major_in_progress = false;
        set_tagged_heap(heap);
        clear_tagged_heap_if_installed(heap);
    }
}

#[test]
fn major_explicit_first_partition_disarm_drops_completed_black_birth_logs() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let image = fake_image::FakeImage::leak(false);
    let mapped = image.register_vector(&mut heap);
    roots.keep(mapped);
    heap.arm_first_cycle_concurrent();
    heap.concurrent_begin();
    heap.launch_concurrent_mark();
    let _cleanup = FirstPartitionCancelCleanup(&mut heap);

    // The first job may already be idle, but publication stays active until
    // join. These births have no snapshot root and retain only allocate-black
    // liveness in this first cycle. Root each before the next allocation.
    let record = heap.alloc_record(vec![TaggedValue::fixnum(211)]);
    let record_root = roots.keep(record);
    let float = heap.alloc_float(2.5);
    let float_root = roots.keep(float);
    let cons = heap.alloc_cons(TaggedValue::fixnum(223), TaggedValue::NIL);
    let cons_root = roots.keep(cons);
    roots.clear(record_root);
    roots.clear(float_root);
    roots.clear(cons_root);

    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.incremental_drain_all();
    let bytes_before = heap.live_bytes();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(heap.first_cycle_concurrent);
    assert!(!heap.dump_blackened);
    assert!(!heap.concurrent_mark_running());
    assert!(!heap.sweep_in_progress());
    assert!(
        heap.current_mutator_gc()
            .black_born
            .contains(&(header_ptr(record) as *mut GcHeader))
    );
    assert!(!heap.current_mutator_gc().black_born_regions.is_empty());
    for value in [record, float, cons] {
        assert!(heap.owns_heap_value_for_test(value));
    }

    // Exactly the explicit-entry ordering: finish the prior sweep, then
    // disarm and run a fresh STW first partition. Calling the permanent
    // splice here instead would pin these unreachable births forever.
    heap.begin_stw_collection();
    assert!(!heap.first_cycle_concurrent);
    assert!(heap.current_mutator_gc().black_born.is_empty());
    assert!(heap.current_mutator_gc().black_born_regions.is_empty());
    heap.seed_root(mapped);
    heap.complete_collection();
    assert!(heap.dump_blackened);
    assert!(!heap.mark_in_progress());
    assert!(!heap.sweep_in_progress());
    for value in [record, float, cons] {
        // Ownership checks only; no payload/header read after reclamation and
        // no allocation between collection and these checks can reuse a slot.
        assert!(!heap.owns_heap_value_for_test(value));
    }
    assert_eq!(heap.generational.old_bytes, 0);
    assert!(heap.current_mutator_gc().black_born.is_empty());
    assert!(heap.current_mutator_gc().black_born_regions.is_empty());
}

struct SymbolInsertionWorkerGuard(*mut TaggedHeap);

impl Drop for SymbolInsertionWorkerGuard {
    fn drop(&mut self) {
        // The heap remains at its stable stack address until this guard drops.
        // An assertion failure still joins the real worker before teardown.
        let heap = unsafe { &mut *self.0 };
        if heap.concurrent_mark_running() {
            heap.join_concurrent_mark();
        }
    }
}

fn claimed_old_cons_symbol_insertion_case(fresh: bool, table_old: bool, car: bool, jit: bool) {
    use crate::emacs_core::intern::{intern_uninterned, is_canonical_id};

    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let owner = heap.alloc_cons(TaggedValue::fixnum(173), TaggedValue::NIL);
    roots.keep(owner);
    let mut table = None;
    if table_old {
        let weak = weak_key_table(&mut heap);
        roots.keep(weak);
        table = Some(weak);
    }
    if let Some(weak) = table {
        minor(&mut heap, &[owner, weak]);
    } else {
        minor(&mut heap, &[owner]);
    }
    let table = table.unwrap_or_else(|| {
        let weak = weak_key_table(&mut heap);
        roots.keep(weak);
        weak
    });
    assert!(heap.value_is_old_for_test(owner));
    assert_eq!(heap.value_is_old_for_test(table), table_old);
    // The weak table's callback is a strong child, also referencing owner.
    // Eq entries do not invoke this callback during the fixture's operations.
    crate::tagged::mutate::with_hash_table_mut(table, |weak| {
        weak.user_cmp_function = Some(owner);
    });
    // An unrelated weak key checks that the repair does not retain all IDs.
    let dead = TaggedValue::from_sym_id(intern_uninterned("u34b-dead-weak-symbol"));
    let dead_root = roots.keep(dead);
    insert(table, dead, TaggedValue::fixnum(181));
    roots.clear(dead_root);
    if !fresh {
        let existing = TaggedValue::from_sym_id(intern_uninterned("u34b-existing-weak-symbol"));
        let existing_root = roots.keep(existing);
        insert(table, existing, TaggedValue::fixnum(179));
        roots.clear(existing_root);
    }

    heap.concurrent_begin();
    heap.seed_root(owner);
    heap.seed_root(table);
    heap.launch_concurrent_mark();
    let _worker = SymbolInsertionWorkerGuard(&mut heap);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    assert!(heap.concurrent_mark_running());
    assert!(
        heap.is_value_marked(owner),
        "the old cons was already claimed"
    );

    let symbol = if fresh {
        TaggedValue::from_sym_id(intern_uninterned("u34b-fresh-inserted-symbol"))
    } else {
        // Obtain an existing ID weakly after claim. No strong start root held it.
        let weak = table.as_hash_table().unwrap();
        weak.entries_in_slot_order()
            .find(|entry| entry.key.bits() != dead.bits())
            .unwrap()
            .key
    };
    let id = symbol.as_symbol_id().unwrap();
    assert!(!is_canonical_id(id));
    let symbol_root = roots.keep(symbol);
    if jit {
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit::compile::{neovm_jit_setcar, neovm_jit_setcdr};
            // Inspected adapters return from the successful cons store before
            // accessing ctx. The valid owned cons guarantees that fast arm.
            // This tests the JIT slow-store adapter, not generated machine code.
            let result = if car {
                neovm_jit_setcar(
                    std::ptr::null_mut(),
                    owner.bits() as i64,
                    symbol.bits() as i64,
                )
            } else {
                neovm_jit_setcdr(
                    std::ptr::null_mut(),
                    owner.bits() as i64,
                    symbol.bits() as i64,
                )
            };
            assert_eq!(result, symbol.bits() as i64);
        }
        #[cfg(not(feature = "jit"))]
        panic!("the JIT fixture is only selected with feature=jit");
    } else if car {
        assert!(crate::tagged::mutate::set_cons_car(owner, symbol));
    } else {
        assert!(crate::tagged::mutate::set_cons_cdr(owner, symbol));
    }
    if fresh {
        insert(table, symbol, TaggedValue::fixnum(179));
    }
    // Construction was rooted. Only the old cons's new strong edge now retains
    // the ID; table keys are weak. Scratch updates do not feed SATB, and the
    // old cons's ordinary claim cannot repeat when roots are reseeded.
    roots.clear(symbol_root);
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(owner);
    heap.seed_root(table);
    heap.incremental_drain_all();
    heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
    assert!(
        heap.marked_symbols.contains(id),
        "fresh={fresh}/old={table_old}/car={car}/jit={jit}"
    );
    assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
    assert_eq!(
        if car {
            owner.cons_car()
        } else {
            owner.cons_cdr()
        },
        symbol
    );
    heap.finish_incremental_sweep_now();
    assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
    assert_eq!(
        table
            .as_hash_table()
            .unwrap()
            .data
            .get(&symbol.to_hash_key(&HashTableTest::Eq)),
        Some(&TaggedValue::fixnum(179))
    );
}

#[test]
fn concurrent_major_keeps_fresh_symbol_inserted_into_claimed_old_cons() {
    for table_old in [false, true] {
        for car in [false, true] {
            claimed_old_cons_symbol_insertion_case(true, table_old, car, false);
            #[cfg(feature = "jit")]
            claimed_old_cons_symbol_insertion_case(true, table_old, car, true);
        }
    }
}

#[test]
fn concurrent_major_keeps_existing_weak_symbol_inserted_into_claimed_old_cons() {
    for table_old in [false, true] {
        for car in [false, true] {
            claimed_old_cons_symbol_insertion_case(false, table_old, car, false);
            #[cfg(feature = "jit")]
            claimed_old_cons_symbol_insertion_case(false, table_old, car, true);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum OldWeakHeapKey {
    Cons,
    Vector,
    String,
}

fn claimed_old_cons_heap_insertion_case(
    kind: OldWeakHeapKey,
    table_old: bool,
    car: bool,
    jit: bool,
    generic: bool,
) {
    let mut heap = heap(true);
    // Isolate insertion liveness from the all-vector snapshot's conservative
    // tracing of children belonging to an otherwise-white weak vector key.
    heap.vec_scan = knobs::VecScanMode::Defer;
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let owner = heap.alloc_cons(TaggedValue::fixnum(191), TaggedValue::NIL);
    roots.keep(owner);
    // The cons/vector key reaches an already-old cold graph, so repairing only
    // its owner mark without tracing its children cannot satisfy this test.
    let cold_graph = if matches!(kind, OldWeakHeapKey::Cons | OldWeakHeapKey::Vector) {
        let payload = heap.alloc_string(LispString::from_utf8("old cold graph payload"));
        let payload_root = roots.keep(payload);
        let child = heap.alloc_cons(payload, TaggedValue::NIL);
        let child_root = roots.keep(child);
        Some((payload, payload_root, child, child_root))
    } else {
        None
    };
    let key = match kind {
        OldWeakHeapKey::Cons => heap.alloc_cons(cold_graph.unwrap().2, TaggedValue::NIL),
        OldWeakHeapKey::Vector => heap.alloc_vector(vec![cold_graph.unwrap().2]),
        OldWeakHeapKey::String => {
            heap.alloc_string(LispString::from_utf8("old weak insertion key"))
        }
    };
    let key_root = roots.keep(key);
    let dead = heap.alloc_cons(TaggedValue::fixnum(197), TaggedValue::NIL);
    let dead_root = roots.keep(dead);
    let mut table = None;
    if table_old {
        let weak = weak_key_table(&mut heap);
        roots.keep(weak);
        table = Some(weak);
    }
    if let Some(weak) = table {
        minor(&mut heap, &[owner, key, dead, weak]);
    } else {
        minor(&mut heap, &[owner, key, dead]);
    }
    let table = table.unwrap_or_else(|| {
        let weak = weak_key_table(&mut heap);
        roots.keep(weak);
        weak
    });
    for value in [owner, key, dead] {
        assert_ordinary_old(&heap, value);
    }
    assert_eq!(heap.value_is_old_for_test(table), table_old);
    // Keep the existing weak-table strong callback trace, without tracing keys.
    // The fixture's Eq operations never invoke this optional callback.
    crate::tagged::mutate::with_hash_table_mut(table, |weak| {
        weak.user_cmp_function = Some(owner);
    });
    insert(table, key, TaggedValue::fixnum(199));
    insert(table, dead, TaggedValue::fixnum(211));
    roots.clear(key_root);
    roots.clear(dead_root);
    if let Some((payload, payload_root, child, child_root)) = cold_graph {
        assert_ordinary_old(&heap, payload);
        assert_ordinary_old(&heap, child);
        roots.clear(payload_root);
        roots.clear(child_root);
    }

    heap.concurrent_begin();
    heap.seed_root(owner);
    heap.seed_root(table);
    heap.launch_concurrent_mark();
    let _worker = SymbolInsertionWorkerGuard(&mut heap);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    assert!(heap.concurrent_mark_running());
    assert!(
        heap.is_value_marked(owner),
        "destination was already claimed"
    );
    assert!(
        !heap.is_value_marked(key),
        "weakly held old key must start white"
    );
    assert!(!heap.is_value_marked(dead), "dead control must start white");
    if let Some((payload, _, child, _)) = cold_graph {
        assert!(
            !heap.is_value_marked(payload),
            "cold payload must start white"
        );
        assert!(!heap.is_value_marked(child), "cold child must start white");
    }
    assert!(heap.current_mutator_gc().black_born.is_empty());
    assert!(heap.current_mutator_gc().black_born_regions.is_empty());
    // Obtain the existing heap object through the weak table after the worker
    // drained. It predates this major and is not an allocate-black birth.
    let inserted = table
        .as_hash_table()
        .unwrap()
        .entries_in_slot_order()
        .find(|entry| entry.key.bits() != dead.bits())
        .unwrap()
        .key;
    assert_eq!(inserted, key);
    let insertion_root = roots.keep(inserted);
    if generic {
        let write = if car {
            HeapWriteKind::ConsCar
        } else {
            HeapWriteKind::ConsCdr
        };
        crate::tagged::gc::gc_thread::note_heap_write(owner, write);
        // Public generic hook followed by its caller's field write. No production
        // cons writer uses this seam today, but it must remain safely supported.
        unsafe {
            let cell = &mut *(owner.xcons_ptr() as *mut ConsCell);
            if car {
                cell.set_car(inserted);
            } else {
                cell.set_cdr(inserted);
            }
        }
    } else if jit {
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit::compile::{neovm_jit_setcar, neovm_jit_setcdr};
            // Valid-cons adapters return before touching ctx. This checks their
            // ordinary slow-store hook, not generated machine-code execution.
            let result = if car {
                neovm_jit_setcar(
                    std::ptr::null_mut(),
                    owner.bits() as i64,
                    inserted.bits() as i64,
                )
            } else {
                neovm_jit_setcdr(
                    std::ptr::null_mut(),
                    owner.bits() as i64,
                    inserted.bits() as i64,
                )
            };
            assert_eq!(result, inserted.bits() as i64);
        }
        #[cfg(not(feature = "jit"))]
        panic!("the JIT fixture requires feature=jit");
    } else if car {
        assert!(crate::tagged::mutate::set_cons_car(owner, inserted));
    } else {
        assert!(crate::tagged::mutate::set_cons_cdr(owner, inserted));
    }
    // Scratch root changes intentionally do not feed SATB. Only the destination
    // cons's inserted strong edge survives through termination; table keys are weak.
    roots.clear(insertion_root);
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(owner);
    heap.seed_root(table);
    heap.incremental_drain_all();
    assert!(
        heap.is_value_marked(inserted),
        "claimed old cons lost an inserted existing old heap object: {kind:?}/table_old={table_old}/car={car}/jit={jit}/generic={generic}",
    );
    if let Some((payload, _, child, _)) = cold_graph {
        assert!(
            heap.is_value_marked(child),
            "insertion repair must trace cold child"
        );
        assert!(
            heap.is_value_marked(payload),
            "insertion repair must trace cold payload"
        );
    }
    // This assertion fails before reclamation on the unfixed source. With the
    // repair, weak sweep keeps the inserted key and drops the unrelated control.
    heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
    assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
    heap.finish_incremental_sweep_now();
    assert!(heap.owns_heap_value_for_test(inserted));
    if let Some((payload, _, child, _)) = cold_graph {
        assert!(heap.owns_heap_value_for_test(child));
        assert!(heap.owns_heap_value_for_test(payload));
    }
    assert!(!heap.owns_heap_value_for_test(dead));
    assert_eq!(
        if car {
            owner.cons_car()
        } else {
            owner.cons_cdr()
        },
        inserted,
    );
    assert_eq!(
        table
            .as_hash_table()
            .unwrap()
            .data
            .get(&inserted.to_hash_key(&HashTableTest::Eq)),
        Some(&TaggedValue::fixnum(199)),
    );
}

#[test]
fn concurrent_major_keeps_existing_weak_cons_inserted_into_claimed_old_cons() {
    for table_old in [false, true] {
        for car in [false, true] {
            claimed_old_cons_heap_insertion_case(
                OldWeakHeapKey::Cons,
                table_old,
                car,
                false,
                false,
            );
            #[cfg(feature = "jit")]
            claimed_old_cons_heap_insertion_case(OldWeakHeapKey::Cons, table_old, car, true, false);
        }
    }
}

#[test]
fn concurrent_major_keeps_existing_weak_vector_inserted_into_claimed_old_cons() {
    for table_old in [false, true] {
        for car in [false, true] {
            claimed_old_cons_heap_insertion_case(
                OldWeakHeapKey::Vector,
                table_old,
                car,
                false,
                false,
            );
            #[cfg(feature = "jit")]
            claimed_old_cons_heap_insertion_case(
                OldWeakHeapKey::Vector,
                table_old,
                car,
                true,
                false,
            );
        }
    }
}

#[test]
fn concurrent_major_keeps_existing_weak_string_inserted_into_claimed_old_cons() {
    for table_old in [false, true] {
        for car in [false, true] {
            claimed_old_cons_heap_insertion_case(
                OldWeakHeapKey::String,
                table_old,
                car,
                false,
                false,
            );
            #[cfg(feature = "jit")]
            claimed_old_cons_heap_insertion_case(
                OldWeakHeapKey::String,
                table_old,
                car,
                true,
                false,
            );
        }
    }
}

#[test]
fn concurrent_major_generic_cons_hook_keeps_existing_weak_heap_objects() {
    for kind in [
        OldWeakHeapKey::Cons,
        OldWeakHeapKey::Vector,
        OldWeakHeapKey::String,
    ] {
        for table_old in [false, true] {
            for car in [false, true] {
                claimed_old_cons_heap_insertion_case(kind, table_old, car, false, true);
            }
        }
    }
}

/// Remembered membership is owner identity, even when two live payloads
/// compare structurally equal. A claim without its append must be rejected.
#[cfg(debug_assertions)]
#[test]
fn major_remembered_membership_requires_owner_identity() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let first = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    roots.keep(first);
    let second = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    roots.keep(second);
    major(&mut heap, &[first, second]);
    assert_ordinary_old(&heap, first);
    assert_ordinary_old(&heap, second);
    assert_ne!(first.bits(), second.bits(), "separately allocated owners");
    assert_eq!(first, second, "the payloads compare structurally equal");

    heap.remember_owner(first);
    assert!(heap.is_remembered_for_test(first));
    heap.debug_assert_remembered_membership(first);
    assert!(unsafe { (*header_ptr(second)).claim_remembered() });
    // Keep the probe result until after restoring the deliberately invalid
    // claim, so either reader's regression leaves a clean fixture on failure.
    let second_was_reported_remembered = heap.is_remembered_for_test(second);
    let invariant = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        heap.debug_assert_remembered_membership(second);
    }));
    unsafe {
        (*header_ptr(second))
            .remembered
            .store(RememberedState::Unlogged as u8, Ordering::Relaxed);
    }
    assert!(
        !second_was_reported_remembered,
        "an equal payload cannot supply another owner's remembered fact",
    );
    assert!(
        invariant.is_err(),
        "remembered claim without this owner's log must violate the invariant",
    );
    heap.debug_assert_remembered_membership(second);
}
