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
