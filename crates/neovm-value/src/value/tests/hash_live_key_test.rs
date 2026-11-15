//! An `equal` table keyed by a cons, vector, record or string files the
//! entry under the key object itself, as GNU `hash_put` (src/fns.c) stores
//! `key` beside its hash: `puthash` of a fresh key costs nothing that grows
//! with the key, and every reader of the stored key still sees the key it
//! saw when the index held a materialized copy.
use super::super::*;
use crate::tagged::collection_reads::allocation_tests::measured;

fn list(items: &[Value]) -> Value {
    items
        .iter()
        .rev()
        .fold(Value::NIL, |tail, item| Value::cons(*item, tail))
}

/// What `puthash` does: probe, and file the key when it is absent.
fn put(storage: &mut HashTableStorage, key: Value, value: Value) {
    let test = HashTableTest::Equal;
    match storage
        .try_probe_for_insert(key, test, false)
        .expect("probing a non-circular key signals nothing")
    {
        HashProbe::Found(slot) => *storage.slot_value_mut(slot).expect("a live slot") = value,
        HashProbe::Absent(hash) => storage.insert_absent(hash, test, false, key, value),
    }
}

/// The keys the finding measured: a 100,000-element vector, a
/// 100,000-character string and a 10,000-element list, each `equal` to a
/// fresh copy.
fn large_keys() -> Vec<(&'static str, Value, Value)> {
    let vector = || {
        let mut slots = vec![Value::fixnum(0); 100_000];
        slots[0] = Value::fixnum(7);
        Value::vector(slots)
    };
    let string = || Value::string("k".repeat(100_000));
    let items = vec![Value::fixnum(3); 10_000];
    vec![
        ("vector", vector(), vector()),
        ("string", string(), string()),
        ("list", list(&items), list(&items)),
    ]
}

#[test]
fn hash_puthash_of_a_large_structural_key_allocates_nothing_per_element() {
    for (shape, key, copy) in large_keys() {
        let mut storage = HashTableStorage::with_capacity(4);
        let ((), (calls, bytes)) = measured(|| put(&mut storage, key, Value::T));
        assert!(
            bytes < 1024,
            "puthash of a fresh {shape} key requested {bytes} bytes in {calls} allocations"
        );
        assert_eq!(
            storage
                .lookup(copy, HashTableTest::Equal, false)
                .map(|v| v.bits()),
            Some(Value::T.bits()),
            "an equal copy of the {shape} key finds the entry"
        );
    }
}

#[test]
fn hash_entries_keyed_by_live_objects_answer_every_key_reader() {
    let test = HashTableTest::Equal;
    let keys = [
        list(&[
            Value::fixnum(1),
            Value::string("two"),
            Value::make_float(3.5),
        ]),
        Value::vector(vec![Value::fixnum(1), list(&[Value::fixnum(2)])]),
        Value::make_record(vec![Value::symbol("np-rec"), Value::fixnum(1)]),
        Value::string("a string key"),
    ];
    let copy_of = |key: Value| -> Value {
        match key.kind() {
            ValueKind::Cons => list(&[
                Value::fixnum(1),
                Value::string("two"),
                Value::make_float(3.5),
            ]),
            ValueKind::String => Value::string("a string key"),
            _ if key.is_vector() => {
                Value::vector(vec![Value::fixnum(1), list(&[Value::fixnum(2)])])
            }
            _ => Value::make_record(vec![Value::symbol("np-rec"), Value::fixnum(1)]),
        }
    };
    let mut storage = HashTableStorage::with_capacity(8);
    for (i, key) in keys.iter().enumerate() {
        put(&mut storage, *key, Value::fixnum(i as i64));
    }
    put(&mut storage, copy_of(keys[0]), Value::fixnum(10));
    assert_eq!(
        storage.len(),
        keys.len(),
        "an equal copy replaces, never adds"
    );

    // Iteration and the dump writer read the entry's key object.
    let in_order: Vec<usize> = storage
        .keyed_entries_in_slot_order()
        .into_iter()
        .map(|(_, entry)| entry.key.bits())
        .collect();
    assert_eq!(in_order, keys.iter().map(|k| k.bits()).collect::<Vec<_>>());

    for (i, key) in keys.iter().enumerate() {
        let copy = copy_of(*key);
        let materialized = copy.to_hash_key_swp(&test, false);
        let want = if i == 0 { 10 } else { i as i64 };
        assert_eq!(
            storage.lookup(copy, test, false).copied(),
            Some(Value::fixnum(want))
        );
        // The by-key readers still find the entry from a materialized key.
        assert!(storage.contains_key(&materialized), "{key:?}");
        assert_eq!(
            storage.get(&materialized).copied(),
            Some(Value::fixnum(want))
        );
        assert_eq!(
            storage.key_snapshot(&materialized).map(|k| k.bits()),
            Some(key.bits()),
            "the visible key is the object that was put"
        );
        // A by-key insert of an equal key replaces the entry.
        assert_eq!(
            storage.insert(materialized, copy, Value::fixnum(want)),
            Some(Value::fixnum(want))
        );
    }
    assert_eq!(storage.len(), keys.len());

    // `copy-hash-table`.
    let copied = storage.clone();
    for key in &keys {
        assert!(copied.lookup(copy_of(*key), test, false).is_some());
    }

    assert_eq!(
        storage.remove(&copy_of(keys[1]).to_hash_key_swp(&test, false)),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        storage.remove_by_value(copy_of(keys[3]), test, false),
        Some(Value::fixnum(3))
    );
    assert_eq!(storage.len(), keys.len() - 2);
    assert!(storage.lookup(copy_of(keys[1]), test, false).is_none());
    assert!(storage.lookup(copy_of(keys[3]), test, false).is_none());
    assert!(copied.lookup(copy_of(keys[1]), test, false).is_some());
}

/// GNU keeps the hash computed at insertion and compares the LIVE key
/// object (`hash_find_with_hash`). A list key mutated after `puthash` is then
/// found by nothing: the object and a copy of its new shape hash elsewhere,
/// and a copy of its old shape is no longer `equal` to it.
#[test]
fn hash_mutated_list_key_is_compared_as_the_live_object() {
    let test = HashTableTest::Equal;
    let shape = [Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)];
    let key = list(&shape);
    let mut storage = HashTableStorage::with_capacity(4);
    put(&mut storage, key, Value::T);
    key.set_car(Value::fixnum(9));
    assert!(storage.lookup(key, test, false).is_none());
    assert!(storage.lookup(list(&shape), test, false).is_none());
    let new_shape = list(&[Value::fixnum(9), Value::fixnum(2), Value::fixnum(3)]);
    assert!(storage.lookup(new_shape, test, false).is_none());
    put(&mut storage, new_shape, Value::NIL);
    assert_eq!(storage.len(), 2);
}

/// Keys whose `equal` key is a leaf are stored materialized, as before.
#[test]
fn hash_leaf_keys_keep_their_materialized_form() {
    let test = HashTableTest::Equal;
    let bignum = Value::make_integer(Integer::from(1u128 << 70));
    let keys = [
        Value::make_float(1.5),
        bignum,
        Value::make_bool_vector(3, vec![0b111]),
        Value::symbol("np-sym"),
        Value::fixnum(42),
    ];
    let mut storage = HashTableStorage::with_capacity(8);
    for key in keys {
        put(&mut storage, key, Value::T);
    }
    for ((stored, _), key) in storage.keyed_entries_in_slot_order().into_iter().zip(keys) {
        assert_eq!(*stored, key.to_hash_key_swp(&test, false));
        assert!(storage.get(stored).is_some());
    }
}
