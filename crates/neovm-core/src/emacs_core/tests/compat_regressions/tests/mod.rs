use crate::emacs_core::error::Flow;
use crate::emacs_core::value::{HashTableTest, Value, eq_value};
use malachite::integer::Integer;
use std::str::FromStr;

#[test]
fn fillarray_vector_is_in_place() {
    crate::test_utils::init_test_tracing();
    let vec = Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]);
    let out = crate::emacs_core::builtins::builtin_fillarray(vec![vec, Value::fixnum(9)]).unwrap();
    assert_eq!(out, vec);
    if !out.is_vector() {
        panic!("expected vector");
    };
    let values = out.as_vector_data().unwrap().clone();
    assert_eq!(&*values, &[Value::fixnum(9), Value::fixnum(9)]);
}

#[test]
fn fillarray_bool_vector_preserves_layout_and_sets_bits() {
    crate::test_utils::init_test_tracing();
    let bv =
        crate::emacs_core::chartable::builtin_make_bool_vector(vec![Value::fixnum(4), Value::NIL])
            .unwrap();
    let out =
        crate::emacs_core::builtins::builtin_fillarray(vec![bv, Value::symbol("non-nil")]).unwrap();
    assert_eq!(out, bv);
    assert_eq!(
        crate::emacs_core::chartable::builtin_bool_vector_p(vec![bv]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::chartable::builtin_bool_vector_count_population(vec![bv]).unwrap(),
        Value::fixnum(4)
    );

    crate::emacs_core::builtins::builtin_fillarray(vec![bv, Value::NIL]).unwrap();
    assert_eq!(
        crate::emacs_core::chartable::builtin_bool_vector_count_population(vec![bv]).unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn fillarray_char_table_preserves_shape_and_updates_default_slot() {
    crate::test_utils::init_test_tracing();
    let table = crate::emacs_core::chartable::make_char_table_value(
        Value::symbol("syntax-table"),
        Value::fixnum(0),
    );
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![table, Value::fixnum('a' as i64), Value::fixnum(9)],
        None,
    )
    .unwrap();

    let out =
        crate::emacs_core::builtins::builtin_fillarray(vec![table, Value::fixnum(7)]).unwrap();
    assert_eq!(out, table);
    assert_eq!(
        crate::emacs_core::chartable::builtin_char_table_p(vec![table]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::chartable::builtin_char_table_subtype(vec![table]).unwrap(),
        Value::symbol("syntax-table")
    );
    assert_eq!(
        crate::emacs_core::chartable::builtin_char_table_range(
            vec![table, Value::fixnum('a' as i64)],
            None
        )
        .unwrap(),
        Value::fixnum(9)
    );
    assert_eq!(
        crate::emacs_core::chartable::builtin_char_table_range(vec![table, Value::NIL], None)
            .unwrap(),
        Value::fixnum(7)
    );
}

#[test]
fn reverse_unibyte_raw_string_preserves_bytes() {
    crate::test_utils::init_test_tracing();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A', 0x80,
    ]));
    let out = crate::emacs_core::builtins::builtin_reverse(vec![input]).unwrap();
    let ls = out.as_lisp_string().expect("string result");
    assert!(!ls.is_multibyte());
    assert_eq!(ls.as_bytes(), &[0x80, b'A', 0xFF]);
}

#[test]
fn reverse_multibyte_raw_string_preserves_emacs_chars() {
    crate::test_utils::init_test_tracing();
    let input = crate::emacs_core::misc::builtin_string_as_multibyte(vec![Value::heap_string(
        crate::heap_types::LispString::from_unibyte(vec![0xFF, b'A']),
    )])
    .unwrap();
    let out = crate::emacs_core::builtins::builtin_reverse(vec![input]).unwrap();
    let ls = out.as_lisp_string().expect("string result");
    assert!(ls.is_multibyte());
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(ls),
        vec![
            b'A' as u32,
            crate::emacs_core::emacs_char::byte8_to_char(0xFF)
        ]
    );
}

