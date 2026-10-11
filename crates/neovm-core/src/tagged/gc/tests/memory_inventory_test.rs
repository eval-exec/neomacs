use super::*;
use crate::buffer::{CharPos0, CharRange};
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

fn heap(on: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = on;
    heap.publish_barrier_window();
    heap
}

fn header(value: TaggedValue) -> &'static mut GcHeader {
    unsafe { &mut *(TaggedHeap::value_heap_addr(value).unwrap() as *mut GcHeader) }
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
fn memory_inventory_splits_retained_ages_and_owned_payload_capacities() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let _young_vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    let old_vector = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    let permanent_vector = heap.alloc_vector(vec![TaggedValue::NIL; 3]);
    let _young_cons = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    let old_cons = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    heap.close_alloc_regions();
    header(old_vector).tenured = true;
    header(permanent_vector).make_permanent();
    let (trailer, index) = heap.old_cons_trailer(old_cons).unwrap();
    trailer.set_old(index);
    trailer.set_unlogged(index);

    let inventory = snapshot(&heap, false);
    assert_eq!(inventory.ages.young.objects, 2);
    assert_eq!(inventory.ages.old.objects, 2);
    assert_eq!(inventory.ages.permanent.objects, 1);
    assert_eq!(inventory.ages.young.known_owned_payload_capacity_bytes, 8);
    assert_eq!(inventory.ages.old.known_owned_payload_capacity_bytes, 16);
    assert_eq!(
        inventory.ages.permanent.known_owned_payload_capacity_bytes,
        24
    );
    assert_eq!(inventory.cons.young_cells, 1);
    assert_eq!(inventory.cons.old_cells, 1);
    assert!(inventory.old_dead_pacing_bytes.is_none());
    assert!(inventory.ages.old.marked_live_pacing_bytes.is_none());
}

#[test]
fn memory_inventory_reports_old_dead_only_before_major_promotion() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let live_cons = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    let dead_cons = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    let live_vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    let dead_vector = heap.alloc_vector(vec![TaggedValue::NIL; 17]);
    minor(&mut heap, &[live_cons, dead_cons, live_vector, dead_vector]);

    heap.begin_minor_collection();
    heap.seed_root(live_cons);
    heap.seed_root(live_vector);
    heap.incremental_drain_all();
    let minor_inventory = snapshot(&heap, true);
    assert!(minor_inventory.old_dead_pacing_bytes.is_none());
    assert!(minor_inventory.old_dead_object_and_payload_bytes.is_none());
    assert!(minor_inventory.ages.old.marked_live_pacing_bytes.is_none());
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();

    heap.begin_stw_collection();
    heap.seed_root(live_cons);
    heap.seed_root(live_vector);
    heap.incremental_drain_all();
    heap.mark_and_queue_doomed_finalizers();
    heap.mark_and_sweep_weak_tables();
    let inventory = snapshot(&heap, true);
    let dead_vector_bytes = size_of::<VectorObj>() + 17 * size_of::<TaggedValue>();
    assert_eq!(
        inventory.old_dead_pacing_bytes,
        Some(size_of::<ConsCell>() + dead_vector_bytes)
    );
    assert_eq!(
        inventory.old_dead_object_and_payload_bytes,
        inventory.old_dead_pacing_bytes
    );
    assert_eq!(
        inventory.ages.old.marked_live_pacing_bytes,
        Some(size_of::<ConsCell>() + size_of::<VectorObj>() + size_of::<TaggedValue>())
    );
    heap.complete_collection();
    let completed = snapshot(&heap, false);
    assert_eq!(completed.ages.old.objects, 2);
    assert!(completed.old_dead_pacing_bytes.is_none());
}

#[test]
fn memory_inventory_counts_arena_spares_and_partial_page_holes_once() {
    let mut heap = heap(false);
    set_tagged_heap(&mut heap);
    let survivor = heap.alloc_string(LispString::from_utf8("kept"));
    for _ in 0..ObjectPage::<StringObj>::SLOTS {
        heap.alloc_string(LispString::from_utf8("discarded"));
    }
    heap.collect_exact(std::iter::once(survivor));
    let inventory = snapshot(&heap, false);
    let string = inventory
        .arenas
        .iter()
        .find(|class| class.class == "string")
        .unwrap();
    assert_eq!(string.active_pages, 1);
    assert_eq!(string.partial_pages, 1);
    assert_eq!(string.spare_pages, 1);
    assert_eq!(string.active_backing_bytes, OBJECT_PAGE_BYTES);
    assert_eq!(string.spare_backing_bytes, OBJECT_PAGE_BYTES);
    assert_eq!(string.occupied_slot_bytes, StringObj::SLOT_BYTES);
    assert_eq!(
        string.reclaimed_slot_bytes,
        OBJECT_PAGE_BYTES - StringObj::SLOT_BYTES
    );
    assert_eq!(string.never_bumped_slot_bytes, 0);
    assert_eq!(string.page_tail_bytes, 0);
    let capacity = unsafe { (*(survivor.as_string_ptr().unwrap())).data.owned_capacity() };
    assert_eq!(string.known_owned_payload_capacity_bytes, capacity);
    assert_eq!(
        inventory.known_allocator_owned_bytes,
        2 * OBJECT_PAGE_BYTES + capacity
    );
}

