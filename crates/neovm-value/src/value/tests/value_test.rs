use super::*;
use crate::heap_types::LispMarker;
use crate::intern::{intern, intern_uninterned, resolve_sym};
use crate::tagged::header::CLOSURE_ARGLIST;
use crate::value::buffer_vocab::BufferId;
use malachite::integer::Integer;
use std::str::FromStr;

/// Helper: set up a temporary heap for tests that use Value constructors.
/// With the tagged-pointer runtime the test fallback heap is auto-created,
/// so this wrapper is now a simple pass-through.
fn with_test_heap<R>(f: impl FnOnce() -> R) -> R {
    f()
}

#[test]
fn hash_table_domains_match_gnu_symbols() {
    assert_eq!(
        HashTableTest::from_symbol_value(&Value::symbol("eq")),
        Some(HashTableTest::Eq)
    );
    assert_eq!(
        HashTableTest::from_symbol_value(&Value::symbol("eql")),
        Some(HashTableTest::Eql)
    );
    assert_eq!(
        HashTableTest::from_symbol_value(&Value::symbol("equal")),
        Some(HashTableTest::Equal)
    );
    assert_eq!(
        HashTableTest::from_symbol_name("equal-including-properties"),
        None
    );
    assert_eq!(HashTableTest::Equal.name(), "equal");
    assert_eq!(HashTableTest::Eql.gnu_code(), 0);
    assert_eq!(HashTableTest::Eq.gnu_code(), 1);
    assert_eq!(HashTableTest::Equal.gnu_code(), 2);
    assert_eq!(HashTableTest::from_gnu_code(0), Some(HashTableTest::Eql));
    assert_eq!(HashTableTest::from_gnu_code(1), Some(HashTableTest::Eq));
    assert_eq!(HashTableTest::from_gnu_code(2), Some(HashTableTest::Equal));
    assert_eq!(HashTableTest::from_gnu_code(3), None);

    assert_eq!(
        HashTableWeakness::from_symbol_value(&Value::symbol("key")),
        Some(HashTableWeakness::Key)
    );
    assert_eq!(
        HashTableWeakness::from_symbol_value(&Value::symbol("value")),
        Some(HashTableWeakness::Value)
    );
    assert_eq!(
        HashTableWeakness::from_symbol_value(&Value::symbol("key-or-value")),
        Some(HashTableWeakness::KeyOrValue)
    );
    assert_eq!(
        HashTableWeakness::from_symbol_value(&Value::symbol("key-and-value")),
        Some(HashTableWeakness::KeyAndValue)
    );
    assert_eq!(HashTableWeakness::from_symbol_name("weak"), None);
    assert_eq!(HashTableWeakness::KeyOrValue.name(), "key-or-value");
    assert_eq!(HashTableWeakness::from_gnu_code(0), None);
    assert_eq!(HashTableWeakness::option_from_gnu_code(0), Some(None));
    assert_eq!(HashTableWeakness::Key.gnu_code(), 1);
    assert_eq!(HashTableWeakness::Value.gnu_code(), 2);
    assert_eq!(HashTableWeakness::KeyOrValue.gnu_code(), 3);
    assert_eq!(HashTableWeakness::KeyAndValue.gnu_code(), 4);
    assert_eq!(
        HashTableWeakness::option_from_gnu_code(1),
        Some(Some(HashTableWeakness::Key))
    );
    assert_eq!(
        HashTableWeakness::option_from_gnu_code(4),
        Some(Some(HashTableWeakness::KeyAndValue))
    );
    assert_eq!(HashTableWeakness::option_from_gnu_code(5), None);

    assert_eq!(
        HashTableMakeKeyword::from_symbol_value(&Value::keyword(":test")),
        Some(HashTableMakeKeyword::Test)
    );
    assert_eq!(
        HashTableMakeKeyword::from_symbol_value(&Value::keyword(":rehash-size")),
        Some(HashTableMakeKeyword::RehashSize)
    );
    assert_eq!(
        HashTableMakeKeyword::from_symbol_value(&Value::keyword(":purecopy")),
        Some(HashTableMakeKeyword::Purecopy)
    );
    assert_eq!(HashTableMakeKeyword::from_symbol_name(":data"), None);
    assert_eq!(
        HashTableMakeKeyword::RehashThreshold.name(),
        ":rehash-threshold"
    );

    assert_eq!(
        HashTableLiteralKey::from_symbol_value(&Value::symbol("test")),
        Some(HashTableLiteralKey::Test)
    );
    assert_eq!(
        HashTableLiteralKey::from_symbol_value(&Value::symbol("data")),
        Some(HashTableLiteralKey::Data)
    );
    assert_eq!(
        HashTableLiteralKey::from_symbol_value(&Value::symbol("purecopy")),
        Some(HashTableLiteralKey::Purecopy)
    );
    assert_eq!(HashTableLiteralKey::from_symbol_name(":test"), None);
    assert_eq!(HashTableLiteralKey::RehashSize.name(), "rehash-size");
}