#[test]
fn reverse_bool_vector_preserves_layout_and_reverses_bits() {
    crate::test_utils::init_test_tracing();
    let bv = crate::emacs_core::chartable::builtin_bool_vector(vec![
        Value::T,
        Value::NIL,
        Value::T,
        Value::NIL,
    ])
    .unwrap();
    let out = crate::emacs_core::builtins::builtin_reverse(vec![bv]).unwrap();
    assert_eq!(
        crate::emacs_core::chartable::builtin_bool_vector_p(vec![out]).unwrap(),
        Value::T
    );
    let values = out.as_vector_data().unwrap().clone();
    assert_eq!(values[0], Value::symbol("--bool-vector--"));
    assert_eq!(values[1], Value::fixnum(4));
    assert_eq!(
        &values[2..6],
        &[
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(0),
            Value::fixnum(1)
        ]
    );
}

#[test]
fn reverse_char_table_signals_sequencep() {
    crate::test_utils::init_test_tracing();
    let table = crate::emacs_core::chartable::make_char_table_value(
        Value::symbol("syntax-table"),
        Value::fixnum(0),
    );
    let err = crate::emacs_core::builtins::builtin_reverse(vec![table]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("sequencep"));
            assert_eq!(sig.data[1], table);
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn nreverse_bool_vector_preserves_layout_and_reverses_bits_in_place() {
    crate::test_utils::init_test_tracing();
    let bv = crate::emacs_core::chartable::builtin_bool_vector(vec![
        Value::T,
        Value::NIL,
        Value::T,
        Value::NIL,
    ])
    .unwrap();
    let out = crate::emacs_core::builtins::builtin_nreverse(vec![bv]).unwrap();
    assert_eq!(out, bv);
    let values = out.as_vector_data().unwrap().clone();
    assert_eq!(values[0], Value::symbol("--bool-vector--"));
    assert_eq!(values[1], Value::fixnum(4));
    assert_eq!(
        &values[2..6],
        &[
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(0),
            Value::fixnum(1)
        ]
    );
}

#[test]
fn nreverse_char_table_signals_arrayp() {
    crate::test_utils::init_test_tracing();
    let table = crate::emacs_core::chartable::make_char_table_value(
        Value::symbol("syntax-table"),
        Value::fixnum(0),
    );
    let err = crate::emacs_core::builtins::builtin_nreverse(vec![table]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("arrayp"));
            assert_eq!(sig.data[1], table);
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn nreverse_dotted_list_mutates_before_listp_signal_like_gnu() {
    crate::test_utils::init_test_tracing();
    let tail = Value::cons(Value::fixnum(2), Value::fixnum(3));
    let list = Value::cons(Value::fixnum(1), tail);

    let err = crate::emacs_core::builtins::builtin_nreverse(vec![list]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("listp"));
            assert!(eq_value(&sig.data[1], &list));
            assert_eq!(list.cons_car(), Value::fixnum(1));
            assert!(list.cons_cdr().is_nil());
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn nreverse_circular_list_signals_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();
    let list = Value::cons(Value::fixnum(1), Value::NIL);
    let tail = Value::cons(Value::fixnum(2), list);
    list.set_cdr(tail);

    let err = crate::emacs_core::builtins::builtin_nreverse(vec![list]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "circular-list");
            assert!(eq_value(&sig.data[0], &list));
            assert_eq!(list.cons_car(), Value::fixnum(1));
            assert!(list.cons_cdr().is_nil());
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn reverse_circular_list_reports_gnu_for_each_tail_cycle_cell() {
    crate::test_utils::init_test_tracing();
    let list = Value::cons(Value::fixnum(1), Value::NIL);
    let second = Value::cons(Value::fixnum(2), Value::NIL);
    let third = Value::cons(Value::fixnum(3), list);
    list.set_cdr(second);
    second.set_cdr(third);

    let err = crate::emacs_core::builtins::builtin_reverse(vec![list]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "circular-list");
            assert!(eq_value(&sig.data[0], &third));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn length_predicate_circular_list_reports_gnu_for_each_tail_cycle_cell() {
    crate::test_utils::init_test_tracing();
    let list = Value::cons(Value::fixnum(1), Value::NIL);
    let second = Value::cons(Value::fixnum(2), Value::NIL);
    let third = Value::cons(Value::fixnum(3), list);
    list.set_cdr(second);
    second.set_cdr(third);

    let err = crate::emacs_core::builtins::builtin_length_lt(vec![list, Value::fixnum(65535)])
        .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "circular-list");
            assert!(eq_value(&sig.data[0], &third));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn nthcdr_positive_bignum_reduces_over_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();
    let list = Value::cons(Value::symbol("a"), Value::NIL);
    let second = Value::cons(Value::symbol("b"), Value::NIL);
    let third = Value::cons(Value::symbol("c"), list);
    list.set_cdr(second);
    second.set_cdr(third);

    let count = Value::make_integer(
        Integer::from_str("100000000000000000000000000000000000001")
            .expect("valid bignum")
            .into(),
    );
    let tail = crate::emacs_core::builtins::builtin_nthcdr(vec![count, list]).unwrap();

    assert!(eq_value(&tail, &third));
    assert_eq!(tail.cons_car(), Value::symbol("c"));
}

#[test]
fn rassoc_improper_tail_reports_original_alist_like_gnu() {
    crate::test_utils::init_test_tracing();
    let entry = Value::cons(Value::symbol("a"), Value::string("one"));
    let list = Value::cons(entry, Value::symbol("tail"));

    let err =
        crate::emacs_core::misc::builtin_rassoc(vec![Value::string("missing"), list]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("listp"));
            assert!(eq_value(&sig.data[1], &list));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn external_debugging_rejects_negative_fixnum() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let err = crate::emacs_core::builtins::builtin_external_debugging_output(
        &mut eval,
        vec![Value::fixnum(-1)],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

fn debugging_output_test_file() -> tempfile::NamedTempFile {
    let directory = std::env::current_dir()
        .expect("current workspace directory")
        .join("tmp/neovm-core-test-artifacts");
    std::fs::create_dir_all(&directory).expect("create workspace-local test artifact directory");
    tempfile::Builder::new()
        .prefix("redirect-debugging-output-")
        .tempfile_in(directory)
        .expect("workspace-local debugging output file")
}

#[test]
fn redirect_debugging_output_captures_external_debugging_output() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let output = debugging_output_test_file();
    let path = output.path();
    let path_value = Value::string(path.to_string_lossy().as_ref());

    crate::emacs_core::builtins::builtin_redirect_debugging_output(&mut eval, vec![path_value])
        .expect("redirect debug output to temp file");
    crate::emacs_core::builtins::builtin_external_debugging_output(
        &mut eval,
        vec![Value::fixnum('A' as i64)],
    )
    .expect("write A");
    crate::emacs_core::builtins::builtin_external_debugging_output(
        &mut eval,
        vec![Value::fixnum('B' as i64)],
    )
    .expect("write B");
    crate::emacs_core::builtins::builtin_redirect_debugging_output(&mut eval, vec![Value::NIL])
        .expect("reset debug output");

    let contents = std::fs::read_to_string(&path).expect("debug output file contents");
    assert_eq!(contents, "AB");
}

#[test]
fn prin1_external_debugging_output_preserves_nested_unibyte_bytes_with_print_circle_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.obarray
        .set_symbol_value("locale-coding-system", Value::symbol("utf-8-unix"));
    eval.obarray.set_symbol_value("print-circle", Value::T);
    let output = debugging_output_test_file();
    let path = output.path();
    let path_value = Value::string(path.to_string_lossy().as_ref());

    crate::emacs_core::builtins::builtin_redirect_debugging_output(&mut eval, vec![path_value])
        .expect("redirect debug output to workspace-local file");
    let payload = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xCE, 0xBB,
    ]));
    crate::emacs_core::builtins::builtin_prin1(
        &mut eval,
        vec![
            Value::cons(payload, Value::NIL),
            Value::symbol("external-debugging-output"),
        ],
    )
    .expect("print unibyte payload through external-debugging-output");
    crate::emacs_core::builtins::builtin_redirect_debugging_output(&mut eval, vec![Value::NIL])
        .expect("reset debug output");

    assert_eq!(
        std::fs::read(path).expect("debug output bytes"),
        [b'(', b'"', 0xCE, 0xBB, b'"', b')']
    );
}

