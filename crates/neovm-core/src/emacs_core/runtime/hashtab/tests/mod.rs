use super::*;
use crate::emacs_core::builtins::{
    builtin_gethash, builtin_hash_table_count, builtin_make_hash_table, builtin_puthash,
    builtin_remhash,
};
use crate::emacs_core::intern::{intern, intern_uninterned, lookup_interned};
use crate::heap_types::LispString;

#[test]
fn hash_table_keys_values_basics() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Equal);
    if table.is_hash_table() {
        let _ = table.with_hash_table_mut(|raw| {
            let test = raw.test.clone();
            let key_alpha = Value::symbol("alpha").to_hash_key(&test);
            raw.insert(key_alpha, Value::symbol("alpha"), Value::fixnum(1));
            let key_beta = Value::symbol("beta").to_hash_key(&test);
            raw.insert(key_beta, Value::symbol("beta"), Value::fixnum(2));
        });
    } else {
        panic!("expected hash table");
    }

    let keys = builtin_hash_table_keys(vec![table]).unwrap();
    let keys = list_to_vec(&keys).expect("proper list");
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().any(|v| v.as_symbol_name() == Some("alpha")));
    assert!(keys.iter().any(|v| v.as_symbol_name() == Some("beta")));

    let values = builtin_hash_table_values(vec![table]).unwrap();
    let values = list_to_vec(&values).expect("proper list");
    assert_eq!(values.len(), 2);
    assert!(values.iter().any(|v| v.as_int() == Some(1)));
    assert!(values.iter().any(|v| v.as_int() == Some(2)));
}

#[test]
fn hash_table_keys_values_errors() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_hash_table_keys(vec![]).is_err());
    assert!(builtin_hash_table_values(vec![]).is_err());
    assert!(builtin_hash_table_keys(vec![Value::NIL]).is_err());
    assert!(builtin_hash_table_values(vec![Value::NIL]).is_err());
}

#[test]
fn hash_table_rehash_defaults() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![]).unwrap();
    let size = builtin_hash_table_rehash_size(vec![table]).unwrap();
    let threshold = builtin_hash_table_rehash_threshold(vec![table]).unwrap();

    assert_eq!(size, Value::make_float(1.5));
    assert_eq!(threshold, Value::make_float(0.8125));
}

#[test]
fn hash_table_rehash_options_are_ignored() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![
        Value::keyword(":rehash-size"),
        Value::make_float(2.0),
        Value::keyword(":rehash-threshold"),
        Value::make_float(0.9),
        Value::keyword(":purecopy"),
        Value::T,
    ])
    .unwrap();

    let size = builtin_hash_table_rehash_size(vec![table]).unwrap();
    let threshold = builtin_hash_table_rehash_threshold(vec![table]).unwrap();

    assert_eq!(size, Value::make_float(1.5));
    assert_eq!(threshold, Value::make_float(0.8125));

    assert!(
        builtin_make_hash_table(vec![
            Value::keyword(":rehash-size"),
            Value::string("x"),
            Value::keyword(":rehash-threshold"),
            Value::make_float(1.5),
        ])
        .is_ok()
    );
    assert!(
        builtin_make_hash_table(vec![
            Value::keyword(":rehash-threshold"),
            Value::string("x"),
            Value::keyword(":rehash-size"),
            Value::make_float(1.5),
        ])
        .is_ok()
    );
}

#[test]
fn gethash_large_table_lookup_does_not_copy_table() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Eq);
    let entries = 20_000;

    for i in 0..entries {
        builtin_puthash(vec![Value::fixnum(i), Value::fixnum(i * 2), table]).unwrap();
    }

    let start = std::time::Instant::now();
    for i in 0..entries {
        let value = builtin_gethash(vec![Value::fixnum(i), table]).unwrap();
        assert_eq!(value, Value::fixnum(i * 2));
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "gethash on a large table should borrow the table, not copy it; elapsed={elapsed:?}"
    );
}

#[test]
fn remhash_unlinks_entry_slot_without_compacting_table_order() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Eq);

    for i in 0..16 {
        builtin_puthash(vec![Value::fixnum(i), Value::fixnum(i * 10), table]).unwrap();
    }
    builtin_remhash(vec![Value::fixnum(5), table]).unwrap();

    let removed_key = {
        let raw = table.as_hash_table().unwrap();
        Value::fixnum(5).to_hash_key(&raw.test)
    };
    let raw = table.as_hash_table().unwrap();
    assert_eq!(raw.data.len(), 15);
    assert!(!raw.data.contains_key(&removed_key));
    assert_eq!(raw.entry_slot_count(), 16);
    assert_eq!(raw.live_hash_keys_in_slot_order().len(), 15);

    builtin_puthash(vec![Value::fixnum(5), Value::fixnum(50), table]).unwrap();

    let raw = table.as_hash_table().unwrap();
    assert_eq!(raw.data.len(), 16);
    assert_eq!(raw.entry_slot_count(), 16);
    assert_eq!(raw.live_hash_keys_in_slot_order().len(), 16);
    assert_eq!(
        builtin_gethash(vec![Value::fixnum(5), table]).unwrap(),
        Value::fixnum(50)
    );
}

