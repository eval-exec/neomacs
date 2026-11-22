//! GNU alloc.c's percentage census excludes immortal dump storage.
use super::*;
use crate::heap_types::LispString;
use crate::tagged::header::{ConsCdrWord, ConsWord};
use crate::value::{HashTableTest, LispHashTable};

fn full(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.request_gc_object_counts();
    heap.begin_stw_collection();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_collection();
    heap.publish_gc_object_counts_if_requested(0);
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
                heap.request_gc_object_counts();
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
                heap.publish_gc_object_counts_if_requested(0);
            } else {
                full(&mut heap, &roots);
            }
            assert_eq!(heap.live_bytes(), expected, "first image classification");
            assert_eq!(heap.gc_object_counts().strings_used, 1);
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
            assert_eq!(heap.gc_object_counts().strings_used, 0);
            drop(heap);
            // SAFETY: the heap no longer retains or inspects these mappings.
            unsafe {
                drop(Box::from_raw(mapped));
                drop(Box::from_raw(mapped_cons));
            }
        }
    }
}

#[test]
fn string_gc_allocation_charge_is_distinct_from_live_census() {
    // GNU alloc.c:1701/1741/1818 charges allocation storage, whereas
    // sweep_strings:1862-1863 publishes logical bytes for the live census.
    let mut reserved = Vec::with_capacity(512);
    reserved.extend_from_slice(b"abc");
    for (label, string) in [
        ("owned spare capacity", LispString::from_unibyte(reserved)),
        (
            "borrowed payload",
            LispString::from_rodata_unibyte(b"abc\0"),
        ),
    ] {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        let before = heap.bytes_since_gc_exact();
        let value = heap.alloc_string(string);
        let charged = heap.bytes_since_gc_exact() - before;
        heap.close_alloc_regions();
        let layout = heap.layout_stats();
        let storage = layout
            .arenas
            .iter()
            .find(|arena| arena.class == "string")
            .unwrap();
        assert_eq!(storage.allocated_slots, 1);
        assert_eq!(
            charged,
            storage.occupied_slot_bytes + storage.payload_capacity_bytes,
            "allocation charge must include the allocated extent: {label}"
        );
        let counts = heap.memory_use_counts_snapshot();
        full(&mut heap, &[value]);
        assert_eq!(
            heap.live_bytes(),
            size_of::<StringObj>() + 3,
            "allocation capacity must not inflate the logical live census: {label}"
        );
        assert_eq!(heap.gc_object_counts().string_bytes, 3);
        assert_eq!(heap.memory_use_counts_snapshot(), counts);
    }
}

#[test]
fn string_gc_published_backing_changes_charge_the_new_extent_once() {
    let mut observed = Vec::new();
    for mode in [
        "owned growth",
        "owned replacement",
        "owned replacement unwind",
        "borrowed generic COW",
        "borrowed byte COW",
        "borrowed in-store COW",
    ] {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        let string = if mode.starts_with("borrowed") {
            LispString::from_rodata_unibyte(b"abc\0")
        } else {
            LispString::from_utf8("abc")
        };
        let value = heap.alloc_string(string);
        let before = heap.bytes_since_gc_exact();
        match mode {
            "owned growth" => {
                crate::tagged::mutate::with_lisp_string_mut(value, |string| {
                    string.mutate_bytes(|bytes| bytes.reserve_exact(512));
                })
                .unwrap();
            }
            "owned replacement" => {
                crate::tagged::mutate::with_lisp_string_mut(value, |string| {
                    *string = LispString::from_utf8("abc");
                })
                .unwrap();
            }
            "owned replacement unwind" => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::tagged::mutate::with_lisp_string_mut(value, |string| {
                        *string = LispString::from_utf8("abc");
                        panic!("expected string backing unwind");
                    });
                }));
                assert!(result.is_err());
            }
            "borrowed generic COW" => {
                crate::tagged::mutate::with_lisp_string_mut(value, |string| {
                    string.mutate_bytes(|bytes| bytes[0] = b'z');
                })
                .unwrap();
            }
            "borrowed byte COW" => {
                assert!(crate::tagged::mutate::set_string_byte_same_char_count(
                    value, 0, b'z',
                ));
            }
            "borrowed in-store COW" => {
                assert!(with_store_state(|store| {
                    crate::tagged::mutate::set_string_byte_same_char_count_in_store(
                        store, value, 0, b'z',
                    )
                }));
            }
            _ => unreachable!(),
        }
        heap.close_alloc_regions();
        let layout = heap.layout_stats();
        let storage = layout
            .arenas
            .iter()
            .find(|arena| arena.class == "string")
            .unwrap();
        assert!(storage.payload_capacity_bytes > 0);
        observed.push((
            mode,
            heap.bytes_since_gc_exact() - before,
            storage.payload_capacity_bytes,
        ));
        let charged = heap.bytes_since_gc_exact();
        assert!(crate::tagged::mutate::set_string_byte_same_char_count(
            value, 0, b'x',
        ));
        assert_eq!(
            heap.bytes_since_gc_exact(),
            charged,
            "owned byte store: {mode}"
        );
        full(&mut heap, &[value]);
        assert_eq!(heap.live_bytes(), size_of::<StringObj>() + 3, "{mode}");
        assert_eq!(heap.gc_object_counts().string_bytes, 3, "{mode}");
    }
    assert!(
        observed
            .iter()
            .all(|(_, charged, extent)| charged == extent),
        "each newly published backing extent must be charged exactly once: {observed:?}"
    );
}
