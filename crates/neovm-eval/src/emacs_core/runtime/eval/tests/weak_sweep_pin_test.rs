//! Lock: the stopped major's weak sweep and the serial stop keep the same
//! weak-table entries.
//!
//! E3 slice 1 runs `finish_stopped_mark_adapters` on the stopped major, so
//! `mark_and_sweep_weak_tables` runs there without the test `WeakSweepPermit`.
//! The era entry below is the ordinary proof admission
//! (`admit_owned_proof_parallel`); the serial entry is `gc_collect_exact`.
//!
//! GNU `keep_entry_p` (fns.c) decides each weakness. GNU
//! `mark_and_sweep_weak_table_contents` (alloc.c) repeats until marking
//! stops. The comment there is a value-weak table A holding X -> Y and a
//! later key-weak table B holding Z -> Y, with Z rooted. The cons spine
//! below is built so the mark records A before B: the list is (B A Z …)
//! and the gray worklist is a stack.

use std::collections::BTreeSet;

use neovm_value::value::{HashTableTest, HashTableWeakness, LispHashTable, Value};

use super::Context;
use super::world::Baton;

struct Tables {
    key: Value,
    value: Value,
    key_or_value: Value,
    key_and_value: Value,
    value_weak: Value,
    key_weak: Value,
}

fn cons(context: &mut Context, car: i64) -> Value {
    // SAFETY: the fixture Context is the sole serial owner. Allocation runs
    // no collection and no callback.
    unsafe { context.tagged_heap_mut() }.alloc_cons(Value::fixnum(car), Value::NIL)
}

fn weak_table(context: &mut Context, weakness: HashTableWeakness) -> Value {
    // SAFETY: same sole serial owner as `cons`.
    unsafe { context.tagged_heap_mut() }.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        8,
        Some(weakness),
        1.5,
        0.8,
    ))
}

fn insert(table: Value, key: Value, value: Value) {
    neovm_value::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(key.to_hash_key(&HashTableTest::Eq), key, value);
    })
    .expect("insert into a weak table");
}

fn list(context: &mut Context, items: &[Value]) -> Value {
    let mut tail = Value::NIL;
    for &item in items.iter().rev() {
        // SAFETY: same sole serial owner as `cons`.
        tail = unsafe { context.tagged_heap_mut() }.alloc_cons(item, tail);
    }
    tail
}

