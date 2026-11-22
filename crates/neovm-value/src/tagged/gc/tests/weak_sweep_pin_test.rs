//! Lock: the serial stop keeps weak-table entries by GNU's rule, and repeats
//! that rule until marking stops.
//!
//! `complete_collection` is the serial stop. GNU `keep_entry_p` (fns.c)
//! keeps a key entry when the key is strong, a value entry when the value
//! is strong, a key-or-value entry when either is, and a key-and-value
//! entry when both are. GNU `mark_and_sweep_weak_table_contents` (alloc.c,
//! the comment above the fixpoint) is a value-weak table A holding X -> Y
//! and a later key-weak table B holding Z -> Y. Z is rooted. One pass would
//! drop X -> Y before B marks Y. The repeat keeps both entries.
//!
//! The gray worklist is a stack. Seeding B, then A, then Z records A before B.

use super::*;
use crate::value::{HashTableTest, HashTableWeakness, LispHashTable};
use std::collections::BTreeSet;

fn cons(heap: &mut TaggedHeap, car: i64) -> TaggedValue {
    heap.alloc_cons(TaggedValue::fixnum(car), TaggedValue::NIL)
}

fn weak_table(heap: &mut TaggedHeap, weakness: HashTableWeakness) -> TaggedValue {
    heap.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        8,
        Some(weakness),
        1.5,
        0.8,
    ))
}

fn insert(table: TaggedValue, key: TaggedValue, value: TaggedValue) {
    crate::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(key.to_hash_key(&HashTableTest::Eq), key, value);
    })
    .expect("insert into a weak table");
}

fn pairs(table: TaggedValue) -> BTreeSet<(i64, i64)> {
    table
        .as_hash_table()
        .expect("hash table")
        .data
        .entries_in_slot_order()
        .map(|entry| {
            (
                entry.key.cons_car().as_fixnum().expect("key car"),
                entry.value.cons_car().as_fixnum().expect("value car"),
            )
        })
        .collect()
}

fn fresh() -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap
}

/// Four entries on one table. Cars 1/2 are both strong, 3/4 is a strong key
/// and a dead value, 5/6 is a dead key and a strong value, 7/8 are both dead.
fn sweep_one(weakness: HashTableWeakness) -> BTreeSet<(i64, i64)> {
    let mut heap = fresh();
    let table = weak_table(&mut heap, weakness);
    let both_key = cons(&mut heap, 1);
    let both_value = cons(&mut heap, 2);
    let strong_key = cons(&mut heap, 3);
    let dead_value = cons(&mut heap, 4);
    let dead_key = cons(&mut heap, 5);
    let strong_value = cons(&mut heap, 6);
    let neither_key = cons(&mut heap, 7);
    let neither_value = cons(&mut heap, 8);
    insert(table, both_key, both_value);
    insert(table, strong_key, dead_value);
    insert(table, dead_key, strong_value);
    insert(table, neither_key, neither_value);
    heap.collect_exact([table, both_key, both_value, strong_key, strong_value].into_iter());
    pairs(table)
}

#[test]
fn serial_weak_sweep_keeps_each_weakness() {
    assert_eq!(
        sweep_one(HashTableWeakness::Key),
        BTreeSet::from([(1, 2), (3, 4)]),
        "key weakness keeps the two strong keys"
    );
    assert_eq!(
        sweep_one(HashTableWeakness::Value),
        BTreeSet::from([(1, 2), (5, 6)]),
        "value weakness keeps the two strong values"
    );
    assert_eq!(
        sweep_one(HashTableWeakness::KeyOrValue),
        BTreeSet::from([(1, 2), (3, 4), (5, 6)]),
        "key-or-value drops only the entry whose key and value are both dead"
    );
    assert_eq!(
        sweep_one(HashTableWeakness::KeyAndValue),
        BTreeSet::from([(1, 2)]),
        "key-and-value keeps only the entry whose key and value are both strong"
    );
}

/// A is recorded before B. Z is the only rooted payload. Both entries stay.
#[test]
fn serial_weak_sweep_keeps_a_value_reachable_from_a_later_key_weak_table() {
    let mut heap = fresh();
    let table_a = weak_table(&mut heap, HashTableWeakness::Value);
    let table_b = weak_table(&mut heap, HashTableWeakness::Key);
    let x = cons(&mut heap, 11);
    let y = cons(&mut heap, 12);
    let z = cons(&mut heap, 13);
    insert(table_a, x, y);
    insert(table_b, z, y);

    heap.begin_stw_collection();
    heap.seed_root(table_b);
    heap.seed_root(table_a);
    heap.seed_root(z);
    heap.mark_all();
    let recorded: Vec<usize> = heap
        .weak_hash_tables
        .iter()
        .map(|ptr| *ptr as usize)
        .collect();
    assert_eq!(
        recorded,
        vec![
            table_a.as_veclike_ptr().expect("table A") as usize,
            table_b.as_veclike_ptr().expect("table B") as usize,
        ],
        "the value-weak table is recorded before the key-weak table"
    );
    heap.complete_collection();

    assert_eq!(pairs(table_a), BTreeSet::from([(11, 12)]));
    assert_eq!(pairs(table_b), BTreeSet::from([(13, 12)]));
}