#[test]
fn sxhash_variants_return_fixnums_and_preserve_hash_contracts() {
    crate::test_utils::init_test_tracing();
    assert!(
        builtin_sxhash_eq(vec![Value::symbol("foo")])
            .unwrap()
            .is_fixnum()
    );
    assert!(
        builtin_sxhash_eql(vec![Value::symbol("foo")])
            .unwrap()
            .is_fixnum()
    );
    assert!(
        builtin_sxhash_equal(vec![Value::symbol("foo")])
            .unwrap()
            .is_fixnum()
    );
    assert!(
        builtin_sxhash_equal_including_properties(vec![Value::symbol("foo")])
            .unwrap()
            .is_fixnum()
    );

    let left = Value::string("x");
    let right = Value::string("x");
    assert_eq!(
        builtin_sxhash_equal(vec![left]).unwrap(),
        builtin_sxhash_equal(vec![right]).unwrap()
    );
    assert_eq!(
        builtin_sxhash_equal_including_properties(vec![left]).unwrap(),
        builtin_sxhash_equal_including_properties(vec![right]).unwrap()
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::list(vec![Value::fixnum(1), Value::fixnum(2)])]).unwrap(),
        builtin_sxhash_equal(vec![Value::list(vec![Value::fixnum(1), Value::fixnum(2)])]).unwrap()
    );
}

#[test]
fn sxhash_equal_handles_raw_unibyte_strings() {
    crate::test_utils::init_test_tracing();
    let left = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A',
    ]));
    let right = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A',
    ]));

    assert_eq!(
        builtin_sxhash_equal(vec![left]).unwrap(),
        builtin_sxhash_equal(vec![right]).unwrap()
    );
    assert_eq!(
        builtin_sxhash_equal_including_properties(vec![left]).unwrap(),
        builtin_sxhash_equal_including_properties(vec![right]).unwrap()
    );
}

#[test]
fn sxhash_equal_matches_oracle_for_small_int_and_string_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_sxhash_equal(vec![Value::string("a")]).unwrap(),
        Value::fixnum(109)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::string("b")]).unwrap(),
        Value::fixnum(110)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::string("ab")]).unwrap(),
        Value::fixnum(31265)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(2)
    );
}

#[test]
fn sxhash_eq_eql_fixnum_and_char_match_oracle_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_sxhash_eq(vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(6)
    );
    assert_eq!(
        builtin_sxhash_eq(vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_sxhash_eq(vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(4)
    );
    assert_eq!(
        builtin_sxhash_eq(vec![Value::fixnum(65)]).unwrap(),
        Value::fixnum(86)
    );
    assert_eq!(
        builtin_sxhash_eq(vec![Value::fixnum(97)]).unwrap(),
        Value::fixnum(126)
    );
    assert_eq!(
        builtin_sxhash_eq(vec![Value::fixnum(-1)]).unwrap(),
        Value::fixnum(-1_152_921_504_606_846_969)
    );
    assert_eq!(
        builtin_sxhash_eq(vec![Value::fixnum(-2)]).unwrap(),
        Value::fixnum(-1_152_921_504_606_846_973)
    );

    assert_eq!(
        builtin_sxhash_eql(vec![Value::fixnum(65)]).unwrap(),
        Value::fixnum(86)
    );
    assert_eq!(
        builtin_sxhash_eql(vec![Value::char('A')]).unwrap(),
        Value::fixnum(86)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::fixnum(65)]).unwrap(),
        Value::fixnum(81)
    );
}

#[test]
fn sxhash_float_matches_oracle_fixnum_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_sxhash_eql(vec![Value::make_float(1.0)]).unwrap(),
        Value::fixnum(-1_149_543_804_886_319_104)
    );
    assert_eq!(
        builtin_sxhash_eql(vec![Value::make_float(2.0)]).unwrap(),
        Value::fixnum(1_152_921_504_606_846_976)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::make_float(1.0)]).unwrap(),
        Value::fixnum(-1_149_543_804_886_319_104)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::make_float(2.0)]).unwrap(),
        Value::fixnum(1_152_921_504_606_846_976)
    );
}

#[test]
fn unintern_with_nil_obarray_targets_initial_obarray() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray_mut().intern("vm-unintern-single");
    assert!(eval.obarray().intern_soft("vm-unintern-single").is_some());

    // GNU `unintern` requires the OBARRAY arg (arity 2,2); a nil OBARRAY targets
    // the default/initial obarray.
    let removed = builtin_unintern(
        &mut eval,
        vec![Value::string("vm-unintern-single"), Value::NIL],
    )
    .expect("unintern with nil obarray should target the initial obarray");

    assert_eq!(removed, Value::T);
    assert!(eval.obarray().intern_soft("vm-unintern-single").is_none());
}

