//! Runtime-owned hash backing remains in GNU's census even for dump objects.
use super::*;
use crate::value::{HashKey, HashTableTest, LispHashTable};

fn table(entries: i64) -> LispHashTable {
    let mut table = LispHashTable::new_with_options(HashTableTest::Eq, 0, None, 1.5, 0.8125);
    insert(&mut table, 0, entries);
    table
}

fn insert(table: &mut LispHashTable, start: i64, end: i64) {
    for key in start..end {
        let value = TaggedValue::fixnum(key);
        table.insert(HashKey::Int(key), value, value);
    }
}

fn backing(heap: &TaggedHeap, value: TaggedValue) -> usize {
    let ptr = value
        .as_veclike_ptr()
        .unwrap()
        .cast_mut()
        .cast::<HashTableObj>();
    // SAFETY: the fixture retains this initialized owned or mapped table;
    // no mutator, safepoint, or collection runs during this storage read.
    unsafe {
        heap.collector_tables().inspect(ptr, |table| {
            let bytes = table.data.materialized_storage_bytes();
            if table.data.pending_entries().is_none() {
                assert_eq!(
                    bytes,
                    table.data.sealed_storage_bytes(),
                    "fixnum-only eq table"
                );
            }
            bytes
        })
    }
}

fn full(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_stw_collection();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_collection();
}

fn deferred_first(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
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
    heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    heap.finish_first_partition_cycle();
}

#[test]
fn image_hash_pacing_counts_owned_backing_before_and_after_promotion() {
    for generational in [false, true] {
        for deferred in [false, true] {
            let mut raw = TaggedHeap::new();
            // SAFETY: construction precedes publication and any other heap view.
            unsafe { raw.set_generational_enabled(generational) };
            let mut heap = TestHeap::from_heap(raw);
            full(&mut heap, &[]);
            let baseline = heap.live_bytes();
            let runtime = heap.alloc_hash_table(table(256));
            let image = heap.alloc_hash_table(table(256));
            assert_eq!(heap.note_image_object(image), ImageStamp::Stamped);
            let mapped = Box::into_raw(Box::new(HashTableObj {
                header: VecLikeHeader::new(VecLikeType::HashTable),
                table: table(256),
            }));
            // SAFETY: initialized stable fixture storage remains allocated
            // until after the heap has dropped its mapped registries.
            let mapped_value = unsafe {
                heap.register_mapped_veclike_object(mapped.cast(), size_of::<HashTableObj>());
                TaggedValue::from_veclike_ptr(mapped.cast())
            };
            let roots = [runtime, image, mapped_value];
            let runtime_bytes = size_of::<HashTableObj>() + backing(&heap, runtime);
            let image_bytes = backing(&heap, image);
            let mapped_bytes = backing(&heap, mapped_value);
            assert!(image_bytes > 0 && mapped_bytes > 0);
            let expected = baseline + runtime_bytes + image_bytes + mapped_bytes;
            if deferred {
                deferred_first(&mut heap, &roots);
            } else {
                full(&mut heap, &roots);
            }
            assert_eq!(heap.live_bytes(), expected, "first image classification");
            full(&mut heap, &roots);
            assert_eq!(
                heap.live_bytes(),
                expected,
                "permanent image backing survives"
            );

            image
                .with_hash_table_mut(|table| insert(table, 256, 1024))
                .unwrap();
            let grown_image_bytes = backing(&heap, image);
            assert!(grown_image_bytes > image_bytes);
            full(&mut heap, &roots);
            let grown_expected = baseline + runtime_bytes + grown_image_bytes + mapped_bytes;
            assert_eq!(heap.live_bytes(), grown_expected, "grown dump backing");
            if generational {
                heap.begin_minor_collection().unwrap();
                for &root in &roots {
                    heap.seed_root(root);
                }
                heap.complete_minor_collection();
                heap.finish_incremental_sweep_now();
                assert_eq!(heap.live_bytes(), grown_expected, "minor image census");
            }
            full(&mut heap, &[image, mapped_value]);
            assert_eq!(
                heap.live_bytes(),
                baseline + grown_image_bytes + mapped_bytes,
                "runtime header and backing are collected; image headers never enter the census"
            );
            drop(heap);
            // SAFETY: the heap no longer retains this fixture-owned mapping.
            unsafe { drop(Box::from_raw(mapped)) };
        }
    }
}

