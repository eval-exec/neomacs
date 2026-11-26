//! Minor collections and old weak tables. An old weak table that was not
//! written since the last cycle holds only old children, so a minor leaves
//! it out of the weak pass; a store puts it on the remembered set, which
//! brings it back into that minor's pass.

use super::mark_sweep::WEAK_ENTRY_VISITS;
use super::*;
use crate::value::{HashTableTest, LispHashTable};
use std::sync::atomic::Ordering;

fn heap_with_generations() -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { heap.set_generational_enabled(true) };
    // Run the collector's own generational verifier on every cycle here.
    heap.generational.verify = true;
    // SAFETY: serial fixture; the heap is exclusively borrowed by the test
    // thread and no other mutator or compiled leaf runs across the publication.
    unsafe { heap.publish_barrier_window() };
    heap
}

fn weak_table(heap: &mut TaggedHeap, weakness: HashTableWeakness) -> TaggedValue {
    heap.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        Some(weakness),
        1.5,
        0.8,
    ))
}

fn insert(table: TaggedValue, key: TaggedValue, value: TaggedValue) {
    crate::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(key.to_hash_key(&HashTableTest::Eq), key, value);
    });
}

fn table_len(table: TaggedValue) -> usize {
    table.as_hash_table().unwrap().data.len()
}

/// The first partition cycle tenures every survivor and puts each weak
/// table in the always-swept registry. `scan_permanents_for_young_children`
/// then remembers every old owner that still holds a collectible child,
/// TABLE included.
fn first_partition(heap: &mut TaggedHeap, table: TaggedValue, roots: &[TaggedValue]) {
    heap.extend_dump_span(4096, 16);
    heap.collect_rooted(roots.iter().copied());
    assert!(!heap.sweep_in_progress());
    assert!(heap.value_is_old_for_test(table));
    let ptr = table.as_veclike_ptr().unwrap() as *mut HashTableObj;
    assert!(heap.always_swept_weak_tables_set.contains(&ptr));
}

/// [`first_partition`], then one minor that drains and empties the
/// remembered set, leaving TABLE old, unwritten and fully old
/// inside.
fn tenure(heap: &mut TaggedHeap, table: TaggedValue, roots: &[TaggedValue]) {
    first_partition(heap, table, roots);
    minor(heap, &[]);
    assert!(heap.current_mutator_gc().remset.is_empty());
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection_for_test().unwrap();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection_for_test();
    heap.finish_incremental_sweep_now_for_test();
}

/// `WEAK_ENTRY_VISITS` is process-global: the exact count relies on nextest
/// running each test in its own process.
#[test]
fn minor_skips_an_unwritten_old_weak_table() {
    const ENTRIES: usize = 100_000;
    let mut heap = heap_with_generations();
    set_tagged_heap(&mut heap);
    let table = weak_table(&mut heap, HashTableWeakness::Key);
    let keys: Vec<TaggedValue> = (0..ENTRIES)
        .map(|i| heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL))
        .collect();
    for &key in &keys {
        insert(table, key, TaggedValue::T);
    }
    let holder = heap.alloc_vector(keys);
    tenure(&mut heap, table, &[table, holder]);
    assert_eq!(table_len(table), ENTRIES);

    WEAK_ENTRY_VISITS.store(0, Ordering::Relaxed);
    minor(&mut heap, &[]);
    assert_eq!(WEAK_ENTRY_VISITS.load(Ordering::Relaxed), 0);
    assert_eq!(table_len(table), ENTRIES);

    // A major keeps the full pass over every always-swept table.
    WEAK_ENTRY_VISITS.store(0, Ordering::Relaxed);
    heap.collect_rooted([table, holder].into_iter());
    assert!(WEAK_ENTRY_VISITS.load(Ordering::Relaxed) >= ENTRIES);
    assert_eq!(table_len(table), ENTRIES);
}

const WEAKNESSES: [HashTableWeakness; 4] = [
    HashTableWeakness::Key,
    HashTableWeakness::Value,
    HashTableWeakness::KeyOrValue,
    HashTableWeakness::KeyAndValue,
];