#[test]
fn unintern_symbol_argument_removes_only_the_exact_symbol() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let name = "vm-unintern-exact-symbol";
    let canonical = intern(name);
    let uninterned = intern_uninterned(name);
    eval.obarray_mut().ensure_interned_global_id(canonical);

    let removed_shadow = builtin_unintern(&mut eval, vec![Value::symbol(uninterned), Value::NIL])
        .expect("unintern should accept uninterned symbol argument");
    assert!(removed_shadow.is_nil());
    assert!(
        eval.obarray().intern_soft(name).is_some(),
        "uninterned shadow must not remove canonical namesake"
    );

    let removed_canonical = builtin_unintern(&mut eval, vec![Value::symbol(canonical), Value::NIL])
        .expect("unintern should remove exact canonical symbol");
    assert_eq!(removed_canonical, Value::T);
    assert!(eval.obarray().intern_soft(name).is_none());
}

#[test]
fn intern_after_unintern_allocates_fresh_symbol_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let name = "vm-unintern-fresh-symbol-after-remove";
    let original = eval
        .obarray_mut()
        .intern_lisp_string(&LispString::from_utf8(name));

    let removed = builtin_unintern(&mut eval, vec![Value::from_sym_id(original), Value::NIL])
        .expect("unintern should remove exact canonical symbol");
    assert_eq!(removed, Value::T);

    let reinterned = eval
        .obarray_mut()
        .intern_lisp_string(&LispString::from_utf8(name));
    assert_ne!(
        original, reinterned,
        "GNU unintern marks the old symbol uninterned; a later intern creates a new object"
    );
    assert!(eval.obarray().intern_soft(name).is_some());
}

#[test]
fn unintern_missing_string_does_not_intern_new_canonical_symbol() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let missing = "vm-unintern-missing-6f20e7c2-c0d2-4d63-b337-93ac7fe9a6bd";
    assert!(lookup_interned(missing).is_none());

    let removed = builtin_unintern(&mut eval, vec![Value::string(missing), Value::NIL])
        .expect("unintern should return nil for missing names");

    assert!(removed.is_nil());
    assert!(
        lookup_interned(missing).is_none(),
        "missing-name unintern must not allocate a canonical symbol"
    );
}

#[test]
fn sxhash_float_signed_zero_and_nan_semantics_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_sxhash_eql(vec![Value::make_float(0.0)]).unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_sxhash_eql(vec![Value::make_float(-0.0)]).unwrap(),
        Value::fixnum(-2_305_843_009_213_693_952)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::make_float(0.0)]).unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![Value::make_float(-0.0)]).unwrap(),
        Value::fixnum(-2_305_843_009_213_693_952)
    );

    let nan = Value::make_float(0.0_f64 / 0.0_f64);
    let nan_eql = builtin_sxhash_eql(vec![nan]).unwrap();
    let nan_equal = builtin_sxhash_equal(vec![nan]).unwrap();
    assert_eq!(nan_eql, nan_equal);

    for test_name in ["eql", "equal"] {
        let table =
            builtin_make_hash_table(vec![Value::keyword(":test"), Value::symbol(test_name)])
                .expect("hash table");
        let _ = builtin_puthash(vec![Value::make_float(0.0), Value::symbol("zero"), table])
            .expect("puthash zero");
        assert_eq!(
            builtin_gethash(vec![Value::make_float(-0.0), table, Value::symbol("miss")])
                .expect("gethash -0.0"),
            Value::symbol("miss")
        );

        let _ = builtin_puthash(vec![nan, Value::symbol("nan"), table]).expect("puthash nan");
        assert_eq!(
            builtin_gethash(vec![nan, table, Value::symbol("miss")]).expect("gethash nan"),
            Value::symbol("nan")
        );
    }
}

