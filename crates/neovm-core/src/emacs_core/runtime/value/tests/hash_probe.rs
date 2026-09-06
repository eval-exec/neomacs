//! The in-place hash-table probe must find exactly what the materialized
//! [`HashKey`] finds: same hash stream, same equivalence relation.
use super::super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use std::hash::{Hash, Hasher};

fn fx_hash<T: Hash + ?Sized>(t: &T) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    t.hash(&mut hasher);
    hasher.finish()
}

fn list(items: &[Value]) -> Value {
    items
        .iter()
        .rev()
        .fold(Value::NIL, |tail, item| Value::cons(*item, tail))
}

/// Values covering every arm of `to_eq_key` / `to_eql_key` /
/// `to_equal_key_depth_swp` the probe admits, plus ones it must decline.
fn corpus() -> Vec<Value> {
    let sym = Value::from_sym_id(intern("hash-probe-sym"));
    let other_sym = Value::from_sym_id(intern("hash-probe-other"));
    let mut deep = Value::fixnum(0);
    for _ in 0..250 {
        deep = Value::cons(deep, Value::NIL);
    }
    vec![
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::fixnum(-1),
        Value::fixnum(1 << 40),
        sym,
        other_sym,
        Value::keyword("hash-probe-key"),
        Value::string(""),
        Value::string("abc"),
        Value::string("abc"),
        Value::string("ünïcødé"),
        Value::make_float(1.5),
        Value::make_float(1.5),
        Value::make_float(-0.0),
        Value::make_float(0.0),
        Value::make_float(f64::NAN),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        list(&[Value::fixnum(1), Value::fixnum(2)]),
        list(&[Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]),
        list(&[sym, Value::string("s"), Value::make_float(2.5)]),
        list(&[sym, Value::string("s"), Value::make_float(2.5)]),
        Value::cons(list(&[Value::fixnum(1)]), list(&[Value::fixnum(2)])),
        Value::cons(Value::string("a"), Value::string("b")),
        Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]),
        Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]),
        Value::cons(Value::vector(vec![Value::fixnum(1)]), Value::NIL),
        deep,
    ]
}

const TESTS: [HashTableTest; 3] = [HashTableTest::Eq, HashTableTest::Eql, HashTableTest::Equal];

#[test]
fn probe_hash_and_equivalence_match_the_materialized_key() {
    let values = corpus();
    for test in TESTS {
        let keys: Vec<HashKey> = values
            .iter()
            .map(|v| v.to_hash_key_swp(&test, false))
            .collect();
        for (i, value) in values.iter().enumerate() {
            let Some(probe) = ValueKeyProbe::new(*value, test, false) else {
                continue;
            };
            assert_eq!(
                fx_hash(&probe),
                fx_hash(&keys[i]),
                "hash stream differs for {value:?} under {test:?}"
            );
            for (j, key) in keys.iter().enumerate() {
                assert_eq!(
                    hashbrown::Equivalent::equivalent(&probe, key),
                    keys[i] == *key,
                    "equivalence differs for {value:?} vs {:?} under {test:?}",
                    values[j]
                );
            }
        }
    }
}

#[test]
fn probe_admits_structural_shapes_and_declines_the_rest() {
    let values = corpus();
    let supported = |v: &Value, test| ValueKeyProbe::new(*v, test, false).is_some();
    // Everything keys by identity under `eq`, so every value is admitted.
    assert!(values.iter().all(|v| supported(v, HashTableTest::Eq)));
    // Under `equal`, vectors and 250-deep lists take the materializing path.
    assert!(!supported(&values[values.len() - 1], HashTableTest::Equal));
    assert!(!supported(
        &Value::vector(vec![Value::fixnum(1)]),
        HashTableTest::Equal
    ));
    assert!(!supported(
        &Value::cons(Value::vector(vec![]), Value::NIL),
        HashTableTest::Equal
    ));
    let wide = list(&vec![Value::fixnum(7); FAST_PROBE_NODE_BUDGET + 1]);
    assert!(!supported(&wide, HashTableTest::Equal));
    assert!(supported(
        &list(&[Value::fixnum(1), Value::string("s")]),
        HashTableTest::Equal
    ));
    assert!(supported(&Value::string("s"), HashTableTest::Equal));
}

#[test]
fn storage_lookups_by_value_agree_with_lookups_by_key() {
    let values = corpus();
    for test in TESTS {
        let mut storage = HashTableStorage::default();
        for (i, value) in values.iter().enumerate() {
            storage.insert(
                value.to_hash_key_swp(&test, false),
                *value,
                Value::fixnum(i as i64),
            );
        }
        for value in &values {
            let by_key = storage.get(&value.to_hash_key_swp(&test, false)).copied();
            assert_eq!(
                storage.get_by_value(*value, test, false).copied(),
                by_key,
                "{value:?} under {test:?}"
            );
        }
        let fresh = list(&[Value::fixnum(1), Value::fixnum(2)]);
        let expect_hit = matches!(test, HashTableTest::Equal);
        assert_eq!(
            storage.get_by_value(fresh, test, false).is_some(),
            expect_hit
        );
        if expect_hit {
            *storage.get_mut_by_value(fresh, test, false).unwrap() = Value::T;
            assert_eq!(
                storage.get(&fresh.to_hash_key_swp(&test, false)).copied(),
                Some(Value::T)
            );
            assert_eq!(storage.remove_by_value(fresh, test, false), Some(Value::T));
            assert!(storage.get(&fresh.to_hash_key_swp(&test, false)).is_none());
        }
    }
}

#[test]
fn gethash_puthash_remhash_match_gnu() {
    let mut eval = Context::new();
    // Expectation taken from GNU Emacs 31.0.90 --batch.
    let result = eval
        .eval_str(
            r#"(let ((h (make-hash-table :test 'equal)) (e (make-hash-table :test 'eql)) (q (make-hash-table)))
                 (puthash (list 1 2) 'x h) (puthash "s" 'y h) (puthash 1.5 'f h)
                 (puthash 1.5 'g e) (puthash 'sym 's q) (puthash 7 'seven q)
                 (format "%S" (list (gethash (list 1 2) h) (gethash "s" h) (gethash (list 1 2 3) h)
                                    (gethash 1.5 h) (gethash 1.5 e) (gethash 1.5 q) (gethash 'sym q)
                                    (gethash 7 q) (gethash "s" q)
                                    (progn (remhash (list 1 2) h) (gethash (list 1 2) h 'gone))
                                    (progn (puthash "s" 'z h) (gethash "s" h))
                                    (hash-table-count h))))"#,
        )
        .expect("hash table forms evaluate");
    assert_eq!(
        result.as_utf8_str(),
        Some("(x y nil f g nil s seven nil gone z 2)")
    );
}
