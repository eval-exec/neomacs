//! Validate the capability writer and stopped-start scan through the real collector.
//!
//! These tests drive the real collector with one admitted mutator. TSan supplies
//! the original RED evidence for mixed atomic/plain accesses; batch 65 already
//! supplies the production repair. Ordinary assertions additionally check
//! the unchanged table semantics and immutable image backing. No Lisp value or
//! borrowed table is sent to a test-created thread. All post-publication writes
//! use builtins and therefore the sole CharTableWrite/SubCharTableWrite seam.

use super::*;
use crate::emacs_core::chartable::{
    builtin_set_char_table_extra_slot, builtin_set_char_table_range, ct_lookup,
};
use crate::tagged::header::{CharTableObj, LispValueVec, SubCharTableObj};

#[derive(Clone, Copy, Debug)]
enum SlotStorage {
    Owned,
    Mapped,
}

/// One process-lifetime allocation, like a pdump image. Keeping its headers
/// adjacent prevents the synthetic dump span from swallowing unrelated boxes.
#[repr(C)]
struct ChartableImage {
    table: CharTableObj,
    child: SubCharTableObj,
    child_slots: [TaggedValue; 128],
    extra_slots: [TaggedValue; 3],
}

/// Only the immutable backing is borrowed after registration. The live table
/// headers remain behind their Values, so no shared borrow spans their writes.
struct ImageBacking {
    child_slots: &'static [TaggedValue; 128],
    extra_slots: &'static [TaggedValue; 3],
}

fn image(heap: &mut TaggedHeap, storage: SlotStorage) -> (TaggedValue, ImageBacking) {
    image_with_child(heap, storage, TaggedValue::NIL)
}

fn image_with_child(
    heap: &mut TaggedHeap,
    storage: SlotStorage,
    initial_child: TaggedValue,
) -> (TaggedValue, ImageBacking) {
    let image = Box::leak(Box::new(ChartableImage {
        table: CharTableObj {
            header: VecLikeHeader::new(VecLikeType::CharTable),
            defalt: TaggedValue::NIL.into(),
            parent: TaggedValue::NIL.into(),
            purpose: TaggedValue::NIL.into(),
            ascii: TaggedValue::NIL.into(),
            contents: [TaggedValue::NIL; CHAR_TABLE_TOP_SLOTS].map(AtomicTaggedValue::new),
            extras: LispValueVec::owned(vec![TaggedValue::NIL; 3]),
        },
        child: SubCharTableObj {
            header: VecLikeHeader::new(VecLikeType::SubCharTable),
            depth: 3,
            min_char: 0,
            contents: LispValueVec::owned(vec![initial_child; 128]),
        },
        child_slots: [initial_child; 128],
        extra_slots: [TaggedValue::NIL; 3],
    }));
    match storage {
        SlotStorage::Owned => {}
        SlotStorage::Mapped => {
            // SAFETY: both spans are immutable slots in this leaked image;
            // mutation must first promote them to a private owned backing.
            image.child.contents = unsafe { LispValueVec::mapped(image.child_slots.as_ptr(), 128) };
            // SAFETY: the same process-lifetime immutable image contract.
            image.table.extras = unsafe { LispValueVec::mapped(image.extra_slots.as_ptr(), 3) };
        }
    }
    // SAFETY: headers are fully initialized in process-lifetime storage and are
    // exclusively owned until registration and the admitted collection start.
    let child = unsafe { TaggedValue::from_veclike_ptr(&image.child.header) };
    image.table.ascii.store_atomic(child);
    // SAFETY: the registered extents are adjacent, live, initialized image
    // headers. The leaked allocation outlives the heap and its marker.
    unsafe {
        heap.register_mapped_veclike_object(
            &mut image.table.header,
            std::mem::size_of::<CharTableObj>(),
        );
        heap.register_mapped_veclike_object(
            &mut image.child.header,
            std::mem::size_of::<SubCharTableObj>(),
        );
    }
    // SAFETY: registration retains this initialized table header as image data.
    let table = unsafe { TaggedValue::from_veclike_ptr(&image.table.header) };
    let backing = ImageBacking {
        child_slots: &image.child_slots,
        extra_slots: &image.extra_slots,
    };
    (table, backing)
}