#[test]
fn define_hash_table_test_requires_symbol_name() {
    crate::test_utils::init_test_tracing();
    let err = crate::emacs_core::builtins::builtin_define_hash_table_test(vec![
        Value::fixnum(1),
        Value::symbol("eq"),
        Value::symbol("sxhash-eq"),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn custom_hash_table_test_closures_are_gc_roots() {
    // Regression (JIT/GC audit): a `define-hash-table-test` comparison/hash
    // function given as a lambda lives ONLY in the thread-local test-alias
    // registry (and any table's user_cmp_function/user_hash_function). Neither
    // was traced by the GC, so a collection swept the closure while it was still
    // referenced and the next custom-test gethash/puthash called a freed
    // function (use-after-free). The collector must now surface both closures.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.eval_str(
        "(define-hash-table-test 'gc-rooting-regression \
           (lambda (a b) (equal a b)) \
           (lambda (k) (sxhash-equal k)))",
    )
    .unwrap();
    let mut roots: Vec<Value> = Vec::new();
    crate::emacs_core::builtins::collections::collect_hash_table_test_alias_gc_roots(&mut roots);
    assert_eq!(
        roots.len(),
        2,
        "both the custom comparison and hash closures must be GC roots, got {roots:?}"
    );
    assert!(
        roots.iter().all(|r| r.is_function()),
        "the rooted values must be the registered closures: {roots:?}"
    );
}

#[test]
fn weak_key_hash_table_drops_entry_when_key_unreachable() {
    // GNU mark_and_sweep_weak_table_contents: a :weakness 'key table must drop an
    // entry once its key is no longer reachable from anywhere else. The key here
    // (a fresh cons) lives only inside the table.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    ev.eval_str(
        "(progn \
           (setq gc-weak-ht (make-hash-table :test 'equal :weakness 'key)) \
           (puthash (list 'gc-weak-probe) t gc-weak-ht) \
           nil)",
    )
    .unwrap();
    assert_eq!(
        ev.eval_str("(hash-table-count gc-weak-ht)")
            .unwrap()
            .as_fixnum(),
        Some(1),
        "entry present before GC"
    );
    ev.eval_str("(garbage-collect)").unwrap();
    assert_eq!(
        ev.eval_str("(hash-table-count gc-weak-ht)")
            .unwrap()
            .as_fixnum(),
        Some(0),
        "weak-key entry must be dropped once its key is unreachable"
    );
}

#[test]
fn weak_key_hash_table_keeps_entry_when_key_reachable() {
    // The mirror case: while the key is reachable (held by a global), the
    // weak-key entry must survive.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    ev.eval_str(
        "(progn \
           (setq gc-live-key (list 'gc-live-probe)) \
           (setq gc-weak-ht2 (make-hash-table :test 'eq :weakness 'key)) \
           (puthash gc-live-key t gc-weak-ht2) \
           nil)",
    )
    .unwrap();
    ev.eval_str("(garbage-collect)").unwrap();
    assert_eq!(
        ev.eval_str("(hash-table-count gc-weak-ht2)")
            .unwrap()
            .as_fixnum(),
        Some(1),
        "weak-key entry survives while its key is reachable (gc-live-key)"
    );
}

