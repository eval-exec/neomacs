use super::*;
use crate::buffer::BufferId;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{intern, intern_uninterned, resolve_sym};
use crate::heap_types::LispMarker;
use crate::tagged::header::CLOSURE_ARGLIST;
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
        assert!(Value::symbol("foo").is_symbol());
        assert!(Value::keyword(":bar").is_keyword());
    });
}

/// Foundation smoke test for bignum support: a value bigger than the
/// 62-bit fixnum range must round-trip through `Value::make_integer`,
/// classify as `integerp` / `bignump` (and *not* `fixnump`), and print
/// back to its decimal text. Mirrors GNU `make_integer_mpz`
/// (`src/bignum.c:146`).
#[test]
fn bignum_constructor_and_predicates() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        // Pick a value that can never fit in fixnum: 2^100.
        let mut huge = Integer::from(1);
        huge <<= 100;
        let big = Value::make_integer(huge.clone());
        assert!(big.is_bignum(), "expected bignum, got {:?}", big.kind());
        assert!(big.is_integer(), "bignum should satisfy integerp");
        assert!(big.is_number(), "bignum should satisfy numberp");
        assert!(!big.is_fixnum(), "bignum must not be a fixnum");
        assert_eq!(big.type_name(), "integer");

        let borrowed = big.as_bignum().expect("as_bignum");
        assert_eq!(*borrowed, huge);
        assert_eq!(
            crate::emacs_core::print::print_value(&big),
            "1267650600228229401496703205376"
        );

        // Values that fit must come back as fixnums, not bignums.
        let small = Value::make_integer(Integer::from(42));
        assert!(small.is_fixnum());
        assert!(!small.is_bignum());
        assert_eq!(small.as_fixnum(), Some(42));
    });
}

#[test]
fn eql_compares_bignums_by_numeric_value() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let left = Value::make_integer(
            Integer::from_str("1000000000000000000000001")
                .expect("valid bignum")
                .into(),
        );
        let right = Value::make_integer(
            Integer::from_str("1000000000000000000000001")
                .expect("valid bignum")
                .into(),
        );
        let different = Value::make_integer(
            Integer::from_str("1000000000000000000000002")
                .expect("valid bignum")
                .into(),
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
            Integer::from_str("1267650600228229401496703205376")
                .expect("valid bignum")
                .into(),
        );
        let right = Value::make_integer(
            Integer::from_str("1267650600228229401496703205376")
                .expect("valid bignum")
                .into(),
        );
        let different = Value::make_integer(
            Integer::from_str("1267650600228229401496703205377")
                .expect("valid bignum")
                .into(),
        );

        assert!(left.is_bignum());
        assert!(right.is_bignum());
        assert!(!eq_value(&left, &right));
        assert!(equal_value(&left, &right, 0));
        assert!(!equal_value(&left, &different, 0));
    });
}