fn launch_image_scan(heap: &mut TaggedHeap, table: TaggedValue) {
    assert!(heap.is_partition_first_cycle());
    heap.arm_first_cycle_concurrent();
    heap.concurrent_begin();
    heap.seed_root(table);
    heap.launch_concurrent_mark();
    // Do not acquire the marker's done flag or join until AFTER the writes.
    // Its channel publication precedes both threads' accesses, but must not
    // order the collector's slot reads before the mutator's later stores.
}

#[test]
fn mapped_subtable_owned_slot_mutation_follows_collector_contract() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let (table, _) = image(&mut heap, SlotStorage::Owned);
    launch_image_scan(&mut heap, table);
    for index in 0..16_384 {
        let character = index % 128;
        builtin_set_char_table_range(
            vec![
                table,
                TaggedValue::fixnum(character),
                TaggedValue::fixnum(index),
            ],
            None,
        )
        .unwrap();
    }
    heap.join_concurrent_mark();
    assert_eq!(ct_lookup(&table, 127).unwrap(), TaggedValue::fixnum(16_383));
}

#[test]
fn mapped_char_table_fields_and_extras_mutation_follows_collector_contract() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let (table, _) = image(&mut heap, SlotStorage::Owned);
    launch_image_scan(&mut heap, table);
    for index in 0..4_096 {
        let value = TaggedValue::fixnum(index);
        builtin_set_char_table_range(vec![table, TaggedValue::NIL, value], None).unwrap();
        builtin_set_char_table_range(vec![table, TaggedValue::T, value], None).unwrap();
        builtin_set_char_table_extra_slot(vec![table, TaggedValue::fixnum(1), value]).unwrap();
    }
    heap.join_concurrent_mark();
    let obj = table.as_char_table_obj().unwrap();
    assert_eq!(obj.defalt.get(), TaggedValue::fixnum(4_095));
    assert_eq!(obj.ascii.get(), TaggedValue::fixnum(4_095));
    assert!(
        obj.contents
            .iter()
            .all(|value| value.get() == TaggedValue::fixnum(4_095))
    );
    assert_eq!(obj.extras[1], TaggedValue::fixnum(4_095));
}

#[test]
fn mapped_char_table_backing_promotion_retains_capture_storage() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let (table, backing) = image(&mut heap, SlotStorage::Mapped);
    launch_image_scan(&mut heap, table);
    for index in 0..16_384 {
        builtin_set_char_table_range(
            vec![
                table,
                TaggedValue::fixnum(index % 128),
                TaggedValue::fixnum(index),
            ],
            None,
        )
        .unwrap();
        builtin_set_char_table_extra_slot(vec![
            table,
            TaggedValue::fixnum(1),
            TaggedValue::fixnum(index),
        ])
        .unwrap();
    }
    heap.join_concurrent_mark();
    assert_eq!(ct_lookup(&table, 127).unwrap(), TaggedValue::fixnum(16_383));
    assert_eq!(
        table.as_char_table_obj().unwrap().extras[1],
        TaggedValue::fixnum(16_383)
    );
    assert!(backing.child_slots.iter().all(|value| value.is_nil()));
    assert!(backing.extra_slots.iter().all(|value| value.is_nil()));
}

#[test]
fn mapped_chartable_promotion_retains_start_graph_heap_children() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let tail = heap.alloc_cons(TaggedValue::fixnum(23), TaggedValue::NIL);
    let head = heap.alloc_cons(TaggedValue::fixnum(19), tail);
    let unreachable = heap.alloc_cons(TaggedValue::fixnum(99), TaggedValue::NIL);
    let (table, backing) = image_with_child(&mut heap, SlotStorage::Mapped, head);
    launch_image_scan(&mut heap, table);

    // Promote the backing, then remove EVERY live-table reference to the old
    // heap graph. Neither head nor tail is an explicit root: the admitted
    // start graph and owner barrier must keep both alive through this cycle.
    for character in 0..128 {
        builtin_set_char_table_range(
            vec![
                table,
                TaggedValue::fixnum(character),
                TaggedValue::fixnum(7),
            ],
            None,
        )
        .unwrap();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(table);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    heap.finish_first_partition_cycle();

    assert!(heap.owns_heap_value_for_test(head));
    assert!(heap.owns_heap_value_for_test(tail));
    assert!(!heap.owns_heap_value_for_test(unreachable));
    assert_eq!(head.cons_car(), TaggedValue::fixnum(19));
    assert_eq!(head.cons_cdr(), tail);
    assert_eq!(tail.cons_car(), TaggedValue::fixnum(23));
    assert!(backing.child_slots.iter().all(|value| *value == head));
    for character in 0..128 {
        assert_eq!(
            ct_lookup(&table, character).unwrap(),
            TaggedValue::fixnum(7)
        );
    }
}