#[test]
fn weak_value_hash_table_drops_entry_when_value_unreachable() {
    // :weakness 'value drops the entry once the VALUE is unreachable, regardless
    // of the (reachable, interned) key.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    ev.eval_str(
        "(progn \
           (setq gc-wv-ht (make-hash-table :test 'eq :weakness 'value)) \
           (puthash 'gc-wv-key (list 'gc-wv-value) gc-wv-ht) \
           nil)",
    )
    .unwrap();
    ev.eval_str("(garbage-collect)").unwrap();
    assert_eq!(
        ev.eval_str("(hash-table-count gc-wv-ht)")
            .unwrap()
            .as_fixnum(),
        Some(0),
        "weak-value entry must be dropped once its value is unreachable"
    );
}

#[test]
fn weak_key_hash_table_keeps_entry_for_same_sequence_eval_args() {
    // GNU's exact observable behavior comes from eval.c stack slots: evaluated
    // subr arguments remain visible to conservative GC until the surrounding
    // Fprogn returns.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    assert_eq!(
        ev.eval_str(
            "(let ((ht (make-hash-table :test 'eq :weakness 'key))) \
               (puthash (cons 1 2) :val ht) \
               (garbage-collect) \
               (hash-table-count ht))",
        )
        .unwrap()
        .as_fixnum(),
        Some(1)
    );
}

