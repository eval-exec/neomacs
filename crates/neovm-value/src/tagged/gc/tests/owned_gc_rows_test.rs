//! GNU GC rows describe owned sweep survivors, excluding dump residents.
use super::*;
use crate::heap_types::LispString;
use crate::tagged::header::{ConsCdrWord, ConsWord};

struct MappedFixture {
    cons: Box<[ConsCell; 1]>,
    string: Box<StringObj>,
    vector: Box<VectorObj>,
    float: Box<FloatObj>,
}

impl MappedFixture {
    fn new() -> Self {
        Self {
            cons: Box::new([ConsCell {
                car: ConsWord::new(TaggedValue::NIL),
                cdr_or_next: ConsCdrWord::new(TaggedValue::NIL),
            }]),
            string: Box::new(StringObj {
                header: GcHeader::new(HeapObjectKind::String),
                data: LispString::from_utf8(&"m".repeat(777)),
            }),
            vector: Box::new(VectorObj {
                header: VecLikeHeader::new(VecLikeType::Vector),
                data: vec![TaggedValue::NIL; 10000].into(),
            }),
            float: Box::new(FloatObj {
                header: GcHeader::new(HeapObjectKind::Float),
                value: 23.5,
            }),
        }
    }

    fn register(&mut self, heap: &mut TaggedHeap) -> [TaggedValue; 4] {
        let cons = self.cons.as_mut_ptr();
        let string = &mut *self.string as *mut StringObj;
        let vector = &mut *self.vector as *mut VectorObj;
        let float = &mut *self.float as *mut FloatObj;
        // SAFETY: initialized, aligned fixture allocations remain stable and
        // live until after this heap drops all four mapped registries.
        unsafe {
            heap.register_mapped_cons_range(cons, 1);
            heap.register_mapped_string_object(string, size_of::<StringObj>());
            heap.register_mapped_veclike_object(vector.cast(), size_of::<VectorObj>());
            heap.register_mapped_float_range(float, 1);
            [
                TaggedValue::from_cons_ptr(cons),
                TaggedValue::from_string_ptr(string),
                TaggedValue::from_veclike_ptr(vector.cast()),
                TaggedValue::from_float_ptr(float),
            ]
        }
    }
}

fn full(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.request_gc_object_counts();
    heap.begin_stw_collection_for_test();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_collection_for_test();
    heap.publish_gc_object_counts_if_requested(0);
}

fn deferred_first(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.request_gc_object_counts();
    heap.arm_first_cycle_concurrent();
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.incremental_drain_all();
    heap.incremental_finish_for_test(heap.live_bytes(), std::time::Instant::now());
    heap.finish_incremental_sweep_now_for_test();
    heap.finish_first_partition_cycle_for_test();
    heap.publish_gc_object_counts_if_requested(0);
}

fn assert_runtime_rows(rows: GcObjectCounts) {
    assert_eq!(rows.conses_used, 1);
    assert_eq!(rows.strings_used, 1);
    assert_eq!(
        rows.string_bytes, 87,
        "logical bytes, without sdata padding"
    );
    assert_eq!(rows.vectors_used, 1);
    assert_eq!(
        rows.vector_slots_used, 10001,
        "large vector plus header word"
    );
    assert_eq!(rows.floats_used, 1);
    assert_eq!(
        rows.symbols_used, 0,
        "fixture supplied an empty symbol census"
    );
}

#[test]
fn owned_gc_rows_exclude_mapped_and_loader_image_residents() {
    for generational in [false, true] {
        for deferred in [false, true] {
            let mut mapped = MappedFixture::new();
            let mut raw = TaggedHeap::new();
            // SAFETY: construction precedes publication or another heap view.
            unsafe { raw.set_generational_enabled(generational) };
            let mut heap = TestHeap::from_heap(raw);
            let mapped_values = mapped.register(&mut heap);
            let image_string = heap.alloc_string(LispString::from_utf8(&"i".repeat(777)));
            let image_vector = heap.alloc_vector(vec![TaggedValue::NIL; 10000]);
            let image_float = heap.alloc_float(17.25);
            for image in [image_string, image_vector, image_float] {
                heap.note_image_object(image);
            }
            let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
            let string = heap.alloc_string(LispString::from_utf8(&"r".repeat(87)));
            let vector = heap.alloc_vector(vec![TaggedValue::NIL; 10000]);
            let float = heap.alloc_float(4.5);
            let mut roots = mapped_values.to_vec();
            roots.extend([
                image_string,
                image_vector,
                image_float,
                cons,
                string,
                vector,
                float,
            ]);
            if deferred {
                deferred_first(&mut heap, &roots);
            } else {
                full(&mut heap, &roots);
            }
            assert_runtime_rows(heap.gc_object_counts());
            full(&mut heap, &roots);
            assert_runtime_rows(heap.gc_object_counts());
            if generational {
                heap.request_gc_object_counts();
                heap.begin_minor_collection_for_test().unwrap();
                for &root in &roots {
                    heap.seed_root(root);
                }
                heap.complete_minor_collection_for_test();
                heap.finish_incremental_sweep_now_for_test();
                heap.publish_gc_object_counts_if_requested(0);
                assert_runtime_rows(heap.gc_object_counts());
            }

            // A row read is the completed collection's snapshot. Fresh
            // allocations do not alter it until another collection publishes.
            let snapshot = heap.gc_object_counts();
            let _fresh = [
                heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL),
                heap.alloc_string(LispString::from_utf8("uncollected")),
                heap.alloc_vector(vec![TaggedValue::NIL; 3]),
                heap.alloc_float(8.25),
            ];
            assert_eq!(heap.gc_object_counts(), snapshot);
            full(&mut heap, &roots);
            assert_runtime_rows(heap.gc_object_counts());
            drop(heap);
            drop(mapped);
        }
    }
}

#[test]
fn owned_gc_rows_do_not_rescan_allocations_after_publication() {
    let mut heap = TestHeap::new();
    let live = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    full(&mut heap, &[live]);
    let before = heap.gc_object_counts();
    assert_eq!(before.conses_used, 1);
    let _uncollected = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    assert_eq!(
        heap.gc_object_counts(),
        before,
        "USED and FREE are one frozen snapshot"
    );
    full(&mut heap, &[live]);
    assert_eq!(heap.gc_object_counts().conses_used, 1);
}

#[test]
fn owned_gc_rows_do_not_count_retired_image_page_slots_as_free() {
    let mut mapped = MappedFixture::new();
    let mut heap = TestHeap::new();
    let mapped_values = mapped.register(&mut heap);
    let mut roots = mapped_values.to_vec();
    for _ in 0..ObjectPage::<StringObj>::SLOTS {
        let image = heap.alloc_string(LispString::from_utf8("image"));
        heap.note_image_object(image);
        roots.push(image);
    }
    full(&mut heap, &roots);
    assert!(heap.string_arena.page_retired_for_test(0));
    let rows = heap.gc_object_counts();
    assert_eq!(rows.strings_used, 0);
    assert_eq!(rows.string_bytes, 0);
    assert_eq!(rows.strings_free, 0, "retired image slots cannot be reused");
    drop(heap);
    drop(mapped);
}
