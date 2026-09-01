use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{LambdaData, LambdaParams};
use crate::test_utils::runtime_startup_eval_all;
use malachite::integer::Integer;

fn bootstrap_eval(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

#[test]
fn remove_family_bootstrap_matches_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'remove))
        (subrp (symbol-function 'remq))
        (subrp (symbol-function 'flatten-tree))
        (remove 2 '(1 2 3 2))
        (remq 'a '(a b a c))
        (flatten-tree '(1 (2 . 3) nil (4 5 (6)) 7))
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK (1 3)");
    assert_eq!(results[4], "OK (b c)");
    assert_eq!(results[5], "OK (1 2 3 4 5 6 7)");
}

#[test]
fn take_from_list() {
    crate::test_utils::init_test_tracing();
    let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    let result = builtin_take(vec![Value::fixnum(2), list]).unwrap();
    let items = super::super::value::list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn string_empty_blank() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (string-empty-p "")
        (string-empty-p "a")
        (string-blank-p "  ")
        (string-blank-p "x")
        "#,
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK 0");
    assert_eq!(results[3], "OK nil");
}

#[test]
fn string_replace_bootstrap_matches_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'string-replace))
        (string-replace "world" "rust" "hello world")
        (string-replace "x" "y" "no match")
        (condition-case err (string-replace "" "-" "abc") (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK "hello rust""#);
    assert_eq!(results[2], r#"OK "no match""#);
    // wrong-length-argument is a subtype of error, so condition-case
    // catches it and (car err) returns the error symbol.
    assert_eq!(results[3], "OK wrong-length-argument");
}

#[test]
fn string_search() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_string_search(vec![Value::string("world"), Value::string("hello world")]).unwrap();
    assert_eq!(result.as_int(), Some(6));

    let result = builtin_string_search(vec![Value::string("xyz"), Value::string("hello")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn string_search_explicit_nil_start_defaults_to_zero_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_string_search(vec![Value::string(" "), Value::string("a b c"), Value::NIL])
            .unwrap();

    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn string_search_gnu_start_pos_errors() {
    crate::test_utils::init_test_tracing();
    let err = builtin_string_search(vec![
        Value::string("a"),
        Value::string("abc"),
        Value::fixnum(-1),
    ])
    .unwrap_err();
    match err {
        crate::emacs_core::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol, intern("args-out-of-range"));
            assert_eq!(sig.data, vec![Value::fixnum(-1)]);
        }
        other => panic!("expected signal, got {other:?}"),
    }

    let err = builtin_string_search(vec![
        Value::string("a"),
        Value::string("abc"),
        Value::bignum(Integer::from(1u64) << 100u32),
    ])
    .unwrap_err();
    match err {
        crate::emacs_core::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol, intern("wrong-type-argument"));
            assert_eq!(sig.data[0], Value::symbol("fixnump"));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn string_search_raw_byte_multibyte_conversion() {
    crate::test_utils::init_test_tracing();
    let unibyte_e9 = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xE9]));
    let raw_byte_e9 = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        crate::emacs_core::emacs_char::str_to_multibyte(&[0xE9]),
    ));
    let eacute = Value::string("é");

    assert_eq!(
        builtin_string_search(vec![raw_byte_e9.clone(), unibyte_e9.clone()]).unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_string_search(vec![unibyte_e9.clone(), raw_byte_e9]).unwrap(),
        Value::fixnum(0)
    );
    assert!(
        builtin_string_search(vec![eacute, unibyte_e9])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn proper_list_p() {
    crate::test_utils::init_test_tracing();
    let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    // proper-list-p returns the length of the list (2), not t
    assert_eq!(builtin_proper_list_p(vec![list]).unwrap(), Value::fixnum(2),);
    assert!(
        builtin_proper_list_p(vec![Value::fixnum(5)])
            .unwrap()
            .is_nil(),
    );
}

#[test]
fn closurep_true_for_lambda_values() {
    crate::test_utils::init_test_tracing();
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x")]),
        body: vec![].into(),
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    assert!(builtin_closurep(vec![lambda]).unwrap().is_truthy());
    assert!(builtin_closurep(vec![Value::fixnum(1)]).unwrap().is_nil());
}

#[test]
fn bare_symbol_and_predicate_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_bare_symbol(vec![Value::symbol("alpha")]).unwrap(),
        Value::symbol("alpha")
    );
    assert_eq!(
        builtin_bare_symbol(vec![Value::keyword(":k")]).unwrap(),
        Value::keyword(":k")
    );
    assert_eq!(builtin_bare_symbol(vec![Value::NIL]).unwrap(), Value::NIL);

    assert!(
        builtin_bare_symbol_p(vec![Value::symbol("alpha")])
            .unwrap()
            .is_truthy()
    );
    assert!(
        builtin_bare_symbol_p(vec![Value::keyword(":k")])
            .unwrap()
            .is_truthy()
    );
    assert!(builtin_bare_symbol_p(vec![Value::NIL]).unwrap().is_truthy());
    assert!(
        builtin_bare_symbol_p(vec![Value::fixnum(1)])
            .unwrap()
            .is_nil()
    );

    let err = builtin_bare_symbol(vec![Value::fixnum(1)]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[1], Value::fixnum(1));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn byteorder_shape_and_arity() {
    crate::test_utils::init_test_tracing();
    let byteorder = builtin_byteorder(vec![]).unwrap();
    assert!(byteorder.is_fixnum() || byteorder.is_fixnum());

    let err = builtin_byteorder(vec![Value::NIL]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn assoc_string_unifies_unibyte_and_multibyte_eight_bit() {
    crate::test_utils::init_test_tracing();
    // GNU `compare-strings` (used by assoc-string) treats a unibyte raw byte and
    // the corresponding multibyte eight-bit character as the same character, so
    // the lookup matches across the two representations.
    let unibyte_key = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let mut buf = [0u8; 8];
    let len = crate::emacs_core::emacs_char::char_string(
        crate::emacs_core::emacs_char::byte8_to_char(0xFF),
        &mut buf,
    );
    let multibyte_entry = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        buf[..len].to_vec(),
    ));
    let alist = Value::list(vec![Value::cons(multibyte_entry, Value::fixnum(7))]);
    let hit = builtin_assoc_string(vec![unibyte_key, alist]).unwrap();
    assert!(
        hit.is_cons(),
        "unibyte 0xFF should match the multibyte eight-bit entry"
    );
    assert_eq!(hit.cons_cdr(), Value::fixnum(7));

    // Case-folding still matches ASCII case differences.
    let fold_hit = builtin_assoc_string(vec![
        Value::string("ABC"),
        Value::list(vec![Value::cons(Value::string("abc"), Value::fixnum(1))]),
        Value::T,
    ])
    .unwrap();
    assert!(fold_hit.is_cons());
}

#[test]
fn assoc_string_and_car_less_than_car_semantics() {
    crate::test_utils::init_test_tracing();
    let result = builtin_assoc_string(vec![
        Value::string("A"),
        Value::list(vec![
            Value::cons(Value::string("a"), Value::fixnum(1)),
            Value::cons(Value::string("b"), Value::fixnum(2)),
        ]),
        Value::T,
    ])
    .unwrap();
    if !result.is_cons() {
        panic!("expected dotted pair result");
    };
    let result_pair_car = result.cons_car();
    let result_pair_cdr = result.cons_cdr();
    assert_eq!(result_pair_car, Value::string("a"));
    assert_eq!(result_pair_cdr, Value::fixnum(1));

    let symbol_alist = Value::list(vec![
        Value::cons(Value::symbol("foo"), Value::fixnum(1)),
        Value::cons(Value::keyword(":k"), Value::fixnum(2)),
    ]);
    let symbol_hit = builtin_assoc_string(vec![Value::string("foo"), symbol_alist]).unwrap();
    if !symbol_hit.is_cons() {
        panic!("expected dotted pair result");
    };
    let symbol_pair_car = symbol_hit.cons_car();
    let symbol_pair_cdr = symbol_hit.cons_cdr();
    assert_eq!(symbol_pair_car, Value::symbol("foo"));
    assert_eq!(symbol_pair_cdr, Value::fixnum(1));

    let nil_tail = Value::cons(
        Value::cons(Value::string("x"), Value::fixnum(1)),
        Value::fixnum(2),
    );
    assert!(
        builtin_assoc_string(vec![Value::string("x"), nil_tail])
            .unwrap()
            .is_truthy()
    );
    assert!(
        builtin_assoc_string(vec![Value::string("y"), Value::fixnum(1)])
            .unwrap()
            .is_nil()
    );

    assert!(
        builtin_assoc_string(vec![Value::fixnum(1), Value::NIL])
            .unwrap()
            .is_nil()
    );

    assert!(
        builtin_car_less_than_car(vec![
            Value::cons(Value::fixnum(1), Value::symbol("a")),
            Value::cons(Value::fixnum(2), Value::symbol("b")),
        ])
        .unwrap()
        .is_truthy()
    );
    assert!(
        builtin_car_less_than_car(vec![
            Value::cons(Value::make_float(3.0), Value::symbol("a")),
            Value::cons(Value::fixnum(2), Value::symbol("b")),
        ])
        .unwrap()
        .is_nil()
    );
    let left_marker = crate::emacs_core::marker::make_marker_value(
        Some(crate::buffer::BufferId(1)),
        Some(LispCharPos1::new(3)),
        false,
    );
    let right_marker = crate::emacs_core::marker::make_marker_value(
        Some(crate::buffer::BufferId(1)),
        Some(LispCharPos1::new(8)),
        false,
    );
    assert!(
        builtin_car_less_than_car(vec![
            Value::cons(left_marker, Value::symbol("a")),
            Value::cons(right_marker, Value::symbol("b")),
        ])
        .unwrap()
        .is_truthy()
    );

    let list_err = builtin_car_less_than_car(vec![
        Value::fixnum(1),
        Value::cons(Value::fixnum(2), Value::NIL),
    ])
    .unwrap_err();
    match list_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let number_err = builtin_car_less_than_car(vec![
        Value::cons(Value::symbol("x"), Value::NIL),
        Value::cons(Value::fixnum(1), Value::NIL),
    ])
    .unwrap_err();
    match number_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn assoc_string_matches_gnu_atom_entries_and_deferred_key_errors() {
    crate::test_utils::init_test_tracing();

    let atom_string_hit = builtin_assoc_string(vec![
        Value::string("solo"),
        Value::list(vec![
            Value::string("solo"),
            Value::cons(Value::string("solo"), Value::symbol("pair-hit")),
        ]),
    ])
    .unwrap();
    assert_eq!(atom_string_hit, Value::string("solo"));

    let atom_symbol_hit = builtin_assoc_string(vec![
        Value::string("symbol-key"),
        Value::list(vec![
            Value::symbol("symbol-key"),
            Value::cons(Value::string("symbol-key"), Value::symbol("string-pair")),
        ]),
    ])
    .unwrap();
    assert_eq!(atom_symbol_hit, Value::symbol("symbol-key"));

    let no_list_no_key_check = builtin_assoc_string(vec![Value::fixnum(42), Value::NIL]).unwrap();
    assert!(no_list_no_key_check.is_nil());

    let skipped_non_string_entries_then_hit = builtin_assoc_string(vec![
        Value::string("hit"),
        Value::list(vec![
            Value::cons(Value::fixnum(1), Value::symbol("skip-number")),
            Value::cons(Value::NIL, Value::symbol("skip-nil")),
            Value::cons(
                Value::list(vec![Value::symbol("hit")]),
                Value::symbol("skip-list"),
            ),
            Value::cons(Value::string("hit"), Value::symbol("string-hit")),
        ]),
    ])
    .unwrap();
    assert_eq!(
        skipped_non_string_entries_then_hit.cons_cdr(),
        Value::symbol("string-hit")
    );

    let deferred_key_err = builtin_assoc_string(vec![
        Value::fixnum(42),
        Value::list(vec![Value::cons(
            Value::string("42"),
            Value::symbol("would-error"),
        )]),
    ])
    .unwrap_err();
    match deferred_key_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn number_predicates() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_zerop(vec![Value::fixnum(0)]).unwrap().is_t());
    assert!(builtin_zerop(vec![Value::fixnum(1)]).unwrap().is_nil());
    assert!(builtin_natnump(vec![Value::fixnum(5)]).unwrap().is_t());
    assert!(builtin_natnump(vec![Value::fixnum(-1)]).unwrap().is_nil());
    assert!(
        builtin_natnump(vec![Value::make_integer((i128::from(i64::MAX) + 1).into())])
            .unwrap()
            .is_t()
    );
    assert!(
        builtin_natnump(vec![Value::make_integer((i128::from(i64::MIN) - 1).into())])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn fixnum_predicates_bootstrap_match_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'fixnump))
        (subrp (symbol-function 'bignump))
        (list (fixnump 0)
              (fixnump most-positive-fixnum)
              (fixnump 1.0)
              (fixnump nil))
        (list (bignump 0)
              (bignump most-positive-fixnum)
              (bignump 1.0)
              (bignump nil))
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK (t t nil nil)");
    assert_eq!(results[3], "OK (nil nil nil nil)");
}

#[test]
fn seq_uniq() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (seq-uniq '(1 2 1 3))
        (seq-uniq '("Hello" "hello" "HELLO") #'string-equal-ignore-case)
        "#,
    );
    assert_eq!(results[0], "OK (1 2 3)");
    assert_eq!(results[1], "OK (\"Hello\")");
}

#[test]
fn seq_length_list_and_string() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (seq-length '(1 2 3))
        (seq-length "hello")
        (seq-into '(1 2 3) 'vector)
        (seq-into [?h ?i] 'string)
        "#,
    );
    assert_eq!(results[0], "OK 3");
    assert_eq!(results[1], "OK 5");
    assert_eq!(results[2], "OK [1 2 3]");
    assert_eq!(results[3], "OK \"hi\"");
}

#[test]
fn seq_length_wrong_type_errors() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err
            (seq-length 42)
          (wrong-type-argument (car err)))
        (condition-case err
            (seq-into '(1 2 3) 'hash-table)
          (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK wrong-type-argument");
    assert_eq!(results[1], "OK error");
}

#[test]
fn user_info() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    // These should not panic, just return strings.
    assert!(
        builtin_user_login_name(&mut ctx, vec![])
            .unwrap()
            .is_string()
    );
    assert!(
        builtin_user_real_login_name(&mut ctx, vec![])
            .unwrap()
            .is_string()
    );
    assert!(
        builtin_user_full_name(&mut ctx, vec![])
            .unwrap()
            .is_string()
    );
    let mut eval = super::super::eval::Context::new();
    assert!(builtin_system_name(&mut eval, vec![]).unwrap().is_string());
    let system_configuration = system_configuration_value();
    assert!(system_configuration.is_string());
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert_eq!(
        system_configuration.as_utf8_str(),
        Some("x86_64-pc-linux-gnu")
    );
    assert!(system_configuration_options_value().is_string());
    assert!(system_configuration_features_value().is_string());
    assert!(
        operating_system_release_value().is_nil() || operating_system_release_value().is_string()
    );
    assert!(builtin_emacs_version(vec![]).unwrap().is_string());
}

#[test]
fn user_full_name_uses_gecos_prefix_verbatim() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        canonical_full_name(&PasswdEntry {
            login: "login".to_string(),
            gecos: "Full Name,Room,Phone".to_string(),
        }),
        "Full Name"
    );
    assert_eq!(
        canonical_full_name(&PasswdEntry {
            login: "login".to_string(),
            gecos: "".to_string(),
        }),
        ""
    );
    assert_eq!(
        canonical_full_name(&PasswdEntry {
            login: "login".to_string(),
            gecos: "  Full Name  ".to_string(),
        }),
        "  Full Name  "
    );
}

#[test]
fn user_full_name_no_arg_reads_current_special_variable() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (setq user-full-name "Alice")
        (user-full-name)
        (let ((user-full-name "Bob"))
          (user-full-name))
        "#,
    );
    assert_eq!(results[0], "OK \"Alice\"");
    assert_eq!(results[1], "OK \"Alice\"");
    assert_eq!(results[2], "OK \"Bob\"");
}

#[test]
fn user_identity_optional_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let login_for_uid =
        builtin_user_login_name(&mut ctx, vec![Value::fixnum(effective_uid())]).unwrap();
    assert!(login_for_uid.is_nil() || login_for_uid.is_string());

    let by_uid = builtin_user_full_name(&mut ctx, vec![Value::fixnum(effective_uid())]).unwrap();
    assert!(by_uid.is_nil() || by_uid.is_string());

    let login = builtin_user_login_name(&mut ctx, vec![]).unwrap();
    let by_login = builtin_user_full_name(&mut ctx, vec![login]).unwrap();
    assert!(by_login.is_nil() || by_login.is_string());
}

#[test]
fn user_identity_arity_contracts() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let login_name_err =
        builtin_user_login_name(&mut ctx, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap_err();
    match login_name_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }

    let real_login_err =
        builtin_user_real_login_name(&mut ctx, vec![Value::fixnum(1)]).unwrap_err();
    match real_login_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }

    let full_name_err =
        builtin_user_full_name(&mut ctx, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap_err();
    match full_name_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn user_identity_type_contracts() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let login_name_err =
        builtin_user_login_name(&mut ctx, vec![Value::string("root")]).unwrap_err();
    match login_name_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }

    let full_name_err =
        builtin_user_full_name(&mut ctx, vec![Value::list(vec![Value::fixnum(1)])]).unwrap_err();
    match full_name_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }

    let negative_uid_login =
        builtin_user_login_name(&mut ctx, vec![Value::fixnum(-1)]).unwrap_err();
    match negative_uid_login {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }

    let negative_uid_full_name =
        builtin_user_full_name(&mut ctx, vec![Value::fixnum(-1)]).unwrap_err();
    match negative_uid_full_name {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn emacs_pid() {
    crate::test_utils::init_test_tracing();
    let pid = builtin_emacs_pid(vec![]).unwrap();
    assert!(pid.as_fixnum().map_or(false, |n| n > 0));
}

#[test]
fn runtime_identity_arity_contracts() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let system_name_err = builtin_system_name(&mut eval, vec![Value::NIL]).unwrap_err();
    match system_name_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }

    let version_with_nil = builtin_emacs_version(vec![Value::NIL]).unwrap();
    assert!(version_with_nil.is_string());

    let version_with_non_nil = builtin_emacs_version(vec![Value::T]).unwrap();
    assert!(version_with_non_nil.is_nil());

    let version_err = builtin_emacs_version(vec![Value::NIL, Value::NIL]).unwrap_err();
    match version_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }

    let pid_err = builtin_emacs_pid(vec![Value::NIL]).unwrap_err();
    match pid_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn garbage_collect_shape_and_arity() {
    crate::test_utils::init_test_tracing();
    let gc = builtin_garbage_collect_stats().unwrap();
    let buckets = super::super::value::list_to_vec(&gc).expect("gc list");
    assert_eq!(buckets.len(), 9);
    let names = buckets
        .iter()
        .map(|bucket| {
            let bucket_items = super::super::value::list_to_vec(bucket).expect("bucket list");
            match bucket_items.first() {
                Some(v) if v.as_symbol_id().is_some() => {
                    crate::emacs_core::intern::resolve_sym(v.as_symbol_id().unwrap()).to_owned()
                }
                other => panic!("expected bucket symbol, got {other:?}"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "conses".to_string(),
            "symbols".to_string(),
            "strings".to_string(),
            "string-bytes".to_string(),
            "vectors".to_string(),
            "vector-slots".to_string(),
            "floats".to_string(),
            "intervals".to_string(),
            "buffers".to_string(),
        ]
    );
    for bucket in &buckets {
        let bucket_items = super::super::value::list_to_vec(bucket).expect("bucket list");
        assert!(bucket_items.len() >= 2);
        assert!(bucket_items[0].is_symbol());
        assert!(bucket_items[1..].iter().all(|item| item.is_fixnum()));
    }
}

#[test]
fn memory_use_counts_shape_and_arity() {
    crate::test_utils::init_test_tracing();
    let counts = builtin_memory_use_counts(vec![]).unwrap();
    let items = super::super::value::list_to_vec(&counts).expect("counts list");
    assert_eq!(items.len(), 7);
    assert!(items.iter().all(|item| item.is_fixnum()));

    let err = builtin_memory_use_counts(vec![Value::fixnum(1)]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }
}