/// Insert an entry whose survival under WEAKNESS depends on YOUNG alone:
/// the weak side(s) hold YOUNG and any strong side holds a fixnum.
fn insert_young_entry(table: TaggedValue, weakness: HashTableWeakness, young: TaggedValue) {
    let other = TaggedValue::fixnum(3);
    match weakness {
        HashTableWeakness::Key => insert(table, young, other),
        HashTableWeakness::Value | HashTableWeakness::KeyAndValue => insert(table, other, young),
        HashTableWeakness::KeyOrValue => insert(table, young, young),
    }
}

/// Store a young object into a new entry of an old table that also holds
/// one old entry, then run a minor, rooted at the young object when
/// KEEP_YOUNG. Answers the table and the young object.
fn young_entry_in_old_table(
    heap: &mut TaggedHeap,
    weakness: HashTableWeakness,
    keep_young: bool,
) -> (TaggedValue, TaggedValue) {
    let table = weak_table(heap, weakness);
    let old = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    insert(table, old, old);
    let holder = heap.alloc_vector(vec![old]);
    tenure(heap, table, &[table, holder]);

    let young = heap.alloc_cons(TaggedValue::fixnum(2), TaggedValue::NIL);
    insert_young_entry(table, weakness, young);
    // The store is what brings the old table into the minor's weak pass.
    assert!(heap.current_mutator_gc().remset.contains(&table));
    assert_eq!(table_len(table), 2);
    let roots: &[TaggedValue] = if keep_young { &[young] } else { &[] };
    minor(heap, roots);
    (table, young)
}

/// A table tenured by the first partition cycle and written before the
/// next minor: the window where the always-swept registry used to be the
/// minor's only route to the table. The old entry's cons was promoted by
/// that cycle, yet `scan_permanents_for_young_children` remembered the
/// table, which holds a collectible child; the store logs it as well. The
/// minor is rooted at the young object when KEEP_YOUNG. Answers the table
/// and the young object.
fn young_entry_after_first_partition(
    heap: &mut TaggedHeap,
    weakness: HashTableWeakness,
    keep_young: bool,
) -> (TaggedValue, TaggedValue) {
    let table = weak_table(heap, weakness);
    let old = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    insert(table, old, old);
    let holder = heap.alloc_vector(vec![old]);
    first_partition(heap, table, &[table, holder]);
    assert!(heap.value_is_old_for_test(old));
    assert!(heap.current_mutator_gc().remset.contains(&table));

    let young = heap.alloc_cons(TaggedValue::fixnum(2), TaggedValue::NIL);
    insert_young_entry(table, weakness, young);
    assert_eq!(table_len(table), 2);
    let roots: &[TaggedValue] = if keep_young { &[young] } else { &[] };
    minor(heap, roots);
    (table, young)
}

#[test]
fn minor_drops_a_dead_young_entry_from_an_old_weak_table() {
    for weakness in WEAKNESSES {
        let mut heap = heap_with_generations();
        set_tagged_heap(&mut heap);
        let (table, _) = young_entry_in_old_table(&mut heap, weakness, false);
        assert_eq!(table_len(table), 1, "{weakness:?}");
    }
}

#[test]
fn minor_keeps_a_live_young_entry_in_an_old_weak_table() {
    for weakness in WEAKNESSES {
        let mut heap = heap_with_generations();
        set_tagged_heap(&mut heap);
        let (table, young) = young_entry_in_old_table(&mut heap, weakness, true);
        assert_eq!(table_len(table), 2, "{weakness:?}");
        assert!(heap.value_is_old_for_test(young), "{weakness:?}");
        assert_eq!(young.cons_car(), TaggedValue::fixnum(2), "{weakness:?}");
    }
}

#[test]
fn minor_drops_a_dead_young_entry_written_after_the_first_partition() {
    for weakness in WEAKNESSES {
        let mut heap = heap_with_generations();
        set_tagged_heap(&mut heap);
        let (table, _) = young_entry_after_first_partition(&mut heap, weakness, false);
        assert_eq!(table_len(table), 1, "{weakness:?}");
    }
}

#[test]
fn minor_keeps_a_live_young_entry_written_after_the_first_partition() {
    for weakness in WEAKNESSES {
        let mut heap = heap_with_generations();
        set_tagged_heap(&mut heap);
        let (table, young) = young_entry_after_first_partition(&mut heap, weakness, true);
        assert_eq!(table_len(table), 2, "{weakness:?}");
        assert!(heap.value_is_old_for_test(young), "{weakness:?}");
        assert_eq!(young.cons_car(), TaggedValue::fixnum(2), "{weakness:?}");
    }
}