#[test]
fn weak_key_hash_table_drops_entry_after_inner_sequence_returns() {
    // The same temporary key is no longer kept once the inner Fprogn frame has
    // returned before GC runs in the outer sequence.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    assert_eq!(
        ev.eval_str(
            "(let ((ht (make-hash-table :test 'eq :weakness 'key))) \
               (progn (puthash (cons 1 2) :val ht)) \
               (garbage-collect) \
               (hash-table-count ht))",
        )
        .unwrap()
        .as_fixnum(),
        Some(0)
    );
}

#[test]
fn weak_key_hash_table_keeps_entry_for_active_let_init_temp() {
    // GNU Flet keeps its SAFE_ALLOCA temps array until SAFE_FREE_UNBIND_TO,
    // after the body and dynamic/lexical unbinding complete.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    assert_eq!(
        ev.eval_str(
            "(let ((ht (make-hash-table :test 'eq :weakness 'key))) \
               (let ((obj (cons 1 2))) \
                 (progn (puthash obj :val ht)) \
                 (setq obj nil) \
                 (garbage-collect) \
                 (hash-table-count ht)))",
        )
        .unwrap()
        .as_fixnum(),
        Some(1)
    );
}

#[test]
fn weak_key_hash_table_let_star_temp_matches_gnu_last_value_only() {
    // GNU FletX has a single `val` local, so a one-binding let* keeps OBJ, but
    // a later binding overwrites that transient root before the body.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    assert_eq!(
        ev.eval_str(
            "(let ((ht (make-hash-table :test 'eq :weakness 'key))) \
               (let* ((obj (cons 1 2))) \
                 (progn (puthash obj :val ht)) \
                 (setq obj nil) \
                 (garbage-collect) \
                 (hash-table-count ht)))",
        )
        .unwrap()
        .as_fixnum(),
        Some(1)
    );
    assert_eq!(
        ev.eval_str(
            "(let ((ht (make-hash-table :test 'eq :weakness 'key))) \
               (let* ((obj (cons 1 2)) \
                      (dummy 0)) \
                 (progn (puthash obj :val ht)) \
                 (setq obj nil) \
                 (garbage-collect) \
                 (hash-table-count ht)))",
        )
        .unwrap()
        .as_fixnum(),
        Some(0)
    );
}

#[test]
fn non_weak_hash_table_keeps_entry_after_gc() {
    // Control: a non-weak table traces its entries, so an entry with an
    // otherwise-unreachable key survives.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    ev.eval_str(
        "(progn \
           (setq gc-strong-ht (make-hash-table :test 'equal)) \
           (puthash (list 'gc-strong-probe) t gc-strong-ht) \
           nil)",
    )
    .unwrap();
    ev.eval_str("(garbage-collect)").unwrap();
    assert_eq!(
        ev.eval_str("(hash-table-count gc-strong-ht)")
            .unwrap()
            .as_fixnum(),
        Some(1),
        "non-weak table must retain its entry across GC"
    );
}