#[test]
fn hash_table_nan_payloads_remain_distinct_for_eql_and_equal() {
    crate::test_utils::init_test_tracing();
    let nan_a = Value::make_float(f64::from_bits(0x7ff8_0000_0000_0000));
    let nan_b = Value::make_float(f64::from_bits(0x7ff8_0000_0000_0001));
    assert_eq!(
        builtin_sxhash_eql(vec![nan_a]).unwrap(),
        builtin_sxhash_equal(vec![nan_a]).unwrap()
    );
    assert_eq!(
        builtin_sxhash_eql(vec![nan_b]).unwrap(),
        builtin_sxhash_equal(vec![nan_b]).unwrap()
    );
    assert_ne!(
        builtin_sxhash_eql(vec![nan_a]).unwrap(),
        builtin_sxhash_eql(vec![nan_b]).unwrap()
    );

    for test_name in ["eql", "equal"] {
        let table = builtin_make_hash_table(vec![
            Value::keyword(":test"),
            Value::symbol(test_name),
            Value::keyword(":size"),
            Value::fixnum(5),
        ])
        .expect("hash table");
        let _ = builtin_puthash(vec![nan_a, Value::symbol("a"), table]).expect("puthash nan-a");
        let _ = builtin_puthash(vec![nan_b, Value::symbol("b"), table]).expect("puthash nan-b");
        assert_eq!(
            builtin_hash_table_count(vec![table]).expect("hash-table-count"),
            Value::fixnum(2)
        );
        assert_eq!(
            builtin_gethash(vec![nan_a, table, Value::symbol("miss")]).expect("gethash nan-a"),
            Value::symbol("a")
        );
        assert_eq!(
            builtin_gethash(vec![nan_b, table, Value::symbol("miss")]).expect("gethash nan-b"),
            Value::symbol("b")
        );

        let buckets = builtin_internal_hash_table_buckets(vec![table]).expect("bucket diagnostics");
        let outer = list_to_vec(&buckets).expect("outer list");
        let mut hashes = Vec::new();
        for bucket in outer {
            let entries = list_to_vec(&bucket).expect("bucket alist");
            for entry in entries {
                if !entry.is_cons() {
                    panic!("expected alist cons entry");
                };
                let _pair_car = entry.cons_car();
                let pair_cdr = entry.cons_cdr();
                hashes.push(pair_cdr.as_int().expect("diagnostic hash integer"));
            }
        }
        hashes.sort_unstable();
        assert_eq!(hashes.len(), 2);
        assert_ne!(hashes[0], hashes[1]);
    }
}

#[test]
fn internal_hash_table_introspection_empty_defaults() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![]).unwrap();
    assert_eq!(
        builtin_internal_hash_table_buckets(vec![table]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_internal_hash_table_histogram(vec![table]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![table]).unwrap(),
        Value::fixnum(1)
    );
}

#[test]
fn internal_hash_table_index_size_uses_declared_size() {
    crate::test_utils::init_test_tracing();
    let table_one = builtin_make_hash_table(vec![Value::keyword(":size"), Value::fixnum(1)])
        .expect("size 1 table");
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![table_one]).unwrap(),
        Value::fixnum(2)
    );

    let table_mid = builtin_make_hash_table(vec![Value::keyword(":size"), Value::fixnum(37)])
        .expect("size 37 table");
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![table_mid]).unwrap(),
        Value::fixnum(64)
    );
}

#[test]
fn internal_hash_table_index_size_tracks_growth_boundaries() {
    crate::test_utils::init_test_tracing();
    let tiny = builtin_make_hash_table(vec![Value::keyword(":size"), Value::fixnum(1)])
        .expect("size 1 table");
    let _ = builtin_puthash(vec![Value::fixnum(1), Value::symbol("x"), tiny])
        .expect("puthash for first tiny entry");
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![tiny]).unwrap(),
        Value::fixnum(2)
    );
    let _ = builtin_puthash(vec![Value::fixnum(2), Value::symbol("y"), tiny])
        .expect("puthash for second tiny entry");
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![tiny]).unwrap(),
        Value::fixnum(32)
    );

    let default_table = builtin_make_hash_table(vec![]).expect("default table");
    let _ = builtin_puthash(vec![Value::fixnum(1), Value::symbol("x"), default_table])
        .expect("puthash for default table");
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![default_table]).unwrap(),
        Value::fixnum(8)
    );

    let mid = builtin_make_hash_table(vec![Value::keyword(":size"), Value::fixnum(10)])
        .expect("size 10 table");
    for i in 0..10 {
        let i = i as i64;
        let _ = builtin_puthash(vec![Value::fixnum(i), Value::fixnum(i), mid])
            .expect("puthash while filling size 10 table");
    }
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![mid]).unwrap(),
        Value::fixnum(16)
    );
    let _ = builtin_puthash(vec![Value::fixnum(10), Value::fixnum(10), mid])
        .expect("puthash crossing size 10 threshold");
    assert_eq!(
        builtin_internal_hash_table_index_size(vec![mid]).unwrap(),
        Value::fixnum(64)
    );
}

#[test]
fn hash_table_size_tracks_growth_boundaries() {
    crate::test_utils::init_test_tracing();
    let tiny = builtin_make_hash_table(vec![Value::keyword(":size"), Value::fixnum(1)])
        .expect("size 1 table");
    let _ = builtin_puthash(vec![Value::fixnum(1), Value::symbol("x"), tiny])
        .expect("puthash for first tiny entry");
    assert_eq!(
        builtin_hash_table_size(vec![tiny]).unwrap(),
        Value::fixnum(1)
    );
    let _ = builtin_puthash(vec![Value::fixnum(2), Value::symbol("y"), tiny])
        .expect("puthash for second tiny entry");
    assert_eq!(
        builtin_hash_table_size(vec![tiny]).unwrap(),
        Value::fixnum(24)
    );

    let default_table = builtin_make_hash_table(vec![]).expect("default table");
    let _ = builtin_puthash(vec![
        Value::fixnum(1),
        Value::symbol("default-value"),
        default_table,
    ])
    .expect("puthash for default table");
    assert_eq!(
        builtin_hash_table_size(vec![default_table]).unwrap(),
        Value::fixnum(6)
    );

    let mid = builtin_make_hash_table(vec![Value::keyword(":size"), Value::fixnum(10)])
        .expect("size 10 table");
    for i in 0..11 {
        let i = i as i64;
        let _ = builtin_puthash(vec![Value::fixnum(i), Value::fixnum(i), mid])
            .expect("puthash while filling size 10 table");
    }
    assert_eq!(
        builtin_hash_table_size(vec![mid]).unwrap(),
        Value::fixnum(40)
    );
}

