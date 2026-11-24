//! GNU alloc.c's percentage census excludes immortal dump storage.
use super::*;
use crate::heap_types::LispString;
use crate::tagged::header::{ConsCdrWord, ConsWord};
use crate::value::{HashTableTest, LispHashTable};

fn full(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.request_gc_object_counts();
    heap.begin_stw_collection_for_test();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_collection_for_test();
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
                heap.incremental_finish_for_test(heap.live_bytes(), std::time::Instant::now());
                heap.finish_incremental_sweep_now_for_test();
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
                heap.begin_minor_collection_for_test().unwrap();
                for &root in &roots {
                    heap.seed_root(root);
                }
                heap.complete_minor_collection_for_test();
                heap.finish_incremental_sweep_now_for_test();
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
fn string_gc_typed_published_edits_charge_only_new_backing_extents() {
    let mut observed = Vec::new();
    for mode in [
        "owned fixed edit",
        "owned fixed unwind",
        "borrowed fixed edit",
        "borrowed fixed unwind",
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
            "owned fixed edit" | "borrowed fixed edit" => {
                crate::tagged::mutate::with_string_bytes_mut(value, |bytes| bytes[0] = b'z')
                    .unwrap();
            }
            "owned fixed unwind" | "borrowed fixed unwind" => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::tagged::mutate::with_string_bytes_mut(value, |bytes| {
                        bytes[0] = b'z';
                        panic!("expected typed string edit unwind");
                    });
                }));
                assert!(result.is_err());
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
            if mode.starts_with("borrowed") {
                storage.payload_capacity_bytes
            } else {
                0
            },
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
        "typed edits bill borrowed materialization once and owned edits zero: {observed:?}"
    );
}

#[test]
fn string_gc_typed_byte_edits_charge_only_materialization_and_restore_shape() {
    use crate::tagged::mutate::{
        clear_string_contents, clear_string_intervals, pin_string_immovable, with_string_bytes_mut,
    };

    for borrowed in [false, true] {
        for unwind in [false, true] {
            let mut heap = TaggedHeap::new();
            set_tagged_heap(&mut heap);
            let string = if borrowed {
                LispString::from_rodata_unibyte(b"abc\0")
            } else {
                LispString::from_unibyte(b"abc".to_vec())
            };
            let value = heap.alloc_string(string);
            let before = heap.bytes_since_gc_exact();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_string_bytes_mut(value, |bytes| {
                    bytes.copy_from_slice(b"xyz");
                    if unwind {
                        panic!("expected typed byte edit unwind");
                    }
                })
                .unwrap();
            }));
            assert_eq!(result.is_err(), unwind);
            let string = value.as_lisp_string().unwrap();
            assert_eq!(string.as_bytes(), b"xyz");
            assert_eq!(string.schars(), 3);
            assert_eq!(string.sbytes(), 3);
            // SAFETY: every live string includes its initialized trailing NUL.
            assert_eq!(unsafe { *string.data().add(string.sbytes()) }, 0);
            let allocated = if borrowed { string.owned_capacity() } else { 0 };
            assert_eq!(heap.bytes_since_gc_exact() - before, allocated);
            let charged = heap.bytes_since_gc_exact();
            with_string_bytes_mut(value, |bytes| bytes.fill(b'a')).unwrap();
            // No intervals: GNU `set_text_properties` returns nil and does not
            // touch the string. The charge check below is the pacing claim.
            assert!(!clear_string_intervals(value));
            assert!(pin_string_immovable(value));
            assert_eq!(heap.bytes_since_gc_exact(), charged);
            assert!(value.as_lisp_string().unwrap().is_immovable());
            full(&mut heap, &[value]);
            assert_eq!(heap.live_bytes(), size_of::<StringObj>() + 3);
        }
    }

    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let multibyte = heap.alloc_string(LispString::from_utf8("é"));
    let before = heap.bytes_since_gc_exact();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_string_bytes_mut(multibyte, |bytes| {
            bytes.copy_from_slice(b"aa");
            panic!("expected multibyte shape unwind");
        });
    }));
    assert!(result.is_err());
    let string = multibyte.as_lisp_string().unwrap();
    assert_eq!(string.schars(), 2, "fixed bytes may change character count");
    assert_eq!(string.sbytes(), 2);
    assert!(string.is_multibyte());
    // SAFETY: the fixed-size mutation never exposes or removes this NUL.
    assert_eq!(unsafe { *string.data().add(2) }, 0);
    assert_eq!(heap.bytes_since_gc_exact(), before);

    let borrowed = heap.alloc_string(LispString::from_rodata_unibyte(b"pin\0"));
    let before = heap.bytes_since_gc_exact();
    assert!(pin_string_immovable(borrowed));
    assert_eq!(
        heap.bytes_since_gc_exact() - before,
        borrowed.as_lisp_string().unwrap().owned_capacity()
    );
    let before = heap.bytes_since_gc_exact();
    assert!(clear_string_contents(borrowed));
    assert_eq!(
        heap.bytes_since_gc_exact(),
        before,
        "pinned clear is in place"
    );
    assert_eq!(borrowed.as_lisp_string().unwrap().as_bytes(), b"\0\0\0");

    let large = heap.alloc_string(LispString::from_utf8(&"x".repeat(8192)));
    let before = heap.bytes_since_gc_exact();
    assert!(clear_string_contents(large));
    let string = large.as_lisp_string().unwrap();
    assert_eq!(
        heap.bytes_since_gc_exact() - before,
        string.owned_capacity()
    );
    assert!(string.as_bytes().iter().all(|byte| *byte == 0));
    assert!(!string.is_multibyte());
}