#[test]
fn face_attributes_as_vector_shape() {
    crate::test_utils::init_test_tracing();
    let out =
        crate::emacs_core::builtins::builtin_face_attributes_as_vector(vec![Value::NIL]).unwrap();
    if !out.is_vector() {
        panic!("expected vector");
    };
    let values = out.as_vector_data().unwrap().clone();
    assert_eq!(values.len(), 20);
}

#[test]
fn frame_face_hash_table_uses_eq_test() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let out = crate::emacs_core::xfaces::builtin_frame_face_hash_table(&mut eval, vec![]).unwrap();
    if !out.is_hash_table() {
        panic!("expected hash table");
    };
    assert!(matches!(
        out.as_hash_table().unwrap().test.clone(),
        HashTableTest::Eq
    ));
}

#[test]
fn frame_set_was_invisible_returns_new_state() {
    crate::test_utils::init_test_tracing();
    let out =
        crate::emacs_core::builtins::builtin_frame_set_was_invisible(vec![Value::NIL, Value::T])
            .unwrap();
    assert_eq!(out, Value::T);
}

#[test]
fn frame_bottom_divider_width_rejects_non_frame_designator() {
    crate::test_utils::init_test_tracing();
    let err =
        crate::emacs_core::builtins::builtin_frame_bottom_divider_width(vec![Value::fixnum(0)])
            .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn frame_scale_factor_defaults_to_one_float() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let out = crate::emacs_core::frame::builtin_frame_scale_factor(&mut eval, vec![]).unwrap();
    assert_eq!(out, Value::make_float(1.0));
}

#[test]
fn garbage_collect_maybe_requires_whole_number() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let err = crate::emacs_core::builtins::builtin_garbage_collect_maybe(&mut eval, vec![Value::T])
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
    // A negative FACTOR is likewise not a wholenump (GNU `CHECK_FIXNAT`).
    let neg = crate::emacs_core::builtins::builtin_garbage_collect_maybe(
        &mut eval,
        vec![Value::fixnum(-1)],
    )
    .unwrap_err();
    match neg {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn garbage_collect_maybe_collects_when_factor_scaled_threshold_exceeded() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();

    // FACTOR 0 never collects: GNU requires `FACTOR >= 1`.
    let out = crate::emacs_core::builtins::builtin_garbage_collect_maybe(
        &mut eval,
        vec![Value::fixnum(0)],
    )
    .unwrap();
    assert_eq!(out, Value::NIL, "FACTOR 0 must never collect");

    // Allocate under a huge threshold so nothing auto-collects mid-eval;
    // `bytes_since_gc` (GNU's `since_gc`) accumulates deterministically.
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    eval.eval_str_each("(setq gc-maybe-probe (make-list 256 'x))");
    let since_gc = eval.tagged_heap.bytes_since_gc();
    assert!(
        since_gc > 0,
        "allocation should have advanced bytes_since_gc"
    );

    // Below the (still-huge) threshold, the FACTOR-scaled check does not trip:
    // no collection happens and the result is nil.
    let collections_before = eval.tagged_heap.gc_collections();
    let out = crate::emacs_core::builtins::builtin_garbage_collect_maybe(
        &mut eval,
        vec![Value::fixnum(1)],
    )
    .unwrap();
    assert_eq!(out, Value::NIL, "since_gc below gc_threshold => no collect");
    assert_eq!(
        eval.tagged_heap.gc_collections(),
        collections_before,
        "no GC cycle should have run"
    );

    // Drop the threshold below what we consed. Now FACTOR 1 exceeds
    // `gc_threshold / 1`, so garbage-collect-maybe runs a real collection
    // (a new GC cycle completes) and returns t.
    eval.tagged_heap.set_gc_threshold(1);
    let out = crate::emacs_core::builtins::builtin_garbage_collect_maybe(
        &mut eval,
        vec![Value::fixnum(1)],
    )
    .unwrap();
    assert_eq!(out, Value::T, "FACTOR 1 past the threshold must collect");
    assert!(
        eval.tagged_heap.gc_collections() > collections_before,
        "garbage-collect-maybe should have driven a real collection"
    );
}

