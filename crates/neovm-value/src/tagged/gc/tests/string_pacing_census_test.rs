//! GNU alloc.c's percentage census excludes immortal dump storage.
use super::*;
use crate::heap_types::LispString;
use crate::tagged::header::{ConsCdrWord, ConsWord};
use crate::value::{HashTableTest, LispHashTable};

fn full(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_stw_collection();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_collection();
}

#[test]
fn string_gc_pacing_counts_collectible_survivors_not_image_residents() {
    for generational in [false, true] {
        for deferred_first in [false, true] {
            let mut heap = TaggedHeap::new();
            // SAFETY: this fixture owns the only heap view before publication.
            unsafe { heap.set_generational_enabled(generational) };
            set_tagged_heap(&mut heap);
            // The boxes stand in for loader-owned mapped storage. They remain
            // allocated until after the heap drops its mapped registries.
            let mapped = Box::into_raw(Box::new(StringObj {
                header: GcHeader::new(HeapObjectKind::String),
                data: LispString::from_utf8(&"mapped".repeat(1024)),
            }));
            let mapped_cons = Box::into_raw(Box::new([ConsCell {
                car: ConsWord::new(TaggedValue::NIL),
                cdr_or_next: ConsCdrWord::new(TaggedValue::NIL),
            }]));
            // SAFETY: initialized, stable fixture storage outlives this heap.
            unsafe {
                heap.register_mapped_string_object(mapped, size_of::<StringObj>());
                heap.register_mapped_cons_range(mapped_cons.cast(), 1);
            }
            assert_eq!(heap.live_bytes(), 0, "mapping is not collectible consing");

            let runtime = heap.alloc_string(LispString::from_utf8(&"r".repeat(4096)));
            let image = heap.alloc_string(LispString::from_utf8(&"i".repeat(1024 * 1024)));
            let image_box = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
            heap.note_image_object(image);
            heap.note_image_object(image_box);
            // SAFETY: mapped points to an initialized StringObj whose stable
            // fixture storage outlives this heap and every use of this value.
            let mapped_value = unsafe { TaggedValue::from_string_ptr(mapped) };
            let roots = [runtime, image, image_box, mapped_value];
            let allocation_counts = heap.memory_use_counts_snapshot();
            let expected = size_of::<StringObj>() + 4096;

            if deferred_first {
                heap.arm_first_cycle_concurrent();
                heap.concurrent_begin();
                for &root in &roots {
                    heap.seed_root(root);
                }
                heap.launch_concurrent_mark();
                while !heap.concurrent_mark_done() {
                    std::thread::yield_now();
                }
                heap.join_concurrent_mark();
                heap.reseed_runtime_and_remembered_roots();
                for &root in &roots {
                    heap.seed_root(root);
                }
                heap.incremental_drain_all();
                heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
                heap.finish_incremental_sweep_now();
                heap.finish_first_partition_cycle();
            } else {
                full(&mut heap, &roots);
            }
            assert_eq!(heap.live_bytes(), expected, "first image classification");
            assert_eq!(heap.gc_object_counts().strings_used, 3);
            assert_eq!(heap.memory_use_counts_snapshot(), allocation_counts);
            let layout = heap.layout_stats();
            assert_eq!(layout.mapped.strings, 1);
            assert_eq!(layout.mapped.conses, 1);
            assert!(layout.mapped.object_image_bytes >= size_of::<StringObj>() + 16);
            assert!(layout.managed_live_bytes >= expected + layout.mapped.object_image_bytes);

            // Ordinary-old runtime storage remains collectible and contributes
            // to both eager major and deferred minor percentage budgets.
            full(&mut heap, &roots);
            assert_eq!(heap.live_bytes(), expected, "ordinary-old full census");
            if generational {
                heap.begin_minor_collection().unwrap();
                for &root in &roots {
                    heap.seed_root(root);
                }
                heap.complete_minor_collection();
                heap.finish_incremental_sweep_now();
                assert_eq!(heap.live_bytes(), expected, "ordinary-old minor census");
            }
            assert_eq!(heap.memory_use_counts_snapshot(), allocation_counts);
            full(&mut heap, &[image, image_box, mapped_value]);
            assert_eq!(heap.live_bytes(), 0, "unrooted runtime string reclaimed");
            assert_eq!(heap.gc_object_counts().strings_used, 2);
            drop(heap);
            // SAFETY: the heap no longer retains or inspects these mappings.
            unsafe {
                drop(Box::from_raw(mapped));
                drop(Box::from_raw(mapped_cons));
            }
        }
    }
}