#[test]
fn logical_hash_table_growth_does_not_eagerly_reserve_every_index() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![Value::keyword(":size"), Value::fixnum(1)])
        .expect("size 1 table");
    let _ = builtin_puthash(vec![Value::fixnum(1), Value::symbol("x"), table])
        .expect("puthash for first tiny entry");
    let _ = builtin_puthash(vec![Value::fixnum(2), Value::symbol("y"), table])
        .expect("puthash crossing the logical growth boundary");

    table.with_hash_table_mut(|raw| {
        let logical_size = usize::try_from(raw.size).unwrap();
        assert_eq!(logical_size, 24);
        assert!(raw.data.capacity() < logical_size);
        let five_key_legacy_floor = logical_size
            .saturating_mul(5)
            .saturating_mul(std::mem::size_of::<HashKey>());
        assert!(raw.data.known_storage_bytes() < five_key_legacy_floor);
    });
}

#[test]
fn hash_key_inline_footprint_stays_within_three_words() {
    crate::test_utils::init_test_tracing();
    assert!(
        std::mem::size_of::<HashKey>() <= 3 * std::mem::size_of::<usize>(),
        "large structural variants must stay behind pointers so every hash-table bucket stays compact"
    );
}

#[test]
fn internal_hash_table_buckets_report_hash_diagnostics() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![
        Value::keyword(":test"),
        Value::symbol("equal"),
        Value::keyword(":size"),
        Value::fixnum(3),
    ])
    .expect("hash table");
    if table.is_hash_table() {
        let _ = table.with_hash_table_mut(|raw| {
            let test = raw.test.clone();
            let key_a = Value::string("a").to_hash_key(&test);
            raw.insert(key_a, Value::string("a"), Value::symbol("value-a"));
            let key_b = Value::string("b").to_hash_key(&test);
            raw.insert(key_b, Value::string("b"), Value::symbol("value-b"));
        });
    } else {
        panic!("expected hash table");
    }

    let buckets = builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists");
    let outer = list_to_vec(&buckets).expect("outer list");
    let mut seen = std::collections::BTreeMap::new();
    for bucket in outer {
        let entries = list_to_vec(&bucket).expect("bucket alist");
        for entry in entries {
            if !entry.is_cons() {
                panic!("expected alist cons entry");
            };
            let pair_car = entry.cons_car();
            let pair_cdr = entry.cons_cdr();
            let key = pair_car.as_utf8_str().expect("string key").to_string();
            let hash = pair_cdr.as_int().expect("diagnostic hash integer");
            seen.insert(key, hash);
        }
    }

    assert_eq!(seen.len(), 2);
    assert!(seen.contains_key("a"));
    assert!(seen.contains_key("b"));
}

#[test]
fn internal_hash_table_buckets_match_oracle_small_string_hashes() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![
        Value::keyword(":test"),
        Value::symbol("equal"),
        Value::keyword(":size"),
        Value::fixnum(3),
    ])
    .expect("hash table");
    let _ = builtin_puthash(vec![Value::string("a"), Value::fixnum(1), table]).expect("puthash a");
    let _ = builtin_puthash(vec![Value::string("b"), Value::fixnum(2), table]).expect("puthash b");

    assert_eq!(
        builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists"),
        Value::list(vec![
            Value::list(vec![Value::cons(Value::string("b"), Value::fixnum(114))]),
            Value::list(vec![Value::cons(Value::string("a"), Value::fixnum(113))]),
        ])
    );
}

#[test]
fn internal_hash_table_buckets_match_oracle_eq_eql_fixnum_hashes() {
    crate::test_utils::init_test_tracing();
    for test_name in ["eq", "eql"] {
        let table = builtin_make_hash_table(vec![
            Value::keyword(":test"),
            Value::symbol(test_name),
            Value::keyword(":size"),
            Value::fixnum(3),
        ])
        .expect("hash table");
        let _ = builtin_puthash(vec![Value::char('A'), Value::symbol("char"), table])
            .expect("puthash char");
        assert_eq!(
            builtin_gethash(vec![Value::fixnum(65), table, Value::symbol("miss")])
                .expect("gethash int"),
            Value::symbol("char")
        );
        assert_eq!(
            builtin_gethash(vec![Value::char('A'), table, Value::symbol("miss")])
                .expect("gethash char"),
            Value::symbol("char")
        );
        assert_eq!(
            builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists"),
            Value::list(vec![Value::list(vec![Value::cons(
                Value::fixnum(65),
                Value::fixnum(71)
            )])])
        );
    }

    let table = builtin_make_hash_table(vec![
        Value::keyword(":test"),
        Value::symbol("equal"),
        Value::keyword(":size"),
        Value::fixnum(3),
    ])
    .expect("hash table");
    let _ = builtin_puthash(vec![Value::char('A'), Value::symbol("char"), table])
        .expect("puthash char");
    assert_eq!(
        builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists"),
        Value::list(vec![Value::list(vec![Value::cons(
            Value::fixnum(65),
            Value::fixnum(65)
        )])])
    );
}

