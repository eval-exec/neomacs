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

/// With `symbols-with-pos-enabled` set, the materialized key strips
/// positioned symbols, so the key is stored materialized as before.
#[test]
fn hash_keys_put_with_symbols_with_pos_enabled_keep_their_materialized_form() {
    let test = HashTableTest::Equal;
    let key = list(&[Value::symbol("np-swp"), Value::fixnum(1)]);
    let mut storage = HashTableStorage::with_capacity(4);
    let HashProbe::Absent(hash) = storage
        .try_probe_for_insert(key, test, true)
        .expect("probing a non-circular key signals nothing")
    else {
        panic!("a fresh table holds no key");
    };
    storage.insert_absent(hash, test, true, key, Value::T);
    let keyed = storage.keyed_entries_in_slot_order();
    assert_eq!(keyed.len(), 1);
    assert_eq!(*keyed[0].0, key.to_hash_key_swp(&test, true));
}

/// A jump table of `keys` put as `puthash` puts them, each targeting the
/// byte offset `8 * (i + 1)`.
fn jump_table(keys: &[Value]) -> LispHashTable {
    let mut table = LispHashTable::new(HashTableTest::Equal);
    for (i, key) in keys.iter().enumerate() {
        put(&mut table.data, *key, Value::fixnum(8 * (i as i64 + 1)));
    }
    table
}

/// What the switch plan and the hashed lookup answer for `value`, after the
/// two dispatches that build the plan.
fn dispatch(table: &LispHashTable, value: Value) -> (Option<usize>, Option<usize>) {
    table.switch_target(Value::NIL, false);
    table.switch_target(Value::NIL, false);
    (
        table.switch_target(value, false).map(Value::bits),
        table
            .data
            .lookup(value, HashTableTest::Equal, false)
            .map(|target| target.bits()),
    )
}

/// A key mutated after `puthash` and before the plan is built now hashes
/// elsewhere than its entry: the plan must not answer from its new shape,
/// which the hashed lookup (and GNU `Bswitch`) misses.
#[test]
fn hash_a_live_key_mutated_before_planning_is_not_planned_by_its_new_shape() {
    let key = list(&[Value::symbol("a")]);
    let table = jump_table(&[key, list(&[Value::symbol("z")])]);
    key.set_car(Value::symbol("b"));
    let (plan, lookup) = dispatch(&table, list(&[Value::symbol("b")]));
    assert_eq!(lookup, None);
    assert_eq!(plan, lookup);
}

/// `((((leaf))))`: `equal_value_hash`, like GNU `sxhash_obj`, folds in
/// nothing below `SXHASH_MAX_DEPTH`, so `leaf` is not part of the hash.
fn nested(leaf: i64) -> Value {
    list(&[list(&[list(&[list(&[Value::fixnum(leaf)])])])])
}

/// Set the leaf of a [`nested`] key in place.
fn set_leaf(key: Value, leaf: i64) {
    key.cons_car()
        .cons_car()
        .cons_car()
        .set_car(Value::fixnum(leaf));
}

/// A key mutated before planning, but still under its entry's hash, is
/// found by its new shape through both the plan and the lookup.
#[test]
fn hash_a_live_key_mutated_under_its_hash_before_planning_answers_by_its_new_shape() {
    let key = nested(5);
    let table = jump_table(&[key, list(&[Value::fixnum(0)])]);
    let hash = equal_value_hash(key);
    set_leaf(key, 6);
    assert_eq!(equal_value_hash(key), hash, "the mutation keeps the hash");
    let hit = Some(Value::fixnum(8).bits());
    assert_eq!(dispatch(&table, nested(6)), (hit, hit));
    assert_eq!(dispatch(&table, nested(5)), (None, None));
    assert_eq!(table.shape(), Some(switch_plan::PlanShape::Nodes));
}

/// Two keys mutated into `equal` ones before planning: the plan declines,
/// so the hashed lookup decides which entry answers.
#[test]
fn hash_live_keys_mutated_into_equal_keys_are_not_planned() {
    let first = nested(5);
    let table = jump_table(&[first, nested(6)]);
    set_leaf(first, 6);
    let (plan, lookup) = dispatch(&table, nested(6));
    assert!(lookup.is_some());
    assert_eq!(plan, lookup);
    assert_eq!(table.shape(), Some(switch_plan::PlanShape::Generic));
}

/// A key mutated in place AFTER the plan is built, under its hash: the plan
/// still answers for the key's old shape (the module doc's one exception),
/// and the debug check, which compares every dispatch with the lookup,
/// recognizes the mutation rather than reporting a divergence.
#[test]
fn hash_a_live_key_mutated_after_planning_answers_from_the_plan() {
    let key = nested(5);
    let table = jump_table(&[key, list(&[Value::fixnum(0)])]);
    let hit = Some(Value::fixnum(8).bits());
    assert_eq!(dispatch(&table, nested(5)), (hit, hit));
    set_leaf(key, 6);
    assert_eq!(dispatch(&table, nested(5)), (hit, None));
    assert_eq!(dispatch(&table, nested(6)), (None, hit));
}

/// GNU `hash_table_thaw` (src/fns.c) rehashes every dumped entry and keeps
/// all `count` of them. Two live keys mutated into `equal` shapes are dumped
/// as the same materialized key; the rebuild keeps both entries, in order.
#[test]
fn hash_rebuild_keeps_entries_whose_keys_became_equal() {
    let mut table = LispHashTable::new(HashTableTest::Equal);
    let first = list(&[Value::symbol("a")]);
    let second = list(&[Value::symbol("b")]);
    put(&mut table.data, first, Value::fixnum(1));
    put(&mut table.data, second, Value::fixnum(2));
    second.set_car(Value::symbol("a"));
    // What the dump writer writes: each key object's materialized key, the
    // value, and the key object as the snapshot.
    let dumped: Vec<_> = table
        .data
        .keyed_entries_in_slot_order()
        .into_iter()
        .map(|(_, entry)| {
            (
                entry.key.to_hash_key(&HashTableTest::Equal),
                entry.value,
                Some(entry.key),
            )
        })
        .collect();
    assert_eq!(dumped[0].0, dumped[1].0, "both keys materialize alike");

    let mut reloaded = LispHashTable::new(HashTableTest::Equal);
    reloaded.rebuild_from_ordered_entries(dumped);
    let entries: Vec<(usize, Value)> = reloaded
        .entries_in_slot_order()
        .map(|entry| (entry.key.bits(), entry.value))
        .collect();
    assert_eq!(
        entries,
        vec![
            (first.bits(), Value::fixnum(1)),
            (second.bits(), Value::fixnum(2)),
        ]
    );
    assert_eq!(reloaded.data.len(), 2);
}