#[test]
fn value_constructors() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        assert!(Value::NIL.is_nil());
        assert!(Value::T.is_truthy());
        assert!(Value::fixnum(42).is_integer());
        assert!(Value::make_float(3.125).is_float());
        assert!(Value::string("hello").is_string());
        assert!(Value::char('a').is_char());
        assert!(Value::symbol("foo").is_bare_symbol());
        assert!(Value::keyword(":bar").is_bare_keyword());
    });
}

#[test]
fn eql_compares_bignums_by_numeric_value() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let left = Value::make_integer(
            Integer::from_str("1000000000000000000000001").expect("valid bignum"),
        );
        let right = Value::make_integer(
            Integer::from_str("1000000000000000000000001").expect("valid bignum"),
        );
        let different = Value::make_integer(
            Integer::from_str("1000000000000000000000002").expect("valid bignum"),
        );

        assert!(left.is_bignum());
        assert!(right.is_bignum());
        assert!(!eq_value(&left, &right));
        assert!(eql_value(&left, &right));
        assert!(!eql_value(&left, &different));
    });
}

#[test]
fn equal_compares_bignums_by_numeric_value() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let left = Value::make_integer(
            Integer::from_str("1267650600228229401496703205376").expect("valid bignum"),
        );
        let right = Value::make_integer(
            Integer::from_str("1267650600228229401496703205376").expect("valid bignum"),
        );
        let different = Value::make_integer(
            Integer::from_str("1267650600228229401496703205377").expect("valid bignum"),
        );

        assert!(left.is_bignum());
        assert!(right.is_bignum());
        assert!(!eq_value(&left, &right));
        assert!(equal_value(&left, &right, 0));
        assert!(!equal_value(&left, &different, 0));
    });
}

#[test]
fn internal_equal_compares_char_table_and_sub_char_table_storage() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let purpose = Value::symbol("neomacs-equal-internal");
        let left = Value::make_char_table(purpose, Value::NIL, 1);
        let right = Value::make_char_table(purpose, Value::NIL, 1);
        let left_sub = Value::make_sub_char_table(3, 128, vec![Value::NIL; 128]);
        let right_sub = Value::make_sub_char_table(3, 128, vec![Value::NIL; 128]);
        left.with_char_table_mut(|table| table.set_contents(1, left_sub))
            .expect("left char-table");
        right
            .with_char_table_mut(|table| table.set_contents(1, right_sub))
            .expect("right char-table");

        assert!(equal_value(&left, &right, 0));

        right_sub
            .with_sub_char_table_mut(|table| {
                table.set_contents(17, Value::T);
            })
            .expect("right sub-char-table");
        assert!(!equal_value(&left, &right, 0));
    });
}

#[test]
fn equal_hash_keys_preserve_structural_pseudovector_types() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let slots = vec![Value::symbol("kind"), Value::fixnum(1)];
        let vector = Value::vector(slots.clone());
        let record = Value::make_record(slots);

        assert!(!equal_value(&vector, &record, 0));
        assert_ne!(
            vector.to_hash_key(&HashTableTest::Equal),
            record.to_hash_key(&HashTableTest::Equal)
        );
    });
}