#[test]
fn internal_hash_table_buckets_eq_pointer_keys_keep_distinct_hashes() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![
        Value::keyword(":test"),
        Value::symbol("eq"),
        Value::keyword(":size"),
        Value::fixnum(5),
    ])
    .expect("hash table");
    let key_a = Value::string("x");
    let key_b = Value::string("x");
    let _ = builtin_puthash(vec![key_a, Value::symbol("a"), table]).expect("puthash key-a");
    let _ = builtin_puthash(vec![key_b, Value::symbol("b"), table]).expect("puthash key-b");
    assert_eq!(
        builtin_hash_table_count(vec![table]).expect("hash-table-count"),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_gethash(vec![key_a, table, Value::symbol("miss")]).expect("gethash a"),
        Value::symbol("a")
    );
    assert_eq!(
        builtin_gethash(vec![key_b, table, Value::symbol("miss")]).expect("gethash b"),
        Value::symbol("b")
    );

    let buckets = builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists");
    let outer = list_to_vec(&buckets).expect("outer list");
    let mut hashes = Vec::new();
    let mut keys = Vec::new();
    for bucket in outer {
        let entries = list_to_vec(&bucket).expect("bucket alist");
        for entry in entries {
            if !entry.is_cons() {
                panic!("expected alist cons entry");
            };
            let pair_car = entry.cons_car();
            let pair_cdr = entry.cons_cdr();
            keys.push(pair_car);
            hashes.push(pair_cdr.as_int().expect("diagnostic hash integer"));
        }
    }
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().all(|key| key.as_utf8_str().is_some()));
    hashes.sort_unstable();
    assert_eq!(hashes.len(), 2);
    assert_ne!(hashes[0], hashes[1]);
}

#[test]
fn internal_hash_table_buckets_equal_preserve_first_key_identity_on_overwrite() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_hash_table(vec![
        Value::keyword(":test"),
        Value::symbol("equal"),
        Value::keyword(":size"),
        Value::fixnum(5),
    ])
    .expect("hash table");
    let key_a = Value::string("x");
    let key_b = Value::string("x");
    let _ = builtin_puthash(vec![key_a, Value::symbol("a"), table]).expect("puthash key-a");
    let _ =
        builtin_puthash(vec![key_b, Value::symbol("b"), table]).expect("puthash key-b overwrite");
    assert_eq!(
        builtin_hash_table_count(vec![table]).expect("hash-table-count"),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_gethash(vec![Value::string("x"), table, Value::symbol("miss")]).expect("gethash x"),
        Value::symbol("b")
    );

    let buckets = builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists");
    let outer = list_to_vec(&buckets).expect("outer list");
    assert_eq!(outer.len(), 1);
    let entries = list_to_vec(&outer[0]).expect("bucket alist");
    assert_eq!(entries.len(), 1);
    if !&entries[0].is_cons() {
        panic!("expected alist cons entry");
    };
    let pair_car = entries[0].cons_car();
    assert_eq!(pair_car.as_utf8_str(), Some("x"));
    assert!(eq_value(&pair_car, &key_a));
    assert!(!eq_value(&pair_car, &key_b));
}

#[test]
fn internal_hash_table_buckets_match_oracle_small_float_hashes() {
    crate::test_utils::init_test_tracing();
    fn collect_float_hashes(table: Value) -> std::collections::BTreeMap<u64, i64> {
        let buckets = builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists");
        let outer = list_to_vec(&buckets).expect("outer list");
        let mut seen = std::collections::BTreeMap::new();
        for bucket in outer {
            let entries = list_to_vec(&bucket).expect("bucket alist");
            for entry in entries {
                if !entry.is_cons() {
                    panic!("expected alist cons entry");
                };
                let pair_car = entry.cons_car();
                let pair_cdr = entry.cons_cdr();
                let key_bits = pair_car.as_float().expect("float key").to_bits();
                let hash = pair_cdr.as_int().expect("diagnostic hash integer");
                seen.insert(key_bits, hash);
            }
        }
        seen
    }

    let expected = std::collections::BTreeMap::from([
        (1.0_f64.to_bits(), 1_072_693_248_i64),
        (2.0_f64.to_bits(), 1_073_741_824_i64),
    ]);

    for test_name in ["eql", "equal"] {
        let table = builtin_make_hash_table(vec![
            Value::keyword(":test"),
            Value::symbol(test_name),
            Value::keyword(":size"),
            Value::fixnum(3),
        ])
        .expect("hash table");
        let _ = builtin_puthash(vec![Value::make_float(1.0), Value::fixnum(1), table])
            .expect("puthash 1.0");
        let _ = builtin_puthash(vec![Value::make_float(2.0), Value::fixnum(2), table])
            .expect("puthash 2.0");

        assert_eq!(collect_float_hashes(table), expected);
    }
}