#[test]
fn memory_inventory_does_not_count_free_unmarked_cons_as_occupied() {
    let mut heap = heap(false);
    set_tagged_heap(&mut heap);
    let survivor = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::NIL);
    for _ in 0..CONS_BLOCK_SIZE {
        heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    }
    heap.collect_exact(std::iter::once(survivor));
    // Keep a reclaimed cell white: age/mark bits alone cannot tell this free
    // cell apart from the newly allocated white cell immediately below.
    assert_eq!(heap.cons_blocks.len(), 1);
    let inventory = snapshot(&heap, false);
    assert_eq!(inventory.cons.occupied_cells, 1);
    assert_eq!(inventory.cons.free_cells, CONS_BLOCK_SIZE - 1);
    assert_eq!(inventory.cons.backing_bytes, CONS_BLOCK_BYTES);
    assert_eq!(inventory.ages.young.objects, 1);
    assert_eq!(inventory.known_allocator_owned_bytes, CONS_BLOCK_BYTES);
    let _new_white = heap.alloc_cons(TaggedValue::fixnum(9), TaggedValue::NIL);
    heap.close_alloc_regions();
    let second = snapshot(&heap, false);
    assert_eq!(second.cons.occupied_cells, 2);
    assert_eq!(second.cons.free_cells, CONS_BLOCK_SIZE - 2);
}

#[test]
fn memory_inventory_handles_heap_values_in_cons_cars_without_lisp_equality() {
    let mut heap = heap(false);
    set_tagged_heap(&mut heap);
    let string = heap.alloc_string(LispString::from_utf8("occupied"));
    heap.alloc_cons(string, TaggedValue::NIL);
    heap.close_alloc_regions();
    // DEAD is allocator poison, not a valid operand to Lisp structural
    // equality. A string car must be classified using raw bits only.
    let inventory = snapshot(&heap, false);
    assert_eq!(inventory.cons.occupied_cells, 1);
    assert_eq!(inventory.cons.free_cells, 0);
    assert_eq!(inventory.ages.young.objects, 2);
}

#[test]
fn memory_inventory_includes_detached_young_box_sweep_list() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};
    let mut heap = heap(false);
    set_tagged_heap(&mut heap);
    let table = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    heap.begin_collection();
    heap.seed_root(table);
    heap.incremental_drain_all();
    heap.incremental_finish(0, std::time::Instant::now());
    assert!(heap.all_objects.is_null());
    assert!(!heap.sweep_noncons_pending.is_null());
    let inventory = snapshot(&heap, false);
    let table = inventory
        .boxed
        .iter()
        .find(|class| class.class == "hash-table")
        .unwrap();
    assert_eq!(table.objects, 1);
    assert_eq!(inventory.ages.young.objects, 1);
    heap.finish_incremental_sweep_now();
}

#[test]
fn memory_inventory_includes_string_interval_owned_storage_without_shared_names() {
    let mut heap = heap(false);
    set_tagged_heap(&mut heap);
    let mut string = LispString::from_utf8("abc");
    string.intervals_mut().put_property_in_char_range(
        CharRange::new(CharPos0::new(0), CharPos0::new(3)),
        Value::symbol("face"),
        Value::T,
    );
    let clone = string.clone();
    let storage = string.intervals().memory_telemetry_storage();
    assert!(storage.node_logical_bytes > 0);
    assert!(storage.node_capacity_bytes >= storage.node_logical_bytes);
    assert_eq!(storage.shared_property_names, 1);
    assert_eq!(storage.shared_property_name_owners, 2);
    assert_eq!(
        storage.known_boxed_owned_capacity_bytes(),
        storage.table_struct_bytes
            + storage.node_capacity_bytes
            + storage.cached_range_capacity_bytes.unwrap()
    );
    let string_capacity = string.owned_capacity();
    heap.alloc_string(string);
    heap.close_alloc_regions();
    let inventory = snapshot(&heap, false);
    let string = inventory
        .arenas
        .iter()
        .find(|class| class.class == "string")
        .unwrap();
    assert_eq!(
        string.known_owned_payload_capacity_bytes,
        string_capacity + storage.known_boxed_owned_capacity_bytes()
    );
    assert_eq!(
        string.known_text_property_capacity_bytes,
        storage.known_boxed_owned_capacity_bytes()
    );
    assert_eq!(
        inventory.known_allocator_owned_bytes,
        inventory.cons.backing_bytes
            + OBJECT_PAGE_BYTES
            + string_capacity
            + storage.known_boxed_owned_capacity_bytes()
    );
    assert_eq!(inventory.text_properties.tables, 1);
    assert_eq!(inventory.text_properties.shared_property_name_tables, 1);
    drop(clone);
}