#[test]
fn make_int_uses_gnu_fixnum_boundary() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let max = Value::make_int(Value::MOST_POSITIVE_FIXNUM);
        assert!(max.is_fixnum());
        assert_eq!(max.as_fixnum(), Some(Value::MOST_POSITIVE_FIXNUM));

        let min = Value::make_int(Value::MOST_NEGATIVE_FIXNUM);
        assert!(min.is_fixnum());
        assert_eq!(min.as_fixnum(), Some(Value::MOST_NEGATIVE_FIXNUM));

        let above = Value::make_int(Value::MOST_POSITIVE_FIXNUM + 1);
        assert!(above.is_bignum());
        assert_eq!(
            above.as_bignum().expect("bignum").to_string(),
            "2305843009213693952"
        );

        let below = Value::make_int(Value::MOST_NEGATIVE_FIXNUM - 1);
        assert!(below.is_bignum());
        assert_eq!(
            below.as_bignum().expect("bignum").to_string(),
            "-2305843009213693953"
        );
    });
}

#[test]
fn list_round_trip() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let lst = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
        let vec = list_to_vec(&lst).unwrap();
        assert_eq!(vec.len(), 3);
    });
}

#[test]
fn eq_identity() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        assert!(eq_value(&Value::NIL, &Value::NIL));
        assert!(eq_value(&Value::fixnum(42), &Value::fixnum(42)));
        assert!(!eq_value(&Value::fixnum(1), &Value::fixnum(2)));
        assert!(eq_value(&Value::char('a'), &Value::fixnum(97)));
        assert!(eq_value(&Value::fixnum(97), &Value::char('a')));
        assert!(eq_value(&Value::symbol("foo"), &Value::symbol("foo")));
    });
}

#[test]
fn keyword_identity_is_consistent_across_constructors() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let keyword_from_symbol_ctor = Value::symbol(":kw");
        let keyword_from_keyword_ctor = Value::keyword(":kw");
        let keyword_from_bare_ctor = Value::keyword("kw");
        let keyword_from_sym_id = Value::keyword_id(intern(":kw"));

        // Keywords are ordinary symbols whose canonical names start with `:`.
        assert!(keyword_from_symbol_ctor.is_bare_keyword());
        assert!(eq_value(
            &keyword_from_symbol_ctor,
            &keyword_from_keyword_ctor
        ));
        assert!(eq_value(&keyword_from_symbol_ctor, &keyword_from_bare_ctor));
        assert!(eq_value(&keyword_from_symbol_ctor, &keyword_from_sym_id));

        // Bare `kw` and keyword `:kw` are distinct GNU symbols.
        let bare_symbol = Value::symbol("kw");
        assert!(!eq_value(&keyword_from_symbol_ctor, &bare_symbol));
        assert!(!equal_value(&keyword_from_symbol_ctor, &bare_symbol, 0));

        for test in [HashTableTest::Eq, HashTableTest::Eql, HashTableTest::Equal] {
            let left = keyword_from_symbol_ctor.to_hash_key(&test);
            let right = bare_symbol.to_hash_key(&test);
            assert_ne!(left, right);
        }
    });
}

#[test]
fn uninterned_colon_name_is_not_treated_as_keyword() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let uninterned = Value::symbol(intern_uninterned(":vm-shadow-keyword"));
        assert!(!uninterned.is_bare_keyword());
        assert!(uninterned.as_bare_keyword_id().is_none());

        let canonical = Value::keyword(":vm-shadow-keyword");
        assert!(canonical.is_bare_keyword());
        assert_eq!(
            canonical.as_bare_keyword_id(),
            Some(intern(":vm-shadow-keyword"))
        );
    });
}

#[test]
fn equal_structural() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let a = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
        let b = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
        assert!(equal_value(&a, &b, 0));
        assert!(!eq_value(&a, &b));
    });
}

#[test]
fn string_equality() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let a = Value::string("hello");
        let b = Value::string("hello");
        assert!(equal_value(&a, &b, 0));
        // eq compares heap object identity — different allocations
        assert!(!eq_value(&a, &b));
    });
}

fn test_marker(buffer: Option<BufferId>, bytepos: usize) -> Value {
    Value::make_marker(LispMarker::new(
        match buffer {
            Some(id) => crate::heap_types::MarkerBuffer::attached(id),
            None => crate::heap_types::MarkerBuffer::DETACHED,
        },
        crate::value::positions::CharPos0::new(bytepos),
        crate::heap_types::byte_positions::EmacsBytePos::new(bytepos),
        false,
        buffer.is_some(),
    ))
}