#[test]
fn image_hash_pacing_serial_growth_is_charged_once() {
    for inactive_entry in [false, true] {
        let mut heap = TestHeap::new();
        let image = heap.alloc_hash_table(table(0));
        assert_eq!(heap.note_image_object(image), ImageStamp::Stamped);
        let before = backing(&heap, image);
        assert_eq!(
            before, 0,
            "unmaterialized image entries are not runtime backing"
        );
        full(&mut heap, &[image]);
        assert_eq!(
            heap.live_bytes(),
            0,
            "untouched image table contributes no backing"
        );
        heap.reset_bytes_since_gc();
        if inactive_entry {
            // SAFETY: this fixture runs serially without an active Tier-H mark,
            // callbacks or collection inside the mutation closure.
            unsafe { image.with_hash_table_mut_inactive(|table| insert(table, 0, 256)) }.unwrap();
        } else {
            image
                .with_hash_table_mut(|table| insert(table, 0, 256))
                .unwrap();
        }
        let after = backing(&heap, image);
        assert!(after > before);
        assert_eq!(heap.bytes_since_gc_exact(), after - before);
        // Existing keys change neither backing capacity nor the charge.
        image
            .with_hash_table_mut(|table| insert(table, 0, 256))
            .unwrap();
        assert_eq!(backing(&heap, image), after);
        assert_eq!(heap.bytes_since_gc_exact(), after - before);
        full(&mut heap, &[image]);
        assert_eq!(heap.live_bytes(), after, "hydrated backing counts once");
    }
}

#[test]
fn image_hash_pacing_lazy_hydration_is_charged_once() {
    for mutable in [false, true] {
        let mut heap = TestHeap::new();
        let mut pending = table(0);
        pending.set_pending_dump_entries(
            (0..256)
                .map(|key| {
                    let value = TaggedValue::fixnum(key);
                    (HashKey::Int(key), value, Some(value))
                })
                .collect(),
        );
        let image = heap.alloc_hash_table(pending);
        assert_eq!(heap.note_image_object(image), ImageStamp::Stamped);
        let before = backing(&heap, image);
        assert_eq!(before, 0, "pending image entries remain outside the census");
        full(&mut heap, &[image]);
        assert_eq!(
            heap.live_bytes(),
            0,
            "untouched image table contributes no backing"
        );
        heap.reset_bytes_since_gc();
        if mutable {
            image.with_hash_table_mut(|_| ()).unwrap();
        } else {
            assert_eq!(image.as_hash_table().unwrap().data.len(), 256);
        }
        let after = backing(&heap, image);
        assert!(after > before, "hydration materializes runtime backing");
        assert_eq!(heap.bytes_since_gc_exact(), after - before);
        // A second access has no hydration allocation and must not charge again.
        if mutable {
            image.with_hash_table_mut(|_| ()).unwrap();
        } else {
            assert_eq!(image.as_hash_table().unwrap().data.len(), 256);
        }
        assert_eq!(backing(&heap, image), after);
        assert_eq!(heap.bytes_since_gc_exact(), after - before);
        full(&mut heap, &[image]);
        assert_eq!(
            heap.live_bytes(),
            after,
            "hydrated image backing counts once"
        );
    }
}