#[test]
fn gnutls_error_string_zero_is_success() {
    crate::test_utils::init_test_tracing();
    let out =
        crate::emacs_core::builtins::gnutls::builtin_gnutls_error_string(vec![Value::fixnum(0)])
            .unwrap();
    assert_eq!(out, Value::string("Success."));
}

#[test]
fn gnutls_peer_status_warning_describe_rejects_non_symbol() {
    crate::test_utils::init_test_tracing();
    let err =
        crate::emacs_core::builtins::gnutls::builtin_gnutls_peer_status_warning_describe(vec![
            Value::fixnum(0),
        ])
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn dynamic_library_alist_is_gnu_bound_nil_variable() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    let out = ev
        .eval_str(
            "(list (boundp 'dynamic-library-alist) \
                   dynamic-library-alist \
                   (get 'dynamic-library-alist 'risky-local-variable))",
        )
        .unwrap();
    assert_eq!(out, Value::list(vec![Value::T, Value::NIL, Value::T]));
}

#[test]
fn inotify_valid_p_returns_nil() {
    crate::test_utils::init_test_tracing();
    let out = crate::emacs_core::builtins::builtin_inotify_valid_p(vec![Value::fixnum(0)]).unwrap();
    assert_eq!(out, Value::NIL);
}

#[test]
fn inotify_watch_lifecycle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let watch = crate::emacs_core::builtins::builtin_inotify_add_watch(
        &mut eval,
        vec![Value::string("."), Value::NIL, Value::symbol("ignore")],
    )
    .unwrap();
    let active = crate::emacs_core::builtins::builtin_inotify_valid_p(vec![watch]).unwrap();
    assert_eq!(active, Value::T);
    let removed = crate::emacs_core::builtins::builtin_inotify_rm_watch(vec![watch]).unwrap();
    assert_eq!(removed, Value::T);
    let inactive = crate::emacs_core::builtins::builtin_inotify_valid_p(vec![watch]).unwrap();
    assert_eq!(inactive, Value::NIL);
}