#[test]
fn marker_equal_matches_gnu_buffer_and_bytepos_rules() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let detached_left = test_marker(None, 4);
        let detached_right = test_marker(None, 5);
        let attached_left = test_marker(Some(BufferId(1)), 4);
        let attached_same = test_marker(Some(BufferId(1)), 4);
        let attached_different_pos = test_marker(Some(BufferId(1)), 5);
        let attached_different_buffer = test_marker(Some(BufferId(2)), 4);

        assert!(equal_value(&detached_left, &detached_right, 0));
        assert!(equal_value(&attached_left, &attached_same, 0));
        assert!(!equal_value(&attached_left, &attached_different_pos, 0));
        assert!(!equal_value(&attached_left, &attached_different_buffer, 0));
    });
}

#[test]
fn closure_equal_is_structural() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let env_a = Value::list(vec![Value::cons(Value::symbol("n"), Value::fixnum(5))]);
        let env_b = Value::list(vec![Value::cons(Value::symbol("n"), Value::fixnum(5))]);
        let env_c = Value::list(vec![Value::cons(Value::symbol("n"), Value::fixnum(10))]);

        let make = |env| {
            Value::make_lambda(LambdaData {
                params: LambdaParams::simple(vec![intern("x")]).into(),
                body: vec![Value::list(vec![
                    Value::symbol("+"),
                    Value::symbol("n"),
                    Value::symbol("x"),
                ])],
                env: Some(env),
                docstring: None,
                doc_form: None,
                interactive: None,
            })
        };

        let left = make(env_a);
        let same = make(env_b);
        let different = make(env_c);

        assert!(!eq_value(&left, &same));
        assert!(equal_value(&left, &same, 0));
        assert!(!equal_value(&left, &different, 0));
        assert_eq!(
            left.to_hash_key(&HashTableTest::Equal),
            same.to_hash_key(&HashTableTest::Equal)
        );
    });
}

#[test]
fn recursive_closure_equal_and_hash_are_structural() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let make_recursive = || {
            let binding = Value::cons(Value::symbol("f"), Value::NIL);
            let env = Value::list(vec![binding]);
            let closure = Value::make_lambda(LambdaData {
                params: LambdaParams::simple(vec![]).into(),
                body: vec![Value::symbol("f")],
                env: Some(env),
                docstring: None,
                doc_form: None,
                interactive: None,
            });
            binding.set_cdr(closure);
            closure
        };

        let left = make_recursive();
        let right = make_recursive();

        assert!(!eq_value(&left, &right));
        assert!(equal_value(&left, &right, 0));
        assert_eq!(
            left.to_hash_key(&HashTableTest::Equal),
            right.to_hash_key(&HashTableTest::Equal)
        );
    });
}

#[test]
fn equal_deferred_cycle_tracking_still_handles_circular_lists() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let left = Value::cons(Value::symbol("x"), Value::NIL);
        left.set_cdr(left);
        let right = Value::cons(Value::symbol("x"), Value::NIL);
        right.set_cdr(right);

        assert!(equal_value(&left, &right, 0));
    });
}

#[test]
fn repeated_shallow_equal_avoids_cycle_tracking_overhead() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let make_value = || {
            Value::vector(vec![
                Value::symbol("menu-item"),
                Value::string("Option documentation"),
                Value::vector(vec![
                    Value::symbol("lambda"),
                    Value::list(vec![Value::symbol("interactive")]),
                    Value::symbol("doom/help"),
                ]),
            ])
        };
        let left = make_value();
        let right = make_value();

        let start = std::time::Instant::now();
        for _ in 0..200_000 {
            assert!(equal_value(&left, &right, 0));
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "shallow equal should not allocate cycle tracking for every call; elapsed={elapsed:?}"
        );
    });
}