#[test]
#[cfg(target_pointer_width = "64")]
fn image_hash_pacing_pending_replacement_keeps_one_net_charge_on_unwind() {
    // A size-3 table owns three 40-byte index entries and three 24-byte
    // Option<HashTableEntry> slots on the supported 64-bit layout. This
    // expected extent is independent of the pacing calculation under test.
    const REPLACEMENT_BYTES: usize = 3 * (40 + 24);
    for inactive_entry in [false, true] {
        for unwind in [false, true] {
            let mut heap = TestHeap::new();
            let mut pending = table(0);
            pending.set_pending_dump_entries(
                (0..1024)
                    .map(|key| {
                        let value = TaggedValue::fixnum(key);
                        (HashKey::Int(key), value, Some(value))
                    })
                    .collect(),
            );
            let image = heap.alloc_hash_table(pending);
            assert_eq!(heap.note_image_object(image), ImageStamp::Stamped);
            assert_eq!(backing(&heap, image), 0, "pending starts unmaterialized");
            heap.reset_bytes_since_gc();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let replace = |data: &mut LispHashTable| {
                    assert!(data.data.pending_entries().is_none());
                    assert_eq!(data.data.len(), 1024, "the callback sees hydration");
                    *data =
                        LispHashTable::new_with_options(HashTableTest::Eq, 3, None, 1.5, 0.8125);
                    if unwind {
                        panic!("after replacing hydrated backing with a smaller table");
                    }
                };
                if inactive_entry {
                    // SAFETY: sole serial fixture owner; the callback neither
                    // calls Lisp nor collects, including before its unwind.
                    unsafe { image.with_hash_table_mut_inactive(replace) }.unwrap();
                } else {
                    image.with_hash_table_mut(replace).unwrap();
                }
            }));
            assert_eq!(outcome.is_err(), unwind);
            assert_eq!(
                heap.bytes_since_gc_exact(),
                REPLACEMENT_BYTES,
                "one initial/final net charge, not hydration plus replacement"
            );
            assert!(
                !crate::tagged::mutate::in_heap_mut_closure_for_test(),
                "the mutation extent ends before accounting, also on unwind"
            );
            assert_eq!(image.as_hash_table().unwrap().data.len(), 0);
        }
    }
}

#[test]
fn image_hash_pacing_minor_first_excludes_stamped_header_and_pending() {
    for pending in [false, true] {
        let mut raw = TaggedHeap::new();
        // SAFETY: construction precedes publication and any other heap view.
        unsafe { raw.set_generational_enabled(true) };
        let mut heap = TestHeap::from_heap(raw);
        full(&mut heap, &[]);
        let mut data = table(0);
        if pending {
            data.set_pending_dump_entries(
                (0..256)
                    .map(|key| {
                        let value = TaggedValue::fixnum(key);
                        (HashKey::Int(key), value, Some(value))
                    })
                    .collect(),
            );
        }
        let image = heap.alloc_hash_table(data);
        assert_eq!(heap.note_image_object(image), ImageStamp::Stamped);
        assert!(
            !heap.partition_dump,
            "no mapped registration in this fixture"
        );
        heap.begin_minor_collection().unwrap();
        heap.seed_root(image);
        heap.complete_minor_collection();
        heap.finish_incremental_sweep_now();
        assert_eq!(
            heap.live_bytes(),
            0,
            "first minor must exclude image bytes without a major recompute"
        );
        image
            .with_hash_table_mut(|data| insert(data, 0, 256))
            .unwrap();
        let bytes = backing(&heap, image);
        assert!(bytes > 0);
        heap.begin_minor_collection().unwrap();
        heap.seed_root(image);
        heap.complete_minor_collection();
        heap.finish_incremental_sweep_now();
        assert_eq!(
            heap.live_bytes(),
            bytes,
            "materialized backing counted once"
        );
        full(&mut heap, &[image]);
        assert_eq!(heap.live_bytes(), bytes, "major and minor census agree");
    }
}

#[test]
fn image_hash_pacing_minor_census_visits_only_registered_tables() {
    let mut raw = TaggedHeap::new();
    // SAFETY: construction precedes publication and any other heap view.
    unsafe { raw.set_generational_enabled(true) };
    let mut heap = TestHeap::from_heap(raw);
    let first = heap.alloc_hash_table(table(8));
    let second = heap.alloc_hash_table(table(16));
    assert_eq!(heap.note_image_object(first), ImageStamp::Stamped);
    assert_eq!(heap.note_image_object(second), ImageStamp::Stamped);
    full(&mut heap, &[first, second]);
    assert_eq!(heap.image_hash_census_visited_for_test(), 2);
    let runtime = heap.alloc_hash_table(table(8));
    let mut roots = vec![first, second, runtime];
    // Windows are residual Boxes, unlike ordinary arena vectors: the old
    // census really did filter every one of these unrelated owned headers.
    roots.extend((0..50_000).map(|id| heap.alloc_window(id)));
    heap.begin_minor_collection().unwrap();
    for &root in &roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    assert_eq!(
        heap.image_hash_census_visited_for_test(),
        2,
        "the pacing census must not filter 50K unrelated owned headers"
    );
    // This fixture never enabled partitioning: a later major can reclaim
    // the two unrooted stamped tables and must unregister them before reuse.
    full(&mut heap, &roots[2..]);
    assert_eq!(heap.image_hash_census_visited_for_test(), 0);
}