#[test]
fn equal_compares_char_tables_by_gnu_pseudovector_slots() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"
(let* ((a (make-char-table 'neomacs-equal nil))
       (b (make-char-table 'neomacs-equal nil))
       (different (make-char-table 'neomacs-equal nil))
       (parent-a (make-char-table 'neomacs-equal-parent nil))
       (parent-b (make-char-table 'neomacs-equal-parent nil))
       (child-a (make-char-table 'neomacs-equal nil))
       (child-b (make-char-table 'neomacs-equal nil))
       (props-a (make-char-table 'neomacs-equal nil))
       (props-b (make-char-table 'neomacs-equal nil))
       (cycle-a (make-char-table 'neomacs-equal nil))
       (cycle-b (make-char-table 'neomacs-equal nil)))
  (set-char-table-range a #x1f600 'same)
  (set-char-table-range b #x1f600 'same)
  (set-char-table-range different #x1f600 'different)
  (set-char-table-range parent-a ?a 'parent)
  (set-char-table-range parent-b ?a 'parent)
  (set-char-table-parent child-a parent-a)
  (set-char-table-parent child-b parent-b)
  (set-char-table-range props-a ?x (propertize "v" 'face 'bold))
  (set-char-table-range props-b ?x (propertize "v" 'face 'italic))
  (set-char-table-range cycle-a nil cycle-a)
  (set-char-table-range cycle-b nil cycle-b)
  (list
   (equal (make-char-table 'neomacs-equal nil)
          (make-char-table 'neomacs-equal nil))
   (equal (make-char-table 'one nil)
          (make-char-table 'two nil))
   (equal a b)
   (equal a different)
   (equal child-a child-b)
   (equal props-a props-b)
   (equal-including-properties props-a props-b)
   (equal cycle-a cycle-b)))"#,
        )
        .expect("char-table equality probe should evaluate");

    assert_eq!(
        result,
        Value::list(vec![
            Value::T,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::T,
            Value::T,
            Value::NIL,
            Value::T,
        ])
    );
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
        left.with_char_table_mut(|table| table.contents[1] = left_sub)
            .expect("left char-table");
        right
            .with_char_table_mut(|table| table.contents[1] = right_sub)
            .expect("right char-table");

        assert!(equal_value(&left, &right, 0));

        right_sub
            .with_sub_char_table_mut(|table| {
                table.contents.ensure_owned()[17] = Value::T;
            })
            .expect("right sub-char-table");
        assert!(!equal_value(&left, &right, 0));
    });
}

#[test]
fn equal_char_tables_share_hashes_and_equal_hash_table_keys() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"
(let ((left (make-char-table 'neomacs-equal-hash nil))
      (right (make-char-table 'neomacs-equal-hash nil))
      (table (make-hash-table :test 'equal)))
  (set-char-table-range left #x1f600 'same)
  (set-char-table-range right #x1f600 'same)
  (puthash left 'found table)
  (list
   (equal left right)
   (= (sxhash-equal left) (sxhash-equal right))
   (gethash right table 'missing)))"#,
        )
        .expect("char-table hash contract probe should evaluate");

    assert_eq!(
        result,
        Value::list(vec![Value::T, Value::T, Value::symbol("found")])
    );
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
        assert!(keyword_from_symbol_ctor.is_keyword());
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
        assert!(!uninterned.is_keyword());
        assert!(uninterned.as_keyword_id().is_none());

        let canonical = Value::keyword(":vm-shadow-keyword");
        assert!(canonical.is_keyword());
        assert_eq!(
            canonical.as_keyword_id(),
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
fn equal_on_circular_structures_matches_gnu_cycle_semantics() {
    // GNU 31.0.90 `equal` (internal_equal_1 / internal_equal_cycle) detects cycles
    // and returns t/nil instead of signaling `circular-list`.  Regression for the
    // cons-loop tortoise-hare that used to error on circular structures; goes
    // through the lisp `equal` (try_equal_value_inner), not the bool equal_value.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    // (form, expected truthy)
    let cases: [(&str, bool); 5] = [
        // two separate self-circular lists, equal cars -> t
        (
            "(let ((a (list 'x)) (b (list 'x))) (setcdr a a) (setcdr b b) (equal a b))",
            true,
        ),
        // self-circular lists, differing cars -> nil
        (
            "(let ((a (list 'x)) (c (list 'y))) (setcdr a a) (setcdr c c) (equal a c))",
            false,
        ),
        // two separate self-circular vectors, equal -> t
        (
            "(let ((v1 (vector 'x nil)) (v2 (vector 'x nil))) (aset v1 1 v1) (aset v2 1 v2) (equal v1 v2))",
            true,
        ),
        // self-circular vectors, differing -> nil
        (
            "(let ((v1 (vector 'x nil)) (v2 (vector 'y nil))) (aset v1 1 v1) (aset v2 1 v2) (equal v1 v2))",
            false,
        ),
        // circular left vs finite right -> nil (right terminates first)
        (
            "(let ((a (list 'x))) (setcdr a a) (equal a (list 'x 'x 'x)))",
            false,
        ),
    ];
    for (src, truthy) in cases {
        let got = ev
            .eval_str(src)
            .ok()
            .unwrap_or_else(|| panic!("`equal` signaled instead of returning for: {src}"));
        let is_truthy = got.bits() != Value::NIL.bits();
        assert_eq!(is_truthy, truthy, "wrong `equal` result for: {src}");
    }
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

fn test_marker(buffer: Option<BufferId>, bytepos: usize, marker_id: u64) -> Value {
    Value::make_marker(LispMarker {
        buffer,
        insertion_type: false,
        marker_id: Some(marker_id),
        bytepos,
        charpos: bytepos,
        last_position_valid: buffer.is_some(),
        next_marker: std::ptr::null_mut(),
    })
}

#[test]
fn marker_equal_matches_gnu_buffer_and_bytepos_rules() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        let detached_left = test_marker(None, 4, 1);
        let detached_right = test_marker(None, 5, 2);
        let attached_left = test_marker(Some(BufferId(1)), 4, 1);
        let attached_same = test_marker(Some(BufferId(1)), 4, 2);
        let attached_different_pos = test_marker(Some(BufferId(1)), 5, 3);
        let attached_different_buffer = test_marker(Some(BufferId(2)), 4, 4);

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
                params: LambdaParams::simple(vec![intern("x")]),
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
                params: LambdaParams::simple(vec![]),
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
            params: LambdaParams::simple(vec![intern("x")]),
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
        assert!(Value::symbol("foo").is_symbol());
        assert!(Value::keyword("bar").is_keyword());
        assert!(Value::char('x').is_char());
    });
}

/// GNU `internal_equal_1` (src/fns.c:2984-2998 in emacs-31.1, :2987-3001 on
/// master) compares a `PVEC_CLOSURE` exactly like a vector: the size check
/// first (`ASIZE`, so a five-slot closure never equals a four-slot one), then
/// every slot element-wise -- arglist, bytecode string, constants vector,
/// max depth, doc, interactive spec, and any extras.  `sxhash_obj`
/// (:5525-5536) hashes the same slots through `sxhash_vector`, so two `equal`
/// closures land in the same `equal` hash-table bucket.
///
/// The Lisp-visible consequence lsp-mode depends on: it removes the request
/// cancel closure it put on the global `post-command-hook` by rebuilding an
/// `equal` closure and calling `remove-hook` -> `delete`.  When byte-code
/// objects compare only by identity, that hook is never removed, fires on
/// every later command, and keeps sending `$/cancelRequest` to a server that
/// has since been shut down ("Sending to process failed ... not running").
///
/// Expected values are GNU Emacs 32's for this exact probe.
#[test]
fn equal_compares_byte_code_functions_element_wise_like_gnu_closures() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"
(let* ((a (make-byte-code 257 "\300\207" [42] 2))
       (b (make-byte-code 257 "\300\207" [42] 2))
       (c (make-byte-code 257 "\300\207" [43] 2))
       (d (make-byte-code 257 "\300\207" [42] 3))
       (e (make-byte-code 257 "\300\207" [42] 2 "doc"))
       (f (make-byte-code 257 "\300\207" [42] 2 "doc"))
       (g (make-byte-code 257 "\300\207" [42] 2 nil))
       (h (make-byte-code 257 "\300\207" [42] 2 nil (list 'interactive "p")))
       (i (make-byte-code 257 "\300\207" [42] 2 nil (list 'interactive "p")))
       (j (make-byte-code 257 "\301\207" [42] 2))
       (k (make-byte-code '(x) "\300\207" [42] 2))
       (proto (make-byte-code 257 "\300\207" [placeholder] 2))
       (k1 (make-closure proto 'w))
       (k2 (make-closure proto 'w))
       (k3 (make-closure proto 'z))
       (table (make-hash-table :test 'equal))
       (v (vector 257 "\300\207" [42] 2)))
  (puthash a 'found table)
  (list (equal a b)                       ; same slots, distinct objects
        (equal a c)                       ; constants differ
        (equal a d)                       ; max depth differs
        (equal a e)                       ; doc slot differs
        (equal e f)                       ; same doc
        (equal a g)                       ; GNU ASIZE: 4 slots vs 5 slots
        (equal h i)                       ; same interactive spec
        (equal a j)                       ; bytecode differs
        (equal a k)                       ; arglist differs
        (equal k1 k2)                     ; make-closure: same captures
        (equal k1 k3)                     ; make-closure: different capture
        (eq k1 k2)
        (equal a v)                       ; a plain vector is not a closure
        (equal (list a 1) (list b 1))
        (length (delete b (list a 1)))    ; what remove-hook does
        (eq (car (member b (list 1 a))) a)
        (= (sxhash-equal a) (sxhash-equal b))
        (= (sxhash-equal k1) (sxhash-equal k2))
        (gethash b table)
        (gethash k1 (let ((tb (make-hash-table :test 'equal)))
                      (puthash k2 'closure-found tb)
                      tb))))"#,
        )
        .expect("byte-code equality probe should evaluate");

    assert_eq!(
        list_to_vec(&result).expect("probe returns a list"),
        vec![
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::fixnum(1),
            Value::T,
            Value::T,
            Value::T,
            Value::symbol("found"),
            Value::symbol("closure-found"),
        ]
    );
}

/// Build a byte-code object the way the VM and `make-closure` do, so the
/// probes below can reach states `make-byte-code` cannot: a captured
/// lexical environment (`Op::MakeClosure` stores `Some(lexenv)`, and that
/// environment is `nil` for a closure over no variables), a docstring in
/// either string representation, and a function with no retained GNU
/// bytecode string.
fn byte_code_probe(
    env: Option<Value>,
    docstring: Option<crate::heap_types::LispString>,
    gnu_bytes: Option<Vec<u8>>,
    ops: Vec<crate::emacs_core::bytecode::Op>,
    constants: Vec<Value>,
) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    Value::make_bytecode(ByteCodeFunction {
        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
        ops_sealed: true,
        stack_verified: false,
        ops,
        constants: constants.into(),
        max_stack: 1,
        params: LambdaParams::simple(vec![]),
        arglist: Value::fixnum(257),
        lexical: true,
        env,
        gnu_byte_offset_map: None,
        gnu_bytecode_bytes: gnu_bytes.map(crate::tagged::header::LispByteVec::owned),
        docstring,
        doc_form: None,
        interactive: None,
        closure_slot_count: 4,
        extra_slots: Vec::new(),
        #[cfg(feature = "jit")]
        runtime: Some(crate::emacs_core::jit::Runtime::new()),
        lazy_gnu_code: None,
    })
}

/// `(equal A B)`, whether `sxhash-equal` agrees, and whether an `equal`
/// hash table keyed by A finds B -- the three surfaces that must agree.
fn byte_code_probe_agreement(eval: &mut Context, a: Value, b: Value) -> (bool, bool, bool) {
    eval.obarray.set_symbol_value("neomacs-probe-a", a);
    eval.obarray.set_symbol_value("neomacs-probe-b", b);
    let result = eval
        .eval_str(
            r#"
(list (equal neomacs-probe-a neomacs-probe-b)
      (= (sxhash-equal neomacs-probe-a) (sxhash-equal neomacs-probe-b))
      (let ((table (make-hash-table :test 'equal)))
        (puthash neomacs-probe-a 'found table)
        (eq (gethash neomacs-probe-b table) 'found)))"#,
        )
        .expect("probe should evaluate");
    let values = list_to_vec(&result).expect("probe list");
    (
        !values[0].is_nil(),
        !values[1].is_nil(),
        !values[2].is_nil(),
    )
}

fn simple_ops() -> Vec<crate::emacs_core::bytecode::Op> {
    use crate::emacs_core::bytecode::Op;
    vec![Op::Constant(0), Op::Return]
}

/// A closure that captured an empty lexical environment (`env: Some(nil)`)
/// is observably different from a function with no environment (`aref`
/// slot 2 answers nil versus the constants vector), so `equal` says so --
/// and the `equal`-table key must say the same, or two unequal objects
/// share a bucket.
#[test]
fn equal_table_keys_keep_the_presence_of_a_captured_environment() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let consts = || vec![Value::fixnum(42)];
    let bytes = || Some(vec![0xC0, 0x87]);

    let absent = byte_code_probe(None, None, bytes(), simple_ops(), consts());
    let captured_nil = byte_code_probe(Some(Value::NIL), None, bytes(), simple_ops(), consts());
    let captured_nil_twin =
        byte_code_probe(Some(Value::NIL), None, bytes(), simple_ops(), consts());
    let captured_w = byte_code_probe(
        Some(Value::symbol("w")),
        None,
        bytes(),
        simple_ops(),
        consts(),
    );

    assert_eq!(
        byte_code_probe_agreement(&mut eval, absent, captured_nil),
        (false, false, false),
        "no environment vs a captured nil environment"
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, captured_nil, captured_nil_twin),
        (true, true, true)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, captured_nil, captured_w),
        (false, false, false),
        "the captured environment participates in equal, sxhash and the key"
    );
}

/// GNU string equality is character count, byte count and contents
/// (src/fns.c `internal_equal_1`, Lisp_String); the unibyte/multibyte
/// representation flag is not part of it.  An ASCII docstring stored
/// unibyte equals the same text stored multibyte -- verified with GNU
/// Emacs: `(equal (make-byte-code 257 "\300\207" [42] 2 "doc")
/// (make-byte-code 257 "\300\207" [42] 2 (string-to-multibyte "doc")))` is t
/// -- while a raw-byte unibyte string and the multibyte string with the
/// same bytes differ in character count and are not equal.
#[test]
fn docstrings_compare_with_gnu_string_equality_not_representation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    use crate::heap_types::LispString;
    let consts = || vec![Value::fixnum(42)];
    let bytes = || Some(vec![0xC0, 0x87]);

    let unibyte = byte_code_probe(
        None,
        Some(LispString::from_unibyte(b"doc".to_vec())),
        bytes(),
        simple_ops(),
        consts(),
    );
    let multibyte = byte_code_probe(
        None,
        Some(LispString::new("doc".to_owned(), true)),
        bytes(),
        simple_ops(),
        consts(),
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, unibyte, multibyte),
        (true, true, true),
        "same characters, same bytes: equal in GNU"
    );

    let raw_bytes = byte_code_probe(
        None,
        Some(LispString::from_unibyte(vec![0xC3, 0xA9])),
        bytes(),
        simple_ops(),
        consts(),
    );
    let e_acute = byte_code_probe(
        None,
        Some(LispString::new("\u{e9}".to_owned(), true)),
        bytes(),
        simple_ops(),
        consts(),
    );
    // GNU `sxhash_string` hashes the bytes, so these two collide in the
    // hash -- legal for unequal objects -- while `equal` and the table key
    // both tell them apart by character count.
    assert_eq!(
        byte_code_probe_agreement(&mut eval, raw_bytes, e_acute),
        (false, true, false),
        "two raw bytes are two characters; one multibyte character is one"
    );
}

/// A function with no retained GNU bytecode string (the `byte-code` subr's
/// transient functions, pdump stubs) has only its decoded instructions to
/// compare: equal instructions and constants are equal, and different
/// instructions are not, even though GNU's slot 1 would be nil for both.
#[test]
fn functions_without_gnu_bytes_compare_their_decoded_instructions() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    use crate::emacs_core::bytecode::Op;
    let consts = || vec![Value::fixnum(42)];

    let a = byte_code_probe(None, None, None, simple_ops(), consts());
    let b = byte_code_probe(None, None, None, simple_ops(), consts());
    let other_ops = byte_code_probe(
        None,
        None,
        None,
        vec![Op::Constant(0), Op::Constant(0), Op::Return],
        consts(),
    );
    let with_bytes = byte_code_probe(None, None, Some(vec![0xC0, 0x87]), simple_ops(), consts());

    assert_eq!(
        byte_code_probe_agreement(&mut eval, a, b),
        (true, true, true)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, a, other_ops),
        (false, false, false)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, a, with_bytes),
        (false, false, false),
        "retained bytes versus none is a different object"
    );
}

/// `sxhash_obj` hashes every slot `internal_equal` walks (src/fns.c:5525):
/// the constants vector and the captured environment both move the hash.
#[test]
fn sxhash_equal_folds_constants_and_the_captured_environment() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let bytes = || Some(vec![0xC0, 0x87]);
    let hash = |eval: &mut Context, value: Value| -> i64 {
        eval.obarray.set_symbol_value("neomacs-probe-a", value);
        eval.eval_str("(sxhash-equal neomacs-probe-a)")
            .expect("sxhash")
            .as_fixnum()
            .expect("fixnum hash")
    };

    let base = byte_code_probe(
        Some(Value::NIL),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(1)],
    );
    let other_constants = byte_code_probe(
        Some(Value::NIL),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(2)],
    );
    let other_env = byte_code_probe(
        Some(Value::list(vec![Value::cons(
            Value::symbol("x"),
            Value::fixnum(1),
        )])),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(1)],
    );

    let base_hash = hash(&mut eval, base);
    assert_ne!(
        base_hash,
        hash(&mut eval, other_constants),
        "constants participate"
    );
    assert_ne!(
        base_hash,
        hash(&mut eval, other_env),
        "the environment participates"
    );
}