#[test]
fn closure_slot_mutation_invalidates_cached_params() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let closure = Value::make_lambda(LambdaData {
            params: LambdaParams::simple(vec![intern("x")]).into(),
            body: vec![Value::symbol("x")],
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        });

        assert_eq!(
            closure
                .closure_params()
                .unwrap()
                .required
                .iter()
                .map(|sym| resolve_sym(*sym))
                .collect::<Vec<_>>(),
            vec!["x"]
        );

        let new_arglist = Value::list(vec![Value::symbol("y"), Value::symbol("z")]);
        assert!(closure.set_closure_slot(CLOSURE_ARGLIST, new_arglist));

        assert_eq!(
            closure
                .closure_params()
                .unwrap()
                .required
                .iter()
                .map(|sym| resolve_sym(*sym))
                .collect::<Vec<_>>(),
            vec!["y", "z"]
        );
    });
}

#[test]
fn hash_key_char_int_equivalence() {
    crate::test_utils::init_test_tracing();
    for test in [HashTableTest::Eq, HashTableTest::Eql, HashTableTest::Equal] {
        let char_key = Value::char('a').to_hash_key(&test);
        let int_key = Value::fixnum(97).to_hash_key(&test);
        assert_eq!(char_key, int_key);
    }
}

#[test]
fn lambda_params_arity() {
    crate::test_utils::init_test_tracing();
    let p = LambdaParams {
        required: vec![intern("a"), intern("b")],
        optional: vec![intern("c")],
        rest: None,
    };
    assert_eq!(p.min_arity(), 2);
    assert_eq!(p.max_arity(), Some(3));

    let p2 = LambdaParams {
        required: vec![intern("a")],
        optional: vec![],
        rest: Some(intern("rest")),
    };
    assert_eq!(p2.min_arity(), 1);
    assert_eq!(p2.max_arity(), None);
}

#[test]
fn cons_accessors() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let c = Value::cons(Value::fixnum(1), Value::fixnum(2));
        assert_eq!(c.cons_car(), Value::fixnum(1));
        assert_eq!(c.cons_cdr(), Value::fixnum(2));
        c.set_car(Value::fixnum(10));
        assert_eq!(c.cons_car(), Value::fixnum(10));
    });
}

#[test]
fn value_is_copy_and_16_bytes() {
    crate::test_utils::init_test_tracing();
    // Value is Copy — this assignment would fail to compile if not.
    let a = Value::fixnum(42);
    let b = a; // copy, not move
    let _ = a; // still usable after copy
    let _ = b;

    assert_eq!(
        std::mem::size_of::<Value>(),
        8,
        "Value should stay word-sized under the tagged-pointer runtime"
    );
}

#[test]
fn float_equality() {
    crate::test_utils::init_test_tracing();
    use super::equal_value;

    with_test_heap(|| {
        // 1.0 == 1.0
        assert!(equal_value(
            &Value::make_float(1.0),
            &Value::make_float(1.0),
            0
        ));
        // Emacs equal: NaN == NaN (bitwise comparison via to_bits)
        assert!(equal_value(
            &Value::make_float(f64::NAN),
            &Value::make_float(f64::NAN),
            0
        ));
        // Inf == Inf
        assert!(equal_value(
            &Value::make_float(f64::INFINITY),
            &Value::make_float(f64::INFINITY),
            0
        ));
        // Different values are not equal
        assert!(!equal_value(
            &Value::make_float(1.0),
            &Value::make_float(2.0),
            0
        ));
        // Int and Float are not equal under equal_value
        assert!(!equal_value(&Value::fixnum(1), &Value::make_float(1.0), 0));
    });
}

#[test]
fn vector_operations() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let v = Value::vector(vec![
            Value::fixnum(10),
            Value::fixnum(20),
            Value::fixnum(30),
        ]);
        assert!(v.is_vector());
        let items = v.as_vector_data().unwrap().clone();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::fixnum(10));
        assert_eq!(items[1], Value::fixnum(20));
        assert_eq!(items[2], Value::fixnum(30));
    });
}

#[test]
fn list_length_proper() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
        assert_eq!(super::list_length(&list), Some(3));
        assert_eq!(super::list_length(&Value::NIL), Some(0));
    });
}

#[test]
fn list_length_dotted() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        // (1 . 2) improper list
        let dotted = Value::cons(Value::fixnum(1), Value::fixnum(2));
        assert_eq!(super::list_length(&dotted), None);
    });
}