#[test]
fn internal_hash_table_buckets_match_oracle_float_special_hashes() {
    crate::test_utils::init_test_tracing();
    fn collect_hashes(table: Value) -> Vec<i64> {
        let buckets = builtin_internal_hash_table_buckets(vec![table]).expect("bucket alists");
        let outer = list_to_vec(&buckets).expect("outer list");
        let mut seen = Vec::new();
        for bucket in outer {
            let entries = list_to_vec(&bucket).expect("bucket alist");
            for entry in entries {
                if !entry.is_cons() {
                    panic!("expected alist cons entry");
                };
                let _pair_car = entry.cons_car();
                let pair_cdr = entry.cons_cdr();
                let hash = pair_cdr.as_int().expect("diagnostic hash integer");
                seen.push(hash);
            }
        }
        seen.sort_unstable();
        seen
    }

    let expected = vec![0_i64, 2_146_959_360_i64, 2_147_483_648_i64];
    for test_name in ["eql", "equal"] {
        let table = builtin_make_hash_table(vec![
            Value::keyword(":test"),
            Value::symbol(test_name),
            Value::keyword(":size"),
            Value::fixnum(5),
        ])
        .expect("hash table");
        let _ = builtin_puthash(vec![Value::make_float(-0.0), Value::symbol("neg"), table])
            .expect("puthash -0.0");
        let _ = builtin_puthash(vec![Value::make_float(0.0), Value::symbol("pos"), table])
            .expect("puthash 0.0");
        let _ = builtin_puthash(vec![
            Value::make_float(f64::NAN),
            Value::symbol("nan"),
            table,
        ])
        .expect("puthash nan");
        assert_eq!(collect_hashes(table), expected);
    }
}

#[test]
fn internal_hash_table_introspection_type_errors() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_internal_hash_table_buckets(vec![Value::NIL]).is_err());
    assert!(builtin_internal_hash_table_histogram(vec![Value::NIL]).is_err());
    assert!(builtin_internal_hash_table_index_size(vec![Value::NIL]).is_err());
}

// GNU `Sunintern` is `2, 2`: the OBARRAY argument is mandatory (may be nil, but
// must be supplied).  `(unintern "zzz")` signals
// `(wrong-number-of-arguments unintern 1)`.
#[test]
fn unintern_requires_obarray_argument() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();

    // One-argument call: GNU signals wrong-number-of-arguments.
    let one_arg = ctx
        .eval_str(r#"(condition-case e (unintern "zzz") (error e))"#)
        .expect("condition-case should catch the arity error");
    let parts = list_to_vec(&one_arg).expect("error is a proper list");
    assert_eq!(
        parts.len(),
        3,
        "expected (wrong-number-of-arguments unintern 1)"
    );
    assert_eq!(parts[0].as_symbol_name(), Some("wrong-number-of-arguments"));
    assert_eq!(parts[1].as_symbol_name(), Some("unintern"));
    assert_eq!(parts[2].as_int(), Some(1));

    // Two-argument call (OBARRAY nil) still works and returns nil for an absent
    // symbol, exactly like GNU.
    let two_arg = ctx
        .eval_str(r#"(unintern "zzz" nil)"#)
        .expect("two-arg unintern should succeed");
    assert!(two_arg.is_nil(), "absent symbol returns nil");
}

// GNU `Fclrhash` returns the cleared table itself (for XEmacs compatibility),
// not nil.  `(eq (clrhash h) h)` must be t.
#[test]
fn clrhash_returns_the_hash_table_not_nil() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = ctx
        .eval_str("(let ((h (make-hash-table))) (puthash 'a 1 h) (eq (clrhash h) h))")
        .expect("clrhash should evaluate");
    assert!(
        result.is_truthy(),
        "clrhash must return the table arg so (eq (clrhash h) h) is t"
    );
}