#[test]
fn readable_subtable_outside_its_span_preserves_optional_slot_policy() {
    use crate::emacs_core::chartable::{
        make_char_table_from_external_slots, make_sub_char_table_from_external_slots,
    };

    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    // Both external constructors accept this placement. The top content
    // slot normally spans 65536 characters, but its depth-three child has
    // only 128 slots. Character128 is valid input, outside that child.
    let mut child_fields = vec![TaggedValue::fixnum(3), TaggedValue::fixnum(0)];
    child_fields.extend(std::iter::repeat_n(TaggedValue::NIL, 128));
    let child = make_sub_char_table_from_external_slots(&child_fields).unwrap();
    // DEFAULT, PARENT, PURPOSE, ASCII, followed by the top content slots.
    let mut table_fields = vec![TaggedValue::NIL; 4];
    table_fields.push(child);
    table_fields.extend(std::iter::repeat_n(
        TaggedValue::NIL,
        CHAR_TABLE_TOP_SLOTS - 1,
    ));
    let table = make_char_table_from_external_slots(&table_fields).unwrap();

    assert_eq!(
        builtin_set_char_table_range(
            vec![table, TaggedValue::fixnum(128), TaggedValue::fixnum(9)],
            None,
        )
        .unwrap(),
        TaggedValue::fixnum(9),
    );
    assert!(
        child
            .as_sub_char_table_obj()
            .unwrap()
            .contents
            .iter()
            .all(|slot| slot.is_nil())
    );
    assert_eq!(ct_lookup(&table, 128).unwrap(), TaggedValue::NIL);
    // The caller's bounds policy does not drop valid stores.
    builtin_set_char_table_range(
        vec![table, TaggedValue::fixnum(9), TaggedValue::fixnum(11)],
        None,
    )
    .unwrap();
    assert_eq!(
        child.as_sub_char_table_obj().unwrap().contents[9],
        TaggedValue::fixnum(11)
    );
}

/// Keep a marker's raw cell provenance across the real fixed-slot capability.
/// No Context or bootstrap obarray is needed for this storage regression.
#[test]
fn miriub3_char_table_fixed_slots_keep_cell_provenance() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let table = heap.alloc_char_table(TaggedValue::NIL, TaggedValue::NIL, 1);
    let object = table.as_veclike_ptr().unwrap().cast::<CharTableObj>();
    // SAFETY: the live table owns these initialized fixed cells until heap drop.
    // Raw field projection retains the allocation's provenance across stores.
    let slots = unsafe {
        [
            std::ptr::addr_of!((*object).defalt),
            std::ptr::addr_of!((*object).parent),
            std::ptr::addr_of!((*object).purpose),
            std::ptr::addr_of!((*object).ascii),
            std::ptr::addr_of!((*object).contents).cast::<AtomicTaggedValue>(),
            std::ptr::addr_of!((*object).contents)
                .cast::<AtomicTaggedValue>()
                .add(63),
        ]
    };
    for slot in slots {
        // SAFETY: the retained pointers address the actual cells, not owner views.
        assert_eq!(unsafe { &*slot }.load_atomic(), TaggedValue::NIL);
    }
    table
        .with_char_table_mut(|writer| {
            writer.set_default(TaggedValue::fixnum(11));
            writer.set_parent(TaggedValue::NIL);
            writer.set_purpose(TaggedValue::NIL);
            writer.set_ascii(TaggedValue::fixnum(13));
            writer.set_contents(0, TaggedValue::fixnum(17));
            writer.set_contents(63, TaggedValue::fixnum(19));
            writer.set_extra(0, TaggedValue::fixnum(23));
        })
        .unwrap();
    let expected = [11, 0, 0, 13, 17, 19];
    for (index, slot) in slots.into_iter().enumerate() {
        // SAFETY: fixed-shape stores neither move nor uniquely borrow the cells.
        let value = unsafe { &*slot }.load_atomic();
        assert_eq!(
            value,
            if index == 1 || index == 2 {
                TaggedValue::NIL
            } else {
                TaggedValue::fixnum(expected[index])
            }
        );
    }
    assert_eq!(
        table.as_char_table_obj().unwrap().extras[0],
        TaggedValue::fixnum(23)
    );
}