#[test]
fn list_length_circular() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let one = Value::cons(Value::fixnum(1), Value::NIL);
        one.set_cdr(one);
        assert_eq!(super::list_length(&one), None);

        let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
        let cycle_start = list.cons_cdr();
        let cycle_tail = cycle_start.cons_cdr();
        cycle_tail.set_cdr(cycle_start);
        assert_eq!(super::list_length(&list), None);
    });
}

#[test]
fn as_int_as_float() {
    crate::test_utils::init_test_tracing();
    assert_eq!(Value::fixnum(42).as_int(), Some(42));
    assert_eq!(Value::make_float(3.125).as_int(), None);
    assert_eq!(Value::make_float(3.125).as_float(), Some(3.125));
    assert_eq!(Value::fixnum(42).as_float(), None);
    // as_number_f64 coerces both
    assert_eq!(Value::fixnum(7).as_number_f64(), Some(7.0));
    assert_eq!(Value::make_float(2.5).as_number_f64(), Some(2.5));
    assert_eq!(Value::NIL.as_number_f64(), None);
}

#[test]
fn type_predicates() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        assert!(Value::fixnum(1).is_integer());
        assert!(Value::fixnum(1).is_number());
        assert!(!Value::fixnum(1).is_float());

        assert!(Value::make_float(1.0).is_float());
        assert!(Value::make_float(1.0).is_number());
        assert!(!Value::make_float(1.0).is_integer());

        assert!(Value::string("hi").is_string());
        assert!(!Value::string("hi").is_integer());

        let c = Value::cons(Value::fixnum(1), Value::NIL);
        assert!(c.is_cons());
        assert!(c.is_list());

        assert!(Value::NIL.is_list());
        assert!(!Value::NIL.is_cons());

        assert!(Value::vector(vec![]).is_vector());
        assert!(Value::symbol("foo").is_bare_symbol());
        assert!(Value::keyword("bar").is_bare_keyword());
        assert!(Value::char('x').is_char());
    });
}

/// `list_iter` exists so that walking a list costs nothing. The allocating
/// alternative is what made the automatic-composition scan pay a
/// `Vec::with_capacity(16)` malloc/free for every character of the whole
/// buffer that had no composition rule.
#[test]
fn list_iter_walks_a_proper_list_without_allocating() {
    let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    let seen: Vec<i64> = list_iter(list).filter_map(|v| v.as_fixnum()).collect();
    assert_eq!(seen, vec![1, 2, 3]);

    // The empty list yields nothing, and asks for no memory to say so.
    assert_eq!(list_iter(Value::NIL).count(), 0);
}

/// An improper list yields its elements and stops at the non-cons tail,
/// where `list_to_vec` instead reports `None`. Both contracts are wanted:
/// the iterator is for consuming, `list_to_vec` for validating.
#[test]
fn list_iter_stops_at_an_improper_tail_where_list_to_vec_reports_none() {
    let improper = Value::cons(Value::fixnum(1), Value::fixnum(2));
    let seen: Vec<i64> = list_iter(improper).filter_map(|v| v.as_fixnum()).collect();
    assert_eq!(
        seen,
        vec![1],
        "the car is yielded, the non-cons tail ends it"
    );
    assert_eq!(list_to_vec(&improper), None);

    // A non-list is not a one-element list.
    assert_eq!(list_iter(Value::fixnum(7)).count(), 0);
}

/// A circular list TERMINATES the iteration rather than hanging. This is the
/// guard that hand-rolled cons walks keep forgetting, which is the reason to
/// have one iterator instead of many loops.
#[test]
fn list_iter_terminates_on_a_circular_list() {
    let tail = Value::cons(Value::fixnum(2), Value::NIL);
    let head = Value::cons(Value::fixnum(1), tail);
    tail.set_cdr(head);

    let count = list_iter(head).take(64).count();
    assert!(
        count < 64,
        "a circular list must stop the iterator, got {count} elements"
    );
    assert_eq!(
        list_to_vec(&head),
        None,
        "list_to_vec still reports the cycle"
    );
}

#[path = "hash_probe_test.rs"]
mod hash_probe;