fn pairs(table: Value) -> BTreeSet<(i64, i64)> {
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

/// One fresh context, the dead-windows table cleared, then the six tables.
fn populate() -> (Context, Tables) {
    let mut context = Context::new(crate::test_utils::unit_runtime());
    context.set_variable("window-dead-windows-table", Value::NIL);
    context.gc_collect_exact();

    let key = weak_table(&mut context, HashTableWeakness::Key);
    let value = weak_table(&mut context, HashTableWeakness::Value);
    let key_or_value = weak_table(&mut context, HashTableWeakness::KeyOrValue);
    let key_and_value = weak_table(&mut context, HashTableWeakness::KeyAndValue);
    let value_weak = weak_table(&mut context, HashTableWeakness::Value);
    let key_weak = weak_table(&mut context, HashTableWeakness::Key);

    let mut rooted = Vec::new();
    // B, then A, then Z. The spine push records A before B.
    rooted.push(key_weak);
    rooted.push(value_weak);
    let z = cons(&mut context, 13);
    rooted.push(z);
    let y = cons(&mut context, 12);
    let x = cons(&mut context, 11);
    insert(value_weak, x, y);
    insert(key_weak, z, y);

    plant_matrix(&mut context, key, 100, &mut rooted);
    plant_matrix(&mut context, value, 200, &mut rooted);
    plant_matrix(&mut context, key_or_value, 300, &mut rooted);
    plant_matrix(&mut context, key_and_value, 400, &mut rooted);

    rooted.extend([key, value, key_or_value, key_and_value]);
    let root = list(&mut context, &rooted);
    context.set_variable("s2-weak-pin-root", root);
    (
        context,
        Tables {
            key,
            value,
            key_or_value,
            key_and_value,
            value_weak,
            key_weak,
        },
    )
}

/// Cars `base+1`/`base+2` are both strong, `base+3`/`base+4` is a strong
/// key and a dead value, `base+5`/`base+6` is a dead key and a strong
/// value, `base+7`/`base+8` are both dead. Only the strong objects are rooted.
fn plant_matrix(context: &mut Context, table: Value, base: i64, rooted: &mut Vec<Value>) {
    let both_key = cons(context, base + 1);
    let both_value = cons(context, base + 2);
    let strong_key = cons(context, base + 3);
    let dead_value = cons(context, base + 4);
    let dead_key = cons(context, base + 5);
    let strong_value = cons(context, base + 6);
    let neither_key = cons(context, base + 7);
    let neither_value = cons(context, base + 8);
    insert(table, both_key, both_value);
    insert(table, strong_key, dead_value);
    insert(table, dead_key, strong_value);
    insert(table, neither_key, neither_value);
    rooted.extend([both_key, both_value, strong_key, strong_value]);
}

fn expect(base: i64, weakness: HashTableWeakness) -> BTreeSet<(i64, i64)> {
    let both = (base + 1, base + 2);
    let strong_key = (base + 3, base + 4);
    let strong_value = (base + 5, base + 6);
    match weakness {
        HashTableWeakness::Key => BTreeSet::from([both, strong_key]),
        HashTableWeakness::Value => BTreeSet::from([both, strong_value]),
        HashTableWeakness::KeyOrValue => BTreeSet::from([both, strong_key, strong_value]),
        HashTableWeakness::KeyAndValue => BTreeSet::from([both]),
    }
}

fn assert_tables(tables: &Tables, label: &str) {
    assert!(
        tables
            .key
            .as_hash_table()
            .is_some_and(|table| table.weakness == Some(HashTableWeakness::Key)),
        "{label} key table"
    );
    assert_eq!(
        pairs(tables.key),
        expect(100, HashTableWeakness::Key),
        "{label} key"
    );
    assert_eq!(
        pairs(tables.value),
        expect(200, HashTableWeakness::Value),
        "{label} value"
    );
    assert_eq!(
        pairs(tables.key_or_value),
        expect(300, HashTableWeakness::KeyOrValue),
        "{label} key-or-value"
    );
    assert_eq!(
        pairs(tables.key_and_value),
        expect(400, HashTableWeakness::KeyAndValue),
        "{label} key-and-value"
    );
    assert_eq!(
        pairs(tables.value_weak),
        BTreeSet::from([(11, 12)]),
        "{label} value-weak X -> Y"
    );
    assert_eq!(
        pairs(tables.key_weak),
        BTreeSet::from([(13, 12)]),
        "{label} key-weak Z -> Y"
    );
}

/// One ordinary-admission stopped major: the proof plan, no weak permit.
fn run_stopped_major(context: &mut Context) -> Result<(), String> {
    context
        .admit_owned_proof_parallel(|projection| {
            let era = projection.era();
            let mut stopped = era.stop().expect("stop the admitted era");
            era.collect_stopped(&mut stopped, Baton::collect_parallel_world_roots)
                .expect("stopped major");
            stopped.complete().expect("release the stop");
            let (collector, keeper) = projection.finish().expect("return the era");
            (collector, keeper, ())
        })
        .map_err(|error| format!("{error:?}"))
}

#[test]
fn stopped_major_weak_sweep_matches_serial_for_every_weakness() {
    let (mut serial, serial_tables) = populate();
    serial.gc_collect_exact();
    assert_tables(&serial_tables, "serial");
    drop(serial);

    let (mut stopped, stopped_tables) = populate();
    run_stopped_major(&mut stopped).expect("the weak tables need no permit");
    assert_tables(&stopped_tables, "stopped major");
}