#[test]
fn inotify_rm_watch_invalid_descriptor_signals() {
    crate::test_utils::init_test_tracing();
    let err =
        crate::emacs_core::builtins::builtin_inotify_rm_watch(vec![Value::fixnum(1)]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-notify-error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn gnutls_bye_requires_process() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let err =
        crate::emacs_core::process::builtin_gnutls_bye(&mut eval, vec![Value::NIL, Value::NIL])
            .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn gnutls_format_certificate_requires_string() {
    crate::test_utils::init_test_tracing();
    let err =
        crate::emacs_core::builtins::gnutls::builtin_gnutls_format_certificate(vec![Value::NIL])
            .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn gnutls_hash_digest_nil_method_signals_error() {
    crate::test_utils::init_test_tracing();
    let err = crate::emacs_core::builtins::gnutls::builtin_gnutls_hash_digest(vec![
        Value::NIL,
        Value::string("a"),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn gnutls_hash_mac_returns_raw_hmac_bytes() {
    crate::test_utils::init_test_tracing();
    let mac = crate::emacs_core::builtins::gnutls::builtin_gnutls_hash_mac(vec![
        Value::symbol("SHA256"),
        Value::string("k"),
        Value::string("a"),
    ])
    .expect("gnutls-hash-mac should evaluate");
    assert_eq!(
        mac.as_lisp_string()
            .expect("mac should be a string")
            .as_bytes(),
        &[
            0x78, 0xda, 0x91, 0x51, 0x1e, 0x67, 0x55, 0x87, 0xf5, 0xb9, 0xdf, 0x78, 0xbe, 0xde,
            0xba, 0xf5, 0x56, 0x0d, 0xa2, 0xab, 0xb8, 0x81, 0x62, 0xee, 0x87, 0x5d, 0xcd, 0xf7,
            0x44, 0x95, 0x1d, 0x9e,
        ]
    );
}

#[test]
fn gnutls_symmetric_encrypt_requires_gnutls_support() {
    crate::test_utils::init_test_tracing();
    let err = crate::emacs_core::builtins::gnutls::builtin_gnutls_symmetric_encrypt(vec![
        Value::symbol("AES-128-GCM"),
        Value::string("k"),
        Value::string("iv"),
        Value::string("data"),
        Value::string("aad"),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn handle_switch_frame_accepts_switch_frame_event_and_rejects_nil() {
    crate::test_utils::init_test_tracing();
    let frame_event = Value::list(vec![Value::symbol("switch-frame"), Value::make_frame(1)]);
    let out = crate::emacs_core::builtins::builtin_handle_switch_frame(vec![frame_event])
        .expect("switch-frame event should be accepted");
    assert_eq!(out, Value::NIL);

    let err =
        crate::emacs_core::builtins::builtin_handle_switch_frame(vec![Value::NIL]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn interactive_form_for_a_c_subr_returns_interactive_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    // `make-local-variable' is DEFUN'ed with an intspec (src/data.c); this
    // used to ask about `ignore', which is lisp/subr.el:501 and has no subr
    // (DIVERGENCES.md 152).
    let out = crate::emacs_core::builtins::symbols::builtin_interactive_form(
        &mut eval,
        vec![Value::symbol("make-local-variable")],
    )
    .unwrap();
    assert_eq!(
        out,
        Value::list(vec![
            Value::symbol("interactive"),
            Value::string("vMake Local Variable: ")
        ])
    );
}

#[test]
fn lock_file_requires_string_argument() {
    crate::test_utils::init_test_tracing();
    let err = crate::emacs_core::builtins::builtin_lock_file(vec![Value::NIL]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn unlock_file_requires_string_argument() {
    crate::test_utils::init_test_tracing();
    let err = crate::emacs_core::builtins::builtin_unlock_file(vec![Value::NIL]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn inotify_add_watch_requires_string_path_argument() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let err = crate::emacs_core::builtins::builtin_inotify_add_watch(
        &mut eval,
        vec![Value::NIL, Value::NIL, Value::NIL],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn window_bottom_divider_width_rejects_non_window_designator() {
    crate::test_utils::init_test_tracing();
    let err =
        crate::emacs_core::builtins::builtin_window_bottom_divider_width(vec![Value::fixnum(1)])
            .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("window-live-p")));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn treesit_available_p_reports_runtime_support() {
    crate::test_utils::init_test_tracing();
    let out = crate::emacs_core::builtins::builtin_treesit_available_p(vec![]).unwrap();
    assert_eq!(out, Value::T);
}

#[test]
fn treesit_query_compile_validates_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let err =
        crate::emacs_core::builtins::builtin_treesit_query_compile(&mut eval, vec![Value::NIL])
            .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn internal_stack_stats_returns_nil() {
    crate::test_utils::init_test_tracing();
    let out = crate::emacs_core::builtins::builtin_internal_stack_stats(vec![]).unwrap();
    assert_eq!(out, Value::NIL);
}

#[test]
fn internal_labeled_narrow_to_region_validates_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let err = crate::emacs_core::builtins::builtin_internal_labeled_narrow_to_region(
        &mut eval,
        vec![Value::NIL, Value::NIL],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn lossage_size_defaults_to_three_hundred() {
    crate::test_utils::init_test_tracing();
    let out = crate::emacs_core::builtins::builtin_lossage_size(vec![]).unwrap();
    assert_eq!(out, Value::fixnum(300));
}
