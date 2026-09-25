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
            let Some(hasher) = probe_hasher(*value, test, false) else {
                continue;
            };
            assert_eq!(
                hasher.finish(),
                fx_hash(&keys[i]),
                "hash stream differs for {value:?} under {test:?}"
            );
            for (j, key) in keys.iter().enumerate() {
                assert_eq!(
                    value_matches(*value, test, key),
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
    let supported = |v: &Value, test| probe_hasher(*v, test, false).is_some();
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

/// The probe follows a list's cdr in a loop, not a call per element: the
/// longest list the probe admits (200 conses; one more and a cons would sit
/// at depth 200, where the probe declines) hashes and matches on a thread
/// whose stack would not hold a frame per cons, and a 4,000-element list is
/// declined there without walking past depth 200.
#[test]
fn a_long_list_probe_does_not_recurse_per_cdr() {
    let longest = list(&vec![Value::fixnum(7); FAST_PROBE_MAX_DEPTH]);
    let too_long = list(&vec![Value::fixnum(7); 4_000]);
    let test = HashTableTest::Equal;
    let key = longest.to_hash_key_swp(&test, false);
    let expected = fx_hash(&key);
    let probe = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(16 * 1024)
            .spawn_scoped(scope, || {
                (
                    probe_hasher(longest, test, false).map(|hasher| hasher.finish()),
                    value_matches(longest, test, &key),
                    probe_hasher(too_long, test, false).is_none(),
                )
            })
            .expect("spawn a small-stack thread")
            .join()
            .expect("the probe fits a 16 KiB stack")
    });
    assert_eq!(probe, (Some(expected), true, true));
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
                storage.lookup(*value, test, false).copied(),
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

/// Small `eq`/`eql` tables answer fixnum and symbol lookups by scanning
/// slots for the key's bits (`small_identity_scan`). Across every table size
/// up to and past the scan limit, with removals leaving holes and mixed key
/// shapes, the answer must be the hashed lookup's.
#[test]
fn small_table_identity_scans_agree_with_hashed_lookups() {
    struct Rng(u64);
    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }
    let mut rng = Rng(0x243f_6a88_85a3_08d3);
    let syms: Vec<Value> = (0..24)
        .map(|i| Value::from_sym_id(intern(&format!("small-scan-sym-{i}"))))
        .collect();
    let pool = |rng: &mut Rng| -> Value {
        match rng.below(7) {
            0 | 1 => Value::fixnum(rng.below(40) as i64 - 5),
            2 | 3 => syms[rng.below(syms.len())],
            4 => Value::make_float(rng.below(4) as f64),
            5 => Value::string(format!("k{}", rng.below(4))),
            _ => [Value::NIL, Value::T, Value::keyword("small-scan-kw")][rng.below(3)],
        }
    };
    let mut checked = 0;
    for test in [HashTableTest::Eq, HashTableTest::Eql] {
        for size in 0..40 {
            let mut storage = HashTableStorage::default();
            for i in 0..size {
                let key = pool(&mut rng);
                storage.insert(
                    key.to_hash_key_swp(&test, false),
                    key,
                    Value::fixnum(i as i64),
                );
                if rng.below(5) == 0 {
                    let gone = pool(&mut rng);
                    storage.remove(&gone.to_hash_key_swp(&test, false));
                }
            }
            for _ in 0..60 {
                let probe = pool(&mut rng);
                let by_key = storage.get(&probe.to_hash_key_swp(&test, false)).copied();
                assert_eq!(
                    storage
                        .lookup(probe, test, false)
                        .copied()
                        .map(|v| v.bits()),
                    by_key.map(|v| v.bits()),
                    "{probe:?} under {test:?} in a {size}-entry table"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 4000, "checked {checked}");
}

/// With `symbols-with-pos-enabled`, a positioned key `eq`s its bare symbol,
/// which no bit comparison sees: the scan must decline.
#[test]
fn small_table_scans_decline_under_symbols_with_pos() {
    let mut ctx = Context::new();
    let sym = Value::from_sym_id(intern("small-scan-positioned"));
    let positioned = ctx.tagged_heap.alloc_symbol_with_pos(sym, Value::fixnum(3));
    for test in [HashTableTest::Eq, HashTableTest::Eql] {
        let mut storage = HashTableStorage::default();
        storage.insert(
            positioned.to_hash_key_swp(&test, true),
            positioned,
            Value::fixnum(7),
        );
        let by_key = storage.get(&sym.to_hash_key_swp(&test, true)).copied();
        assert_eq!(
            storage.lookup(sym, test, true).copied().map(|v| v.bits()),
            by_key.map(|v| v.bits()),
            "bare symbol against a positioned key under {test:?}"
        );
    }
}

/// `retain_entries` (the weak-table sweep) leaves exactly what removing each
/// rejected entry by key in `iter` order left: the same survivors in the same
/// slots, and the freed slots reused in the same order, so `maphash` order
/// after later insertions is unchanged.
#[test]
fn retain_entries_matches_removal_by_key_in_iteration_order() {
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
        let keep = |_key: Value, value: Value| value.as_fixnum().is_some_and(|i| i % 3 != 1);
        let mut by_key = storage.clone();
        let dead: Vec<HashKey> = by_key
            .iter()
            .filter(|&(hk, &value)| {
                let key = *by_key.key_snapshot(hk).expect("a live key");
                !keep(key, value)
            })
            .map(|(hk, _)| hk.clone())
            .collect();
        for hk in dead {
            by_key.remove(&hk);
        }
        storage.retain_entries(keep);
        let fresh = |storage: &mut HashTableStorage| {
            for i in 0..values.len() {
                let key = Value::fixnum(1_000_000 + i as i64);
                storage.insert(key.to_hash_key_swp(&test, false), key, key);
            }
            storage
                .entries_in_slot_order()
                .map(|entry| (entry.key.bits(), entry.value.bits()))
                .collect::<Vec<_>>()
        };
        assert_eq!(fresh(&mut storage), fresh(&mut by_key), "{test:?}");
    }
}

/// `user_hash_incomplete` derives "some live key has no remembered hash" from
/// `index.len() != user_hashes.len()`, which is exact only while neither memo
/// map holds a key that is no longer in the index.
///
/// A stale memo is not merely wasted space: it would cancel against a live
/// unhashed key, making the comparison read "complete" when it is not, and a
/// lookup would then answer the default for a key that IS present. So every
/// removal path has to drop the memo -- including `retain_entries`, the weak
/// table sweep, which frees slots itself instead of going through `remove`
/// and is the one that would otherwise be missed.
#[test]
fn every_removal_path_drops_the_user_hash_memo() {
    let test = HashTableTest::Equal;
    let key_of = |i: i64| Value::fixnum(i).to_hash_key_swp(&test, false);

    let mut storage = HashTableStorage::with_capacity(8);
    for i in 0..6 {
        let key = Value::fixnum(i);
        storage.insert(key_of(i), key, key);
        storage.set_user_hash(key_of(i), i % 2);
    }
    assert!(!storage.user_hash_incomplete(), "all six were remembered");

    // remove
    storage.remove(&key_of(0));
    // remove_by_value
    storage.remove_by_value(Value::fixnum(1), test, false);
    // retain_entries: the weak sweep. Drop 2 and 3, keep the rest.
    storage.retain_entries(|key, _| !matches!(key.as_fixnum(), Some(2 | 3)));

    let live: Vec<_> = (0..6)
        .filter(|i| storage.contains_key(&key_of(*i)))
        .collect();
    assert_eq!(live, vec![4, 5], "only 4 and 5 should survive");

    for key in storage.remembered_user_hash_keys() {
        assert!(
            storage.contains_key(&key),
            "a removal path left a memo for a key no longer in the table"
        );
    }
    assert!(
        !storage.user_hash_incomplete(),
        "the survivors are all remembered, so nothing is incomplete"
    );
    // The buckets must have been pruned too, not just `user_hashes`.
    for hash in [0, 1] {
        for key in storage.user_candidates(hash) {
            assert!(
                storage.contains_key(key),
                "a bucket still names a removed key"
            );
        }
    }

    storage.clear();
    assert!(storage.remembered_user_hash_keys().is_empty());
    assert!(storage.user_candidates(0).is_empty());
    assert!(storage.user_candidates(1).is_empty());
}

/// A Lisp hash function is free to answer differently for the same key on two
/// calls -- GNU never sees this because it hashes once, at insertion -- so
/// re-remembering a key must move it between buckets rather than leave it in
/// both.
#[test]
fn re_remembering_a_key_moves_it_between_buckets() {
    let test = HashTableTest::Equal;
    let key = Value::fixnum(7);
    let hash_key = key.to_hash_key_swp(&test, false);

    let mut storage = HashTableStorage::with_capacity(4);
    storage.insert(hash_key.clone(), key, key);
    storage.set_user_hash(hash_key.clone(), 100);
    assert_eq!(storage.user_candidates(100).len(), 1);

    storage.set_user_hash(hash_key.clone(), 200);
    assert!(
        storage.user_candidates(100).is_empty(),
        "the key was left behind in its old bucket"
    );
    assert_eq!(storage.user_candidates(200).len(), 1);

    // Re-recording the SAME hash must not duplicate the key in its bucket.
    storage.set_user_hash(hash_key, 200);
    assert_eq!(storage.user_candidates(200).len(), 1);
}

/// Hashing a string key must cost the same whatever the string's length.
///
/// GNU `hash_char_array' (src/fns.c) seeds with the LENGTH, strides through
/// at most eight machine words, and adds the last word.  It does not walk the
/// string.  We fed every byte to the hasher instead, so a table keyed by long
/// strings paid O(len) on every probe -- including every MISS, where the cost
/// buys nothing at all: 2,000 `gethash` misses against a 320,000-character key
/// took 39.7ms against GNU's flat 0.7ms.
///
/// Counting the bytes handed to the hasher is the assertion. Nothing about
/// the ANSWERS changes when the hash walks the whole string, so a correctness
/// test cannot see this; only the volume can.
#[test]
fn hashing_a_string_key_does_not_scale_with_its_length() {
    #[derive(Default)]
    struct CountingHasher {
        bytes: usize,
    }
    impl Hasher for CountingHasher {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.bytes += bytes.len();
        }
    }

    fn hashed_bytes(len: usize) -> usize {
        let key = HashKey::Text("x".repeat(len).into_boxed_str());
        let mut hasher = CountingHasher::default();
        key.hash(&mut hasher);
        hasher.bytes
    }

    let small = hashed_bytes(64);
    for len in [1_000usize, 100_000, 1_000_000] {
        assert_eq!(
            hashed_bytes(len),
            small,
            "hashing a {len}-byte key fed a different volume than a 64-byte one"
        );
    }
}

/// The probe and the stored key must still agree after the sampling change --
/// including for keys built to defeat sampling.
///
/// Sampling reads the length, eight strided words and the tail, so the keys
/// that stress it are ones sharing a long prefix AND a long suffix and
/// differing only in between.  Those may COLLIDE, which is fine: `equal`
/// decides.  What must never happen is a key failing to find its own entry.
#[test]
fn sampled_string_hashing_still_finds_every_key() {
    let mut eval = Context::new();
    // Only special forms and subrs: `Context::new()` is bare, so `push`
    // and `dolist` (subr.el macros) are not available here.
    let program = r#"(let ((h (make-hash-table :test 'equal))
                          (pre (make-string 5000 ?a))
                          (suf (make-string 5000 ?b))
                          (keys nil)
                          (i 0)
                          (found 0)
                          (rest nil))
                      (while (< i 200)
                        (setq keys (cons (concat pre (number-to-string i) suf) keys))
                        (setq i (1+ i)))
                      (setq rest keys)
                      (while rest
                        (puthash (car rest) (length (car rest)) h)
                        (setq rest (cdr rest)))
                      (setq rest keys)
                      (while rest
                        (if (gethash (car rest) h) (setq found (1+ found)))
                        (setq rest (cdr rest)))
                      (list (hash-table-count h)
                            found
                            (gethash (concat pre "999999" suf) h)
                            (progn (setq rest keys)
                                   (while rest
                                     (puthash (car rest) 0 h)
                                     (setq rest (cdr rest)))
                                   (hash-table-count h))))"#;
    let observed = crate::emacs_core::format_eval_result(&eval.eval_str(program));
    assert_eq!(observed, "OK (200 200 nil 200)");
}