// GNU `remhash` dispatches its key lookup through the table's `cmpfn`, so a
// table created with a user-defined `:test` removes the entry whose key the
// custom comparator matches -- not just the literal `eq`/`eql`/`equal` key.
// Here `(mod 17 7) == (mod 10 7) == 3`, so `(remhash 17 h)` must delete the
// `10 -> ten` entry, leaving the table empty.
#[test]
fn remhash_dispatches_user_defined_test() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = ctx
        .eval_str(
            r#"(progn
                 (define-hash-table-test 'mod7
                   (lambda (a b) (= (mod a 7) (mod b 7)))
                   (lambda (k) (mod k 7)))
                 (let ((h (make-hash-table :test 'mod7)))
                   (puthash 10 'ten h)
                   (remhash 17 h)
                   (list (gethash 10 h) (hash-table-count h))))"#,
        )
        .expect("remhash with user-defined test should evaluate");
    let parts = list_to_vec(&result).expect("result is a proper list");
    assert_eq!(parts.len(), 2);
    assert!(
        parts[0].is_nil(),
        "remhash with custom test must have removed the 10->ten entry (got non-nil)"
    );
    assert_eq!(
        parts[1].as_int(),
        Some(0),
        "hash-table-count must be 0 after the custom-test removal"
    );
}

// The default `eq`/`eql`/`equal` remhash path must still work after teaching
// remhash to dispatch user-defined tests.
#[test]
fn remhash_default_equal_test_still_works() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = ctx
        .eval_str(
            r#"(let ((h (make-hash-table :test 'equal)))
                 (puthash "x" 1 h)
                 (puthash "y" 2 h)
                 (remhash "x" h)
                 (list (gethash "x" h) (gethash "y" h) (hash-table-count h)))"#,
        )
        .expect("default-test remhash should evaluate");
    let parts = list_to_vec(&result).expect("result is a proper list");
    assert_eq!(parts.len(), 3);
    assert!(parts[0].is_nil(), "removed key \"x\" must now be absent");
    assert_eq!(parts[1].as_int(), Some(2), "untouched key \"y\" stays");
    assert_eq!(parts[2].as_int(), Some(1), "exactly one entry remains");
}

// eq/eql default tests: remhash removes the matching key and leaves others.
#[test]
fn remhash_default_eq_eql_tests_still_work() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();

    let eq_result = ctx
        .eval_str(
            r#"(let ((h (make-hash-table :test 'eq)))
                 (puthash 'a 1 h)
                 (puthash 'b 2 h)
                 (remhash 'a h)
                 (list (gethash 'a h) (gethash 'b h) (hash-table-count h)))"#,
        )
        .expect("eq remhash should evaluate");
    let eq_parts = list_to_vec(&eq_result).expect("proper list");
    assert!(eq_parts[0].is_nil());
    assert_eq!(eq_parts[1].as_int(), Some(2));
    assert_eq!(eq_parts[2].as_int(), Some(1));

    let eql_result = ctx
        .eval_str(
            r#"(let ((h (make-hash-table :test 'eql)))
                 (puthash 1.5 'a h)
                 (puthash 2.5 'b h)
                 (remhash 1.5 h)
                 (list (gethash 1.5 h) (gethash 2.5 h) (hash-table-count h)))"#,
        )
        .expect("eql remhash should evaluate");
    let eql_parts = list_to_vec(&eql_result).expect("proper list");
    assert!(eql_parts[0].is_nil());
    assert_eq!(eql_parts[1].as_symbol_name(), Some("b"));
    assert_eq!(eql_parts[2].as_int(), Some(1));
}

/// R3b-lite: a table whose entries are parked (the lazy pdump-load form)
/// must hydrate transparently at the `as_hash_table` /
/// `with_hash_table_mut` choke points, and its parked entries must be
/// visible to the GC child enumeration before hydration.
#[test]
fn pending_dump_entries_hydrate_at_first_access() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Eq);

    // Park two entries the way the pdump loader does, WITHOUT hydrating:
    // build the tuples through the raw pointer so no accessor runs first.
    let k1 = Value::fixnum(7).to_hash_key(&HashTableTest::Eq);
    let k2 = Value::symbol("lazy-key").to_hash_key(&HashTableTest::Eq);
    {
        let ptr = table.as_veclike_ptr().unwrap() as *mut crate::tagged::header::HashTableObj;
        unsafe {
            (*ptr).table.set_pending_dump_entries(vec![
                (k1.clone(), Value::fixnum(71), None),
                (
                    k2.clone(),
                    Value::fixnum(72),
                    Some(Value::symbol("lazy-key")),
                ),
            ]);
            assert!((*ptr).table.needs_hydration());
            // GC-visible while parked.
            let pending = (*ptr).table.data.pending_entries().unwrap();
            assert_eq!(pending.len(), 2);
        }
    }

    // First accessor hydrates; lookups observe the parked entries.
    let ht = table.as_hash_table().unwrap();
    assert!(!ht.needs_hydration());
    assert_eq!(ht.data.get(&k1).copied(), Some(Value::fixnum(71)));
    assert_eq!(ht.data.get(&k2).copied(), Some(Value::fixnum(72)));
    assert_eq!(ht.data.len(), 2);
    assert_eq!(
        ht.key_snapshot(&k2).and_then(|v| v.as_symbol_name()),
        Some("lazy-key")
    );

    // Mutation path stays coherent post-hydration.
    let _ = table.with_hash_table_mut(|raw| {
        raw.data.remove(&k1);
    });
    assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
}
