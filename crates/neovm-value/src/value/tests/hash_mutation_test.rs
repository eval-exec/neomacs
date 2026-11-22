use super::*;
use crate::tagged::gc::TestHeap;

fn storage(value: Value) -> usize {
    value
        .as_hash_table()
        .unwrap()
        .data
        .materialized_storage_bytes()
}

fn put_range(table: Value, start: i64, end: i64) {
    // SAFETY: this fixture is the sole serial owner, and neither the typed
    // operations nor this closure run Lisp, collect, or change Tier-H mode.
    unsafe {
        table.with_hash_table_write_inactive(|write| {
            for key in start..end {
                write
                    .put(Value::fixnum(key), Value::fixnum(key + 1), false)
                    .unwrap();
            }
        })
    }
    .unwrap();
}

#[test]
fn image_hash_pacing_typed_insert_bills_growth_hydration_and_unwind_once() {
    let mut heap = TestHeap::new();
    heap.reset_bytes_since_gc();
    let mut detached = LispHashTable::new(HashTableTest::Eq);
    for key in 0..128 {
        detached.insert(HashKey::Int(key), Value::fixnum(key), Value::fixnum(key));
    }
    assert_eq!(
        heap.bytes_since_gc_exact(),
        0,
        "detached builder has no owner charge"
    );
    let table = heap.alloc_hash_table(detached);
    heap.note_image_object(table);
    let before = storage(table);
    heap.reset_bytes_since_gc();
    put_range(table, 128, 1024);
    let after = storage(table);
    assert!(after > before);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        after - before,
        "typed insertion charges once"
    );

    heap.reset_bytes_since_gc();
    put_range(table, 0, 1024);
    assert_eq!(storage(table), after);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        0,
        "overwriting does not charge backing"
    );

    let before_unwind = storage(table);
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: same sole serial owner; unwinding releases the capability
        // before this test observes the heap or table again.
        unsafe {
            table.with_hash_table_write_inactive(|write| {
                for key in 1024..4096 {
                    write.put(Value::fixnum(key), Value::NIL, false).unwrap();
                }
                panic!("after completed backing growth");
            })
        };
    }));
    assert!(unwound.is_err());
    assert!(storage(table) > before_unwind);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        storage(table) - before_unwind,
        "completed allocations are billed before later unwind"
    );

    let mut pending = LispHashTable::new(HashTableTest::Eq);
    pending.set_pending_dump_entries(
        (0..256)
            .map(|key| {
                let key = Value::fixnum(key);
                (HashKey::Int(key.as_fixnum().unwrap()), key, Some(key))
            })
            .collect(),
    );
    let image = heap.alloc_hash_table(pending);
    heap.note_image_object(image);
    heap.reset_bytes_since_gc();
    put_range(image, 0, 1);
    let hydrated_bytes = storage(image);
    assert!(hydrated_bytes > 0);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        hydrated_bytes,
        "cold hydration charges once"
    );
    heap.reset_bytes_since_gc();
    put_range(image, 0, 256);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        0,
        "hydrated overwrites have no repeated charge"
    );

    // Unrestricted replacement remains the cold generic wrapper's contract.
    let replacement_before = storage(image);
    image
        .with_hash_table_mut(|table| {
            *table = LispHashTable::new_with_options(HashTableTest::Eq, 4096, None, 1.5, 0.8125);
        })
        .unwrap();
    assert_eq!(
        heap.bytes_since_gc_exact(),
        storage(image) - replacement_before,
        "generic replacement retains its net backing charge"
    );
}

fn clustered_table() -> (LispHashTable, Vec<i64>) {
    // Hashbrown's 128-bucket table has 112 usable entries. A shared initial
    // bucket gives removal a real tombstone case even below that load limit.
    let mut table = LispHashTable::new_with_options(HashTableTest::Eq, 112, None, 1.5, 0.8125);
    assert_eq!(table.data.index.capacity(), 112);
    let keys: Vec<_> = (0..100_000)
        .filter(|&key| stored_hash(&HashKey::Int(key), Value::fixnum(key)) & 127 == 0)
        .take(64)
        .collect();
    assert_eq!(keys.len(), 64, "enough keys share the initial hash bucket");
    for &key in &keys {
        table.insert(
            HashKey::Int(key),
            Value::fixnum(key),
            Value::fixnum(key + 1),
        );
    }
    (table, keys)
}

fn remove_key(table: Value, key: i64) {
    // SAFETY: this sole serial owner remains installed and live; neither the
    // typed operation nor this closure invokes Lisp, GC, or a Tier-H switch.
    unsafe {
        table.with_hash_table_write_inactive(|write| write.remove(Value::fixnum(key), false))
    }
    .unwrap()
    .unwrap();
}

