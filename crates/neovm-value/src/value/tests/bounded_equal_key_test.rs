//! The stored structural key of an `equal` table is budgeted.
//!
//! A structural key unfolds a DAG into a tree, so its size is exponential in
//! depth even though the object graph is small.  GNU bounds its equal hash
//! (`SXHASH_MAX_DEPTH`/`SXHASH_MAX_LEN`); this stored key exists only for
//! iteration, dumping and switch plans, while membership uses
//! `equal_value_hash` plus a live `equal`.  Budget exhaustion must therefore
//! truncate the key, not recurse to an allocation failure.
use super::super::*;

/// A shared cons DAG: `2^n` unfolded nodes for `n` conses.
fn shared_dag(depth: usize) -> Value {
    let mut value = Value::fixnum(1);
    for _ in 0..depth {
        value = Value::cons(value, value);
    }
    value
}

fn list(items: &[Value]) -> Value {
    items
        .iter()
        .rev()
        .fold(Value::NIL, |tail, item| Value::cons(*item, tail))
}

#[test]
fn bounded_equal_key_truncates_a_shared_dag() {
    let dag = shared_dag(40);
    let key = dag.to_equal_key_bounded(64, false);
    assert!(
        key.contains_truncation(),
        "a 2^40-node unfolded key must hit the budget"
    );
    assert!(
        dag.to_hash_key_for_insert(HashTableTest::Equal, false)
            .contains_truncation()
    );
}

#[test]
fn equal_shared_dags_get_the_same_stored_key() {
    let left = shared_dag(40);
    let right = shared_dag(40);
    assert_eq!(
        left.to_hash_key_for_insert(HashTableTest::Equal, false),
        right.to_hash_key_for_insert(HashTableTest::Equal, false),
    );
}

#[test]
fn an_ordinary_key_is_unchanged_by_the_budget() {
    let sym = Value::symbol("bounded-key-sym");
    let value = list(&[
        Value::fixnum(1),
        Value::cons(Value::fixnum(2), Value::fixnum(3)),
        sym,
    ]);
    let unbounded = value.to_equal_key_depth_swp(0, &mut EqualKeySeen::new(), false);
    assert!(!unbounded.contains_truncation());
    assert_eq!(
        unbounded,
        value.to_equal_key_bounded(EQUAL_KEY_NODE_BUDGET, false)
    );
    assert_eq!(
        unbounded,
        value.to_hash_key_for_insert(HashTableTest::Equal, false)
    );
}

#[test]
fn a_truncated_key_still_finds_and_updates_its_entry() {
    // Depth 24 already unfolds to 2^24 key nodes, so the stored key truncates.
    // `equal` itself costs GNU and Neomacs alike binomial work in depth here,
    // so a deeper DAG would only make the test slow, not more discriminating.
    let dag = shared_dag(24);
    let table = Value::hash_table(HashTableTest::Equal);
    table.with_hash_table_mut(|table| {
        let probe = table
            .data
            .try_probe_for_insert(dag, HashTableTest::Equal, false)
            .expect("an equal table has no user hash function to signal");
        let HashProbe::Absent(hash) = probe else {
            panic!("the table is empty");
        };
        table
            .data
            .insert_absent(hash, HashTableTest::Equal, false, dag, Value::fixnum(7));

        assert_eq!(
            table.data.lookup(dag, HashTableTest::Equal, false),
            Some(&Value::fixnum(7)),
            "a live-equal key must find the entry filed under a truncated key"
        );
        // An equal but non-identical DAG is the same table key in GNU.
        let equal_dag = shared_dag(24);
        assert_eq!(
            table.data.lookup(equal_dag, HashTableTest::Equal, false),
            Some(&Value::fixnum(7))
        );
    });
}