fn removal_shape(table: Value) -> (usize, usize, usize, usize) {
    let table = table.as_hash_table().unwrap();
    (
        table.data.materialized_storage_bytes(),
        table.data.free_slots.len(),
        table.data.free_slots.capacity(),
        table.data.index.capacity(),
    )
}

#[test]
fn image_hash_pacing_typed_remove_preserves_net_growth_hydration_and_unwind() {
    let mut heap = TestHeap::new();
    heap.reset_bytes_since_gc();
    let (detached, keys) = clustered_table();
    let mut detached_copy = detached.clone();
    assert!(
        detached_copy
            .data
            .try_remove_by_value(Value::fixnum(keys[0]), HashTableTest::Eq, false,)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        heap.bytes_since_gc_exact(),
        0,
        "detached removal has no owner charge"
    );

    let table = heap.alloc_hash_table(detached);
    heap.note_image_object(table);
    assert_eq!(
        removal_shape(table).2,
        0,
        "first removal starts with no free-slot capacity"
    );
    heap.reset_bytes_since_gc();
    let mut saw_growth_with_index_decrease = false;
    let mut saw_spare_capacity = false;
    let mut total_growth_charge = 0;
    for (index, &key) in keys.iter().enumerate() {
        let (before, free_len, free_capacity, index_capacity) = removal_shape(table);
        let before_charge = heap.bytes_since_gc_exact();
        remove_key(table, key);
        let (after, _, after_free_capacity, after_index_capacity) = removal_shape(table);
        let charge = heap.bytes_since_gc_exact() - before_charge;
        assert_eq!(
            charge,
            after.saturating_sub(before),
            "net domain at remove {index}"
        );
        if index == 0 {
            assert!(
                after_free_capacity > 0,
                "first removal allocates the reusable-slot Vec"
            );
        }
        if free_len < free_capacity {
            saw_spare_capacity = true;
            assert_eq!(charge, 0, "spare-capacity removal is uncharged");
        } else if free_capacity > 0 && after_index_capacity < index_capacity {
            saw_growth_with_index_decrease = true;
            assert!(after_free_capacity > free_capacity);
        }
        total_growth_charge += charge;
    }
    assert!(saw_spare_capacity);
    assert!(
        saw_growth_with_index_decrease,
        "cold growth must include the concurrent fall in usable index capacity"
    );
    assert!(
        total_growth_charge > 0,
        "a later growth has a positive net charge"
    );
    let charged = heap.bytes_since_gc_exact();
    remove_key(table, -1);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        charged,
        "missing removal is uncharged"
    );

    let (detached, keys) = clustered_table();
    let unwind_table = heap.alloc_hash_table(detached);
    let mut next = 0;
    loop {
        let (_, len, capacity, _) = removal_shape(unwind_table);
        if len == capacity && len >= 8 {
            break;
        }
        assert!(next < keys.len());
        remove_key(unwind_table, keys[next]);
        next += 1;
    }
    let before = storage(unwind_table);
    heap.reset_bytes_since_gc();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: this owner stays installed while the sole typed mutation
        // unwinds, and its loans end before the test reads the heap again.
        unsafe {
            unwind_table.with_hash_table_write_inactive(|write| {
                write.remove(Value::fixnum(keys[next]), false).unwrap();
                panic!("after completed removal growth");
            })
        };
    }));
    assert!(unwound.is_err());
    let expected = storage(unwind_table).saturating_sub(before);
    assert!(
        expected > 0,
        "this removal completed a positive net allocation"
    );
    assert_eq!(
        heap.bytes_since_gc_exact(),
        expected,
        "completed growth precedes unwind"
    );

    let mut pending = LispHashTable::new(HashTableTest::Eq);
    pending.set_pending_dump_entries(
        keys.iter()
            .map(|&key| {
                let key_value = Value::fixnum(key);
                (HashKey::Int(key), Value::NIL, Some(key_value))
            })
            .collect(),
    );
    let image = heap.alloc_hash_table(pending);
    heap.note_image_object(image);
    heap.reset_bytes_since_gc();
    remove_key(image, -1);
    let hydrated = storage(image);
    assert!(hydrated > 0);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        hydrated,
        "image hydration is charged once"
    );
    heap.reset_bytes_since_gc();
    remove_key(image, -1);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        0,
        "a second miss does not hydrate again"
    );
    let before = storage(image);
    remove_key(image, keys[0]);
    assert_eq!(
        heap.bytes_since_gc_exact(),
        storage(image).saturating_sub(before),
        "the first hydrated removal charges only its separate net growth"
    );
}
