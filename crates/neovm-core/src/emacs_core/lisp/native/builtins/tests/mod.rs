use super::*;
use crate::buffer::{CharRange, EmacsByteRange, LispCharPos1};
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::emacs_core::buffer::builtin_make_overlay;
use crate::emacs_core::editfns::{
    builtin_delete_and_extract_region, builtin_delete_region, builtin_erase_buffer,
};
use crate::emacs_core::intern::intern_uninterned;
use crate::emacs_core::treesit as runtime_treesit;
use crate::emacs_core::value::{
    HashTableTest, LambdaData, LambdaParams, StringTextPropertyRun, Value, ValueKind,
    get_string_text_properties_for_value,
};
use crate::emacs_core::{Context, format_eval_result};
use crate::test_utils::{load_minimal_gnu_backquote_runtime, runtime_startup_context};
use malachite::integer::Integer;
use std::fs;
use tree_sitter::Language;

/// GNU stores aliases in ordinary function cells.  Interning the same printed
/// name in a fresh obarray does not synthesize a definition from that name:
/// `(fboundp (intern "string<" (obarray-make)))` is nil.  Besides being
/// observable, keeping an empty cell empty prevents M-x's `commandp` predicate
/// from resolving the printed name of every unbound symbol in the obarray.
#[test]
fn empty_function_cell_does_not_synthesize_builtin_alias() {
    crate::test_utils::init_test_tracing();
    let mut obarray = crate::emacs_core::symbol::Obarray::new();
    obarray.intern("string<");
    let symbol = crate::emacs_core::intern::intern("string<");

    assert_eq!(
        super::symbols::symbol_function_cell_in_obarray(&obarray, symbol),
        None
    );
}

fn put_string_property(
    table: &mut crate::buffer::text_props::TextPropertyTable,
    start: usize,
    end: usize,
    name: Value,
    value: Value,
) -> bool {
    table.put_property_in_char_range(CharRange::from_usize(start, end), name, value)
}

/// Decode char codes from a Value's LispString using emacs_char.
fn decode_value_char_codes(v: &Value) -> Vec<u32> {
    let ls = v.as_lisp_string().expect("expected string value");
    super::lisp_string_char_codes(ls)
}

/// Backward-compat: decode char codes from an &str (for tests using as_str).
fn dispatch_builtin_pure(name: &str, args: Vec<Value>) -> Option<EvalResult> {
    super::dispatch_builtin_without_eval_state(name, args)
}

fn install_variable_watcher_probe(eval: &mut crate::emacs_core::eval::Context, callback: &str) {
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams {
            required: vec![
                intern("symbol"),
                intern("newval"),
                intern("operation"),
                intern("where"),
            ],
            optional: Vec::new(),
            rest: None,
        },
        body: vec![
            Value::list(vec![
                Value::symbol("setq"),
                Value::symbol("vm-watcher-last-op"),
                Value::symbol("operation"),
            ]),
            Value::list(vec![
                Value::symbol("setq"),
                Value::symbol("vm-watcher-last-symbol"),
                Value::symbol("symbol"),
            ]),
            Value::list(vec![
                Value::symbol("setq"),
                Value::symbol("vm-watcher-last-value"),
                Value::symbol("newval"),
            ]),
            Value::symbol("newval"),
        ],
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    eval.obarray_mut().set_symbol_function(callback, lambda);
}

fn install_noarg_hook_probe(
    eval: &mut crate::emacs_core::eval::Context,
    callback: &str,
    body: Vec<Value>,
) {
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![]),
        body,
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    eval.obarray_mut().set_symbol_function(callback, lambda);
}

fn create_unique_test_buffer(eval: &mut crate::emacs_core::eval::Context, name: &str) -> Value {
    let unique_name = eval.buffers.generate_new_buffer_name(name);
    Value::make_buffer(eval.buffers.create_buffer(&unique_name))
}

fn install_test_treesit_json_language(eval: &mut Context) {
    eval.treesit.cache_loaded_language(
        crate::emacs_core::intern::intern("json"),
        runtime_treesit::LoadedLanguage {
            language: Language::new(tree_sitter_json::LANGUAGE),
            filename: Some("test:json".to_string()),
            _library: None,
        },
    );
}

fn eval_first_gnu_form_after_marker(eval: &mut Context, source: &str, marker: &str) {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing GNU Lisp marker: {marker}"));
    let (form, _) = crate::emacs_core::value_reader::read_one(&source[start..], 0, &test_ob())
        .unwrap_or_else(|err| panic!("parse GNU Lisp from {marker} failed: {err:?}"))
        .unwrap_or_else(|| panic!("no GNU Lisp form found after marker: {marker}"));
    eval.eval_form(form)
        .unwrap_or_else(|err| panic!("evaluate GNU Lisp form {marker} failed: {err:?}"));
}

fn load_gnu_save_selected_window_runtime(eval: &mut Context) {
    eval.eval_str(
        r#"
        (defalias 'frames-on-display-list
          #'(lambda (&optional _device)
              (frame-list)))
        "#,
    )
    .expect("eval forms");

    let window_path = concat!(env!("CARGO_WORKSPACE_DIR"), "/lisp/window.el");
    let window_source = fs::read_to_string(window_path).expect("read GNU window.el");
    for marker in [
        "(defun internal--before-save-selected-window ()",
        "(defun internal--after-save-selected-window (state)",
        "(defmacro save-selected-window (&rest body)",
    ] {
        eval_first_gnu_form_after_marker(eval, &window_source, marker);
    }
}

#[test]
fn pure_dispatch_typed_add_still_works() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure("+", vec![Value::fixnum(2), Value::fixnum(3)])
        .expect("builtin + should resolve")
        .expect("builtin + should evaluate");
    assert_eq!(result, Value::fixnum(5));
}

#[test]
fn pure_dispatch_typed_percent_and_mod_follow_emacs_sign_rules() {
    crate::test_utils::init_test_tracing();
    let percent = dispatch_builtin_pure("%", vec![Value::fixnum(-5), Value::fixnum(2)])
        .expect("builtin % should resolve")
        .expect("builtin % should evaluate");
    let mod_name = dispatch_builtin_pure("mod", vec![Value::fixnum(-5), Value::fixnum(2)])
        .expect("builtin mod should resolve")
        .expect("builtin mod should evaluate");
    assert_eq!(percent, Value::fixnum(-1));
    assert_eq!(mod_name, Value::fixnum(1));
}

#[test]
fn pure_dispatch_typed_mod_zero_remainder_with_negative_divisor_stays_zero() {
    crate::test_utils::init_test_tracing();
    let int_mod = dispatch_builtin_pure("mod", vec![Value::fixnum(0), Value::fixnum(-3)])
        .expect("builtin mod should resolve")
        .expect("builtin mod should evaluate");
    assert_eq!(int_mod, Value::fixnum(0));

    let float_mod =
        dispatch_builtin_pure("mod", vec![Value::make_float(0.5), Value::make_float(-0.5)])
            .expect("builtin mod should resolve")
            .expect("builtin mod should evaluate");
    match float_mod.kind() {
        ValueKind::Float => {
            let f = float_mod.as_float().unwrap();
            assert_eq!(f, 0.0);
            assert!(!f.is_sign_negative(), "expected +0.0");
        }
        other => panic!("expected float, got {other:?}"),
    }

    let neg_zero_mod = dispatch_builtin_pure(
        "mod",
        vec![Value::make_float(-0.5), Value::make_float(-0.5)],
    )
    .expect("builtin mod should resolve")
    .expect("builtin mod should evaluate");
    match neg_zero_mod.kind() {
        ValueKind::Float => {
            let f = neg_zero_mod.as_float().unwrap();
            assert_eq!(f, 0.0);
            assert!(f.is_sign_negative(), "expected -0.0");
        }
        other => panic!("expected float, got {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_max_min_preserve_selected_operand_type() {
    crate::test_utils::init_test_tracing();
    let max_int = dispatch_builtin_pure("max", vec![Value::make_float(-2.5), Value::fixnum(1)])
        .expect("builtin max should resolve")
        .expect("builtin max should evaluate");
    assert_eq!(max_int, Value::fixnum(1));

    let min_int = dispatch_builtin_pure("min", vec![Value::fixnum(1), Value::make_float(1.0)])
        .expect("builtin min should resolve")
        .expect("builtin min should evaluate");
    assert_eq!(min_int, Value::fixnum(1));

    let max_float = dispatch_builtin_pure("max", vec![Value::make_float(1.0), Value::fixnum(1)])
        .expect("builtin max should resolve")
        .expect("builtin max should evaluate");
    assert_eq!(max_float, Value::make_float(1.0));
}

#[test]
fn pure_dispatch_typed_numeric_primitives_accept_markers() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abcdef");
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(4),
        false,
    );

    let max_with_marker = dispatch_builtin_pure("max", vec![Value::fixnum(1), marker])
        .expect("builtin max should resolve")
        .expect("builtin max should evaluate");
    assert_eq!(max_with_marker, Value::fixnum(4));

    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(4),
        false,
    );
    let min_with_marker = dispatch_builtin_pure("min", vec![Value::fixnum(10), marker])
        .expect("builtin min should resolve")
        .expect("builtin min should evaluate");
    assert_eq!(min_with_marker, Value::fixnum(4));

    let left_marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(2),
        false,
    );
    let right_marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(5),
        false,
    );
    let lt_with_markers = dispatch_builtin_pure("<", vec![left_marker, right_marker])
        .expect("builtin < should resolve")
        .expect("builtin < should evaluate");
    assert_eq!(lt_with_markers, Value::T);

    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(4),
        false,
    );
    let add1_with_marker = dispatch_builtin_pure("1+", vec![marker])
        .expect("builtin 1+ should resolve")
        .expect("builtin 1+ should evaluate");
    assert_eq!(add1_with_marker, Value::fixnum(5));

    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(4),
        false,
    );
    let sub1_with_marker = dispatch_builtin_pure("1-", vec![marker])
        .expect("builtin 1- should resolve")
        .expect("builtin 1- should evaluate");
    assert_eq!(sub1_with_marker, Value::fixnum(3));
}

#[test]
fn eval_dispatch_typed_max_uses_live_marker_position_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str_each(
            r#"(insert "abc")
           (let ((m (copy-marker (point-max) t)))
             (goto-char 2)
             (insert "XYZ")
             (max 1 m))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");
    assert_eq!(result, Value::fixnum(7));
}

#[test]
fn eval_dispatch_typed_min_uses_live_marker_position_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str_each(
            r#"(insert "abc")
           (let ((m (copy-marker (point-max) t)))
             (goto-char 2)
             (insert "XYZ")
             (min 10 m))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");
    assert_eq!(result, Value::fixnum(7));
}

#[test]
fn pure_dispatch_typed_percent_rejects_float_args() {
    crate::test_utils::init_test_tracing();
    let err = dispatch_builtin_pure("%", vec![Value::make_float(1.5), Value::fixnum(2)])
        .expect("builtin % should resolve")
        .expect_err("builtin % should reject non-integer args");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::make_float(1.5)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_log_bitops_reject_with_integer_or_marker_p() {
    crate::test_utils::init_test_tracing();
    for name in ["logand", "logior", "logxor"] {
        let err = dispatch_builtin_pure(name, vec![Value::fixnum(1), Value::make_float(2.0)])
            .expect("builtin should resolve")
            .expect_err("bit operation should reject non-integer args");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("integer-or-marker-p"), Value::make_float(2.0)]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }
}

#[test]
fn pure_dispatch_typed_numeric_symbol_rejections_use_number_or_marker_p() {
    crate::test_utils::init_test_tracing();
    let symbol_arg = Value::symbol("a");
    let cases = [
        ("+", vec![Value::fixnum(1), symbol_arg]),
        ("mod", vec![Value::fixnum(1), symbol_arg]),
        ("logand", vec![Value::fixnum(1), symbol_arg]),
        ("=", vec![Value::fixnum(1), symbol_arg]),
    ];

    for (name, args) in cases {
        let err = dispatch_builtin_pure(name, args)
            .expect("builtin should resolve")
            .expect_err("numeric builtin should reject non-numeric symbols");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument", "name={name}");
                let actual_name = sig.data[0].as_symbol_name().map(String::from);
                assert_eq!(
                    actual_name.as_deref(),
                    Some("number-or-marker-p"),
                    "name={name}, full data={:?}",
                    sig.data
                );
                assert_eq!(sig.data[1], symbol_arg, "name={name}");
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }
}

#[test]
fn pure_dispatch_typed_div_float_zero_uses_ieee_results() {
    crate::test_utils::init_test_tracing();
    let pos_inf = dispatch_builtin_pure("/", vec![Value::make_float(1.0), Value::make_float(0.0)])
        .expect("builtin / should resolve")
        .expect("float division should evaluate");
    match pos_inf.kind() {
        ValueKind::Float => {
            let f = pos_inf.as_float().unwrap();
            assert!(f.is_infinite() && f.is_sign_positive());
        }
        other => panic!("expected float, got {other:?}"),
    }

    let neg_inf = dispatch_builtin_pure("/", vec![Value::make_float(-1.0), Value::make_float(0.0)])
        .expect("builtin / should resolve")
        .expect("float division should evaluate");
    match neg_inf.kind() {
        ValueKind::Float => {
            let f = neg_inf.as_float().unwrap();
            assert!(f.is_infinite() && f.is_sign_negative());
        }
        other => panic!("expected float, got {other:?}"),
    }

    let neg_nan = dispatch_builtin_pure("/", vec![Value::make_float(0.0), Value::make_float(0.0)])
        .expect("builtin / should resolve")
        .expect("float division should evaluate");
    match neg_nan.kind() {
        ValueKind::Float => {
            let f = neg_nan.as_float().unwrap();
            assert!(f.is_nan() && f.is_sign_negative());
        }
        other => panic!("expected float, got {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_ash_handles_extreme_negative_shift_counts() {
    crate::test_utils::init_test_tracing();
    let right = dispatch_builtin_pure(
        "ash",
        vec![Value::fixnum(3), Value::fixnum(Value::MOST_NEGATIVE_FIXNUM)],
    )
    .expect("builtin ash should resolve")
    .expect("builtin ash should evaluate");
    assert_eq!(right, Value::fixnum(0));

    let right_neg = dispatch_builtin_pure(
        "ash",
        vec![
            Value::fixnum(-3),
            Value::fixnum(Value::MOST_NEGATIVE_FIXNUM),
        ],
    )
    .expect("builtin ash should resolve")
    .expect("builtin ash should evaluate");
    assert_eq!(right_neg, Value::fixnum(-1));
}

#[test]
fn pure_dispatch_typed_abs_min_fixnum_promotes_to_bignum() {
    // GNU emacs 31.0.50 verified:
    //   (abs most-negative-fixnum) -> 2305843009213693952
    // -- the absolute value of the most-negative fixnum overflows
    // i64 by 1 bit, so GNU promotes to a bignum. Mirrors the
    // bignum-promotion path in builtin_abs (arithmetic.rs).
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure("abs", vec![Value::fixnum(Value::MOST_NEGATIVE_FIXNUM)])
        .expect("builtin abs should resolve")
        .expect("abs on most-negative-fixnum should promote to bignum");
    assert!(result.is_bignum(), "expected bignum, got {:?}", result);
    let s = result.as_bignum().unwrap().to_string();
    assert_eq!(s, "2305843009213693952");
}

#[test]
fn pure_dispatch_typed_eq_returns_truthy_for_same_symbol() {
    crate::test_utils::init_test_tracing();
    let sym = Value::symbol("typed-dispatch-test");
    let result = dispatch_builtin_pure("eq", vec![sym, sym])
        .expect("builtin eq should resolve")
        .expect("builtin eq should evaluate");
    assert!(result.is_truthy());
}

#[test]
fn pure_dispatch_typed_append_concatenates_lists() {
    crate::test_utils::init_test_tracing();
    let left = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    let right = Value::list(vec![Value::fixnum(3), Value::fixnum(4)]);
    let result = dispatch_builtin_pure("append", vec![left, right])
        .expect("builtin append should resolve")
        .expect("builtin append should evaluate");
    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(3),
            Value::fixnum(4)
        ])
    );
}

#[test]
fn pure_dispatch_typed_append_preserves_dotted_tail() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure(
        "append",
        vec![Value::list(vec![Value::fixnum(1)]), Value::T],
    )
    .expect("builtin append should resolve")
    .expect("builtin append should evaluate");
    assert_eq!(result, Value::cons(Value::fixnum(1), Value::T));

    let atom = dispatch_builtin_pure("append", vec![Value::NIL, Value::T])
        .expect("builtin append should resolve")
        .expect("builtin append should evaluate");
    assert_eq!(atom, Value::T);
}

#[test]
fn pure_dispatch_typed_append_reports_final_improper_tail_like_gnu() {
    crate::test_utils::init_test_tracing();
    let improper = Value::cons(Value::symbol("a"), Value::symbol("bad-tail"));
    let result = dispatch_builtin_pure("append", vec![improper, Value::NIL])
        .expect("builtin append should resolve");

    match result {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("listp"), Value::symbol("bad-tail")]
            );
        }
        other => panic!("expected wrong-type-argument listp bad-tail, got {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_append_flattens_vector_and_string() {
    crate::test_utils::init_test_tracing();
    let vector = Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]);
    let string = Value::string("AB");
    let result = dispatch_builtin_pure("append", vec![vector, string, Value::NIL])
        .expect("builtin append should resolve")
        .expect("builtin append should evaluate");
    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(65),
            Value::fixnum(66)
        ])
    );
}

#[test]
fn pure_dispatch_typed_append_flattens_bool_vector_logical_bits() {
    crate::test_utils::init_test_tracing();
    let bool_vector =
        super::chartable::builtin_bool_vector(vec![Value::NIL, Value::T, Value::NIL, Value::T])
            .expect("bool-vector");

    let result = dispatch_builtin_pure("append", vec![bool_vector, Value::NIL])
        .expect("builtin append should resolve")
        .expect("builtin append should evaluate");

    assert_eq!(
        result,
        Value::list(vec![Value::NIL, Value::T, Value::NIL, Value::T])
    );
}

#[test]
fn pure_dispatch_typed_append_rejects_char_table_like_gnu_concat_to_list() {
    crate::test_utils::init_test_tracing();
    let char_table =
        super::chartable::make_char_table_value(Value::symbol("test-only"), Value::fixnum(65));

    let err = dispatch_builtin_pure("append", vec![char_table, Value::NIL])
        .expect("builtin append should resolve")
        .expect_err("GNU append rejects char-tables");

    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("sequencep"), char_table]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_append_flattens_bytecode_slots() {
    crate::test_utils::init_test_tracing();
    let bc = Value::make_bytecode(crate::emacs_core::bytecode::ByteCodeFunction::new(
        LambdaParams::simple(vec![intern("x")]),
    ));
    let result = dispatch_builtin_pure("append", vec![bc, Value::NIL])
        .expect("builtin append should resolve")
        .expect("builtin append should evaluate");
    let slots = list_to_vec(&result).expect("bytecode append should produce a proper list");
    // GNU bytecode objects expose exactly the PVEC_CLOSURE slot count.
    // A four-argument make-byte-code object has only slots 0-3.
    assert_eq!(slots.len(), 4);
    assert!((slots[0].is_cons() || slots[0].is_nil()));
    assert!((slots[1].is_nil() || slots[1].is_string()));
    assert!(slots[2].is_vector());
    assert!(slots[3].is_fixnum());
}

#[test]
fn pure_dispatch_typed_length_predicates_accept_bytecode_functions() {
    crate::test_utils::init_test_tracing();
    let bc = Value::make_bytecode(crate::emacs_core::bytecode::ByteCodeFunction::new(
        LambdaParams::simple(vec![intern("x")]),
    ));
    let eq = dispatch_builtin_pure("length=", vec![bc, Value::fixnum(4)])
        .expect("builtin length= should resolve")
        .expect("builtin length= should evaluate");
    assert!(eq.is_truthy());
    let gt = dispatch_builtin_pure("length>", vec![bc, Value::fixnum(3)])
        .expect("builtin length> should resolve")
        .expect("builtin length> should evaluate");
    assert!(gt.is_truthy());
}

#[test]
fn pure_dispatch_typed_length_tracks_bytecode_doc_slot() {
    crate::test_utils::init_test_tracing();
    let mut bc =
        crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams::simple(vec![intern("x")]));
    bc.docstring = Some(crate::heap_types::LispString::from_utf8("doc"));
    let bc = Value::make_bytecode(bc);

    let len = dispatch_builtin_pure("length", vec![bc])
        .expect("builtin length should resolve")
        .expect("builtin length should evaluate");

    assert_eq!(len, Value::fixnum(5));
}

#[test]
fn pure_dispatch_typed_vconcat_flattens_bytecode_slots() {
    crate::test_utils::init_test_tracing();
    let bc = Value::make_bytecode(crate::emacs_core::bytecode::ByteCodeFunction::new(
        LambdaParams::simple(vec![intern("x")]),
    ));
    let result = dispatch_builtin_pure("vconcat", vec![bc])
        .expect("builtin vconcat should resolve")
        .expect("builtin vconcat should evaluate");
    if !result.is_vector() {
        panic!("expected vector result, got {result:?}");
    };
    let slots = result.as_vector_data().unwrap().clone();
    // GNU bytecode objects expose exactly the PVEC_CLOSURE slot count.
    assert_eq!(slots.len(), 4);
    assert!((slots[0].is_cons() || slots[0].is_nil()));
    assert!((slots[1].is_nil() || slots[1].is_string()));
    assert!(slots[2].is_vector());
    assert!(slots[3].is_fixnum());
}

#[test]
fn pure_dispatch_typed_vconcat_flattens_bool_vector_logical_bits() {
    crate::test_utils::init_test_tracing();
    let bool_vector =
        super::chartable::builtin_bool_vector(vec![Value::NIL, Value::T, Value::NIL, Value::T])
            .expect("bool-vector");

    let result = dispatch_builtin_pure("vconcat", vec![bool_vector])
        .expect("builtin vconcat should resolve")
        .expect("builtin vconcat should evaluate");

    assert_eq!(
        result,
        Value::vector(vec![Value::NIL, Value::T, Value::NIL, Value::T])
    );
}

#[test]
fn pure_dispatch_typed_length_tracks_interpreted_closure_slot_count() {
    crate::test_utils::init_test_tracing();
    let bare = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x")]),
        body: vec![Value::symbol("x")],
        env: Some(Value::NIL),
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    let with_doc = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x")]),
        body: vec![Value::symbol("x")],
        env: Some(Value::NIL),
        docstring: Some(crate::heap_types::LispString::from_utf8("doc")),
        doc_form: None,
        interactive: None,
    });

    let bare_len = dispatch_builtin_pure("length", vec![bare])
        .expect("builtin length should resolve")
        .expect("builtin length should evaluate");
    let doc_len = dispatch_builtin_pure("length", vec![with_doc])
        .expect("builtin length should resolve")
        .expect("builtin length should evaluate");

    assert_eq!(bare_len, Value::fixnum(3));
    assert_eq!(doc_len, Value::fixnum(5));
}

#[test]
fn make_interpreted_closure_preserves_nil_interactive_slot_presence() {
    crate::test_utils::init_test_tracing();
    let params = Value::NIL;
    let body = Value::list(vec![Value::fixnum(1)]);
    let env = Value::list(vec![Value::T]);
    let iform = Value::list(vec![Value::symbol("interactive")]);

    let closure = super::symbols::make_interpreted_closure_from_parts(
        &params,
        &body,
        &env,
        None,
        Some(&iform),
    )
    .expect("make-interpreted-closure should accept no-arg interactive form");

    assert_eq!(
        dispatch_builtin_pure("length", vec![closure])
            .expect("builtin length should resolve")
            .expect("builtin length should evaluate"),
        Value::fixnum(6)
    );
    assert_eq!(closure.closure_interactive(), Some(Value::NIL));

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        super::symbols::builtin_interactive_form(&mut eval, vec![closure])
            .expect("interactive-form should see nil slot"),
        Value::list(vec![Value::symbol("interactive"), Value::NIL])
    );
}

#[test]
fn make_interpreted_closure_matches_gnu_arglist_construction_checks() {
    crate::test_utils::init_test_tracing();
    let body = Value::list(vec![Value::NIL]);
    let env = Value::NIL;

    for params in [
        Value::list(vec![Value::fixnum(1)]),
        Value::list(vec![Value::symbol("&rest")]),
        Value::cons(Value::symbol("a"), Value::symbol("b")),
    ] {
        let closure =
            super::symbols::make_interpreted_closure_from_parts(&params, &body, &env, None, None)
                .expect("GNU accepts listp arglists without parsing formals");
        assert!(closure.is_lambda());
        assert_eq!(closure.closure_slot(0), Some(params));
    }

    let err = super::symbols::make_interpreted_closure_from_parts(
        &Value::fixnum(1),
        &body,
        &env,
        None,
        None,
    )
    .expect_err("GNU rejects non-listp arglist object");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn malformed_interpreted_closure_arglists_signal_invalid_function_at_call_time() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let rendered = format_eval_result(&eval.eval_str(
        r#"(list
             (condition-case err
                 (funcall (make-interpreted-closure '(1) '(nil) nil))
               (error err))
             (condition-case err
                 (funcall (make-interpreted-closure '(&rest) '(nil) nil))
               (error err))
             (condition-case err
                 (funcall (make-interpreted-closure '(a . b) '(nil) nil) 1)
               (error err)))"#,
    ));

    assert_eq!(
        rendered,
        "OK ((invalid-function #[(1) (nil) nil]) (invalid-function #[(&rest) (nil) nil]) (invalid-function #[(a . b) (nil) nil]))"
    );
}

#[test]
fn compiled_literal_reifier_preserves_ordinary_vectors() {
    crate::test_utils::init_test_tracing();
    let closure_vec = Value::vector(vec![
        Value::list(vec![Value::symbol("x")]),
        Value::list(vec![Value::list(vec![
            Value::symbol("+"),
            Value::symbol("x"),
            Value::fixnum(1),
        ])]),
        Value::NIL,
    ]);

    let converted = super::symbols::try_convert_nested_compiled_literal(closure_vec);
    assert!(
        converted.is_vector(),
        "ordinary vectors are not reader closures"
    );
}

#[test]
fn compiled_literal_reifier_preserves_cperl_key_vector_shape() {
    crate::test_utils::init_test_tracing();
    let key_vec = Value::vector(vec![
        Value::list(vec![Value::symbol("control"), Value::fixnum(99)]),
        Value::list(vec![Value::symbol("control"), Value::fixnum(104)]),
        Value::fixnum(70),
    ]);

    let converted = super::symbols::try_convert_nested_compiled_literal(key_vec);
    assert!(
        converted.is_vector(),
        "key vectors must not become closures"
    );
}

// `pure_dispatch_typed_string_equal_aliases_match' is gone: it asserted that
// two REGISTERED subrs, `string-equal' and `string=', dispatched to the same
// Rust function.  GNU has only the first; `string=' is
// `(defalias 'string= #'string-equal)' at lisp/subr.el:2277, so the identity
// to state is the alias one, and it is stated of the loaded runtime in
// `the_five_alias_cells_hold_a_symbol_in_the_loaded_runtime_like_gnu'
// (`builtins/lisp_only_predicates_and_aliases_test.rs').  DIVERGENCES.md 148.

#[test]
fn string_comparison_orders_eight_bit_above_ascii_and_keeps_unibyte_distinct() {
    crate::test_utils::init_test_tracing();
    let mut buf = [0u8; 8];
    let len = crate::emacs_core::emacs_char::char_string(
        crate::emacs_core::emacs_char::byte8_to_char(0xFF),
        &mut buf,
    );
    let multibyte_eight_bit = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        buf[..len].to_vec(),
    ));

    // An eight-bit char sorts above ASCII (char-code order, matching GNU).
    let lt = dispatch_builtin_pure(
        "string-lessp",
        vec![Value::string("a"), multibyte_eight_bit],
    )
    .expect("string-lessp resolves")
    .expect("string-lessp evaluates");
    assert!(lt.is_truthy(), "ASCII should sort before an eight-bit char");

    // string-equal keeps a unibyte raw byte distinct from the multibyte
    // eight-bit char.
    let unibyte = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let eq = dispatch_builtin_pure("string-equal", vec![unibyte, multibyte_eight_bit])
        .expect("string-equal resolves")
        .expect("string-equal evaluates");
    assert!(
        !eq.is_truthy(),
        "unibyte 0xFF must not equal the multibyte eight-bit char"
    );
}

#[test]
fn pure_dispatch_typed_string_comparisons_accept_symbol_designators() {
    crate::test_utils::init_test_tracing();
    let less = dispatch_builtin_pure(
        "string-lessp",
        vec![Value::symbol("foo"), Value::string("g")],
    )
    .expect("builtin string-lessp should resolve")
    .expect("builtin string-lessp should evaluate");
    assert!(less.is_truthy());

    let equal = dispatch_builtin_pure("string-equal", vec![Value::T, Value::string("t")])
        .expect("builtin string-equal should resolve")
        .expect("builtin string-equal should evaluate");
    assert!(equal.is_truthy());

    // `(string-greaterp A B)' is `(string-lessp B A)' -- lisp/subr.el:6285 --
    // so the designator arms it used to be asked about are `string-lessp's.
    let greater = dispatch_builtin_pure("string-lessp", vec![Value::string("a"), Value::NIL])
        .expect("builtin string-lessp should resolve")
        .expect("builtin string-lessp should evaluate");
    assert!(greater.is_truthy());

    let err = dispatch_builtin_pure("string-lessp", vec![Value::string("a"), Value::fixnum(7)])
        .expect("builtin string-lessp should resolve")
        .expect_err("string-lessp should reject non string/symbol designators");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(7)],);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_downcase_unicode_edge_payloads_match_oracle() {
    crate::test_utils::init_test_tracing();
    let cases = [
        (304, 304),
        (7305, 7305),
        (8490, 8490),
        (42955, 42955),
        (42956, 42956),
        (42958, 42958),
        (42962, 42962),
        (42964, 42964),
        (42970, 42970),
        (42972, 42972),
        (68944, 68944),
        (68965, 68965),
        (93856, 93856),
        (93880, 93880),
        (66560, 66600),
    ];

    for (input, expected) in cases {
        let result = dispatch_builtin_pure("downcase", vec![Value::fixnum(input)])
            .expect("builtin downcase should resolve")
            .expect("builtin downcase should evaluate");
        assert_eq!(
            result,
            Value::fixnum(expected),
            "downcase({input}) should equal {expected}"
        );
    }

    let dotted_i = dispatch_builtin_pure("downcase", vec![Value::char('\u{0130}')])
        .expect("builtin downcase should resolve")
        .expect("builtin downcase should evaluate");
    assert_eq!(dotted_i, Value::char('\u{0130}'));

    let kelvin = dispatch_builtin_pure("downcase", vec![Value::string("\u{212A}")])
        .expect("builtin downcase should resolve")
        .expect("builtin downcase should evaluate");
    assert_eq!(kelvin, Value::string("\u{212A}"));

    let dotted_i_string = dispatch_builtin_pure("downcase", vec![Value::string("\u{0130}")])
        .expect("builtin downcase should resolve")
        .expect("builtin downcase should evaluate");
    assert_eq!(dotted_i_string, Value::string("i\u{307}"));

    let preserve_latin = dispatch_builtin_pure("downcase", vec![Value::string("\u{A7CB}")])
        .expect("builtin downcase should resolve")
        .expect("builtin downcase should evaluate");
    assert_eq!(preserve_latin, Value::string("\u{A7CB}"));

    let preserve_cyrillic_sup = dispatch_builtin_pure("downcase", vec![Value::string("\u{10D50}")])
        .expect("builtin downcase should resolve")
        .expect("builtin downcase should evaluate");
    assert_eq!(preserve_cyrillic_sup, Value::string("\u{10D50}"));

    let preserve_adlam = dispatch_builtin_pure("downcase", vec![Value::string("\u{16EA0}")])
        .expect("builtin downcase should resolve")
        .expect("builtin downcase should evaluate");
    assert_eq!(preserve_adlam, Value::string("\u{16EA0}"));

    let negative = dispatch_builtin_pure("downcase", vec![Value::fixnum(-1)])
        .expect("builtin downcase should resolve")
        .expect_err("builtin downcase should reject negative integer designators");
    match negative {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("char-or-string-p"), Value::fixnum(-1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_upcase_unicode_edge_payloads_match_oracle() {
    crate::test_utils::init_test_tracing();
    let cases = [
        (223, 7838),
        (305, 305),
        (7306, 7306),
        (8064, 8072),
        (8071, 8079),
        (8080, 8088),
        (8087, 8095),
        (8096, 8104),
        (8103, 8111),
        (8115, 8124),
        (8131, 8140),
        (8179, 8188),
        (42957, 42957),
        (68976, 68976),
        (68997, 68997),
        (93883, 93883),
        (93907, 93907),
        (97, 65),
    ];

    for (input, expected) in cases {
        let result = dispatch_builtin_pure("upcase", vec![Value::fixnum(input)])
            .expect("builtin upcase should resolve")
            .expect("builtin upcase should evaluate");
        assert_eq!(
            result,
            Value::fixnum(expected),
            "upcase({input}) should equal {expected}"
        );
    }

    let sharp_s = dispatch_builtin_pure("upcase", vec![Value::char('ß')])
        .expect("builtin upcase should resolve")
        .expect("builtin upcase should evaluate");
    assert_eq!(sharp_s, Value::char('\u{1E9E}'));

    let sharp_s_string = dispatch_builtin_pure("upcase", vec![Value::string("ß")])
        .expect("builtin upcase should resolve")
        .expect("builtin upcase should evaluate");
    assert_eq!(sharp_s_string, Value::string("SS"));

    let dotless_i_string = dispatch_builtin_pure("upcase", vec![Value::string("\u{0131}")])
        .expect("builtin upcase should resolve")
        .expect("builtin upcase should evaluate");
    assert_eq!(dotless_i_string, Value::string("\u{0131}"));

    let preserve_latin = dispatch_builtin_pure("upcase", vec![Value::string("\u{019B}")])
        .expect("builtin upcase should resolve")
        .expect("builtin upcase should evaluate");
    assert_eq!(preserve_latin, Value::string("\u{019B}"));

    let preserve_cyrillic_sup = dispatch_builtin_pure("upcase", vec![Value::string("\u{10D70}")])
        .expect("builtin upcase should resolve")
        .expect("builtin upcase should evaluate");
    assert_eq!(preserve_cyrillic_sup, Value::string("\u{10D70}"));

    let preserve_adlam = dispatch_builtin_pure("upcase", vec![Value::string("\u{16EBB}")])
        .expect("builtin upcase should resolve")
        .expect("builtin upcase should evaluate");
    assert_eq!(preserve_adlam, Value::string("\u{16EBB}"));

    let negative = dispatch_builtin_pure("upcase", vec![Value::fixnum(-1)])
        .expect("builtin upcase should resolve")
        .expect_err("builtin upcase should reject negative integer designators");
    match negative {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("char-or-string-p"), Value::fixnum(-1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_case_conversion_preserves_raw_unibyte_payloads() {
    crate::test_utils::init_test_tracing();
    let upper_raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'A', 0xFF,
    ]));
    let downcased = dispatch_builtin_pure("downcase", vec![upper_raw])
        .expect("builtin downcase should resolve")
        .expect("builtin downcase should evaluate");
    let downcased = downcased.as_lisp_string().expect("downcase string");
    assert!(!downcased.is_multibyte());
    assert_eq!(downcased.as_bytes(), &[b'a', 0xFF]);

    let lower_raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF,
    ]));
    let upcased = dispatch_builtin_pure("upcase", vec![lower_raw])
        .expect("builtin upcase should resolve")
        .expect("builtin upcase should evaluate");
    let upcased = upcased.as_lisp_string().expect("upcase string");
    assert!(!upcased.is_multibyte());
    assert_eq!(upcased.as_bytes(), &[b'A', 0xFF]);
}

#[test]
fn keymapp_accepts_lisp_keymap_cons_cells() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let proper = Value::list(vec![Value::symbol("keymap")]);
    assert_eq!(builtin_keymapp(&mut eval, vec![proper]).unwrap(), Value::T);

    let proper_with_entry = Value::cons(
        Value::symbol("keymap"),
        Value::cons(
            Value::cons(Value::fixnum(97), Value::symbol("ignore")),
            Value::NIL,
        ),
    );
    assert_eq!(
        builtin_keymapp(&mut eval, vec![proper_with_entry]).unwrap(),
        Value::T
    );

    let improper = Value::cons(Value::symbol("keymap"), Value::symbol("tail"));
    assert_eq!(
        builtin_keymapp(&mut eval, vec![improper]).unwrap(),
        Value::T
    );

    let non_keymap = Value::list(vec![Value::symbol("foo"), Value::symbol("keymap")]);
    assert_eq!(
        builtin_keymapp(&mut eval, vec![non_keymap]).unwrap(),
        Value::NIL
    );
}

#[test]
fn keymapp_rejects_non_keymap_integer_designators() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let keymap = builtin_make_sparse_keymap(&mut eval, vec![]).unwrap();
    assert_eq!(builtin_keymapp(&mut eval, vec![keymap]).unwrap(), Value::T);
    assert_eq!(
        builtin_keymapp(&mut eval, vec![Value::fixnum(16)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_keymapp(&mut eval, vec![Value::fixnum(999_999)]).unwrap(),
        Value::NIL
    );
}

/// GNU `access_keymap_1` searches a keymap embedded in a composed keymap's spine
/// by recursing into ITSELF -- `access_keymap_1 (submap, idx, t_ok, noinherit,
/// autoload)` -- so the member is searched with its own parent chain, and a
/// prefix it shares with that parent is merged before the composed map's own
/// parent is considered.
///
/// Searching the member one level deep instead loses every binding a composed
/// map's member inherits. That is the shape every ivy/swiper minibuffer has:
/// `read-from-minibuffer` gets `(make-composed-keymap swiper-isearch-map
/// ivy-minibuffer-map)`, and `swiper-isearch-map`'s parent `swiper-map` is where
/// `M-q` lives -- so `M-q` fell through to the global `fill-paragraph` and
/// swiper's query-replace was unreachable.
#[test]
fn lookup_key_follows_the_parent_of_a_composed_keymaps_member() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.eval_str(
        r#"(progn
             (setq p15-grand (make-sparse-keymap))
             (setq p15-inner (make-sparse-keymap))
             (setq p15-outer-parent (make-sparse-keymap))
             ;; Every map binds under the SAME prefix (ESC, the meta prefix), so
             ;; the lookup has to merge three submaps rather than take the first.
             (define-key p15-grand [27 ?q] 'p15-from-grandparent)
             (define-key p15-inner [27 ?n] 'p15-from-inner)
             (define-key p15-outer-parent [27 ?o] 'p15-from-outer-parent)
             (set-keymap-parent p15-inner p15-grand)
             ;; What `make-composed-keymap' builds: (keymap MEMBER... . PARENT).
             (setq p15-composed (cons 'keymap (cons p15-inner p15-outer-parent))))"#,
    )
    .unwrap();

    let looked_up = eval
        .eval_str(
            r#"(prin1-to-string
                 (list (lookup-key p15-inner [27 ?q])
                       (lookup-key p15-composed [27 ?q])
                       (lookup-key p15-composed [27 ?n])
                       (lookup-key p15-composed [27 ?o])))"#,
        )
        .unwrap();
    assert_eq!(
        looked_up.as_str_owned().as_deref(),
        Some("(p15-from-grandparent p15-from-grandparent p15-from-inner p15-from-outer-parent)"),
        "a composed map must reach its member's inherited bindings, not only the member's own"
    );
}

/// GNU's own `help--describe-vector/bug-9293-*` tests (test/src/keymap-tests.el),
/// which pin that a key RANGE is only printed when every key in it is shadowed
/// the same way. Pinned here because `help--describe-vector` was a no-op stub:
/// `describe-map` reaches every dense keymap element through it, so while it
/// returned nil every char-table binding -- `self-insert-command` over the whole
/// printable range, all of `key-translation-map` -- was missing from
/// `describe-bindings`.
#[test]
fn help_describe_vector_prints_ranges_only_where_shadowing_agrees() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::load::create_bootstrap_evaluator_cached;
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let rendered = eval
        .eval_str(
            r#"(let ((orig-map (let ((map (make-keymap)))
                                 (define-key map "e" 'foo)
                                 (define-key map "f" 'foo)
                                 (define-key map "g" 'foo)
                                 (define-key map "h" 'foo)
                                 map))
                     (shadow-map (let ((map (make-keymap)))
                                   (define-key map "f" 'bar)
                                   map))
                     (text-quoting-style 'grave)
                     (describe-bindings-check-shadowing-in-ranges 'ignore-self-insert))
                 (with-temp-buffer
                   (help--describe-vector (cadr orig-map) nil
                                          (lambda (def) (insert (symbol-name def) "\n"))
                                          t shadow-map orig-map t)
                   (buffer-substring-no-properties (point-min) (point-max))))"#,
        )
        .unwrap();
    assert_eq!(
        rendered.as_str_owned().as_deref(),
        Some("\nefoo\nffoo  (currently shadowed by `bar')\ng .. hfoo\n"),
        "GNU prints e and f singly (f is shadowed by a different command) and g..h as a range"
    );
}

/// GNU `map_keymap_char_table_item` (keymap.c) copies a char-table range key
/// before handing it on, with the comment "make a copy since map_char_table
/// modifies it in place". `map-char-table` really does reuse ONE cons and mutate
/// it as it walks -- our `for_each_char_table_mapping` mirrors that on purpose --
/// so a keymap walk that plans all its pairs before calling FUNCTION hands every
/// range the SAME cons, carrying the final post-walk `(LAST+1 . MAX_CHAR)`.
///
/// That is not cosmetic: `keymap-canonicalize` collects range keys during
/// `map-keymap` and calls `define-key` on them afterwards, so every dense
/// binding in the map lands on the wrong characters, and `describe-bindings`
/// loses the whole printable range of the global map.
#[test]
fn map_keymap_copies_char_table_range_keys_before_the_shared_cons_moves() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.eval_str(
        r#"(progn
             (setq p15-dense (make-keymap))
             (define-key p15-dense [?a] 'p15-cmd)
             (define-key p15-dense [?b] 'p15-cmd)
             (define-key p15-dense [?c] 'p15-cmd)
             (setq p15-seen nil)
             (map-keymap (lambda (key def)
                           (setq p15-seen (cons (cons key def) p15-seen)))
                         p15-dense))"#,
    )
    .unwrap();
    let seen = eval.eval_str("(prin1-to-string p15-seen)").unwrap();
    assert_eq!(
        seen.as_str_owned().as_deref(),
        Some("(((97 . 99) . p15-cmd))"),
        "map-keymap must report the bound range, not the shared cons's final value"
    );
}

/// GNU `accessible_keymaps_1` resolves each binding with
/// `get_keymap (get_keyelt (cmd, 0), 0, 0)`, so a prefix bound to a SYMBOL whose
/// function cell is a keymap -- what `define-prefix-command` builds, and how
/// every one of the global map's own prefixes (`C-x` -> `Control-X-prefix`,
/// `C-c` -> `mode-specific-command-prefix`) is stored -- is a prefix like any
/// other. Testing only cons-valued bindings hid that the entire global keymap
/// tree was invisible to `describe-bindings`.
#[test]
fn accessible_keymaps_follows_a_prefix_bound_to_a_symbol_naming_a_keymap() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.eval_str(
        r#"(progn
             (setq p15-root (make-sparse-keymap))
             (setq p15-prefix-map (make-sparse-keymap))
             ;; What `define-prefix-command' builds: the symbol's FUNCTION cell
             ;; holds the keymap, and the key is bound to the symbol.
             (fset 'p15-prefix p15-prefix-map)
             (define-key p15-root "\C-x" 'p15-prefix)
             (define-key p15-prefix-map "a" 'p15-cmd))"#,
    )
    .unwrap();

    let all = eval.eval_str("(accessible-keymaps p15-root)").unwrap();
    let items = list_to_vec(&all).expect("accessible-keymaps should return list");
    let prefixes: Vec<Vec<Value>> = items
        .iter()
        .map(|entry| entry.cons_car().as_vector_data().unwrap().clone())
        .collect();
    assert_eq!(
        prefixes,
        vec![vec![], vec![Value::fixnum(24)]],
        "the symbol-valued C-x prefix must be followed"
    );
}

/// GNU `accessible_keymaps_1`: when the sequence so far ends in
/// `meta-prefix-char`, the next key REPLACES that ESC rather than extending the
/// sequence, and gains the meta bit. GNU reports `ESC s`-reachable maps under
/// `[M-s]`, never under `[27 115]`.
#[test]
fn accessible_keymaps_metizes_a_key_following_meta_prefix_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.eval_str(
        r#"(progn
             (setq p15-root (make-sparse-keymap))
             (setq p15-esc (make-sparse-keymap))
             (setq p15-sub (make-sparse-keymap))
             (define-key p15-root [27] p15-esc)
             (define-key p15-esc "s" p15-sub))"#,
    )
    .unwrap();

    let all = eval.eval_str("(accessible-keymaps p15-root)").unwrap();
    let items = list_to_vec(&all).expect("accessible-keymaps should return list");
    let prefixes: Vec<Vec<Value>> = items
        .iter()
        .map(|entry| entry.cons_car().as_vector_data().unwrap().clone())
        .collect();
    assert_eq!(
        prefixes,
        vec![
            vec![],
            vec![Value::fixnum(27)],
            vec![Value::fixnum(0x800_0000 | i64::from(b's'))],
        ],
        "ESC s must be reported as [M-s], not [27 115]"
    );
}

/// GNU `Fkeymap__get_keyelt` (keymap.c) delegates to `get_keyelt`: a
/// `(menu-item NAME DEFN ...)` wrapper and an old-style `(STRING . DEFN)` menu
/// label both reduce to DEFN.  help.el's describe-map keeps a row only when
/// this reduction equals `(lookup-key tail (vector event) t)`, so an identity
/// stub here empties every menu keymap section.  Verified against GNU 31.0.90.
#[test]
fn keymap_get_keyelt_strips_menu_item_and_string_label_wrappers() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    for (form, expected) in [
        (
            "(keymap--get-keyelt '(menu-item \"Name\" p61-cmd :enable t) nil)",
            "p61-cmd",
        ),
        ("(keymap--get-keyelt '(\"Label\" . p61-cmd) nil)", "p61-cmd"),
        (
            "(keymap--get-keyelt '(\"Label\" \"Help\" . p61-cmd) nil)",
            "p61-cmd",
        ),
        ("(keymap--get-keyelt 'p61-cmd nil)", "p61-cmd"),
    ] {
        let result = eval.eval_str(form).unwrap();
        assert_eq!(result.as_symbol_name(), Some(expected), "{form}");
    }
}

/// GNU `get_keymap (OBJECT, 0, 0)` (keymap.c) treats a symbol whose function
/// cell is an `(autoload FILE DOC INTERACTIVE keymap)` form as a keymap
/// WITHOUT loading it: `keymapp` answers t, and `accessible_keymaps_1` lists
/// the symbol itself as the map the prefix reaches.  Descending is deferred to
/// `map-keymap` (autoload=1), which help.el's describe-map reaches through
/// keymap-canonicalize -- that is how describe-buffer-bindings expands the
/// C-x C-k kmacro-keymap section in an emacs -Q batch where kmacro.el is not
/// loaded.  Verified against GNU 31.0.90:
///   (keymapp 'kmacro-keymap) => t  (while still an autoload form)
///   (accessible-keymaps (list 'keymap (cons ?a 'kmacro-keymap)))
///     => (([] keymap (97 . kmacro-keymap)) ([97] . kmacro-keymap))
#[test]
fn accessible_keymaps_lists_an_autoloaded_keymap_symbol_without_loading_it() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.eval_str(
        r#"(progn
             (setq p61-root (make-sparse-keymap))
             (fset 'p61-prefix '(autoload "p61-missing-file" "doc" t keymap))
             (define-key p61-root "\C-k" 'p61-prefix))"#,
    )
    .unwrap();

    assert_eq!(
        eval.eval_str("(keymapp 'p61-prefix)").unwrap(),
        Value::T,
        "an unloaded autoload keymap symbol satisfies keymapp in GNU"
    );

    let all = eval.eval_str("(accessible-keymaps p61-root)").unwrap();
    let items = list_to_vec(&all).expect("accessible-keymaps should return list");
    assert_eq!(items.len(), 2, "{items:?}");
    let second = &items[1];
    assert_eq!(
        second.cons_car().as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(11)]
    );
    assert_eq!(
        second.cons_cdr().as_symbol_name(),
        Some("p61-prefix"),
        "the symbol itself is listed unexpanded; map-keymap autoloads it later"
    );
}

#[test]
fn accessible_keymaps_reports_root_and_prefix_paths() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let root = builtin_make_sparse_keymap(&mut eval, vec![]).unwrap();
    let child = builtin_make_sparse_keymap(&mut eval, vec![]).unwrap();
    builtin_define_key(
        &mut eval,
        vec![root, Value::string("\x18"), child], // \C-x = 0x18
    )
    .unwrap();

    let all = builtin_accessible_keymaps(&mut eval, vec![root]).unwrap();
    let all_items = list_to_vec(&all).expect("accessible-keymaps should return list");
    assert_eq!(all_items.len(), 2);

    assert!(all_items[0].is_cons(), "expected cons cell");
    let first_car = all_items[0].cons_car();
    let first_cdr = all_items[0].cons_cdr();
    assert_eq!(first_car, Value::vector(vec![]));
    assert_eq!(
        builtin_keymapp(&mut eval, vec![first_cdr]).unwrap(),
        Value::T
    );

    let filtered = builtin_accessible_keymaps(
        &mut eval,
        vec![root, Value::vector(vec![Value::fixnum(24)])],
    )
    .unwrap();
    let filtered_items = list_to_vec(&filtered).expect("filtered accessible-keymaps list");
    assert_eq!(filtered_items.len(), 1);
    assert!(filtered_items[0].is_cons(), "expected cons cell");
    let only_car = filtered_items[0].cons_car();
    assert_eq!(only_car, Value::vector(vec![Value::fixnum(24)]));

    let no_match = builtin_accessible_keymaps(
        &mut eval,
        vec![root, Value::vector(vec![Value::fixnum(97)])],
    )
    .unwrap();
    assert!(no_match.is_nil());
}

#[test]
fn accessible_keymaps_prefix_type_errors_match_oracle_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let map = builtin_make_sparse_keymap(&mut eval, vec![]).unwrap();

    let sequence_err = builtin_accessible_keymaps(&mut eval, vec![map, Value::T]).unwrap_err();
    match sequence_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("sequencep"), Value::T]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let array_err =
        builtin_accessible_keymaps(&mut eval, vec![map, Value::list(vec![Value::symbol("a")])])
            .unwrap_err();
    match array_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("arrayp"),
                    Value::list(vec![Value::symbol("a")])
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn key_description_renders_super_prefixed_symbol_events_with_expected_angles() {
    crate::test_utils::init_test_tracing();
    let super_only = builtin_key_description(vec![Value::vector(vec![Value::symbol("s-f1")])])
        .expect("key-description should succeed");
    assert_eq!(super_only, Value::string("s-<f1>"));

    let ctrl_super = builtin_key_description(vec![Value::vector(vec![Value::symbol("C-s-f1")])])
        .expect("key-description should succeed");
    assert_eq!(ctrl_super, Value::string("C-s-<f1>"));

    let single = builtin_single_key_description(vec![Value::symbol("s-f1")])
        .expect("single-key-description should succeed");
    assert_eq!(single, Value::string("s-<f1>"));
}

#[test]
fn key_description_symbol_modifier_edges_match_emacs() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_single_key_description(vec![Value::symbol("M-a")])
            .expect("single-key-description should succeed"),
        Value::string("<M-a>")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::symbol("C-a")])
            .expect("single-key-description should succeed"),
        Value::string("<C-a>")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::symbol("C-M-a")])
            .expect("single-key-description should succeed"),
        Value::string("C-<M-a>")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::symbol("M-a"), Value::T])
            .expect("single-key-description should succeed"),
        Value::string("M-a")
    );
    assert_eq!(
        builtin_key_description(vec![Value::vector(vec![Value::symbol("M-a")])])
            .expect("key-description should succeed"),
        Value::string("<M-a>")
    );
    assert_eq!(
        builtin_key_description(vec![Value::vector(vec![Value::symbol("C-s-f1")])])
            .expect("key-description should succeed"),
        Value::string("C-s-<f1>")
    );
}

#[test]
fn key_description_integer_modifier_and_nonunicode_edges_match_emacs() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_single_key_description(vec![Value::fixnum(0x40_0000)])
            .expect("single-key-description should succeed"),
        Value::string("A-C-@")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::fixnum(58_720_257)])
            .expect("single-key-description should succeed"),
        Value::string("C-H-S-s-a")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::fixnum(264_241_249)])
            .expect("single-key-description should succeed"),
        Value::string("A-C-H-M-S-s-a")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::fixnum(134_217_737)])
            .expect("single-key-description should succeed"),
        Value::string("C-M-i")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::fixnum(138_412_041)])
            .expect("single-key-description should succeed"),
        Value::string("A-C-M-i")
    );
    assert_eq!(
        builtin_single_key_description(vec![Value::fixnum(201_326_601)])
            .expect("single-key-description should succeed"),
        Value::string("C-M-i")
    );

    // Non-Unicode char codes produce a string (with lossy UTF-8 rendering)
    let single_nonunicode = builtin_single_key_description(vec![Value::fixnum(0x11_0000)])
        .expect("single-key-description should support nonunicode char code");
    assert!(single_nonunicode.is_string());

    let key_nonunicode =
        builtin_key_description(vec![Value::vector(vec![Value::fixnum(0x20_0000)])])
            .expect("key-description should support nonunicode char code");
    assert!(key_nonunicode.is_string());

    assert_eq!(
        builtin_key_description(vec![Value::vector(vec![Value::fixnum(0x40_0000)])])
            .expect("key-description should succeed"),
        Value::string("A-C-@")
    );
    assert_eq!(
        builtin_key_description(vec![Value::vector(vec![Value::fixnum(134_217_737)])])
            .expect("key-description should succeed"),
        Value::string("C-M-i")
    );
    assert_eq!(
        builtin_key_description(vec![Value::vector(vec![Value::fixnum(201_326_601)])])
            .expect("key-description should succeed"),
        Value::string("C-M-i")
    );
}

#[test]
fn key_description_accepts_raw_unibyte_string_sequences() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0x80, 0x9b, 0xf8, 0xff,
    ]));
    let described = builtin_key_description(vec![raw]).expect("key-description should succeed");
    assert_eq!(described, Value::string("C-M-@ M-ESC M-x M-DEL"));
}

#[test]
fn eval_get_file_buffer_matches_visited_paths() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let id = eval.buffers.create_buffer("gfb");

    let path = std::env::temp_dir().join(format!("neovm-gfb-{}-{}", std::process::id(), "eval"));
    std::fs::write(&path, b"gfb").expect("write test file");
    let file = path.to_string_lossy().to_string();
    eval.buffers
        .get_mut(id)
        .unwrap()
        .set_file_name_value(Value::string(file.clone()));

    let exact = builtin_get_file_buffer(&mut eval, vec![Value::string(&file)]).unwrap();
    assert_eq!(exact, Value::make_buffer(id));

    let truename = std::fs::canonicalize(&path)
        .expect("canonicalize file")
        .to_string_lossy()
        .to_string();
    let true_match = builtin_get_file_buffer(&mut eval, vec![Value::string(truename)]).unwrap();
    assert_eq!(true_match, Value::make_buffer(id));

    let default_dir = format!("{}/", path.parent().unwrap().to_string_lossy());
    let basename = path.file_name().unwrap().to_string_lossy().to_string();
    eval.buffers
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("default-directory", Value::string(default_dir));
    let relative = builtin_get_file_buffer(&mut eval, vec![Value::string(basename)]).unwrap();
    assert_eq!(relative, Value::make_buffer(id));

    let _ = std::fs::remove_file(path);
}

#[test]
fn eval_get_file_buffer_type_and_missing_paths() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let missing = builtin_get_file_buffer(
        &mut eval,
        vec![Value::string("/tmp/neovm-no-such-file-for-gfb")],
    )
    .unwrap();
    assert!(missing.is_nil());
    assert!(builtin_get_file_buffer(&mut eval, vec![Value::fixnum(1)]).is_err());
}

#[test]
fn eval_builtin_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let err = builtin_eval(
        &mut eval,
        vec![Value::fixnum(1), Value::NIL, Value::symbol("ignored")],
    )
    .expect_err("eval should reject more than two arguments");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data, vec![Value::symbol("eval"), Value::fixnum(3)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn eval_buffer_live_p_tracks_killed_buffers() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let buf = builtin_get_buffer_create(&mut eval, vec![Value::string("*blp*")]).unwrap();
    let live = builtin_buffer_live_p(&mut eval, vec![buf]).unwrap();
    assert_eq!(live, Value::T);

    let _ = builtin_kill_buffer(&mut eval, vec![buf]).unwrap();
    let dead = builtin_buffer_live_p(&mut eval, vec![buf]).unwrap();
    assert_eq!(dead, Value::NIL);
}

#[test]
fn kill_buffer_optional_arg_and_error_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let a = builtin_get_buffer_create(&mut eval, vec![Value::string("*kb-opt-a*")]).unwrap();
    let b = builtin_get_buffer_create(&mut eval, vec![Value::string("*kb-opt-b*")]).unwrap();
    let _ = builtin_set_buffer(&mut eval, vec![a]).unwrap();

    // Optional argument omitted kills current buffer and selects another.
    let killed_current = builtin_kill_buffer(&mut eval, vec![]).unwrap();
    assert_eq!(killed_current, Value::T);
    assert_eq!(
        builtin_buffer_live_p(&mut eval, vec![a]).unwrap(),
        Value::NIL
    );
    assert!(
        builtin_current_buffer(&mut eval, vec![])
            .unwrap()
            .is_buffer()
    );

    // Missing buffer name signals `(error "No buffer named ...")`.
    let missing = builtin_kill_buffer(&mut eval, vec![Value::string("*kb-opt-missing*")])
        .expect_err("kill-buffer should signal on missing name");
    match missing {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("No buffer named *kb-opt-missing*")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    // Dead buffer object returns nil.
    let dead = create_unique_test_buffer(&mut eval, "*kb-opt-dead*");
    assert_eq!(
        builtin_kill_buffer(&mut eval, vec![dead]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_kill_buffer(&mut eval, vec![dead]).unwrap(),
        Value::NIL
    );

    // Non-buffer/non-string designators signal `wrong-type-argument`.
    let type_err = builtin_kill_buffer(&mut eval, vec![Value::fixnum(1)])
        .expect_err("kill-buffer should reject non-string designator");
    match type_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let _ = builtin_kill_buffer(&mut eval, vec![b]).unwrap();
}

#[test]
fn set_buffer_rejects_deleted_buffer_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let dead = create_unique_test_buffer(&mut eval, "*sb-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead]).unwrap();

    let err = builtin_set_buffer(&mut eval, vec![dead])
        .expect_err("set-buffer should reject deleted buffer objects");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Selecting deleted buffer")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn set_buffer_does_not_record_buffer_list_order_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let scratch = eval
        .buffers
        .find_buffer_by_name("*scratch*")
        .expect("scratch");
    let first = eval.buffers.create_buffer("*sb-order-a*");
    let second = eval.buffers.create_buffer("*sb-order-b*");

    let result = builtin_set_buffer(&mut eval, vec![Value::make_buffer(second)])
        .expect("set-buffer should select live buffer");
    assert_eq!(result, Value::make_buffer(second));
    assert_eq!(eval.buffers.current_buffer_id(), Some(second));
    assert_eq!(
        builtin_buffer_list(&mut eval, vec![]).expect("buffer-list"),
        Value::list(vec![
            Value::make_buffer(scratch),
            Value::make_buffer(first),
            Value::make_buffer(second),
        ])
    );
}

#[test]
fn eval_buffer_live_p_non_buffer_objects_return_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let by_name = builtin_buffer_live_p(&mut eval, vec![Value::string("*scratch*")]).unwrap();
    assert_eq!(by_name, Value::NIL);
    let nil_arg = builtin_buffer_live_p(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(nil_arg, Value::NIL);
}

#[test]
fn get_buffer_create_accepts_optional_second_arg() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let first = builtin_get_buffer_create(
        &mut eval,
        vec![Value::string("*gbc-opt*"), Value::fixnum(7)],
    )
    .unwrap();
    let second =
        builtin_get_buffer_create(&mut eval, vec![Value::string("*gbc-opt*"), Value::NIL]).unwrap();
    assert_eq!(first, second);

    let err = builtin_get_buffer_create(
        &mut eval,
        vec![Value::string("*gbc-opt*"), Value::NIL, Value::NIL],
    )
    .expect_err("get-buffer-create should reject more than two args");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("get-buffer-create"), Value::fixnum(3)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn get_buffer_create_accepts_live_and_killed_buffer_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let live = builtin_get_buffer_create(&mut eval, vec![Value::string("*gbc-live*")]).unwrap();
    let live_again = builtin_get_buffer_create(&mut eval, vec![live]).unwrap();
    assert_eq!(live_again, live);

    let dead = create_unique_test_buffer(&mut eval, "*gbc-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead]).unwrap();
    let dead_again = builtin_get_buffer_create(&mut eval, vec![dead]).unwrap();
    assert_eq!(dead_again, dead);
}

#[test]
fn buffer_creation_helpers_reject_missing_required_name_arg() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let err = builtin_get_buffer_create(&mut eval, vec![])
        .expect_err("get-buffer-create should reject missing required arg");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("get-buffer-create"), Value::fixnum(0)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let err = builtin_generate_new_buffer_name(&mut eval, vec![])
        .expect_err("generate-new-buffer-name should reject missing required arg");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("generate-new-buffer-name"), Value::fixnum(0)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn get_buffer_rejects_non_string_non_buffer_designators() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    for bad in [Value::fixnum(1), Value::NIL, Value::symbol("foo")] {
        let err = builtin_get_buffer(&mut eval, vec![bad])
            .expect_err("get-buffer should reject non-string/non-buffer args");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("stringp"), bad]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    let dead = create_unique_test_buffer(&mut eval, "*gb-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead]).unwrap();
    assert_eq!(builtin_get_buffer(&mut eval, vec![dead]).unwrap(), dead);
}

#[test]
fn generate_new_buffer_name_optional_arg_matches_expected_types() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let _ = builtin_get_buffer_create(&mut eval, vec![Value::string("*gnbn-opt*")]).unwrap();
    let _ = builtin_get_buffer_create(&mut eval, vec![Value::string("*gnbn-opt*<2>")]).unwrap();

    let with_nil =
        builtin_generate_new_buffer_name(&mut eval, vec![Value::string("*gnbn-opt*"), Value::NIL])
            .unwrap();
    let with_true =
        builtin_generate_new_buffer_name(&mut eval, vec![Value::string("*gnbn-opt*"), Value::T])
            .unwrap();
    let with_symbol = builtin_generate_new_buffer_name(
        &mut eval,
        vec![Value::string("*gnbn-opt*"), Value::symbol("ignored")],
    )
    .unwrap();
    let with_keyword = builtin_generate_new_buffer_name(
        &mut eval,
        vec![Value::string("*gnbn-opt*"), Value::keyword("ignored")],
    )
    .unwrap();
    let with_string = builtin_generate_new_buffer_name(
        &mut eval,
        vec![Value::string("*gnbn-opt*"), Value::string("*gnbn-opt*<2>")],
    )
    .unwrap();

    assert_eq!(with_nil, Value::string("*gnbn-opt*<3>"));
    assert_eq!(with_true, Value::string("*gnbn-opt*<3>"));
    assert_eq!(with_symbol, Value::string("*gnbn-opt*<3>"));
    assert_eq!(with_keyword, Value::string("*gnbn-opt*<3>"));
    assert_eq!(with_string, Value::string("*gnbn-opt*<2>"));

    let err = builtin_generate_new_buffer_name(
        &mut eval,
        vec![
            Value::string("*gnbn-opt*"),
            Value::list(vec![Value::fixnum(1)]),
        ],
    )
    .expect_err("generate-new-buffer-name should reject non string/symbol optional arg");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("stringp"),
                    Value::list(vec![Value::fixnum(1)])
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn buffer_size_and_modified_p_return_defaults_for_deleted_buffer_objects() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let dead_for_size = create_unique_test_buffer(&mut eval, "*bs-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead_for_size]).unwrap();
    let size = builtin_buffer_size(&mut eval, vec![dead_for_size]).unwrap();
    assert_eq!(size, Value::fixnum(0));

    let dead_for_modified = create_unique_test_buffer(&mut eval, "*bm-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead_for_modified]).unwrap();
    let modified = builtin_buffer_modified_p(&mut eval, vec![dead_for_modified]).unwrap();
    assert_eq!(modified, Value::NIL);
}

#[test]
fn buffer_size_counts_full_buffer_characters_ignoring_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    builtin_insert(&mut eval, vec![Value::string("aébc")]).unwrap();
    builtin_narrow_to_region(&mut eval, vec![Value::fixnum(2), Value::fixnum(3)]).unwrap();

    assert_eq!(
        builtin_buffer_size(&mut eval, vec![]).unwrap(),
        Value::fixnum(4)
    );
}

#[test]
fn buffer_base_buffer_and_last_name_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let base_id = eval.buffers.current_buffer().unwrap().id;
    let indirect_id = eval.buffers.create_buffer("*indirect*");
    eval.buffers.get_mut(indirect_id).unwrap().base_buffer = Some(base_id);

    assert_eq!(
        builtin_buffer_base_buffer(&mut eval, vec![]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_buffer_last_name(&mut eval, vec![]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_buffer_base_buffer(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_buffer_base_buffer(&mut eval, vec![Value::make_buffer(indirect_id)]).unwrap(),
        Value::make_buffer(base_id)
    );
    assert_eq!(
        builtin_buffer_last_name(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );

    let created = create_unique_test_buffer(&mut eval, "*bln-created*");
    let created_name = builtin_buffer_name(&mut eval, vec![created]).unwrap();
    assert_eq!(
        builtin_buffer_last_name(&mut eval, vec![created]).unwrap(),
        created_name,
        "GNU get-buffer-create initializes last_name to the buffer name"
    );

    let base_type = builtin_buffer_base_buffer(&mut eval, vec![Value::symbol("x")])
        .expect_err("buffer-base-buffer should reject non-buffer, non-nil optional arg");
    match base_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("bufferp"), Value::symbol("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let last_type = builtin_buffer_last_name(&mut eval, vec![Value::symbol("x")])
        .expect_err("buffer-last-name should reject non-buffer, non-nil optional arg");
    match last_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("bufferp"), Value::symbol("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let base_arity = builtin_buffer_base_buffer(&mut eval, vec![Value::NIL, Value::NIL])
        .expect_err("buffer-base-buffer should reject >1 args");
    match base_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("buffer-base-buffer"), Value::fixnum(2)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let last_arity = builtin_buffer_last_name(&mut eval, vec![Value::NIL, Value::NIL])
        .expect_err("buffer-last-name should reject >1 args");
    match last_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("buffer-last-name"), Value::fixnum(2)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let dead = create_unique_test_buffer(&mut eval, "*bln-dead*");
    let live_name = builtin_buffer_name(&mut eval, vec![dead]).unwrap();
    let _ = builtin_kill_buffer(&mut eval, vec![dead]).unwrap();

    assert_eq!(
        builtin_buffer_base_buffer(&mut eval, vec![dead]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_buffer_last_name(&mut eval, vec![dead]).unwrap(),
        live_name
    );
}

#[test]
fn make_indirect_buffer_shares_text_and_flattens_base_buffer_chain() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let base = create_unique_test_buffer(&mut eval, "*mib-base*");
    if !base.is_buffer() {
        panic!("expected buffer object");
    };

    let _ = builtin_set_buffer(&mut eval, vec![base]).unwrap();
    builtin_insert(&mut eval, vec![Value::string("abcd")]).unwrap();

    let indirect =
        builtin_make_indirect_buffer(&mut eval, vec![base, Value::string("*mib-indirect*")])
            .expect("make-indirect-buffer should create a buffer");
    if !indirect.is_buffer() {
        panic!("expected buffer object");
    };
    let base_id = base.as_buffer_id().unwrap();
    let indirect_id = indirect.as_buffer_id().unwrap();

    assert_eq!(
        builtin_buffer_base_buffer(&mut eval, vec![indirect]).unwrap(),
        base
    );
    assert!(
        eval.buffers
            .get(base_id)
            .unwrap()
            .shares_text_storage_with(eval.buffers.get(indirect_id).unwrap())
    );

    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(base_id, crate::buffer::EmacsBytePos::new(0));
    let _ = eval.buffers.insert_into_buffer(base_id, "zz");
    assert_eq!(
        eval.buffers.get(indirect_id).unwrap().buffer_string(),
        "zzabcd"
    );

    let second =
        builtin_make_indirect_buffer(&mut eval, vec![indirect, Value::string("*mib-indirect-2*")])
            .expect("second indirect buffer should be created");
    assert_eq!(
        builtin_buffer_base_buffer(&mut eval, vec![second]).unwrap(),
        base
    );
}

#[test]
fn make_indirect_buffer_rejects_duplicate_and_empty_names() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let base = create_unique_test_buffer(&mut eval, "*mib-errors-base*");

    let duplicate = builtin_make_indirect_buffer(&mut eval, vec![base, Value::string("*scratch*")])
        .expect_err("duplicate indirect name should error");
    match duplicate {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("unexpected flow: {other:?}"),
    }

    let empty = builtin_make_indirect_buffer(&mut eval, vec![base, Value::string("")])
        .expect_err("empty indirect name should error");
    match empty {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn make_indirect_buffer_clone_and_hook_semantics_follow_buffer_c() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let base = create_unique_test_buffer(&mut eval, "*mib-clone-base*");
    if !base.is_buffer() {
        panic!("expected buffer object");
    };
    let base_id = base.as_buffer_id().unwrap();

    let _ = builtin_set_buffer(&mut eval, vec![base]).unwrap();
    let _ =
        eval.buffers
            .set_buffer_local_property(base_id, "major-mode", Value::symbol("neo-mode"));
    let _ = eval
        .buffers
        .set_buffer_local_property(base_id, "mode-name", Value::string("Neo"));

    install_noarg_hook_probe(
        &mut eval,
        "mib-clone-hook",
        vec![Value::list(vec![
            Value::symbol("setq"),
            Value::symbol("mib-last-clone-buffer"),
            Value::list(vec![Value::symbol("buffer-name")]),
        ])],
    );
    install_noarg_hook_probe(
        &mut eval,
        "mib-buffer-list-hook",
        vec![Value::list(vec![
            Value::symbol("setq"),
            Value::symbol("mib-buffer-list-ran"),
            Value::symbol("t"),
        ])],
    );
    eval.obarray_mut().set_symbol_value(
        "clone-indirect-buffer-hook",
        Value::list(vec![Value::symbol("mib-clone-hook")]),
    );
    eval.obarray_mut().set_symbol_value(
        "buffer-list-update-hook",
        Value::list(vec![Value::symbol("mib-buffer-list-hook")]),
    );
    eval.obarray_mut()
        .set_symbol_value("mib-last-clone-buffer", Value::NIL);
    eval.obarray_mut()
        .set_symbol_value("mib-buffer-list-ran", Value::NIL);

    let cloned = builtin_make_indirect_buffer(
        &mut eval,
        vec![base, Value::string("*mib-clone*"), Value::T],
    )
    .expect("clone indirect buffer");
    if !cloned.is_buffer() {
        panic!("expected buffer object");
    };
    let cloned_id = cloned.as_buffer_id().unwrap();

    assert_eq!(
        eval.buffers
            .get(cloned_id)
            .and_then(|buf| buf.get_buffer_local("major-mode")),
        Some(Value::symbol("neo-mode"))
    );
    assert_eq!(
        builtin_buffer_last_name(&mut eval, vec![cloned]).unwrap(),
        builtin_buffer_last_name(&mut eval, vec![base]).unwrap(),
        "GNU clone_per_buffer_values copies base last_name into cloned indirect buffers"
    );
    assert_eq!(
        eval.buffers.current_buffer_id(),
        Some(base_id),
        "make-indirect-buffer should restore the previous current buffer"
    );
    assert_eq!(
        eval.obarray()
            .symbol_value("mib-last-clone-buffer")
            .and_then(|v| v.as_utf8_str()),
        Some("*mib-clone*")
    );
    assert_eq!(
        eval.obarray().symbol_value("mib-buffer-list-ran"),
        Some(&Value::T)
    );

    eval.obarray_mut()
        .set_symbol_value("mib-last-clone-buffer", Value::NIL);
    eval.obarray_mut()
        .set_symbol_value("mib-buffer-list-ran", Value::NIL);

    let _ = builtin_make_indirect_buffer(
        &mut eval,
        vec![
            base,
            Value::string("*mib-clone-inhibit*"),
            Value::T,
            Value::T,
        ],
    )
    .expect("clone indirect buffer with inhibited buffer hooks");

    assert_eq!(
        eval.obarray()
            .symbol_value("mib-last-clone-buffer")
            .and_then(|v| v.as_utf8_str()),
        Some("*mib-clone-inhibit*"),
        "clone-indirect-buffer-hook should still run"
    );
    assert_eq!(
        eval.obarray().symbol_value("mib-buffer-list-ran"),
        Some(&Value::NIL),
        "buffer-list-update-hook should be inhibited"
    );
}

#[test]
fn make_indirect_buffer_clone_copies_lisp_local_var_cells() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let base = create_unique_test_buffer(&mut eval, "*mib-local-clone-base*");
    let base_id = base.as_buffer_id().expect("expected buffer object");
    let sym = intern("mib-cloned-local");

    eval.set_buffer_local_binding_by_id(base_id, sym, Value::fixnum(1))
        .expect("install base buffer-local binding");

    let cloned = builtin_make_indirect_buffer(
        &mut eval,
        vec![base, Value::string("*mib-local-clone*"), Value::T],
    )
    .expect("clone indirect buffer");
    let cloned_id = cloned
        .as_buffer_id()
        .expect("expected cloned buffer object");

    eval.set_buffer_local_binding_by_id(cloned_id, sym, Value::fixnum(2))
        .expect("update cloned buffer-local binding");

    assert_eq!(
        eval.buffers
            .get(base_id)
            .and_then(|buf| buf.buffer_local_value("mib-cloned-local")),
        Some(Value::fixnum(1)),
        "GNU clone_per_buffer_values allocates fresh local_var_alist cells for the indirect buffer"
    );
    assert_eq!(
        eval.buffers
            .get(cloned_id)
            .and_then(|buf| buf.buffer_local_value("mib-cloned-local")),
        Some(Value::fixnum(2))
    );
}

#[test]
fn make_indirect_buffer_clone_nil_resets_buffer_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let base = create_unique_test_buffer(&mut eval, "*mib-clone-nil-base*");
    if !base.is_buffer() {
        panic!("expected buffer object");
    };
    let base_id = base.as_buffer_id().unwrap();

    let _ = builtin_set_buffer(&mut eval, vec![base]).unwrap();
    let _ =
        eval.buffers
            .set_buffer_local_property(base_id, "major-mode", Value::symbol("neo-mode"));
    let _ = eval
        .buffers
        .set_buffer_local_property(base_id, "mode-name", Value::string("Neo"));

    let indirect =
        builtin_make_indirect_buffer(&mut eval, vec![base, Value::string("*mib-default*")])
            .expect("indirect buffer without clone");
    if !indirect.is_buffer() {
        panic!("expected buffer object");
    };
    let indirect_id = indirect.as_buffer_id().unwrap();

    assert_eq!(
        builtin_buffer_last_name(&mut eval, vec![indirect]).unwrap(),
        Value::string("*mib-default*"),
        "GNU non-clone make-indirect-buffer keeps the new buffer name as last_name"
    );
    let indirect_buf = eval.buffers.get(indirect_id).expect("indirect buffer");
    assert_eq!(
        indirect_buf.get_buffer_local("major-mode"),
        Some(Value::symbol("fundamental-mode"))
    );
    assert_eq!(
        indirect_buf
            .get_buffer_local("mode-name")
            .and_then(|v| v.as_utf8_str()),
        Some("Fundamental")
    );
}

#[test]
fn buffer_modified_tick_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    assert_eq!(
        builtin_buffer_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_buffer_chars_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(1)
    );

    builtin_insert(&mut eval, vec![Value::string("abcdef")]).unwrap();
    assert_eq!(
        builtin_buffer_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(4)
    );
    assert_eq!(
        builtin_buffer_chars_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(4)
    );

    builtin_set_buffer_modified_p(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(
        builtin_buffer_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(4)
    );
    assert_eq!(
        builtin_buffer_chars_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(4)
    );

    builtin_delete_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();
    assert_eq!(
        builtin_buffer_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(7)
    );
    assert_eq!(
        builtin_buffer_chars_modified_tick(&mut eval, vec![]).unwrap(),
        Value::fixnum(7)
    );

    let dead = create_unique_test_buffer(&mut eval, "*ticks-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead]).unwrap();
    assert_eq!(
        builtin_buffer_modified_tick(&mut eval, vec![dead]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_buffer_chars_modified_tick(&mut eval, vec![dead]).unwrap(),
        Value::fixnum(1)
    );

    let type_error = builtin_buffer_modified_tick(&mut eval, vec![Value::symbol("x")])
        .expect_err("buffer-modified-tick should reject non-buffer optional arg");
    match type_error {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("bufferp"), Value::symbol("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let arity_error = builtin_buffer_chars_modified_tick(&mut eval, vec![Value::NIL, Value::NIL])
        .expect_err("buffer-chars-modified-tick should reject >1 args");
    match arity_error {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("buffer-chars-modified-tick"),
                    Value::fixnum(2)
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn subst_char_in_region_replaces_chars_in_accessible_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("hello world hello")]).unwrap();

    builtin_subst_char_in_region(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(18),
            Value::fixnum('l' as i64),
            Value::fixnum('L' as i64),
        ],
    )
    .expect("subst-char-in-region should succeed");

    assert_eq!(
        builtin_buffer_string(&mut eval, vec![])
            .unwrap()
            .as_utf8_str(),
        Some("heLLo worLd heLLo")
    );
}

/// A same-length substitution (overwrite-mode / subst-char-in-region) must feed
/// the unchanged-region accumulator a dirty span, so the localized-edit fast
/// path applies instead of a full rebuild (Phase 3 same-len hook).
#[test]
fn subst_char_in_region_records_unchanged_region_dirty_span() {
    let mut eval = super::super::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("hello world hello")]).unwrap();

    // Ack the initial insert's dirty span so we measure only the substitution.
    eval.buffers
        .current_buffer()
        .unwrap()
        .reset_unchanged_region();
    assert_eq!(
        eval.buffers.current_buffer().unwrap().changed_char_range(),
        None
    );

    builtin_subst_char_in_region(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(18),
            Value::fixnum('l' as i64),
            Value::fixnum('L' as i64),
        ],
    )
    .expect("subst-char-in-region should succeed");

    let dirty = eval.buffers.current_buffer().unwrap().changed_char_range();
    assert!(
        dirty.is_some(),
        "same-len subst records a dirty range (got {dirty:?})"
    );
    let (start, end) = dirty.unwrap();
    assert!(start < end, "non-empty dirty span (got {start}..{end})");
}

#[test]
fn subst_char_in_region_noundo_marks_buffer_modified_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("a\nb\n")]).unwrap();
    builtin_set_buffer_modified_p(&mut eval, vec![Value::NIL]).unwrap();

    builtin_subst_char_in_region(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(5),
            Value::fixnum('\n' as i64),
            Value::fixnum(' ' as i64),
            Value::T,
        ],
    )
    .expect("subst-char-in-region with NOUNDO should succeed");

    assert_eq!(
        builtin_buffer_string(&mut eval, vec![])
            .unwrap()
            .as_utf8_str(),
        Some("a b ")
    );
    assert_eq!(
        builtin_buffer_modified_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
}

#[test]
fn subst_char_in_region_replaces_trailing_newline_with_marker_end() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval
        .eval_str_each(
            r#"(insert "a\nb\n")
           (let ((end (copy-marker (point-max) t)))
             (subst-char-in-region (point-min) end ?\n ?\s t)
             (buffer-string))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");

    assert_eq!(format_eval_result(&Ok(result)), r#"OK "a b ""#);
}

#[test]
fn subst_char_in_region_uses_live_marker_end_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval
        .eval_str_each(
            r#"(insert "a\n")
           (let ((end (copy-marker (point-max) t)))
             (goto-char (point-min))
             (insert " ")
             (subst-char-in-region (point-min) end ?\n ?\s t)
             (buffer-string))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");

    assert_eq!(format_eval_result(&Ok(result)), r#"OK " a ""#);
}

#[test]
fn subst_char_in_region_replaces_raw_byte_chars_in_multibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    builtin_insert_byte(&mut eval, vec![Value::fixnum(200), Value::fixnum(2)]).unwrap();
    builtin_subst_char_in_region(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::fixnum(0x3FFF00 + 200),
            Value::fixnum(0x3FFF00 + 201),
        ],
    )
    .expect("subst-char-in-region should replace raw-byte chars");

    let result = builtin_buffer_string(&mut eval, vec![]).unwrap();
    assert_eq!(
        decode_value_char_codes(&result),
        vec![0x3FFF00 + 201, 0x3FFF00 + 201]
    );
}

#[test]
fn subst_char_in_region_replaces_nonunicode_emacs_chars() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    builtin_insert(
        &mut eval,
        vec![Value::fixnum(0x11_0000), Value::fixnum(0x11_0000)],
    )
    .unwrap();
    builtin_subst_char_in_region(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::fixnum(0x11_0000),
            Value::fixnum(0x12_0000),
        ],
    )
    .expect("subst-char-in-region should replace nonunicode chars");

    let result = builtin_buffer_string(&mut eval, vec![]).unwrap();
    assert_eq!(decode_value_char_codes(&result), vec![0x12_0000, 0x12_0000]);
}

#[test]
fn goto_char_uses_live_marker_position_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval
        .eval_str_each(
            r#"(insert "ab")
           (let ((m (copy-marker (point-max) t)))
             (goto-char (point-min))
             (insert "X")
             (goto-char m)
             (point))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");

    assert_eq!(format_eval_result(&Ok(result)), "OK 4");
}

#[test]
fn buffer_substring_accepts_marker_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval
        .eval_str(
            r#"(progn
                 (insert "abcdef")
                 (let ((beg (copy-marker 2))
                       (end (copy-marker 5 t)))
                   (buffer-substring beg end)))"#,
        )
        .expect("evaluation succeeds");

    assert_eq!(format_eval_result(&Ok(result)), r#"OK "bcd""#);
}

#[test]
fn char_queries_use_live_marker_positions_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval
        .eval_str_each(
            r#"(insert "ab")
           (let ((m (copy-marker 2)))
             (goto-char 1)
             (insert "X")
             (list (marker-position m)
                   (char-after m)
                   (char-before m)))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");

    assert_eq!(format_eval_result(&Ok(result)), "OK (3 98 97)");
}

#[test]
fn search_forward_uses_live_marker_bound_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval
        .eval_str_each(
            r#"(insert "ab")
           (let ((end (copy-marker (point-max) t)))
             (goto-char (point-min))
             (insert "X")
             (goto-char (point-min))
             (list (search-forward "b" end t)
                   (point)
                   (marker-position end)))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");

    assert_eq!(format_eval_result(&Ok(result)), "OK (4 4 4)");
}

#[test]
fn subst_char_in_region_rejects_different_utf8_lengths() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("aa")]).unwrap();

    let err = builtin_subst_char_in_region(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::fixnum('a' as i64),
            Value::fixnum('ß' as i64),
        ],
    )
    .unwrap_err();

    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Characters in `subst-char-in-region' have different byte-lengths",
                )]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn insert_honors_inhibit_read_only_override() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.obarray.set_symbol_value("buffer-read-only", Value::T);
    eval.obarray.set_symbol_value("inhibit-read-only", Value::T);

    builtin_insert(&mut eval, vec![Value::string("ok")]).expect("insert should bypass read-only");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "ok");
    assert_eq!(buf.point_lisp_char_pos().as_i64(), 3);
}

#[test]
fn insert_inherit_variants_reuse_insert_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    assert_eq!(
        builtin_insert_and_inherit(
            &mut eval,
            vec![
                Value::string("a"),
                Value::char('b'),
                Value::fixnum('c' as i64)
            ],
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_insert_before_markers_and_inherit(&mut eval, vec![Value::string("d")]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_buffer_string(&mut eval, vec![])
            .unwrap()
            .as_utf8_str()
            .map(str::to_owned),
        Some("abcd".to_string())
    );

    let type_error =
        builtin_insert_and_inherit(&mut eval, vec![Value::list(vec![Value::fixnum(1)])])
            .expect_err("insert-and-inherit should reject non char/string values");
    match type_error {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("char-or-string-p"),
                    Value::list(vec![Value::fixnum(1)])
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn insert_copies_string_text_properties_into_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let text = Value::string("xy");
    assert!(text.is_string(), "expected string value");

    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        2,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(text, table);

    assert_eq!(builtin_insert(&mut eval, vec![text]).unwrap(), Value::NIL);

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "xy");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("face")
        ),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(1),
            Value::symbol("face")
        ),
        Some(Value::symbol("bold"))
    );
}

#[test]
fn insert_before_markers_copies_string_text_properties_into_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let text = Value::string("xy");
    assert!(text.is_string(), "expected string value");

    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        1,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_string_property(
        &mut table,
        1,
        2,
        Value::symbol("help-echo"),
        Value::string("tip"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(text, table);

    assert_eq!(
        builtin_insert_before_markers(&mut eval, vec![text]).unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "xy");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("face")
        ),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(1),
            Value::symbol("help-echo")
        ),
        Some(Value::string("tip"))
    );
}

#[test]
fn insert_and_inherit_copies_previous_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("ab");
        buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(1, 2),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
    }

    assert_eq!(
        builtin_insert_and_inherit(&mut eval, vec![Value::string("X")]).unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "abX");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(2),
            Value::symbol("face")
        ),
        Some(Value::symbol("bold"))
    );
}

#[test]
fn plain_insert_does_not_inherit_spanning_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("ab");
        buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(0, 2),
            Value::symbol("foo"),
            Value::symbol("bar"),
        );
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    }

    assert_eq!(
        builtin_insert(&mut eval, vec![Value::string("X")]).unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "aXb");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("foo")
        ),
        Some(Value::symbol("bar"))
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(1),
            Value::symbol("foo")
        ),
        None
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(2),
            Value::symbol("foo")
        ),
        Some(Value::symbol("bar"))
    );
}

#[test]
fn insert_inherits_left_multibyte_text_property_at_char_boundary() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("é");
        buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(0, 2),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
    }

    assert_eq!(
        builtin_insert_and_inherit(&mut eval, vec![Value::string("X")]).unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "éX");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(2),
            Value::symbol("face")
        ),
        Some(Value::symbol("bold"))
    );
}

#[test]
fn insert_char_nil_count_defaults_to_one_and_can_inherit_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("ab");
        buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(1, 2),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
    }

    assert_eq!(
        builtin_insert_char(
            &mut eval,
            vec![Value::fixnum('X' as i64), Value::NIL, Value::T],
        )
        .unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "abX");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(2),
            Value::symbol("face")
        ),
        Some(Value::symbol("bold"))
    );
}

#[test]
fn insert_and_inherit_copies_string_properties_then_inherits_overlapping_names() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("a")]).unwrap();
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    let text = Value::string("X");
    assert!(text.is_string(), "expected string value");
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        1,
        Value::symbol("face"),
        Value::symbol("italic"),
    );
    put_string_property(
        &mut table,
        0,
        1,
        Value::symbol("mouse-face"),
        Value::symbol("highlight"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(text, table);

    builtin_insert_and_inherit(&mut eval, vec![text]).unwrap();

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "aX");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(1),
            Value::symbol("face")
        ),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(1),
            Value::symbol("mouse-face")
        ),
        Some(Value::symbol("highlight"))
    );
}

#[test]
fn delete_all_overlays_clears_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("hello");
    }
    builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)])
        .expect("first overlay should be created");
    builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(5)])
        .expect("second overlay should be created");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.overlays.len(), 2);

    assert_eq!(
        builtin_delete_all_overlays(&mut eval, vec![]).unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.overlays.len(), 0);
}

#[test]
fn insert_buffer_substring_inserts_source_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let source_id = eval.buffers.create_buffer("*ibs-source*");
    eval.buffers.set_current(source_id);
    builtin_insert(&mut eval, vec![Value::string("abcdef")]).unwrap();

    let dest_id = eval.buffers.create_buffer("*ibs-dest*");
    eval.buffers.set_current(dest_id);
    builtin_insert(&mut eval, vec![Value::string("start:")]).unwrap();

    assert_eq!(
        builtin_insert_buffer_substring(
            &mut eval,
            vec![
                Value::make_buffer(source_id),
                Value::fixnum(2),
                Value::fixnum(5)
            ],
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        eval.buffers
            .get(dest_id)
            .expect("destination buffer should exist")
            .buffer_string(),
        "start:bcd"
    );

    let bad_designator = builtin_insert_buffer_substring(&mut eval, vec![Value::fixnum(9)])
        .expect_err("insert-buffer-substring should reject non-buffer designators");
    match bad_designator {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(9)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let bad_start = builtin_insert_buffer_substring(
        &mut eval,
        vec![Value::make_buffer(source_id), Value::string("x")],
    )
    .expect_err("insert-buffer-substring should reject non integer-or-marker START");
    match bad_start {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::string("x")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn insert_buffer_substring_defaults_to_source_accessible_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let source_id = eval.buffers.create_buffer("*ibs-source-defaults*");
    eval.buffers.set_current(source_id);
    builtin_insert(&mut eval, vec![Value::string("abcdef")]).unwrap();
    let _ = eval
        .buffers
        .narrow_buffer_to_emacs_byte_range(source_id, EmacsByteRange::from_usize(1, 4));

    let dest_id = eval.buffers.create_buffer("*ibs-dest-defaults*");
    eval.buffers.set_current(dest_id);

    assert_eq!(
        builtin_insert_buffer_substring(&mut eval, vec![Value::make_buffer(source_id)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        eval.buffers
            .get(dest_id)
            .expect("destination buffer should exist")
            .buffer_string(),
        "bcd"
    );
}

#[test]
fn insert_buffer_substring_signals_when_bounds_escape_source_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let source_id = eval.buffers.create_buffer("*ibs-source-range*");
    eval.buffers.set_current(source_id);
    builtin_insert(&mut eval, vec![Value::string("abcdef")]).unwrap();
    let _ = eval
        .buffers
        .narrow_buffer_to_emacs_byte_range(source_id, EmacsByteRange::from_usize(1, 4));

    let err = builtin_insert_buffer_substring(
        &mut eval,
        vec![Value::make_buffer(source_id), Value::fixnum(1)],
    )
    .expect_err("insert-buffer-substring should reject out-of-range narrowed START");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(1), Value::fixnum(5)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn insert_buffer_substring_rejects_deleted_buffer_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let dead = create_unique_test_buffer(&mut eval, "*ibs-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead]).unwrap();

    let err = builtin_insert_buffer_substring(&mut eval, vec![dead])
        .expect_err("insert-buffer-substring should reject deleted buffer objects");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Selecting deleted buffer")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn kill_all_local_variables_clears_buffer_locals() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().unwrap();
        buf.set_buffer_local("tab-width", Value::fixnum(8));
        buf.set_buffer_local("fill-column", Value::fixnum(80));
        buf.set_buffer_local("major-mode", Value::symbol("neo-mode"));
        buf.set_buffer_local("mode-name", Value::string("Neo"));
        buf.set_buffer_local("buffer-undo-list", Value::T);
    }
    let _ = eval
        .buffers
        .set_current_local_map(crate::emacs_core::keymap::make_sparse_list_keymap());

    assert_eq!(
        builtin_kill_all_local_variables(&mut eval, vec![]).unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().unwrap();
    assert!(buf.get_buffer_local("tab-width").is_none());
    assert!(buf.get_buffer_local("fill-column").is_none());
    // buffer-read-only is a BUFFER_OBJFWD-style slot now: it always
    // resolves through the slot, never goes void, and starts at nil.
    assert_eq!(buf.get_read_only(), false);
    assert_eq!(
        buf.get_buffer_local("major-mode"),
        Some(Value::symbol("fundamental-mode"))
    );
    assert_eq!(
        buf.get_buffer_local("mode-name"),
        Some(Value::string("Fundamental"))
    );
    assert_eq!(buf.get_buffer_local("buffer-undo-list"), Some(Value::T));
    assert!(eval.buffers.current_local_map().is_nil());
}

#[test]
fn kill_all_local_variables_crosses_the_global_mode_line_menu_boundary_while_offscreen() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let visible = eval.buffers.create_buffer("kill-locals-visible");
    eval.frames.create_frame("F1", 800, 600, visible);
    let offscreen = eval.buffers.create_buffer("kill-locals-offscreen");
    eval.buffers.set_current(offscreen);

    let initial = eval.menu_bar_rebuild_generation();
    builtin_kill_all_local_variables(&mut eval, vec![])
        .expect("kill-all-local-variables should reset the offscreen buffer");

    // Every GNU major-mode function enters through this primitive. Its final
    // `bset_update_mode_line` raises global `update_mode_lines` regardless of
    // whether CURRENT-BUFFER is displayed (src/buffer.c:3019-3048).
    assert_ne!(eval.menu_bar_rebuild_generation(), initial);
}

#[test]
fn ntake_destructively_truncates_lists() {
    crate::test_utils::init_test_tracing();
    let list = Value::list(vec![
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
        Value::fixnum(4),
    ]);
    let kept = builtin_ntake(vec![Value::fixnum(2), list]).unwrap();
    assert_eq!(kept, Value::list(vec![Value::fixnum(1), Value::fixnum(2)]));
    assert_eq!(
        list_to_vec(&list).expect("list should stay proper after ntake"),
        vec![Value::fixnum(1), Value::fixnum(2)]
    );

    let unchanged = Value::list(vec![Value::fixnum(5), Value::fixnum(6)]);
    assert_eq!(
        builtin_ntake(vec![Value::fixnum(10), unchanged]).unwrap(),
        unchanged
    );
    assert_eq!(
        builtin_ntake(vec![Value::fixnum(0), list]).unwrap(),
        Value::NIL
    );

    let type_error = builtin_ntake(vec![Value::fixnum(1), Value::fixnum(3)])
        .expect_err("ntake should reject non-list arguments");
    match type_error {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::fixnum(3)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn kill_all_local_variables_preserves_partial_permanent_local_hooks() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.create_buffer("*kill-all-local-hook*");
    eval.buffers.set_current(buf_id);
    eval.obarray
        .put_property(
            "compat-mixed-hook",
            "permanent-local",
            Value::symbol("permanent-local-hook"),
        )
        .unwrap();
    eval.obarray
        .put_property("compat--keep-hook", "permanent-local-hook", Value::T)
        .unwrap();
    {
        let buf = eval.buffers.current_buffer_mut().unwrap();
        buf.set_buffer_local(
            "compat-mixed-hook",
            Value::list(vec![
                Value::symbol("compat--drop-hook"),
                Value::symbol("compat--keep-hook"),
                Value::T,
            ]),
        );
    }

    assert_eq!(
        builtin_kill_all_local_variables(&mut eval, vec![]).unwrap(),
        Value::NIL
    );

    let buf = eval.buffers.current_buffer().unwrap();
    let hook = buf
        .get_buffer_local("compat-mixed-hook")
        .expect("partial permanent hook should remain local");
    let items =
        crate::emacs_core::value::list_to_vec(&hook).expect("hook value should stay a proper list");
    assert_eq!(items, vec![Value::symbol("compat--keep-hook"), Value::T]);
}

/// GNU `reset_buffer_local_variables' (`src/buffer.c:1168-1225') filters
/// `local_var_alist' in place with a `last' cursor: a permanent-local entry
/// advances the cursor, every other entry is spliced out with
/// `XSETCDR (last, XCDR (tmp))'. Retention therefore depends only on each
/// entry's own `permanent-local' property, never on its position.
///
/// `make-local-variable' prepends, so the LAST local created sits at the
/// alist HEAD. When that head entry is the permanent one, the filtered list
/// keeps the head cons cell it started with -- the splice happens entirely in
/// the interior. Any bookkeeping that infers "unchanged structure" from an
/// unchanged head cons goes stale exactly there, and the killed ordinary
/// locals keep answering `local-variable-p' with t.
#[test]
fn kill_all_local_variables_drops_ordinary_locals_under_a_head_permanent_local() {
    crate::test_utils::init_test_tracing();

    // Each case is (source, expected). Expectations are GNU 31.0.90
    // `emacs --batch -Q' output.
    let cases: &[(&str, &str, &str)] = &[
        (
            "permanent local created LAST (alist head)",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-a*"))
                 (put 'pa 'permanent-local t)
                 (set (make-local-variable 'na) 1)
                 (set (make-local-variable 'pa) 2)
                 (kill-all-local-variables)
                 (list (local-variable-p 'na) (local-variable-p 'pa)))"#,
            "OK (nil t)",
        ),
        (
            "permanent local in the middle",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-b*"))
                 (put 'pb 'permanent-local t)
                 (set (make-local-variable 'nb1) 1)
                 (set (make-local-variable 'pb) 2)
                 (set (make-local-variable 'nb2) 3)
                 (kill-all-local-variables)
                 (list (local-variable-p 'nb1)
                       (local-variable-p 'pb)
                       (local-variable-p 'nb2)))"#,
            "OK (nil t nil)",
        ),
        (
            "permanent local created FIRST (alist tail)",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-c*"))
                 (put 'pc 'permanent-local t)
                 (set (make-local-variable 'pc) 2)
                 (set (make-local-variable 'nc) 1)
                 (kill-all-local-variables)
                 (list (local-variable-p 'pc) (local-variable-p 'nc)))"#,
            "OK (t nil)",
        ),
        (
            "two ordinary locals behind a head permanent local",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-d*"))
                 (put 'pd 'permanent-local t)
                 (set (make-local-variable 'nd1) 1)
                 (set (make-local-variable 'nd2) 2)
                 (set (make-local-variable 'pd) 3)
                 (kill-all-local-variables)
                 (list (local-variable-p 'nd1)
                       (local-variable-p 'nd2)
                       (local-variable-p 'pd)))"#,
            "OK (nil nil t)",
        ),
        (
            "killed ordinary local is gone from buffer-local-variables too",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-e*"))
                 (put 'pe 'permanent-local t)
                 (set (make-local-variable 'ne) 1)
                 (set (make-local-variable 'pe) 2)
                 (kill-all-local-variables)
                 (list (assq 'ne (buffer-local-variables))
                       (cdr (assq 'pe (buffer-local-variables)))))"#,
            "OK (nil 2)",
        ),
        (
            "killed ordinary local also stops resolving through the BLV cache",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-f*"))
                 (put 'pf 'permanent-local t)
                 (set (make-local-variable 'nf) 1)
                 (set (make-local-variable 'pf) 2)
                 (kill-all-local-variables)
                 (list (local-variable-p 'nf) (local-variable-p 'pf)
                       (boundp 'nf) (boundp 'pf) pf))"#,
            "OK (nil t nil t 2)",
        ),
        // Retention depends only on each entry's OWN property, so re-`set'ing
        // an already-local permanent does NOT move it to the head: the head
        // here is the ordinary `gn3', it is dropped, and this case was already
        // correct before the fix. "Permanent-local written last" is not the
        // trigger; "permanent-local at the alist head" is.
        (
            "permanent local re-set last is still not the head",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-g*"))
                 (put 'gp1 'permanent-local t)
                 (put 'gp2 'permanent-local t)
                 (set (make-local-variable 'gn1) 1)
                 (set (make-local-variable 'gp1) 2)
                 (set (make-local-variable 'gn2) 3)
                 (set (make-local-variable 'gp2) 4)
                 (set (make-local-variable 'gn3) 5)
                 (set 'gp1 6)
                 (kill-all-local-variables)
                 (list (local-variable-p 'gn1) (local-variable-p 'gp1)
                       (local-variable-p 'gn2) (local-variable-p 'gp2)
                       (local-variable-p 'gn3)))"#,
            "OK (nil t nil t nil)",
        ),
        (
            "void buffer-local behind a head permanent local",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-h*"))
                 (put 'hp 'permanent-local t)
                 (make-local-variable 'hn)
                 (set (make-local-variable 'hp) 2)
                 (kill-all-local-variables)
                 (list (local-variable-p 'hn) (local-variable-p 'hp)))"#,
            "OK (nil t)",
        ),
        // The partial-preserve filter rewrites the retained entry's cdr in
        // place, which leaves the head cons identical for a second reason.
        (
            "permanent-local-hook at the head still drops the ordinary local",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-i*"))
                 (put 'ih 'permanent-local 'permanent-local-hook)
                 (put 'ikeep 'permanent-local-hook t)
                 (set (make-local-variable 'in1) 1)
                 (set (make-local-variable 'ih) (list 'idrop 'ikeep t))
                 (kill-all-local-variables)
                 (list (local-variable-p 'in1) (local-variable-p 'ih) ih))"#,
            "OK (nil t (ikeep t))",
        ),
        (
            "every local permanent, nothing unlinked",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-j*"))
                 (put 'jp1 'permanent-local t)
                 (put 'jp2 'permanent-local t)
                 (set (make-local-variable 'jp1) 1)
                 (set (make-local-variable 'jp2) 2)
                 (kill-all-local-variables)
                 (list (local-variable-p 'jp1) (local-variable-p 'jp2)))"#,
            "OK (t t)",
        ),
        // A second kill does not launder the first: both keep the same
        // permanent head, so nothing invalidates in between.
        (
            "two kills in a row with the same permanent head",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-k*"))
                 (put 'kp 'permanent-local t)
                 (set (make-local-variable 'kn) 1)
                 (set (make-local-variable 'kp) 2)
                 (kill-all-local-variables)
                 (kill-all-local-variables)
                 (list (local-variable-p 'kn) (local-variable-p 'kp)))"#,
            "OK (nil t)",
        ),
        (
            "local created after the kill, permanent head",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-l*"))
                 (put 'lp 'permanent-local t)
                 (set (make-local-variable 'ln1) 1)
                 (set (make-local-variable 'lp) 2)
                 (kill-all-local-variables)
                 (set (make-local-variable 'ln2) 9)
                 (list (local-variable-p 'ln1) (local-variable-p 'lp)
                       (local-variable-p 'ln2) ln2))"#,
            "OK (nil t t 9)",
        ),
        // The sharpest statement of the defect: two readers of the same
        // buffer, in one form. `buffer-local-variables' walks the alist and
        // `local-variable-p' goes through the derived index, and they must
        // never disagree.
        (
            "alist reader and index reader must agree",
            r#"(progn
                 (set-buffer (get-buffer-create " *l213-m*"))
                 (put 'mp 'permanent-local t)
                 (set (make-local-variable 'mn1) 1)
                 (set (make-local-variable 'mn2) 2)
                 (set (make-local-variable 'mp) 3)
                 (kill-all-local-variables)
                 (list (and (assq 'mn1 (buffer-local-variables)) t)
                       (local-variable-p 'mn1)
                       (and (assq 'mn2 (buffer-local-variables)) t)
                       (local-variable-p 'mn2)
                       (and (assq 'mp (buffer-local-variables)) t)
                       (local-variable-p 'mp)))"#,
            "OK (nil nil nil nil t t)",
        ),
    ];

    let mut failures = Vec::new();
    for (label, source, expected) in cases {
        let mut eval = crate::emacs_core::eval::Context::new();
        let actual = format_eval_result(&eval.eval_str(source));
        if actual != *expected {
            failures.push(format!("  {label}: GNU {expected}, neomacs {actual}"));
        }
    }
    assert!(
        failures.is_empty(),
        "kill-all-local-variables must retain exactly the permanent locals:\n{}",
        failures.join("\n")
    );
}

#[test]
fn replace_buffer_contents_and_set_buffer_multibyte_runtime_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let source_id = eval.buffers.create_buffer("*rbc-source*");
    eval.buffers.set_current(source_id);
    builtin_insert(&mut eval, vec![Value::string("source-text")]).unwrap();

    let dest_id = eval.buffers.create_buffer("*rbc-dest*");
    eval.buffers.set_current(dest_id);
    builtin_insert(&mut eval, vec![Value::string("dest-text")]).unwrap();

    assert_eq!(
        builtin_replace_buffer_contents(&mut eval, vec![Value::make_buffer(source_id)]).unwrap(),
        Value::T
    );
    assert_eq!(
        eval.buffers
            .get(dest_id)
            .expect("destination buffer should exist")
            .buffer_string(),
        "source-text"
    );

    assert_eq!(
        builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );
    assert!(!eval.buffers.current_buffer().unwrap().get_multibyte());

    assert_eq!(
        builtin_set_buffer_multibyte(&mut eval, vec![Value::symbol("foo")]).unwrap(),
        Value::symbol("foo")
    );
    assert!(eval.buffers.current_buffer().unwrap().get_multibyte());
}

#[test]
fn replace_buffer_contents_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let source_id = eval.buffers.create_buffer("*rbc-raw-source*");
    {
        let source = eval
            .buffers
            .get_mut(source_id)
            .expect("source buffer should exist");
        source.set_multibyte_value(false);
    }
    eval.buffers
        .insert_lisp_string_into_buffer(
            source_id,
            &crate::heap_types::LispString::from_unibyte(vec![0xFF]),
        )
        .expect("raw source insert should succeed");

    let dest_id = eval.buffers.create_buffer("*rbc-raw-dest*");
    {
        let dest = eval
            .buffers
            .get_mut(dest_id)
            .expect("destination buffer should exist");
        dest.set_multibyte_value(false);
    }
    eval.buffers.set_current(dest_id);

    assert_eq!(
        builtin_replace_buffer_contents(&mut eval, vec![Value::make_buffer(source_id)]).unwrap(),
        Value::T
    );

    let dest = eval
        .buffers
        .get(dest_id)
        .expect("destination buffer should exist");
    let text = dest.buffer_substring_lisp_string_range(dest.accessible_emacs_byte_range());
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn buffer_swap_text_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let first_id = eval.buffers.create_buffer("*swap-raw-a*");
    let second_id = eval.buffers.create_buffer("*swap-raw-b*");

    {
        let first = eval
            .buffers
            .get_mut(first_id)
            .expect("first buffer should exist");
        first.set_multibyte_value(false);
    }
    {
        let second = eval
            .buffers
            .get_mut(second_id)
            .expect("second buffer should exist");
        second.set_multibyte_value(false);
    }

    eval.buffers
        .insert_lisp_string_into_buffer(
            first_id,
            &crate::heap_types::LispString::from_unibyte(vec![0xFE]),
        )
        .expect("first raw insert should succeed");
    eval.buffers
        .insert_lisp_string_into_buffer(
            second_id,
            &crate::heap_types::LispString::from_unibyte(vec![0xFF]),
        )
        .expect("second raw insert should succeed");

    eval.buffers.set_current(first_id);
    assert_eq!(
        builtin_buffer_swap_text(&mut eval, vec![Value::make_buffer(second_id)]).unwrap(),
        Value::NIL
    );

    let first = eval
        .buffers
        .get(first_id)
        .expect("first buffer should exist");
    let second = eval
        .buffers
        .get(second_id)
        .expect("second buffer should exist");
    let first_text = first.buffer_substring_lisp_string_range(first.accessible_emacs_byte_range());
    let second_text =
        second.buffer_substring_lisp_string_range(second.accessible_emacs_byte_range());
    assert_eq!(first_text.as_bytes(), &[0xFF]);
    assert_eq!(second_text.as_bytes(), &[0xFE]);
    assert!(!first_text.is_multibyte());
    assert!(!second_text.is_multibyte());
}

fn buffer_swap_text_signal_message(result: EvalResult) -> String {
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            sig.data[0]
                .as_runtime_string_owned()
                .expect("error signal should carry a message")
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

fn full_buffer_bytes(buffer: &crate::buffer::Buffer) -> Vec<u8> {
    buffer
        .buffer_substring_lisp_string_range(buffer.full_emacs_byte_range())
        .as_bytes()
        .to_vec()
}

#[test]
fn buffer_swap_text_swaps_owned_backend_and_state() {
    crate::test_utils::init_test_tracing();
    for backend_kind in crate::buffer::BufferTextBackendKind::non_gap_implemented_variants() {
        let mut eval = super::super::eval::Context::new();
        let first_id = eval.buffers.create_buffer("*swap-state-a*");
        let second_id = eval.buffers.create_buffer("*swap-state-b*");

        eval.buffers
            .insert_into_buffer(first_id, "abcde")
            .expect("first insert should succeed");
        eval.buffers
            .insert_into_buffer(second_id, "uvwxyz")
            .expect("second insert should succeed");

        {
            let second = eval
                .buffers
                .get_mut(second_id)
                .expect("second buffer should exist");
            second.convert_text_backend_kind(
                backend_kind
                    .implemented()
                    .expect("test backend should be implemented"),
            );
        }

        eval.buffers
            .narrow_buffer_to_emacs_byte_range(first_id, EmacsByteRange::from_usize(1, 4))
            .expect("first narrow should succeed");
        eval.buffers
            .goto_buffer_emacs_byte_pos(first_id, crate::buffer::EmacsBytePos::new(2))
            .expect("first point should move");
        eval.buffers
            .goto_buffer_emacs_byte_pos(second_id, crate::buffer::EmacsBytePos::new(5))
            .expect("second point should move");
        eval.buffers
            .set_buffer_mark_emacs_byte_pos(first_id, crate::buffer::EmacsBytePos::new(3))
            .expect("first mark should set");
        eval.buffers
            .set_buffer_mark_emacs_byte_pos(second_id, crate::buffer::EmacsBytePos::new(1))
            .expect("second mark should set");

        {
            let first = eval
                .buffers
                .get_mut(first_id)
                .expect("first buffer should exist");
            first.slots[crate::buffer::buffer::BUFFER_SLOT_MARK_ACTIVE.index()] = Value::T;
            first.slots[crate::buffer::buffer::BUFFER_SLOT_POINT_BEFORE_SCROLL.index()] =
                Value::fixnum(2);
            first.set_undo_list(Value::symbol("first-undo"));
        }
        {
            let second = eval
                .buffers
                .get_mut(second_id)
                .expect("second buffer should exist");
            second.slots[crate::buffer::buffer::BUFFER_SLOT_MARK_ACTIVE.index()] = Value::NIL;
            second.slots[crate::buffer::buffer::BUFFER_SLOT_POINT_BEFORE_SCROLL.index()] =
                Value::fixnum(4);
            second.set_undo_list(Value::symbol("second-undo"));
        }

        eval.buffers.set_current(second_id);
        let overlay = builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(4)])
            .expect("overlay creation should succeed");

        eval.buffers.set_current(first_id);
        assert_eq!(
            builtin_buffer_swap_text(&mut eval, vec![Value::make_buffer(second_id)]).unwrap(),
            Value::NIL
        );

        let first = eval
            .buffers
            .get(first_id)
            .expect("first buffer should exist");
        let second = eval
            .buffers
            .get(second_id)
            .expect("second buffer should exist");

        assert_eq!(full_buffer_bytes(first), b"uvwxyz");
        assert_eq!(full_buffer_bytes(second), b"abcde");
        assert_eq!(first.text_backend_kind(), backend_kind);
        assert_eq!(
            second.text_backend_kind(),
            crate::buffer::BufferTextBackendKind::GapBuffer
        );
        assert_eq!(first.point_emacs_byte_pos().get(), 5);
        assert_eq!(first.point_min_emacs_byte_pos().get(), 0);
        assert_eq!(first.point_max_emacs_byte_pos().get(), 6);
        assert_eq!(first.mark_emacs_byte_pos().map(|pos| pos.get()), Some(1));
        assert_eq!(second.point_emacs_byte_pos().get(), 2);
        assert_eq!(second.point_min_emacs_byte_pos().get(), 1);
        assert_eq!(second.point_max_emacs_byte_pos().get(), 4);
        assert_eq!(second.mark_emacs_byte_pos().map(|pos| pos.get()), Some(3));
        assert_eq!(
            first.slots[crate::buffer::buffer::BUFFER_SLOT_MARK_ACTIVE.index()],
            Value::NIL
        );
        assert_eq!(
            second.slots[crate::buffer::buffer::BUFFER_SLOT_MARK_ACTIVE.index()],
            Value::T
        );
        assert_eq!(
            first.slots[crate::buffer::buffer::BUFFER_SLOT_POINT_BEFORE_SCROLL.index()],
            Value::NIL
        );
        assert_eq!(
            second.slots[crate::buffer::buffer::BUFFER_SLOT_POINT_BEFORE_SCROLL.index()],
            Value::NIL
        );
        assert_eq!(first.get_undo_list(), Value::symbol("second-undo"));
        assert_eq!(second.get_undo_list(), Value::symbol("first-undo"));
        assert_eq!(overlay.as_overlay_data().unwrap().buffer, Some(first_id));
        assert_eq!(first.overlays.len(), 1);
        assert!(second.overlays.is_empty());
    }
}

#[test]
fn buffer_swap_text_rejects_dead_and_indirect_buffers_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let base_id = eval.buffers.create_buffer("*swap-base*");
    let indirect_id = eval
        .buffers
        .create_indirect_buffer(base_id, "*swap-indirect*", false)
        .expect("indirect buffer should be created");
    let other_id = eval.buffers.create_buffer("*swap-other*");

    eval.buffers.set_current(indirect_id);
    let indirect_message = buffer_swap_text_signal_message(builtin_buffer_swap_text(
        &mut eval,
        vec![Value::make_buffer(other_id)],
    ));
    assert_eq!(indirect_message, "Cannot swap indirect buffers's text");

    eval.buffers.set_current(base_id);
    let has_indirect_message = buffer_swap_text_signal_message(builtin_buffer_swap_text(
        &mut eval,
        vec![Value::make_buffer(other_id)],
    ));
    assert_eq!(
        has_indirect_message,
        "One of the buffers to swap has indirect buffers"
    );

    let dead_id = eval.buffers.create_buffer("*swap-dead*");
    assert!(eval.buffers.kill_buffer(dead_id));
    eval.buffers.set_current(other_id);
    let dead_message = buffer_swap_text_signal_message(builtin_buffer_swap_text(
        &mut eval,
        vec![Value::make_buffer(dead_id)],
    ));
    assert_eq!(dead_message, "Cannot swap a dead buffer's text");
}

#[test]
fn buffer_swap_text_self_keeps_gnu_side_effects() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let buffer_id = eval.buffers.create_buffer("*swap-self*");
    eval.buffers
        .insert_into_buffer(buffer_id, "abc")
        .expect("insert should succeed");
    {
        let buffer = eval
            .buffers
            .get_mut(buffer_id)
            .expect("buffer should exist");
        buffer.slots[crate::buffer::buffer::BUFFER_SLOT_POINT_BEFORE_SCROLL.index()] =
            Value::fixnum(2);
    }
    let before_modified = eval.buffers.get(buffer_id).unwrap().modified_tick();
    let before_chars_modified = eval.buffers.get(buffer_id).unwrap().chars_modified_tick();
    let before_overlay_modified = eval.buffers.get(buffer_id).unwrap().overlay_modified_tick();

    eval.buffers.set_current(buffer_id);
    assert_eq!(
        builtin_buffer_swap_text(&mut eval, vec![Value::make_buffer(buffer_id)]).unwrap(),
        Value::NIL
    );

    let buffer = eval.buffers.get(buffer_id).expect("buffer should exist");
    assert_eq!(full_buffer_bytes(buffer), b"abc");
    assert_eq!(buffer.modified_tick(), before_modified + 2);
    assert_eq!(buffer.chars_modified_tick(), before_chars_modified + 2);
    assert_eq!(buffer.overlay_modified_tick(), before_overlay_modified + 2);
    assert_eq!(
        buffer.slots[crate::buffer::buffer::BUFFER_SLOT_POINT_BEFORE_SCROLL.index()],
        Value::NIL
    );
}

#[test]
fn compare_buffer_substrings_nil_bounds_use_accessible_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let left_id = eval.buffers.create_buffer("*cbs-left*");
    eval.buffers.set_current(left_id);
    builtin_insert(&mut eval, vec![Value::string("xaBCy")]).unwrap();
    let _ = eval
        .buffers
        .narrow_buffer_to_emacs_byte_range(left_id, EmacsByteRange::from_usize(1, 4));

    let right_id = eval.buffers.create_buffer("*cbs-right*");
    eval.buffers.set_current(right_id);
    builtin_insert(&mut eval, vec![Value::string("zaBCw")]).unwrap();
    let _ = eval
        .buffers
        .narrow_buffer_to_emacs_byte_range(right_id, EmacsByteRange::from_usize(1, 4));

    assert_eq!(
        builtin_compare_buffer_substrings(
            &mut eval,
            vec![
                Value::make_buffer(left_id),
                Value::NIL,
                Value::NIL,
                Value::make_buffer(right_id),
                Value::NIL,
                Value::NIL,
            ],
        )
        .unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn compare_buffer_substrings_signals_when_bounds_escape_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let left_id = eval.buffers.create_buffer("*cbs-left-range*");
    eval.buffers.set_current(left_id);
    builtin_insert(&mut eval, vec![Value::string("xaBCy")]).unwrap();
    let _ = eval
        .buffers
        .narrow_buffer_to_emacs_byte_range(left_id, EmacsByteRange::from_usize(1, 4));

    let right_id = eval.buffers.create_buffer("*cbs-right-range*");
    eval.buffers.set_current(right_id);
    builtin_insert(&mut eval, vec![Value::string("zaBCw")]).unwrap();
    let _ = eval
        .buffers
        .narrow_buffer_to_emacs_byte_range(right_id, EmacsByteRange::from_usize(1, 4));

    let err = builtin_compare_buffer_substrings(
        &mut eval,
        vec![
            Value::make_buffer(left_id),
            Value::fixnum(1),
            Value::NIL,
            Value::make_buffer(right_id),
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("compare-buffer-substrings should reject out-of-range narrowed START");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(1), Value::NIL]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn compare_buffer_substrings_rejects_deleted_buffer_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let dead = create_unique_test_buffer(&mut eval, "*cbs-dead*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead]).unwrap();

    let live = create_unique_test_buffer(&mut eval, "*cbs-live*");
    eval.buffers
        .set_current(expect_buffer_id(&live).expect("buffer id"));
    builtin_insert(&mut eval, vec![Value::string("abc")]).unwrap();

    let err = builtin_compare_buffer_substrings(
        &mut eval,
        vec![dead, Value::NIL, Value::NIL, live, Value::NIL, Value::NIL],
    )
    .expect_err("compare-buffer-substrings should reject deleted buffer objects");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Selecting deleted buffer")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn replace_region_contents_replaces_from_string_and_buffer_sources() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("abXXef")]).unwrap();

    assert_eq!(
        builtin_replace_region_contents(
            &mut eval,
            vec![
                Value::fixnum(3),
                Value::fixnum(5),
                Value::string("cd"),
                Value::fixnum(0)
            ]
        )
        .unwrap(),
        Value::T
    );
    assert_eq!(
        eval.buffers.current_buffer().unwrap().buffer_string(),
        "abcdef"
    );

    let source_id = eval.buffers.create_buffer("*rrc-source*");
    eval.buffers.set_current(source_id);
    builtin_insert(&mut eval, vec![Value::string("1234")]).unwrap();

    let dest_id = eval.buffers.create_buffer("*rrc-dest*");
    eval.buffers.set_current(dest_id);
    builtin_insert(&mut eval, vec![Value::string("abYYef")]).unwrap();

    assert_eq!(
        builtin_replace_region_contents(
            &mut eval,
            vec![
                Value::fixnum(3),
                Value::fixnum(5),
                Value::make_buffer(source_id)
            ]
        )
        .unwrap(),
        Value::T
    );
    assert_eq!(
        eval.buffers.current_buffer().unwrap().buffer_string(),
        "ab1234ef"
    );
}

#[test]
fn replace_region_contents_accepts_vector_buffer_slices() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let source_id = eval.buffers.create_buffer("*rrc-slice-source*");
    eval.buffers.set_current(source_id);
    builtin_insert(&mut eval, vec![Value::string("1234")]).unwrap();

    let dest_id = eval.buffers.create_buffer("*rrc-slice-dest*");
    eval.buffers.set_current(dest_id);
    builtin_insert(&mut eval, vec![Value::string("abZZef")]).unwrap();

    assert_eq!(
        builtin_replace_region_contents(
            &mut eval,
            vec![
                Value::fixnum(3),
                Value::fixnum(5),
                Value::vector(vec![
                    Value::make_buffer(source_id),
                    Value::fixnum(2),
                    Value::fixnum(4)
                ])
            ]
        )
        .unwrap(),
        Value::T
    );
    assert_eq!(
        eval.buffers
            .get(dest_id)
            .expect("destination buffer should exist")
            .buffer_string(),
        "ab23ef"
    );
}

#[test]
fn split_window_internal_validates_core_argument_types() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let split = builtin_split_window_internal(
        &mut eval,
        vec![Value::NIL, Value::NIL, Value::symbol("below"), Value::NIL],
    )
    .unwrap();
    assert!(split.is_window());

    let window_type = builtin_split_window_internal(
        &mut eval,
        vec![
            Value::symbol("not-a-window"),
            Value::NIL,
            Value::symbol("below"),
            Value::NIL,
        ],
    )
    .expect_err("split-window-internal should reject non-window objects");
    match window_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("windowp"), Value::symbol("not-a-window")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let size_type = builtin_split_window_internal(
        &mut eval,
        vec![
            Value::NIL,
            Value::string("bad"),
            Value::symbol("below"),
            Value::NIL,
        ],
    )
    .expect_err("split-window-internal should reject non-fixnum sizes");
    match size_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("fixnump"), Value::string("bad")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let side_type = builtin_split_window_internal(
        &mut eval,
        vec![Value::NIL, Value::NIL, Value::fixnum(9), Value::NIL],
    )
    .expect_err("split-window-internal should reject non-symbol SIDE");
    match side_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("symbolp"), Value::fixnum(9)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn barf_bury_char_equal_cl_type_and_cancel_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    assert!(
        builtin_char_equal(&mut eval, vec![Value::fixnum(97), Value::fixnum(65)])
            .unwrap()
            .is_truthy()
    );
    eval.obarray
        .set_symbol_value("case-fold-search", Value::NIL);
    assert!(
        builtin_char_equal(&mut eval, vec![Value::fixnum(97), Value::fixnum(65)])
            .unwrap()
            .is_nil()
    );
    eval.obarray.set_symbol_value("case-fold-search", Value::T);

    let char_type = builtin_char_equal(&mut eval, vec![Value::fixnum(1), Value::string("a")])
        .expect_err("char-equal should reject non-character args");
    match char_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::string("a")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    assert_eq!(
        builtin_cl_type_of(&[Value::NIL]).unwrap(),
        Value::symbol("null")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::T]).unwrap(),
        Value::symbol("boolean")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::fixnum(1)]).unwrap(),
        Value::symbol("fixnum")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::make_float(1.0)]).unwrap(),
        Value::symbol("float")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::string("x")]).unwrap(),
        Value::symbol("string")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::symbol("foo")]).unwrap(),
        Value::symbol("symbol")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::cons(Value::fixnum(1), Value::fixnum(2))]).unwrap(),
        Value::symbol("cons")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::vector(vec![Value::fixnum(1)])]).unwrap(),
        Value::symbol("vector")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::hash_table(HashTableTest::Equal)]).unwrap(),
        Value::symbol("hash-table")
    );
    assert_eq!(
        builtin_cl_type_of(&[Value::subr(intern("car"))]).unwrap(),
        Value::symbol("primitive-function")
    );
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x")]),
        body: Vec::new().into(),
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    assert_eq!(
        builtin_cl_type_of(&[lambda]).unwrap(),
        Value::symbol("interpreted-function")
    );

    let mut cancel_eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        builtin_cancel_kbd_macro_events(&mut cancel_eval, vec![]).unwrap(),
        Value::NIL
    );
    let cancel_arity = builtin_cancel_kbd_macro_events(&mut cancel_eval, vec![Value::NIL])
        .expect_err("cancel-kbd-macro-events should reject args");
    match cancel_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("cancel-kbd-macro-events"), Value::fixnum(1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let barf_buffer = builtin_get_buffer_create(&mut eval, vec![Value::string("*barf*")])
        .expect("create buffer for barf-if-buffer-read-only tests");
    let _ = builtin_set_buffer(&mut eval, vec![barf_buffer]).expect("select barf test buffer");
    builtin_insert(&mut eval, vec![Value::string("abc")]).expect("seed barf test buffer");

    assert_eq!(
        builtin_barf_if_buffer_read_only(&mut eval, vec![Value::fixnum(0)]).unwrap(),
        Value::NIL
    );
    if let Some(buf) = eval.buffers.current_buffer_mut() {
        buf.set_buffer_local("buffer-read-only", Value::T);
    }
    let barf_read_only = builtin_barf_if_buffer_read_only(&mut eval, vec![])
        .expect_err("barf-if-buffer-read-only should signal on read-only buffers");
    match barf_read_only {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "buffer-read-only");
            assert_eq!(sig.data, vec![barf_buffer]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    if let Some(buf) = eval.buffers.current_buffer_mut() {
        buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(1, 2),
            Value::symbol("inhibit-read-only"),
            Value::T,
        );
    }
    assert_eq!(
        builtin_barf_if_buffer_read_only(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::NIL
    );

    let barf_range = builtin_barf_if_buffer_read_only(&mut eval, vec![Value::fixnum(0)])
        .expect_err("barf-if-buffer-read-only should check lower-bound positions");
    match barf_range {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(0), Value::fixnum(0)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let barf_type = builtin_barf_if_buffer_read_only(&mut eval, vec![Value::string("x")])
        .expect_err("barf-if-buffer-read-only should reject non-fixnum positions");
    match barf_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::string("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let barf_arity = builtin_barf_if_buffer_read_only(&mut eval, vec![Value::NIL, Value::NIL])
        .expect_err("barf-if-buffer-read-only should reject >1 args");
    match barf_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("barf-if-buffer-read-only"), Value::fixnum(2)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
    if let Some(buf) = eval.buffers.current_buffer_mut() {
        buf.set_buffer_local("buffer-read-only", Value::NIL);
    }

    let buffer = create_unique_test_buffer(&mut eval, "*bury*");
    assert_eq!(
        builtin_bury_buffer_internal(&mut eval, vec![buffer]).unwrap(),
        Value::NIL
    );
    let bury_type = builtin_bury_buffer_internal(&mut eval, vec![Value::symbol("x")])
        .expect_err("bury-buffer-internal should reject non-buffer values");
    match bury_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("bufferp"), Value::symbol("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
    let bury_arity = builtin_bury_buffer_internal(&mut eval, vec![])
        .expect_err("bury-buffer-internal should reject wrong arity");
    match bury_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("bury-buffer-internal"), Value::fixnum(0)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn byte_position_and_clear_bitmap_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::NIL
    );

    builtin_erase_buffer(&mut eval, vec![]).unwrap();
    builtin_insert(&mut eval, vec![Value::string("a\u{00E9}")]).unwrap();

    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(4)]).unwrap(),
        Value::fixnum(3)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(4)
    );
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(2),
        false,
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![marker]).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(5)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(0)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(-1)]).unwrap(),
        Value::NIL
    );

    let byte_to_position_type = builtin_byte_to_position(&mut eval, vec![Value::string("x")])
        .expect_err("byte-to-position should enforce fixnum input");
    match byte_to_position_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::string("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let byte_to_position_arity =
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)])
            .expect_err("byte-to-position should reject wrong arity");
    match byte_to_position_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("byte-to-position"), Value::fixnum(2)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let byte_to_string = builtin_byte_to_string(vec![Value::fixnum(255)]).unwrap();
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(0), byte_to_string]).unwrap(),
        Value::fixnum(255)
    );

    let byte_to_string_type = builtin_byte_to_string(vec![Value::symbol("x")])
        .expect_err("byte-to-string should enforce fixnum input");
    match byte_to_string_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::symbol("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let byte_to_string_range = builtin_byte_to_string(vec![Value::fixnum(256)])
        .expect_err("byte-to-string should reject bytes above 255");
    match byte_to_string_range {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Invalid byte")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    assert_eq!(builtin_bitmap_spec_p(vec![Value::NIL]).unwrap(), Value::NIL);
    let bitmap_arity =
        builtin_bitmap_spec_p(vec![]).expect_err("bitmap-spec-p should reject wrong arity");
    match bitmap_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("bitmap-spec-p"), Value::fixnum(0)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let mut face_cache_ctx = crate::emacs_core::eval::Context::new();
    let face_change_before = face_cache_ctx.face_change_count;
    assert_eq!(
        builtin_clear_face_cache(&mut face_cache_ctx, vec![]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_clear_face_cache(&mut face_cache_ctx, vec![Value::symbol("all")]).unwrap(),
        Value::NIL
    );
    // GNU `Fclear_face_cache` sets `face_change = true` (src/xfaces.c:800), and
    // discarding every realized face is the whole point: without it a
    // `tty-color-define` that moves a colour never reaches the screen.
    assert!(
        face_cache_ctx.face_change_count > face_change_before,
        "clear-face-cache must invalidate the realized faces"
    );
    let clear_face_arity =
        builtin_clear_face_cache(&mut face_cache_ctx, vec![Value::NIL, Value::NIL])
            .expect_err("clear-face-cache should reject >1 args");
    match clear_face_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("clear-face-cache"), Value::fixnum(2)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    assert_eq!(
        builtin_clear_buffer_auto_save_failure(vec![]).unwrap(),
        Value::NIL
    );
    let clear_auto_save_arity = builtin_clear_buffer_auto_save_failure(vec![Value::NIL])
        .expect_err("clear-buffer-auto-save-failure should reject args");
    match clear_auto_save_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("clear-buffer-auto-save-failure"),
                    Value::fixnum(1)
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

/// `buffer-enable-undo' only.  Its partner `buffer-disable-undo' used to be
/// asserted here too, on the same bare `Context', but GNU has no C version of
/// it -- `buffer-enable-undo' is `DEFUN ("buffer-enable-undo", ...)' at
/// src/buffer.c:1829 while `buffer-disable-undo' is a `defun' at
/// lisp/simple.el:3591.  Its arms moved to `buffer_disable_undo_arms_match_gnu'
/// (`lisp_only_undo_commands_test.rs'), which asks the runtime.
/// DIVERGENCES.md 150.
#[test]
fn buffer_undo_designators_match_deleted_and_missing_buffer_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    // A bare `Context' is GNU before `loadup.el', so the disable half is void
    // here exactly as it is there.
    assert_eq!(
        crate::emacs_core::print::print_value(
            &eval.eval_str("(fboundp 'buffer-disable-undo)").unwrap()
        ),
        "nil",
    );

    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .configure_buffer_undo_list(current_id, Value::T)
        .expect("current buffer");
    let current = eval.buffers.get(current_id).expect("current buffer");
    assert!(crate::buffer::undo_list_is_disabled(
        &current.get_undo_list()
    ));
    assert_eq!(current.get_buffer_local("buffer-undo-list"), Some(Value::T));

    let enable_current =
        builtin_buffer_enable_undo(&mut eval, vec![]).expect("buffer-enable-undo should work");
    assert_eq!(enable_current, Value::NIL);
    let current = eval.buffers.get(current_id).expect("current buffer");
    assert!(!crate::buffer::undo_list_is_disabled(
        &current.get_undo_list()
    ));
    assert_eq!(
        current.get_buffer_local("buffer-undo-list"),
        Some(Value::NIL)
    );

    let enable_missing_name =
        builtin_buffer_enable_undo(&mut eval, vec![Value::string("*undo-enable-missing*")])
            .expect_err("buffer-enable-undo missing string should signal");
    match enable_missing_name {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("No buffer named *undo-enable-missing*")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let dead_for_enable = create_unique_test_buffer(&mut eval, "*undo-enable-deleted*");
    let _ = builtin_kill_buffer(&mut eval, vec![dead_for_enable]).unwrap();
    let enable_deleted = builtin_buffer_enable_undo(&mut eval, vec![dead_for_enable]).unwrap();
    assert_eq!(enable_deleted, Value::NIL);
}

#[test]
fn other_buffer_prefers_live_alternative_and_enforces_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let _ = builtin_get_buffer_create(&mut eval, vec![Value::string("*Messages*")]).unwrap();
    let avoid = builtin_get_buffer_create(&mut eval, vec![Value::string("*ob-avoid*")])
        .expect("create avoid buffer");
    let _ = builtin_get_buffer_create(&mut eval, vec![Value::string("*ob-alt*")]).unwrap();
    let _ = builtin_get_buffer_create(&mut eval, vec![Value::string(" hidden")]).unwrap();

    let other = builtin_other_buffer(&mut eval, vec![avoid]).expect("other-buffer");
    assert_eq!(
        other,
        eval.buffers
            .find_buffer_by_name("*Messages*")
            .map(Value::make_buffer)
            .expect("messages buffer")
    );

    let visible_ok =
        builtin_other_buffer(&mut eval, vec![avoid, Value::T]).expect("other-buffer visible-ok");
    assert_eq!(
        visible_ok,
        eval.buffers
            .find_buffer_by_name("*scratch*")
            .map(Value::make_buffer)
            .expect("scratch buffer")
    );

    let from_non_buffer =
        builtin_other_buffer(&mut eval, vec![Value::fixnum(1)]).expect("other-buffer int");
    assert_eq!(
        from_non_buffer,
        eval.buffers
            .find_buffer_by_name("*Messages*")
            .map(Value::make_buffer)
            .expect("messages buffer")
    );

    let from_missing_name = builtin_other_buffer(&mut eval, vec![Value::string("*missing*")])
        .expect("other-buffer missing name");
    assert_eq!(
        from_missing_name,
        eval.buffers
            .find_buffer_by_name("*Messages*")
            .map(Value::make_buffer)
            .expect("messages buffer")
    );

    let err = builtin_other_buffer(
        &mut eval,
        vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL],
    )
    .expect_err("other-buffer should reject more than three args");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("other-buffer"), Value::fixnum(4)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn buffer_list_returns_live_buffers_in_creation_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let scratch = eval
        .buffers
        .find_buffer_by_name("*scratch*")
        .expect("scratch");
    let a = builtin_get_buffer_create(&mut eval, vec![Value::string("*bl-a*")]).unwrap();
    let b = builtin_get_buffer_create(&mut eval, vec![Value::string("*bl-b*")]).unwrap();

    assert_eq!(
        builtin_buffer_list(&mut eval, vec![]).expect("buffer-list"),
        Value::list(vec![Value::make_buffer(scratch), a, b])
    );
}

#[test]
fn buffer_list_moves_selected_buffer_to_front_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::Context::new();
    let scratch = eval
        .buffers
        .find_buffer_by_name("*scratch*")
        .expect("scratch");
    eval.frames.create_frame("F1", 800, 600, scratch);
    let messages =
        builtin_get_buffer_create(&mut eval, vec![Value::string("*Messages*")]).expect("messages");
    let target = builtin_get_buffer_create(&mut eval, vec![Value::string("buffer-list-target")])
        .expect("target");

    // GNU's `switch-to-buffer' (lisp/window.el:9558) reduces to three C
    // primitives, and only one of them touches the list this test measures:
    // `record_buffer' is reachable from Lisp ONLY through `Fselect_window'
    // (src/window.c:582; the comment at :540 says so in as many words).
    // DIVERGENCES.md 154 deleted the Rust `switch-to-buffer' subr, so the
    // setup now names the C primitives GNU's Lisp would have called.
    eval.eval_str(
        r#"(let ((b (get-buffer "buffer-list-target")))
             (set-window-buffer (selected-window) b)
             (select-window (selected-window))
             (set-buffer b))"#,
    )
    .expect("switch to the target buffer through GNU's C primitives");

    assert_eq!(
        builtin_buffer_list(&mut eval, vec![]).expect("buffer-list"),
        Value::list(vec![target, Value::make_buffer(scratch), messages])
    );
}

#[test]
fn bury_buffer_internal_moves_buffer_to_end_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::Context::new();
    let scratch = eval
        .buffers
        .find_buffer_by_name("*scratch*")
        .expect("scratch");
    eval.frames.create_frame("F1", 800, 600, scratch);
    let messages =
        builtin_get_buffer_create(&mut eval, vec![Value::string("*Messages*")]).expect("messages");
    let target = builtin_get_buffer_create(&mut eval, vec![Value::string("bury-order-target")])
        .expect("target");

    // GNU's `switch-to-buffer' (lisp/window.el:9558) reduces to three C
    // primitives, and only one of them touches the list this test measures:
    // `record_buffer' is reachable from Lisp ONLY through `Fselect_window'
    // (src/window.c:582; the comment at :540 says so in as many words).
    // DIVERGENCES.md 154 deleted the Rust `switch-to-buffer' subr, so the
    // setup now names the C primitives GNU's Lisp would have called.
    eval.eval_str(
        r#"(let ((b (get-buffer "bury-order-target")))
             (set-window-buffer (selected-window) b)
             (select-window (selected-window))
             (set-buffer b))"#,
    )
    .expect("switch to the target buffer through GNU's C primitives");
    builtin_bury_buffer_internal(&mut eval, vec![target]).expect("bury-buffer-internal");

    assert_eq!(
        builtin_buffer_list(&mut eval, vec![]).expect("buffer-list"),
        Value::list(vec![Value::make_buffer(scratch), messages, target])
    );
}

#[test]
fn buffer_list_frame_arg_moves_displayed_buffer_to_front_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::Context::new();
    let scratch = eval
        .buffers
        .find_buffer_by_name("*scratch*")
        .expect("scratch");
    let fid = eval.frames.create_frame("F1", 800, 600, scratch);
    let frame = Value::make_frame(fid.0);
    let messages =
        builtin_get_buffer_create(&mut eval, vec![Value::string("*Messages*")]).expect("messages");
    let target = builtin_get_buffer_create(&mut eval, vec![Value::string("buffer-list-target")])
        .expect("target");

    // GNU's `switch-to-buffer' (lisp/window.el:9558) reduces to three C
    // primitives, and only one of them touches the list this test measures:
    // `record_buffer' is reachable from Lisp ONLY through `Fselect_window'
    // (src/window.c:582; the comment at :540 says so in as many words).
    // DIVERGENCES.md 154 deleted the Rust `switch-to-buffer' subr, so the
    // setup now names the C primitives GNU's Lisp would have called.
    eval.eval_str(
        r#"(let ((b (get-buffer "buffer-list-target")))
             (set-window-buffer (selected-window) b)
             (select-window (selected-window))
             (set-buffer b))"#,
    )
    .expect("switch to the target buffer through GNU's C primitives");

    assert_eq!(
        builtin_buffer_list(&mut eval, vec![frame]).expect("buffer-list"),
        Value::list(vec![target, Value::make_buffer(scratch), messages])
    );
}

#[test]
fn featurep_sees_emacs_from_core_startup_features() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    assert_eq!(
        builtin_featurep(&mut eval, vec![Value::symbol("emacs")]).unwrap(),
        Value::T
    );
}

#[test]
fn featurep_accepts_optional_subfeature_arg() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.set_variable(
        "features",
        Value::list(vec![Value::symbol("vm-featurep-present")]),
    );
    eval.obarray_mut()
        .put_property(
            "vm-featurep-present",
            "subfeatures",
            Value::list(vec![Value::symbol("vm-sub"), Value::fixnum(1)]),
        )
        .unwrap();

    let base = builtin_featurep(&mut eval, vec![Value::symbol("vm-featurep-present")]).unwrap();
    assert_eq!(base, Value::T);

    let with_nil = builtin_featurep(
        &mut eval,
        vec![Value::symbol("vm-featurep-present"), Value::NIL],
    )
    .unwrap();
    assert_eq!(with_nil, Value::T);

    let with_sub = builtin_featurep(
        &mut eval,
        vec![
            Value::symbol("vm-featurep-present"),
            Value::symbol("vm-sub"),
        ],
    )
    .unwrap();
    assert_eq!(with_sub, Value::T);

    let with_other = builtin_featurep(
        &mut eval,
        vec![
            Value::symbol("vm-featurep-present"),
            Value::symbol("vm-other"),
        ],
    )
    .unwrap();
    assert_eq!(with_other, Value::NIL);
}

#[test]
fn featurep_accepts_symbol_with_pos_when_enabled() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.set_variable(
        "features",
        Value::list(vec![Value::symbol("vm-featurep-present")]),
    );
    eval.set_variable("symbols-with-pos-enabled", Value::T);
    let feature = eval
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("vm-featurep-present"), Value::fixnum(42));

    let result = builtin_featurep(&mut eval, vec![feature]).unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn featurep_subfeatures_property_must_be_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.set_variable(
        "features",
        Value::list(vec![Value::symbol("vm-featurep-present")]),
    );
    eval.obarray_mut()
        .put_property("vm-featurep-present", "subfeatures", Value::fixnum(1))
        .unwrap();

    let err = builtin_featurep(
        &mut eval,
        vec![
            Value::symbol("vm-featurep-present"),
            Value::symbol("vm-sub"),
        ],
    )
    .expect_err("featurep should signal listp when subfeatures is not a list");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn featurep_rejects_more_than_two_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let err = builtin_featurep(
        &mut eval,
        vec![
            Value::symbol("vm-featurep-present"),
            Value::NIL,
            Value::symbol("extra"),
        ],
    )
    .expect_err("featurep should reject more than two arguments");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data, vec![Value::symbol("featurep"), Value::fixnum(3)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_string_constructor_builds_string() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure(
        "string",
        vec![Value::fixnum(65), Value::fixnum(66), Value::char('C')],
    )
    .expect("builtin string should resolve")
    .expect("builtin string should evaluate");
    assert_eq!(result, Value::string("ABC"));
}

#[test]
fn pure_dispatch_typed_string_constructor_rejects_modified_events() {
    crate::test_utils::init_test_tracing();
    let modified_a = crate::emacs_core::keyboard::pure::KEY_CHAR_META | i64::from(b'a');
    let err = dispatch_builtin_pure("string", vec![Value::fixnum(modified_a)])
        .expect("builtin string should resolve")
        .expect_err("GNU string rejects modified event codes");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(modified_a)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_propertize_validates_and_returns_string() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure(
        "propertize",
        vec![
            Value::string("x"),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .expect("builtin propertize should resolve")
    .expect("builtin propertize should evaluate");
    assert_eq!(result, Value::string("x"));
}

#[test]
fn pure_dispatch_typed_propertize_non_string_signals_stringp() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure("propertize", vec![Value::fixnum(1)])
        .expect("builtin propertize should resolve")
        .expect_err("propertize should reject non-string first arg");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_propertize_odd_property_list_signals_arity() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure(
        "propertize",
        vec![Value::string("x"), Value::symbol("face")],
    )
    .expect("builtin propertize should resolve")
    .expect_err("propertize should reject odd property argument count");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("propertize"), Value::fixnum(2)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_propertize_accepts_non_symbol_property_keys() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure(
        "propertize",
        vec![Value::string("x"), Value::fixnum(1), Value::symbol("v")],
    )
    .expect("builtin propertize should resolve")
    .expect("builtin propertize should evaluate");
    assert_eq!(result, Value::string("x"));
}

#[test]
fn pure_dispatch_typed_propertize_preserves_raw_unibyte_payload() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'x', 0xFF,
    ]));
    let result = dispatch_builtin_pure(
        "propertize",
        vec![raw, Value::symbol("face"), Value::symbol("bold")],
    )
    .expect("builtin propertize should resolve")
    .expect("builtin propertize should evaluate");
    let result = result.as_lisp_string().expect("string");
    assert!(!result.is_multibyte());
    assert_eq!(result.as_bytes(), &[b'x', 0xFF]);
}

#[test]
fn pure_dispatch_typed_unibyte_string_round_trips_bytes() {
    crate::test_utils::init_test_tracing();
    let s = dispatch_builtin_pure(
        "unibyte-string",
        vec![Value::fixnum(65), Value::fixnum(255), Value::fixnum(66)],
    )
    .expect("builtin unibyte-string should resolve")
    .expect("builtin unibyte-string should evaluate");

    let len = dispatch_builtin_pure("string-bytes", vec![s])
        .expect("builtin string-bytes should resolve")
        .expect("builtin string-bytes should evaluate");
    assert_eq!(len, Value::fixnum(3));

    let a = dispatch_builtin_pure("aref", vec![s, Value::fixnum(0)])
        .expect("builtin aref should resolve")
        .expect("builtin aref should evaluate");
    assert_eq!(a, Value::fixnum(65));

    let ff = dispatch_builtin_pure("aref", vec![s, Value::fixnum(1)])
        .expect("builtin aref should resolve")
        .expect("builtin aref should evaluate");
    assert_eq!(ff, Value::fixnum(255));

    let b = dispatch_builtin_pure("aref", vec![s, Value::fixnum(2)])
        .expect("builtin aref should resolve")
        .expect("builtin aref should evaluate");
    assert_eq!(b, Value::fixnum(66));
}

#[test]
fn pure_dispatch_typed_unibyte_string_validates_range_and_type() {
    crate::test_utils::init_test_tracing();
    let out_of_range = dispatch_builtin_pure("unibyte-string", vec![Value::fixnum(256)])
        .expect("builtin unibyte-string should resolve")
        .expect_err("expected args-out-of-range");
    match out_of_range {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(256), Value::fixnum(0), Value::fixnum(255)]
            );
        }
        other => panic!("expected signal flow, got {other:?}"),
    }

    let wrong_type = dispatch_builtin_pure("unibyte-string", vec![Value::string("x")])
        .expect("builtin unibyte-string should resolve")
        .expect_err("expected wrong-type-argument");
    match wrong_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integerp"), Value::string("x")]
            );
        }
        other => panic!("expected signal flow, got {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_vector_builds_vector() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure("vector", vec![Value::fixnum(7), Value::fixnum(9)])
        .expect("builtin vector should resolve")
        .expect("builtin vector should evaluate");
    assert_eq!(
        result,
        Value::vector(vec![Value::fixnum(7), Value::fixnum(9)])
    );
}

#[test]
fn pure_dispatch_typed_make_vector_validates_wholenump_length() {
    crate::test_utils::init_test_tracing();
    let ok = dispatch_builtin_pure("make-vector", vec![Value::fixnum(3), Value::symbol("x")])
        .expect("builtin make-vector should resolve")
        .expect("builtin make-vector should evaluate");
    assert_eq!(
        ok,
        Value::vector(vec![
            Value::symbol("x"),
            Value::symbol("x"),
            Value::symbol("x")
        ])
    );

    for bad_len in [
        Value::fixnum(-1),
        Value::make_float(1.5),
        Value::symbol("foo"),
    ] {
        let err = dispatch_builtin_pure("make-vector", vec![bad_len, Value::NIL])
            .expect("builtin make-vector should resolve")
            .expect_err("invalid lengths should signal");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("wholenump"), bad_len]);
            }
            other => panic!("expected signal flow, got {other:?}"),
        }
    }
}

#[test]
fn pure_dispatch_typed_aref_bool_vector_returns_boolean_bits() {
    crate::test_utils::init_test_tracing();
    let bv = Value::vector(vec![
        Value::symbol("--bool-vector--"),
        Value::fixnum(4),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
    ]);

    let initial = dispatch_builtin_pure("aref", vec![bv, Value::fixnum(2)])
        .expect("builtin aref should resolve")
        .expect("builtin aref should evaluate");
    assert!(initial.is_nil());

    let _ = dispatch_builtin_pure("aset", vec![bv, Value::fixnum(2), Value::T])
        .expect("builtin aset should resolve")
        .expect("builtin aset should evaluate");

    let updated = dispatch_builtin_pure("aref", vec![bv, Value::fixnum(2)])
        .expect("builtin aref should resolve")
        .expect("builtin aref should evaluate");
    assert!(updated.is_truthy());
}

#[test]
fn pure_dispatch_typed_aref_aset_char_table_uses_character_index_semantics() {
    crate::test_utils::init_test_tracing();
    let ct = Value::vector(vec![
        Value::symbol("--char-table--"),
        Value::NIL,
        Value::NIL,
        Value::symbol("syntax-table"),
        Value::fixnum(0),
    ]);

    let initial = dispatch_builtin_pure("aref", vec![ct, Value::fixnum(0)])
        .expect("builtin aref should resolve")
        .expect("builtin aref should evaluate");
    assert_eq!(initial, Value::NIL);

    let _ = dispatch_builtin_pure("aset", vec![ct, Value::fixnum(0x3F_FFFF), Value::fixnum(9)])
        .expect("builtin aset should resolve")
        .expect("builtin aset should evaluate");

    let edge = dispatch_builtin_pure("aref", vec![ct, Value::fixnum(0x3F_FFFF)])
        .expect("builtin aref should resolve")
        .expect("builtin aref should evaluate");
    assert_eq!(edge, Value::fixnum(9));

    let elt = dispatch_builtin_pure("elt", vec![ct, Value::fixnum(0x3F_FFFF)])
        .expect("builtin elt should resolve")
        .expect("builtin elt should evaluate");
    assert_eq!(elt, Value::fixnum(9));

    let negative = dispatch_builtin_pure("aref", vec![ct, Value::fixnum(-1)])
        .expect("builtin aref should resolve")
        .expect_err("negative char-table index should fail");
    match negative {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(-1)],
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let too_large =
        dispatch_builtin_pure("aset", vec![ct, Value::fixnum(0x40_0000), Value::fixnum(1)])
            .expect("builtin aset should resolve")
            .expect_err("out-of-range char-table index should fail");
    match too_large {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)],
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_length_family_uses_bool_vector_logical_length() {
    crate::test_utils::init_test_tracing();
    let bv = Value::vector(vec![
        Value::symbol("--bool-vector--"),
        Value::fixnum(3),
        Value::fixnum(1),
        Value::fixnum(0),
        Value::fixnum(1),
    ]);

    let len = dispatch_builtin_pure("length", vec![bv])
        .expect("builtin length should resolve")
        .expect("builtin length should evaluate");
    assert_eq!(len, Value::fixnum(3));

    let lt = dispatch_builtin_pure("length<", vec![bv, Value::fixnum(4)])
        .expect("builtin length< should resolve")
        .expect("builtin length< should evaluate");
    assert_eq!(lt, Value::T);

    let eq = dispatch_builtin_pure("length=", vec![bv, Value::fixnum(3)])
        .expect("builtin length= should resolve")
        .expect("builtin length= should evaluate");
    assert_eq!(eq, Value::T);

    let gt = dispatch_builtin_pure("length>", vec![bv, Value::fixnum(2)])
        .expect("builtin length> should resolve")
        .expect("builtin length> should evaluate");
    assert_eq!(gt, Value::T);
}

#[test]
fn pure_dispatch_typed_length_family_uses_char_table_logical_length() {
    crate::test_utils::init_test_tracing();
    let ct = Value::vector(vec![
        Value::symbol("--char-table--"),
        Value::NIL,
        Value::NIL,
        Value::symbol("syntax-table"),
        Value::fixnum(0),
    ]);

    let len = dispatch_builtin_pure("length", vec![ct])
        .expect("builtin length should resolve")
        .expect("builtin length should evaluate");
    assert_eq!(len, Value::fixnum(0x40_0000));

    let lt = dispatch_builtin_pure("length<", vec![ct, Value::fixnum(100)])
        .expect("builtin length< should resolve")
        .expect("builtin length< should evaluate");
    assert_eq!(lt, Value::NIL);

    let eq = dispatch_builtin_pure("length=", vec![ct, Value::fixnum(0x40_0000)])
        .expect("builtin length= should resolve")
        .expect("builtin length= should evaluate");
    assert_eq!(eq, Value::T);

    let gt = dispatch_builtin_pure("length>", vec![ct, Value::fixnum(0)])
        .expect("builtin length> should resolve")
        .expect("builtin length> should evaluate");
    assert_eq!(gt, Value::T);
}

#[test]
fn pure_dispatch_typed_length_family_accepts_records() {
    crate::test_utils::init_test_tracing();
    let record = Value::make_record(vec![
        Value::symbol("vm-record-type"),
        Value::fixnum(1),
        Value::fixnum(2),
    ]);

    let len = dispatch_builtin_pure("length", vec![record])
        .expect("builtin length should resolve")
        .expect("builtin length should evaluate");
    assert_eq!(len, Value::fixnum(3));

    let lt = dispatch_builtin_pure("length<", vec![record, Value::fixnum(4)])
        .expect("builtin length< should resolve")
        .expect("builtin length< should evaluate");
    assert_eq!(lt, Value::T);

    let eq = dispatch_builtin_pure("length=", vec![record, Value::fixnum(3)])
        .expect("builtin length= should resolve")
        .expect("builtin length= should evaluate");
    assert_eq!(eq, Value::T);

    let gt = dispatch_builtin_pure("length>", vec![record, Value::fixnum(2)])
        .expect("builtin length> should resolve")
        .expect("builtin length> should evaluate");
    assert_eq!(gt, Value::T);
}

#[test]
fn pure_dispatch_typed_aset_string_returns_new_element_and_computes_replacement() {
    crate::test_utils::init_test_tracing();
    let result = dispatch_builtin_pure(
        "aset",
        vec![Value::string("abc"), Value::fixnum(1), Value::fixnum(120)],
    )
    .expect("builtin aset should resolve")
    .expect("builtin aset should evaluate");
    assert_eq!(result, Value::fixnum(120));

    let replacement = aset_string_replacement(
        &Value::string("abc"),
        &Value::fixnum(1),
        &Value::fixnum(120),
    )
    .expect("string replacement should succeed");
    assert_eq!(replacement, Value::string("axc"));
}

#[test]
fn repeated_unibyte_aset_is_linear_enough_for_binary_protocol_frames() {
    crate::test_utils::init_test_tracing();
    const BYTE_COUNT: usize = 10_000;
    let string = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'x';
        BYTE_COUNT
    ]));
    let started = std::time::Instant::now();

    for index in 0..BYTE_COUNT {
        let replacement = Value::fixnum((index % 256) as i64);
        let returned = aset_string_replacement(&string, &Value::fixnum(index as i64), &replacement)
            .expect("replace one byte in a protocol frame");
        assert_eq!(returned, string);
    }

    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "GNU writes one unibyte string element in O(1); {BYTE_COUNT} replacements took {elapsed:?}"
    );
    let bytes = string.as_lisp_string().expect("unibyte string").as_bytes();
    assert_eq!(bytes[0], 0);
    assert_eq!(bytes[BYTE_COUNT - 1], ((BYTE_COUNT - 1) % 256) as u8);
}

#[test]
fn pure_dispatch_typed_aset_string_errors_match_oracle() {
    crate::test_utils::init_test_tracing();
    let out_of_range = dispatch_builtin_pure(
        "aset",
        vec![Value::string("abc"), Value::fixnum(-1), Value::fixnum(120)],
    )
    .expect("builtin aset should resolve")
    .expect_err("aset should reject negative index");
    match out_of_range {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::string("abc"), Value::fixnum(-1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let wrong_type = dispatch_builtin_pure(
        "aset",
        vec![Value::string("abc"), Value::fixnum(1), Value::NIL],
    )
    .expect("builtin aset should resolve")
    .expect_err("aset should validate replacement character");
    match wrong_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("characterp"), Value::NIL]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_char_string_conversions_work() {
    crate::test_utils::init_test_tracing();
    let as_code = dispatch_builtin_pure("string-to-char", vec![Value::string("A")])
        .expect("builtin string-to-char should resolve")
        .expect("builtin string-to-char should evaluate");
    assert_eq!(as_code, Value::fixnum(65));

    let as_string = dispatch_builtin_pure("char-to-string", vec![Value::fixnum(65)])
        .expect("builtin char-to-string should resolve")
        .expect("builtin char-to-string should evaluate");
    assert_eq!(as_string, Value::string("A"));
}

#[test]
fn pure_dispatch_typed_hash_table_round_trip() {
    crate::test_utils::init_test_tracing();
    let table = dispatch_builtin_pure(
        "make-hash-table",
        vec![Value::keyword(":test"), Value::symbol("equal")],
    )
    .expect("builtin make-hash-table should resolve")
    .expect("builtin make-hash-table should evaluate");

    dispatch_builtin_pure(
        "puthash",
        vec![Value::string("answer"), Value::fixnum(42), table],
    )
    .expect("builtin puthash should resolve")
    .expect("builtin puthash should evaluate");

    let value = dispatch_builtin_pure("gethash", vec![Value::string("answer"), table])
        .expect("builtin gethash should resolve")
        .expect("builtin gethash should evaluate");
    assert_eq!(value, Value::fixnum(42));

    let count = dispatch_builtin_pure("hash-table-count", vec![table])
        .expect("builtin hash-table-count should resolve")
        .expect("builtin hash-table-count should evaluate");
    assert_eq!(count, Value::fixnum(1));
}

#[test]
fn pure_dispatch_typed_hash_table_extended_builtins_round_trip() {
    crate::test_utils::init_test_tracing();
    let alias = Value::symbol("neovm--pure-dispatch-eq-test-alias");
    dispatch_builtin_pure(
        "define-hash-table-test",
        vec![alias, Value::symbol("eq"), Value::symbol("sxhash-eq")],
    )
    .expect("define-hash-table-test should resolve")
    .expect("define-hash-table-test should evaluate");

    let table = dispatch_builtin_pure("make-hash-table", vec![Value::keyword(":test"), alias])
        .expect("make-hash-table should resolve")
        .expect("make-hash-table should evaluate");

    let test_name = dispatch_builtin_pure("hash-table-test", vec![table])
        .expect("hash-table-test should resolve")
        .expect("hash-table-test should evaluate");
    assert_eq!(test_name, alias.clone());

    let size = dispatch_builtin_pure("hash-table-size", vec![table])
        .expect("hash-table-size should resolve")
        .expect("hash-table-size should evaluate");
    assert_eq!(size, Value::fixnum(0));

    let weakness = dispatch_builtin_pure("hash-table-weakness", vec![table])
        .expect("hash-table-weakness should resolve")
        .expect("hash-table-weakness should evaluate");
    assert_eq!(weakness, Value::NIL);

    let rehash_size = dispatch_builtin_pure("hash-table-rehash-size", vec![table])
        .expect("hash-table-rehash-size should resolve")
        .expect("hash-table-rehash-size should evaluate");
    assert_eq!(rehash_size, Value::make_float(1.5));

    let rehash_threshold = dispatch_builtin_pure("hash-table-rehash-threshold", vec![table])
        .expect("hash-table-rehash-threshold should resolve")
        .expect("hash-table-rehash-threshold should evaluate");
    assert_eq!(rehash_threshold, Value::make_float(0.8125));

    let sxhash = dispatch_builtin_pure("sxhash-eq", vec![Value::symbol("k")])
        .expect("sxhash-eq should resolve")
        .expect("sxhash-eq should evaluate");
    assert!(sxhash.is_fixnum());

    let buckets_before = dispatch_builtin_pure("internal--hash-table-buckets", vec![table])
        .expect("internal--hash-table-buckets should resolve")
        .expect("internal--hash-table-buckets should evaluate");
    assert_eq!(buckets_before, Value::NIL);

    let _ = dispatch_builtin_pure("puthash", vec![Value::symbol("k"), Value::fixnum(1), table])
        .expect("puthash should resolve")
        .expect("puthash should evaluate");

    let buckets_after = dispatch_builtin_pure("internal--hash-table-buckets", vec![table])
        .expect("internal--hash-table-buckets should resolve")
        .expect("internal--hash-table-buckets should evaluate");
    assert!(!buckets_after.is_nil());

    let histogram = dispatch_builtin_pure("internal--hash-table-histogram", vec![table])
        .expect("internal--hash-table-histogram should resolve")
        .expect("internal--hash-table-histogram should evaluate");
    assert!(!histogram.is_nil());

    let index_size = dispatch_builtin_pure("internal--hash-table-index-size", vec![table])
        .expect("internal--hash-table-index-size should resolve")
        .expect("internal--hash-table-index-size should evaluate");
    assert!(index_size.as_fixnum().map_or(false, |n| n >= 1));

    let copied = dispatch_builtin_pure("copy-hash-table", vec![table])
        .expect("copy-hash-table should resolve")
        .expect("copy-hash-table should evaluate");
    let copied_test = dispatch_builtin_pure("hash-table-test", vec![copied])
        .expect("hash-table-test should resolve for copied table")
        .expect("hash-table-test should evaluate for copied table");
    assert_eq!(copied_test, alias);
}

#[test]
fn pure_dispatch_typed_define_hash_table_test_registers_alias() {
    crate::test_utils::init_test_tracing();
    let alias = Value::symbol("neovm--eq-test-alias");

    let defined = dispatch_builtin_pure(
        "define-hash-table-test",
        vec![alias, Value::symbol("eq"), Value::symbol("sxhash-eq")],
    )
    .expect("define-hash-table-test should resolve")
    .expect("define-hash-table-test should evaluate");
    assert_eq!(
        defined,
        Value::list(vec![Value::symbol("eq"), Value::symbol("sxhash-eq")])
    );

    let table = dispatch_builtin_pure("make-hash-table", vec![Value::keyword(":test"), alias])
        .expect("make-hash-table should resolve")
        .expect("make-hash-table should evaluate");
    let observed = crate::emacs_core::hashtab::builtin_hash_table_test(vec![table])
        .expect("hash-table-test should evaluate");
    assert_eq!(observed, alias);

    if !table.is_hash_table() {
        panic!("expected hash table");
    };
    assert!(matches!(
        table.as_hash_table().unwrap().test.clone(),
        HashTableTest::Eq
    ));
}

#[test]
fn pure_dispatch_typed_define_hash_table_test_accepts_equal_including_properties_pair() {
    crate::test_utils::init_test_tracing();
    let alias = Value::symbol("neovm--equal-props-test-alias");

    let defined = dispatch_builtin_pure(
        "define-hash-table-test",
        vec![
            alias,
            Value::symbol("equal-including-properties"),
            Value::symbol("sxhash-equal-including-properties"),
        ],
    )
    .expect("define-hash-table-test should resolve")
    .expect("define-hash-table-test should evaluate");
    assert_eq!(
        defined,
        Value::list(vec![
            Value::symbol("equal-including-properties"),
            Value::symbol("sxhash-equal-including-properties"),
        ])
    );

    let table = dispatch_builtin_pure("make-hash-table", vec![Value::keyword(":test"), alias])
        .expect("make-hash-table should resolve")
        .expect("make-hash-table should evaluate");
    let observed = crate::emacs_core::hashtab::builtin_hash_table_test(vec![table])
        .expect("hash-table-test should evaluate");
    assert_eq!(observed, alias);

    if !table.is_hash_table() {
        panic!("expected hash table");
    };
    assert!(matches!(
        table.as_hash_table().unwrap().test.clone(),
        HashTableTest::Equal
    ));
}

#[test]
fn define_hash_table_test_alias_is_thread_local() {
    crate::test_utils::init_test_tracing();
    let alias_name = "neovm--cross-thread-eq-test-alias";
    // Register alias on a spawned thread
    std::thread::spawn(move || {
        let _ = builtin_define_hash_table_test(vec![
            Value::symbol(alias_name),
            Value::symbol("eq"),
            Value::symbol("sxhash-eq"),
        ])
        .expect("define-hash-table-test should evaluate in worker thread");
        // Verify it IS visible on the same thread
        assert!(lookup_hash_table_test_alias(alias_name).is_some());
    })
    .join()
    .expect("worker thread should complete");

    // Alias registered on other thread should NOT be visible here
    assert!(lookup_hash_table_test_alias(alias_name).is_none());
}

#[test]
fn define_hash_table_test_alias_redefinition_updates_mapping() {
    crate::test_utils::init_test_tracing();
    let alias_name = "neovm--eq-test-alias-redefined";

    dispatch_builtin_pure(
        "define-hash-table-test",
        vec![
            Value::symbol(alias_name),
            Value::symbol("eq"),
            Value::symbol("sxhash-eq"),
        ],
    )
    .expect("initial define-hash-table-test should resolve")
    .expect("initial define-hash-table-test should evaluate");
    let first = dispatch_builtin_pure(
        "make-hash-table",
        vec![Value::keyword(":test"), Value::symbol(alias_name)],
    )
    .expect("make-hash-table should resolve")
    .expect("make-hash-table should evaluate");
    let first_name = crate::emacs_core::hashtab::builtin_hash_table_test(vec![first])
        .expect("hash-table-test should evaluate for initial alias mapping");
    assert_eq!(first_name, Value::symbol(alias_name));

    if !first.is_hash_table() {
        panic!("expected hash table");
    };
    assert!(matches!(
        first.as_hash_table().unwrap().test.clone(),
        HashTableTest::Eq
    ));

    dispatch_builtin_pure(
        "define-hash-table-test",
        vec![
            Value::symbol(alias_name),
            Value::symbol("equal"),
            Value::symbol("sxhash-equal"),
        ],
    )
    .expect("redefined hash-table test should resolve")
    .expect("redefined hash-table test should evaluate");
    let second = dispatch_builtin_pure(
        "make-hash-table",
        vec![Value::keyword(":test"), Value::symbol(alias_name)],
    )
    .expect("make-hash-table should resolve after alias redefinition")
    .expect("make-hash-table should evaluate after alias redefinition");
    let second_name = crate::emacs_core::hashtab::builtin_hash_table_test(vec![second])
        .expect("hash-table-test should evaluate after alias redefinition");
    assert_eq!(second_name, Value::symbol(alias_name));

    if !second.is_hash_table() {
        panic!("expected hash table");
    };
    assert!(matches!(
        second.as_hash_table().unwrap().test.clone(),
        HashTableTest::Equal
    ));
}

#[test]
fn define_hash_table_test_accepts_user_defined_lisp_functions() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let result = eval.eval_str(
        r#"
        (progn
          (define-hash-table-test
           'neovm-len-test
           (lambda (a b) (= (length a) (length b)))
           (lambda (a) (length a)))
          (let ((h (make-hash-table :test 'neovm-len-test)))
            (puthash "aa" 1 h)
            (list (gethash "bb" h)
                  (gethash "c" h 'missing)
                  (hash-table-test h))))
        "#,
    );

    assert_eq!(format_eval_result(&result), "OK (1 missing neovm-len-test)");
}

#[test]
fn pure_dispatch_typed_plist_and_symbol_round_trip() {
    crate::test_utils::init_test_tracing();
    let plist = dispatch_builtin_pure(
        "plist-put",
        vec![Value::NIL, Value::keyword(":lang"), Value::string("rust")],
    )
    .expect("builtin plist-put should resolve")
    .expect("builtin plist-put should evaluate");

    let lang = dispatch_builtin_pure("plist-get", vec![plist, Value::keyword(":lang")])
        .expect("builtin plist-get should resolve")
        .expect("builtin plist-get should evaluate");
    assert_eq!(lang, Value::string("rust"));

    let sym = dispatch_builtin_pure("make-symbol", vec![Value::string("neo-vm")])
        .expect("builtin make-symbol should resolve")
        .expect("builtin make-symbol should evaluate");
    let name = dispatch_builtin_pure("symbol-name", vec![sym])
        .expect("builtin symbol-name should resolve")
        .expect("builtin symbol-name should evaluate");
    assert_eq!(name, Value::string("neo-vm"));
}

#[test]
fn make_symbol_and_symbol_name_preserve_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let raw_name = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'a',
    ]));

    let sym = dispatch_builtin_pure("make-symbol", vec![raw_name])
        .expect("builtin make-symbol should resolve")
        .expect("builtin make-symbol should evaluate");
    let printed_name = dispatch_builtin_pure("symbol-name", vec![sym])
        .expect("builtin symbol-name should resolve")
        .expect("builtin symbol-name should evaluate");

    let ls = printed_name
        .as_lisp_string()
        .expect("symbol-name should return a string");
    assert_eq!(ls.as_bytes(), &[0xFF, b'a']);
    assert!(!ls.is_multibyte());
}

#[test]
fn make_symbol_keeps_exact_name_string_object() {
    crate::test_utils::init_test_tracing();
    let name = Value::string("abc");

    let sym = dispatch_builtin_pure("make-symbol", vec![name])
        .expect("builtin make-symbol should resolve")
        .expect("builtin make-symbol should evaluate");
    let first_name = dispatch_builtin_pure("symbol-name", vec![sym])
        .expect("builtin symbol-name should resolve")
        .expect("builtin symbol-name should evaluate");
    assert_eq!(first_name, name);

    super::collections::builtin_aset(vec![name, Value::fixnum(1), Value::fixnum('Z' as i64)])
        .expect("aset should mutate symbol name string");

    let second_name = dispatch_builtin_pure("symbol-name", vec![sym])
        .expect("builtin symbol-name should resolve")
        .expect("builtin symbol-name should evaluate");
    assert_eq!(second_name, name);
    assert_eq!(
        second_name.as_lisp_string().and_then(|s| s.as_utf8_str()),
        Some("aZc")
    );
}

#[test]
fn intern_and_intern_soft_preserve_raw_unibyte_symbol_names() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let raw_name = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'a',
    ]));

    let sym = dispatch_builtin(&mut eval, "intern", vec![raw_name])
        .expect("builtin intern should resolve")
        .expect("builtin intern should evaluate");
    let soft = dispatch_builtin(&mut eval, "intern-soft", vec![raw_name])
        .expect("builtin intern-soft should resolve")
        .expect("builtin intern-soft should evaluate");
    let roundtrip = dispatch_builtin(&mut eval, "symbol-name", vec![sym])
        .expect("builtin symbol-name should resolve")
        .expect("builtin symbol-name should evaluate");

    assert_eq!(soft, sym);
    let ls = roundtrip
        .as_lisp_string()
        .expect("symbol-name should return a string");
    assert_eq!(ls.as_bytes(), &[0xFF, b'a']);
    assert!(!ls.is_multibyte());
}

#[test]
fn custom_obarray_intern_reuses_ascii_multibyte_symbol_names() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let obarray = dispatch_builtin(&mut eval, "obarray-make", vec![Value::fixnum(17)])
        .expect("builtin obarray-make should resolve")
        .expect("builtin obarray-make should evaluate");
    let ascii_multibyte = Value::multibyte_string("/tmp/HELLO");

    let first = dispatch_builtin(&mut eval, "intern", vec![ascii_multibyte, obarray])
        .expect("builtin intern should resolve")
        .expect("builtin intern should evaluate");
    let second = dispatch_builtin(
        &mut eval,
        "intern",
        vec![Value::multibyte_string("/tmp/HELLO"), obarray],
    )
    .expect("builtin intern should resolve")
    .expect("builtin intern should evaluate");
    let soft = dispatch_builtin(
        &mut eval,
        "intern-soft",
        vec![Value::string("/tmp/HELLO"), obarray],
    )
    .expect("builtin intern-soft should resolve")
    .expect("builtin intern-soft should evaluate");

    assert_eq!(first, second);
    assert_eq!(soft, first);

    let stored_name = dispatch_builtin(&mut eval, "symbol-name", vec![first])
        .expect("builtin symbol-name should resolve")
        .expect("builtin symbol-name should evaluate")
        .as_lisp_string()
        .expect("symbol-name should return a string")
        .clone();
    assert_eq!(stored_name.as_bytes(), b"/tmp/HELLO");
    // The name KEEPS the representation it was interned from. This assertion
    // read `!is_multibyte` until it was checked against GNU 31.0.90, where
    // `(multibyte-string-p (symbol-name (intern (string-to-multibyte "/tmp/HELLO") ob)))`
    // is `t` for a custom obarray and for the global one alike: GNU stores the
    // argument string and folds representation only when deciding IDENTITY.
    assert!(stored_name.is_multibyte());
}

#[test]
fn custom_obarray_symbol_properties_roundtrip_for_ascii_multibyte_names() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let obarray = dispatch_builtin(&mut eval, "obarray-make", vec![Value::fixnum(17)])
        .expect("builtin obarray-make should resolve")
        .expect("builtin obarray-make should evaluate");
    let first = dispatch_builtin(
        &mut eval,
        "intern",
        vec![Value::multibyte_string("/tmp/HELLO"), obarray],
    )
    .expect("builtin intern should resolve")
    .expect("builtin intern should evaluate");

    let set = dispatch_builtin(
        &mut eval,
        "put",
        vec![first, Value::symbol("vc-backend"), Value::symbol("Git")],
    )
    .expect("builtin put should resolve")
    .expect("builtin put should evaluate");
    assert_eq!(set, Value::symbol("Git"));

    let second = dispatch_builtin(
        &mut eval,
        "intern",
        vec![Value::multibyte_string("/tmp/HELLO"), obarray],
    )
    .expect("builtin intern should resolve")
    .expect("builtin intern should evaluate");
    let got = dispatch_builtin(&mut eval, "get", vec![second, Value::symbol("vc-backend")])
        .expect("builtin get should resolve")
        .expect("builtin get should evaluate");

    assert_eq!(second, first);
    assert_eq!(got, Value::symbol("Git"));
}

#[test]
fn pure_dispatch_typed_math_ops_work() {
    crate::test_utils::init_test_tracing();
    let sqrt = dispatch_builtin_pure("sqrt", vec![Value::fixnum(4)])
        .expect("builtin sqrt should resolve")
        .expect("builtin sqrt should evaluate");
    assert_eq!(sqrt, Value::make_float(2.0));

    let expt = dispatch_builtin_pure("expt", vec![Value::fixnum(2), Value::fixnum(8)])
        .expect("builtin expt should resolve")
        .expect("builtin expt should evaluate");
    assert_eq!(expt, Value::fixnum(256));

    let nan_check = dispatch_builtin_pure("isnan", vec![Value::make_float(f64::NAN)])
        .expect("builtin isnan should resolve")
        .expect("builtin isnan should evaluate");
    assert!(nan_check.is_truthy());
}

#[test]
fn pure_dispatch_typed_expt_and_isnan_type_errors_match_oracle() {
    crate::test_utils::init_test_tracing();
    let expt_base = dispatch_builtin_pure("expt", vec![Value::symbol("a"), Value::fixnum(2)])
        .expect("builtin expt should resolve")
        .expect_err("expt should reject non-numeric base");
    match expt_base {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("numberp"), Value::symbol("a")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let expt_exp = dispatch_builtin_pure("expt", vec![Value::fixnum(2), Value::symbol("a")])
        .expect("builtin expt should resolve")
        .expect_err("expt should reject non-numeric exponent");
    match expt_exp {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("numberp"), Value::symbol("a")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let isnan_non_float = dispatch_builtin_pure("isnan", vec![Value::fixnum(1)])
        .expect("builtin isnan should resolve")
        .expect_err("isnan should reject non-floats");
    match isnan_non_float {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("floatp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_typed_round_half_ties_to_even() {
    crate::test_utils::init_test_tracing();
    let positive_half = dispatch_builtin_pure("round", vec![Value::make_float(2.5)])
        .expect("builtin round should resolve")
        .expect("builtin round should evaluate");
    assert_eq!(positive_half, Value::fixnum(2));

    let negative_half = dispatch_builtin_pure("round", vec![Value::make_float(-2.5)])
        .expect("builtin round should resolve")
        .expect("builtin round should evaluate");
    assert_eq!(negative_half, Value::fixnum(-2));

    let zero_half = dispatch_builtin_pure("round", vec![Value::make_float(0.5)])
        .expect("builtin round should resolve")
        .expect("builtin round should evaluate");
    assert_eq!(zero_half, Value::fixnum(0));

    let negative_zero_half = dispatch_builtin_pure("round", vec![Value::make_float(-0.5)])
        .expect("builtin round should resolve")
        .expect("builtin round should evaluate");
    assert_eq!(negative_zero_half, Value::fixnum(0));
}

#[test]
fn pure_dispatch_typed_string_width_and_bytes_work() {
    crate::test_utils::init_test_tracing();
    let width = dispatch_builtin_pure("string-width", vec![Value::string("ab")])
        .expect("builtin string-width should resolve")
        .expect("builtin string-width should evaluate");
    assert_eq!(width, Value::fixnum(2));

    let bytes = dispatch_builtin_pure("string-bytes", vec![Value::string("ab")])
        .expect("builtin string-bytes should resolve")
        .expect("builtin string-bytes should evaluate");
    assert_eq!(bytes, Value::fixnum(2));
}

#[test]
fn pure_dispatch_typed_extended_list_ops_work() {
    crate::test_utils::init_test_tracing();
    let _seq = Value::list(vec![
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
        Value::fixnum(4),
    ]);

    let truncated = dispatch_builtin_pure(
        "ntake",
        vec![
            Value::fixnum(2),
            Value::list(vec![Value::fixnum(7), Value::fixnum(8), Value::fixnum(9)]),
        ],
    )
    .expect("builtin ntake should resolve")
    .expect("builtin ntake should evaluate");
    assert_eq!(
        truncated,
        Value::list(vec![Value::fixnum(7), Value::fixnum(8)])
    );
}

#[test]
fn pure_dispatch_obarray_make_returns_gnu_obarray_and_clear_keeps_vector_compat() {
    crate::test_utils::init_test_tracing();
    let made = dispatch_builtin_pure("obarray-make", vec![Value::fixnum(3)])
        .expect("builtin obarray-make should resolve")
        .expect("builtin obarray-make should evaluate");
    if !&made.is_obarray() {
        panic!("obarray-make should return obarray");
    };
    let created_data = crate::emacs_core::builtins::symbols::obarray_buckets(made).unwrap();
    assert_eq!(created_data.len(), 3);
    assert!(created_data.iter().all(|v| v.is_nil()));

    let default = dispatch_builtin_pure("obarray-make", vec![])
        .expect("builtin obarray-make should resolve")
        .expect("builtin obarray-make should evaluate");
    if !&default.is_obarray() {
        panic!("obarray-make default should return obarray");
    };
    assert_eq!(
        crate::emacs_core::builtins::symbols::obarray_len(default).unwrap(),
        1511
    );

    let table = Value::vector(vec![Value::NIL, Value::list(vec![Value::symbol("x")])]);
    let cleared = dispatch_builtin_pure("obarray-clear", vec![table])
        .expect("builtin obarray-clear should resolve")
        .expect("builtin obarray-clear should evaluate");
    assert!(cleared.is_nil());
    if !&table.is_vector() {
        panic!("table should stay vector");
    };
    assert!(table.as_vector_data().unwrap().iter().all(|v| v.is_nil()));

    let wrong_type = dispatch_builtin_pure("obarray-clear", vec![Value::fixnum(1)])
        .expect("builtin obarray-clear should resolve")
        .expect_err("obarray-clear should reject non-obarray arguments");
    match wrong_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("obarrayp"), Value::fixnum(1)]);
        }
        other => panic!("expected signal, got: {other:?}"),
    }
}

#[test]
fn eval_dispatch_obarrayp_accepts_custom_obarrays() {
    crate::test_utils::init_test_tracing();
    let table = crate::emacs_core::builtins::symbols::builtin_obarray_make(vec![Value::fixnum(3)])
        .expect("obarray-make should evaluate");
    let result = crate::emacs_core::builtins::symbols::builtin_obarrayp(vec![table])
        .expect("obarrayp should evaluate");
    assert!(result.is_truthy());

    let non_obarray = Value::vector(vec![Value::fixnum(0); 3]);
    let result = crate::emacs_core::builtins::symbols::builtin_obarrayp(vec![non_obarray])
        .expect("obarrayp should evaluate");
    assert!(result.is_nil());
}

#[test]
fn pure_dispatch_make_temp_file_internal_matches_gnu_core_modes() {
    crate::test_utils::init_test_tracing();
    let created = dispatch_builtin_pure(
        "make-temp-file-internal",
        vec![
            Value::string("neovm-mtfi-"),
            Value::NIL,
            Value::string(".tmp"),
            Value::string("payload"),
        ],
    )
    .expect("builtin make-temp-file-internal should resolve")
    .expect("builtin make-temp-file-internal should evaluate");
    let path = created
        .as_utf8_str()
        .expect("make-temp-file-internal should return file path");
    assert!(path.contains("neovm-mtfi-"));
    assert!(path.ends_with(".tmp"));
    assert_eq!(path.len(), "neovm-mtfi-".len() + 6 + ".tmp".len());
    assert!(std::path::Path::new(path).exists());
    assert_eq!(fs::read_to_string(path).expect("read temp file"), "payload");
    std::fs::remove_file(path).expect("temp file should be removable");

    let name_only = dispatch_builtin_pure(
        "make-temp-file-internal",
        vec![
            Value::string("neovm-mtfi-name-"),
            Value::fixnum(0),
            Value::string(".tmp"),
            Value::NIL,
        ],
    )
    .expect("builtin make-temp-file-internal should resolve")
    .expect("make-temp-file-internal should generate a name without creating it");
    let name_path = name_only
        .as_utf8_str()
        .expect("make-temp-file-internal should return file path");
    assert_eq!(name_path.len(), "neovm-mtfi-name-".len() + 6 + ".tmp".len());
    assert!(!std::path::Path::new(name_path).exists());
}

#[test]
fn pure_dispatch_minibuffer_and_frame_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    assert!(
        dispatch_builtin_pure("minibuffer-prompt-end", vec![]).is_none(),
        "minibuffer-prompt-end should use eval-aware minibuffer state"
    );

    for (name, args) in vec![
        ("next-frame", vec![]),
        ("next-frame", vec![Value::NIL, Value::NIL]),
        ("previous-frame", vec![]),
        ("previous-frame", vec![Value::NIL, Value::NIL]),
        ("raise-frame", vec![]),
        ("raise-frame", vec![Value::NIL]),
        ("suspend-emacs", vec![]),
        ("suspend-emacs", vec![Value::string("hold")]),
    ] {
        let value = dispatch_builtin_pure(name, args)
            .expect("builtin placeholder should resolve")
            .expect("builtin placeholder should evaluate");
        assert!(value.is_nil(), "{name} should return nil");
    }

    // vertical-motion needs buffer state — correctly eval-backed.
    assert!(dispatch_builtin_pure("vertical-motion", vec![Value::fixnum(3)]).is_none());

    let redisplay = dispatch_builtin_pure("redisplay", vec![])
        .expect("builtin redisplay should resolve")
        .expect("builtin redisplay should evaluate");
    assert_eq!(redisplay, Value::T);

    let redisplay_force = dispatch_builtin_pure("redisplay", vec![Value::T])
        .expect("builtin redisplay should resolve with optional arg")
        .expect("builtin redisplay should evaluate with optional arg");
    assert_eq!(redisplay_force, Value::T);
}

#[test]
fn pure_dispatch_buffer_placeholder_mutators_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    // rename-buffer is now an eval builtin — test it through the evaluator
    {
        let mut eval = crate::emacs_core::eval::Context::new();
        let buf_id = eval.buffers.create_buffer("old-name");
        eval.buffers.set_current(buf_id);
        let renamed = dispatch_builtin(&mut eval, "rename-buffer", vec![Value::string("new-name")])
            .expect("builtin rename-buffer should resolve")
            .expect("builtin rename-buffer should evaluate");
        assert_eq!(renamed, Value::string("new-name"));
        assert_eq!(
            eval.buffers.get(buf_id).unwrap().name_value(),
            Value::string("new-name")
        );
        assert_eq!(
            builtin_buffer_last_name(&mut eval, vec![Value::make_buffer(buf_id)]).unwrap(),
            Value::string("old-name")
        );
    }

    {
        let mut eval = crate::emacs_core::eval::Context::new();
        let redisplay = dispatch_builtin(
            &mut eval,
            "set-buffer-redisplay",
            vec![
                Value::NIL,
                Value::fixnum(1),
                Value::fixnum(1),
                Value::fixnum(0),
            ],
        )
        .expect("builtin set-buffer-redisplay should resolve")
        .expect("builtin set-buffer-redisplay should evaluate");
        assert!(redisplay.is_nil());
    }
}

#[test]
fn rename_buffer_crosses_the_global_mode_line_menu_boundary_while_offscreen() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let visible = eval.buffers.create_buffer("rename-visible");
    eval.frames.create_frame("F1", 800, 600, visible);
    let offscreen = eval.buffers.create_buffer("rename-offscreen");
    eval.buffers.set_current(offscreen);

    let initial = eval.menu_bar_rebuild_generation();
    dispatch_builtin(
        &mut eval,
        "rename-buffer",
        vec![Value::string("rename-offscreen-new")],
    )
    .expect("rename-buffer should resolve")
    .expect("rename-buffer should rename an offscreen buffer");

    // GNU `Frename_buffer` calls `bset_update_mode_line` after a real rename.
    // Unlike the public local `force-mode-line-update` operation, that helper
    // unconditionally raises global `update_mode_lines`, even when the renamed
    // buffer is not displayed yet (src/buffer.c, src/xdisp.c).
    assert_ne!(eval.menu_bar_rebuild_generation(), initial);
}

#[test]
fn rename_and_generate_buffer_names_preserve_lisp_string_identity_and_properties_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let current_id = eval.buffers.create_buffer("origin");
    eval.buffers.set_current(current_id);

    let free_name = Value::string("named");
    let mut free_properties = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut free_properties,
        0,
        5,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(
        free_name,
        free_properties,
    );

    let generated = builtin_generate_new_buffer_name(&mut eval, vec![free_name])
        .expect("an unused buffer name should be accepted");
    assert!(
        crate::emacs_core::value::eq_value(&free_name, &generated),
        "GNU returns the supplied string object when the name is unused"
    );

    let renamed = dispatch_builtin(&mut eval, "rename-buffer", vec![free_name])
        .expect("rename-buffer should resolve")
        .expect("rename-buffer should accept a propertized name");
    assert!(
        crate::emacs_core::value::eq_value(&free_name, &renamed),
        "GNU returns the exact supplied string for a free name"
    );
    assert!(crate::emacs_core::value::eq_value(
        &free_name,
        &eval
            .buffers
            .get(current_id)
            .expect("current buffer")
            .name_value()
    ));

    let equal_but_distinct_name = Value::string("named");
    let same = dispatch_builtin(&mut eval, "rename-buffer", vec![equal_but_distinct_name])
        .expect("rename-buffer should resolve")
        .expect("renaming to the current text should succeed");
    assert!(
        crate::emacs_core::value::eq_value(&free_name, &same),
        "GNU returns the already-stored name object for a non-unique no-op"
    );

    let occupied_id = eval.buffers.create_buffer("taken");
    assert_ne!(occupied_id, current_id);
    let colliding_name = Value::string("taken");
    let mut colliding_properties = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut colliding_properties,
        0,
        5,
        Value::symbol("face"),
        Value::symbol("italic"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(
        colliding_name,
        colliding_properties,
    );

    let unique = dispatch_builtin(&mut eval, "rename-buffer", vec![colliding_name, Value::T])
        .expect("rename-buffer should resolve")
        .expect("unique rename should resolve a collision");
    assert_eq!(unique.as_utf8_str(), Some("taken<2>"));
    let properties = crate::emacs_core::value::get_string_text_properties_table_for_value(unique)
        .expect("the original name span should retain its face");
    assert_eq!(
        properties.get_property_at_char_pos(crate::buffer::CharPos0::new(0), Value::symbol("face")),
        Some(Value::symbol("italic"))
    );
    assert_eq!(
        properties.get_property_at_char_pos(crate::buffer::CharPos0::new(5), Value::symbol("face")),
        None,
        "GNU does not extend the source name's property over the generated suffix"
    );
    assert!(crate::emacs_core::value::eq_value(
        &unique,
        &eval
            .buffers
            .get(current_id)
            .expect("renamed buffer")
            .name_value()
    ));
}

#[test]
fn eval_dispatch_set_buffer_major_mode_runs_default_mode_function() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::load::create_bootstrap_evaluator_cached;

    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    eval.eval_str(
        r#"(progn
             (defun sbmm-test-mode ()
               (setq-local sbmm-ran t))
             (set-default 'major-mode 'sbmm-test-mode))"#,
    )
    .expect("define synthetic major mode");

    let buf_id = eval.buffers.create_buffer("sbmm-target");
    let result = dispatch_builtin(
        &mut eval,
        "set-buffer-major-mode",
        vec![Value::make_buffer(buf_id)],
    )
    .expect("builtin set-buffer-major-mode should resolve")
    .expect("builtin set-buffer-major-mode should evaluate");
    assert!(result.is_nil());

    eval.buffers.set_current(buf_id);
    assert_eq!(format_eval_result(&eval.eval_str("sbmm-ran")), "OK t");
}

#[test]
fn bootstrap_set_buffer_major_mode_runs_fundamental_mode_file_locals() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::load::create_bootstrap_evaluator_cached;

    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = format_eval_result(&eval.eval_str(
        r#"(let ((buf (get-buffer-create " *sbmm-file-locals*")))
             (with-current-buffer buf
               (erase-buffer)
               (setq buffer-file-name
                     (expand-file-name "sbmm-file-locals.txt" temporary-file-directory))
               (setq default-directory temporary-file-directory)
               (insert "hello\n\n;; Local Variables:\n;; tab-width: 42\n;; End:\n")
               (kill-all-local-variables)
               (set-buffer-major-mode buf)
               (list major-mode tab-width
                     (local-variable-p 'tab-width (current-buffer)))))"#,
    ));
    assert_eq!(result, "OK (fundamental-mode 42 t)");
}

#[test]
fn pure_dispatch_unicode_and_re_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    // GNU's `Fput_unicode_property_internal` runs `CHECK_CHAR_TABLE` first, so a
    // nil CHAR-TABLE signals `(wrong-type-argument char-table-p nil)`.
    let unicode_err = dispatch_builtin_pure(
        "put-unicode-property-internal",
        vec![Value::NIL, Value::fixnum(0), Value::fixnum(1)],
    )
    .expect("builtin put-unicode-property-internal should resolve")
    .expect_err("nil char-table should signal wrong-type-argument");
    match unicode_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("char-table-p"), Value::NIL]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    let re_default = dispatch_builtin_pure("re--describe-compiled", vec![Value::string("x")])
        .expect("builtin re--describe-compiled should resolve")
        .expect("builtin re--describe-compiled should evaluate");
    assert!(re_default.is_nil());

    let re_indent = dispatch_builtin_pure(
        "re--describe-compiled",
        vec![Value::string("x"), Value::fixnum(2)],
    )
    .expect("builtin re--describe-compiled should resolve with indent")
    .expect("builtin re--describe-compiled should evaluate with indent");
    assert!(re_indent.is_nil());
}

#[test]
fn pure_dispatch_map_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let map_charset_chars = dispatch_builtin_pure(
        "map-charset-chars",
        vec![Value::NIL, Value::symbol("unicode"), Value::NIL],
    )
    .expect("builtin map-charset-chars should resolve");
    match map_charset_chars {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "void-function"),
        other => {
            panic!("map-charset-chars with nil callback should signal void-function: {other:?}")
        }
    }

    // These correctly return None from pure dispatch.
    assert!(
        dispatch_builtin_pure(
            "map-keymap",
            vec![Value::NIL, Value::list(vec![Value::symbol("keymap")])],
        )
        .is_none()
    );

    assert!(
        dispatch_builtin_pure(
            "map-keymap-internal",
            vec![Value::NIL, Value::list(vec![Value::symbol("keymap")])],
        )
        .is_none()
    );

    assert!(dispatch_builtin_pure("mapbacktrace", vec![Value::symbol("ignore")]).is_none());
}

#[test]
fn pure_dispatch_record_and_state_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let make_record = dispatch_builtin_pure(
        "make-record",
        vec![Value::symbol("tag"), Value::fixnum(0), Value::fixnum(0)],
    )
    .expect("builtin make-record should resolve")
    .expect("builtin make-record should evaluate");
    assert!(make_record.is_record());

    let marker_last_position = dispatch_builtin_pure(
        "marker-last-position",
        vec![crate::emacs_core::marker::make_marker_value(
            None, None, false,
        )],
    )
    .expect("builtin marker-last-position should resolve")
    .expect("builtin marker-last-position should evaluate");
    assert_eq!(marker_last_position, Value::fixnum(0));

    // match-data--translate is now dispatched in eval path (needs &mut eval)
    assert!(dispatch_builtin_pure("match-data--translate", vec![Value::fixnum(1)]).is_none());

    let newline_cache_check = dispatch_builtin_pure("newline-cache-check", vec![])
        .expect("builtin newline-cache-check should resolve")
        .expect("builtin newline-cache-check should evaluate");
    assert!(newline_cache_check.is_nil());

    let old_selected_frame = dispatch_builtin_pure("old-selected-frame", vec![])
        .expect("builtin old-selected-frame should resolve")
        .expect("builtin old-selected-frame should evaluate");
    assert!(old_selected_frame.is_nil());
}

#[test]
fn pure_dispatch_frame_menu_mouse_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    // make-frame-invisible mutates live frame state, so it must stay on the
    // eval-state dispatch path rather than the pure placeholder path.
    assert!(dispatch_builtin_pure("make-frame-invisible", vec![Value::NIL, Value::T]).is_none());

    let menu_at = dispatch_builtin_pure(
        "menu-bar-menu-at-x-y",
        vec![Value::fixnum(0), Value::fixnum(0), Value::NIL],
    )
    .expect("builtin menu-bar-menu-at-x-y should resolve")
    .expect("builtin menu-bar-menu-at-x-y should evaluate");
    assert!(menu_at.is_nil());

    let menu_active = dispatch_builtin_pure("menu-or-popup-active-p", vec![])
        .expect("builtin menu-or-popup-active-p should resolve")
        .expect("builtin menu-or-popup-active-p should evaluate");
    assert!(menu_active.is_nil());
}

#[test]
fn pure_dispatch_native_comp_surface_matches_no_native_comp_build() {
    crate::test_utils::init_test_tracing();
    let available = dispatch_builtin_pure("native-comp-available-p", vec![])
        .expect("builtin native-comp-available-p should resolve")
        .expect("builtin native-comp-available-p should evaluate");
    assert!(available.is_nil());

    assert!(dispatch_builtin_pure("native-comp-unit-file", vec![Value::NIL]).is_none());
    assert!(dispatch_builtin_pure("native-comp-unit-set-file", vec![Value::NIL]).is_none());

    // Ledger 190: this test's own name is now true.  GNU's syms_of_comp
    // registers fourteen subrs inside `#ifdef HAVE_NATIVE_COMP`
    // (src/comp.c:5693-5706) and exactly one outside it,
    // `native-comp-available-p` at :5828 -- so a build that answers nil to
    // that, as this one and the reference GNU both do, declares no others.
    // The block that used to pin `native-elisp-load`'s error now pins its
    // absence, along with its twelve `comp` siblings.
    for inside_have_native_comp in [
        "native-elisp-load",
        "comp--compile-ctxt-to-file0",
        "comp--init-ctxt",
        "comp--install-trampoline",
        "comp--late-register-subr",
        "comp--register-lambda",
        "comp--register-subr",
        "comp--release-ctxt",
        "comp--subr-signature",
        "comp-el-to-eln-filename",
        "comp-el-to-eln-rel-filename",
        "comp-native-compiler-options-effective-p",
        "comp-native-driver-options-effective-p",
    ] {
        assert!(
            dispatch_builtin_pure(inside_have_native_comp, vec![Value::NIL]).is_none(),
            "{inside_have_native_comp} must not resolve: it is inside GNU's \
             #ifdef HAVE_NATIVE_COMP and native-comp-available-p is nil here"
        );
    }

    super::symbols::reset_symbols_thread_locals();
    assert!(
        dispatch_builtin_pure("new-fontset", vec![Value::string("x"), Value::string("y")])
            .is_none(),
        "new-fontset now requires evaluator runtime state and must bypass pure dispatch"
    );
}

#[test]
fn pure_dispatch_open_overlay_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let open_font = dispatch_builtin_pure(
        "open-font",
        vec![Value::vector(vec![Value::keyword("font-entity")])],
    )
    .expect("builtin open-font should resolve")
    .expect("builtin open-font should evaluate");
    assert!(open_font.is_nil());

    let open_dribble = dispatch_builtin_pure("open-dribble-file", vec![Value::string("x.log")])
        .expect("builtin open-dribble-file should resolve")
        .expect("builtin open-dribble-file should evaluate");
    assert!(open_dribble.is_nil());

    let intervals = dispatch_builtin_pure("object-intervals", vec![Value::string("x")])
        .expect("builtin object-intervals should resolve")
        .expect("builtin object-intervals should evaluate");
    assert!(intervals.is_nil());

    let char_table =
        crate::emacs_core::chartable::make_char_table_value(Value::symbol("test-only"), Value::NIL);

    let optimized = dispatch_builtin_pure(
        "optimize-char-table",
        vec![char_table, Value::symbol("test-only")],
    )
    .expect("builtin optimize-char-table should resolve")
    .expect("builtin optimize-char-table should evaluate");
    assert!(optimized.is_nil());

    // GNU `Foverlay_lists` always returns the `(BEFORE . AFTER)` pair; with no
    // overlays that pair is `(nil)` (`(cons nil nil)`), never bare `nil`.
    let overlays = dispatch_builtin_pure("overlay-lists", vec![])
        .expect("builtin overlay-lists should resolve")
        .expect("builtin overlay-lists should evaluate");
    assert!(overlays.is_cons());
    assert!(overlays.cons_car().is_nil());
    assert!(overlays.cons_cdr().is_nil());

    let recentered = dispatch_builtin_pure("overlay-recenter", vec![Value::fixnum(0)])
        .expect("builtin overlay-recenter should resolve")
        .expect("builtin overlay-recenter should evaluate");
    assert!(recentered.is_nil());
}

#[test]
fn pure_dispatch_profiler_primitives_collect_cpu_and_memory_logs() {
    crate::test_utils::init_test_tracing();
    let cpu_log = dispatch_builtin_pure("profiler-cpu-log", vec![])
        .expect("builtin profiler-cpu-log should resolve")
        .expect("builtin profiler-cpu-log should evaluate");
    assert!(cpu_log.is_nil());

    let cpu_running = dispatch_builtin_pure("profiler-cpu-running-p", vec![])
        .expect("builtin profiler-cpu-running-p should resolve")
        .expect("builtin profiler-cpu-running-p should evaluate");
    assert!(cpu_running.is_nil());

    let cpu_start = dispatch_builtin_pure("profiler-cpu-start", vec![Value::fixnum(1)])
        .expect("builtin profiler-cpu-start should resolve")
        .expect("builtin profiler-cpu-start should evaluate");
    assert!(cpu_start.is_truthy());

    let cpu_running = dispatch_builtin_pure("profiler-cpu-running-p", vec![])
        .expect("builtin profiler-cpu-running-p should resolve")
        .expect("builtin profiler-cpu-running-p should evaluate");
    assert!(cpu_running.is_truthy());

    let cpu_second_start = dispatch_builtin_pure("profiler-cpu-start", vec![Value::fixnum(1)])
        .expect("builtin profiler-cpu-start should resolve");
    assert!(cpu_second_start.is_err());

    for value in 0..10_000 {
        std::hint::black_box(value * value);
    }

    let cpu_stop = dispatch_builtin_pure("profiler-cpu-stop", vec![])
        .expect("builtin profiler-cpu-stop should resolve")
        .expect("builtin profiler-cpu-stop should evaluate");
    assert!(cpu_stop.is_truthy());

    let cpu_log = dispatch_builtin_pure("profiler-cpu-log", vec![])
        .expect("builtin profiler-cpu-log should resolve")
        .expect("builtin profiler-cpu-log should evaluate");
    assert!(cpu_log.is_hash_table());
    assert!(!cpu_log.as_hash_table().unwrap().data.is_empty());

    let cpu_log = dispatch_builtin_pure("profiler-cpu-log", vec![])
        .expect("builtin profiler-cpu-log should resolve")
        .expect("builtin profiler-cpu-log should evaluate");
    assert!(cpu_log.is_nil());

    let invalid_cpu_start = dispatch_builtin_pure("profiler-cpu-start", vec![Value::fixnum(0)])
        .expect("builtin profiler-cpu-start should resolve");
    assert!(invalid_cpu_start.is_err());

    let mem_log = dispatch_builtin_pure("profiler-memory-log", vec![])
        .expect("builtin profiler-memory-log should resolve")
        .expect("builtin profiler-memory-log should evaluate");
    assert!(mem_log.is_nil());

    let _ = dispatch_builtin_pure("profiler-memory-stop", vec![]);

    let mem_running = dispatch_builtin_pure("profiler-memory-running-p", vec![])
        .expect("builtin profiler-memory-running-p should resolve")
        .expect("builtin profiler-memory-running-p should evaluate");
    assert!(mem_running.is_nil());

    let mem_start = dispatch_builtin_pure("profiler-memory-start", vec![])
        .expect("builtin profiler-memory-start should resolve")
        .expect("builtin profiler-memory-start should evaluate");
    assert!(mem_start.is_truthy());

    let allocated = Value::list((0..256).map(Value::fixnum).collect());
    std::hint::black_box(allocated);

    let mem_running = dispatch_builtin_pure("profiler-memory-running-p", vec![])
        .expect("builtin profiler-memory-running-p should resolve")
        .expect("builtin profiler-memory-running-p should evaluate");
    assert!(mem_running.is_truthy());

    let mem_second_start = dispatch_builtin_pure("profiler-memory-start", vec![])
        .expect("builtin profiler-memory-start should resolve");
    assert!(mem_second_start.is_err());

    let mem_stop = dispatch_builtin_pure("profiler-memory-stop", vec![])
        .expect("builtin profiler-memory-stop should resolve")
        .expect("builtin profiler-memory-stop should evaluate");
    assert!(mem_stop.is_truthy());

    let mem_log = dispatch_builtin_pure("profiler-memory-log", vec![])
        .expect("builtin profiler-memory-log should resolve")
        .expect("builtin profiler-memory-log should evaluate");
    assert!(mem_log.is_hash_table());
    assert!(!mem_log.as_hash_table().unwrap().data.is_empty());

    let mem_log = dispatch_builtin_pure("profiler-memory-log", vec![])
        .expect("builtin profiler-memory-log should resolve")
        .expect("builtin profiler-memory-log should evaluate");
    assert!(mem_log.is_nil());

    let mem_stop = dispatch_builtin_pure("profiler-memory-stop", vec![])
        .expect("builtin profiler-memory-stop should resolve")
        .expect("builtin profiler-memory-stop should evaluate");
    assert!(mem_stop.is_nil());
}

#[test]
fn pure_dispatch_position_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let pdumper = dispatch_builtin_pure("pdumper-stats", vec![])
        .expect("builtin pdumper-stats should resolve")
        .expect("builtin pdumper-stats should evaluate");
    assert!(pdumper.is_nil());

    let position_symbol = dispatch_builtin_pure(
        "position-symbol",
        vec![Value::symbol("x"), Value::fixnum(42)],
    )
    .expect("builtin position-symbol should resolve")
    .expect("builtin position-symbol should evaluate");
    assert!(position_symbol.is_symbol_with_pos());

    let posn_at_point = dispatch_builtin_pure("posn-at-point", vec![])
        .expect("builtin posn-at-point should resolve")
        .expect("builtin posn-at-point should evaluate");
    assert!(posn_at_point.is_nil());

    let posn_at_xy = dispatch_builtin_pure("posn-at-x-y", vec![Value::fixnum(0), Value::fixnum(0)])
        .expect("builtin posn-at-x-y should resolve")
        .expect("builtin posn-at-x-y should evaluate");
    assert!(posn_at_xy.is_nil());

    let play_sound_err = dispatch_builtin_pure("play-sound-internal", vec![Value::NIL])
        .expect("builtin play-sound-internal should resolve")
        .expect_err("play-sound-internal should reject nil as invalid spec");
    match play_sound_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn pure_dispatch_record_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let record = dispatch_builtin_pure("record", vec![Value::symbol("tag"), Value::fixnum(1)])
        .expect("builtin record should resolve")
        .expect("builtin record should evaluate");
    assert!(record.is_record());

    let record_arity = dispatch_builtin_pure("record", vec![])
        .expect("builtin record should resolve")
        .expect_err("record should reject empty slot lists");
    match record_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data, vec![Value::symbol("record"), Value::fixnum(0)]);
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    let recordp = dispatch_builtin_pure("recordp", vec![Value::NIL])
        .expect("builtin recordp should resolve")
        .expect("builtin recordp should evaluate");
    assert!(recordp.is_nil());

    super::symbols::reset_symbols_thread_locals();
    let query_fontset =
        dispatch_builtin_pure("query-fontset", vec![Value::string("fontset-default")])
            .expect("builtin query-fontset should resolve")
            .expect("builtin query-fontset should evaluate");
    assert_eq!(
        query_fontset,
        Value::string("-*-*-*-*-*-*-*-*-*-*-*-*-fontset-default")
    );

    let read_pos = dispatch_builtin_pure("read-positioning-symbols", vec![Value::string("x")])
        .expect("builtin read-positioning-symbols should resolve")
        .expect("builtin read-positioning-symbols should evaluate");
    assert!(read_pos.is_symbol_with_pos());
    assert_eq!(
        read_pos.as_symbol_with_pos_sym().unwrap(),
        Value::symbol("x")
    );

    let recent_auto_save = dispatch_builtin_pure("recent-auto-save-p", vec![])
        .expect("builtin recent-auto-save-p should resolve")
        .expect("builtin recent-auto-save-p should evaluate");
    assert!(recent_auto_save.is_nil());
}

#[test]
fn query_fontset_matches_emacs_regexp_syntax() {
    crate::test_utils::init_test_tracing();
    super::symbols::reset_symbols_thread_locals();

    // `[([]` is the character class emitted by GNU `rx` for `[` or `(`.
    // The second Emacs-regexp alternative matches the default fontset.
    let query_fontset = dispatch_builtin_pure(
        "query-fontset",
        vec![Value::string("[([]\\|fontset-default"), Value::T],
    )
    .expect("builtin query-fontset should resolve")
    .expect("valid Emacs regexp should evaluate");

    assert_eq!(
        query_fontset,
        Value::string("-*-*-*-*-*-*-*-*-*-*-*-*-fontset-default")
    );
}

#[test]
fn pure_dispatch_reconsider_redirect_placeholders_match_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let reconsider = dispatch_builtin_pure("reconsider-frame-fonts", vec![Value::NIL])
        .expect("builtin reconsider-frame-fonts should resolve")
        .expect_err("reconsider-frame-fonts should require a window system frame");
    match reconsider {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    let redirect_dbg = dispatch_builtin_pure("redirect-debugging-output", vec![Value::NIL])
        .expect("builtin redirect-debugging-output should resolve")
        .expect("builtin redirect-debugging-output should evaluate");
    assert!(redirect_dbg.is_nil());

    let remove_pos = dispatch_builtin_pure("remove-pos-from-symbol", vec![Value::symbol("x")])
        .expect("builtin remove-pos-from-symbol should resolve")
        .expect("builtin remove-pos-from-symbol should evaluate");
    assert_eq!(remove_pos, Value::symbol("x"));

    // Wrong-type argument (not a window) → wrong-type-argument signal.
    let resize_mini_bad_type =
        dispatch_builtin_pure("resize-mini-window-internal", vec![Value::fixnum(42)])
            .expect("builtin resize-mini-window-internal should resolve")
            .expect_err("resize-mini-window-internal should reject non-window args");
    match resize_mini_bad_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
        }
        other => panic!("expected signal, got: {other:?}"),
    }
    // Valid window handle that is not present in the synthesized batch frame.
    let resize_mini_no_frame = dispatch_builtin_pure(
        "resize-mini-window-internal",
        vec![Value::make_window(999_999)],
    )
    .expect("builtin resize-mini-window-internal should resolve")
    .expect_err("resize-mini-window-internal should signal when window has no frame");
    match resize_mini_no_frame {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data.len(), 1);
            assert_eq!(
                sig.data[0].as_runtime_string_owned().as_deref(),
                Some("Window not found")
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    let redirect_focus = dispatch_builtin_pure("redirect-frame-focus", vec![Value::NIL])
        .expect("builtin redirect-frame-focus should resolve")
        .expect("builtin redirect-frame-focus should evaluate");
    assert!(redirect_focus.is_nil());

    let restore_modified = dispatch_builtin_pure("restore-buffer-modified-p", vec![Value::NIL])
        .expect("builtin restore-buffer-modified-p should resolve")
        .expect("builtin restore-buffer-modified-p should evaluate");
    assert!(restore_modified.is_nil());

    let mut eval = crate::emacs_core::eval::Context::new();
    let set_command_keys = builtin_set_this_command_keys(&mut eval, vec![Value::string("x")])
        .expect("builtin set--this-command-keys should evaluate");
    assert!(set_command_keys.is_nil());
    assert_eq!(eval.read_command_keys(), &[Value::fixnum('x' as i64)]);

    let set_auto_saved = dispatch_builtin_pure("set-buffer-auto-saved", vec![])
        .expect("builtin set-buffer-auto-saved should resolve")
        .expect("builtin set-buffer-auto-saved should evaluate");
    assert!(set_auto_saved.is_nil());
}

#[test]
fn reconsider_frame_fonts_accepts_live_window_system_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.set_parameter(
        Value::symbol("font-parameter"),
        Value::string("monospace-12"),
    );

    let result = dispatch_builtin(
        &mut eval,
        "reconsider-frame-fonts",
        vec![Value::make_frame(frame_id.0)],
    )
    .expect("builtin reconsider-frame-fonts should resolve")
    .expect("window-system frame should be accepted");

    assert!(result.is_nil());
}

#[test]
fn pure_dispatch_set_window_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let set_charset = dispatch_builtin_pure(
        "set-charset-plist",
        vec![Value::symbol("unicode"), Value::list(vec![])],
    )
    .expect("builtin set-charset-plist should resolve")
    .expect("builtin set-charset-plist should evaluate");
    assert_eq!(set_charset, Value::list(vec![]));

    assert!(
        dispatch_builtin_pure(
            "set-fontset-font",
            vec![
                Value::string("fontset-default"),
                Value::symbol("target"),
                Value::NIL,
            ],
        )
        .is_none()
    );

    let set_state = dispatch_builtin_pure("set-frame-window-state-change", vec![])
        .expect("builtin set-frame-window-state-change should resolve")
        .expect("builtin set-frame-window-state-change should evaluate");
    assert!(set_state.is_nil());

    let set_fringe = dispatch_builtin_pure(
        "set-fringe-bitmap-face",
        vec![Value::symbol("left-triangle")],
    )
    .expect("builtin set-fringe-bitmap-face should resolve")
    .expect("builtin set-fringe-bitmap-face should evaluate");
    assert!(set_fringe.is_nil());

    // set-minibuffer-window now detects the minibuffer structurally (per
    // frame's `minibuffer_window`) rather than via a magic id range. In the
    // frame-less pure-dispatch harness no window is a minibuffer window, so a
    // live window is rejected.
    let set_mini = dispatch_builtin_pure("set-minibuffer-window", vec![Value::make_window(1)])
        .expect("builtin set-minibuffer-window should resolve")
        .expect_err("set-minibuffer-window should reject a non-minibuffer window");
    match set_mini {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window is not a minibuffer window")]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    let set_combination = dispatch_builtin_pure(
        "set-window-combination-limit",
        vec![Value::make_window(1), Value::NIL],
    )
    .expect("builtin set-window-combination-limit should resolve")
    .expect_err("set-window-combination-limit should reject leaf windows");
    match set_combination {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Combination limit is meaningful for internal windows only",
                )]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    let set_new_normal = dispatch_builtin_pure("set-window-new-normal", vec![Value::NIL])
        .expect("builtin set-window-new-normal should resolve")
        .expect("builtin set-window-new-normal should evaluate");
    assert!(set_new_normal.is_nil());

    let set_new_pixel = dispatch_builtin_pure(
        "set-window-new-pixel",
        vec![Value::NIL, Value::fixnum(1), Value::fixnum(2)],
    )
    .expect("builtin set-window-new-pixel should resolve")
    .expect("builtin set-window-new-pixel should evaluate");
    assert_eq!(set_new_pixel, Value::fixnum(1));

    let set_new_total = dispatch_builtin_pure(
        "set-window-new-total",
        vec![Value::NIL, Value::fixnum(1), Value::fixnum(2)],
    )
    .expect("builtin set-window-new-total should resolve")
    .expect("builtin set-window-new-total should evaluate");
    assert_eq!(set_new_total, Value::fixnum(1));
}

#[test]
fn pure_dispatch_sort_subr_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let sort_charsets = dispatch_builtin_pure("sort-charsets", vec![Value::list(vec![])])
        .expect("builtin sort-charsets should resolve")
        .expect("builtin sort-charsets should evaluate");
    assert!(sort_charsets.is_nil());

    // GNU `Fsplit_char` returns the charset name and the position-codes of the
    // character: `(split-char ?A)` => `(ascii 65)`.
    let split_char = dispatch_builtin_pure("split-char", vec![Value::fixnum(65)])
        .expect("builtin split-char should resolve")
        .expect("builtin split-char should evaluate");
    assert_eq!(
        split_char,
        Value::list(vec![Value::symbol("ascii"), Value::fixnum(65)])
    );

    let string_distance = dispatch_builtin_pure(
        "string-distance",
        vec![Value::string("a"), Value::string("b")],
    )
    .expect("builtin string-distance should resolve")
    .expect("builtin string-distance should evaluate");
    assert_eq!(string_distance, Value::fixnum(1));

    assert!(dispatch_builtin_pure("subr-native-comp-unit", vec![Value::NIL]).is_none());

    let subr_lambda_list = dispatch_builtin_pure("subr-native-lambda-list", vec![Value::NIL])
        .expect("builtin subr-native-lambda-list should resolve")
        .expect("builtin subr-native-lambda-list should evaluate");
    assert!(subr_lambda_list.is_nil());

    let subr_type = dispatch_builtin_pure("subr-type", vec![Value::NIL])
        .expect("builtin subr-type should resolve")
        .expect("builtin subr-type should evaluate");
    assert!(subr_type.is_nil());

    let single_keys = dispatch_builtin_pure("this-single-command-keys", vec![])
        .expect("builtin this-single-command-keys should resolve")
        .expect("builtin this-single-command-keys should evaluate");
    assert!(single_keys.is_nil());

    let single_raw_keys = dispatch_builtin_pure("this-single-command-raw-keys", vec![])
        .expect("builtin this-single-command-raw-keys should resolve")
        .expect("builtin this-single-command-raw-keys should evaluate");
    assert!(single_raw_keys.is_nil());
}

#[test]
fn pure_dispatch_tty_tool_bar_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let thread_blocker = dispatch_builtin_pure("thread--blocker", vec![Value::NIL])
        .expect("builtin thread--blocker should resolve")
        .expect("builtin thread--blocker should evaluate");
    assert!(thread_blocker.is_nil());

    let tool_bar_style = dispatch_builtin_pure("tool-bar-get-system-style", vec![])
        .expect("builtin tool-bar-get-system-style should resolve")
        .expect("builtin tool-bar-get-system-style should evaluate");
    assert!(tool_bar_style.is_nil());

    let tool_bar_width = dispatch_builtin_pure("tool-bar-pixel-width", vec![])
        .expect("builtin tool-bar-pixel-width should resolve")
        .expect("builtin tool-bar-pixel-width should evaluate");
    assert_eq!(tool_bar_width, Value::fixnum(0));

    assert!(
        dispatch_builtin_pure(
            "transpose-regions",
            vec![
                Value::fixnum(1),
                Value::fixnum(2),
                Value::fixnum(3),
                Value::fixnum(4),
                Value::NIL,
            ],
        )
        .is_none(),
        "transpose-regions needs the current buffer, matching GNU editfns.c"
    );

    let tty_buf = dispatch_builtin_pure("tty--output-buffer-size", vec![])
        .expect("builtin tty--output-buffer-size should resolve")
        .expect("builtin tty--output-buffer-size should evaluate");
    assert_eq!(tty_buf, Value::fixnum(0));

    let tty_set = dispatch_builtin_pure("tty--set-output-buffer-size", vec![Value::fixnum(4096)])
        .expect("builtin tty--set-output-buffer-size should resolve")
        .expect("builtin tty--set-output-buffer-size should evaluate");
    assert!(tty_set.is_nil());

    let tty_suppress =
        dispatch_builtin_pure("tty-suppress-bold-inverse-default-colors", vec![Value::NIL])
            .expect("builtin tty-suppress-bold-inverse-default-colors should resolve")
            .expect("builtin tty-suppress-bold-inverse-default-colors should evaluate");
    assert!(tty_suppress.is_nil());
}

#[test]
fn transpose_regions_moves_text_properties_and_markers_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval
        .eval_str_each(
            r#"(insert "abcdef")
               (put-text-property 1 3 'face 'r1)
               (put-text-property 5 7 'face 'r2)
               (let ((m1 (copy-marker 2))
                     (m2 (copy-marker 6)))
                 (transpose-regions 1 3 5 7 nil)
                 (list (buffer-string)
                       (marker-position m1)
                       (marker-position m2)
                       (text-properties-at 1)
                       (text-properties-at 2)
                       (text-properties-at 3)
                       (text-properties-at 4)
                       (text-properties-at 5)
                       (text-properties-at 6)))"#,
        )
        .into_iter()
        .last()
        .expect("one form")
        .expect("evaluation succeeds");

    assert_eq!(
        format_eval_result(&Ok(result)),
        r#"OK (#("efcdab" 0 2 (face r2) 4 6 (face r1)) 6 2 (face r2) (face r2) nil nil (face r1) (face r1))"#
    );
}

#[test]
fn pure_dispatch_unicode_value_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    // unencodable-char-position is no longer a nil-returning placeholder: it now
    // validates its region like GNU. In the pure-dispatch path (no buffer) the
    // requested region 1..2 is out of range, so it signals args-out-of-range
    // rather than silently returning nil.
    let unencodable = dispatch_builtin_pure(
        "unencodable-char-position",
        vec![Value::fixnum(1), Value::fixnum(2), Value::symbol("utf-8")],
    )
    .expect("builtin unencodable-char-position should resolve");
    assert!(
        unencodable.is_err(),
        "unencodable-char-position should signal on an out-of-range region, got {unencodable:?}"
    );

    let unicode_table = dispatch_builtin_pure(
        "unicode-property-table-internal",
        vec![Value::symbol("foo")],
    )
    .expect("builtin unicode-property-table-internal should resolve")
    .expect("builtin unicode-property-table-internal should evaluate");
    assert!(unicode_table.is_nil());

    let unify = dispatch_builtin_pure("unify-charset", vec![Value::symbol("eight-bit")])
        .expect("builtin unify-charset should resolve")
        .expect("builtin unify-charset should evaluate");
    assert!(unify.is_nil());

    let unix_sync = dispatch_builtin_pure("unix-sync", vec![])
        .expect("builtin unix-sync should resolve")
        .expect("builtin unix-sync should evaluate");
    assert!(unix_sync.is_nil());

    let value_lt = dispatch_builtin_pure("value<", vec![Value::fixnum(1), Value::fixnum(2)])
        .expect("builtin value< should resolve")
        .expect("builtin value< should evaluate");
    assert_eq!(value_lt, Value::T);

    let binding_locus = dispatch_builtin_pure("variable-binding-locus", vec![Value::symbol("x")])
        .expect("builtin variable-binding-locus should resolve")
        .expect("builtin variable-binding-locus should evaluate");
    assert!(binding_locus.is_nil());
}

#[test]
fn value_lt_accepts_raw_unibyte_strings() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let low = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFE]));
    let high = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));

    assert_eq!(
        super::symbols::builtin_value_lt(&mut eval, vec![low, high]).unwrap(),
        Value::T
    );
    assert_eq!(
        super::symbols::builtin_value_lt(&mut eval, vec![high, low]).unwrap(),
        Value::NIL
    );
}

#[test]
fn value_lt_compares_buffers_by_live_name_and_dead_before_live() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let z = eval.buffers.create_buffer("value-lt-z");
    let a = eval.buffers.create_buffer("value-lt-a");

    assert_eq!(
        super::symbols::builtin_value_lt(
            &mut eval,
            vec![Value::make_buffer(a), Value::make_buffer(z)]
        )
        .unwrap(),
        Value::T
    );

    assert!(eval.buffers.kill_buffer(a));

    assert_eq!(
        super::symbols::builtin_value_lt(
            &mut eval,
            vec![Value::make_buffer(a), Value::make_buffer(z)]
        )
        .unwrap(),
        Value::T
    );
}

#[test]
fn pure_dispatch_x_display_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let x_begin_drag = dispatch_builtin_pure("x-begin-drag", vec![Value::NIL])
        .expect("builtin x-begin-drag should resolve")
        .expect("builtin x-begin-drag should evaluate");
    assert!(x_begin_drag.is_nil());

    let x_double_buffered = dispatch_builtin_pure("x-double-buffered-p", vec![])
        .expect("builtin x-double-buffered-p should resolve")
        .expect("builtin x-double-buffered-p should evaluate");
    assert!(x_double_buffered.is_nil());

    let x_menu_open = dispatch_builtin_pure("x-menu-bar-open-internal", vec![])
        .expect("builtin x-menu-bar-open-internal should resolve")
        .expect("builtin x-menu-bar-open-internal should evaluate");
    assert!(x_menu_open.is_nil());

    let xw_defined = dispatch_builtin_pure(
        "xw-color-defined-p",
        vec![Value::string("black"), Value::NIL],
    )
    .expect("builtin xw-color-defined-p should resolve")
    .expect("builtin xw-color-defined-p should evaluate");
    assert!(xw_defined.is_nil());

    let xw_values = dispatch_builtin_pure("xw-color-values", vec![Value::string("black")])
        .expect("builtin xw-color-values should resolve")
        .expect("builtin xw-color-values should evaluate");
    assert!(xw_values.is_nil());

    let xw_display_color = dispatch_builtin_pure("xw-display-color-p", vec![])
        .expect("builtin xw-display-color-p should resolve")
        .expect("builtin xw-display-color-p should evaluate");
    assert!(xw_display_color.is_nil());
}

#[test]
fn pure_dispatch_minibuffer_lock_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    assert!(
        dispatch_builtin_pure("minibuffer-innermost-command-loop-p", vec![]).is_none(),
        "minibuffer-innermost-command-loop-p should use eval-aware minibuffer state"
    );
    assert!(
        dispatch_builtin_pure("innermost-minibuffer-p", vec![]).is_none(),
        "innermost-minibuffer-p should use eval-aware minibuffer state"
    );

    // `make-local-variable' is DEFUN'ed with an intspec in GNU (src/data.c),
    // so a bare evaluator can be asked for its form; measured on GNU 31.0.90,
    // (interactive-form 'make-local-variable) is
    // (interactive "vMake Local Variable: ").
    let interactive_make_local_variable = dispatch_builtin_pure(
        "interactive-form",
        vec![Value::symbol("make-local-variable")],
    )
    .expect("builtin interactive-form should resolve")
    .expect("builtin interactive-form should evaluate");
    assert_eq!(
        interactive_make_local_variable,
        Value::list(vec![
            Value::symbol("interactive"),
            Value::string("vMake Local Variable: ")
        ])
    );

    let local_if_set = dispatch_builtin_pure("local-variable-if-set-p", vec![Value::symbol("x")])
        .expect("builtin local-variable-if-set-p should resolve")
        .expect("builtin local-variable-if-set-p should evaluate");
    assert!(local_if_set.is_nil());

    let lock_buffer = dispatch_builtin_pure("lock-buffer", vec![])
        .expect("builtin lock-buffer should resolve")
        .expect("builtin lock-buffer should evaluate");
    assert!(lock_buffer.is_nil());

    let tmp = tempfile::NamedTempFile::new().expect("temp file for lock-file");
    let path = tmp.path().to_string_lossy().into_owned();
    let lock_file = dispatch_builtin_pure("lock-file", vec![Value::string(&path)])
        .expect("builtin lock-file should resolve")
        .expect("builtin lock-file should evaluate");
    assert!(lock_file.is_nil());

    let lossage_size = dispatch_builtin_pure("lossage-size", vec![])
        .expect("builtin lossage-size should resolve")
        .expect("builtin lossage-size should evaluate");
    assert_eq!(lossage_size, Value::fixnum(300));

    let unlock_buffer = dispatch_builtin_pure("unlock-buffer", vec![])
        .expect("builtin unlock-buffer should resolve")
        .expect("builtin unlock-buffer should evaluate");
    assert!(unlock_buffer.is_nil());

    let unlock_file = dispatch_builtin_pure("unlock-file", vec![Value::string(&path)])
        .expect("builtin unlock-file should resolve")
        .expect("builtin unlock-file should evaluate");
    assert!(unlock_file.is_nil());
}

#[test]
fn interactive_form_eval_resolves_symbol_lambda_and_alias() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let lambda = Value::list(vec![
        Value::symbol("lambda"),
        Value::NIL,
        Value::list(vec![Value::symbol("interactive"), Value::string("p")]),
        Value::fixnum(1),
    ]);
    eval.obarray_mut()
        .set_symbol_function("vm-interactive-form-lambda", lambda);
    eval.obarray_mut().set_symbol_function(
        "vm-interactive-form-alias",
        Value::symbol("vm-interactive-form-lambda"),
    );

    let expected = Value::list(vec![Value::symbol("interactive"), Value::string("p")]);
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("vm-interactive-form-lambda")])
            .expect("interactive-form should read lambda interactive spec"),
        expected
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("vm-interactive-form-alias")])
            .expect("interactive-form should follow function aliases"),
        expected
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![lambda])
            .expect("interactive-form should parse quoted lambda designators"),
        expected
    );
}

#[test]
fn interactive_form_eval_uses_symbol_properties_and_builtin_subr_specs() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let target = Value::list(vec![Value::symbol("lambda"), Value::NIL, Value::fixnum(1)]);
    eval.obarray_mut()
        .set_symbol_function("vm-interactive-form-property-target", target);
    eval.obarray_mut().set_symbol_function(
        "vm-interactive-form-property-alias",
        Value::symbol("vm-interactive-form-property-target"),
    );
    builtin_put(
        &mut eval,
        vec![
            Value::symbol("vm-interactive-form-property-alias"),
            Value::symbol("interactive-form"),
            Value::list(vec![Value::symbol("interactive"), Value::string("P")]),
        ],
    )
    .expect("put should install interactive-form symbol property");

    assert_eq!(
        builtin_interactive_form(
            &mut eval,
            vec![Value::symbol("vm-interactive-form-property-alias")]
        )
        .expect("interactive-form should consult symbol property chain"),
        Value::list(vec![Value::symbol("interactive"), Value::string("P")])
    );
    assert!(
        builtin_interactive_form(
            &mut eval,
            vec![Value::symbol("vm-interactive-form-property-target")]
        )
        .expect("interactive-form should evaluate target symbol")
        .is_nil()
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("forward-char")])
            .expect("interactive-form should expose builtin subr spec"),
        Value::list(vec![Value::symbol("interactive"), Value::string("^p")])
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("narrow-to-region")])
            .expect("interactive-form should expose builtin region spec"),
        Value::list(vec![Value::symbol("interactive"), Value::string("r")])
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("rename-file")])
            .expect("interactive-form should expose builtin file rename spec"),
        Value::list(vec![
            Value::symbol("interactive"),
            Value::string("fRename file: \nGRename %s to file: \np"),
        ])
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("insert-char")])
            .expect("interactive-form should expose builtin character reader spec"),
        Value::list(vec![
            Value::symbol("interactive"),
            Value::list(vec![
                Value::symbol("list"),
                Value::list(vec![
                    Value::symbol("read-char-by-name"),
                    Value::string("Insert character (Unicode name or hex): "),
                ]),
                Value::list(vec![
                    Value::symbol("prefix-numeric-value"),
                    Value::symbol("current-prefix-arg"),
                ]),
                Value::T,
            ]),
        ])
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("goto-char")])
            .expect("interactive-form should expose computed builtin form"),
        Value::list(vec![
            Value::symbol("interactive"),
            Value::list(vec![
                Value::symbol("goto-char--read-natnum-interactive"),
                Value::string("Go to char: "),
            ]),
        ])
    );
    assert!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("car")])
            .expect("interactive-form should evaluate")
            .is_nil()
    );
}

#[test]
fn interactive_form_eval_skips_docstring_before_interactive_spec() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let lambda_with_doc = Value::list(vec![
        Value::symbol("lambda"),
        Value::NIL,
        Value::string("doc"),
        Value::list(vec![Value::symbol("interactive"), Value::string("P")]),
        Value::fixnum(1),
    ]);
    eval.obarray_mut()
        .set_symbol_function("vm-interactive-form-doc", lambda_with_doc);

    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("vm-interactive-form-doc")])
            .expect("interactive-form should inspect lambda body after docstring"),
        Value::list(vec![Value::symbol("interactive"), Value::string("P")])
    );
}

#[test]
fn interactive_form_eval_returns_nil_for_non_interactive_lambda() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let lambda = Value::list(vec![Value::symbol("lambda"), Value::NIL, Value::fixnum(1)]);
    eval.obarray_mut()
        .set_symbol_function("vm-interactive-form-plain", lambda);

    assert!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("vm-interactive-form-plain")])
            .expect("interactive-form should evaluate")
            .is_nil()
    );
    assert!(
        builtin_interactive_form(&mut eval, vec![lambda])
            .expect("interactive-form should evaluate")
            .is_nil()
    );
    assert!(
        builtin_interactive_form(&mut eval, vec![Value::fixnum(0)])
            .expect("interactive-form should evaluate")
            .is_nil()
    );
}

#[test]
fn interactive_form_eval_preserves_noarg_and_explicit_nil_shapes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let noarg_lambda = Value::list(vec![
        Value::symbol("lambda"),
        Value::NIL,
        Value::list(vec![Value::symbol("interactive")]),
        Value::fixnum(1),
    ]);
    let nil_lambda = Value::list(vec![
        Value::symbol("lambda"),
        Value::NIL,
        Value::list(vec![Value::symbol("interactive"), Value::NIL]),
        Value::fixnum(1),
    ]);
    eval.obarray_mut()
        .set_symbol_function("vm-interactive-form-noarg", noarg_lambda);
    eval.obarray_mut()
        .set_symbol_function("vm-interactive-form-nil", nil_lambda);

    assert_eq!(
        builtin_interactive_form(&mut eval, vec![noarg_lambda])
            .expect("interactive-form should evaluate"),
        Value::list(vec![Value::symbol("interactive"), Value::NIL])
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("vm-interactive-form-noarg")])
            .expect("interactive-form should evaluate"),
        Value::list(vec![Value::symbol("interactive"), Value::NIL])
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![nil_lambda])
            .expect("interactive-form should evaluate"),
        Value::list(vec![Value::symbol("interactive"), Value::NIL])
    );
    assert_eq!(
        builtin_interactive_form(&mut eval, vec![Value::symbol("vm-interactive-form-nil")])
            .expect("interactive-form should evaluate"),
        Value::list(vec![Value::symbol("interactive"), Value::NIL])
    );
}

#[test]
fn interactive_form_eval_signals_listp_for_improper_lambda_shapes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let dotted_interactive = Value::list(vec![
        Value::symbol("lambda"),
        Value::NIL,
        Value::cons(Value::symbol("interactive"), Value::string("p")),
        Value::fixnum(1),
    ]);
    let dotted_body = Value::cons(
        Value::symbol("lambda"),
        Value::cons(Value::NIL, Value::cons(Value::fixnum(1), Value::fixnum(2))),
    );
    let doc_dotted_body = Value::cons(
        Value::symbol("lambda"),
        Value::cons(
            Value::NIL,
            Value::cons(Value::string("doc"), Value::fixnum(2)),
        ),
    );
    let doc_interactive_dotted_tail = Value::cons(
        Value::symbol("lambda"),
        Value::cons(
            Value::NIL,
            Value::cons(
                Value::string("doc"),
                Value::cons(
                    Value::list(vec![Value::symbol("interactive")]),
                    Value::fixnum(2),
                ),
            ),
        ),
    );

    let dotted_interactive_err =
        builtin_interactive_form(&mut eval, vec![dotted_interactive]).unwrap_err();
    match dotted_interactive_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::string("p")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let dotted_body_err = builtin_interactive_form(&mut eval, vec![dotted_body]).unwrap_err();
    match dotted_body_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("listp"),
                    Value::cons(Value::fixnum(1), Value::fixnum(2))
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let doc_dotted_body_err =
        builtin_interactive_form(&mut eval, vec![doc_dotted_body]).unwrap_err();
    match doc_dotted_body_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("listp"),
                    Value::cons(Value::string("doc"), Value::fixnum(2))
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    assert_eq!(
        builtin_interactive_form(&mut eval, vec![doc_interactive_dotted_tail])
            .expect("interactive-form should stop at first interactive form"),
        Value::list(vec![Value::symbol("interactive"), Value::NIL])
    );
}

#[test]
fn pure_dispatch_internal_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    // `internal-char-font` is no longer an always-nil stub: with a non-nil
    // POSITION it range-checks against the current buffer and signals
    // `args-out-of-range` for an out-of-range position, exactly like GNU
    // (fontset.c `Finternal_char_font`: CHECK_FIXNUM_COERCE_MARKER + the
    // BEGV/ZV bound). The pure-dispatch buffer is empty, so position 65 is out
    // of range.
    let char_font_oob = dispatch_builtin_pure("internal-char-font", vec![Value::fixnum(65)])
        .expect("builtin internal-char-font should resolve")
        .expect_err("out-of-range POSITION should signal args-out-of-range");
    match char_font_oob {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "args-out-of-range"),
        other => panic!("unexpected flow: {other:?}"),
    }

    let char_font_with_nil_position =
        dispatch_builtin_pure("internal-char-font", vec![Value::NIL, Value::fixnum(65)])
            .expect("builtin internal-char-font should resolve nil-position probe")
            .expect("builtin internal-char-font should accept nil position with char probe");
    assert!(char_font_with_nil_position.is_nil());

    let describe_syntax =
        dispatch_builtin_pure("internal-describe-syntax-value", vec![Value::fixnum(0)])
            .expect("builtin internal-describe-syntax-value should resolve")
            .expect("builtin internal-describe-syntax-value should evaluate");
    assert_eq!(describe_syntax, Value::fixnum(0));

    let parse_modifiers = dispatch_builtin_pure(
        "internal-event-symbol-parse-modifiers",
        vec![Value::symbol("C-x")],
    )
    .expect("builtin internal-event-symbol-parse-modifiers should resolve")
    .expect("builtin internal-event-symbol-parse-modifiers should evaluate");
    assert_eq!(
        parse_modifiers,
        Value::list(vec![Value::symbol("x"), Value::symbol("control")])
    );

    let handle_focus_in = dispatch_builtin_pure("internal-handle-focus-in", vec![Value::NIL])
        .expect("builtin internal-handle-focus-in should resolve")
        .expect_err("builtin internal-handle-focus-in should signal on invalid events");
    match handle_focus_in {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("invalid focus-in event")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let stack_stats = dispatch_builtin_pure("internal-stack-stats", vec![])
        .expect("builtin internal-stack-stats should resolve")
        .expect("builtin internal-stack-stats should evaluate");
    assert!(stack_stats.is_nil());

    let subr_doc = dispatch_builtin_pure("internal-subr-documentation", vec![Value::NIL])
        .expect("builtin internal-subr-documentation should resolve")
        .expect("builtin internal-subr-documentation should evaluate");
    assert_eq!(subr_doc, Value::T);
}

#[test]
fn internal_describe_syntax_routes_prefix_documentation_through_quote_substitution() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(fset 'substitute-command-keys
                 (lambda (_string &optional _no-face _include-menus)
                   "GNU-QUOTED-PREFIX-DOC"))"#,
    )
    .expect("install observable quote-substitution probe");

    // GNU syntax.h: Squote == 6; syntax.c stores the prefix flag in bit 20.
    let syntax = Value::cons(Value::fixnum(6 + (1 << 20)), Value::NIL);
    builtin_internal_describe_syntax_value(&mut eval, vec![syntax])
        .expect("describe prefix syntax");

    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .buffer_string(),
        "' p\twhich means: prefixGNU-QUOTED-PREFIX-DOC"
    );
}

#[test]
fn internal_set_lisp_face_attribute_from_resource_matches_gnu_conversion() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"(let ((face 'resource-face))
                 (list
                  (eq (internal-set-lisp-face-attribute-from-resource
                       face :weight "thin")
                      face)
                  (eq (internal-get-lisp-face-attribute face :weight) 'thin)
                  (progn
                    (internal-set-lisp-face-attribute-from-resource
                     face :underline "red")
                    (equal (internal-get-lisp-face-attribute face :underline) "red"))
                  (progn
                    (internal-set-lisp-face-attribute-from-resource
                     face :inverse-video "TRUE")
                    (eq (internal-get-lisp-face-attribute face :inverse-video) t))
                  (progn
                    (internal-set-lisp-face-attribute-from-resource
                     face :box "(:line-width 2 :color \"red\")")
                    (equal (internal-get-lisp-face-attribute face :box)
                           '(:line-width 2 :color "red")))
                  (progn
                    (internal-set-lisp-face-attribute-from-resource
                     face :height "120px")
                    (= (internal-get-lisp-face-attribute face :height) 120))
                  (condition-case err
                      (internal-set-lisp-face-attribute-from-resource
                       face :height "value")
                    (error (car err)))))"#,
        )
        .expect("internal-set-lisp-face-attribute-from-resource");
    assert_eq!(
        result,
        Value::list(vec![
            Value::T,
            Value::T,
            Value::T,
            Value::T,
            Value::T,
            Value::T,
            Value::symbol("error"),
        ])
    );
}

#[test]
fn internal_track_mouse_binds_and_restores_track_mouse() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"(progn
           (setq track-mouse 'outer)
           (list
            (internal--track-mouse (lambda () track-mouse))
            track-mouse))"#,
        )
        .expect("internal--track-mouse");
    assert_eq!(result, Value::list(vec![Value::T, Value::symbol("outer")]));
}

#[test]
fn internal_track_mouse_restores_track_mouse_after_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"(progn
           (setq track-mouse 'outer)
           (condition-case err
               (internal--track-mouse
                (lambda ()
                  (setq track-mouse 'dragging)
                  (signal 'error nil)))
             (error (list track-mouse (car err)))))"#,
        )
        .expect("internal--track-mouse condition-case");
    assert_eq!(
        result,
        Value::list(vec![Value::symbol("outer"), Value::symbol("error")])
    );
}

#[test]
fn internal_make_var_non_special_clears_special_flag() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray_mut().make_special("x");
    assert!(eval.obarray().is_special("x"));

    let result = dispatch_builtin(
        &mut eval,
        "internal-make-var-non-special",
        vec![Value::symbol("x")],
    )
    .expect("builtin internal-make-var-non-special should resolve")
    .expect("builtin internal-make-var-non-special should evaluate");

    assert!(result.is_nil());
    assert!(!eval.obarray().is_special("x"));
}

#[test]
fn pure_dispatch_memory_module_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let malloc_info = dispatch_builtin_pure("malloc-info", vec![])
        .expect("builtin malloc-info should resolve")
        .expect("builtin malloc-info should evaluate");
    assert!(malloc_info.is_nil());

    let malloc_trim = dispatch_builtin_pure("malloc-trim", vec![])
        .expect("builtin malloc-trim should resolve")
        .expect("builtin malloc-trim should evaluate");
    assert_eq!(malloc_trim, Value::T);

    let malloc_trim_nil = dispatch_builtin_pure("malloc-trim", vec![Value::NIL])
        .expect("builtin malloc-trim should resolve with nil pad")
        .expect("builtin malloc-trim should evaluate with nil pad");
    assert_eq!(malloc_trim_nil, Value::T);

    let malloc_trim_zero = dispatch_builtin_pure("malloc-trim", vec![Value::fixnum(0)])
        .expect("builtin malloc-trim should resolve with integer pad")
        .expect("builtin malloc-trim should evaluate with integer pad");
    assert_eq!(malloc_trim_zero, Value::T);

    for bad in [
        Value::fixnum(-1),
        Value::T,
        Value::vector(vec![Value::fixnum(1)]),
    ] {
        let err = dispatch_builtin_pure("malloc-trim", vec![bad])
            .expect("builtin malloc-trim should resolve for bad pad")
            .expect_err("malloc-trim should reject non-wholenump pad");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("wholenump"), bad]);
            }
            other => panic!("expected signal, got: {other:?}"),
        }
    }

    let memory_info = dispatch_builtin_pure("memory-info", vec![])
        .expect("builtin memory-info should resolve")
        .expect("builtin memory-info should evaluate");
    let items = list_to_vec(&memory_info).expect("memory-info should return list");
    assert_eq!(items.len(), 4);
    assert!(items.iter().all(|item| item.is_fixnum()));

    let module_path = "__neovm_missing_module__.so";
    let module_load_err = dispatch_builtin_pure("module-load", vec![Value::string(module_path)])
        .expect("builtin module-load should resolve")
        .expect_err("builtin module-load should signal on missing path");
    match module_load_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "module-open-failed");
            assert_eq!(sig.data.first(), Some(&Value::string(module_path)));
            assert!(
                sig.data.get(1).map_or(false, |v| v.is_string()),
                "module-open-failed should include string error message payload"
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    let module_load_type_err = dispatch_builtin_pure("module-load", vec![Value::NIL])
        .expect("builtin module-load should resolve")
        .expect_err("module-load should reject non-string path");
    match module_load_type_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::NIL]);
        }
        other => panic!("expected signal, got: {other:?}"),
    }
}

#[test]
fn pure_dispatch_dump_portable_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let dump_portable = dispatch_builtin_pure(
        "dump-emacs-portable",
        vec![Value::string("dump.pdmp"), Value::NIL],
    )
    .expect("builtin dump-emacs-portable should resolve")
    .expect("builtin dump-emacs-portable should evaluate");
    assert!(dump_portable.is_nil());

    let sort_predicate = dispatch_builtin_pure(
        "dump-emacs-portable--sort-predicate",
        vec![Value::NIL, Value::NIL],
    )
    .expect("builtin dump-emacs-portable--sort-predicate should resolve")
    .expect("builtin dump-emacs-portable--sort-predicate should evaluate");
    assert!(sort_predicate.is_nil());

    let sort_predicate_copied = dispatch_builtin_pure(
        "dump-emacs-portable--sort-predicate-copied",
        vec![Value::NIL, Value::NIL],
    )
    .expect("builtin dump-emacs-portable--sort-predicate-copied should resolve")
    .expect("builtin dump-emacs-portable--sort-predicate-copied should evaluate");
    assert!(sort_predicate_copied.is_nil());
}

#[test]
fn pure_dispatch_coding_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let byte_code = dispatch_builtin_pure(
        "byte-code",
        vec![Value::string(""), Value::vector(vec![]), Value::fixnum(0)],
    )
    .expect("builtin byte-code should resolve")
    .expect("builtin byte-code should evaluate");
    assert!(byte_code.is_nil());

    let decode_region = dispatch_builtin_pure(
        "decode-coding-region",
        vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::symbol("utf-8"),
            Value::NIL,
        ],
    )
    .expect("builtin decode-coding-region should resolve")
    .expect("builtin decode-coding-region should evaluate");
    assert_eq!(decode_region, Value::fixnum(0));

    let encode_region = dispatch_builtin_pure(
        "encode-coding-region",
        vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::symbol("utf-8"),
            Value::NIL,
        ],
    )
    .expect("builtin encode-coding-region should resolve")
    .expect("builtin encode-coding-region should evaluate");
    assert_eq!(encode_region, Value::fixnum(0));

    assert!(
        dispatch_builtin_pure("find-operation-coding-system", vec![Value::symbol("write")])
            .is_none()
    );

    assert!(
        dispatch_builtin_pure(
            "handler-bind-1",
            vec![Value::list(vec![]), Value::symbol("body")],
        )
        .is_none()
    );
}

#[test]
fn pure_dispatch_def_keymap_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    assert!(
        dispatch_builtin_pure(
            "defconst-1",
            vec![Value::symbol("foo"), Value::fixnum(1), Value::string("doc")],
        )
        .is_none()
    );

    assert!(
        dispatch_builtin_pure("defvar-1", vec![Value::symbol("foo"), Value::fixnum(1)]).is_none()
    );

    let iso_charset = dispatch_builtin_pure(
        "iso-charset",
        vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)],
    )
    .expect("builtin iso-charset should resolve")
    .expect("builtin iso-charset should evaluate");
    assert!(iso_charset.is_nil());

    let keyelt = dispatch_builtin_pure("keymap--get-keyelt", vec![Value::NIL, Value::NIL])
        .expect("builtin keymap--get-keyelt should resolve")
        .expect("builtin keymap--get-keyelt should evaluate");
    assert!(keyelt.is_nil());

    let keyelt_true = dispatch_builtin_pure("keymap--get-keyelt", vec![Value::T, Value::NIL])
        .expect("builtin keymap--get-keyelt should resolve")
        .expect("builtin keymap--get-keyelt should evaluate");
    assert!(keyelt_true.is_truthy());

    let keymap_prompt = dispatch_builtin_pure("keymap-prompt", vec![Value::NIL])
        .expect("builtin keymap-prompt should resolve")
        .expect("builtin keymap-prompt should evaluate");
    assert!(keymap_prompt.is_nil());
}

#[test]
fn defvar_1_binds_only_when_default_is_unbound() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = builtin_defvar_1(
        &mut eval,
        vec![
            Value::symbol("vm-defvar-1"),
            Value::fixnum(7),
            Value::string("doc"),
        ],
    )
    .expect("defvar-1 should succeed");
    assert_eq!(result, Value::symbol("vm-defvar-1"));
    assert_eq!(
        eval.obarray()
            .symbol_value_id(intern("vm-defvar-1"))
            .copied(),
        Some(Value::fixnum(7))
    );

    let result = builtin_defvar_1(
        &mut eval,
        vec![Value::symbol("vm-defvar-1"), Value::fixnum(9)],
    )
    .expect("second defvar-1 should succeed");
    assert_eq!(result, Value::symbol("vm-defvar-1"));
    assert_eq!(
        eval.obarray()
            .symbol_value_id(intern("vm-defvar-1"))
            .copied(),
        Some(Value::fixnum(7))
    );
}

#[test]
fn defconst_1_sets_value_and_risky_local_property_without_constant_trap() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = builtin_defconst_1(
        &mut eval,
        vec![
            Value::symbol("vm-defconst-1"),
            Value::fixnum(11),
            Value::string("doc"),
        ],
    )
    .expect("defconst-1 should succeed");

    assert_eq!(result, Value::symbol("vm-defconst-1"));
    let symbol = intern("vm-defconst-1");
    assert_eq!(
        eval.obarray().symbol_value_id(symbol).copied(),
        Some(Value::fixnum(11))
    );
    assert_eq!(
        eval.obarray()
            .get_property_id(symbol, intern("risky-local-variable")),
        Some(Value::T)
    );
    assert!(
        !eval.obarray().is_constant_id(symbol),
        "GNU defconst marks the variable special and risky, but does not enforce constancy"
    );
    builtin_set(
        &mut eval,
        vec![Value::symbol("vm-defconst-1"), Value::fixnum(12)],
    )
    .expect("GNU allows setting a defconst variable");
    assert_eq!(
        eval.obarray().symbol_value_id(symbol).copied(),
        Some(Value::fixnum(12))
    );
}

#[test]
fn pure_dispatch_define_coding_system_internal_not_in_pure_path() {
    crate::test_utils::init_test_tracing();
    // define-coding-system-internal is now dispatched via the eval-aware
    // path (it needs &mut CodingSystemManager from the Context).
    // The pure dispatch returns None for it.
    let result = dispatch_builtin_pure("define-coding-system-internal", vec![Value::NIL; 13]);
    assert!(result.is_none(), "should not resolve in pure dispatch");
}

#[test]
fn pure_dispatch_process_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let kill_emacs = dispatch_builtin_pure("kill-emacs", vec![]);
    assert!(
        kill_emacs.is_none(),
        "kill-emacs now requires the eval-aware dispatch path"
    );

    let lower_frame = dispatch_builtin_pure("lower-frame", vec![])
        .expect("builtin lower-frame should resolve")
        .expect("builtin lower-frame should evaluate");
    assert!(lower_frame.is_nil());

    let lread_substitute = dispatch_builtin_pure(
        "lread--substitute-object-in-subtree",
        vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)],
    )
    .expect("builtin lread--substitute-object-in-subtree should resolve")
    .expect("builtin lread--substitute-object-in-subtree should evaluate");
    assert!(lread_substitute.is_nil());
}

#[test]
fn kill_emacs_eval_requests_shutdown_and_stops_command_loop() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.command_loop.running = true;

    let result = super::symbols::builtin_kill_emacs(&mut eval, vec![Value::fixnum(7)]);
    match result {
        Err(crate::emacs_core::error::Flow::Shutdown(request)) => {
            assert_eq!(request.exit_code, 7);
        }
        other => panic!("kill-emacs should unwind as a shutdown, got {other:?}"),
    }
    assert_eq!(
        eval.shutdown_request,
        Some(crate::emacs_core::eval::ShutdownRequest {
            exit_code: 7,
            restart: false,
        })
    );
    assert!(
        !eval.command_loop.running,
        "kill-emacs should stop the interactive command loop"
    );
}

#[test]
fn pure_dispatch_make_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let make_byte_code = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            Value::fixnum(0),
            Value::string(""),
            Value::vector(vec![]),
            Value::fixnum(0),
        ],
    )
    .expect("builtin make-byte-code should resolve")
    .expect("builtin make-byte-code should evaluate");
    assert!(
        make_byte_code.is_bytecode(),
        "make-byte-code should return a ByteCode value, got {:?}",
        make_byte_code
    );

    let hash_literal = Value::list(vec![
        Value::symbol("make-hash-table-from-literal"),
        Value::list(vec![
            Value::symbol("quote"),
            Value::list(vec![
                Value::symbol("hash-table"),
                Value::symbol("test"),
                Value::symbol("eq"),
                Value::symbol("data"),
                Value::list(vec![Value::symbol("foo"), Value::fixnum(42)]),
            ]),
        ]),
    ]);
    let make_byte_code_with_hash = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            Value::fixnum(0),
            Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
                0xC0, 0x87,
            ])),
            Value::vector(vec![hash_literal]),
            Value::fixnum(1),
        ],
    )
    .expect("builtin make-byte-code with hash literal should resolve")
    .expect("builtin make-byte-code with hash literal should evaluate");
    let bc = make_byte_code_with_hash
        .get_bytecode_data()
        .expect("make-byte-code should produce bytecode data");
    if !bc.constants[0].is_hash_table() {
        panic!("expected hash-table constant, got {:?}", bc.constants[0]);
    };
    let entry = {
        let table = bc.constants[0].as_hash_table().unwrap();
        let key = Value::symbol("foo").to_hash_key(&table.test);
        table.data.get(&key).copied()
    };
    assert_eq!(entry, Some(Value::fixnum(42)));

    let make_char_result = dispatch_builtin_pure("make-char", vec![Value::fixnum(1)])
        .expect("builtin make-char should resolve");
    assert!(
        make_char_result.is_err(),
        "make-char with a non-charset should signal"
    );

    // make-closure requires a bytecode prototype; nil signals wrong-type-argument
    let make_closure_result = dispatch_builtin_pure("make-closure", vec![Value::NIL])
        .expect("builtin make-closure should resolve");
    assert!(
        make_closure_result.is_err(),
        "make-closure with nil should signal error"
    );

    let make_finalizer = dispatch_builtin_pure("make-finalizer", vec![Value::symbol("ignore")])
        .expect("builtin make-finalizer should resolve")
        .expect("builtin make-finalizer should evaluate");
    assert_eq!(
        make_finalizer.veclike_type(),
        Some(crate::tagged::header::VecLikeType::Finalizer),
        "make-finalizer should produce a live finalizer object"
    );

    assert!(
        dispatch_builtin_pure(
            "make-indirect-buffer",
            vec![Value::symbol("buf"), Value::string("name")],
        )
        .is_none(),
        "make-indirect-buffer should dispatch via eval-aware path"
    );

    let make_interpreted = dispatch_builtin_pure(
        "make-interpreted-closure",
        vec![
            Value::list(vec![]),
            Value::list(vec![Value::NIL]),
            Value::NIL,
        ],
    )
    .expect("builtin make-interpreted-closure should resolve")
    .expect("builtin make-interpreted-closure should evaluate");
    // make-interpreted-closure now returns a Lambda value (not nil)
    assert!(make_interpreted.is_lambda());
}

#[test]
fn make_byte_code_pins_bytecode_string_like_gnu() {
    crate::test_utils::init_test_tracing();

    let bytecode = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xC0, 0x87,
    ]));
    assert!(
        !bytecode
            .as_lisp_string()
            .expect("bytecode string")
            .is_immovable()
    );

    let value = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            Value::fixnum(0),
            bytecode,
            Value::vector(vec![]),
            Value::fixnum(1),
        ],
    )
    .expect("builtin make-byte-code should resolve")
    .expect("builtin make-byte-code should evaluate");
    assert!(value.is_bytecode());
    assert!(
        value
            .get_bytecode_data()
            .expect("bytecode payload")
            .resident_ops()
            .is_empty(),
        "validated GNU IR should stay deferred until execution"
    );
    assert!(
        bytecode
            .as_lisp_string()
            .expect("bytecode string")
            .is_immovable()
    );
}

#[test]
fn make_byte_code_rejects_multibyte_bytecode_string_like_gnu() {
    crate::test_utils::init_test_tracing();

    let err = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            Value::fixnum(0),
            Value::multibyte_string("lambda"),
            Value::vector(vec![]),
            Value::fixnum(1),
        ],
    )
    .expect("builtin make-byte-code should resolve")
    .expect_err("multibyte bytecode string should fail");

    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Invalid byte-code object")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn make_byte_code_preserves_gnu_arg_slot_zero() {
    crate::test_utils::init_test_tracing();

    let lexical_arg_descriptor = Value::fixnum(514);
    let lexical = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            lexical_arg_descriptor,
            Value::string(""),
            Value::vector(vec![]),
            Value::fixnum(0),
        ],
    )
    .expect("builtin make-byte-code should resolve")
    .expect("builtin make-byte-code should evaluate");
    let lexical_bc = lexical
        .get_bytecode_data()
        .expect("make-byte-code should produce bytecode data");
    assert_eq!(lexical_bc.arglist, lexical_arg_descriptor);
    assert!(
        lexical_bc.lexical,
        "GNU integer arg descriptors are lexical byte-code frames"
    );

    let dynamic_arglist = Value::list(vec![
        Value::symbol("arg0"),
        Value::symbol("&optional"),
        Value::symbol("arg1"),
    ]);
    let dynamic = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            dynamic_arglist,
            Value::string(""),
            Value::vector(vec![]),
            Value::fixnum(0),
        ],
    )
    .expect("builtin make-byte-code should resolve")
    .expect("builtin make-byte-code should evaluate");
    let dynamic_bc = dynamic
        .get_bytecode_data()
        .expect("make-byte-code should produce bytecode data");
    assert!(
        !dynamic_bc.lexical,
        "GNU list arglists are old dynamically-bound byte-code frames"
    );
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&dynamic_bc.arglist)
            .expect("arg slot should stay a proper list"),
        vec![
            Value::symbol("arg0"),
            Value::symbol("&optional"),
            Value::symbol("arg1"),
        ]
    );
}

#[test]
fn make_byte_code_preserves_gnu_closure_slot_count() {
    crate::test_utils::init_test_tracing();

    let with_nil_doc = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            Value::fixnum(0),
            Value::string(""),
            Value::vector(vec![]),
            Value::fixnum(0),
            Value::NIL,
        ],
    )
    .expect("builtin make-byte-code should resolve")
    .expect("nil doc slot bytecode should build");

    let len = dispatch_builtin_pure("length", vec![with_nil_doc])
        .expect("builtin length should resolve")
        .expect("length should evaluate");
    assert_eq!(len, Value::fixnum(5));

    let code = dispatch_builtin_pure("aref", vec![with_nil_doc, Value::fixnum(1)])
        .expect("builtin aref should resolve")
        .expect("aref 1 should evaluate");
    assert_eq!(code, Value::string(""));

    let slot4 = dispatch_builtin_pure("aref", vec![with_nil_doc, Value::fixnum(4)])
        .expect("builtin aref should resolve")
        .expect("aref 4 should evaluate");
    assert_eq!(slot4, Value::NIL);

    let slot5 = dispatch_builtin_pure("aref", vec![with_nil_doc, Value::fixnum(5)])
        .expect("builtin aref should resolve");
    assert!(slot5.is_err(), "slot 5 should be out of range");

    let with_extra = dispatch_builtin_pure(
        "make-byte-code",
        vec![
            Value::fixnum(0),
            Value::string(""),
            Value::vector(vec![]),
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::symbol("extra"),
        ],
    )
    .expect("builtin make-byte-code should resolve")
    .expect("extra slot bytecode should build");

    let len = dispatch_builtin_pure("length", vec![with_extra])
        .expect("builtin length should resolve")
        .expect("length should evaluate");
    assert_eq!(len, Value::fixnum(7));

    let slot6 = dispatch_builtin_pure("aref", vec![with_extra, Value::fixnum(6)])
        .expect("builtin aref should resolve")
        .expect("aref 6 should evaluate");
    assert_eq!(slot6, Value::symbol("extra"));
}

#[test]
fn make_char_matches_common_gnu_charset_mappings() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::charset::reset_charset_registry();
    assert_eq!(
        dispatch_builtin_pure("make-char", vec![Value::symbol("ascii"), Value::fixnum(65)])
            .expect("builtin make-char should resolve")
            .expect("ascii make-char should evaluate"),
        Value::fixnum(65)
    );
    assert_eq!(
        dispatch_builtin_pure(
            "make-char",
            vec![Value::symbol("latin-iso8859-1"), Value::fixnum(65)]
        )
        .expect("builtin make-char should resolve")
        .expect("latin-1 make-char should evaluate"),
        Value::fixnum(193)
    );

    let mut thai_args = vec![Value::NIL; 17];
    thai_args[0] = Value::symbol("thai-tis620-test");
    thai_args[1] = Value::fixnum(1);
    thai_args[2] = Value::vector(vec![Value::fixnum(32), Value::fixnum(127)]);
    thai_args[5] = Value::fixnum('T' as i64);
    thai_args[11] = Value::fixnum(0x0E00);
    crate::emacs_core::charset::builtin_define_charset_internal(thai_args)
        .expect("test Thai charset should register");

    assert_eq!(
        dispatch_builtin_pure(
            "make-char",
            vec![Value::symbol("thai-tis620-test"), Value::fixnum(230)]
        )
        .expect("builtin make-char should resolve")
        .expect("thai make-char should evaluate"),
        Value::fixnum(0x0E46)
    );
    assert_eq!(
        dispatch_builtin_pure("make-char", vec![Value::symbol("latin-iso8859-1")])
            .expect("builtin make-char should resolve")
            .expect("latin-1 default make-char should evaluate"),
        Value::fixnum(160)
    );
}

#[test]
fn pure_dispatch_treesit_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let available = dispatch_builtin_pure("treesit-available-p", vec![])
        .expect("builtin treesit-available-p should resolve")
        .expect("builtin treesit-available-p should evaluate");
    assert!(available.is_truthy());

    let compiled_query = dispatch_builtin_pure("treesit-compiled-query-p", vec![Value::NIL])
        .expect("builtin treesit-compiled-query-p should resolve")
        .expect("builtin treesit-compiled-query-p should evaluate");
    assert!(compiled_query.is_nil());

    let language_abi = dispatch_builtin_pure("treesit-language-abi-version", vec![])
        .expect("builtin treesit-language-abi-version should resolve")
        .expect("builtin treesit-language-abi-version should evaluate");
    assert!(language_abi.is_nil());

    let language_available = dispatch_builtin_pure(
        "treesit-language-available-p",
        vec![Value::symbol("definitely-missing-language"), Value::NIL],
    )
    .expect("builtin treesit-language-available-p should resolve")
    .expect("builtin treesit-language-available-p should evaluate");
    assert!(language_available.is_nil());

    let library_abi = dispatch_builtin_pure("treesit-library-abi-version", vec![])
        .expect("builtin treesit-library-abi-version should resolve")
        .expect("builtin treesit-library-abi-version should evaluate");
    assert_eq!(
        library_abi.as_fixnum(),
        Some(tree_sitter::LANGUAGE_VERSION as i64)
    );

    let node_check = dispatch_builtin_pure("treesit-node-check", vec![Value::NIL, Value::NIL])
        .expect("builtin treesit-node-check should resolve")
        .expect("builtin treesit-node-check should evaluate");
    assert!(node_check.is_nil());

    let node_child =
        dispatch_builtin_pure("treesit-node-child", vec![Value::NIL, Value::fixnum(0)])
            .expect("builtin treesit-node-child should resolve")
            .expect("builtin treesit-node-child should evaluate");
    assert!(node_child.is_nil());

    let node_child_by_field = dispatch_builtin_pure(
        "treesit-node-child-by-field-name",
        vec![Value::NIL, Value::string("name")],
    )
    .expect("builtin treesit-node-child-by-field-name should resolve")
    .expect("builtin treesit-node-child-by-field-name should evaluate");
    assert!(node_child_by_field.is_nil());

    let node_child_count =
        dispatch_builtin_pure("treesit-node-child-count", vec![Value::NIL, Value::NIL])
            .expect("builtin treesit-node-child-count should resolve")
            .expect("builtin treesit-node-child-count should evaluate");
    assert!(node_child_count.is_nil());

    let node_descendant = dispatch_builtin_pure(
        "treesit-node-descendant-for-range",
        vec![Value::NIL, Value::fixnum(0), Value::fixnum(1), Value::NIL],
    )
    .expect("builtin treesit-node-descendant-for-range should resolve")
    .expect("builtin treesit-node-descendant-for-range should evaluate");
    assert!(node_descendant.is_nil());
}

#[test]
fn pure_dispatch_make_interpreted_closure_accepts_raw_unibyte_docstring() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let closure = dispatch_builtin_pure(
        "make-interpreted-closure",
        vec![Value::list(vec![]), Value::list(vec![Value::NIL]), raw],
    )
    .expect("builtin make-interpreted-closure should resolve")
    .expect("builtin make-interpreted-closure should evaluate");
    assert!(closure.is_lambda());
}

#[test]
fn pure_dispatch_intern_soft_accepts_raw_unibyte_string_name() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let value = dispatch_builtin_pure("intern-soft", vec![raw])
        .expect("builtin intern-soft should resolve")
        .expect("builtin intern-soft should evaluate");
    assert!(value.is_symbol() || value.is_nil());
}

#[test]
fn make_byte_code_from_parts_preserves_non_string_doc_slot_as_doc_form() {
    crate::test_utils::init_test_tracing();
    let value = make_byte_code_from_parts(
        &Value::list(vec![]),
        &Value::string(""),
        &Value::vector(vec![]),
        &Value::fixnum(0),
        Some(&Value::symbol("advice")),
        None,
    )
    .expect("byte-code constructor should accept oclosure type slot");

    let bytecode = value
        .get_bytecode_data()
        .expect("constructor should return a bytecode function");
    assert_eq!(bytecode.docstring, None);
    assert_eq!(bytecode.doc_form, Some(Value::symbol("advice")));
}

#[test]
fn make_byte_code_from_parts_preserves_raw_unibyte_docstring() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let value = make_byte_code_from_parts(
        &Value::list(vec![]),
        &Value::string(""),
        &Value::vector(vec![]),
        &Value::fixnum(0),
        Some(&raw),
        None,
    )
    .expect("byte-code constructor should accept raw unibyte docstring");

    let bytecode = value
        .get_bytecode_data()
        .expect("constructor should return a bytecode function");
    assert_eq!(
        bytecode.docstring,
        Some(raw.as_lisp_string().expect("raw docstring string").clone())
    );
    assert_eq!(bytecode.doc_form, None);
}

#[test]
fn pure_dispatch_treesit_node_placeholder_cluster_matches_compat_contracts() {
    crate::test_utils::init_test_tracing();
    let node_end = dispatch_builtin_pure("treesit-node-end", vec![Value::NIL])
        .expect("builtin treesit-node-end should resolve")
        .expect("builtin treesit-node-end should evaluate");
    assert!(node_end.is_nil());

    let node_eq = dispatch_builtin_pure("treesit-node-eq", vec![Value::NIL, Value::NIL])
        .expect("builtin treesit-node-eq should resolve")
        .expect("builtin treesit-node-eq should evaluate");
    assert!(node_eq.is_nil());

    let field_name = dispatch_builtin_pure(
        "treesit-node-field-name-for-child",
        vec![Value::NIL, Value::fixnum(0)],
    )
    .expect("builtin treesit-node-field-name-for-child should resolve")
    .expect("builtin treesit-node-field-name-for-child should evaluate");
    assert!(field_name.is_nil());

    let first_child_for_pos = dispatch_builtin_pure(
        "treesit-node-first-child-for-pos",
        vec![Value::NIL, Value::fixnum(0), Value::NIL],
    )
    .expect("builtin treesit-node-first-child-for-pos should resolve")
    .expect("builtin treesit-node-first-child-for-pos should evaluate");
    assert!(first_child_for_pos.is_nil());

    let next_sibling = dispatch_builtin_pure("treesit-node-next-sibling", vec![Value::NIL])
        .expect("builtin treesit-node-next-sibling should resolve")
        .expect("builtin treesit-node-next-sibling should evaluate");
    assert!(next_sibling.is_nil());

    let node_p = dispatch_builtin_pure("treesit-node-p", vec![Value::NIL])
        .expect("builtin treesit-node-p should resolve")
        .expect("builtin treesit-node-p should evaluate");
    assert!(node_p.is_nil());

    let parent = dispatch_builtin_pure("treesit-node-parent", vec![Value::NIL])
        .expect("builtin treesit-node-parent should resolve")
        .expect("builtin treesit-node-parent should evaluate");
    assert!(parent.is_nil());

    let err = dispatch_builtin_pure("treesit-node-parser", vec![Value::NIL])
        .expect("builtin treesit-node-parser should resolve")
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let prev_sibling = dispatch_builtin_pure("treesit-node-prev-sibling", vec![Value::NIL])
        .expect("builtin treesit-node-prev-sibling should resolve")
        .expect("builtin treesit-node-prev-sibling should evaluate");
    assert!(prev_sibling.is_nil());

    let start = dispatch_builtin_pure("treesit-node-start", vec![Value::NIL])
        .expect("builtin treesit-node-start should resolve")
        .expect("builtin treesit-node-start should evaluate");
    assert!(start.is_nil());

    let node_string = dispatch_builtin_pure("treesit-node-string", vec![Value::NIL])
        .expect("builtin treesit-node-string should resolve")
        .expect("builtin treesit-node-string should evaluate");
    assert!(node_string.is_nil());

    let node_type = dispatch_builtin_pure("treesit-node-type", vec![Value::NIL])
        .expect("builtin treesit-node-type should resolve")
        .expect("builtin treesit-node-type should evaluate");
    assert!(node_type.is_nil());
}

#[test]
fn match_data_round_trip_with_nil_groups() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_set_match_data(
        &mut eval,
        vec![Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::fixnum(5),
            Value::fixnum(7),
        ])],
    )
    .expect("set-match-data should succeed");

    let md = builtin_match_data(&mut eval, vec![]).expect("match-data should succeed");
    assert_eq!(
        md,
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::fixnum(5),
            Value::fixnum(7)
        ])
    );
}

#[test]
fn buffer_match_data_keeps_lisp_positions_after_matched_text_is_deleted() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);

    let result = eval
        .eval_str_each(
            r#"(with-temp-buffer
                 (insert "${1:`\"Foo\"`}")
                 (re-search-backward "`\"Foo\"`")
                 (delete-region (match-beginning 0) (match-end 0))
                 (let ((saved (match-data t)))
                   (goto-char (car saved))
                   (insert "Foo")
                   (list (car saved) (buffer-string))))"#,
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();

    assert_eq!(result[0], r#"OK (5 "${1:Foo}")"#);
}

#[test]
fn bootstrap_runtime_set_match_data_restores_multibyte_buffer_positions_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);
    let result = eval
        .eval_str_each(
            r#"(with-temp-buffer
             (insert "a—b")
             (goto-char (point-min))
             (re-search-forward "—" nil t)
             (let ((saved (match-data t)))
               (with-temp-buffer
                 (insert "other")
                 (set-match-data saved)
                 (list (bufferp (car (last saved)))
                       (equal (match-data t) saved)
                       (match-beginning 0)
                       (match-end 0)
                       (match-string 0)))))"#,
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();

    assert_eq!(result[0], r#"OK (t t 2 3 "t")"#);
}

#[test]
fn replace_match_after_set_match_data_uses_gnu_buffer_char_positions() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::builtins::search::{
        builtin_match_data, builtin_replace_match, builtin_search_forward, builtin_set_match_data,
    };
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("alpha one");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    builtin_search_forward(&mut eval, vec![Value::string("alpha")])
        .expect("seed buffer match data");
    let saved = builtin_match_data(&mut eval, vec![Value::T])
        .expect("read match data with integer positions");
    builtin_set_match_data(&mut eval, vec![saved]).expect("restore GNU-style buffer positions");
    let restored = eval.match_data.as_ref().expect("restored match data");
    assert_eq!(
        restored.group(0),
        Some(crate::emacs_core::regex::MatchGroup::new(1, 6))
    );
    assert!(matches!(
        restored.source(),
        crate::emacs_core::regex::MatchDataSource::Buffer(_)
    ));
    let (start, end, replacement) =
        crate::emacs_core::regex::compute_buffer_replacement_with_syntax(
            eval.buffers.current_buffer().expect("current buffer"),
            "omega",
            true,
            true,
            0,
            &eval.match_data,
            false,
        )
        .expect("compute replacement span");
    assert_eq!((start, end), (0, 5));
    assert_eq!(
        replacement
            .as_utf8_str()
            .expect("replacement should stay utf-8"),
        "omega"
    );
    builtin_replace_match(&mut eval, vec![Value::string("omega"), Value::T, Value::T])
        .expect("replace-match should honor restored buffer match positions");

    let buffer = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(
        buffer.buffer_substring_range(crate::buffer::EmacsByteRange::from_usize(
            0,
            buffer.total_emacs_byte_len().get(),
        )),
        "omega one"
    );
}

#[test]
fn match_beginning_end_return_nil_without_match_data() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_set_match_data(&mut eval, vec![Value::NIL]).expect("set-match-data nil");

    let beg = builtin_match_beginning(&mut eval, vec![Value::fixnum(0)])
        .expect("match-beginning should not error");
    let end =
        builtin_match_end(&mut eval, vec![Value::fixnum(0)]).expect("match-end should not error");
    assert!(beg.is_nil());
    assert!(end.is_nil());
}

#[test]
fn negative_match_group_signals_args_out_of_range() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let match_string_err = builtin_match_string(&mut eval, vec![Value::fixnum(-1)])
        .expect_err("negative subgroup should signal");
    match match_string_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(-1), Value::fixnum(0)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let match_beginning_err = builtin_match_beginning(&mut eval, vec![Value::fixnum(-1)])
        .expect_err("negative subgroup should signal");
    match match_beginning_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(-1), Value::fixnum(0)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let match_end_err = builtin_match_end(&mut eval, vec![Value::fixnum(-1)])
        .expect_err("negative subgroup should signal");
    match match_end_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(-1), Value::fixnum(0)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn buffer_region_negative_bounds_signal_without_panicking() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("abc")]).expect("insert should succeed");
    let current = builtin_current_buffer(&mut eval, vec![]).expect("current-buffer should work");

    let substring_err =
        builtin_buffer_substring(&mut eval, vec![Value::fixnum(-1), Value::fixnum(2)])
            .expect_err("negative start should signal");
    match substring_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![current, Value::fixnum(-1), Value::fixnum(2)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let delete_err = builtin_delete_region(&mut eval, vec![Value::fixnum(-1), Value::fixnum(2)])
        .expect_err("negative start should signal");
    match delete_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![current, Value::fixnum(-1), Value::fixnum(2)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let narrow_err = builtin_narrow_to_region(&mut eval, vec![Value::fixnum(-1), Value::fixnum(2)])
        .expect_err("negative start should signal");
    match narrow_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(-1), Value::fixnum(2)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    assert_eq!(
        builtin_char_after(&mut eval, vec![Value::fixnum(-1)]).expect("char-after should succeed"),
        Value::NIL
    );
    assert_eq!(
        builtin_char_before(&mut eval, vec![Value::fixnum(0)]).expect("char-before should succeed"),
        Value::NIL
    );
}

#[test]
fn goto_char_rejects_bignum_but_fix_position_builtins_clamp_it() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("abc")]).expect("insert should succeed");

    let positive_big = Value::bignum(Integer::from(1u64) << 100u32);
    let negative_big = Value::bignum(-(Integer::from(1u64) << 100u32));

    let goto_err = builtin_goto_char(&mut eval, vec![positive_big])
        .expect_err("GNU goto-char rejects bignum even though fix_position accepts it");
    match goto_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), positive_big]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    assert_eq!(
        builtin_char_after(&mut eval, vec![positive_big]).expect("char-after should succeed"),
        Value::NIL
    );
    assert_eq!(
        builtin_char_after(&mut eval, vec![negative_big]).expect("char-after should succeed"),
        Value::NIL
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![positive_big])
            .expect("position-bytes should clamp positive bignum"),
        Value::NIL
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![negative_big])
            .expect("position-bytes should clamp negative bignum"),
        Value::NIL
    );
}

#[test]
fn compute_motion_validates_positions_and_offsets_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("abc")]).expect("insert should succeed");
    builtin_narrow_to_region(&mut eval, vec![Value::fixnum(2), Value::fixnum(3)])
        .expect("narrow-to-region should succeed");
    let frompos = Value::cons(Value::fixnum(0), Value::fixnum(0));

    let from_err = crate::emacs_core::indent::compute_motion(
        &mut eval,
        vec![
            Value::fixnum(1),
            frompos,
            Value::fixnum(3),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("FROM outside narrowing should signal");
    match from_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let to_err = crate::emacs_core::indent::compute_motion(
        &mut eval,
        vec![
            Value::fixnum(2),
            frompos,
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("TO outside narrowing should signal");
    match to_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(4), Value::fixnum(2), Value::fixnum(3)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let positive_big = Value::bignum(Integer::from(1u64) << 100u32);
    let bignum_err = crate::emacs_core::indent::compute_motion(
        &mut eval,
        vec![
            positive_big,
            frompos,
            Value::fixnum(3),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("GNU compute-motion coerces bignum with fix_position before range check");
    match bignum_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![
                    Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
                    Value::fixnum(2),
                    Value::fixnum(3),
                ]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let cons_bignum_err = crate::emacs_core::indent::compute_motion(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::cons(positive_big, Value::fixnum(0)),
            Value::fixnum(3),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("FROMPOS fields are CHECK_FIXNUM, not fix_position");
    match cons_bignum_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("fixnump"), positive_big]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let offsets_err = crate::emacs_core::indent::compute_motion(
        &mut eval,
        vec![
            Value::fixnum(2),
            frompos,
            Value::fixnum(3),
            Value::NIL,
            Value::NIL,
            Value::cons(Value::fixnum(-1), Value::fixnum(0)),
            Value::NIL,
        ],
    )
    .expect_err("negative OFFSETS should signal args-out-of-range");
    match offsets_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(-1), Value::fixnum(0)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn delete_region_normalizes_reversed_bounds() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("abc")]).expect("insert should succeed");

    builtin_delete_region(&mut eval, vec![Value::fixnum(3), Value::fixnum(2)])
        .expect("delete-region should accept reversed bounds");

    let text = builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should succeed");
    assert_eq!(text.as_utf8_str(), Some("ac"));
}

#[test]
fn delete_and_extract_region_preserves_buffer_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_insert(&mut eval, vec![Value::string("AAABBBCCC")]).expect("insert should succeed");
    let buf = eval.buffers.current_buffer().expect("current buffer");
    let _ = eval.buffers.put_buffer_text_property_in_emacs_byte_range(
        buf.id,
        EmacsByteRange::from_usize(3, 6),
        Value::symbol("face"),
        Value::symbol("italic"),
    );

    let extracted =
        builtin_delete_and_extract_region(&mut eval, vec![Value::fixnum(4), Value::fixnum(7)])
            .expect("delete-and-extract-region should succeed");

    assert_eq!(extracted.as_utf8_str(), Some("BBB"));
    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(extracted)
        .expect("extracted string should keep text properties");
    let intervals = props.intervals_snapshot();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].start, 0);
    assert_eq!(intervals[0].end, 3);
    assert_eq!(
        intervals[0].properties.get(&Value::symbol("face")),
        Some(&Value::symbol("italic"))
    );
}

#[test]
fn string_match_start_handles_nil_and_negative_offsets() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let with_nil = builtin_string_match(
        &mut eval,
        vec![Value::string("a"), Value::string("ba"), Value::NIL],
    )
    .expect("string-match with nil start");
    assert_eq!(with_nil, Value::fixnum(1));

    let with_negative = builtin_string_match(
        &mut eval,
        vec![Value::string("a"), Value::string("ba"), Value::fixnum(-1)],
    )
    .expect("string-match with negative start");
    assert_eq!(with_negative, Value::fixnum(1));

    let out_of_range = builtin_string_match(
        &mut eval,
        vec![Value::string("a"), Value::string("ba"), Value::fixnum(3)],
    );
    assert!(out_of_range.is_err());
}

#[test]
fn string_match_handles_raw_unibyte_pattern_without_panicking() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = builtin_string_match(&mut eval, vec![raw, raw]).expect("raw string-match");
    assert_eq!(result, Value::fixnum(0));

    let matched = builtin_match_string(&mut eval, vec![Value::fixnum(0)]).expect("match-string");
    let matched = matched.as_lisp_string().expect("matched string");
    assert!(!matched.is_multibyte());
    assert_eq!(matched.as_bytes(), &[0xFF]);
}

#[test]
fn string_match_unibyte_high_byte_charset_range_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let high_byte_range = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'[', 0x80, b'-', 0xFF, b']',
    ]));
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xC3]));
    let result = builtin_string_match(&mut eval, vec![high_byte_range, raw])
        .expect("raw high-byte range should compile");
    assert_eq!(result, Value::fixnum(0));

    let all_bytes = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'[', b'^', 0x00, b'-', 0x7F, 0x80, b'-', 0xFF, b']',
    ]));
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xC3]));
    let result = builtin_string_match(&mut eval, vec![all_bytes, raw]).expect("negated byte range");
    assert_eq!(result, Value::NIL);

    let high_byte_range = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'[', 0x80, b'-', 0xFF, b']',
    ]));
    let multibyte_e_acute = Value::string("é");
    let result = builtin_string_match(&mut eval, vec![high_byte_range, multibyte_e_acute])
        .expect("unibyte high-byte range against multibyte string");
    assert_eq!(result, Value::NIL);

    let all_bytes = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'[', b'^', 0x00, b'-', 0x7F, 0x80, b'-', 0xFF, b']',
    ]));
    let multibyte_e_acute = Value::string("é");
    let result = builtin_string_match(&mut eval, vec![all_bytes, multibyte_e_acute])
        .expect("negated byte range against multibyte string");
    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn substring_preserves_raw_unibyte_string_content() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A',
    ]));

    let result = builtin_substring(vec![raw, Value::fixnum(0), Value::fixnum(1)])
        .expect("substring should handle raw unibyte strings");
    let text = result
        .as_lisp_string()
        .expect("substring result should be a string");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn substring_no_properties_preserves_raw_unibyte_string_content() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A',
    ]));

    let result = builtin_substring_no_properties(vec![raw, Value::fixnum(0), Value::fixnum(1)])
        .expect("substring-no-properties should handle raw unibyte strings");
    let text = result
        .as_lisp_string()
        .expect("substring result should be a string");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn substring_no_properties_drops_string_text_properties_like_gnu() {
    crate::test_utils::init_test_tracing();
    let source = Value::string("abcd");
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    let _ = put_string_property(
        &mut table,
        1,
        3,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(source, table);

    let result = builtin_substring_no_properties(vec![source, Value::fixnum(1), Value::fixnum(3)])
        .expect("substring-no-properties should return an unpropertized substring");

    assert_eq!(result.as_lisp_string().unwrap().as_utf8_str(), Some("bc"));
    assert!(
        crate::emacs_core::value::get_string_text_properties_table_for_value(result).is_none(),
        "GNU Fsubstring_no_properties returns a string without intervals"
    );
}

#[test]
fn search_match_runtime_arity_edges_match_oracle_contracts() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let search_over_arity = builtin_search_forward(
        &mut eval,
        vec![
            Value::string("a"),
            Value::NIL,
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
        ],
    );
    assert!(matches!(
        search_over_arity,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let regex_over_arity = builtin_re_search_forward(
        &mut eval,
        vec![
            Value::string("a"),
            Value::NIL,
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
        ],
    );
    assert!(matches!(
        regex_over_arity,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let looking_at_optional_second =
        builtin_looking_at(&mut eval, vec![Value::string("a"), Value::T]);
    assert!(looking_at_optional_second.is_ok());

    let looking_at_over_arity =
        builtin_looking_at(&mut eval, vec![Value::string("a"), Value::NIL, Value::NIL]);
    assert!(matches!(
        looking_at_over_arity,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let looking_at_p_over_arity =
        builtin_looking_at_p(&mut eval, vec![Value::string("a"), Value::NIL]);
    assert!(matches!(
        looking_at_p_over_arity,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let looking_at_p_bad_type = builtin_looking_at_p(&mut eval, vec![Value::fixnum(1)]);
    assert!(matches!(
        looking_at_p_bad_type,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));

    let match_string_over_arity = builtin_match_string(
        &mut eval,
        vec![Value::fixnum(0), Value::string("a"), Value::NIL],
    );
    assert!(matches!(
        match_string_over_arity,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let replace_match_over_arity = builtin_replace_match(
        &mut eval,
        vec![
            Value::string("x"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        replace_match_over_arity,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let string_match_over_arity = builtin_string_match(
        &mut eval,
        vec![
            Value::string("a"),
            Value::string("a"),
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        string_match_over_arity,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn set_match_data_rejects_non_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_set_match_data(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn looking_at_inhibit_modify_preserves_match_data() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("abc");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let baseline = Value::list(vec![Value::fixnum(10), Value::fixnum(11)]);
    builtin_set_match_data(&mut eval, vec![baseline]).expect("setting baseline match-data");
    let result = builtin_looking_at(&mut eval, vec![Value::string("a"), Value::T]);
    assert!(result.is_ok());

    let observed = builtin_match_data(&mut eval, vec![]).expect("read match-data");
    assert_eq!(observed, baseline);
}

#[test]
fn looking_at_updates_match_data_when_allowed() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("abc");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    builtin_set_match_data(&mut eval, vec![Value::NIL]).expect("clear match-data");
    let result = builtin_looking_at(&mut eval, vec![Value::string("a"), Value::NIL]);
    assert!(result.is_ok());

    let observed = builtin_match_data(&mut eval, vec![]).expect("read match-data");
    // GNU returns markers for buffer matches. Verify match-data is non-nil
    // and contains correct position information.
    assert!(
        observed.is_cons(),
        "match-data should return a non-nil list"
    );
    // Check with INTEGERS flag to get integer positions.
    let int_md = builtin_match_data(&mut eval, vec![Value::T]).expect("read match-data integers");
    // Compare structurally: extract the integer values
    let items =
        crate::emacs_core::value::list_to_vec(&int_md).expect("match-data should be a proper list");
    assert!(
        items.len() >= 2,
        "expected at least 2-element match-data list, got {} elements: {:?}",
        items.len(),
        items
    );
    // GNU positions are 1-based
    assert!(
        items[0].as_int() == Some(1) && items[1].as_int() == Some(2),
        "expected match-data (1 2), got ({:?} {:?})",
        items[0],
        items[1]
    );
}

#[test]
fn looking_at_p_preserves_match_data() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("abc");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let baseline = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    builtin_set_match_data(&mut eval, vec![baseline]).expect("seed baseline");
    let _ = builtin_looking_at_p(&mut eval, vec![Value::string("z")])
        .expect("looking-at-p handles non-match");
    let observed = builtin_match_data(&mut eval, vec![]).expect("read match-data");
    assert_eq!(observed, baseline);
}

#[test]
fn looking_at_handles_raw_unibyte_pattern_without_panicking() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("abc");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let pattern = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let _ = builtin_looking_at(&mut eval, vec![pattern]);
}

#[test]
fn compare_value_lt_handles_raw_unibyte_strings_without_panicking() {
    crate::test_utils::init_test_tracing();

    let eval = crate::emacs_core::eval::Context::new();
    let left = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFE]));
    let right = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));

    let ordering = crate::emacs_core::builtins::symbols::compare_value_lt(&eval, &left, &right)
        .expect("raw unibyte string comparison should succeed");
    assert_eq!(ordering, std::cmp::Ordering::Less);
}

#[test]
fn reverse_handles_raw_unibyte_strings_without_panicking() {
    crate::test_utils::init_test_tracing();

    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A', 0x80,
    ]));
    let result = builtin_reverse(vec![raw]).expect("reverse should handle raw unibyte strings");
    let reversed = result
        .as_lisp_string()
        .expect("reverse should return a string");

    assert!(!reversed.is_multibyte());
    assert_eq!(reversed.as_bytes(), &[0x80, b'A', 0xFF]);
}

#[test]
fn copy_sequence_handles_raw_unibyte_strings_and_preserves_text_properties() {
    crate::test_utils::init_test_tracing();

    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A',
    ]));
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    let _ = put_string_property(
        &mut table,
        0,
        2,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(raw, table);

    let result =
        builtin_copy_sequence(vec![raw]).expect("copy-sequence should handle raw unibyte strings");
    let copied = result
        .as_lisp_string()
        .expect("copy-sequence should return a string");

    assert_ne!(
        result.bits(),
        raw.bits(),
        "non-empty strings should be copied"
    );
    assert!(!copied.is_multibyte());
    assert_eq!(copied.as_bytes(), &[0xFF, b'A']);

    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("copied string should keep text properties");
    let intervals = props.intervals_snapshot();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].start, 0);
    assert_eq!(intervals[0].end, 2);
    assert_eq!(
        intervals[0].properties.get(&Value::symbol("face")),
        Some(&Value::symbol("bold"))
    );
}

#[test]
fn list_copy_length_and_delq_signal_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();

    fn assert_circular(result: EvalResult) {
        match result {
            Err(crate::emacs_core::error::Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "circular-list");
            }
            other => panic!("expected circular-list signal, got {other:?}"),
        }
    }

    let copy_input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    copy_input.cons_cdr().set_cdr(copy_input);
    assert_circular(builtin_copy_sequence(vec![copy_input]));

    let length_input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    length_input.cons_cdr().set_cdr(length_input);
    assert_circular(builtin_length(vec![length_input]));

    let delq_input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    delq_input.cons_cdr().set_cdr(delq_input);
    assert_circular(builtin_delq(vec![Value::fixnum(9), delq_input]));

    let reverse_input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    reverse_input.cons_cdr().set_cdr(reverse_input);
    assert_circular(builtin_reverse(vec![reverse_input]));
}

#[test]
fn large_length_predicates_signal_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();

    fn assert_circular(result: EvalResult) {
        match result {
            Err(crate::emacs_core::error::Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "circular-list");
            }
            other => panic!("expected circular-list signal, got {other:?}"),
        }
    }

    for predicate in [
        builtin_length_lt as fn(Vec<Value>) -> EvalResult,
        builtin_length_gt,
        builtin_length_eq,
    ] {
        let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
        input.cons_cdr().set_cdr(input);
        assert_circular(predicate(vec![input, Value::fixnum(100_000)]));
    }
}

#[test]
fn vconcat_signals_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();

    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    input.cons_cdr().set_cdr(input);

    match builtin_vconcat(vec![input]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "circular-list");
        }
        other => panic!("expected circular-list signal, got {other:?}"),
    }
}

#[test]
fn nconc_nonfinal_circular_list_and_dotted_tail_match_gnu() {
    crate::test_utils::init_test_tracing();

    let cyclic = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    cyclic.cons_cdr().set_cdr(cyclic);
    match builtin_nconc(vec![cyclic, Value::list(vec![Value::fixnum(3)])]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "circular-list");
        }
        other => panic!("expected circular-list signal, got {other:?}"),
    }

    let dotted = Value::cons(Value::fixnum(1), Value::fixnum(2));
    let result = builtin_nconc(vec![dotted, Value::list(vec![Value::fixnum(3)])]).unwrap();
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![Value::fixnum(1), Value::fixnum(3)]
    );
}

#[test]
fn plist_get_circular_list_safe_tail_matches_gnu() {
    crate::test_utils::init_test_tracing();

    let missing = Value::list(vec![
        Value::keyword(":a"),
        Value::fixnum(1),
        Value::keyword(":b"),
        Value::fixnum(2),
    ]);
    missing.cons_cdr().cons_cdr().cons_cdr().set_cdr(missing);
    assert_eq!(
        builtin_plist_get(vec![missing, Value::keyword(":z")]).unwrap(),
        Value::NIL
    );

    let found = Value::list(vec![
        Value::keyword(":a"),
        Value::fixnum(1),
        Value::keyword(":b"),
        Value::fixnum(2),
    ]);
    found.cons_cdr().cons_cdr().cons_cdr().set_cdr(found);
    assert_eq!(
        builtin_plist_get(vec![found, Value::keyword(":b")]).unwrap(),
        Value::fixnum(2)
    );
}

#[test]
fn equal_circular_list_behavior_matches_gnu() {
    crate::test_utils::init_test_tracing();

    let same = Value::list(vec![Value::fixnum(1)]);
    same.set_cdr(same);
    assert_eq!(builtin_equal(vec![same, same]).unwrap(), Value::T);

    // GNU 31.0.90 (internal_equal_1 / internal_equal_cycle): two separate
    // self-circular lists with equal cars are `equal' (t), not a circular-list
    // error.  (Pre-31 GNU and old neomacs signaled circular-list here.)
    let left = Value::list(vec![Value::fixnum(1)]);
    left.set_cdr(left);
    let right = Value::list(vec![Value::fixnum(1)]);
    right.set_cdr(right);
    assert_eq!(builtin_equal(vec![left, right]).unwrap(), Value::T);

    let left = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    left.cons_cdr().set_cdr(left);
    let right = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    right.cons_cdr().set_cdr(right.cons_cdr());
    assert_eq!(builtin_equal(vec![left, right]).unwrap(), Value::NIL);
}

#[test]
fn string_match_inhibit_modify_preserves_match_data() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    let baseline = Value::list(vec![Value::fixnum(10), Value::fixnum(11)]);
    builtin_set_match_data(&mut eval, vec![baseline]).expect("seed baseline");

    let result = builtin_string_match(
        &mut eval,
        vec![
            Value::string("\\(foo\\)\\(bar\\)"),
            Value::string("foobar"),
            Value::NIL,
            Value::T,
        ],
    )
    .expect("string-match with inhibit-modify");
    assert_eq!(result, Value::fixnum(0));

    let observed = builtin_match_data(&mut eval, vec![]).expect("read match-data");
    assert_eq!(observed, baseline);
}

#[test]
fn string_match_honors_inhibit_changing_match_data() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    let baseline = Value::list(vec![Value::fixnum(10), Value::fixnum(11)]);
    builtin_set_match_data(&mut eval, vec![baseline]).expect("seed baseline");
    eval.set_variable("inhibit-changing-match-data", Value::T);

    let result = builtin_string_match(
        &mut eval,
        vec![Value::string("\\(foo\\)"), Value::string("foo")],
    )
    .expect("string-match with inhibited publication");
    assert_eq!(result, Value::fixnum(0));

    let observed = builtin_match_data(&mut eval, vec![]).expect("read match-data");
    assert_eq!(observed, baseline);
}

#[test]
fn string_match_handles_gnu_flex_regex_with_repeated_nongreedy_segments() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    let result = builtin_string_match(
        &mut eval,
        vec![
            Value::string("\\`[^z-a]*?f[^z-a]*?i[^z-a]*?n[^z-a]*?d[^z-a]*?-[^z-a]*?f[^z-a]*?"),
            Value::string("find-file"),
        ],
    )
    .expect("flex-style string-match should succeed");

    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn replace_match_missing_subexp_signals_error() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    builtin_string_match(
        &mut eval,
        vec![Value::string("\\(foo\\)"), Value::string("foo")],
    )
    .expect("seed match data");

    let result = builtin_replace_match(
        &mut eval,
        vec![
            Value::string("bar"),
            Value::NIL,
            Value::NIL,
            Value::string("foo"),
            Value::fixnum(2),
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
                && sig.data
                    == vec![
                        Value::string("replace-match subexpression does not exist"),
                        Value::fixnum(2),
                    ]
    ));
}

// Regex audit #11 / #12: `replace-match` must reject `\0` and unknown
// `\X` sequences with the same `"Invalid use of `\\' in replacement
// text"` error GNU raises at src/search.c:2584 and 2713. This is the
// builtin-facing sibling of the unit tests in `regex/tests.rs`; it
// verifies the error actually propagates through `builtin_replace_match`
// as a Lisp signal rather than being stringified into successful output.
// Regex audit #2: `posix-string-match` must use POSIX longest-match
// semantics. GNU `src/search.c:Fposix_string_match` calls
// `string_match_1` with `posix = 1`, which threads through
// `compile_pattern` into `re_match_2_internal`. The matcher tracks
// the best (longest) match across all backtracks
// (regex-emacs.c:4143-4344) and returns it via `restore_best_regs`.
// Before the fix `posix-string-match` was a silent alias for
// `string-match` and returned leftmost-first.
//
// Reference shape from GNU Emacs 31.0.50:
//   (string-match "a\\|aa\\|aaa" "aaaa")       => 0, m0="a"
//   (posix-string-match "a\\|aa\\|aaa" "aaaa") => 0, m0="aaa"
#[test]
fn posix_string_match_returns_longest_alternative_like_gnu() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();

    // Baseline: non-POSIX `string-match` returns the leftmost-first
    // alternative and sets match data accordingly.
    let result = builtin_string_match(
        &mut eval,
        vec![Value::string("a\\|aa\\|aaa"), Value::string("aaaa")],
    )
    .expect("string-match should succeed");
    assert_eq!(result, Value::fixnum(0));
    let observed = builtin_match_end(&mut eval, vec![Value::fixnum(0)]).expect("match-end 0");
    assert_eq!(observed, Value::fixnum(1), "non-POSIX matches 1 char 'a'");

    // POSIX: `posix-string-match` explores every alternative and
    // picks the longest.
    let result = builtin_posix_string_match(
        &mut eval,
        vec![Value::string("a\\|aa\\|aaa"), Value::string("aaaa")],
    )
    .expect("posix-string-match should succeed");
    assert_eq!(result, Value::fixnum(0));
    let observed = builtin_match_end(&mut eval, vec![Value::fixnum(0)]).expect("match-end 0");
    assert_eq!(
        observed,
        Value::fixnum(3),
        "POSIX picks the 3-character 'aaa' alternative"
    );
}

#[test]
fn posix_string_match_grouped_alternation_picks_longest_like_gnu() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();

    // Non-POSIX: m0="a", m1="a"
    let result = builtin_string_match(
        &mut eval,
        vec![Value::string("\\(a\\|ab\\|abc\\)"), Value::string("abcdef")],
    )
    .expect("string-match should succeed");
    assert_eq!(result, Value::fixnum(0));
    let end0 = builtin_match_end(&mut eval, vec![Value::fixnum(0)]).expect("match-end 0");
    assert_eq!(end0, Value::fixnum(1));

    // POSIX: m0="abc", m1="abc"
    let result = builtin_posix_string_match(
        &mut eval,
        vec![Value::string("\\(a\\|ab\\|abc\\)"), Value::string("abcdef")],
    )
    .expect("posix-string-match should succeed");
    assert_eq!(result, Value::fixnum(0));
    let end0 = builtin_match_end(&mut eval, vec![Value::fixnum(0)]).expect("match-end 0");
    assert_eq!(end0, Value::fixnum(3));
    let end1 = builtin_match_end(&mut eval, vec![Value::fixnum(1)]).expect("match-end 1");
    assert_eq!(end1, Value::fixnum(3));
}

#[test]
fn replace_match_rejects_backslash_zero_and_unknown_escape_like_gnu() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();

    // Seed match data via string-match against a plain string so the
    // replacement path is the Fstring-based one.
    builtin_string_match(&mut eval, vec![Value::string("foo"), Value::string("foo")])
        .expect("seed match data");

    // `\0` must signal, not return the whole match.
    let result = builtin_replace_match(
        &mut eval,
        vec![
            Value::string("\\0"),
            Value::NIL,
            Value::NIL,
            Value::string("foo"),
        ],
    );
    assert!(
        matches!(
            &result,
            Err(Flow::Signal(sig))
                if sig.symbol_name() == "error"
                    && sig.data
                        == vec![Value::string("Invalid use of \u{2018}\\\u{2019} in replacement text")]
        ),
        "expected Invalid use of backslash error for \\0, got {:?}",
        result
    );

    // `\n` (unknown escape) must signal.
    let result = builtin_replace_match(
        &mut eval,
        vec![
            Value::string("a\\nb"),
            Value::NIL,
            Value::NIL,
            Value::string("foo"),
        ],
    );
    assert!(
        matches!(
            &result,
            Err(Flow::Signal(sig))
                if sig.symbol_name() == "error"
                    && sig.data
                        == vec![Value::string("Invalid use of \u{2018}\\\u{2019} in replacement text")]
        ),
        "expected Invalid use of backslash error for \\n, got {:?}",
        result
    );

    // `\?` is GNU's exception (search.c:2583) and must be accepted,
    // passing through literally.
    let result = builtin_replace_match(
        &mut eval,
        vec![
            Value::string("a\\?b"),
            Value::NIL,
            Value::NIL,
            Value::string("foo"),
        ],
    )
    .expect("\\? must be accepted in replacement template");
    assert_eq!(result, Value::string("a\\?b"));
}

#[test]
fn replace_match_string_preserves_source_and_replacement_text_properties_like_gnu() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    fn put_face(value: Value, start: usize, end: usize, face: &str) {
        let mut table = crate::emacs_core::value::get_string_text_properties_table_for_value(value)
            .unwrap_or_else(crate::buffer::text_props::TextPropertyTable::new);
        let _ = put_string_property(
            &mut table,
            start,
            end,
            Value::symbol("face"),
            Value::symbol(face),
        );
        crate::emacs_core::value::set_string_text_properties_table_for_value(value, table);
    }

    fn assert_face_run(
        interval: &crate::buffer::text_props::PropertyInterval,
        start: usize,
        end: usize,
        face: &str,
    ) {
        assert_eq!(interval.start, start);
        assert_eq!(interval.end, end);
        assert_eq!(
            interval.properties.get(&Value::symbol("face")),
            Some(&Value::symbol(face))
        );
    }

    let mut eval = Context::new();
    let source = Value::string("abcde");
    put_face(source, 0, 2, "a");
    put_face(source, 2, 5, "b");
    let replacement = Value::string("XY");
    put_face(replacement, 0, 2, "x");

    builtin_string_match(&mut eval, vec![Value::string("bc"), source]).expect("seed match data");
    let result = builtin_replace_match(&mut eval, vec![replacement, Value::T, Value::NIL, source])
        .expect("replace-match should preserve string intervals");

    assert_eq!(result.as_runtime_string_owned().as_deref(), Some("aXYde"));
    let table = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("result should keep text properties");
    let intervals = table.intervals_snapshot();
    assert_eq!(intervals.len(), 3);
    assert_face_run(&intervals[0], 0, 1, "a");
    assert_face_run(&intervals[1], 1, 3, "x");
    assert_face_run(&intervals[2], 3, 5, "b");

    let source_table = crate::emacs_core::value::get_string_text_properties_table_for_value(source)
        .expect("source should remain propertized");
    let source_intervals = source_table.intervals_snapshot();
    assert_eq!(source_intervals.len(), 2);
    assert_face_run(&source_intervals[0], 0, 2, "a");
    assert_face_run(&source_intervals[1], 2, 5, "b");
}

#[test]
fn replace_match_without_active_match_data_signals_missing_subexp_like_gnu() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    builtin_set_match_data(&mut eval, vec![Value::NIL]).expect("clear match data");

    let result = builtin_replace_match(&mut eval, vec![Value::string("bar")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
                && sig.data
                    == vec![
                        Value::string("replace-match subexpression does not exist"),
                        Value::NIL,
                    ]
    ));
}

#[test]
fn replace_match_buffer_updates_live_match_data_like_gnu() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("foo-42");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    builtin_re_search_forward(&mut eval, vec![Value::string("\\([a-z]+\\)-\\([0-9]+\\)")])
        .expect("seed buffer match data");
    builtin_replace_match(&mut eval, vec![Value::string("\\2-\\1")])
        .expect("replace-match should succeed");

    let buffer = eval.buffers.current_buffer().expect("scratch buffer");
    assert_eq!(
        buffer.buffer_substring_range(crate::buffer::EmacsByteRange::from_usize(
            0,
            buffer.total_emacs_byte_len().get(),
        )),
        "42-foo"
    );

    assert_eq!(
        builtin_match_beginning(&mut eval, vec![Value::fixnum(0)]).expect("match-beginning 0"),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_match_end(&mut eval, vec![Value::fixnum(0)]).expect("match-end 0"),
        Value::fixnum(7)
    );
    assert_eq!(
        builtin_match_beginning(&mut eval, vec![Value::fixnum(1)]).expect("match-beginning 1"),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_match_end(&mut eval, vec![Value::fixnum(1)]).expect("match-end 1"),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_match_beginning(&mut eval, vec![Value::fixnum(2)]).expect("match-beginning 2"),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_match_end(&mut eval, vec![Value::fixnum(2)]).expect("match-end 2"),
        Value::fixnum(7)
    );
}

#[test]
fn match_data_translate_shifts_groups_in_shared_eval_state() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    builtin_set_match_data(
        &mut eval,
        vec![Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(4),
            Value::fixnum(2),
            Value::fixnum(3),
        ])],
    )
    .expect("seed match data");

    builtin_match_data_translate(&mut eval, vec![Value::fixnum(5)]).expect("translate match data");

    assert_eq!(
        builtin_match_data(&mut eval, vec![]).expect("read translated match data"),
        Value::list(vec![
            Value::fixnum(6),
            Value::fixnum(9),
            Value::fixnum(7),
            Value::fixnum(8),
        ])
    );
}

#[test]
fn looking_at_p_respects_case_fold_search() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("A");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    eval.set_variable("case-fold-search", Value::NIL);
    let sensitive = builtin_looking_at(&mut eval, vec![Value::string("a"), Value::NIL])
        .expect("looking-at with case-fold-search=nil");
    let sensitive_p = builtin_looking_at_p(&mut eval, vec![Value::string("a")])
        .expect("looking-at-p with case-fold-search=nil");
    assert!(sensitive.is_nil());
    assert!(sensitive_p.is_nil());

    eval.set_variable("case-fold-search", Value::T);
    let insensitive = builtin_looking_at(&mut eval, vec![Value::string("a"), Value::NIL])
        .expect("looking-at with case-fold-search=t");
    let insensitive_p = builtin_looking_at_p(&mut eval, vec![Value::string("a")])
        .expect("looking-at-p with case-fold-search=t");
    assert!(insensitive.is_truthy());
    assert!(insensitive_p.is_truthy());
}

/// Regression for `drafts/regex-search-audit.md` finding #3: the
/// search builtins must honor per-buffer overrides of
/// `case-fold-search`. Before the fix, the helper
/// `dynamic_or_global_symbol_value` called
/// `obarray.symbol_value("case-fold-search")` which returns the BLV
/// default cell for `SymbolValue::BufferLocal`, silently ignoring the
/// current buffer's per-buffer binding. The fix routes the read
/// through `eval_symbol_by_id`, which walks lexenv → alias →
/// LOCALIZED `read_localized` (with BLV swap-in to the current
/// buffer's valcell) → buffer-local → FORWARDED → global, mirroring
/// GNU `find_symbol_value` at `src/data.c:1584-1609`.
///
/// Scenario: the global default of `case-fold-search` stays `t`
/// while the current buffer has a per-buffer binding of `nil` via
/// `(setq-local case-fold-search nil)`. With the broken helper, the
/// search reads the default (`t`) and matches case-insensitively;
/// with the fix it reads the per-buffer binding (`nil`) and matches
/// case-sensitively.
#[test]
fn looking_at_honors_per_buffer_case_fold_search() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("A");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    // Confirm the global default is t.
    eval.set_variable("case-fold-search", Value::T);

    // Install a per-buffer binding of nil in the current buffer.
    // `setq-local` is a macro not defined in a bare `Context::new()`,
    // so expand it by hand: `make-local-variable` then `set`.
    let make_local = Value::list(vec![
        Value::symbol("make-local-variable"),
        Value::list(vec![
            Value::symbol("quote"),
            Value::symbol("case-fold-search"),
        ]),
    ]);
    eval.eval_value(&make_local).expect("make-local-variable");
    let set_form = Value::list(vec![
        Value::symbol("set"),
        Value::list(vec![
            Value::symbol("quote"),
            Value::symbol("case-fold-search"),
        ]),
        Value::NIL,
    ]);
    eval.eval_value(&set_form)
        .expect("set case-fold-search nil (per-buffer)");

    // With the per-buffer binding in place, looking-at "a" against
    // "A" must fail (case-sensitive match, no hit).
    let result = builtin_looking_at(&mut eval, vec![Value::string("a"), Value::NIL])
        .expect("looking-at after setq-local");
    assert!(
        result.is_nil(),
        "per-buffer case-fold-search=nil should make pattern case-sensitive; got {:?}",
        result,
    );

    // And `default-value` should still report the global default
    // (`t`), verifying our per-buffer binding didn't leak into the
    // default cell.
    let default_form = Value::list(vec![
        Value::symbol("default-value"),
        Value::list(vec![
            Value::symbol("quote"),
            Value::symbol("case-fold-search"),
        ]),
    ]);
    let default_val = eval
        .eval_value(&default_form)
        .expect("default-value case-fold-search");
    assert!(
        default_val.is_truthy(),
        "default-value should remain t; got {:?}",
        default_val,
    );
}

/// Regression for `drafts/regex-search-audit.md` finding #4:
/// `inhibit-changing-match-data` was documented by GNU but never
/// read anywhere in our source. GNU `src/search.c:282, 376, 1168,
/// 2053` all start with
///
///     bool modify_match_data = NILP (Vinhibit_changing_match_data)
///                              && modify_data;
///
/// so when the variable is non-nil the search does not publish its
/// successful match data and the caller's match data stays frozen.
/// The Rust fix: every Context-facing search wrapper
/// (`looking-at`, `search-forward`, `re-search-forward`, etc.)
/// checks the variable via `read_inhibit_changing_match_data` and
/// passes no match-data commit target when set.
///
/// Scenario: run `re-search-forward` twice. The first call (with
/// the variable nil) primes `eval.match_data` to match `"a"`. The
/// second call (with the variable non-nil) runs against `"b"` in
/// the same buffer; without the fix, this clobbers `eval.match_data`
/// to the new match. With the fix, the new search result is not
/// stored — `eval.match_data` still points at the `"a"` match.
#[test]
fn re_search_forward_honors_inhibit_changing_match_data() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("axxxxb");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    eval.set_variable("case-fold-search", Value::T);
    eval.set_variable("inhibit-changing-match-data", Value::NIL);

    // Prime match data with a successful "a" search. The exact
    // numeric form of `match_data.group(0)` is an internal detail
    // (1-based marker span over bytes); we only need a reference
    // snapshot to compare against after the second search.
    let first = builtin_re_search_forward(&mut eval, vec![Value::string("a")])
        .expect("first re-search-forward 'a'");
    // Marker positions are 1-based in our match_data layout, so the
    // char position after the "a" match at buffer head is 2.
    assert_eq!(first.as_fixnum(), Some(2), "point should be after 'a'");
    let primed_span = eval
        .match_data
        .as_ref()
        .and_then(|md| md.group(0))
        .expect("match data should be populated after a successful search");

    // Set the global and move point back to the start.
    eval.set_variable("inhibit-changing-match-data", Value::T);
    {
        let buf = eval.buffers.current_buffer_mut().expect("scratch");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    // Run a fresh search against "b" — the evaluator's match_data
    // must NOT update. The return value of re-search-forward is
    // still the character position of the match (GNU `search.c:422`
    // documents the return being independent of match-data update).
    let second = builtin_re_search_forward(&mut eval, vec![Value::string("b")])
        .expect("second re-search-forward 'b' with inhibit set");
    assert_eq!(second.as_fixnum(), Some(7), "search still returns position");

    let span_after_second = eval
        .match_data
        .as_ref()
        .and_then(|md| md.group(0))
        .expect("match data snapshot after second search");
    assert_eq!(
        span_after_second, primed_span,
        "inhibit-changing-match-data=t should freeze match_data at the 'a' span; \
         expected {:?}, got {:?}",
        primed_span, span_after_second,
    );
}

#[test]
fn dispatch_builtin_resolves_typed_only_pure_names() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = dispatch_builtin(
        &mut eval,
        "string-equal",
        vec![Value::string("neo"), Value::string("neo")],
    )
    .expect("dispatch_builtin should resolve string-equal")
    .expect("dispatch_builtin should evaluate string-equal");
    assert!(result.is_truthy());
}

#[test]
fn dispatch_builtin_pure_defers_evaluator_window_accessors_and_mutators() {
    crate::test_utils::init_test_tracing();
    assert!(dispatch_builtin_pure("functionp", vec![]).is_none());
    assert!(dispatch_builtin_pure("copy-file", vec![]).is_none());
    assert!(dispatch_builtin_pure("defvaralias", vec![]).is_none());
    assert!(dispatch_builtin_pure("delete-file", vec![]).is_none());
    assert!(dispatch_builtin_pure("display-color-p", vec![]).is_none());
    // format and format-message are correctly in pure dispatch (GNU editfns.c —
    // they don't need eval state).
    assert!(dispatch_builtin_pure("indirect-variable", vec![]).is_none());
    assert!(dispatch_builtin_pure("insert-and-inherit", vec![]).is_none());
    assert!(dispatch_builtin_pure("insert-before-markers-and-inherit", vec![]).is_none());
    assert!(dispatch_builtin_pure("insert-buffer-substring", vec![]).is_none());
    assert!(dispatch_builtin_pure("kill-all-local-variables", vec![]).is_none());
    assert!(dispatch_builtin_pure("make-directory", vec![]).is_none());
    assert!(dispatch_builtin_pure("make-terminal-frame", vec![]).is_none());
    assert!(dispatch_builtin_pure("make-temp-file", vec![]).is_none());
    assert!(dispatch_builtin_pure("macroexpand", vec![]).is_none());
    assert!(dispatch_builtin_pure("message", vec![]).is_none());
    assert!(dispatch_builtin_pure("message-box", vec![]).is_none());
    assert!(dispatch_builtin_pure("message-or-box", vec![]).is_none());
    assert!(dispatch_builtin_pure("error", vec![]).is_none());
    assert!(dispatch_builtin_pure("princ", vec![]).is_none());
    assert!(dispatch_builtin_pure("prin1", vec![]).is_none());
    assert!(dispatch_builtin_pure("prin1-to-string", vec![]).is_none());
    assert!(dispatch_builtin_pure("print", vec![]).is_none());
    assert!(dispatch_builtin_pure("rename-file", vec![]).is_none());
    assert!(dispatch_builtin_pure("replace-buffer-contents", vec![]).is_none());
    assert!(dispatch_builtin_pure("set-buffer-multibyte", vec![]).is_none());
    assert!(dispatch_builtin_pure("split-window-internal", vec![]).is_none());
    assert!(dispatch_builtin_pure("setplist", vec![]).is_none());
    assert!(dispatch_builtin_pure("terminal-live-p", vec![]).is_none());
    assert!(dispatch_builtin_pure("terminal-name", vec![]).is_none());
    assert!(dispatch_builtin_pure("terpri", vec![]).is_none());
    assert!(dispatch_builtin_pure("translate-region-internal", vec![]).is_none());
    assert!(dispatch_builtin_pure("undo-boundary", vec![]).is_none());
    assert!(dispatch_builtin_pure("write-char", vec![]).is_none());
    assert!(dispatch_builtin_pure("assoc", vec![]).is_none());
    assert!(dispatch_builtin_pure("alist-get", vec![]).is_none());
    assert!(dispatch_builtin_pure("plist-member", vec![]).is_none());
    assert!(dispatch_builtin_pure("old-selected-window", vec![]).is_none());
    assert!(dispatch_builtin_pure("frame-old-selected-window", vec![]).is_none());
    assert!(dispatch_builtin_pure("set-frame-selected-window", vec![]).is_none());
    assert!(dispatch_builtin_pure("window-system", vec![]).is_none());
    assert!(dispatch_builtin_pure("frame-edges", vec![]).is_none());
    assert!(dispatch_builtin_pure("window-at", vec![]).is_none());
    assert!(dispatch_builtin_pure("window-bump-use-time", vec![]).is_none());
    assert!(dispatch_builtin_pure("window-list-1", vec![]).is_none());
    assert!(dispatch_builtin_pure("add-variable-watcher", vec![]).is_none());
    assert!(dispatch_builtin_pure("remove-variable-watcher", vec![]).is_none());
    assert!(dispatch_builtin_pure("get-variable-watchers", vec![]).is_none());
}

#[test]
fn dispatch_builtin_pure_handles_treesit_parser_and_query_entrypoints() {
    crate::test_utils::init_test_tracing();
    let err = dispatch_builtin_pure("treesit-parser-buffer", vec![Value::NIL])
        .expect("treesit-parser-buffer should resolve")
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let err = dispatch_builtin_pure("treesit-query-compile", vec![Value::NIL])
        .expect("treesit-query-compile should resolve")
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn treesit_query_compile_creates_lazy_compiled_query_record() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let query = crate::emacs_core::builtins::builtin_treesit_query_compile(
        &mut eval,
        vec![
            Value::symbol("definitely-missing-language"),
            Value::string("(identifier) @id"),
            Value::NIL,
        ],
    )
    .expect("lazy treesit-query-compile should not need a loaded grammar");

    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_compiled_query_p(vec![query]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_p(vec![query]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_language(vec![query]).unwrap(),
        Value::symbol("definitely-missing-language")
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_source(vec![query]).unwrap(),
        Value::string("(identifier) @id")
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_eagerly_compiled_p(
            &mut eval,
            vec![query],
        )
        .unwrap(),
        Value::NIL
    );
}

#[test]
fn treesit_query_compile_reuses_compiled_query_when_language_is_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);
    let query = crate::emacs_core::builtins::builtin_treesit_query_compile(
        &mut eval,
        vec![
            Value::symbol("json"),
            Value::string("(object) @object"),
            Value::NIL,
        ],
    )
    .expect("initial lazy query should compile");

    let reused = crate::emacs_core::builtins::builtin_treesit_query_compile(
        &mut eval,
        vec![Value::NIL, query, Value::T],
    )
    .expect("a compiled query should supply its own language");

    assert_eq!(reused, query);
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_eagerly_compiled_p(
            &mut eval,
            vec![query],
        )
        .unwrap(),
        Value::T
    );
}

#[test]
fn treesit_query_compile_preserves_nil_language_for_lazy_query() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let source = Value::string("(object) @object");

    let query = crate::emacs_core::builtins::builtin_treesit_query_compile(
        &mut eval,
        vec![Value::NIL, source, Value::NIL],
    )
    .expect("nil is a symbol and lazy compilation should not load its grammar");

    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_compiled_query_p(vec![query]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_language(vec![query]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_source(vec![query]).unwrap(),
        source
    );
}

#[test]
fn treesit_query_expand_and_pattern_expand_follow_gnu_sexp_forms() {
    crate::test_utils::init_test_tracing();
    let query = Value::list(vec![
        Value::list(vec![Value::symbol("identifier")]),
        Value::symbol("@id"),
        Value::keyword(":anchor"),
        Value::string("return"),
        Value::symbol("@ret"),
        Value::keyword(":equal"),
        Value::symbol("@lhs"),
        Value::symbol("@rhs"),
    ]);

    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_pattern_expand(vec![Value::keyword(
            ":anchor",
        )])
        .unwrap(),
        Value::string(".")
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_pattern_expand(vec![Value::list(vec![
            Value::symbol("identifier"),
        ])])
        .unwrap(),
        Value::string("(identifier)")
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_pattern_expand(vec![
            Value::keyword(":equal",)
        ])
        .unwrap(),
        Value::string("#eq?")
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_expand(vec![query]).unwrap(),
        Value::string("(identifier) @id . \"return\" @ret #eq? @lhs @rhs")
    );
}

#[test]
fn treesit_query_compile_eager_missing_language_signals_treesit_query_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let err = crate::emacs_core::builtins::builtin_treesit_query_compile(
        &mut eval,
        vec![
            Value::symbol("definitely-missing-language"),
            Value::string("(identifier) @id"),
            Value::T,
        ],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "treesit-query-error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn treesit_query_capture_runs_against_cached_json_grammar() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);
    let buffer = create_unique_test_buffer(&mut eval, "*treesit-json*");
    let _ = eval.buffers.switch_current(buffer.as_buffer_id().unwrap());
    builtin_insert(&mut eval, vec![Value::string("{\"a\": 1, \"b\": 2}")]).unwrap();

    let parser = crate::emacs_core::builtins::builtin_treesit_parser_create(
        &mut eval,
        vec![Value::symbol("json"), Value::NIL, Value::T, Value::NIL],
    )
    .unwrap();
    let captures = crate::emacs_core::builtins::builtin_treesit_query_capture(
        &mut eval,
        vec![
            parser,
            Value::string("(object) @obj"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .unwrap();
    let items = crate::emacs_core::value::list_to_vec(&captures).unwrap();
    assert!(!items.is_empty());
    let first = items[0];
    assert!(first.is_cons());
    assert_eq!(first.cons_car(), Value::symbol("obj"));
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_node_type(&mut eval, vec![first.cons_cdr()])
            .unwrap(),
        Value::string("object")
    );
}

#[test]
fn treesit_query_compile_and_capture_accept_sexp_queries() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);
    let buffer = create_unique_test_buffer(&mut eval, "*treesit-json-sexp*");
    let _ = eval.buffers.switch_current(buffer.as_buffer_id().unwrap());
    builtin_insert(&mut eval, vec![Value::string("{\"a\": 1, \"b\": 2}")]).unwrap();

    let query = Value::list(vec![
        Value::list(vec![Value::symbol("object")]),
        Value::symbol("@obj"),
    ]);
    let compiled = crate::emacs_core::builtins::builtin_treesit_query_compile(
        &mut eval,
        vec![Value::symbol("json"), query, Value::T],
    )
    .unwrap();

    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_query_expand(vec![compiled]).unwrap(),
        Value::string("(object) @obj")
    );

    let parser = crate::emacs_core::builtins::builtin_treesit_parser_create(
        &mut eval,
        vec![Value::symbol("json"), Value::NIL, Value::T, Value::NIL],
    )
    .unwrap();
    let captures = crate::emacs_core::builtins::builtin_treesit_query_capture(
        &mut eval,
        vec![
            parser,
            compiled,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .unwrap();
    let items = crate::emacs_core::value::list_to_vec(&captures).unwrap();
    assert!(!items.is_empty());
    assert_eq!(items[0].cons_car(), Value::symbol("obj"));
}

#[test]
fn treesit_search_and_match_work_against_cached_json_grammar() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);
    let buffer = create_unique_test_buffer(&mut eval, "*treesit-search*");
    let _ = eval.buffers.switch_current(buffer.as_buffer_id().unwrap());
    builtin_insert(&mut eval, vec![Value::string("{\"a\": 1, \"b\": 2}")]).unwrap();

    let parser = crate::emacs_core::builtins::builtin_treesit_parser_create(
        &mut eval,
        vec![Value::symbol("json"), Value::NIL, Value::T, Value::NIL],
    )
    .unwrap();
    let root =
        crate::emacs_core::builtins::builtin_treesit_parser_root_node(&mut eval, vec![parser])
            .unwrap();
    let number = crate::emacs_core::builtins::builtin_treesit_search_subtree(
        &mut eval,
        vec![
            root,
            Value::string("number"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .unwrap();
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_node_type(&mut eval, vec![number]).unwrap(),
        Value::string("number")
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_node_match_p(
            &mut eval,
            vec![number, Value::string("number"), Value::NIL],
        )
        .unwrap(),
        Value::T
    );
}

#[test]
fn treesit_parse_string_and_linecol_cache_are_runtime_backed() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);

    let root = crate::emacs_core::builtins::builtin_treesit_parse_string(
        &mut eval,
        vec![Value::string("{\"x\": 1}"), Value::symbol("json")],
    )
    .unwrap();
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_node_type(&mut eval, vec![root]).unwrap(),
        Value::string("document")
    );

    let buffer = create_unique_test_buffer(&mut eval, "*treesit-linecol*");
    let _ = eval.buffers.switch_current(buffer.as_buffer_id().unwrap());
    builtin_insert(&mut eval, vec![Value::string("a\nbc")]).unwrap();
    crate::emacs_core::builtins::builtin_treesit_linecol_cache_set(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)],
    )
    .unwrap();
    let linecol =
        crate::emacs_core::builtins::builtin_treesit_linecol_at(&mut eval, vec![Value::fixnum(3)])
            .unwrap();
    assert_eq!(linecol.cons_car(), Value::fixnum(2));
    assert_eq!(linecol.cons_cdr(), Value::fixnum(1));
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_tracking_line_column_p(&mut eval, vec![],)
            .unwrap(),
        Value::T
    );
}

#[test]
fn treesit_parse_string_temp_buffer_is_cleaned_up_with_parser_delete() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);

    let root = crate::emacs_core::builtins::builtin_treesit_parse_string(
        &mut eval,
        vec![Value::string("{\"x\": 1}"), Value::symbol("json")],
    )
    .unwrap();
    let parser = crate::emacs_core::builtins::builtin_treesit_node_parser(vec![root]).unwrap();
    let buffer =
        crate::emacs_core::builtins::builtin_treesit_parser_buffer(&mut eval, vec![parser])
            .unwrap();
    let buffer_id = buffer.as_buffer_id().unwrap();
    assert!(eval.buffers.get(buffer_id).is_some());

    crate::emacs_core::builtins::builtin_treesit_parser_delete(&mut eval, vec![parser]).unwrap();
    assert!(eval.buffers.get(buffer_id).is_none());
}

#[test]
fn treesit_parser_create_enables_linecol_tracking_for_configured_languages() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);
    eval.obarray.set_symbol_value(
        "treesit-languages-require-line-column-tracking",
        Value::list(vec![Value::symbol("json")]),
    );

    let buffer = create_unique_test_buffer(&mut eval, "*treesit-linecol-parser*");
    let _ = eval.buffers.switch_current(buffer.as_buffer_id().unwrap());
    builtin_insert(&mut eval, vec![Value::string("{\"x\": 1}")]).unwrap();

    let parser = crate::emacs_core::builtins::builtin_treesit_parser_create(
        &mut eval,
        vec![Value::symbol("json"), Value::NIL, Value::T, Value::NIL],
    )
    .unwrap();

    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_parser_tracking_line_column_p(
            &mut eval,
            vec![parser],
        )
        .unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::builtins::builtin_treesit_tracking_line_column_p(&mut eval, vec![])
            .unwrap(),
        Value::T
    );
}

#[test]
fn treesit_linecol_cache_invalidates_when_edit_moves_before_cached_point() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer = create_unique_test_buffer(&mut eval, "*treesit-linecol-edit*");
    let _ = eval.buffers.switch_current(buffer.as_buffer_id().unwrap());
    builtin_insert(&mut eval, vec![Value::string("a\nbc")]).unwrap();

    crate::emacs_core::builtins::builtin_treesit_linecol_cache_set(
        &mut eval,
        vec![Value::fixnum(2), Value::fixnum(2), Value::fixnum(3)],
    )
    .unwrap();
    builtin_goto_char(&mut eval, vec![Value::fixnum(1)]).unwrap();
    builtin_insert(&mut eval, vec![Value::string("z")]).unwrap();

    let cache =
        crate::emacs_core::builtins::builtin_treesit_linecol_cache(&mut eval, vec![]).unwrap();
    assert_eq!(
        crate::emacs_core::builtins::builtin_plist_get(vec![cache, Value::keyword(":bytepos")],)
            .unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn treesit_parser_changed_regions_reports_buffer_edits() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_test_treesit_json_language(&mut eval);
    let buffer = create_unique_test_buffer(&mut eval, "*treesit-changes*");
    let _ = eval.buffers.switch_current(buffer.as_buffer_id().unwrap());
    builtin_insert(&mut eval, vec![Value::string("{\"a\": 1}")]).unwrap();

    let parser = crate::emacs_core::builtins::builtin_treesit_parser_create(
        &mut eval,
        vec![Value::symbol("json"), Value::NIL, Value::T, Value::NIL],
    )
    .unwrap();
    let _ = crate::emacs_core::builtins::builtin_treesit_parser_root_node(&mut eval, vec![parser])
        .unwrap();

    builtin_goto_char(&mut eval, vec![Value::fixnum(8)]).unwrap();
    builtin_insert(&mut eval, vec![Value::string(", \"b\": 2")]).unwrap();

    let changed = crate::emacs_core::builtins::builtin_treesit_parser_changed_regions(
        &mut eval,
        vec![parser],
    )
    .unwrap();
    let regions = crate::emacs_core::value::list_to_vec(&changed).unwrap();
    assert!(!regions.is_empty());
    assert!(regions[0].is_cons());
}

#[test]
fn gnu_treesit_el_high_level_helpers_work_with_runtime_json_grammar() {
    crate::test_utils::init_test_tracing();
    let mut eval = runtime_startup_context();
    install_test_treesit_json_language(&mut eval);

    eval.eval_str("(require 'treesit)")
        .expect("load GNU treesit.el");

    let value = eval
        .eval_str(
            r#"
            (with-temp-buffer
              (insert "{\"a\": 1}")
              (let ((parser (treesit-parser-create 'json)))
                (setq treesit-primary-parser parser)
                (list
                 (treesit-ready-p 'json t)
                 (treesit-node-type (treesit-buffer-root-node))
                 (treesit-node-text (treesit-buffer-root-node) t)
                 (mapcar #'car
                         (treesit-query-string
                          "{\"a\": 1}"
                          '((object) @obj)
                          'json)))))
            "#,
        )
        .expect("evaluate GNU treesit.el helper forms");

    let values = crate::emacs_core::value::list_to_vec(&value).expect("helper result list");
    assert_eq!(values.len(), 4);
    assert_eq!(values[0], Value::T);
    assert_eq!(values[1], Value::string("document"));
    assert_eq!(values[2], Value::string("{\"a\": 1}"));

    let captures = crate::emacs_core::value::list_to_vec(&values[3]).expect("capture names");
    assert_eq!(captures, vec![Value::symbol("obj")]);
}

#[test]
fn dispatch_builtin_pure_handles_inotify_watch_lifecycle() {
    crate::test_utils::init_test_tracing();
    let watch = dispatch_builtin_pure(
        "inotify-add-watch",
        vec![Value::string("."), Value::NIL, Value::symbol("ignore")],
    )
    .expect("inotify-add-watch should resolve")
    .expect("inotify-add-watch should evaluate");
    let active = dispatch_builtin_pure("inotify-valid-p", vec![watch])
        .expect("inotify-valid-p should resolve")
        .expect("inotify-valid-p should evaluate");
    assert_eq!(active, Value::T);

    let removed = dispatch_builtin_pure("inotify-rm-watch", vec![watch])
        .expect("inotify-rm-watch should resolve")
        .expect("inotify-rm-watch should evaluate");
    assert_eq!(removed, Value::T);

    let inactive = dispatch_builtin_pure("inotify-valid-p", vec![watch])
        .expect("inotify-valid-p should resolve")
        .expect("inotify-valid-p should evaluate");
    assert_eq!(inactive, Value::NIL);

    let removed_again = dispatch_builtin_pure("inotify-rm-watch", vec![watch])
        .expect("inotify-rm-watch should resolve")
        .expect("inotify-rm-watch should evaluate");
    assert_eq!(removed_again, Value::T);
}

#[test]
fn dispatch_builtin_pure_reports_bundled_sqlite() {
    crate::test_utils::init_test_tracing();
    let available = dispatch_builtin_pure("sqlite-available-p", vec![])
        .expect("sqlite-available-p should resolve")
        .expect("sqlite-available-p should evaluate");
    assert_eq!(available, Value::T);

    let sqlitep = dispatch_builtin_pure("sqlitep", vec![Value::symbol("not-a-db")])
        .expect("sqlitep should resolve")
        .expect("sqlitep should evaluate");
    assert_eq!(sqlitep, Value::NIL);
}

#[test]
fn dispatch_builtin_pure_handles_fillarray_and_find_coding_region_internal() {
    crate::test_utils::init_test_tracing();
    let vector = Value::vector(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    let filled = dispatch_builtin_pure("fillarray", vec![vector, Value::fixnum(9)])
        .expect("fillarray should resolve")
        .expect("fillarray should evaluate");
    if !filled.is_vector() {
        panic!("expected vector");
    };
    assert_eq!(
        &*filled.as_vector_data().unwrap().clone(),
        &[Value::fixnum(9), Value::fixnum(9), Value::fixnum(9)]
    );

    assert!(
        dispatch_builtin_pure(
            "find-coding-systems-region-internal",
            vec![Value::fixnum(0), Value::fixnum(1)]
        )
        .is_none(),
        "find-coding-systems-region-internal should use the eval-aware path"
    );

    let mut eval = crate::emacs_core::eval::Context::new();
    let coding = dispatch_builtin(
        &mut eval,
        "find-coding-systems-region-internal",
        vec![Value::string("hello"), Value::NIL],
    )
    .expect("find-coding-systems-region-internal should resolve")
    .expect("find-coding-systems-region-internal should evaluate");
    assert_eq!(coding, Value::T);
}

#[test]
fn dispatch_builtin_pure_fillarray_preserves_unibyte_strings() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'b', b'c',
    ]));
    let filled = dispatch_builtin_pure("fillarray", vec![raw, Value::fixnum('A' as i64)])
        .expect("fillarray should resolve")
        .expect("fillarray should evaluate");
    let filled = filled.as_lisp_string().expect("filled string");
    assert!(!filled.is_multibyte());
    assert_eq!(filled.as_bytes(), b"AAA");
}

#[test]
fn dispatch_builtin_pure_fillarray_multibyte_matches_gnu_byte_length_rule() {
    crate::test_utils::init_test_tracing();
    let multibyte = Value::multibyte_string("éé");
    let filled = dispatch_builtin_pure("fillarray", vec![multibyte, Value::fixnum(256)])
        .expect("fillarray should resolve")
        .expect("fillarray should evaluate");
    let filled_string = filled.as_lisp_string().expect("filled string");
    assert!(filled_string.is_multibyte());
    assert_eq!(filled_string.schars(), 2);
    assert_eq!(filled_string.sbytes(), 4);
    assert_eq!(decode_value_char_codes(&filled), vec![256, 256]);

    let ascii_fill = dispatch_builtin_pure(
        "fillarray",
        vec![Value::multibyte_string("éé"), Value::fixnum(b'x' as i64)],
    )
    .expect("fillarray should resolve")
    .expect_err("GNU fillarray rejects multibyte byte-length changes");
    match ascii_fill {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Attempt to change byte length of a string")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn dispatch_builtin_pure_clear_string_makes_unibyte_and_removes_properties() {
    crate::test_utils::init_test_tracing();
    let string = Value::multibyte_string_with_text_properties(
        "é",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );
    assert!(get_string_text_properties_for_value(string).is_some());

    let result = dispatch_builtin_pure("clear-string", vec![string])
        .expect("clear-string should resolve")
        .expect("clear-string should evaluate");
    assert_eq!(result, Value::NIL);

    let cleared = string.as_lisp_string().expect("cleared string");
    assert!(!cleared.is_multibyte());
    assert_eq!(cleared.schars(), 2);
    assert_eq!(cleared.sbytes(), 2);
    assert_eq!(cleared.as_bytes(), &[0, 0]);
    assert!(get_string_text_properties_for_value(string).is_none());
}

#[test]
fn dispatch_builtin_pure_handles_fringe_display_and_debug_output_placeholders() {
    crate::test_utils::init_test_tracing();
    let bitmap = dispatch_builtin_pure(
        "define-fringe-bitmap",
        vec![Value::symbol("neo"), Value::vector(vec![Value::fixnum(0)])],
    )
    .expect("define-fringe-bitmap should resolve")
    .expect("define-fringe-bitmap should evaluate");
    assert_eq!(bitmap, Value::symbol("neo"));

    let destroy = dispatch_builtin_pure("destroy-fringe-bitmap", vec![Value::symbol("neo")])
        .expect("destroy-fringe-bitmap should resolve")
        .expect("destroy-fringe-bitmap should evaluate");
    assert_eq!(destroy, Value::NIL);

    let line = dispatch_builtin_pure("display--line-is-continued-p", vec![])
        .expect("display--line-is-continued-p should resolve")
        .expect("display--line-is-continued-p should evaluate");
    assert_eq!(line, Value::NIL);

    let autosave = dispatch_builtin_pure("do-auto-save", vec![])
        .expect("do-auto-save should resolve")
        .expect("do-auto-save should evaluate");
    assert_eq!(autosave, Value::NIL);

    let err = dispatch_builtin_pure("external-debugging-output", vec![Value::fixnum(-1)])
        .expect("external-debugging-output should resolve")
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn defined_fringe_bitmap_can_receive_face_until_destroyed() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"
            (progn
              (define-fringe-bitmap 'vm-test-fringe [0 24 60 126])
              (list (get 'vm-test-fringe 'fringe)
                    (set-fringe-bitmap-face 'vm-test-fringe 'success)
                    (destroy-fringe-bitmap 'vm-test-fringe)
                    (get 'vm-test-fringe 'fringe)))
            "#,
        )
        .expect("defined fringe bitmap sequence should evaluate");

    // `define-fringe-bitmap` assigns a registry index stored on the `'fringe`
    // property. User bitmaps start at 25, the slot just past GNU's 25 standard
    // built-in bitmaps. The `set-fringe-bitmap-face`/`destroy-fringe-bitmap`
    // calls still return nil, and `'fringe` is cleared after destroy.
    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "(25 nil nil nil)"
    );

    let err = dispatch_builtin(
        &mut eval,
        "set-fringe-bitmap-face",
        vec![Value::symbol("vm-test-fringe"), Value::symbol("success")],
    )
    .expect("set-fringe-bitmap-face should resolve")
    .expect_err("destroyed fringe bitmap should no longer accept a face");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                crate::emacs_core::print::print_value(&sig.data[0]),
                "\"Undefined fringe bitmap\""
            );
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn set_fringe_bitmap_face_rejects_non_symbol_bitmap_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let err = dispatch_builtin(
        &mut eval,
        "set-fringe-bitmap-face",
        vec![Value::fixnum(1), Value::symbol("success")],
    )
    .expect("set-fringe-bitmap-face should resolve")
    .expect_err("non-symbol bitmap should signal");

    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn mouse_position_builtins_default_to_selected_frame_with_nil_coords() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let selected = dispatch_builtin(&mut eval, "selected-frame", vec![])
        .expect("selected-frame should resolve")
        .expect("selected-frame should evaluate");

    let pixel = dispatch_builtin(&mut eval, "mouse-pixel-position", vec![])
        .expect("mouse-pixel-position should resolve")
        .expect("mouse-pixel-position should evaluate");
    assert_eq!(
        pixel,
        Value::cons(selected, Value::cons(Value::NIL, Value::NIL))
    );

    let pos = dispatch_builtin(&mut eval, "mouse-position", vec![])
        .expect("mouse-position should resolve")
        .expect("mouse-position should evaluate");
    assert_eq!(
        pos,
        Value::cons(selected, Value::cons(Value::NIL, Value::NIL))
    );
}

#[test]
fn display_update_for_mouse_movement_updates_shared_mouse_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let frame = dispatch_builtin(&mut eval, "selected-frame", vec![])
        .expect("selected-frame should resolve")
        .expect("selected-frame should evaluate");
    if !frame.is_frame() {
        panic!("selected-frame should return a frame");
    };
    let frame_id = frame.as_frame_id().unwrap();
    if let Some(frame) = eval.frames.get_mut(crate::window::FrameId(frame_id)) {
        frame.char_width = 8.0;
        frame.char_height = 16.0;
    }

    let update = dispatch_builtin(
        &mut eval,
        "display--update-for-mouse-movement",
        vec![frame, Value::fixnum(16), Value::fixnum(32)],
    )
    .expect("display--update-for-mouse-movement should resolve")
    .expect("display--update-for-mouse-movement should evaluate");
    assert_eq!(update, Value::NIL);

    let pixel = dispatch_builtin(&mut eval, "mouse-pixel-position", vec![])
        .expect("mouse-pixel-position should resolve")
        .expect("mouse-pixel-position should evaluate");
    assert_eq!(
        pixel,
        Value::cons(frame, Value::cons(Value::fixnum(16), Value::fixnum(32)))
    );

    let pos = dispatch_builtin(&mut eval, "mouse-position", vec![])
        .expect("mouse-position should resolve")
        .expect("mouse-position should evaluate");
    assert_eq!(
        pos,
        Value::cons(frame, Value::cons(Value::fixnum(2), Value::fixnum(2)))
    );
}

#[test]
fn set_mouse_position_builtins_update_shared_mouse_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let frame = dispatch_builtin(&mut eval, "selected-frame", vec![])
        .expect("selected-frame should resolve")
        .expect("selected-frame should evaluate");
    if !frame.is_frame() {
        panic!("selected-frame should return a frame");
    };
    let frame_id = frame.as_frame_id().unwrap();
    if let Some(frame) = eval.frames.get_mut(crate::window::FrameId(frame_id)) {
        frame.char_width = 8.0;
        frame.char_height = 16.0;
    }

    let set_pixel = dispatch_builtin(
        &mut eval,
        "set-mouse-pixel-position",
        vec![frame, Value::fixnum(9), Value::fixnum(17)],
    )
    .expect("set-mouse-pixel-position should resolve")
    .expect("set-mouse-pixel-position should evaluate");
    assert_eq!(set_pixel, Value::NIL);
    assert_eq!(
        dispatch_builtin(&mut eval, "mouse-pixel-position", vec![])
            .expect("mouse-pixel-position should resolve")
            .expect("mouse-pixel-position should evaluate"),
        Value::cons(frame, Value::cons(Value::fixnum(9), Value::fixnum(17)))
    );

    let set_char = dispatch_builtin(
        &mut eval,
        "set-mouse-position",
        vec![frame, Value::fixnum(3), Value::fixnum(4)],
    )
    .expect("set-mouse-position should resolve")
    .expect("set-mouse-position should evaluate");
    assert_eq!(set_char, Value::NIL);
    assert_eq!(
        dispatch_builtin(&mut eval, "mouse-position", vec![])
            .expect("mouse-position should resolve")
            .expect("mouse-position should evaluate"),
        Value::cons(frame, Value::cons(Value::fixnum(3), Value::fixnum(4)))
    );
    assert_eq!(
        dispatch_builtin(&mut eval, "mouse-pixel-position", vec![])
            .expect("mouse-pixel-position should resolve")
            .expect("mouse-pixel-position should evaluate"),
        Value::cons(frame, Value::cons(Value::fixnum(28), Value::fixnum(72)))
    );
}

#[test]
fn dispatch_builtin_pure_handles_internal_labeled_and_modified_tick_placeholders() {
    crate::test_utils::init_test_tracing();
    assert!(
        dispatch_builtin_pure(
            "internal--define-uninitialized-variable",
            vec![Value::symbol("neo-var"), Value::NIL],
        )
        .is_none(),
        "internal--define-uninitialized-variable should require evaluator context"
    );

    let narrow = dispatch_builtin_pure(
        "internal--labeled-narrow-to-region",
        vec![Value::fixnum(0), Value::fixnum(1), Value::symbol("tag")],
    )
    .expect("internal--labeled-narrow-to-region should resolve")
    .expect("internal--labeled-narrow-to-region should evaluate");
    assert_eq!(narrow, Value::NIL);

    let widen = dispatch_builtin_pure("internal--labeled-widen", vec![Value::symbol("tag")])
        .expect("internal--labeled-widen should resolve")
        .expect("internal--labeled-widen should evaluate");
    assert_eq!(widen, Value::NIL);

    let buckets = dispatch_builtin_pure("internal--obarray-buckets", vec![Value::vector(vec![])])
        .expect("internal--obarray-buckets should resolve")
        .expect("internal--obarray-buckets should evaluate");
    assert_eq!(buckets, Value::NIL);

    let tick = dispatch_builtin_pure(
        "internal--set-buffer-modified-tick",
        vec![Value::fixnum(0), Value::NIL],
    )
    .expect("internal--set-buffer-modified-tick should resolve")
    .expect("internal--set-buffer-modified-tick should evaluate");
    assert_eq!(tick, Value::NIL);
}

#[test]
fn internal_define_uninitialized_variable_marks_special_and_sets_doc() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = dispatch_builtin(
        &mut eval,
        "internal--define-uninitialized-variable",
        vec![Value::symbol("neo-var"), Value::string("Neo doc")],
    )
    .expect("internal--define-uninitialized-variable should resolve")
    .expect("internal--define-uninitialized-variable should evaluate");
    assert_eq!(result, Value::NIL);
    assert!(eval.obarray().is_special("neo-var"));
    assert_eq!(
        eval.obarray()
            .get_property("neo-var", "variable-documentation"),
        Some(Value::string("Neo doc"))
    );
}

#[test]
fn internal_labeled_narrow_to_region_clamps_within_current_restriction() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.create_buffer(" *vm-labeled-narrow*");
    eval.buffers.set_current(buf_id);
    let _ = eval.buffers.insert_into_buffer(buf_id, "abcdef");
    dispatch_builtin(
        &mut eval,
        "internal--labeled-narrow-to-region",
        vec![
            Value::fixnum(2),
            Value::fixnum(5),
            Value::symbol("outer-tag"),
        ],
    )
    .expect("outer internal--labeled-narrow-to-region should resolve")
    .expect("outer internal--labeled-narrow-to-region should evaluate");

    let narrowed = dispatch_builtin(
        &mut eval,
        "internal--labeled-narrow-to-region",
        vec![
            Value::fixnum(1),
            Value::fixnum(7),
            Value::symbol("inner-tag"),
        ],
    )
    .expect("internal--labeled-narrow-to-region should resolve")
    .expect("internal--labeled-narrow-to-region should evaluate");
    assert_eq!(narrowed, Value::NIL);

    let buf = eval.buffers.get(buf_id).expect("buffer should stay live");
    assert_eq!(buf.point_min_lisp_char_pos().as_i64(), 2);
    assert_eq!(buf.point_max_lisp_char_pos().as_i64(), 5);
}

#[test]
fn dispatch_builtin_pure_handles_window_resize_and_frame_switch_placeholders() {
    crate::test_utils::init_test_tracing();
    let save = dispatch_builtin_pure("handle-save-session", vec![Value::symbol("event")])
        .expect("handle-save-session should resolve")
        .expect("handle-save-session should evaluate");
    assert_eq!(save, Value::NIL);

    let frame = dispatch_builtin_pure("handle-switch-frame", vec![Value::make_frame(1)])
        .expect("handle-switch-frame should resolve")
        .expect("handle-switch-frame should evaluate");
    assert_eq!(frame, Value::NIL);

    let divider = dispatch_builtin_pure("window-bottom-divider-width", vec![])
        .expect("window-bottom-divider-width should resolve")
        .expect("window-bottom-divider-width should evaluate");
    assert_eq!(divider, Value::fixnum(0));

    let resize = dispatch_builtin_pure("window-resize-apply-total", vec![])
        .expect("window-resize-apply-total should resolve")
        .expect("window-resize-apply-total should evaluate");
    assert_eq!(resize, Value::T);
}

#[test]
fn dispatch_builtin_pure_handles_window_placeholder_accessors() {
    crate::test_utils::init_test_tracing();
    // Window accessor functions need eval state (FrameManager) and are
    // correctly deferred from dispatch_builtin_pure to the eval-backed
    // dispatch.  Verify they return None from pure dispatch.
    assert!(dispatch_builtin_pure("window-left-child", vec![Value::NIL]).is_none());
    assert!(dispatch_builtin_pure("window-next-sibling", vec![]).is_none());
    assert!(dispatch_builtin_pure("window-prev-sibling", vec![]).is_none());
    assert!(dispatch_builtin_pure("window-normal-size", vec![]).is_none());

    // These window functions ARE in pure dispatch (they don't need frame state):
    let root_window_id = 1_u64;
    // Skip the pure dispatch tests that were removed — the functions
    // are now tested through the eval-backed path in vm_test.rs.
    let _ = root_window_id; // suppress unused

    // Window functions that don't need frame state and ARE in pure dispatch:
    let line_height = dispatch_builtin_pure(
        "window-line-height",
        vec![Value::fixnum(0), Value::symbol("window")],
    )
    .expect("window-line-height should resolve")
    .expect("window-line-height should evaluate");
    assert_eq!(line_height, Value::NIL);

    let old_body = dispatch_builtin_pure("window-old-body-pixel-height", vec![])
        .expect("window-old-body-pixel-height should resolve")
        .expect("window-old-body-pixel-height should evaluate");
    assert_eq!(old_body, Value::fixnum(0));

    let tab = dispatch_builtin_pure("window-tab-line-height", vec![])
        .expect("window-tab-line-height should resolve")
        .expect("window-tab-line-height should evaluate");
    assert_eq!(tab, Value::fixnum(0));

    let err = dispatch_builtin_pure("window-right-divider-width", vec![Value::fixnum(1)])
        .expect("window-right-divider-width should resolve")
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

/// The pure-dispatch table has no Gpm entry points, and does have the image
/// and help placeholders.
///
/// Ledger 190 deleted `gpm-mouse-start` and `gpm-mouse-stop`: GNU registers
/// them only under `#ifdef HAVE_GPM` (`src/term.c:5282-5286`), this port links
/// no libgpm, and GNU's own `lisp/t-mouse.el:49` uses
/// `(fboundp 'gpm-mouse-start)` as the "was Emacs built with Gpm" test -- so
/// declaring the name replaced GNU's "Emacs must be built with Gpm to use this
/// mode" with the stub's "Gpm-mouse only works in the GNU/Linux console".  The
/// assertion that used to pin the stub's error now pins its absence.
#[test]
fn dispatch_builtin_pure_handles_gpm_help_and_init_image_placeholders() {
    crate::test_utils::init_test_tracing();
    assert!(
        dispatch_builtin_pure("gpm-mouse-start", vec![]).is_none(),
        "gpm-mouse-start must not resolve: this build has no Gpm"
    );
    assert!(
        dispatch_builtin_pure("gpm-mouse-stop", vec![]).is_none(),
        "gpm-mouse-stop must not resolve: this build has no Gpm"
    );

    let init = dispatch_builtin_pure("init-image-library", vec![Value::symbol("png")])
        .expect("init-image-library should resolve")
        .expect("init-image-library should evaluate");
    assert_eq!(init, Value::T);

    let unknown = dispatch_builtin_pure("init-image-library", vec![Value::symbol("neomacs")])
        .expect("init-image-library should resolve")
        .expect("init-image-library should evaluate");
    assert_eq!(unknown, Value::NIL);

    let bmp = dispatch_builtin_pure("init-image-library", vec![Value::symbol("bmp")])
        .expect("init-image-library should resolve")
        .expect("init-image-library should evaluate");
    assert_eq!(bmp, Value::NIL);
}

#[test]
fn dispatch_builtin_pure_handles_frame_placeholder_accessors() {
    crate::test_utils::init_test_tracing();
    let face_table = dispatch_builtin_pure("frame--face-hash-table", vec![])
        .expect("frame--face-hash-table should resolve")
        .expect("frame--face-hash-table should evaluate");
    if !face_table.is_hash_table() {
        panic!("expected hash table");
    };
    assert!(matches!(
        face_table.as_hash_table().unwrap().test.clone(),
        HashTableTest::Eq
    ));

    let was_invisible =
        dispatch_builtin_pure("frame--set-was-invisible", vec![Value::NIL, Value::T])
            .expect("frame--set-was-invisible should resolve")
            .expect("frame--set-was-invisible should evaluate");
    assert_eq!(was_invisible, Value::T);

    let changed = dispatch_builtin_pure("frame-or-buffer-changed-p", vec![])
        .expect("frame-or-buffer-changed-p should resolve")
        .expect("frame-or-buffer-changed-p should evaluate");
    assert_eq!(changed, Value::T);

    let changed_nil = dispatch_builtin_pure("frame-or-buffer-changed-p", vec![Value::NIL])
        .expect("frame-or-buffer-changed-p should resolve")
        .expect("frame-or-buffer-changed-p should evaluate");
    assert_eq!(changed_nil, Value::NIL);

    let scale = dispatch_builtin_pure("frame-scale-factor", vec![])
        .expect("frame-scale-factor should resolve")
        .expect("frame-scale-factor should evaluate");
    assert_eq!(scale, Value::make_float(1.0));

    let pointer = dispatch_builtin_pure("frame-pointer-visible-p", vec![])
        .expect("frame-pointer-visible-p should resolve")
        .expect("frame-pointer-visible-p should evaluate");
    assert_eq!(pointer, Value::T);

    let err = dispatch_builtin_pure("frame-or-buffer-changed-p", vec![Value::fixnum(1)])
        .expect("frame-or-buffer-changed-p should resolve")
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn dispatch_builtin_pure_handles_describe_and_delete_terminal_placeholders() {
    crate::test_utils::init_test_tracing();
    assert!(
        dispatch_builtin_pure(
            "describe-buffer-bindings",
            vec![Value::make_buffer(crate::buffer::BufferId(0))],
        )
        .is_none()
    );

    let described = dispatch_builtin_pure(
        "describe-vector",
        vec![Value::vector(vec![Value::fixnum(1)])],
    )
    .expect("describe-vector should resolve")
    .expect("describe-vector should evaluate");
    assert_eq!(described, Value::NIL);

    let delete_err = dispatch_builtin_pure("delete-terminal", vec![])
        .expect("delete-terminal should resolve")
        .unwrap_err();
    match delete_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }

    let deleted = dispatch_builtin_pure("delete-terminal", vec![Value::symbol("tty")])
        .expect("delete-terminal should resolve")
        .expect("delete-terminal should evaluate");
    assert_eq!(deleted, Value::NIL);
}

#[test]
fn dispatch_builtin_pure_handles_fringe_and_garbage_placeholders() {
    crate::test_utils::init_test_tracing();
    // `fringe-bitmaps-at-pos` answers from the window's redisplay output, so
    // it needs the evaluator's frame state and cannot be dispatched purely.
    assert!(dispatch_builtin_pure("fringe-bitmaps-at-pos", vec![Value::NIL, Value::NIL]).is_none());

    let gc = dispatch_builtin_pure("garbage-collect-maybe", vec![Value::fixnum(0)])
        .expect("garbage-collect-maybe should resolve")
        .expect("garbage-collect-maybe should evaluate");
    assert_eq!(gc, Value::NIL);

    let prop_err = dispatch_builtin_pure(
        "get-unicode-property-internal",
        vec![Value::NIL, Value::fixnum(0)],
    )
    .expect("get-unicode-property-internal should resolve")
    .unwrap_err();
    match prop_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn gap_position_and_size_use_current_buffer_storage() {
    crate::test_utils::init_test_tracing();
    assert!(dispatch_builtin_pure("gap-position", vec![]).is_none());
    assert!(dispatch_builtin_pure("gap-size", vec![]).is_none());

    for backend_kind in crate::buffer::BufferTextBackendKind::non_gap_implemented_variants() {
        let mut eval = super::super::eval::Context::new();
        let current = eval
            .buffers
            .current_buffer_id()
            .expect("scratch buffer should exist");
        let initial = Value::list(vec![
            builtin_gap_position(&mut eval, vec![]).expect("gap-position"),
            builtin_gap_size(&mut eval, vec![]).expect("gap-size"),
        ]);

        eval.buffers
            .insert_into_buffer(current, "abc")
            .expect("insert should succeed");
        let after_gap_insert = Value::list(vec![
            builtin_gap_position(&mut eval, vec![]).expect("gap-position after insert"),
            builtin_gap_size(&mut eval, vec![]).expect("gap-size after insert"),
        ]);

        builtin_neomacs_set_buffer_text_backend(
            &mut eval,
            vec![Value::symbol(backend_kind.symbol_name())],
        )
        .expect("non-gap backend should be implemented");
        eval.buffers
            .insert_into_buffer(current, "d")
            .expect("non-gap insert should succeed");
        let after_non_gap_insert = Value::list(vec![
            builtin_neomacs_buffer_text_backend(&mut eval, vec![]).expect("backend"),
            builtin_gap_position(&mut eval, vec![]).expect("gap-position after backend conversion"),
            builtin_gap_size(&mut eval, vec![]).expect("gap-size after backend conversion"),
        ]);

        let result = Value::list(vec![initial, after_gap_insert, after_non_gap_insert]);
        assert_eq!(
            result,
            Value::list(vec![
                Value::list(vec![Value::fixnum(1), Value::fixnum(20)]),
                Value::list(vec![Value::fixnum(4), Value::fixnum(17)]),
                Value::list(vec![
                    Value::symbol(backend_kind.symbol_name()),
                    Value::fixnum(5),
                    Value::fixnum(16),
                ]),
            ])
        );
    }
}

#[test]
fn dispatch_builtin_pure_handles_gnutls_query_and_error_placeholders() {
    crate::test_utils::init_test_tracing();
    let available = dispatch_builtin_pure("gnutls-available-p", vec![])
        .expect("gnutls-available-p should resolve")
        .expect("gnutls-available-p should evaluate");
    assert_eq!(
        available,
        Value::list(vec![
            Value::symbol("ciphers"),
            Value::symbol("macs"),
            Value::symbol("digests"),
            Value::symbol("gnutls3"),
            Value::symbol("gnutls"),
        ])
    );

    let ciphers = dispatch_builtin_pure("gnutls-ciphers", vec![])
        .expect("gnutls-ciphers should resolve")
        .expect("gnutls-ciphers should evaluate");
    assert!(
        !list_to_vec(&ciphers)
            .expect("gnutls-ciphers should return an alist")
            .is_empty()
    );

    let digests = dispatch_builtin_pure("gnutls-digests", vec![])
        .expect("gnutls-digests should resolve")
        .expect("gnutls-digests should evaluate");
    assert!(
        !list_to_vec(&digests)
            .expect("gnutls-digests should return an alist")
            .is_empty()
    );

    let macs = dispatch_builtin_pure("gnutls-macs", vec![])
        .expect("gnutls-macs should resolve")
        .expect("gnutls-macs should evaluate");
    assert!(
        !list_to_vec(&macs)
            .expect("gnutls-macs should return an alist")
            .is_empty()
    );

    let errorp = dispatch_builtin_pure("gnutls-errorp", vec![Value::fixnum(0)])
        .expect("gnutls-errorp should resolve")
        .expect("gnutls-errorp should evaluate");
    assert_eq!(errorp, Value::T);

    let success = dispatch_builtin_pure("gnutls-errorp", vec![Value::T])
        .expect("gnutls-errorp should resolve")
        .expect("gnutls-errorp should evaluate");
    assert_eq!(success, Value::NIL);

    let again = dispatch_builtin_pure("gnutls-errorp", vec![Value::symbol("gnutls-e-again")])
        .expect("gnutls-errorp should resolve")
        .expect("gnutls-errorp should evaluate");
    assert_eq!(again, Value::NIL);

    let success = dispatch_builtin_pure("gnutls-error-string", vec![Value::fixnum(0)])
        .expect("gnutls-error-string should resolve")
        .expect("gnutls-error-string should evaluate");
    assert_eq!(success, Value::string("Success."));

    let success_sentinel = dispatch_builtin_pure("gnutls-error-string", vec![Value::T])
        .expect("gnutls-error-string should resolve")
        .expect("gnutls-error-string should evaluate");
    assert_eq!(success_sentinel, Value::string("Not an error"));

    let again_string =
        dispatch_builtin_pure("gnutls-error-string", vec![Value::symbol("gnutls-e-again")])
            .expect("gnutls-error-string should resolve")
            .expect("gnutls-error-string should evaluate");
    assert_eq!(
        again_string,
        Value::string("Resource temporarily unavailable, try again.")
    );

    let interrupted_string = dispatch_builtin_pure(
        "gnutls-error-string",
        vec![Value::symbol("gnutls-e-interrupted")],
    )
    .expect("gnutls-error-string should resolve")
    .expect("gnutls-error-string should evaluate");
    assert_eq!(
        interrupted_string,
        Value::string("Function was interrupted.")
    );

    let fatal_err = dispatch_builtin_pure("gnutls-error-fatalp", vec![Value::NIL])
        .expect("gnutls-error-fatalp should resolve")
        .unwrap_err();
    match fatal_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn gnutls_error_helpers_honor_gnu_symbol_code_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval.eval_str(
        r#"(let ((custom 'custom-gnutls-error)
                 (bad-string 'custom-gnutls-bad-string)
                 (bad-float 'custom-gnutls-bad-float))
             (put custom 'gnutls-code -1)
             (put bad-string 'gnutls-code "x")
             (put bad-float 'gnutls-code 1.5)
             (list
              (get 'gnutls-e-again 'gnutls-code)
              (get 'gnutls-e-interrupted 'gnutls-code)
              (get 'gnutls-e-invalid-session 'gnutls-code)
              (get 'gnutls-e-not-ready-for-handshake 'gnutls-code)
              (gnutls-errorp custom)
              (gnutls-error-string custom)
              (gnutls-error-fatalp custom)
              (gnutls-error-string bad-string)
              (condition-case err (gnutls-error-fatalp bad-string) (error err))
              (gnutls-error-string bad-float)
              (condition-case err (gnutls-error-fatalp bad-float) (error err))))"#,
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (-28 -52 -10 -65500 t \"(unknown error code)\" t \"Symbol has no numeric gnutls-code property\" (error \"Symbol has no numeric gnutls-code property\") \"Not an error symbol or code\" (error \"Not an error symbol or code\"))"
    );
}

#[test]
fn dispatch_builtin_pure_handles_gnutls_runtime_placeholders() {
    crate::test_utils::init_test_tracing();
    let peer_warning =
        dispatch_builtin_pure("gnutls-peer-status-warning-describe", vec![Value::NIL])
            .expect("gnutls-peer-status-warning-describe should resolve")
            .expect("gnutls-peer-status-warning-describe should evaluate");
    assert_eq!(peer_warning, Value::NIL);

    let async_err = dispatch_builtin_pure(
        "gnutls-asynchronous-parameters",
        vec![Value::NIL, Value::NIL],
    )
    .expect("gnutls-asynchronous-parameters should resolve")
    .unwrap_err();
    match async_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let bye_err = dispatch_builtin_pure("gnutls-bye", vec![Value::NIL, Value::NIL])
        .expect("gnutls-bye should resolve")
        .unwrap_err();
    match bye_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let deinit_err = dispatch_builtin_pure("gnutls-deinit", vec![Value::fixnum(1)])
        .expect("gnutls-deinit should resolve")
        .unwrap_err();
    match deinit_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let initstage_err = dispatch_builtin_pure("gnutls-get-initstage", vec![Value::fixnum(1)])
        .expect("gnutls-get-initstage should resolve")
        .unwrap_err();
    match initstage_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let cert_err = dispatch_builtin_pure("gnutls-format-certificate", vec![Value::NIL])
        .expect("gnutls-format-certificate should resolve")
        .unwrap_err();
    match cert_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }

    let enc_err = dispatch_builtin_pure(
        "gnutls-symmetric-encrypt",
        vec![
            Value::symbol("AES-128-GCM"),
            Value::string("k"),
            Value::list(vec![Value::symbol("iv-auto"), Value::fixnum(3)]),
            Value::string("data"),
            Value::string("aad"),
        ],
    )
    .expect("gnutls-symmetric-encrypt should resolve")
    .unwrap_err();
    match enc_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }

    let dec_err = dispatch_builtin_pure(
        "gnutls-symmetric-decrypt",
        vec![
            Value::symbol("AES-128-GCM"),
            Value::string("k"),
            Value::string("iv"),
            Value::string("data"),
            Value::string("aad"),
        ],
    )
    .expect("gnutls-symmetric-decrypt should resolve")
    .unwrap_err();
    match dec_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn gnutls_digest_and_mac_crypto_match_gnu_shape() {
    crate::test_utils::init_test_tracing();

    let digests = dispatch_builtin_pure("gnutls-digests", vec![])
        .expect("gnutls-digests should resolve")
        .expect("gnutls-digests should evaluate");
    let digest_entries = list_to_vec(&digests).expect("gnutls-digests should return an alist");
    assert!(
        digest_entries.iter().any(|entry| {
            let Some(pair) = list_to_vec(entry) else {
                return false;
            };
            pair.first() == Some(&Value::symbol("SHA256"))
                && pair.contains(&Value::keyword(":digest-algorithm-id"))
                && pair.contains(&Value::keyword(":digest-algorithm-length"))
        }),
        "gnutls-digests should expose a GNU-shaped SHA256 entry: {digest_entries:?}"
    );

    let macs = dispatch_builtin_pure("gnutls-macs", vec![])
        .expect("gnutls-macs should resolve")
        .expect("gnutls-macs should evaluate");
    let mac_entries = list_to_vec(&macs).expect("gnutls-macs should return an alist");
    assert!(
        mac_entries.iter().any(|entry| {
            let Some(pair) = list_to_vec(entry) else {
                return false;
            };
            pair.first() == Some(&Value::symbol("SHA256"))
                && pair.contains(&Value::keyword(":mac-algorithm-id"))
                && pair.contains(&Value::keyword(":mac-algorithm-length"))
        }),
        "gnutls-macs should expose a GNU-shaped SHA256 entry: {mac_entries:?}"
    );

    let digest = dispatch_builtin_pure(
        "gnutls-hash-digest",
        vec![Value::symbol("SHA256"), Value::string("abc")],
    )
    .expect("gnutls-hash-digest should resolve")
    .expect("gnutls-hash-digest should evaluate");
    assert_eq!(
        digest
            .as_lisp_string()
            .expect("digest should be a string")
            .as_bytes(),
        &[
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ]
    );

    let mac = dispatch_builtin_pure(
        "gnutls-hash-mac",
        vec![
            Value::symbol("SHA256"),
            Value::string("key"),
            Value::string("abc"),
        ],
    )
    .expect("gnutls-hash-mac should resolve")
    .expect("gnutls-hash-mac should evaluate");
    assert_eq!(
        mac.as_lisp_string()
            .expect("mac should be a string")
            .as_bytes(),
        &[
            0x9c, 0x19, 0x6e, 0x32, 0xdc, 0x01, 0x75, 0xf8, 0x6f, 0x4b, 0x1c, 0xb8, 0x92, 0x89,
            0xd6, 0x61, 0x9d, 0xe6, 0xbe, 0xe6, 0x99, 0xe4, 0xc3, 0x78, 0xe6, 0x83, 0x09, 0xed,
            0x97, 0xa1, 0xa6, 0xab,
        ]
    );
}

#[test]
fn gnutls_aes_256_cbc_crypto_matches_gnu_auth_source_shape() {
    crate::test_utils::init_test_tracing();

    let ciphers = dispatch_builtin_pure("gnutls-ciphers", vec![])
        .expect("gnutls-ciphers should resolve")
        .expect("gnutls-ciphers should evaluate");
    let cipher_entries = list_to_vec(&ciphers).expect("gnutls-ciphers should return an alist");
    let aes_256_cbc = cipher_entries
        .iter()
        .find(|entry| {
            let Some(pair) = list_to_vec(entry) else {
                return false;
            };
            pair.first() == Some(&Value::symbol("AES-256-CBC"))
        })
        .expect("gnutls-ciphers should expose AES-256-CBC");
    let aes_256_cbc_items = list_to_vec(aes_256_cbc).expect("cipher entry should be a list");
    assert_eq!(
        aes_256_cbc_items.first(),
        Some(&Value::symbol("AES-256-CBC"))
    );
    assert!(aes_256_cbc_items.contains(&Value::keyword(":cipher-id")));
    assert!(aes_256_cbc_items.contains(&Value::fixnum(5)));
    assert!(aes_256_cbc_items.contains(&Value::keyword(":cipher-blocksize")));
    assert!(aes_256_cbc_items.contains(&Value::fixnum(16)));
    assert!(aes_256_cbc_items.contains(&Value::keyword(":cipher-keysize")));
    assert!(aes_256_cbc_items.contains(&Value::fixnum(32)));
    assert!(aes_256_cbc_items.contains(&Value::keyword(":cipher-ivsize")));

    let encrypted = dispatch_builtin_pure(
        "gnutls-symmetric-encrypt",
        vec![
            Value::symbol("AES-256-CBC"),
            Value::string("0123456789abcdef0123456789abcdef"),
            Value::string("0000000000000000"),
            Value::string("abcdefghijklmnop"),
        ],
    )
    .expect("gnutls-symmetric-encrypt should resolve")
    .expect("gnutls-symmetric-encrypt should evaluate");
    let encrypted_items =
        list_to_vec(&encrypted).expect("gnutls-symmetric-encrypt should return a list");
    assert_eq!(encrypted_items.len(), 2);
    assert_eq!(
        encrypted_items[0]
            .as_lisp_string()
            .expect("ciphertext should be a string")
            .as_bytes(),
        &[
            0xfc, 0xab, 0x6d, 0x29, 0x1c, 0x83, 0xf7, 0xb7, 0x62, 0x07, 0x60, 0x26, 0xe9, 0x71,
            0x30, 0x4e,
        ]
    );
    assert_eq!(
        encrypted_items[1]
            .as_lisp_string()
            .expect("actual IV should be a string")
            .as_bytes(),
        b"0000000000000000"
    );

    let decrypted = dispatch_builtin_pure(
        "gnutls-symmetric-decrypt",
        vec![
            Value::symbol("AES-256-CBC"),
            Value::string("0123456789abcdef0123456789abcdef"),
            encrypted_items[1],
            encrypted_items[0],
        ],
    )
    .expect("gnutls-symmetric-decrypt should resolve")
    .expect("gnutls-symmetric-decrypt should evaluate");
    let decrypted_items =
        list_to_vec(&decrypted).expect("gnutls-symmetric-decrypt should return a list");
    assert_eq!(decrypted_items.len(), 2);
    assert_eq!(
        decrypted_items[0]
            .as_lisp_string()
            .expect("plaintext should be a string")
            .as_bytes(),
        b"abcdefghijklmnop"
    );
    assert_eq!(
        decrypted_items[1]
            .as_lisp_string()
            .expect("actual IV should be a string")
            .as_bytes(),
        b"0000000000000000"
    );
}

#[test]
fn dispatch_builtin_pure_handles_face_attribute_vector() {
    crate::test_utils::init_test_tracing();
    let _eval = crate::emacs_core::Context::new();
    let face = dispatch_builtin_pure("face-attributes-as-vector", vec![Value::NIL])
        .expect("face-attributes-as-vector should resolve")
        .expect("face-attributes-as-vector should evaluate");
    if !face.is_vector() {
        panic!("expected vector");
    };
    assert_eq!(
        face.as_vector_data().unwrap().len(),
        FACE_ATTRIBUTES_VECTOR_LEN
    );
}

#[test]
fn dispatch_builtin_pure_handles_fontset_placeholders() {
    crate::test_utils::init_test_tracing();
    super::symbols::reset_symbols_thread_locals();
    let system = dispatch_builtin_pure("font-get-system-font", vec![])
        .expect("font-get-system-font should resolve")
        .expect("font-get-system-font should evaluate");
    assert_eq!(system, Value::NIL);

    let normal = dispatch_builtin_pure("font-get-system-normal-font", vec![])
        .expect("font-get-system-normal-font should resolve")
        .expect("font-get-system-normal-font should evaluate");
    assert_eq!(normal, Value::NIL);

    let fontset = dispatch_builtin_pure(
        "fontset-font",
        vec![Value::symbol("fontset-default"), Value::fixnum('a' as i64)],
    )
    .expect("fontset-font should resolve")
    .expect("fontset-font should evaluate");
    assert_eq!(fontset, Value::NIL);

    let info_err = dispatch_builtin_pure("fontset-info", vec![Value::symbol("fontset-default")])
        .expect("fontset-info should resolve")
        .unwrap_err();
    match info_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected signal, got {other:?}"),
    }

    let list = dispatch_builtin_pure("fontset-list", vec![])
        .expect("fontset-list should resolve")
        .expect("fontset-list should evaluate");
    assert_eq!(
        list,
        Value::list(vec![Value::string(
            "-*-*-*-*-*-*-*-*-*-*-*-*-fontset-default"
        )])
    );

    assert!(
        dispatch_builtin_pure(
            "new-fontset",
            vec![
                Value::string("-*-fixed-medium-r-normal-*-16-*-*-*-*-*-fontset-standard"),
                Value::list(vec![]),
            ],
        )
        .is_none()
    );

    let fontset_err = dispatch_builtin_pure(
        "fontset-font",
        vec![Value::symbol("fontset-default"), Value::NIL],
    )
    .expect("fontset-font should resolve")
    .unwrap_err();
    match fontset_err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn prin1_to_string_prints_canonical_threading_handles_as_opaque() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let thread = dispatch_builtin(&mut eval, "current-thread", vec![])
        .expect("current-thread should resolve")
        .expect("current-thread should evaluate");
    let thread_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![thread])
        .expect("prin1-to-string should resolve for thread")
        .expect("prin1-to-string should evaluate for thread");
    assert_eq!(thread_text, Value::string("#<thread 0>"));

    let mutex = dispatch_builtin(&mut eval, "make-mutex", vec![])
        .expect("make-mutex should resolve")
        .expect("make-mutex should evaluate");
    let mutex_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![mutex])
        .expect("prin1-to-string should resolve for mutex")
        .expect("prin1-to-string should evaluate for mutex");
    assert_eq!(mutex_text, Value::string("#<mutex 1>"));

    let condvar = dispatch_builtin(&mut eval, "make-condition-variable", vec![mutex])
        .expect("make-condition-variable should resolve")
        .expect("make-condition-variable should evaluate");
    let condvar_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![condvar])
        .expect("prin1-to-string should resolve for condvar")
        .expect("prin1-to-string should evaluate for condvar");
    assert_eq!(condvar_text, Value::string("#<condvar 1>"));
}

#[test]
fn prin1_to_string_keeps_forged_threading_handles_as_cons() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let forged_thread = Value::cons(Value::symbol("thread"), Value::fixnum(0));
    let thread_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![forged_thread])
        .expect("prin1-to-string should resolve for forged thread")
        .expect("prin1-to-string should evaluate for forged thread");
    assert_eq!(thread_text, Value::string("(thread . 0)"));

    let forged_mutex = Value::cons(Value::symbol("mutex"), Value::fixnum(1));
    let mutex_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![forged_mutex])
        .expect("prin1-to-string should resolve for forged mutex")
        .expect("prin1-to-string should evaluate for forged mutex");
    assert_eq!(mutex_text, Value::string("(mutex . 1)"));

    let forged_condvar = Value::cons(Value::symbol("condition-variable"), Value::fixnum(1));
    let condvar_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![forged_condvar])
        .expect("prin1-to-string should resolve for forged condvar")
        .expect("prin1-to-string should evaluate for forged condvar");
    assert_eq!(condvar_text, Value::string("(condition-variable . 1)"));
}

#[test]
fn prin1_to_string_supports_noescape_for_strings() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let value = Value::string("a\nb");

    let escaped = dispatch_builtin(&mut eval, "prin1-to-string", vec![value])
        .expect("prin1-to-string should resolve")
        .expect("prin1-to-string should evaluate");
    // GNU Emacs default: print-escape-newlines is nil, so \n passes
    // through literally in prin1-to-string.
    assert_eq!(escaped, Value::string("\"a\nb\""));

    let noescape = dispatch_builtin(&mut eval, "prin1-to-string", vec![value, Value::T])
        .expect("prin1-to-string should resolve with noescape")
        .expect("prin1-to-string should evaluate with noescape");
    assert_eq!(noescape, Value::string("a\nb"));
}

#[test]
fn prin1_to_string_uses_gnu_default_cycle_tail_indices() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (list
             (let ((x (list 'a 'b)))
               (setcdr (cdr x) x)
               (prin1-to-string x))
             (let ((x (list 'a 'b 'c)))
               (setcdr (cdr (cdr x)) x)
               (prin1-to-string (memq 'b x))))
            "#,
        )
        .expect("prin1-to-string should print circular lists like GNU");

    assert_eq!(
        result,
        Value::list(vec![
            Value::string("(a b a b . #2)"),
            Value::string("(b c a b c . #2)"),
        ])
    );
}

#[test]
fn prin1_runtime_path_detects_string_cycles_through_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (let* ((s "body")
                   (p (list 'section nil s)))
              (put-text-property 0 4 :parent p s)
              (erase-buffer)
              (let ((printed (prin1-to-string (list t s))))
                (prin1 (list t s) (current-buffer))
                (list printed (buffer-string))))
            "#,
        )
        .expect("prin1 should detect active string text-property cycles like GNU");

    let printed = crate::emacs_core::value::list_to_vec(&result)
        .expect("result should be a proper list")
        .into_iter()
        .map(|value| {
            value
                .as_runtime_string_owned()
                .expect("result item should be a string")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        printed,
        vec![
            r#"(t #("body" 0 4 (:parent (section nil #1))))"#.to_string(),
            r#"(t #("body" 0 4 (:parent (section nil #1))))"#.to_string(),
        ]
    );
}

/// Issue #131: printing a real Private-Use-Area glyph (a nerd-font icon) into a
/// buffer must store the glyph's real code point, never reinterpret it as an
/// eight-bit raw byte or a unibyte byte through the legacy "storage string"
/// decode. This was the byte-compile showstopper for the two in-Unicode
/// sentinel collision ranges: U+E080..E0FF (raw-byte sentinel) and
/// U+E380..E3FF (unibyte sentinel). E.g. byte-compiling a buffer containing
/// U+E0A0 must write the real char (\xee\x82\xa0), not byte 0xA0.
#[test]
fn prin1_to_buffer_preserves_private_use_glyphs_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (mapcar (lambda (cp)
                      (erase-buffer)
                      (prin1 (char-to-string cp) (current-buffer))
                      ;; The buffer now holds `"' GLYPH `"'; char-after 2 is
                      ;; the glyph itself.
                      (char-after 2))
                    '(#xe080 #xe0a0 #xe0ff #xe380 #xe3a0 #xe3ff))
            "#,
        )
        .expect("prin1 to a buffer should preserve real PUA glyphs");

    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(0xe080),
            Value::fixnum(0xe0a0),
            Value::fixnum(0xe0ff),
            Value::fixnum(0xe380),
            Value::fixnum(0xe3a0),
            Value::fixnum(0xe3ff),
        ])
    );
}

/// Issue #131: `write-char` of a real Private-Use-Area glyph into a buffer must
/// store the glyph's code point, not a stray raw byte. `write-char` builds the
/// character's canonical Emacs bytes (`CHAR_STRING`) directly, so eight-bit and
/// non-Unicode codes stay disjoint from real glyphs.
#[test]
fn write_char_to_buffer_preserves_private_use_glyphs_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (mapcar (lambda (cp)
                      (erase-buffer)
                      (write-char cp (current-buffer))
                      (char-after 1))
                    '(#xe080 #xe0a0 #xe0ff #xe380 #xe3a0 #xe3ff))
            "#,
        )
        .expect("write-char to a buffer should preserve real PUA glyphs");

    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(0xe080),
            Value::fixnum(0xe0a0),
            Value::fixnum(0xe0ff),
            Value::fixnum(0xe380),
            Value::fixnum(0xe3a0),
            Value::fixnum(0xe3ff),
        ])
    );
}

/// Issue #131: `format`/`format-message` must preserve real Private-Use-Area
/// glyphs through `%c`, `%s`, and literal text rather than decoding them as
/// raw-byte sentinels. Covers both residual sentinel ranges (U+E080..E0FF,
/// U+E380..E3FF) plus the B2 range (U+E300..E37F).
/// Issue #131: string-transforming builtins (`upcase`/`downcase`/`capitalize`/
/// `upcase-initials`/`propertize`) must preserve real Private-Use glyphs — they
/// have no case mapping, so each must round-trip the glyph unchanged rather than
/// decode it as a raw byte through the storage-string form.
#[test]
fn case_and_propertize_preserve_private_use_glyphs_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (mapcar (lambda (cp)
                      (let ((s (char-to-string cp)))
                        (and (= (aref (upcase s) 0) cp)
                             (= (aref (downcase s) 0) cp)
                             (= (aref (capitalize s) 0) cp)
                             (= (aref (upcase-initials s) 0) cp)
                             (= (aref (propertize s 'k 1) 0) cp))))
                    '(#xe080 #xe0a0 #xe0ff #xe380 #xe3a0 #xe3ff))
            "#,
        )
        .expect("case/propertize builtins should preserve real PUA glyphs");

    assert_eq!(result, Value::list(vec![Value::T; 6]));
}

/// Issue #131: `princ` of a real Private-Use glyph (string or char) to a buffer
/// must store the glyph, not a raw-byte sentinel. `princ` of pure-Unicode values
/// routes through the byte-faithful sink; eight-bit values keep the storage path.
#[test]
fn princ_to_buffer_preserves_private_use_glyphs_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (mapcar (lambda (cp)
                      (erase-buffer)
                      (princ (char-to-string cp) (current-buffer))
                      (char-after 1))
                    '(#xe080 #xe0a0 #xe0ff #xe380 #xe3a0 #xe3ff))
            "#,
        )
        .expect("princ to a buffer should preserve real PUA glyphs");

    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(0xe080),
            Value::fixnum(0xe0a0),
            Value::fixnum(0xe0ff),
            Value::fixnum(0xe380),
            Value::fixnum(0xe3a0),
            Value::fixnum(0xe3ff),
        ])
    );
}

#[test]
fn princ_to_function_preserves_private_use_glyph_codes_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // GNU `(let (out) (princ (char-to-string #xe0a0) (lambda (c) ...))
    //       (nreverse out))` => (57504): princ to a function emits the real
    // character code, and a Private-Use glyph must survive as itself rather
    // than collapsing to a raw-byte sentinel. (`push` is a subr.el macro that
    // is not bootstrapped in a bare test context, so build the list with the
    // primitive `setq`/`cons`.)
    let glyph = eval
        .eval_str(
            r#"
            (let ((out nil))
              (princ (char-to-string #xe0a0) (lambda (c) (setq out (cons c out))))
              (nreverse out))
            "#,
        )
        .expect("princ to a function should emit the real glyph code");
    assert_eq!(glyph, Value::list(vec![Value::fixnum(0xe0a0)]));

    // princ of a list recurses with princ semantics (no string quoting) and
    // emits each Emacs character code in turn: "(foo <glyph>)".
    let list = eval
        .eval_str(
            r#"
            (let ((out nil))
              (princ (list 'foo (char-to-string #xe0a0))
                     (lambda (c) (setq out (cons c out))))
              (nreverse out))
            "#,
        )
        .expect("princ of a list to a function should preserve glyph codes");
    assert_eq!(
        list,
        Value::list(
            "(foo "
                .chars()
                .map(|c| Value::fixnum(c as i64))
                .chain([Value::fixnum(0xe0a0), Value::fixnum(')' as i64)])
                .collect()
        )
    );
}

#[test]
fn format_preserves_private_use_glyphs_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (mapcar (lambda (cp)
                      (and (= (aref (format "%c" cp) 0) cp)
                           (= (aref (format "%s" (char-to-string cp)) 0) cp)
                           (= (aref (format "x%cy" cp) 1) cp)
                           (= (aref (format-message "%c" cp) 0) cp)))
                    '(#xe080 #xe0a0 #xe0ff #xe380 #xe3a0 #xe3ff #xe300 #xe322))
            "#,
        )
        .expect("format/format-message should preserve real PUA glyphs");

    assert_eq!(result, Value::list(vec![Value::T; 8]));
}

#[test]
fn format_promotes_raw_byte_to_eight_bit_in_multibyte_result_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // GNU: (format "%c%s" #xe0a0 (unibyte-string 255)) => length 2, multibyte,
    // aref 0 = 57504 (the Private-Use glyph), aref 1 = 4194303 (#x3FFFFF, the
    // eight-bit char). The raw unibyte byte is promoted to an eight-bit char
    // because the glyph forces a multibyte result — the build-the-canonical-
    // bytes-then-down-convert path fixes the old E3FF-sentinel divergence.
    let result = eval
        .eval_str(
            r#"
            (let ((s (format "%c%s" #xe0a0 (unibyte-string 255))))
              (list (length s) (aref s 0) (aref s 1) (multibyte-string-p s)))
            "#,
        )
        .expect("format should evaluate");
    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(2),
            Value::fixnum(0xe0a0),
            Value::fixnum(0x3FFFFF),
            Value::T,
        ])
    );
}

#[test]
fn format_preserves_unibyte_raw_byte_payload_and_width_issue_131() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // GNU: (format "%5s" (unibyte-string 200)) => a UNIBYTE string of 5 bytes,
    // four leading ASCII spaces then the raw byte 200 (one display column, not
    // four), and the result is NOT promoted to multibyte.
    let result = eval
        .eval_str(
            r#"
            (let ((s (format "%5s" (unibyte-string 200))))
              (list (length s) (multibyte-string-p s) (append s nil)))
            "#,
        )
        .expect("format should evaluate");
    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(5),
            Value::NIL,
            Value::list(vec![
                Value::fixnum(32),
                Value::fixnum(32),
                Value::fixnum(32),
                Value::fixnum(32),
                Value::fixnum(200),
            ]),
        ])
    );
}

#[test]
fn prin1_to_string_respects_print_overrides_for_control_characters() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"(prin1-to-string (string ?\t ?\n) nil
                               '(t (escape-newlines . t)
                                   (escape-control-characters . t)))"#,
        )
        .expect("prin1-to-string should accept GNU print overrides");

    assert_eq!(result, Value::string(r#""\11\n""#));
}

#[test]
fn prin1_to_string_honors_print_quoted_binding_and_override() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (list
             (let ((print-quoted t))
               (prin1-to-string '(quote foo)))
             (let ((print-quoted nil))
               (prin1-to-string '(quote foo)))
             (let ((print-quoted nil))
               (prin1-to-string '(function bar)))
             (let ((print-quoted nil))
               (prin1-to-string '(quote foo) nil '((quoted . t))))
             (let ((print-quoted t))
               (prin1-to-string '(quote foo) nil '((quoted . nil)))))
            "#,
        )
        .expect("prin1-to-string should honor GNU print-quoted semantics");

    assert_eq!(
        result,
        Value::list(vec![
            Value::string("'foo"),
            Value::string("(quote foo)"),
            Value::string("(function bar)"),
            Value::string("'foo"),
            Value::string("(quote foo)"),
        ])
    );
}

#[test]
fn prin1_to_string_honors_print_number_table_string_alias() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r##"
            (let* ((s (make-string 1 ?x))
                   (print-circle t)
                   (print-continuous-numbering t)
                   (print-number-table (make-hash-table :test 'eq)))
              (puthash s "#$" print-number-table)
              (prin1-to-string s))
            "##,
        )
        .expect("prin1-to-string should honor print-number-table string aliases");

    assert_eq!(result, Value::string("#$"));
}

#[test]
fn prin1_to_string_honors_print_number_table_string_alias_in_lists() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r##"
            (let* ((s (make-string 1 ?x))
                   (print-circle t)
                   (print-continuous-numbering t)
                   (print-number-table (make-hash-table :test 'eq)))
              (puthash s "#$" print-number-table)
              (prin1-to-string (list s s)))
            "##,
        )
        .expect("prin1-to-string should use aliases for nested repeated objects");

    assert_eq!(result, Value::string("(#$ #$)"));
}

#[test]
fn prin1_to_string_ignores_print_number_table_without_continuous_numbering() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r##"
            (let* ((s (make-string 1 ?x))
                   (print-circle t)
                   (print-continuous-numbering nil)
                   (print-number-table (make-hash-table :test 'eq)))
              (puthash s "#$" print-number-table)
              (prin1-to-string (list s s)))
            "##,
        )
        .expect("print-number-table should reset unless numbering is continuous");

    assert_eq!(result, Value::string(r#"(#1="x" #1#)"#));
}

#[test]
fn prin1_to_string_honors_print_number_table_override() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r##"
            (let* ((s (make-string 1 ?x))
                   (table (make-hash-table :test 'eq)))
              (puthash s "#$" table)
              (prin1-to-string
               s nil
               (list (cons 'circle t)
                     (cons 'continuous-numbering t)
                     (cons 'number-table table))))
            "##,
        )
        .expect("prin1-to-string overrides should include print-number-table");

    assert_eq!(result, Value::string("#$"));
}

#[test]
fn prin1_to_string_continuous_numbering_keeps_global_label_index() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (progn
              (let ((print-circle t)
                    (print-continuous-numbering nil))
                (let ((x (list 0)))
                  (prin1-to-string (list x x))))
              (let ((print-circle t)
                    (print-continuous-numbering t))
                (concat
                 (let ((print-number-table (make-hash-table :test 'eq)))
                   (let ((x (list 1)))
                     (prin1-to-string (list x x))))
                 "\n"
                 (let ((print-number-table (make-hash-table :test 'eq)))
                   (let ((x (list 2)))
                     (prin1-to-string (list x x)))))))
            "#,
        )
        .expect("continuous numbering should keep labels across print calls");

    assert_eq!(result, Value::string("(#1=(1) #1#)\n(#2=(2) #2#)"));
}

#[test]
fn prin1_to_string_updates_print_number_table_after_first_print() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (let* ((x (list 1))
                   (print-circle t)
                   (print-continuous-numbering t)
                   (print-number-table (make-hash-table :test 'eq)))
              (prin1-to-string (list x x))
              (gethash x print-number-table))
            "#,
        )
        .expect("print-number-table should retain GNU positive label entries");

    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn prin1_to_string_reuses_print_number_table_positive_label() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (let* ((x (list 1))
                   (print-circle t)
                   (print-continuous-numbering t)
                   (print-number-table (make-hash-table :test 'eq)))
              (concat (prin1-to-string (list x x))
                      "\n"
                      (prin1-to-string x)))
            "#,
        )
        .expect("positive print-number-table entries should print as references");

    assert_eq!(result, Value::string("(#1=(1) #1#)\n#1#"));
}

#[test]
fn prin1_to_string_continuous_gensym_gets_print_number_label() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (progn
              (let ((print-circle t)
                    (print-continuous-numbering nil))
                (let ((x (list 0)))
                  (prin1-to-string (list x x))))
              (let* ((s (make-symbol "g"))
                     (print-gensym t)
                     (print-circle t)
                     (print-continuous-numbering t)
                     (print-number-table (make-hash-table :test 'eq)))
                (prin1-to-string s)))
            "#,
        )
        .expect("GNU labels continuous print-gensym symbols even when seen once");

    assert_eq!(result, Value::string("#1=#:g"));
}

#[test]
fn prin1_to_string_preserves_gensym_lambda_parameter_identity_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let sym = crate::emacs_core::intern::intern_uninterned("stack");
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![sym]),
        body: vec![Value::from_sym_id(sym)],
        env: Some(Value::NIL),
        docstring: None,
        doc_form: None,
        interactive: None,
    });

    eval.set_variable("print-gensym", Value::T);
    eval.set_variable("print-circle", Value::T);
    eval.set_variable("print-continuous-numbering", Value::T);
    eval.set_variable("print-number-table", Value::hash_table(HashTableTest::Eq));

    let result = dispatch_builtin(&mut eval, "prin1-to-string", vec![lambda])
        .expect("prin1-to-string should resolve")
        .expect("prin1-to-string should evaluate");

    assert_eq!(result, Value::string("#[(#1=#:stack) (#1#) (t)]"));
}

#[test]
fn prin1_to_string_noncontinuous_print_resets_label_index_even_without_circle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (progn
              (let ((print-circle t)
                    (print-continuous-numbering nil))
                (let ((x (list 0)))
                  (prin1-to-string (list x x))))
              (let* ((x (list 1))
                     (print-circle t)
                     (print-continuous-numbering t)
                     (print-number-table (make-hash-table :test 'eq)))
                (prin1-to-string (list x x)))
              (let ((print-continuous-numbering nil))
                (prin1-to-string 'reset))
              (let* ((x (list 2))
                     (print-circle t)
                     (print-continuous-numbering t)
                     (print-number-table (make-hash-table :test 'eq)))
                (prin1-to-string (list x x))))
            "#,
        )
        .expect("GNU resets print_number_index on any noncontinuous print call");

    assert_eq!(result, Value::string("(#1=(2) #1#)"));
}

#[test]
fn prin1_to_string_empty_container_print_resets_label_index() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (progn
              (let ((print-circle t)
                    (print-continuous-numbering nil))
                (let ((x (list 0)))
                  (prin1-to-string (list x x))))
              (let* ((x (list 1))
                     (print-circle t)
                     (print-continuous-numbering t)
                     (print-number-table (make-hash-table :test 'eq)))
                (prin1-to-string (list x x)))
              (let ((print-continuous-numbering nil))
                (prin1-to-string []))
              (let* ((x (list 2))
                     (print-circle t)
                     (print-continuous-numbering t)
                     (print-number-table (make-hash-table :test 'eq)))
                (prin1-to-string (list x x))))
            "#,
        )
        .expect("GNU resets print_number_index at print entry, including empty containers");

    assert_eq!(result, Value::string("(#1=(2) #1#)"));
}

#[test]
fn prin1_loaddefs_docstring_layout_matches_gnu_print_form() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval
        .eval_str(
            r#"
            (progn
              (erase-buffer)
              (let ((def (list 'autoload
                               (list 'quote 'ebrowse-tags-find-declaration)
                               "ebrowse"
                               "Find declaration of member at point."
                               t))
                    (n 0))
                (insert "(")
                (while (< n 3)
                  (prin1 (car def) (current-buffer)
                         '(t (escape-newlines . t)
                             (escape-control-characters . t)))
                  (setq def (cdr def))
                  (insert " ")
                  (setq n (1+ n)))
                (let ((start (point)))
                  (prin1 (car def) (current-buffer) t)
                  (setq def (cdr def))
                  (save-excursion
                    (goto-char (1+ start))
                    (insert "\\
")))
                (while def
                  (insert " ")
                  (prin1 (car def) (current-buffer)
                         '(t (escape-newlines . t)
                             (escape-control-characters . t)))
                  (setq def (cdr def)))
                (insert ")")
                (buffer-string)))
            "#,
        )
        .expect("GNU loaddefs print-form pattern should evaluate");

    assert_eq!(
        result,
        Value::string(
            "(autoload 'ebrowse-tags-find-declaration \"ebrowse\" \"\\\nFind declaration of member at point.\" t)"
        )
    );
}

#[test]
fn prin1_to_string_respects_print_gensym_binding() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let sym = Value::symbol(intern_uninterned("vm-print-gensym"));

    let default_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![sym])
        .expect("prin1-to-string should resolve")
        .expect("prin1-to-string should evaluate");
    assert_eq!(default_text, Value::string("vm-print-gensym"));

    eval.set_variable("print-gensym", Value::T);
    let gensym_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![sym])
        .expect("prin1-to-string should resolve with print-gensym")
        .expect("prin1-to-string should evaluate with print-gensym");
    assert_eq!(gensym_text, Value::string("#:vm-print-gensym"));
}

#[test]
fn prin1_to_string_preserves_raw_unibyte_symbol_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let raw_name = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'a']);
    let sym = Value::symbol(crate::emacs_core::intern::intern_uninterned_lisp_string(
        &raw_name,
    ));

    let default_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![sym])
        .expect("prin1-to-string should resolve")
        .expect("prin1-to-string should evaluate");
    let default_ls = default_text
        .as_lisp_string()
        .expect("prin1-to-string should return a string");
    assert_eq!(default_ls.as_bytes(), &[0xC1, 0xBF, b'a']);
    assert!(default_ls.is_multibyte());

    eval.set_variable("print-gensym", Value::T);
    let gensym_text = dispatch_builtin(&mut eval, "prin1-to-string", vec![sym])
        .expect("prin1-to-string should resolve with print-gensym")
        .expect("prin1-to-string should evaluate with print-gensym");
    let gensym_ls = gensym_text
        .as_lisp_string()
        .expect("prin1-to-string should return a string");
    assert_eq!(gensym_ls.as_bytes(), &[b'#', b':', 0xC1, 0xBF, b'a']);
    assert!(gensym_ls.is_multibyte());
}

#[test]
fn prin1_to_string_ignores_extra_args_for_compat() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = dispatch_builtin(
        &mut eval,
        "prin1-to-string",
        vec![Value::fixnum(1), Value::NIL, Value::NIL],
    )
    .expect("prin1-to-string should resolve with extra args")
    .expect("prin1-to-string should evaluate with extra args");
    assert_eq!(result, Value::string("1"));
}

#[test]
fn format_prints_thread_handles_as_opaque_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let thread = dispatch_builtin(&mut eval, "current-thread", vec![])
        .expect("current-thread should resolve")
        .expect("current-thread should evaluate");

    let upper = dispatch_builtin(&mut eval, "format", vec![Value::string("%S"), thread])
        .expect("format should resolve for %S")
        .expect("format should evaluate for %S");
    assert!(
        upper
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<thread"))
    );

    let lower = dispatch_builtin(&mut eval, "format", vec![Value::string("%s"), thread])
        .expect("format should resolve for %s")
        .expect("format should evaluate for %s");
    assert!(
        lower
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<thread"))
    );
}

#[test]
fn message_prints_thread_handles_as_opaque_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let thread = dispatch_builtin(&mut eval, "current-thread", vec![])
        .expect("current-thread should resolve")
        .expect("current-thread should evaluate");
    let message = dispatch_builtin(&mut eval, "message", vec![Value::string("%S"), thread])
        .expect("message should resolve")
        .expect("message should evaluate");
    assert!(
        message
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<thread"))
    );
}

#[test]
fn format_and_message_render_terminal_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let terminals = dispatch_builtin(&mut eval, "terminal-list", vec![])
        .expect("terminal-list should resolve")
        .expect("terminal-list should evaluate");
    let terminal = list_to_vec(&terminals)
        .and_then(|items| items.into_iter().next())
        .expect("terminal-list should return at least one terminal");

    let mut assert_prefix = |builtin: &str, spec: &str| {
        let rendered = dispatch_builtin(&mut eval, builtin, vec![Value::string(spec), terminal])
            .expect("builtin should resolve")
            .expect("builtin should evaluate");
        assert!(
            rendered
                .as_utf8_str()
                .is_some_and(|s| s.starts_with("#<terminal")),
            "expected {builtin} {spec} output to start with #<terminal, got: {rendered:?}"
        );
    };

    assert_prefix("format", "%s");
    assert_prefix("message", "%s");
    assert_prefix("format", "%S");
    assert_prefix("message", "%S");
}

#[test]
fn format_and_message_render_mutex_condvar_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let mutex = dispatch_builtin(&mut eval, "make-mutex", vec![])
        .expect("make-mutex should resolve")
        .expect("make-mutex should evaluate");
    let condvar = dispatch_builtin(&mut eval, "make-condition-variable", vec![mutex])
        .expect("make-condition-variable should resolve")
        .expect("make-condition-variable should evaluate");

    let mut assert_prefix = |builtin: &str, spec: &str, value: Value, prefix: &str| {
        let rendered = dispatch_builtin(&mut eval, builtin, vec![Value::string(spec), value])
            .expect("builtin should resolve")
            .expect("builtin should evaluate");
        assert!(
            rendered
                .as_utf8_str()
                .is_some_and(|s| s.starts_with(prefix)),
            "expected {builtin} {spec} output to start with {prefix}, got: {rendered:?}"
        );
    };

    assert_prefix("format", "%s", mutex, "#<mutex");
    assert_prefix("message", "%s", mutex, "#<mutex");
    assert_prefix("format", "%S", mutex, "#<mutex");
    assert_prefix("message", "%S", mutex, "#<mutex");

    assert_prefix("format", "%s", condvar, "#<condvar");
    assert_prefix("message", "%s", condvar, "#<condvar");
    assert_prefix("format", "%S", condvar, "#<condvar");
    assert_prefix("message", "%S", condvar, "#<condvar");
}

#[test]
fn format_and_message_render_killed_buffer_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer = create_unique_test_buffer(&mut eval, "*format-killed-buffer*");
    let _ = dispatch_builtin(&mut eval, "kill-buffer", vec![buffer])
        .expect("kill-buffer should resolve")
        .expect("kill-buffer should evaluate");

    let formatted = dispatch_builtin(&mut eval, "format", vec![Value::string("%S"), buffer])
        .expect("format should resolve")
        .expect("format should evaluate");
    assert_eq!(formatted, Value::string("#<killed buffer>"));

    let message = dispatch_builtin(&mut eval, "message", vec![Value::string("%S"), buffer])
        .expect("message should resolve")
        .expect("message should evaluate");
    assert_eq!(message, Value::string("#<killed buffer>"));
}

#[test]
fn format_and_message_render_live_buffer_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer = create_unique_test_buffer(&mut eval, "*format-live-buffer*");

    let formatted = dispatch_builtin(&mut eval, "format", vec![Value::string("%S"), buffer])
        .expect("format should resolve")
        .expect("format should evaluate");
    assert!(
        formatted
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<buffer *format-live-buffer")),
        "expected live buffer name in format output: {formatted:?}"
    );

    let message = dispatch_builtin(&mut eval, "message", vec![Value::string("%S"), buffer])
        .expect("message should resolve")
        .expect("message should evaluate");
    assert!(
        message
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<buffer *format-live-buffer")),
        "expected live buffer name in message output: {message:?}"
    );
}

#[test]
fn format_and_message_percent_s_render_live_buffer_names_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let expected = "*format-live-s-buffer*";
    let buffer = create_unique_test_buffer(&mut eval, expected);

    let formatted = dispatch_builtin(&mut eval, "format", vec![Value::string("%s"), buffer])
        .expect("format should resolve")
        .expect("format should evaluate");
    assert_eq!(formatted, Value::string(expected));

    let message = dispatch_builtin(&mut eval, "message", vec![Value::string("%s"), buffer])
        .expect("message should resolve")
        .expect("message should evaluate");
    assert_eq!(message, Value::string(expected));
}

#[test]
fn format_and_message_percent_s_render_killed_buffer_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer = create_unique_test_buffer(&mut eval, "*format-killed-s-buffer*");
    let _ = dispatch_builtin(&mut eval, "kill-buffer", vec![buffer])
        .expect("kill-buffer should resolve")
        .expect("kill-buffer should evaluate");

    let formatted = dispatch_builtin(&mut eval, "format", vec![Value::string("%s"), buffer])
        .expect("format should resolve")
        .expect("format should evaluate");
    assert_eq!(formatted, Value::string("#<killed buffer>"));

    let message = dispatch_builtin(&mut eval, "message", vec![Value::string("%s"), buffer])
        .expect("message should resolve")
        .expect("message should evaluate");
    assert_eq!(message, Value::string("#<killed buffer>"));
}

#[test]
fn format_and_message_render_frame_window_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let frame = dispatch_builtin(&mut eval, "selected-frame", vec![])
        .expect("selected-frame should resolve")
        .expect("selected-frame should evaluate");
    let window = dispatch_builtin(&mut eval, "selected-window", vec![])
        .expect("selected-window should resolve")
        .expect("selected-window should evaluate");

    {
        let mut assert_prefix = |builtin: &str, spec: &str, value: Value, prefix: &str| {
            let rendered = dispatch_builtin(&mut eval, builtin, vec![Value::string(spec), value])
                .expect("builtin should resolve")
                .expect("builtin should evaluate");
            assert!(
                rendered
                    .as_utf8_str()
                    .is_some_and(|s| s.starts_with(prefix)),
                "expected {builtin} {spec} output to start with {prefix}, got: {rendered:?}"
            );
        };

        assert_prefix("format", "%S", frame, "#<frame");
        assert_prefix("message", "%S", frame, "#<frame");
        assert_prefix("format", "%s", frame, "#<frame");
        assert_prefix("message", "%s", frame, "#<frame");
    }

    {
        let mut assert_contains = |builtin: &str, spec: &str, value: Value, snippet: &str| {
            let rendered = dispatch_builtin(&mut eval, builtin, vec![Value::string(spec), value])
                .expect("builtin should resolve")
                .expect("builtin should evaluate");
            assert!(
                rendered.as_utf8_str().is_some_and(|s| s.contains(snippet)),
                "expected {builtin} {spec} output to contain {snippet}, got: {rendered:?}"
            );
        };

        assert_contains("format", "%S", window, "on *scratch*>");
        assert_contains("message", "%S", window, "on *scratch*>");
        assert_contains("format", "%s", window, "on *scratch*>");
        assert_contains("message", "%s", window, "on *scratch*>");
    }
}

#[test]
fn format_message_renders_opaque_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let thread = dispatch_builtin(&mut eval, "current-thread", vec![])
        .expect("current-thread should resolve")
        .expect("current-thread should evaluate");
    let terminals = dispatch_builtin(&mut eval, "terminal-list", vec![])
        .expect("terminal-list should resolve")
        .expect("terminal-list should evaluate");
    let terminal = list_to_vec(&terminals)
        .and_then(|items| items.into_iter().next())
        .expect("terminal-list should return at least one terminal");
    let frame = dispatch_builtin(&mut eval, "selected-frame", vec![])
        .expect("selected-frame should resolve")
        .expect("selected-frame should evaluate");
    let window = dispatch_builtin(&mut eval, "selected-window", vec![])
        .expect("selected-window should resolve")
        .expect("selected-window should evaluate");

    let mut assert_prefix = |spec: &str, value: Value, prefix: &str| {
        let rendered = dispatch_builtin(
            &mut eval,
            "format-message",
            vec![Value::string(spec), value],
        )
        .expect("format-message should resolve")
        .expect("format-message should evaluate");
        assert!(
            rendered
                .as_utf8_str()
                .is_some_and(|s| s.starts_with(prefix)),
            "expected format-message {spec} output to start with {prefix}, got: {rendered:?}"
        );
    };

    assert_prefix("%S", thread, "#<thread");
    assert_prefix("%s", thread, "#<thread");
    assert_prefix("%S", terminal, "#<terminal");
    assert_prefix("%S", frame, "#<frame");
    assert_prefix("%S", window, "#<window");
    assert!(
        dispatch_builtin(
            &mut eval,
            "format-message",
            vec![Value::string("%S"), window]
        )
        .expect("format-message should resolve")
        .expect("format-message should evaluate")
        .as_utf8_str()
        .is_some_and(|s| s.contains("on *scratch*>")),
        "expected format-message window output to include buffer context"
    );

    let live_name = "*format-message-live-buffer*";
    let live_buffer = create_unique_test_buffer(&mut eval, live_name);
    let live_upper = dispatch_builtin(
        &mut eval,
        "format-message",
        vec![Value::string("%S"), live_buffer],
    )
    .expect("format-message should resolve")
    .expect("format-message should evaluate");
    assert!(
        live_upper
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<buffer *format-message-live-buffer")),
        "expected live buffer in format-message %S output: {live_upper:?}"
    );
    let live_lower = dispatch_builtin(
        &mut eval,
        "format-message",
        vec![Value::string("%s"), live_buffer],
    )
    .expect("format-message should resolve")
    .expect("format-message should evaluate");
    assert_eq!(live_lower, Value::string(live_name));
    let _ = dispatch_builtin(&mut eval, "kill-buffer", vec![live_buffer])
        .expect("kill-buffer should resolve")
        .expect("kill-buffer should evaluate");

    let killed_buffer = create_unique_test_buffer(&mut eval, "*format-message-killed-buffer*");
    let _ = dispatch_builtin(&mut eval, "kill-buffer", vec![killed_buffer])
        .expect("kill-buffer should resolve")
        .expect("kill-buffer should evaluate");
    let killed_upper = dispatch_builtin(
        &mut eval,
        "format-message",
        vec![Value::string("%S"), killed_buffer],
    )
    .expect("format-message should resolve")
    .expect("format-message should evaluate");
    assert_eq!(killed_upper, Value::string("#<killed buffer>"));
    let killed_lower = dispatch_builtin(
        &mut eval,
        "format-message",
        vec![Value::string("%s"), killed_buffer],
    )
    .expect("format-message should resolve")
    .expect("format-message should evaluate");
    assert_eq!(killed_lower, Value::string("#<killed buffer>"));
}

#[test]
fn error_message_string_preserves_percent_s_handle_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let render_error_message = |eval: &mut crate::emacs_core::eval::Context,
                                spec: &str,
                                value: Value|
     -> String {
        // GNU `error` is Elisp (subr.el): (signal 'error (list (format-message FMT ARGS)))
        // Construct the error data directly instead of calling dispatch_builtin.
        let formatted = dispatch_builtin(eval, "format-message", vec![Value::string(spec), value])
            .expect("format-message should resolve")
            .expect("format-message should evaluate");
        let signaled = crate::emacs_core::error::signal("error", vec![formatted]);
        let (symbol, data) = match signaled {
            Flow::Signal(sig) => (sig.symbol, sig.data),
            other => panic!("expected signal flow, got: {other:?}"),
        };
        let mut err_data = Vec::with_capacity(data.len() + 1);
        err_data.push(Value::symbol(symbol));
        err_data.extend(data);
        let rendered = dispatch_builtin(eval, "error-message-string", vec![Value::list(err_data)])
            .expect("error-message-string should resolve")
            .expect("error-message-string should evaluate");
        rendered
            .as_utf8_str()
            .expect("error-message-string should return a string")
            .to_string()
    };

    let live_name = "*ems-live-lower*";
    let live_buffer = create_unique_test_buffer(&mut eval, live_name);
    assert_eq!(
        render_error_message(&mut eval, "%s", live_buffer),
        live_name
    );
    let _ = dispatch_builtin(&mut eval, "kill-buffer", vec![live_buffer])
        .expect("kill-buffer should resolve")
        .expect("kill-buffer should evaluate");
    assert_eq!(
        render_error_message(&mut eval, "%s", live_buffer),
        "#<killed buffer>".to_string()
    );

    let thread = dispatch_builtin(&mut eval, "current-thread", vec![])
        .expect("current-thread should resolve")
        .expect("current-thread should evaluate");
    assert!(render_error_message(&mut eval, "%s", thread).starts_with("#<thread"));

    let mutex = dispatch_builtin(&mut eval, "make-mutex", vec![])
        .expect("make-mutex should resolve")
        .expect("make-mutex should evaluate");
    assert!(render_error_message(&mut eval, "%s", mutex).starts_with("#<mutex"));
    let condvar = dispatch_builtin(&mut eval, "make-condition-variable", vec![mutex])
        .expect("make-condition-variable should resolve")
        .expect("make-condition-variable should evaluate");
    assert!(render_error_message(&mut eval, "%s", condvar).starts_with("#<condvar"));
}

#[test]
fn message_box_wrappers_render_opaque_handles_in_eval_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    for (builtin, symbol) in [
        ("message-box", "message-box"),
        ("message-or-box", "message-or-box"),
    ] {
        let err = dispatch_builtin(&mut eval, builtin, vec![])
            .expect("wrapper should resolve")
            .expect_err("wrapper should signal on missing format argument");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
                assert_eq!(sig.data, vec![Value::symbol(symbol), Value::fixnum(0)]);
            }
            other => panic!("expected signal, got: {other:?}"),
        }
    }

    let message_box_nil = dispatch_builtin(&mut eval, "message-box", vec![Value::NIL])
        .expect("message-box should resolve")
        .expect("message-box should evaluate");
    assert!(message_box_nil.is_nil());
    let message_or_box_nil = dispatch_builtin(&mut eval, "message-or-box", vec![Value::NIL])
        .expect("message-or-box should resolve")
        .expect("message-or-box should evaluate");
    assert!(message_or_box_nil.is_nil());

    for builtin in ["message-box", "message-or-box"] {
        let wrong_type = dispatch_builtin(&mut eval, builtin, vec![Value::fixnum(1)])
            .expect("wrapper should resolve")
            .expect_err("wrapper should signal for non-string format");
        match wrong_type {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
            }
            other => panic!("expected signal, got: {other:?}"),
        }

        let missing = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%s %s"), Value::fixnum(1)],
        )
        .expect("wrapper should resolve")
        .expect_err("wrapper should signal when format args are missing");
        match missing {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Not enough arguments for format string")]
                );
            }
            other => panic!("expected signal, got: {other:?}"),
        }

        let negative_char = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%c"), Value::fixnum(-1)],
        )
        .expect("wrapper should resolve")
        .expect_err("wrapper should reject negative character code");
        match negative_char {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("characterp"), Value::fixnum(-1)]
                );
            }
            other => panic!("expected signal, got: {other:?}"),
        }

        let overflow_char = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%c"), Value::fixnum(0x40_0000)],
        )
        .expect("wrapper should resolve")
        .expect_err("wrapper should reject out-of-range character code");
        match overflow_char {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)]
                );
            }
            other => panic!("expected signal, got: {other:?}"),
        }
    }

    let high_chars = [
        ("message-box", 0x11_0000_i64),
        ("message-or-box", 0x20_0000_i64),
    ];
    for (builtin, value) in high_chars {
        let rendered = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%c"), Value::fixnum(value)],
        )
        .expect("wrapper should resolve")
        .expect("wrapper should evaluate");
        // Non-Unicode chars are rendered with lossy UTF-8; just verify it returns a string
        assert!(rendered.is_string());
    }

    let _ = dispatch_builtin(
        &mut eval,
        "message-box",
        vec![Value::string("mbox-current")],
    )
    .expect("message-box should resolve")
    .expect("message-box should evaluate");
    let current_after_box = dispatch_builtin(&mut eval, "current-message", vec![])
        .expect("current-message should resolve")
        .expect("current-message should evaluate");
    assert!(current_after_box.is_nil());
    let _ = dispatch_builtin(
        &mut eval,
        "message-or-box",
        vec![Value::string("morbox-current")],
    )
    .expect("message-or-box should resolve")
    .expect("message-or-box should evaluate");
    let current_after_or_box = dispatch_builtin(&mut eval, "current-message", vec![])
        .expect("current-message should resolve")
        .expect("current-message should evaluate");
    assert!(current_after_or_box.is_nil());

    let thread = dispatch_builtin(&mut eval, "current-thread", vec![])
        .expect("current-thread should resolve")
        .expect("current-thread should evaluate");
    let terminals = dispatch_builtin(&mut eval, "terminal-list", vec![])
        .expect("terminal-list should resolve")
        .expect("terminal-list should evaluate");
    let terminal = list_to_vec(&terminals)
        .and_then(|items| items.into_iter().next())
        .expect("terminal-list should return at least one terminal");
    let frame = dispatch_builtin(&mut eval, "selected-frame", vec![])
        .expect("selected-frame should resolve")
        .expect("selected-frame should evaluate");
    let window = dispatch_builtin(&mut eval, "selected-window", vec![])
        .expect("selected-window should resolve")
        .expect("selected-window should evaluate");
    let mutex = dispatch_builtin(&mut eval, "make-mutex", vec![])
        .expect("make-mutex should resolve")
        .expect("make-mutex should evaluate");
    let condvar = dispatch_builtin(&mut eval, "make-condition-variable", vec![mutex])
        .expect("make-condition-variable should resolve")
        .expect("make-condition-variable should evaluate");

    let mut assert_prefix = |builtin: &str, spec: &str, value: Value, prefix: &str| {
        let rendered = dispatch_builtin(&mut eval, builtin, vec![Value::string(spec), value])
            .expect("builtin should resolve")
            .expect("builtin should evaluate");
        assert!(
            rendered
                .as_utf8_str()
                .is_some_and(|s| s.starts_with(prefix)),
            "expected {builtin} {spec} output to start with {prefix}, got: {rendered:?}"
        );
    };

    for builtin in ["message-box", "message-or-box"] {
        assert_prefix(builtin, "%S", thread, "#<thread");
        assert_prefix(builtin, "%s", thread, "#<thread");
        assert_prefix(builtin, "%S", terminal, "#<terminal");
        assert_prefix(builtin, "%s", terminal, "#<terminal");
        assert_prefix(builtin, "%S", mutex, "#<mutex");
        assert_prefix(builtin, "%s", mutex, "#<mutex");
        assert_prefix(builtin, "%S", condvar, "#<condvar");
        assert_prefix(builtin, "%s", condvar, "#<condvar");
        assert_prefix(builtin, "%S", frame, "#<frame");
        assert_prefix(builtin, "%s", frame, "#<frame");
        assert_prefix(builtin, "%S", window, "#<window");
        assert_prefix(builtin, "%s", window, "#<window");
    }

    let live_name = "*message-box-live-buffer*";
    let live_buffer = create_unique_test_buffer(&mut eval, live_name);
    let live_upper = dispatch_builtin(
        &mut eval,
        "message-box",
        vec![Value::string("%S"), live_buffer],
    )
    .expect("message-box should resolve")
    .expect("message-box should evaluate");
    assert!(
        live_upper
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<buffer *message-box-live-buffer")),
        "expected live buffer in message-box %S output: {live_upper:?}"
    );
    let live_box_lower = dispatch_builtin(
        &mut eval,
        "message-box",
        vec![Value::string("%s"), live_buffer],
    )
    .expect("message-box should resolve")
    .expect("message-box should evaluate");
    assert_eq!(live_box_lower, Value::string(live_name));
    let live_or_upper = dispatch_builtin(
        &mut eval,
        "message-or-box",
        vec![Value::string("%S"), live_buffer],
    )
    .expect("message-or-box should resolve")
    .expect("message-or-box should evaluate");
    assert!(
        live_or_upper
            .as_utf8_str()
            .is_some_and(|s| s.starts_with("#<buffer *message-box-live-buffer")),
        "expected live buffer in message-or-box %S output: {live_or_upper:?}"
    );
    let live_lower = dispatch_builtin(
        &mut eval,
        "message-or-box",
        vec![Value::string("%s"), live_buffer],
    )
    .expect("message-or-box should resolve")
    .expect("message-or-box should evaluate");
    assert_eq!(live_lower, Value::string(live_name));
    let _ = dispatch_builtin(&mut eval, "kill-buffer", vec![live_buffer])
        .expect("kill-buffer should resolve")
        .expect("kill-buffer should evaluate");

    let killed_buffer = create_unique_test_buffer(&mut eval, "*message-box-killed-buffer*");
    let _ = dispatch_builtin(&mut eval, "kill-buffer", vec![killed_buffer])
        .expect("kill-buffer should resolve")
        .expect("kill-buffer should evaluate");
    let killed_upper = dispatch_builtin(
        &mut eval,
        "message-box",
        vec![Value::string("%S"), killed_buffer],
    )
    .expect("message-box should resolve")
    .expect("message-box should evaluate");
    assert_eq!(killed_upper, Value::string("#<killed buffer>"));
    let killed_box_lower = dispatch_builtin(
        &mut eval,
        "message-box",
        vec![Value::string("%s"), killed_buffer],
    )
    .expect("message-box should resolve")
    .expect("message-box should evaluate");
    assert_eq!(killed_box_lower, Value::string("#<killed buffer>"));
    let killed_or_upper = dispatch_builtin(
        &mut eval,
        "message-or-box",
        vec![Value::string("%S"), killed_buffer],
    )
    .expect("message-or-box should resolve")
    .expect("message-or-box should evaluate");
    assert_eq!(killed_or_upper, Value::string("#<killed buffer>"));
    let killed_lower = dispatch_builtin(
        &mut eval,
        "message-or-box",
        vec![Value::string("%s"), killed_buffer],
    )
    .expect("message-or-box should resolve")
    .expect("message-or-box should evaluate");
    assert_eq!(killed_lower, Value::string("#<killed buffer>"));
}

#[test]
fn message_nil_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // GNU populates the echo area (read back by `current-message`) only on the
    // interactive display path (xdisp.c `message3_frame_nolog`). The default
    // test Context is noninteractive, so opt into the interactive path here.
    eval.set_variable("noninteractive", Value::NIL);

    let eval_result =
        builtin_message(&mut eval, vec![Value::NIL]).expect("message eval should accept nil");
    assert!(eval_result.is_nil());

    let displayed = builtin_message(&mut eval, vec![Value::string("hello echo")])
        .expect("message eval should store echo text");
    assert_eq!(displayed, Value::string("hello echo"));
    let current = builtin_current_message(&mut eval, vec![])
        .expect("current-message should read stored echo text");
    assert_eq!(current, Value::string("hello echo"));

    let cleared = builtin_message(&mut eval, vec![Value::NIL]).expect("message eval should clear");
    assert!(cleared.is_nil());
    let current_after_clear =
        builtin_current_message(&mut eval, vec![]).expect("current-message should clear");
    assert!(current_after_clear.is_nil());
}

#[test]
fn message_logs_to_visible_messages_buffer_name_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        eval.eval_str("(list (boundp 'messages-buffer-name) messages-buffer-name)")
            .expect("messages-buffer-name should be bound"),
        Value::list(vec![Value::T, Value::string("*Messages*")])
    );

    builtin_message(&mut eval, vec![Value::string("hello echo")])
        .expect("message eval should log to *Messages*");

    let messages_id = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .expect("*Messages* buffer");
    let messages = eval.buffers.get(messages_id).expect("*Messages* live");
    assert_eq!(messages.buffer_string(), "hello echo\n");
    assert!(
        eval.buffers.find_buffer_by_name(" *Messages*").is_none(),
        "message should not create hidden legacy messages buffer"
    );
}

#[test]
fn native_add_to_log_coalesces_without_changing_the_echo_area() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    eval.add_to_log("Invalid face reference: missing-face");
    eval.add_to_log("Invalid face reference: missing-face");

    let messages_id = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .expect("native diagnostic should create *Messages*");
    let messages = eval.buffers.get(messages_id).expect("*Messages* live");
    assert_eq!(
        messages.buffer_string(),
        "Invalid face reference: missing-face [2 times]\n"
    );
    assert!(
        builtin_current_message(&mut eval, vec![])
            .expect("current-message should remain readable")
            .is_nil(),
        "GNU add_to_log records a diagnostic without displaying it in the echo area"
    );
}

#[test]
fn message_progress_completion_replaces_previous_log_entry_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_message(&mut eval, vec![Value::string("Indexing project...")])
        .expect("initial progress message should be logged");
    builtin_message(&mut eval, vec![Value::string("Indexing project...done")])
        .expect("completed progress message should be logged");

    let messages_id = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .expect("*Messages* buffer");
    let messages = eval.buffers.get(messages_id).expect("*Messages* live");
    assert_eq!(messages.buffer_string(), "Indexing project...done\n");
}

#[test]
fn message_progress_update_scans_from_previous_line_start_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let long_progress = format!("Indexing project...{}", "working".repeat(32));

    builtin_message(&mut eval, vec![Value::string(long_progress)])
        .expect("long progress message should be logged");
    builtin_message(&mut eval, vec![Value::string("Indexing project...done")])
        .expect("progress update should be logged");

    let messages_id = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .expect("*Messages* buffer");
    let messages = eval.buffers.get(messages_id).expect("*Messages* live");
    assert_eq!(messages.buffer_string(), "Indexing project...done\n");
}

#[test]
fn message_log_appends_after_full_messages_buffer_when_narrowed() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let messages_id = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .unwrap_or_else(|| eval.buffers.create_buffer("*Messages*"));
    {
        let messages = eval
            .buffers
            .get_mut(messages_id)
            .expect("*Messages* buffer");
        messages.insert("keep");
        messages.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(0, 2));
        messages.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(2));
    }

    builtin_message(&mut eval, vec![Value::string("hello")])
        .expect("message should append to *Messages*");

    let messages = eval.buffers.get(messages_id).expect("*Messages* buffer");
    assert_eq!(messages.buffer_string(), "ke");
    assert_eq!(messages.point_emacs_byte_pos().get(), 2);
    assert_eq!(
        messages.buffer_substring_range(messages.full_emacs_byte_range()),
        "keephello\n"
    );
}

#[test]
fn message_respects_rebound_messages_buffer_name_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    eval.eval_str(r#"(let ((messages-buffer-name "*Alt Messages*")) (message "redirected"))"#)
        .expect("message should accept rebound messages-buffer-name");

    assert!(
        eval.buffers.find_buffer_by_name("*Messages*").is_none(),
        "rebound messages-buffer-name should avoid the default buffer"
    );
    let alt_id = eval
        .buffers
        .find_buffer_by_name("*Alt Messages*")
        .expect("alternate messages buffer");
    let alt = eval
        .buffers
        .get(alt_id)
        .expect("alternate messages buffer live");
    assert_eq!(alt.buffer_string(), "redirected\n");
}

#[test]
fn interactive_message_materializes_echo_area_buffers_after_messages_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    builtin_message(&mut eval, vec![Value::string("hello echo")])
        .expect("interactive message should install echo area buffers");

    let names: Vec<String> = eval
        .buffers
        .buffer_list()
        .into_iter()
        .filter_map(|id| {
            eval.buffers
                .get(id)
                .map(|buf| buf.name_runtime_string_owned())
        })
        .collect();
    assert_eq!(
        names,
        vec![
            "*scratch*",
            "*Messages*",
            " *Echo Area 0*",
            " *Echo Area 1*"
        ]
    );
    for name in [" *Echo Area 0*", " *Echo Area 1*"] {
        let id = eval
            .buffers
            .find_buffer_by_name(name)
            .expect("echo area buffer");
        let buffer = eval.buffers.get(id).expect("echo area buffer live");
        assert_eq!(buffer.get_buffer_local("truncate-lines"), Some(Value::NIL));
        assert_eq!(buffer.get_buffer_local("buffer-undo-list"), Some(Value::T));
    }
}

#[test]
fn echo_area_follows_its_buffer_across_a_rename_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU holds the two echo-area buffers as Lisp OBJECTS in `echo_buffer[2]`
    // and re-creates one only when it has DIED
    // (`ensure_echo_area_buffers', src/xdisp.c:12862-12884). Two consequences
    // follow that a name lookup cannot reproduce, and both are measured on GNU
    // Emacs 31.0.90 by scripts/l215-echo-area-identity-probe.el:
    //
    //   * renaming the echo buffer keeps the echo area attached to it -- the
    //     next message lands in the RENAMED buffer, and no fresh
    //     " *Echo Area 0*" is manufactured;
    //   * a user buffer that afterwards takes the freed name is NOT the echo
    //     area, and messages must not overwrite it.
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    builtin_message(&mut eval, vec![Value::string("first")]).expect("message");
    let echo0 = eval
        .buffers
        .find_buffer_by_name(" *Echo Area 0*")
        .expect("echo area buffer");
    eval.buffers
        .rename_buffer(echo0, Value::string(" *Echo Area RENAMED*"))
        .expect("rename echo area buffer");

    builtin_message(&mut eval, vec![Value::string("second")]).expect("message");
    assert_eq!(
        eval.buffers
            .get(echo0)
            .expect("renamed echo buffer")
            .buffer_string(),
        "second",
        "the echo area must follow its buffer across a rename"
    );
    assert!(
        eval.buffers.find_buffer_by_name(" *Echo Area 0*").is_none(),
        "a live echo buffer must not be re-created under its old name"
    );

    let user = eval.buffers.create_buffer(" *Echo Area 0*");
    eval.buffers
        .replace_buffer_contents(user, "PRECIOUS USER CONTENT")
        .expect("seed user buffer");
    builtin_message(&mut eval, vec![Value::string("third")]).expect("message");
    assert_eq!(
        eval.buffers.get(user).expect("user buffer").buffer_string(),
        "PRECIOUS USER CONTENT",
        "a user buffer holding the echo area's old NAME is not the echo area"
    );
    assert_eq!(
        eval.buffers
            .get(echo0)
            .expect("renamed echo buffer")
            .buffer_string(),
        "third"
    );
}

#[test]
fn echo_area_buffer_is_re_created_only_after_it_dies_like_gnu() {
    crate::test_utils::init_test_tracing();
    // The other half of `ensure_echo_area_buffers': a DEAD echo buffer is
    // replaced, and `Fget_buffer_create' is what replaces it -- so a user
    // buffer already standing at the canonical name becomes the echo area,
    // exactly as it does in GNU (src/xdisp.c:12868-12876).
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    builtin_message(&mut eval, vec![Value::string("first")]).expect("message");
    let echo0 = eval
        .buffers
        .find_buffer_by_name(" *Echo Area 0*")
        .expect("echo area buffer");
    eval.buffers.kill_buffer(echo0);
    assert!(eval.buffers.get(echo0).is_none(), "echo buffer killed");

    builtin_message(&mut eval, vec![Value::string("second")]).expect("message");
    let replacement = eval
        .buffers
        .find_buffer_by_name(" *Echo Area 0*")
        .expect("echo area buffer re-created after death");
    assert_ne!(replacement, echo0);
    assert_eq!(
        eval.buffers
            .get(replacement)
            .expect("echo buffer")
            .buffer_string(),
        "second"
    );
}

#[test]
fn format_message_preserves_raw_unibyte_payload_without_quoting() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let formatted = dispatch_builtin(&mut eval, "format-message", vec![raw])
        .expect("format-message should resolve")
        .expect("format-message should evaluate");
    let formatted = formatted.as_lisp_string().expect("format-message string");
    assert!(!formatted.is_multibyte());
    assert_eq!(formatted.as_bytes(), &[0xFF]);
}

/// `message`'s RETURN VALUE keeps the raw unibyte payload; `current-message`
/// does not, because it is a different string.
///
/// GNU `Fmessage` returns what `format-message` produced -- unibyte in, unibyte
/// out. `current-message` is the echo BUFFER read back (`current_message_1`,
/// src/xdisp.c:13437-13446) after `set_message_1` forced that buffer multibyte
/// (src/xdisp.c:13595-13598) and inserted through `insert_from_string`
/// (src/xdisp.c:13615), which promotes byte 0xFF to the raw-byte character
/// 0x3FFFFF. The two answers differ on purpose.
///
/// Measured on GNU Emacs 31.0.90 under a pty at 80x24, both `(message "%s"
/// (unibyte-string 255))` and `(message (unibyte-string 255))`:
/// ```text
///   message-return   multibyte=nil bytes=1 length=1
///   current-message  multibyte=t   bytes=2 length=1 chars=(4194303)
///   echo0            multibyte=t   point-max=2      chars=(4194303)
/// ```
/// This test used to assert `current-message` came back unibyte, which is the
/// half that contradicts GNU (ledger 217).
#[test]
fn current_message_promotes_a_raw_unibyte_payload_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // Interactive echo-area path: GNU only populates `current-message` when not
    // in batch mode (xdisp.c `message3_frame_nolog`).
    eval.set_variable("noninteractive", Value::NIL);
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));

    let displayed =
        builtin_message(&mut eval, vec![raw]).expect("message should preserve raw unibyte echo");
    let displayed = displayed.as_lisp_string().expect("message result string");
    assert!(!displayed.is_multibyte());
    assert_eq!(displayed.as_bytes(), &[0xFF]);

    let current = builtin_current_message(&mut eval, vec![])
        .expect("current-message should answer after an interactive message");
    let current = current.as_lisp_string().expect("current-message string");
    assert!(
        current.is_multibyte(),
        "GNU reads current-message out of the multibyte echo buffer"
    );
    assert_eq!(current.schars(), 1, "one character, the raw byte");
    assert_eq!(
        current.sbytes(),
        2,
        "raw byte 0xFF is the two-byte raw-byte character 0x3FFFFF"
    );
}

#[test]
fn echo_area_print_after_message_replaces_message_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    eval.eval_str(r#"(progn (message "inner") (prin1 "outer" t) nil)"#)
        .expect("message followed by echo print should evaluate");

    assert_eq!(
        eval.current_message_text(),
        Some("\"outer\"".to_string()),
        "GNU xdisp.c setup_echo_area_for_printing clears the prior message when message_buf_print is false"
    );
}

#[test]
fn echo_area_print_appends_the_same_fragment_to_messages_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    eval.eval_str(r#"(progn (message "inner") (prin1 "outer" t) nil)"#)
        .expect("message followed by echo print should evaluate");

    let messages = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .and_then(|id| eval.buffers.get(id))
        .expect("echo print should retain the messages buffer");
    assert_eq!(messages.buffer_string(), "inner\n\"outer\"");
}

#[test]
fn clearing_echo_area_terminates_a_pending_print_fragment_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    eval.eval_str(r#"(progn (message "inner") (prin1 "outer" t) (message ""))"#)
        .expect("clearing the echo area should terminate pending printer output");

    let messages = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .and_then(|id| eval.buffers.get(id))
        .expect("echo print should retain the messages buffer");
    assert_eq!(messages.buffer_string(), "inner\n\"outer\"\n");
}

#[test]
fn echo_area_print_calls_append_until_next_message_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::NIL);

    eval.eval_str("(progn (prin1 'a t) (prin1 'b t) nil)")
        .expect("consecutive echo prints should evaluate");
    assert_eq!(eval.current_message_text(), Some("ab".to_string()));

    eval.eval_str(r#"(progn (message "inner") (prin1 'c t) nil)"#)
        .expect("message should reset echo print append state");
    assert_eq!(eval.current_message_text(), Some("c".to_string()));
}

#[test]
fn inhibit_message_has_gnu_native_boolean_storage_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = eval.eval_str(
        r#"(let ((set-result (set 'inhibit-message :assigned)))
             (list set-result
                   inhibit-message
                   (let ((inhibit-message :inner))
                     (list inhibit-message
                           (condition-case error-data
                               (signal 'error (list inhibit-message))
                             (error
                              (list (car (cdr error-data))
                                    inhibit-message)))))
                   inhibit-message))"#,
    );

    // GNU xdisp.c registers this variable with DEFVAR_BOOL.  data.c's
    // store_symval_forwarding therefore preserves `set' return values while
    // canonicalizing every non-nil stored value (including specbindings and
    // their restoration) to t.
    assert_eq!(format_eval_result(&result), "OK (:assigned t (t (t t)) t)");
}

#[test]
fn inhibited_message_preserves_the_current_echo_area_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // Interactive echo-area path (GNU populates `current-message` only when not
    // in batch mode, xdisp.c `message3_frame_nolog`).
    eval.set_variable("noninteractive", Value::NIL);

    builtin_message(&mut eval, vec![Value::string("visible")]).expect("seed visible message");
    assert_eq!(eval.current_message_text(), Some("visible".to_string()));

    eval.eval_str("(let ((inhibit-message t)) (message \"hidden\"))")
        .expect("message with inhibit-message should evaluate");

    // GNU `message3' always logs first, but `clear_message (true, true)'
    // refuses to clear the current echo-area slot while `inhibit-message' is
    // non-nil, and `message3_nolog' is skipped.  Thus the hidden message is
    // logged without replacing or erasing the message the user can see.
    assert_eq!(eval.current_message_text(), Some("visible".to_string()));
}

#[test]
fn message_uses_set_message_function_string_and_suppression_results() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // set-message-function only runs on the interactive echo-area path; GNU's
    // batch path (xdisp.c `message3_frame_nolog`) routes straight to stderr.
    eval.set_variable("noninteractive", Value::NIL);

    eval.eval_str(
        r#"(progn
             (fset 'neomacs-test-message-prefix
                   (lambda (message) (concat "prefix:" message)))
             (fset 'neomacs-test-message-suppress
                   (lambda (_message) t)))"#,
    )
    .expect("install set-message-function stubs");

    eval.assign(
        "set-message-function",
        Value::symbol("neomacs-test-message-prefix"),
    );
    builtin_message(&mut eval, vec![Value::string("hello")])
        .expect("message should use string rewriting set-message-function");
    assert_eq!(
        eval.current_message_text(),
        Some("prefix:hello".to_string())
    );

    eval.assign(
        "set-message-function",
        Value::symbol("neomacs-test-message-suppress"),
    );
    builtin_message(&mut eval, vec![Value::string("suppressed")])
        .expect("message should allow set-message-function to suppress echo");
    assert_eq!(eval.current_message_text(), None);
}

#[test]
fn message_nil_runs_clear_message_function_after_suppressed_minibuffer_message() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // set-message-function / clear-message-function only run on the interactive
    // echo-area path; GNU's batch path (xdisp.c `message3_frame_nolog`) routes
    // straight to stderr and never invokes them.
    eval.set_variable("noninteractive", Value::NIL);

    eval.eval_str(
        r#"
        (progn
          (defvar neomacs-clear-message-count)
          (defvar neomacs-minibuffer-message-overlay)
          (setq neomacs-clear-message-count 0)
          (setq neomacs-minibuffer-message-overlay nil)
          (setq set-message-function
                (lambda (message)
                  (setq neomacs-minibuffer-message-overlay
                        (make-overlay (point-max) (point-max)))
                  (overlay-put neomacs-minibuffer-message-overlay
                               'after-string message)
                  t))
          (setq clear-message-function
                (lambda ()
                  (setq neomacs-clear-message-count
                        (1+ neomacs-clear-message-count))
                  (if (overlayp neomacs-minibuffer-message-overlay)
                      (delete-overlay neomacs-minibuffer-message-overlay))
                  nil)))"#,
    )
    .expect("install message hooks");

    builtin_message(&mut eval, vec![Value::string("Making completion list...")])
        .expect("set-message-function should handle message");
    assert_eq!(eval.current_message_text(), None);
    assert_eq!(
        eval.eval_str("(overlay-buffer neomacs-minibuffer-message-overlay)")
            .expect("overlay should exist")
            .is_nil(),
        false
    );

    builtin_message(&mut eval, vec![Value::NIL]).expect("message nil should clear");

    assert_eq!(
        eval.eval_str("neomacs-clear-message-count")
            .expect("clear-message count"),
        Value::fixnum(1)
    );
    assert_eq!(
        eval.eval_str("(overlay-buffer neomacs-minibuffer-message-overlay)")
            .expect("overlay should be deleted"),
        Value::NIL
    );
}

#[test]
fn message_ignores_non_function_set_message_function_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // Interactive echo-area path (GNU `message3_frame_nolog` only sets the echo
    // area when not in batch mode).
    eval.set_variable("noninteractive", Value::NIL);

    eval.assign("set-message-function", Value::T);
    builtin_message(&mut eval, vec![Value::string("plain")])
        .expect("GNU ignores non-function set-message-function");

    assert_eq!(eval.current_message_text(), Some("plain".to_string()));
}

#[test]
fn message_demotes_set_message_function_errors_like_gnu_dsafe_call() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // set-message-function only runs on the interactive echo-area path.
    eval.set_variable("noninteractive", Value::NIL);

    eval.eval_str(
        r#"(fset 'neomacs-test-message-error
                 (lambda (_message) (error "bad message hook")))"#,
    )
    .expect("install signaling set-message-function");
    eval.assign(
        "set-message-function",
        Value::symbol("neomacs-test-message-error"),
    );

    builtin_message(&mut eval, vec![Value::string("still visible")])
        .expect("GNU dsafe_call1 demotes set-message-function errors");

    assert_eq!(
        eval.current_message_text(),
        Some("still visible".to_string())
    );
}

#[test]
fn format_message_promotes_unibyte_ascii_to_multibyte_when_quoting() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let formatted = dispatch_builtin(&mut eval, "format-message", vec![Value::string("`'")])
        .expect("format-message should resolve")
        .expect("format-message should evaluate");
    let formatted = formatted.as_lisp_string().expect("format-message string");
    assert!(formatted.is_multibyte());
    assert_eq!(formatted.as_utf8_str(), Some("\u{2018}\u{2019}"));
}

#[test]
fn message_eval_stores_echo_text_without_immediate_redisplay() {
    // GNU Emacs editfns.c Fmessage → message3 → message3_nolog stores the
    // echo text but does NOT call redisplay() — the message becomes visible
    // during the next natural redisplay cycle in read_char().  Verify
    // NeoMacs matches: message() updates current-message but does not
    // invoke the redisplay callback.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // Interactive echo-area path: GNU populates `current-message` only when not
    // in batch mode (xdisp.c `message3_frame_nolog`).
    eval.set_variable("noninteractive", Value::NIL);
    let redisplay_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let redisplay_count_capture = std::sync::Arc::clone(&redisplay_count);

    eval.redisplay_fn = Some(Box::new(move |_ev| {
        redisplay_count_capture.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));

    builtin_message(&mut eval, vec![Value::string("hello echo")])
        .expect("message eval should store echo text");
    assert_eq!(eval.current_message_text(), Some("hello echo".to_string()));
    builtin_message(&mut eval, vec![Value::NIL]).expect("message eval should clear");
    assert_eq!(eval.current_message_text(), None);

    // GNU semantic: message() must NOT invoke redisplay.
    assert_eq!(
        redisplay_count.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "GNU Emacs message() does not call redisplay"
    );
}

#[test]
fn batch_newline_output_uses_stdout_not_echo_area() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("noninteractive", Value::T);

    eval.eval_str("(progn (terpri) (write-char 10) nil)")
        .expect("batch newline output should evaluate");

    assert_eq!(
        eval.current_message_text(),
        None,
        "GNU print.c routes batch terpri/write-char to stdout, not current-message"
    );
}

#[test]
fn make_string_nonunicode_char_code_bounds_match_oracle() {
    crate::test_utils::init_test_tracing();
    let overflow = dispatch_builtin_pure(
        "make-string",
        vec![Value::fixnum(1), Value::fixnum(0x40_0000)],
    )
    .expect("make-string should resolve")
    .expect_err("make-string should reject out-of-range character code");
    match overflow {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    let repeated = dispatch_builtin_pure(
        "make-string",
        vec![Value::fixnum(2), Value::fixnum(0x11_0000)],
    )
    .expect("make-string should resolve")
    .expect("make-string should evaluate");
    assert_eq!(
        decode_value_char_codes(&repeated),
        vec![0x11_0000, 0x11_0000]
    );

    let high = dispatch_builtin_pure(
        "make-string",
        vec![Value::fixnum(1), Value::fixnum(0x20_0000)],
    )
    .expect("make-string should resolve")
    .expect("make-string should evaluate");
    assert_eq!(decode_value_char_codes(&high), vec![0x20_0000]);
}

#[test]
fn make_string_matches_emacs_ascii_boundary() {
    crate::test_utils::init_test_tracing();
    let ascii = dispatch_builtin_pure(
        "make-string",
        vec![Value::fixnum(3), Value::fixnum('a' as i64)],
    )
    .expect("make-string should resolve")
    .expect("ascii make-string should evaluate");
    let byte_200 = dispatch_builtin_pure("make-string", vec![Value::fixnum(2), Value::fixnum(200)])
        .expect("make-string should resolve")
        .expect("byte-200 make-string should evaluate");

    let ascii_multibyte = dispatch_builtin_pure("multibyte-string-p", vec![ascii])
        .expect("multibyte-string-p should resolve")
        .expect("ascii multibyte-string-p should evaluate");
    let byte_200_multibyte = dispatch_builtin_pure("multibyte-string-p", vec![byte_200])
        .expect("multibyte-string-p should resolve")
        .expect("byte-200 multibyte-string-p should evaluate");

    assert_eq!(ascii_multibyte, Value::NIL);
    assert_eq!(byte_200_multibyte, Value::T);
}

#[test]
fn string_matches_emacs_ascii_boundary() {
    crate::test_utils::init_test_tracing();
    let ascii = dispatch_builtin_pure(
        "string",
        vec![
            Value::fixnum('a' as i64),
            Value::fixnum('b' as i64),
            Value::fixnum('c' as i64),
        ],
    )
    .expect("string should resolve")
    .expect("ascii string should evaluate");
    let non_ascii = dispatch_builtin_pure("string", vec![Value::fixnum(0xE9)])
        .expect("string should resolve")
        .expect("non-ASCII string should evaluate");

    let ascii_multibyte = dispatch_builtin_pure("multibyte-string-p", vec![ascii])
        .expect("multibyte-string-p should resolve")
        .expect("ascii multibyte-string-p should evaluate");
    let non_ascii_multibyte = dispatch_builtin_pure("multibyte-string-p", vec![non_ascii])
        .expect("multibyte-string-p should resolve")
        .expect("non-ASCII multibyte-string-p should evaluate");

    assert_eq!(ascii_multibyte, Value::NIL);
    assert_eq!(non_ascii_multibyte, Value::T);
}

#[test]
fn text_char_description_nonunicode_char_code_bounds_match_oracle() {
    crate::test_utils::init_test_tracing();
    // Non-Unicode chars produce strings with lossy UTF-8 rendering
    let high = dispatch_builtin_pure("text-char-description", vec![Value::fixnum(0x11_0000)])
        .expect("text-char-description should resolve")
        .expect("text-char-description should evaluate");
    assert!(high.is_string());

    let higher = dispatch_builtin_pure("text-char-description", vec![Value::fixnum(0x20_0000)])
        .expect("text-char-description should resolve")
        .expect("text-char-description should evaluate");
    assert!(higher.is_string());

    let overflow = dispatch_builtin_pure("text-char-description", vec![Value::fixnum(0x40_0000)])
        .expect("text-char-description should resolve")
        .expect_err("text-char-description should reject out-of-range character code");
    match overflow {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }
}

#[test]
fn assoc_delete_all_supports_default_equal_and_optional_test() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);
    let results = eval
        .eval_str_each(
            r#"
        (assoc-delete-all "foo" '(("foo" . 1) ignored ("bar" . 2) ("foo" . 3)))
        (let* ((key "foo")
               (alist (list (cons key 9) (cons (copy-sequence "foo") 10))))
          (assoc-delete-all key alist 'eq))
        (condition-case err (assoc-delete-all nil nil nil nil) (error (car err)))
        "#,
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(results[0], r#"OK (ignored ("bar" . 2))"#);
    assert_eq!(results[1], r#"OK (("foo" . 10))"#);
    assert_eq!(results[2], "OK wrong-number-of-arguments");
}

#[test]
fn insert_char_nonunicode_char_code_bounds_match_oracle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_erase_buffer(&mut eval, vec![]).expect("erase-buffer should succeed");
    builtin_insert_char(&mut eval, vec![Value::fixnum(0x11_0000), Value::fixnum(1)])
        .expect("insert-char should accept nonunicode char code");
    let first = builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should evaluate");
    assert_eq!(decode_value_char_codes(&first), vec![0x11_0000]);

    builtin_erase_buffer(&mut eval, vec![]).expect("erase-buffer should succeed");
    builtin_insert_char(&mut eval, vec![Value::fixnum(0x20_0000), Value::fixnum(2)])
        .expect("insert-char should repeat nonunicode char code");
    let second = builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should evaluate");
    assert_eq!(decode_value_char_codes(&second), vec![0x20_0000, 0x20_0000]);

    let overflow = builtin_insert_char(&mut eval, vec![Value::fixnum(0x40_0000), Value::fixnum(1)])
        .expect_err("insert-char should reject out-of-range character code");
    match overflow {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }
}

#[test]
fn insert_nonunicode_integer_arguments_match_oracle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_erase_buffer(&mut eval, vec![]).expect("erase-buffer should succeed");
    builtin_insert(&mut eval, vec![Value::fixnum(0x11_0000)])
        .expect("insert should accept nonunicode integer char code");
    let first = builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should evaluate");
    assert_eq!(decode_value_char_codes(&first), vec![0x11_0000]);

    builtin_erase_buffer(&mut eval, vec![]).expect("erase-buffer should succeed");
    builtin_insert(
        &mut eval,
        vec![Value::fixnum(0x20_0000), Value::fixnum(0x20_0000)],
    )
    .expect("insert should repeat nonunicode integer char codes");
    let second = builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should evaluate");
    assert_eq!(decode_value_char_codes(&second), vec![0x20_0000, 0x20_0000]);

    let overflow = builtin_insert(&mut eval, vec![Value::fixnum(0x40_0000)])
        .expect_err("insert should reject out-of-range integer char code");
    match overflow {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("char-or-string-p"), Value::fixnum(0x40_0000)]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }
}

#[test]
fn insert_byte_matches_gnu_multibyte_and_unibyte_storage() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_insert_byte(&mut eval, vec![Value::fixnum(65), Value::fixnum(2)])
        .expect("insert-byte should insert ASCII bytes");
    let ascii = builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should evaluate");
    let ascii_ls = ascii
        .as_lisp_string()
        .expect("buffer-string should return string");
    assert!(ascii_ls.is_multibyte());
    assert_eq!(ascii_ls.as_utf8_str(), Some("AA"));

    builtin_erase_buffer(&mut eval, vec![]).expect("erase-buffer should succeed");
    builtin_insert_byte(&mut eval, vec![Value::fixnum(200), Value::fixnum(1)])
        .expect("insert-byte should insert raw byte chars in multibyte buffers");
    let multibyte =
        builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should evaluate");
    let multibyte_ls = multibyte
        .as_lisp_string()
        .expect("buffer-string should return string");
    assert!(multibyte_ls.is_multibyte());
    assert_eq!(decode_value_char_codes(&multibyte), vec![0x3FFF00 + 200]);

    builtin_erase_buffer(&mut eval, vec![]).expect("erase-buffer should succeed");
    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_multibyte_flag(current_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    builtin_insert_byte(&mut eval, vec![Value::fixnum(200), Value::fixnum(1)])
        .expect("insert-byte should insert plain bytes in unibyte buffers");
    let unibyte = builtin_buffer_string(&mut eval, vec![]).expect("buffer-string should evaluate");
    let unibyte_ls = unibyte
        .as_lisp_string()
        .expect("buffer-string should return string");
    assert!(!unibyte_ls.is_multibyte());
    assert_eq!(unibyte_ls.as_bytes(), &[200]);
}

#[test]
fn buffer_string_returns_unibyte_storage_for_unibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_multibyte_flag(current_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    builtin_insert(&mut eval, vec![Value::string("é")]).unwrap();

    let value = builtin_buffer_string(&mut eval, vec![]).unwrap();
    let ls = value.as_lisp_string().expect("buffer-string result");
    assert!(!ls.is_multibyte());
    assert_eq!(ls.as_bytes(), &[0xE9]);
}

#[test]
fn set_buffer_multibyte_t_preserves_positions_for_consecutive_eight_bit() {
    // Regression: converting consecutive raw high bytes to multibyte produces
    // eight-bit characters (each 2 bytes, GNU `[0xC1 0xBF]` for byte 255).
    // Point, point-max and markers must land on the right characters — earlier
    // the byte->char position cache was poisoned by mid-character byte queries,
    // collapsing the whole buffer to a unibyte (byte==char) mapping.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap();
    for b in [255i64, 254, 255] {
        builtin_insert_byte(&mut eval, vec![Value::fixnum(b), Value::fixnum(1)]).unwrap();
    }
    builtin_goto_char(&mut eval, vec![Value::fixnum(2)]).unwrap();
    builtin_set_buffer_multibyte(&mut eval, vec![Value::T]).unwrap();

    // 3 eight-bit characters, point preserved at character 2.
    assert_eq!(
        builtin_point_max(&mut eval, vec![]).unwrap(),
        Value::fixnum(4)
    );
    assert_eq!(builtin_point(&mut eval, vec![]).unwrap(), Value::fixnum(2));
    assert_eq!(
        builtin_char_after(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(0x3FFF00 + 255)
    );
    assert_eq!(
        builtin_char_after(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(0x3FFF00 + 254)
    );
    assert_eq!(
        builtin_char_after(&mut eval, vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(0x3FFF00 + 255)
    );
    // position-bytes mirrors the eight-bit byte layout (char N -> byte 2N-1).
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(3)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(5)
    );
}

#[test]
fn set_buffer_multibyte_reinterprets_bytes_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_insert(&mut eval, vec![Value::string("é")]).unwrap();
    assert_eq!(
        builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );

    let unibyte = builtin_buffer_string(&mut eval, vec![]).unwrap();
    let unibyte_ls = unibyte.as_lisp_string().expect("unibyte buffer-string");
    assert!(!unibyte_ls.is_multibyte());
    assert_eq!(unibyte_ls.as_bytes(), &[0xC3, 0xA9]);
    assert_eq!(
        builtin_point_max(&mut eval, vec![]).unwrap(),
        Value::fixnum(3)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(3)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(169)
    );

    builtin_erase_buffer(&mut eval, vec![]).unwrap();
    builtin_insert_byte(&mut eval, vec![Value::fixnum(195), Value::fixnum(1)]).unwrap();
    builtin_insert_byte(&mut eval, vec![Value::fixnum(169), Value::fixnum(1)]).unwrap();
    assert_eq!(
        builtin_set_buffer_multibyte(&mut eval, vec![Value::T]).unwrap(),
        Value::T
    );
    let multibyte = builtin_buffer_string(&mut eval, vec![]).unwrap();
    let multibyte_ls = multibyte.as_lisp_string().expect("multibyte buffer-string");
    assert!(multibyte_ls.is_multibyte());
    assert_eq!(multibyte_ls.as_utf8_str(), Some("é"));
    assert_eq!(
        builtin_point_max(&mut eval, vec![]).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(3)
    );

    builtin_erase_buffer(&mut eval, vec![]).unwrap();
    builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap();
    builtin_insert_byte(&mut eval, vec![Value::fixnum(200), Value::fixnum(1)]).unwrap();
    assert_eq!(
        builtin_set_buffer_multibyte(&mut eval, vec![Value::symbol("to")]).unwrap(),
        Value::symbol("to")
    );
    let raw = builtin_buffer_string(&mut eval, vec![]).unwrap();
    assert_eq!(decode_value_char_codes(&raw), vec![0x3FFF00 + 200]);
    assert_eq!(
        builtin_point_max(&mut eval, vec![]).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(3)
    );
}

#[test]
fn set_buffer_multibyte_remaps_last_window_start_as_lisp_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let current_id = eval.buffers.current_buffer_id().expect("current buffer");

    builtin_insert(&mut eval, vec![Value::string("éx")]).unwrap();
    eval.buffers
        .get_mut(current_id)
        .expect("current buffer")
        .last_window_start = LispCharPos1::from_one_based_usize(2);

    builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(
        eval.buffers
            .get(current_id)
            .expect("current buffer")
            .last_window_start,
        LispCharPos1::from_one_based_usize(3)
    );

    builtin_set_buffer_multibyte(&mut eval, vec![Value::T]).unwrap();
    assert_eq!(
        eval.buffers
            .get(current_id)
            .expect("current buffer")
            .last_window_start,
        LispCharPos1::from_one_based_usize(2)
    );
}

#[test]
fn set_buffer_multibyte_rejects_narrowed_and_indirect_buffers() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_insert(&mut eval, vec![Value::string("abcdef")]).unwrap();
    builtin_narrow_to_region(&mut eval, vec![Value::fixnum(2), Value::fixnum(5)]).unwrap();
    let narrowed = builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap_err();
    match narrowed {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Changing multibyteness in a narrowed buffer")]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }

    builtin_widen(&mut eval, vec![]).unwrap();
    let base = eval.buffers.current_buffer_id().expect("current buffer");
    let indirect = builtin_make_indirect_buffer(
        &mut eval,
        vec![
            Value::make_buffer(base),
            Value::string("*set-buffer-multibyte-indirect*"),
        ],
    )
    .unwrap()
    .as_buffer_id()
    .expect("indirect buffer id");
    eval.buffers.set_current(indirect);

    let indirect_err = builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap_err();
    match indirect_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Cannot do `set-buffer-multibyte' on an indirect buffer"
                )]
            );
        }
        other => panic!("expected signal, got: {other:?}"),
    }
}

#[test]
fn set_buffer_multibyte_records_gnu_style_undo_entry() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .get_multibyte()
    );
    assert_eq!(
        builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );

    let undo_list = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .get_undo_list();
    let entries = crate::emacs_core::value::list_to_vec(&undo_list).expect("undo list");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&entries[0]).expect("undo entry"),
        vec![
            Value::symbol("apply"),
            Value::symbol("set-buffer-multibyte"),
            Value::T,
        ]
    );

    // Replaying it is lisp/simple.el's `primitive-undo' -- GNU has no C
    // version and neither do we -- so the round trip is asked of the loaded
    // runtime rather than of this bare evaluator.
    // Transcribed from GNU Emacs 31.0.90 -Q --batch (tmp/pw52-gnu3.el).
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(
            r#"(with-temp-buffer
                 (buffer-enable-undo)
                 (setq buffer-undo-list nil)
                 (set-buffer-multibyte nil)
                 (let ((ul buffer-undo-list))
                   (list ul
                         (progn (primitive-undo 1 ul) enable-multibyte-characters))))"#,
        ),
        "OK (((apply set-buffer-multibyte t)) t)"
    );
}

#[test]
fn insert_string_converts_props_from_multibyte_source_to_unibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_multibyte_flag(current_id, false)
        .expect("set-buffer-multibyte should accept current buffer");

    let text = Value::string("éz");
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    let _ = put_string_property(
        &mut table,
        0,
        2,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(text, table);

    builtin_insert(&mut eval, vec![text]).unwrap();

    let result = builtin_buffer_string(&mut eval, vec![]).unwrap();
    let ls = result.as_lisp_string().expect("buffer-string result");
    assert!(!ls.is_multibyte());
    assert_eq!(ls.as_bytes(), &[0xE9, b'z']);

    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("string properties");
    let intervals = props.intervals_snapshot();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].start, 0);
    assert_eq!(intervals[0].end, 2);
    assert_eq!(
        intervals[0].properties.get(&Value::symbol("face")),
        Some(&Value::symbol("bold"))
    );
}

#[test]
fn byte_position_helpers_count_multibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_insert_byte(&mut eval, vec![Value::fixnum(200), Value::fixnum(1)]).unwrap();

    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_position_bytes(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(3)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_byte_to_position(&mut eval, vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(2)
    );
}

#[test]
fn set_buffer_multibyte_preserves_marker_and_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_insert(&mut eval, vec![Value::string("é")]).unwrap();
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();
    builtin_goto_char(&mut eval, vec![Value::fixnum(2)]).unwrap();
    let marker = crate::emacs_core::marker::builtin_point_marker(&mut eval, vec![])
        .expect("point-marker should succeed");

    builtin_set_buffer_multibyte(&mut eval, vec![Value::NIL]).unwrap();

    assert_eq!(builtin_point(&mut eval, vec![]).unwrap(), Value::fixnum(3));
    assert_eq!(
        crate::emacs_core::marker::marker_position_as_int_with_buffers(&eval.buffers, &marker)
            .expect("marker position"),
        3
    );

    assert_eq!(
        crate::emacs_core::textprop::builtin_get_text_property(
            &mut eval,
            vec![Value::fixnum(1), Value::symbol("face")]
        )
        .unwrap(),
        Value::symbol("bold")
    );
    assert_eq!(
        crate::emacs_core::textprop::builtin_get_text_property(
            &mut eval,
            vec![Value::fixnum(2), Value::symbol("face")]
        )
        .unwrap(),
        Value::symbol("bold")
    );
    assert_eq!(
        crate::emacs_core::textprop::builtin_get_text_property(
            &mut eval,
            vec![Value::fixnum(3), Value::symbol("face")]
        )
        .unwrap(),
        Value::NIL
    );
}

#[test]
fn insert_unibyte_string_converts_props_into_multibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let text = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xE9, b'z',
    ]));
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    let _ = put_string_property(
        &mut table,
        0,
        1,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(text, table);

    builtin_insert(&mut eval, vec![text]).unwrap();

    let result = builtin_buffer_string(&mut eval, vec![]).unwrap();
    let ls = result.as_lisp_string().expect("buffer-string result");
    assert!(ls.is_multibyte());
    assert_eq!(
        decode_value_char_codes(&result),
        vec![0x3FFF00 + 0xE9, b'z' as u32]
    );

    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("string properties");
    let intervals = props.intervals_snapshot();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].start, 0);
    assert_eq!(intervals[0].end, 1);
    assert_eq!(
        intervals[0].properties.get(&Value::symbol("face")),
        Some(&Value::symbol("bold"))
    );
}

#[test]
fn char_queries_return_nonunicode_emacs_character_codes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_insert_char(&mut eval, vec![Value::fixnum(0x20_0000), Value::fixnum(1)]).unwrap();

    assert_eq!(
        builtin_char_after(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(0x20_0000)
    );
    assert_eq!(
        crate::emacs_core::editfns::builtin_following_char(&eval, vec![]).unwrap(),
        Value::fixnum(0)
    );
    builtin_goto_char(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(
        crate::emacs_core::editfns::builtin_preceding_char(&eval, vec![]).unwrap(),
        Value::fixnum(0x20_0000)
    );
}

#[test]
fn delete_char_handles_nonunicode_storage_units() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    crate::test_utils::load_gnu_undo_auto_runtime(&mut eval);
    let test_buffer = create_unique_test_buffer(&mut eval, "*delete-char-nonunicode*");
    builtin_set_buffer(&mut eval, vec![test_buffer]).unwrap();

    builtin_insert_char(&mut eval, vec![Value::fixnum(0x20_0000), Value::fixnum(1)]).unwrap();
    builtin_insert(&mut eval, vec![Value::string("A")]).unwrap();
    builtin_goto_char(&mut eval, vec![Value::fixnum(1)]).unwrap();
    crate::emacs_core::editfns::builtin_delete_char(&mut eval, vec![Value::fixnum(1)]).unwrap();

    let result = builtin_buffer_string(&mut eval, vec![]).unwrap();
    assert_eq!(decode_value_char_codes(&result), vec![b'A' as u32]);
}

#[test]
fn delete_char_handles_unibyte_high_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    crate::test_utils::load_gnu_undo_auto_runtime(&mut eval);
    let test_buffer = create_unique_test_buffer(&mut eval, "*delete-char-unibyte*");
    builtin_set_buffer(&mut eval, vec![test_buffer]).unwrap();

    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_multibyte_flag(current_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    builtin_insert_byte(&mut eval, vec![Value::fixnum(200), Value::fixnum(1)]).unwrap();
    builtin_insert_byte(&mut eval, vec![Value::fixnum(65), Value::fixnum(1)]).unwrap();
    builtin_goto_char(&mut eval, vec![Value::fixnum(1)]).unwrap();
    crate::emacs_core::editfns::builtin_delete_char(&mut eval, vec![Value::fixnum(1)]).unwrap();

    let result = builtin_buffer_string(&mut eval, vec![]).unwrap();
    let ls = result.as_lisp_string().expect("buffer-string result");
    assert!(!ls.is_multibyte());
    assert_eq!(ls.as_bytes(), &[65]);
}

#[test]
fn format_message_and_message_signal_strict_format_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    for builtin in ["format", "format-message"] {
        for bad in [Value::fixnum(1), Value::NIL, Value::symbol("foo")] {
            let err = dispatch_builtin(&mut eval, builtin, vec![bad])
                .expect("builtin should resolve")
                .expect_err("builtin should signal for non-string format");
            match err {
                Flow::Signal(sig) => {
                    assert_eq!(sig.symbol_name(), "wrong-type-argument");
                    assert_eq!(sig.data, vec![Value::symbol("stringp"), bad]);
                }
                other => panic!("expected signal, got: {other:?}"),
            }
        }
    }

    for bad in [Value::fixnum(1), Value::symbol("foo")] {
        let err = dispatch_builtin(&mut eval, "message", vec![bad])
            .expect("message should resolve")
            .expect_err("message should signal for non-string/non-nil format");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("stringp"), bad]);
            }
            other => panic!("expected signal, got: {other:?}"),
        }
    }

    for builtin in ["format", "format-message", "message"] {
        let err = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%s %s"), Value::fixnum(1)],
        )
        .expect("builtin should resolve")
        .expect_err("builtin should signal when format args are missing");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Not enough arguments for format string")]
                );
            }
            other => panic!("expected signal, got: {other:?}"),
        }
    }

    for builtin in ["format", "format-message", "message"] {
        for spec in ["%d", "%f", "%c"] {
            let err = dispatch_builtin(
                &mut eval,
                builtin,
                vec![Value::string(spec), Value::string("x")],
            )
            .expect("builtin should resolve")
            .expect_err("builtin should signal on spec/type mismatch");
            match err {
                Flow::Signal(sig) => {
                    assert_eq!(sig.symbol_name(), "error");
                    assert_eq!(
                        sig.data,
                        vec![Value::string(
                            "Format specifier doesn’t match argument type"
                        )]
                    );
                }
                other => panic!("expected signal, got: {other:?}"),
            }
        }
    }

    for builtin in ["format", "format-message", "message"] {
        let err = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%c"), Value::fixnum(-1)],
        )
        .expect("builtin should resolve")
        .expect_err("builtin should reject negative character code");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("characterp"), Value::fixnum(-1)]
                );
            }
            other => panic!("expected signal, got: {other:?}"),
        }
    }

    for builtin in ["format", "format-message", "message"] {
        let err = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%c"), Value::fixnum(0x40_0000)],
        )
        .expect("builtin should resolve")
        .expect_err("builtin should reject out-of-range character code");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)]
                );
            }
            other => panic!("expected signal, got: {other:?}"),
        }
    }

    let high_chars = [
        ("format", 0x11_0000_i64),
        ("message", 0x11_0000_i64),
        ("format-message", 0x20_0000_i64),
    ];
    for (builtin, value) in high_chars {
        let rendered = dispatch_builtin(
            &mut eval,
            builtin,
            vec![Value::string("%c"), Value::fixnum(value)],
        )
        .expect("builtin should resolve")
        .expect("builtin should evaluate");
        // GNU-aligned read: `LispString.data` is Emacs internal
        // encoding (UTF-8 superset, character.h:227). For nonunicode
        // chars the bytes are the 5-byte extended form, not valid
        // UTF-8, so `as_utf8_str` would return None. Decode via
        // `emacs_char::string_char` matching GNU's `STRING_CHAR` macro.
        let bytes = rendered
            .as_lisp_string()
            .expect("builtin should return a string")
            .as_bytes();
        assert!(
            rendered
                .as_lisp_string()
                .expect("builtin should return a string")
                .is_multibyte(),
            "GNU Emacs returns a multibyte string for non-ASCII %c"
        );
        let mut codes = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let (code, n) = crate::emacs_core::emacs_char::string_char(&bytes[i..]);
            if n == 0 {
                break;
            }
            codes.push(code);
            i += n;
        }
        assert_eq!(codes, vec![value as u32]);
    }
}

#[test]
fn format_matches_gnu_numbered_fields_and_integer_aliases() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let numbered = dispatch_builtin(
        &mut eval,
        "format",
        vec![
            Value::string("%1$s %1$s %% %2$s"),
            Value::string("a"),
            Value::string("b"),
        ],
    )
    .expect("format should resolve")
    .expect("numbered format should evaluate");
    assert_eq!(
        numbered.as_runtime_string_owned().as_deref(),
        Some("a a % b")
    );

    let decimal_alias = dispatch_builtin(
        &mut eval,
        "format",
        vec![Value::string("%03i"), Value::fixnum(1)],
    )
    .expect("format should resolve")
    .expect("%i should evaluate like GNU decimal integer");
    assert_eq!(
        decimal_alias.as_runtime_string_owned().as_deref(),
        Some("001")
    );

    let binary = dispatch_builtin(
        &mut eval,
        "format",
        vec![Value::string("%#b %#B"), Value::fixnum(5), Value::fixnum(5)],
    )
    .expect("format should resolve")
    .expect("%b/%B should evaluate like GNU binary integer");
    assert_eq!(
        binary.as_runtime_string_owned().as_deref(),
        Some("0b101 0B101")
    );

    let alternate_floats = dispatch_builtin(
        &mut eval,
        "format",
        vec![
            Value::string("%#.0f %#.0g %#.3g %#g %#.0e"),
            Value::make_float(3.4),
            Value::make_float(3.4),
            Value::make_float(3.0),
            Value::make_float(3.0),
            Value::make_float(3.4),
        ],
    )
    .expect("format should resolve")
    .expect("alternate float formats should evaluate");
    assert_eq!(
        alternate_floats.as_runtime_string_owned().as_deref(),
        Some("3. 3. 3.00 3.00000 3.e+00")
    );
}

/// `user-error` is an Elisp function in GNU (subr.el:535), not a C builtin.
/// Test it through the bootstrap evaluator which loads subr.el.
#[test]
fn user_error_signals_user_error_symbol_and_formatted_message() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);
    let result = eval
        .eval_str(r#"(condition-case err (user-error "oops %s" "now") (user-error err))"#)
        .expect("eval");
    let printed = crate::emacs_core::print::print_value(&result);
    assert_eq!(printed, "(user-error \"oops now\")");
}

#[test]
fn user_error_requires_message_argument() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);
    let result = eval
        .eval_str(r#"(condition-case err (user-error) (error (car err)))"#)
        .expect("eval");
    let printed = crate::emacs_core::print::print_value(&result);
    assert_eq!(printed, "wrong-number-of-arguments");
}

#[test]
fn internal_save_selected_window_helpers_restore_selected_window() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);
    load_gnu_save_selected_window_runtime(&mut eval);
    let result = eval
        .eval_str(
            r#"(let* ((orig (selected-window))
                  (new (split-window-internal (selected-window) nil nil nil)))
             (select-window new)
             (save-selected-window
               (select-window orig)
               (eq (selected-window) orig)))"#,
        )
        .expect("eval");
    assert!(result.is_truthy(), "save-selected-window should restore");
}

#[test]
fn functionp_eval_matches_symbol_and_lambda_form_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let builtin_symbol = builtin_functionp(&mut eval, vec![Value::symbol("message")])
        .expect("functionp should accept builtin symbol");
    assert!(builtin_symbol.is_truthy());

    let quoted_lambda = Value::list(vec![
        Value::symbol("lambda"),
        Value::list(vec![Value::symbol("x")]),
        Value::symbol("x"),
    ]);
    let lambda_result = builtin_functionp(&mut eval, vec![quoted_lambda])
        .expect("functionp should accept quoted lambda list");
    assert!(lambda_result.is_truthy());

    builtin_fset(
        &mut eval,
        vec![Value::symbol("vm-functionp-alias"), quoted_lambda],
    )
    .expect("fset should accept lambda definition");
    let alias_result = builtin_functionp(&mut eval, vec![Value::symbol("vm-functionp-alias")])
        .expect("functionp should resolve symbol alias to lambda list");
    assert!(alias_result.is_truthy());

    let improper_lambda = Value::cons(Value::symbol("lambda"), Value::fixnum(1));
    let improper_result = builtin_functionp(&mut eval, vec![improper_lambda])
        .expect("functionp should accept improper lambda forms");
    assert!(improper_result.is_truthy());

    // GNU accepts interpreted closure objects, not literal (closure ...) lists.
    let quoted_closure = Value::list(vec![
        Value::symbol("closure"),
        Value::list(vec![Value::T]),
        Value::list(vec![Value::symbol("x")]),
        Value::symbol("x"),
    ]);
    let closure_result = builtin_functionp(&mut eval, vec![quoted_closure])
        .expect("functionp should reject quoted closure lists");
    assert!(closure_result.is_nil());

    let special_symbol = builtin_functionp(&mut eval, vec![Value::symbol("if")])
        .expect("functionp should reject special-form symbols");
    assert!(special_symbol.is_nil());

    let macro_symbol = builtin_functionp(&mut eval, vec![Value::symbol("when")])
        .expect("functionp should reject macro symbols");
    assert!(macro_symbol.is_nil());
    let save_match_data_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("save-match-data")])
            .expect("functionp should reject save-match-data macro symbol");
    assert!(save_match_data_symbol.is_nil());
    let save_mark_and_excursion_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("save-mark-and-excursion")])
            .expect("functionp should reject save-mark-and-excursion macro symbol");
    assert!(save_mark_and_excursion_symbol.is_nil());
    let save_window_excursion_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("save-window-excursion")])
            .expect("functionp should reject save-window-excursion macro symbol");
    assert!(save_window_excursion_symbol.is_nil());
    let save_selected_window_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("save-selected-window")])
            .expect("functionp should reject save-selected-window macro symbol");
    assert!(save_selected_window_symbol.is_nil());
    let with_local_quit_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("with-local-quit")])
            .expect("functionp should reject with-local-quit macro symbol");
    assert!(with_local_quit_symbol.is_nil());
    let with_temp_message_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("with-temp-message")])
            .expect("functionp should reject with-temp-message macro symbol");
    assert!(with_temp_message_symbol.is_nil());
    let with_demoted_errors_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("with-demoted-errors")])
            .expect("functionp should reject with-demoted-errors macro symbol");
    assert!(with_demoted_errors_symbol.is_nil());
    let bound_and_true_p_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("bound-and-true-p")])
            .expect("functionp should reject bound-and-true-p macro symbol");
    assert!(bound_and_true_p_symbol.is_nil());
    let declare_symbol = builtin_functionp(&mut eval, vec![Value::symbol("declare")])
        .expect("functionp should reject declare symbol");
    assert!(declare_symbol.is_nil());
    let inline_symbol = builtin_functionp(&mut eval, vec![Value::symbol("inline")])
        .expect("functionp should reject inline symbol");
    assert!(inline_symbol.is_nil());
    let throw_symbol = builtin_functionp(&mut eval, vec![Value::symbol("throw")])
        .expect("functionp should accept throw symbol");
    assert!(throw_symbol.is_truthy());
    for name in ["funcall", "defalias", "provide", "require"] {
        let result = builtin_functionp(&mut eval, vec![Value::symbol(name)])
            .unwrap_or_else(|_| panic!("functionp should accept {name} symbol"));
        assert!(result.is_truthy(), "expected {name} to satisfy functionp");
    }
    let macro_marker_cons = builtin_functionp(
        &mut eval,
        vec![Value::cons(Value::symbol("macro"), Value::T)],
    )
    .expect("functionp should reject dotted macro marker cons");
    assert!(macro_marker_cons.is_nil());
    let macro_marker_list = builtin_functionp(
        &mut eval,
        vec![Value::list(vec![Value::symbol("macro"), Value::T])],
    )
    .expect("functionp should reject macro marker lists");
    assert!(macro_marker_list.is_nil());

    let special_subr = builtin_functionp(&mut eval, vec![Value::subr(intern("if"))])
        .expect("functionp should reject special-form subr objects");
    assert!(special_subr.is_nil());

    eval.eval_str(r#"(autoload 'vm-test-auto-fn "vm-test-file" nil t)"#)
        .expect("autoload function should register");
    let autoload_function_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("vm-test-auto-fn")])
            .expect("functionp should recognize autoload function symbol");
    assert!(autoload_function_symbol.is_truthy());
    let autoload_function_cell = eval
        .obarray()
        .symbol_function("vm-test-auto-fn")
        .expect("autoload function cell exists");
    let autoload_function_cell = builtin_functionp(&mut eval, vec![autoload_function_cell])
        .expect("functionp should reject raw autoload function cell object");
    assert!(autoload_function_cell.is_nil());

    eval.eval_str(r#"(autoload 'vm-test-auto-macro "vm-test-file" nil nil 'macro)"#)
        .expect("autoload macro should register");
    let autoload_macro_symbol =
        builtin_functionp(&mut eval, vec![Value::symbol("vm-test-auto-macro")])
            .expect("functionp should reject autoload macro symbol");
    assert!(autoload_macro_symbol.is_nil());
}

#[test]
fn functionp_accepts_symbol_with_pos_when_enabled() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let positioned_message = eval
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("message"), Value::fixnum(42));

    eval.set_variable("symbols-with-pos-enabled", Value::T);
    let enabled = builtin_functionp(&mut eval, vec![positioned_message])
        .expect("functionp should accept symbol-with-pos when enabled");
    assert!(enabled.is_truthy());

    eval.set_variable("symbols-with-pos-enabled", Value::NIL);
    let disabled = builtin_functionp(&mut eval, vec![positioned_message])
        .expect("functionp should reject symbol-with-pos when disabled");
    assert!(disabled.is_nil());
}

#[test]
fn functionp_accepts_lambda_form_with_positioned_head_when_enabled() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let positioned_lambda = eval
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("lambda"), Value::fixnum(42));
    let lambda_form = Value::list(vec![
        positioned_lambda,
        Value::list(vec![Value::symbol("x")]),
        Value::symbol("x"),
    ]);

    eval.set_variable("symbols-with-pos-enabled", Value::T);
    let enabled = builtin_functionp(&mut eval, vec![lambda_form])
        .expect("functionp should accept positioned lambda head when enabled");
    assert!(enabled.is_truthy());

    eval.set_variable("symbols-with-pos-enabled", Value::NIL);
    let disabled = builtin_functionp(&mut eval, vec![lambda_form])
        .expect("functionp should reject positioned lambda head when disabled");
    assert!(disabled.is_nil());
}

#[test]
fn functionp_eval_resolves_t_and_keyword_symbol_designators() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let keyword = Value::keyword(":vm-functionp-keyword");
    let orig_t = builtin_symbol_function(&mut eval, vec![Value::T])
        .expect("symbol-function should read t cell");
    let orig_keyword = builtin_symbol_function(&mut eval, vec![keyword])
        .expect("symbol-function should read keyword cell");

    builtin_fset(&mut eval, vec![Value::T, Value::symbol("car")])
        .expect("fset should bind t function cell");
    builtin_fset(&mut eval, vec![keyword, Value::symbol("car")])
        .expect("fset should bind keyword function cell");

    let t_result = builtin_functionp(&mut eval, vec![Value::T]).expect("functionp should accept t");
    assert!(t_result.is_truthy());
    let keyword_result = builtin_functionp(&mut eval, vec![keyword])
        .expect("functionp should accept keyword designator");
    assert!(keyword_result.is_truthy());

    builtin_fset(&mut eval, vec![Value::T, orig_t]).expect("restore t function cell");
    builtin_fset(&mut eval, vec![keyword, orig_keyword]).expect("restore keyword function cell");
}

#[test]
fn fmakunbound_masks_builtin_special_and_evaluator_callables() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    builtin_fmakunbound(&mut eval, vec![Value::symbol("car")])
        .expect("fmakunbound should accept builtin name");
    let car_bound = builtin_fboundp(&mut eval, vec![Value::symbol("car")])
        .expect("fboundp should accept builtin name");
    assert!(car_bound.is_nil());
    let car_fn = builtin_symbol_function(&mut eval, vec![Value::symbol("car")])
        .expect("symbol-function should return nil after fmakunbound");
    assert!(car_fn.is_nil());
    let car_functionp = builtin_functionp(&mut eval, vec![Value::symbol("car")])
        .expect("functionp should accept symbol");
    assert!(car_functionp.is_nil());

    builtin_fmakunbound(&mut eval, vec![Value::symbol("if")])
        .expect("fmakunbound should accept special form name");
    let if_bound = builtin_fboundp(&mut eval, vec![Value::symbol("if")])
        .expect("fboundp should accept special form name");
    assert!(if_bound.is_nil());
    let if_fn = builtin_symbol_function(&mut eval, vec![Value::symbol("if")])
        .expect("symbol-function should return nil after fmakunbound special form");
    assert!(if_fn.is_nil());

    builtin_fmakunbound(&mut eval, vec![Value::symbol("throw")])
        .expect("fmakunbound should accept evaluator callable name");
    let throw_bound = builtin_fboundp(&mut eval, vec![Value::symbol("throw")])
        .expect("fboundp should accept evaluator callable name");
    assert!(throw_bound.is_nil());
    let throw_fn = builtin_symbol_function(&mut eval, vec![Value::symbol("throw")])
        .expect("symbol-function should return nil after fmakunbound evaluator callable");
    assert!(throw_fn.is_nil());
    let throw_functionp = builtin_functionp(&mut eval, vec![Value::symbol("throw")])
        .expect("functionp should accept symbol");
    assert!(throw_functionp.is_nil());
    for name in ["funcall", "defalias", "provide", "require"] {
        builtin_fmakunbound(&mut eval, vec![Value::symbol(name)])
            .unwrap_or_else(|_| panic!("fmakunbound should accept {name}"));
        let bound = builtin_fboundp(&mut eval, vec![Value::symbol(name)])
            .unwrap_or_else(|_| panic!("fboundp should accept {name}"));
        assert!(
            bound.is_nil(),
            "expected {name} to be unbound after fmakunbound"
        );
        let fn_cell = builtin_symbol_function(&mut eval, vec![Value::symbol(name)])
            .unwrap_or_else(|_| panic!("symbol-function should accept {name}"));
        assert!(
            fn_cell.is_nil(),
            "expected symbol-function {name} to be nil"
        );
        let functionp = builtin_functionp(&mut eval, vec![Value::symbol(name)])
            .unwrap_or_else(|_| panic!("functionp should accept {name}"));
        assert!(
            functionp.is_nil(),
            "expected functionp {name} to be nil after fmakunbound"
        );
    }
}

#[test]
fn fset_nil_clears_fboundp_for_regular_and_fallback_names() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let regular = builtin_fset(&mut eval, vec![Value::symbol("vm-fsetnil"), Value::NIL])
        .expect("fset should accept nil definition payload");
    assert!(regular.is_nil());
    let regular_bound = builtin_fboundp(&mut eval, vec![Value::symbol("vm-fsetnil")])
        .expect("fboundp should accept symbol");
    assert!(regular_bound.is_nil());
    let regular_fn = builtin_symbol_function(&mut eval, vec![Value::symbol("vm-fsetnil")])
        .expect("symbol-function should accept symbol");
    assert!(regular_fn.is_nil());

    let fallback = builtin_fset(&mut eval, vec![Value::symbol("length"), Value::NIL])
        .expect("fset should accept nil for fallback builtin name");
    assert!(fallback.is_nil());
    let fallback_bound = builtin_fboundp(&mut eval, vec![Value::symbol("length")])
        .expect("fboundp should honor explicit nil function cell");
    assert!(fallback_bound.is_nil());
}

#[test]
fn fset_nil_nil_is_allowed_and_fmakunbound_rejects_constants() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let fset_nil = builtin_fset(&mut eval, vec![Value::NIL, Value::NIL])
        .expect("fset nil nil should match GNU");
    assert!(fset_nil.is_nil());
    let nil_fn = builtin_symbol_function(&mut eval, vec![Value::NIL])
        .expect("symbol-function should read nil function cell");
    assert!(nil_fn.is_nil());

    for constant in [Value::NIL, Value::T] {
        let err = builtin_fmakunbound(&mut eval, vec![constant])
            .expect_err("fmakunbound should reject constants");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "setting-constant");
                assert_eq!(sig.data, vec![constant]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }
}

#[test]
fn func_arity_eval_resolves_symbol_designators_and_nil_cells() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let keyword = Value::keyword(":vm-func-arity-keyword");
    let vm_nil = Value::symbol("vm-func-arity-nil-cell");
    let orig_t = builtin_symbol_function(&mut eval, vec![Value::T])
        .expect("symbol-function should read t function cell");
    let orig_keyword = builtin_symbol_function(&mut eval, vec![keyword])
        .expect("symbol-function should read keyword function cell");
    let orig_vm_nil = builtin_symbol_function(&mut eval, vec![vm_nil])
        .expect("symbol-function should read symbol function cell");

    builtin_fset(&mut eval, vec![Value::T, Value::symbol("car")])
        .expect("fset should bind t function cell");
    builtin_fset(&mut eval, vec![keyword, Value::symbol("car")])
        .expect("fset should bind keyword function cell");
    builtin_fset(&mut eval, vec![vm_nil, Value::NIL])
        .expect("fset should bind explicit nil function cell");

    let t_arity = builtin_func_arity(&mut eval, vec![Value::T])
        .expect("func-arity should resolve t designator");
    match t_arity.kind() {
        ValueKind::Cons => {
            let pair_car = t_arity.cons_car();
            let pair_cdr = t_arity.cons_cdr();
            assert_eq!(pair_car, Value::fixnum(1));
            assert_eq!(pair_cdr, Value::fixnum(1));
        }
        other => panic!("expected cons arity pair, got {other:?}"),
    }

    let keyword_arity = builtin_func_arity(&mut eval, vec![keyword])
        .expect("func-arity should resolve keyword designator");
    match keyword_arity.kind() {
        ValueKind::Cons => {
            let pair_car = keyword_arity.cons_car();
            let pair_cdr = keyword_arity.cons_cdr();
            assert_eq!(pair_car, Value::fixnum(1));
            assert_eq!(pair_cdr, Value::fixnum(1));
        }
        other => panic!("expected cons arity pair, got {other:?}"),
    }

    let nil_cell_err = builtin_func_arity(&mut eval, vec![vm_nil])
        .expect_err("func-arity should signal void-function for nil function cell");
    match nil_cell_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "void-function");
            assert_eq!(sig.data, vec![vm_nil]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    builtin_fset(&mut eval, vec![Value::T, orig_t]).expect("restore t function cell");
    builtin_fset(&mut eval, vec![keyword, orig_keyword]).expect("restore keyword function cell");
    builtin_fset(&mut eval, vec![vm_nil, orig_vm_nil]).expect("restore symbol function cell");
}

#[test]
fn indirect_function_follows_t_and_keyword_alias_values() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let keyword = Value::keyword(":vm-indirect-keyword-alias");
    let t_alias = Value::symbol("vm-indirect-through-t");
    let keyword_alias = Value::symbol("vm-indirect-through-keyword");
    let orig_t = builtin_symbol_function(&mut eval, vec![Value::T])
        .expect("symbol-function should read t function cell");
    let orig_keyword = builtin_symbol_function(&mut eval, vec![keyword])
        .expect("symbol-function should read keyword function cell");
    let orig_t_alias = builtin_symbol_function(&mut eval, vec![t_alias])
        .expect("symbol-function should read alias function cell");
    let orig_keyword_alias = builtin_symbol_function(&mut eval, vec![keyword_alias])
        .expect("symbol-function should read alias function cell");

    builtin_fset(&mut eval, vec![Value::T, Value::symbol("car")])
        .expect("fset should bind t function cell");
    builtin_fset(&mut eval, vec![keyword, Value::symbol("car")])
        .expect("fset should bind keyword function cell");
    builtin_fset(&mut eval, vec![t_alias, Value::T])
        .expect("fset should bind alias to t symbol designator");
    builtin_fset(&mut eval, vec![keyword_alias, keyword])
        .expect("fset should bind alias to keyword designator");

    let resolved_t_alias = builtin_indirect_function(&mut eval, vec![t_alias])
        .expect("indirect-function should resolve alias through t");
    assert_eq!(resolved_t_alias, Value::subr(intern("car")));
    let resolved_keyword_alias = builtin_indirect_function(&mut eval, vec![keyword_alias])
        .expect("indirect-function should resolve alias through keyword");
    assert_eq!(resolved_keyword_alias, Value::subr(intern("car")));

    builtin_fset(&mut eval, vec![Value::T, orig_t]).expect("restore t function cell");
    builtin_fset(&mut eval, vec![keyword, orig_keyword]).expect("restore keyword function cell");
    builtin_fset(&mut eval, vec![t_alias, orig_t_alias]).expect("restore alias function cell");
    builtin_fset(&mut eval, vec![keyword_alias, orig_keyword_alias])
        .expect("restore alias function cell");
}

// `macrop_eval_resolves_keyword_designators' is gone with the `macrop' subr
// it called.  GNU has no DEFUN `macrop'; the keyword-designator arm is asked
// of the loaded runtime in `macrop_arms_match_gnu'
// (`builtins/lisp_only_predicates_and_aliases_test.rs'), where GNU's own
// `(fset :KEYWORD (cons 'macro ...))' probe was measured first.
// DIVERGENCES.md 148.

#[test]
fn macroexpand_runtime_environment_overrides_and_shadows_global_macros() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // Install a test macro on `vm-test-mac` that acts like `when`:
    // (macro . (lambda (cond &rest body) (list 'if cond (cons 'progn body))))
    // We use `with-temp-buffer` (a real GNU Lisp macro loaded from `subr.el`)
    // to test chained expansion after the environment lambda transforms the form.

    // Part 1: environment lambda transforms (vm-env t) -> (vm-env-result t 1),
    // which is not itself a macro, so macroexpand returns it as-is.
    let env_lambda = Value::list(vec![Value::list(vec![
        Value::symbol("vm-env"),
        Value::symbol("lambda"),
        Value::list(vec![Value::symbol("x")]),
        Value::list(vec![
            Value::symbol("list"),
            Value::list(vec![Value::symbol("quote"), Value::symbol("vm-env-result")]),
            Value::symbol("x"),
            Value::fixnum(1),
        ]),
    ])]);
    let expanded = builtin_macroexpand(
        &mut eval,
        vec![
            Value::list(vec![Value::symbol("vm-env"), Value::T]),
            env_lambda,
        ],
    )
    .expect("macroexpand should apply lambda environment expanders");
    assert_eq!(
        expanded,
        Value::list(vec![
            Value::symbol("vm-env-result"),
            Value::T,
            Value::fixnum(1),
        ])
    );

    // Part 2: shadow entry for vm-env-result suppresses expansion (trivially,
    // since vm-env-result is not a macro).  Use with-temp-buffer instead to
    // test genuine shadowing of a global macro.
    let shadow = builtin_macroexpand(
        &mut eval,
        vec![
            Value::list(vec![Value::symbol("with-temp-buffer"), Value::fixnum(1)]),
            Value::list(vec![Value::list(vec![Value::symbol("with-temp-buffer")])]),
        ],
    )
    .expect("environment shadow entries should suppress global macro expansion");
    assert_eq!(
        shadow,
        Value::list(vec![Value::symbol("with-temp-buffer"), Value::fixnum(1),])
    );
}

#[test]
fn macroexpand_runtime_environment_type_and_payload_edges_match_oracle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let atom_ignores_bad_env =
        builtin_macroexpand(&mut eval, vec![Value::symbol("x"), Value::fixnum(1)])
            .expect("non-list forms should ignore non-list environments");
    assert_eq!(atom_ignores_bad_env, Value::symbol("x"));

    let nonsymbol_head_ignores_bad_env = builtin_macroexpand(
        &mut eval,
        vec![
            Value::list(vec![
                Value::list(vec![Value::symbol("lambda")]),
                Value::fixnum(1),
            ]),
            Value::fixnum(1),
        ],
    )
    .expect("list forms without symbol heads should ignore non-list env");
    assert_eq!(
        nonsymbol_head_ignores_bad_env,
        Value::list(vec![
            Value::list(vec![Value::symbol("lambda")]),
            Value::fixnum(1)
        ])
    );

    let env_type_err = builtin_macroexpand(
        &mut eval,
        vec![
            Value::list(vec![Value::symbol("foo"), Value::fixnum(1)]),
            Value::fixnum(1),
        ],
    )
    .expect_err("symbol-headed forms should validate environment list-ness");
    match env_type_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let invalid_env_function = builtin_macroexpand(
        &mut eval,
        vec![
            Value::list(vec![Value::symbol("vm-f"), Value::fixnum(1)]),
            Value::list(vec![Value::cons(Value::symbol("vm-f"), Value::fixnum(42))]),
        ],
    )
    .expect_err("environment entries with non-callables should surface invalid-function");
    match invalid_env_function {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "invalid-function");
            assert_eq!(sig.data, vec![Value::fixnum(42)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn macroexpand_runtime_improper_lists_match_oracle_error_behavior() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let _ = eval.eval_str_each(
        r#"(fset 'vm-improper-macro
                 (cons 'macro
                       (lambda (&rest body)
                         (cons 'progn body))))"#,
    );

    let not_macro = builtin_macroexpand(
        &mut eval,
        vec![Value::cons(Value::symbol("foo"), Value::fixnum(1))],
    )
    .expect("non-macro improper forms should pass through unchanged");
    assert_eq!(
        not_macro,
        Value::cons(Value::symbol("foo"), Value::fixnum(1))
    );

    let improper_macro = builtin_macroexpand(
        &mut eval,
        vec![Value::cons(
            Value::symbol("vm-improper-macro"),
            Value::fixnum(1),
        )],
    )
    .expect_err("macro expansion should reject improper argument lists");
    match improper_macro {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn macroexpand_runtime_reinvokes_load_macros_without_suppressing_effects() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let _ = eval.eval_str_each(
        r#"(defvar vm-macroexpand-count 0)
            (fset 'vm-cache-macro
                 (cons 'macro
                       (lambda (x)
                         (setq vm-macroexpand-count
                               (1+ vm-macroexpand-count))
                         x)))"#,
    );
    eval.set_variable("load-in-progress", Value::T);
    let calls0 = eval.macro_expand_calls;

    let tail = Value::cons(Value::fixnum(1), Value::NIL);
    let form = Value::cons(Value::symbol("vm-cache-macro"), tail);

    let first = builtin_macroexpand(&mut eval, vec![form]).expect("first expansion");
    let second = builtin_macroexpand(&mut eval, vec![form]).expect("second expansion");
    assert_eq!(first, Value::fixnum(1));
    assert_eq!(second, Value::fixnum(1));
    assert_eq!(eval.macro_expand_calls - calls0, 2);
    assert_eq!(
        eval.obarray().symbol_value("vm-macroexpand-count").copied(),
        Some(Value::fixnum(2))
    );

    let equivalent_tail = Value::cons(Value::fixnum(1), Value::NIL);
    let equivalent_form = Value::cons(Value::symbol("vm-cache-macro"), equivalent_tail);
    let third =
        builtin_macroexpand(&mut eval, vec![equivalent_form]).expect("equivalent runtime form");
    assert_eq!(third, Value::fixnum(1));
    assert_eq!(eval.macro_expand_calls - calls0, 3);

    tail.set_car(Value::fixnum(2));
    let fourth = builtin_macroexpand(&mut eval, vec![form]).expect("mutated expansion");
    assert_eq!(fourth, Value::fixnum(2));
    assert_eq!(eval.macro_expand_calls - calls0, 4);
    assert_eq!(
        eval.obarray().symbol_value("vm-macroexpand-count").copied(),
        Some(Value::fixnum(4))
    );
}

#[test]
fn macroexpand_runtime_repeated_expansions_survive_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let _ = eval.eval_str_each(
        r#"(defvar vm-macroexpand-gc-count 0)
            (fset 'vm-cache-macro
                 (cons 'macro
                       (lambda (x)
                         (setq vm-macroexpand-gc-count
                               (1+ vm-macroexpand-gc-count))
                         x)))"#,
    );
    eval.set_variable("load-in-progress", Value::T);
    let calls0 = eval.macro_expand_calls;

    let tail = Value::cons(Value::fixnum(1), Value::NIL);
    let form = Value::cons(Value::symbol("vm-cache-macro"), tail);

    let first = builtin_macroexpand(&mut eval, vec![form]).expect("first expansion");
    assert_eq!(first, Value::fixnum(1));
    assert_eq!(eval.macro_expand_calls - calls0, 1);

    eval.gc_collect_exact();
    eval.set_variable("load-in-progress", Value::T);
    let second_form = Value::cons(
        Value::symbol("vm-cache-macro"),
        Value::cons(Value::fixnum(1), Value::NIL),
    );

    let second = builtin_macroexpand(&mut eval, vec![second_form]).expect("second expansion");
    assert_eq!(second, Value::fixnum(1));
    assert_eq!(eval.macro_expand_calls - calls0, 2);
    assert_eq!(
        eval.obarray()
            .symbol_value("vm-macroexpand-gc-count")
            .copied(),
        Some(Value::fixnum(2))
    );
}

#[test]
fn indirect_function_nil_and_non_symbol_behavior() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let noerror = builtin_indirect_function(
        &mut eval,
        vec![Value::symbol("definitely-not-a-function"), Value::T],
    )
    .expect("indirect-function should return nil when noerror is non-nil");
    assert!(noerror.is_nil());

    let unresolved =
        builtin_indirect_function(&mut eval, vec![Value::symbol("definitely-not-a-function")])
            .expect("indirect-function should return nil for unresolved function");
    assert!(unresolved.is_nil());

    let nil_input = builtin_indirect_function(&mut eval, vec![Value::NIL])
        .expect("indirect-function should return nil for nil input");
    assert!(nil_input.is_nil());

    let true_input = builtin_indirect_function(&mut eval, vec![Value::T])
        .expect("indirect-function should treat t as a symbol and return nil");
    assert!(true_input.is_nil());

    let keyword_input =
        builtin_indirect_function(&mut eval, vec![Value::keyword(":vm-indirect-keyword")])
            .expect("indirect-function should treat keywords as symbols and return nil");
    assert!(keyword_input.is_nil());

    let passthrough = builtin_indirect_function(&mut eval, vec![Value::fixnum(42)])
        .expect("non-symbol should be returned as-is");
    assert_eq!(passthrough, Value::fixnum(42));
}

#[test]
fn indirect_function_symbol_with_pos_follows_gnu_toggle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let positioned = eval
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("file-exists-p"), Value::fixnum(11));

    let disabled = builtin_indirect_function(&mut eval, vec![positioned])
        .expect("disabled symbol-with-pos should be returned unchanged");
    assert_eq!(disabled, positioned);

    eval.set_variable("symbols-with-pos-enabled", Value::T);
    let enabled = builtin_indirect_function(&mut eval, vec![positioned])
        .expect("enabled symbol-with-pos should resolve like GNU SYMBOLP");
    assert_eq!(enabled, Value::subr(intern("file-exists-p")));
}

#[test]
fn indirect_function_rejects_overflow_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let err = builtin_indirect_function(
        &mut eval,
        vec![Value::symbol("ignore"), Value::NIL, Value::NIL],
    )
    .expect_err("indirect-function should reject more than two arguments");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn indirect_function_resolves_deep_alias_chain_without_depth_cutoff() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let depth = 120;

    for idx in 0..depth {
        let name = format!("vm-test-deep-alias-{idx}");
        let target = if idx == depth - 1 {
            Value::symbol("car")
        } else {
            Value::symbol(format!("vm-test-deep-alias-{}", idx + 1))
        };
        eval.obarray_mut().set_symbol_function(&name, target);
    }

    let resolved =
        builtin_indirect_function(&mut eval, vec![Value::symbol("vm-test-deep-alias-0")])
            .expect("indirect-function should resolve deep alias chains");
    assert_eq!(resolved, Value::subr(intern("car")));
}

#[test]
fn fset_rejects_self_alias_cycle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let err = builtin_fset(
        &mut eval,
        vec![
            Value::symbol("vm-test-fset-cycle-self"),
            Value::symbol("vm-test-fset-cycle-self"),
        ],
    )
    .expect_err("fset should reject self-referential alias cycles");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "cyclic-function-indirection");
            assert_eq!(sig.data, vec![Value::symbol("vm-test-fset-cycle-self")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let unresolved =
        builtin_indirect_function(&mut eval, vec![Value::symbol("vm-test-fset-cycle-self")])
            .expect("indirect-function should still resolve");
    assert!(unresolved.is_nil());
}

#[test]
fn fset_rejects_two_node_alias_cycle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let first = builtin_fset(
        &mut eval,
        vec![
            Value::symbol("vm-test-fset-cycle-a"),
            Value::symbol("vm-test-fset-cycle-b"),
        ],
    )
    .expect("first alias should be accepted");
    assert_eq!(first, Value::symbol("vm-test-fset-cycle-b"));

    let err = builtin_fset(
        &mut eval,
        vec![
            Value::symbol("vm-test-fset-cycle-b"),
            Value::symbol("vm-test-fset-cycle-a"),
        ],
    )
    .expect_err("fset should reject second edge that closes alias cycle");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "cyclic-function-indirection");
            assert_eq!(sig.data, vec![Value::symbol("vm-test-fset-cycle-b")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn fset_rejects_keyword_and_t_alias_cycles() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let first = builtin_fset(
        &mut eval,
        vec![Value::keyword(":vmk2"), Value::keyword(":vmk3")],
    )
    .expect("first keyword alias should be accepted");
    assert_eq!(first, Value::keyword(":vmk3"));

    let keyword_cycle = builtin_fset(
        &mut eval,
        vec![Value::keyword(":vmk3"), Value::keyword(":vmk2")],
    )
    .expect_err("second keyword edge should close cycle");
    match keyword_cycle {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "cyclic-function-indirection");
            assert_eq!(sig.data, vec![Value::symbol(":vmk3")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    builtin_fset(&mut eval, vec![Value::T, Value::keyword(":vmk")])
        .expect("fset should allow binding t to keyword alias");

    let t_cycle = builtin_fset(&mut eval, vec![Value::keyword(":vmk"), Value::T])
        .expect_err("keyword->t edge should be rejected when t->keyword exists");
    match t_cycle {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "cyclic-function-indirection");
            assert_eq!(sig.data, vec![Value::symbol(":vmk")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn fset_nil_signals_setting_constant() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let err = builtin_fset(&mut eval, vec![Value::NIL, Value::symbol("car")])
        .expect_err("fset should reject writing nil's function cell");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "setting-constant");
            assert_eq!(sig.data, vec![Value::symbol("nil")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn fset_t_accepts_symbol_cell_updates() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let result = builtin_fset(&mut eval, vec![Value::T, Value::symbol("car")])
        .expect("fset should allow writing t's function cell");
    assert_eq!(result, Value::symbol("car"));

    let resolved = builtin_indirect_function(&mut eval, vec![Value::T])
        .expect("indirect-function should resolve t after fset");
    assert_eq!(resolved, Value::subr(intern("car")));
}

#[test]
fn keyword_symbols_are_bound_and_special_constants() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let keyword = Value::keyword(":vm-bound-keyword");

    let bound = builtin_boundp(&mut eval, vec![keyword]).expect("boundp should accept keyword");
    assert!(bound.is_truthy());

    let default_bound = super::super::data::default_boundp(&mut eval, vec![keyword])
        .expect("default-boundp should accept keyword");
    assert!(default_bound.is_truthy());

    let special = builtin_special_variable_p(&mut eval, vec![keyword])
        .expect("special-variable-p should accept keyword");
    assert!(special.is_truthy());
}

#[test]
fn boundp_and_symbol_value_see_dynamic_and_current_buffer_local_bindings() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    eval.obarray_mut().make_special_id(intern("vm-bound-dyn"));
    eval.try_specbind(intern("vm-bound-dyn"), Value::fixnum(9))
        .expect("dynamic binding");

    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.set_buffer_local_binding_by_id(current, intern("vm-bound-buf"), Value::fixnum(7))
        .expect("buffer-local binding");

    let dyn_bound = builtin_boundp(&mut eval, vec![Value::symbol("vm-bound-dyn")])
        .expect("boundp should see dynamic binding");
    assert!(dyn_bound.is_truthy());
    let dyn_default =
        super::super::data::default_boundp(&mut eval, vec![Value::symbol("vm-bound-dyn")])
            .expect("default-boundp should see specbind binding in obarray");
    assert!(dyn_default.is_truthy());
    let dyn_value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-bound-dyn")])
        .expect("symbol-value should see dynamic binding");
    assert_eq!(dyn_value, Value::fixnum(9));

    let buf_bound = builtin_boundp(&mut eval, vec![Value::symbol("vm-bound-buf")])
        .expect("boundp should see current buffer-local binding");
    assert!(buf_bound.is_truthy());
    let buf_default =
        super::super::data::default_boundp(&mut eval, vec![Value::symbol("vm-bound-buf")])
            .expect("default-boundp should ignore current buffer-local binding");
    assert!(buf_default.is_nil());
    let buf_value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-bound-buf")])
        .expect("symbol-value should read current buffer-local binding");
    assert_eq!(buf_value, Value::fixnum(7));
}

#[test]
fn boundp_buffer_local_lookup_uses_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let uninterned = crate::emacs_core::intern::intern_uninterned("vm-bound-shadow");
    let interned = intern("vm-bound-shadow");
    let current = eval.buffers.current_buffer_id().expect("current buffer");

    eval.obarray_mut()
        .set_symbol_value_id(uninterned, Value::fixnum(11));
    eval.set_buffer_local_binding_by_id(current, interned, Value::fixnum(22))
        .expect("interned buffer-local binding");

    let bound = builtin_boundp(&mut eval, vec![Value::from_sym_id(uninterned)])
        .expect("boundp should accept uninterned symbol");
    assert!(bound.is_truthy());

    let value = builtin_symbol_value(&mut eval, vec![Value::from_sym_id(uninterned)])
        .expect("symbol-value should read uninterned value cell");
    assert_eq!(value, Value::fixnum(11));
}

#[test]
fn defvaralias_and_indirect_variable_follow_runtime_aliases() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let aliased = builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-new"),
            Value::symbol("vm-defvaralias-old"),
            Value::string("vm-doc"),
        ],
    )
    .expect("defvaralias should succeed");
    assert_eq!(aliased, Value::symbol("vm-defvaralias-old"));

    let doc = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-new"),
            Value::symbol("variable-documentation"),
        ],
    )
    .expect("get should return variable doc");
    assert_eq!(doc, Value::string("vm-doc"));

    let direct = builtin_indirect_variable(&mut eval, vec![Value::symbol("vm-defvaralias-new")])
        .expect("indirect-variable should resolve aliases");
    assert_eq!(direct, Value::symbol("vm-defvaralias-old"));

    let special_new =
        builtin_special_variable_p(&mut eval, vec![Value::symbol("vm-defvaralias-new")])
            .expect("special-variable-p should accept alias");
    assert!(special_new.is_truthy());
    let special_old =
        builtin_special_variable_p(&mut eval, vec![Value::symbol("vm-defvaralias-old")])
            .expect("special-variable-p should mark target special");
    assert!(special_old.is_truthy());

    let set_value = builtin_set(
        &mut eval,
        vec![Value::symbol("vm-defvaralias-new"), Value::fixnum(7)],
    )
    .expect("set should assign through aliases");
    assert_eq!(set_value, Value::fixnum(7));
    let old_value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-defvaralias-old")])
        .expect("symbol-value should read aliased target");
    assert_eq!(old_value, Value::fixnum(7));

    builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-new"),
            Value::symbol("vm-defvaralias-old"),
        ],
    )
    .expect("defvaralias without doc should clear variable-documentation");
    let cleared_doc = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-new"),
            Value::symbol("variable-documentation"),
        ],
    )
    .expect("get should read cleared documentation");
    assert!(cleared_doc.is_nil());

    let unbound = builtin_makunbound(&mut eval, vec![Value::symbol("vm-defvaralias-new")])
        .expect("makunbound should clear target through alias");
    assert_eq!(unbound, Value::symbol("vm-defvaralias-new"));
    let bound_old = builtin_boundp(&mut eval, vec![Value::symbol("vm-defvaralias-old")])
        .expect("boundp should read aliased target");
    assert!(bound_old.is_nil());
}

#[test]
fn variable_watchers_observe_set_setq_and_makunbound() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    install_variable_watcher_probe(&mut eval, "vm-watcher-probe");

    super::super::advice::builtin_add_variable_watcher(
        &mut eval,
        vec![
            Value::symbol("vm-watcher-target"),
            Value::symbol("vm-watcher-probe"),
        ],
    )
    .expect("add-variable-watcher should register callback");

    builtin_set(
        &mut eval,
        vec![Value::symbol("vm-watcher-target"), Value::fixnum(7)],
    )
    .expect("set should trigger watcher");
    let set_op = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-op")])
        .expect("watcher should record operation");
    let set_val = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-value")])
        .expect("watcher should record value");
    assert_eq!(set_op, Value::symbol("set"));
    assert_eq!(set_val, Value::fixnum(7));

    eval.eval_str("(setq vm-watcher-target 11)")
        .expect("setq should trigger watcher");
    let setq_op = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-op")])
        .expect("watcher should record setq operation");
    let setq_val = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-value")])
        .expect("watcher should record setq value");
    assert_eq!(setq_op, Value::symbol("set"));
    assert_eq!(setq_val, Value::fixnum(11));

    builtin_makunbound(&mut eval, vec![Value::symbol("vm-watcher-target")])
        .expect("makunbound should trigger watcher");
    let unbind_op = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-op")])
        .expect("watcher should record makunbound operation");
    let unbind_val = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-value")])
        .expect("watcher should record makunbound value");
    assert_eq!(unbind_op, Value::symbol("makunbound"));
    assert!(unbind_val.is_nil());
}

#[test]
fn variable_watchers_observe_set_default_toplevel_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    install_variable_watcher_probe(&mut eval, "vm-watcher-probe");

    super::super::advice::builtin_add_variable_watcher(
        &mut eval,
        vec![
            Value::symbol("vm-watcher-default-target"),
            Value::symbol("vm-watcher-probe"),
        ],
    )
    .expect("add-variable-watcher should register callback");

    crate::emacs_core::eval::set_default_toplevel_value(
        &mut eval,
        vec![
            Value::symbol("vm-watcher-default-target"),
            Value::fixnum(23),
        ],
    )
    .expect("set-default-toplevel-value should trigger watcher");
    let op = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-op")])
        .expect("watcher should record operation");
    let val = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-value")])
        .expect("watcher should record value");
    assert_eq!(op, Value::symbol("set"));
    assert_eq!(val, Value::fixnum(23));
}

#[test]
fn defvaralias_triggers_variable_watchers_and_clears_alias_entry() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    install_variable_watcher_probe(&mut eval, "vm-defvaralias-watch-probe");

    super::super::advice::builtin_add_variable_watcher(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-watch-new"),
            Value::symbol("vm-defvaralias-watch-probe"),
        ],
    )
    .expect("add-variable-watcher should register callback");

    builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-watch-new"),
            Value::symbol("vm-defvaralias-watch-old"),
        ],
    )
    .expect("defvaralias should trigger watcher callback");

    let symbol = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-symbol")])
        .expect("watcher should record watched symbol");
    let op = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-op")])
        .expect("watcher should record defvaralias operation");
    let value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-value")])
        .expect("watcher should record aliased target");
    assert_eq!(symbol, Value::symbol("vm-defvaralias-watch-new"));
    assert_eq!(op, Value::symbol("defvaralias"));
    assert_eq!(value, Value::symbol("vm-defvaralias-watch-old"));

    let remaining = super::super::advice::builtin_get_variable_watchers(
        &mut eval,
        vec![Value::symbol("vm-defvaralias-watch-new")],
    )
    .expect("get-variable-watchers should return alias watcher list");
    assert!(remaining.is_nil());
}

#[test]
fn defvaralias_raw_plist_error_preserves_gnu_watcher_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    install_variable_watcher_probe(&mut eval, "vm-defvaralias-watch-probe");

    super::super::advice::builtin_add_variable_watcher(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-watch-bad"),
            Value::symbol("vm-defvaralias-watch-probe"),
        ],
    )
    .expect("add-variable-watcher should register callback");

    builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-defvaralias-watch-bad"), Value::fixnum(1)],
    )
    .expect("setplist should install malformed raw plist");

    let err = builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-watch-bad"),
            Value::symbol("vm-defvaralias-watch-target"),
        ],
    )
    .expect_err("defvaralias should preserve plistp error");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("plistp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let op = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-op")])
        .expect("watcher should run before variable-documentation put");
    let value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-value")])
        .expect("watcher should record target before plistp error");
    assert_eq!(op, Value::symbol("defvaralias"));
    assert_eq!(value, Value::symbol("vm-defvaralias-watch-target"));

    let target =
        builtin_indirect_variable(&mut eval, vec![Value::symbol("vm-defvaralias-watch-bad")])
            .expect("GNU installs the alias before the failing documentation put");
    assert_eq!(target, Value::symbol("vm-defvaralias-watch-target"));
}

#[test]
fn defvaralias_repoint_notifies_previous_alias_target_watchers() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    install_variable_watcher_probe(&mut eval, "vm-defvaralias-repoint-watch");

    builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-repoint-alias"),
            Value::symbol("vm-defvaralias-repoint-old"),
        ],
    )
    .expect("first defvaralias should install initial alias");

    super::super::advice::builtin_add_variable_watcher(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-repoint-alias"),
            Value::symbol("vm-defvaralias-repoint-watch"),
        ],
    )
    .expect("add-variable-watcher should resolve alias to old target");

    builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-repoint-alias"),
            Value::symbol("vm-defvaralias-repoint-new"),
        ],
    )
    .expect("second defvaralias should trigger previous-target watcher");

    let symbol = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-symbol")])
        .expect("watcher should record previous alias target");
    let op = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-op")])
        .expect("watcher should record operation");
    let value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-watcher-last-value")])
        .expect("watcher should record new alias target");
    assert_eq!(symbol, Value::symbol("vm-defvaralias-repoint-old"));
    assert_eq!(op, Value::symbol("defvaralias"));
    assert_eq!(value, Value::symbol("vm-defvaralias-repoint-new"));
}

#[test]
fn defvaralias_rejects_invalid_inputs_and_cycles() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let constant_err = builtin_defvaralias(
        &mut eval,
        vec![Value::symbol("nil"), Value::symbol("vm-defvaralias-x")],
    )
    .expect_err("defvaralias should reject constant aliases");
    match constant_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Cannot make a constant an alias: nil")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let type_err = builtin_defvaralias(
        &mut eval,
        vec![Value::symbol("vm-defvaralias-bad"), Value::fixnum(1)],
    )
    .expect_err("defvaralias should validate OLD-BASE");
    match type_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-a"),
            Value::symbol("vm-defvaralias-b"),
        ],
    )
    .expect("first alias edge should succeed");
    let cycle_err = builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-b"),
            Value::symbol("vm-defvaralias-a"),
        ],
    )
    .expect_err("second alias edge should be rejected as a cycle");
    match cycle_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "cyclic-variable-indirection");
            assert_eq!(sig.data, vec![Value::symbol("vm-defvaralias-a")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn setplist_runtime_controls_get_put_and_symbol_plist_edges() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let initial_plist = Value::list(vec![
        Value::symbol("a"),
        Value::fixnum(1),
        Value::symbol("b"),
        Value::fixnum(2),
    ]);
    let stored = builtin_setplist(&mut eval, vec![Value::symbol("vm-setplist"), initial_plist])
        .expect("setplist should store plist values");
    assert_eq!(stored, initial_plist);

    let read_plist = builtin_symbol_plist_fn(&mut eval, vec![Value::symbol("vm-setplist")])
        .expect("symbol-plist should return stored raw plist");
    assert_eq!(
        read_plist,
        Value::list(vec![
            Value::symbol("a"),
            Value::fixnum(1),
            Value::symbol("b"),
            Value::fixnum(2),
        ])
    );

    let lookup = builtin_get(
        &mut eval,
        vec![Value::symbol("vm-setplist"), Value::symbol("a")],
    )
    .expect("get should read entries from raw plist");
    assert_eq!(lookup, Value::fixnum(1));

    let put = builtin_put(
        &mut eval,
        vec![
            Value::symbol("vm-setplist"),
            Value::symbol("a"),
            Value::fixnum(5),
        ],
    )
    .expect("put should update raw plist entries");
    assert_eq!(put, Value::fixnum(5));
    let updated = builtin_symbol_plist_fn(&mut eval, vec![Value::symbol("vm-setplist")])
        .expect("symbol-plist should reflect updated plist values");
    assert_eq!(
        updated,
        Value::list(vec![
            Value::symbol("a"),
            Value::fixnum(5),
            Value::symbol("b"),
            Value::fixnum(2),
        ])
    );

    builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-setplist"), Value::fixnum(1)],
    )
    .expect("setplist should accept non-list plist values");
    let non_list = builtin_symbol_plist_fn(&mut eval, vec![Value::symbol("vm-setplist")])
        .expect("symbol-plist should return raw non-list values");
    assert_eq!(non_list, Value::fixnum(1));

    let missing = builtin_get(
        &mut eval,
        vec![Value::symbol("vm-setplist"), Value::symbol("a")],
    )
    .expect("get should treat non-list plist as missing keys");
    assert!(missing.is_nil());

    let put_err = builtin_put(
        &mut eval,
        vec![
            Value::symbol("vm-setplist"),
            Value::symbol("a"),
            Value::fixnum(8),
        ],
    )
    .expect_err("put should fail on non-plist raw values");
    match put_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("plistp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn put_promotes_symbol_properties_to_live_raw_plists() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let sym = Value::symbol("vm-live-plist");

    builtin_put(
        &mut eval,
        vec![sym, Value::symbol("type"), Value::symbol("float")],
    )
    .expect("first put should succeed");
    builtin_put(
        &mut eval,
        vec![sym, Value::symbol("doc"), Value::string("A z value")],
    )
    .expect("second put should succeed");

    let plist = builtin_symbol_plist_fn(&mut eval, vec![sym])
        .expect("symbol-plist should return a live plist object");

    builtin_put(&mut eval, vec![sym, Value::symbol("type"), Value::NIL])
        .expect("put should mutate the live plist in place");
    builtin_put(&mut eval, vec![sym, Value::symbol("doc"), Value::NIL])
        .expect("put should mutate the live plist in place");

    assert_eq!(
        plist,
        Value::list(vec![
            Value::symbol("type"),
            Value::NIL,
            Value::symbol("doc"),
            Value::NIL,
        ])
    );
}

#[test]
fn put_string_property_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.gc_stress = true;

    let result = eval.eval_str(
        "(progn
           (put 'vm-put-string-property 'custom-version \"29.1\")
           (get 'vm-put-string-property 'custom-version))",
    );
    assert_eq!(format_eval_result(&result), "OK \"29.1\"");
}

#[test]
fn register_code_conversion_map_publishes_symbol_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let map = Value::vector(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);

    let map_id = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![Value::symbol("vm-ccl-map-prop"), map],
    )
    .expect("register-code-conversion-map should dispatch")
    .expect("register-code-conversion-map should succeed");
    let map_id_value = match map_id.kind() {
        ValueKind::Fixnum(id) => {
            assert!(id >= 0);
            Value::fixnum(id)
        }
        other => panic!("expected integer map id, got {other:?}"),
    };

    let published_map = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-ccl-map-prop"),
            Value::symbol("code-conversion-map"),
        ],
    )
    .expect("get should read published conversion map");
    assert_eq!(published_map, map);

    let published_id = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-ccl-map-prop"),
            Value::symbol("code-conversion-map-id"),
        ],
    )
    .expect("get should read published conversion map id");
    assert_eq!(published_id, map_id_value);

    let sym_value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-ccl-map-prop")])
        .expect_err("register-code-conversion-map should not bind symbol value");
    match sym_value {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "void-variable");
            assert_eq!(sig.data, vec![Value::symbol("vm-ccl-map-prop")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn register_ccl_program_publishes_symbol_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let program = Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]);

    let program_id = dispatch_builtin(
        &mut eval,
        "register-ccl-program",
        vec![Value::symbol("vm-ccl-program-prop"), program],
    )
    .expect("register-ccl-program should dispatch")
    .expect("register-ccl-program should succeed");
    let program_id_value = match program_id.kind() {
        ValueKind::Fixnum(id) => {
            assert!(id > 0);
            Value::fixnum(id)
        }
        other => panic!("expected integer program id, got {other:?}"),
    };

    let published_id = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-ccl-program-prop"),
            Value::symbol("ccl-program-idx"),
        ],
    )
    .expect("get should read published CCL program id");
    assert_eq!(published_id, program_id_value);

    let unpublished_program = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-ccl-program-prop"),
            Value::symbol("ccl-program"),
        ],
    )
    .expect("get should return nil for ccl-program property");
    assert_eq!(unpublished_program, Value::NIL);

    let sym_value = builtin_symbol_value(&mut eval, vec![Value::symbol("vm-ccl-program-prop")])
        .expect_err("register-ccl-program should not bind symbol value");
    match sym_value {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "void-variable");
            assert_eq!(sig.data, vec![Value::symbol("vm-ccl-program-prop")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn ccl_symbol_designators_follow_plist_idx_gates() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let _ = builtin_put(
        &mut eval,
        vec![
            Value::symbol("vm-ccl-manual-programp"),
            Value::symbol("ccl-program-idx"),
            Value::fixnum(1),
        ],
    )
    .expect("put should seed ccl-program-idx");
    let manual_programp = dispatch_builtin(
        &mut eval,
        "ccl-program-p",
        vec![Value::symbol("vm-ccl-manual-programp")],
    )
    .expect("ccl-program-p should dispatch")
    .expect("ccl-program-p should evaluate symbol plist idx");
    assert_eq!(manual_programp, Value::T);

    let first_id = dispatch_builtin(
        &mut eval,
        "register-ccl-program",
        vec![
            Value::symbol("vm-ccl-plist-gate"),
            Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        ],
    )
    .expect("initial register-ccl-program should dispatch")
    .expect("initial register-ccl-program should succeed");

    let _ = builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-ccl-plist-gate"), Value::NIL],
    )
    .expect("setplist should clear symbol plist");
    let second_id = dispatch_builtin(
        &mut eval,
        "register-ccl-program",
        vec![
            Value::symbol("vm-ccl-plist-gate"),
            Value::vector(vec![
                Value::fixnum(10),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
            ]),
        ],
    )
    .expect("re-register should dispatch")
    .expect("re-register should keep existing id");
    assert_eq!(second_id, first_id);

    let missing_idx = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-ccl-plist-gate"),
            Value::symbol("ccl-program-idx"),
        ],
    )
    .expect("get should read plist gate");
    assert_eq!(missing_idx, Value::NIL);

    let gated_programp = dispatch_builtin(
        &mut eval,
        "ccl-program-p",
        vec![Value::symbol("vm-ccl-plist-gate")],
    )
    .expect("ccl-program-p should dispatch")
    .expect("ccl-program-p should gate on plist idx");
    assert_eq!(gated_programp, Value::NIL);

    let execute_err = dispatch_builtin(
        &mut eval,
        "ccl-execute",
        vec![
            Value::symbol("vm-ccl-plist-gate"),
            Value::vector(vec![
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
            ]),
        ],
    )
    .expect("ccl-execute should dispatch")
    .expect_err("ccl-execute should treat gated symbol as invalid program");
    match execute_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Invalid CCL program")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let execute_on_string_err = dispatch_builtin(
        &mut eval,
        "ccl-execute-on-string",
        vec![
            Value::symbol("vm-ccl-plist-gate"),
            Value::vector(vec![
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
            ]),
            Value::string("abc"),
        ],
    )
    .expect("ccl-execute-on-string should dispatch")
    .expect_err("ccl-execute-on-string should treat gated symbol as invalid program");
    match execute_on_string_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Invalid CCL program")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let _ = builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-ccl-plist-gate"), Value::fixnum(1)],
    )
    .expect("setplist should allow malformed plist");
    let malformed_reregister = dispatch_builtin(
        &mut eval,
        "register-ccl-program",
        vec![
            Value::symbol("vm-ccl-plist-gate"),
            Value::vector(vec![
                Value::fixnum(10),
                Value::fixnum(0),
                Value::fixnum(0),
                Value::fixnum(0),
            ]),
        ],
    )
    .expect("malformed re-register should dispatch")
    .expect("malformed re-register should return existing id");
    assert_eq!(malformed_reregister, first_id);
}

#[test]
fn register_code_conversion_map_existing_symbol_plist_edges() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let first_id = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![
            Value::symbol("vm-map-plist-edge"),
            Value::vector(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]),
        ],
    )
    .expect("register-code-conversion-map should dispatch")
    .expect("initial register-code-conversion-map should succeed");
    assert_eq!(first_id, Value::fixnum(0));

    let _ = builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-map-plist-edge"), Value::NIL],
    )
    .expect("setplist should clear plist");

    let second_id = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![
            Value::symbol("vm-map-plist-edge"),
            Value::vector(vec![Value::fixnum(4), Value::fixnum(5), Value::fixnum(6)]),
        ],
    )
    .expect("register-code-conversion-map should dispatch after plist clear")
    .expect("register-code-conversion-map should keep id after plist clear");
    assert_eq!(second_id, first_id);

    let republished_map = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-map-plist-edge"),
            Value::symbol("code-conversion-map"),
        ],
    )
    .expect("get should read republished map");
    assert_eq!(
        republished_map,
        Value::vector(vec![Value::fixnum(4), Value::fixnum(5), Value::fixnum(6)])
    );
    let republished_id = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-map-plist-edge"),
            Value::symbol("code-conversion-map-id"),
        ],
    )
    .expect("get should read republished id");
    assert_eq!(republished_id, first_id);

    let _ = builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-map-plist-edge"), Value::fixnum(1)],
    )
    .expect("setplist should seed malformed plist");
    let malformed = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![
            Value::symbol("vm-map-plist-edge"),
            Value::vector(vec![Value::fixnum(7), Value::fixnum(8), Value::fixnum(9)]),
        ],
    )
    .expect("register-code-conversion-map malformed path should dispatch")
    .expect_err("malformed plist should preserve plistp error");
    match malformed {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("plistp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let hidden_id = builtin_get(
        &mut eval,
        vec![
            Value::symbol("vm-map-plist-edge"),
            Value::symbol("code-conversion-map-id"),
        ],
    )
    .expect("get should read hidden id after malformed plist");
    assert_eq!(hidden_id, Value::NIL);

    let next_id = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![
            Value::symbol("vm-map-plist-edge-next"),
            Value::vector(vec![Value::fixnum(9), Value::fixnum(8), Value::fixnum(7)]),
        ],
    )
    .expect("register-code-conversion-map next should dispatch")
    .expect("register-code-conversion-map next should succeed");
    assert_eq!(next_id, Value::fixnum(1));
}

#[test]
fn ccl_registration_plist_errors_preserve_oracle_id_side_effects() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    let baseline_program_id = dispatch_builtin(
        &mut eval,
        "register-ccl-program",
        vec![
            Value::symbol("vm-ccl-program-id-baseline"),
            Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        ],
    )
    .expect("register-ccl-program baseline should dispatch")
    .expect("register-ccl-program baseline should succeed")
    .as_int()
    .expect("baseline program id should be integer");

    builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-ccl-program-id-bad"), Value::fixnum(1)],
    )
    .expect("setplist should seed malformed plist");
    let program_err = dispatch_builtin(
        &mut eval,
        "register-ccl-program",
        vec![
            Value::symbol("vm-ccl-program-id-bad"),
            Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        ],
    )
    .expect("register-ccl-program error path should dispatch")
    .expect_err("register-ccl-program should fail on malformed plist");
    match program_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("plistp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
    let bad_program_designator = dispatch_builtin(
        &mut eval,
        "ccl-program-p",
        vec![Value::symbol("vm-ccl-program-id-bad")],
    )
    .expect("ccl-program-p should dispatch")
    .expect("ccl-program-p should return predicate value");
    assert_eq!(bad_program_designator, Value::NIL);

    let next_program_id = dispatch_builtin(
        &mut eval,
        "register-ccl-program",
        vec![
            Value::symbol("vm-ccl-program-id-next"),
            Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        ],
    )
    .expect("register-ccl-program next should dispatch")
    .expect("register-ccl-program next should succeed")
    .as_int()
    .expect("next program id should be integer");
    assert_eq!(next_program_id, baseline_program_id + 2);

    let baseline_map_id = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![
            Value::symbol("vm-ccl-map-id-baseline"),
            Value::vector(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]),
        ],
    )
    .expect("register-code-conversion-map baseline should dispatch")
    .expect("register-code-conversion-map baseline should succeed")
    .as_int()
    .expect("baseline map id should be integer");

    builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-ccl-map-id-bad"), Value::fixnum(1)],
    )
    .expect("setplist should seed malformed plist");
    let map_err = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![
            Value::symbol("vm-ccl-map-id-bad"),
            Value::vector(vec![Value::fixnum(4), Value::fixnum(5), Value::fixnum(6)]),
        ],
    )
    .expect("register-code-conversion-map error path should dispatch")
    .expect_err("register-code-conversion-map should fail on malformed plist");
    match map_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("plistp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let next_map_id = dispatch_builtin(
        &mut eval,
        "register-code-conversion-map",
        vec![
            Value::symbol("vm-ccl-map-id-next"),
            Value::vector(vec![Value::fixnum(7), Value::fixnum(8), Value::fixnum(9)]),
        ],
    )
    .expect("register-code-conversion-map next should dispatch")
    .expect("register-code-conversion-map next should succeed")
    .as_int()
    .expect("next map id should be integer");
    assert_eq!(next_map_id, baseline_map_id + 1);
}

#[test]
fn variable_alias_to_constant_reports_alias_in_setting_constant_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-alias-constant"),
            Value::symbol("nil"),
            Value::NIL,
        ],
    )
    .expect("defvaralias should allow aliasing to nil");

    let set_err = builtin_set(
        &mut eval,
        vec![Value::symbol("vm-alias-constant"), Value::fixnum(1)],
    )
    .expect_err("set should reject writes through nil aliases");
    match set_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "setting-constant");
            assert_eq!(sig.data, vec![Value::symbol("vm-alias-constant")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let default_err = crate::emacs_core::eval::set_default_toplevel_value(
        &mut eval,
        vec![Value::symbol("vm-alias-constant"), Value::fixnum(1)],
    )
    .expect_err("set-default-toplevel-value should reject nil aliases");
    match default_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "setting-constant");
            assert_eq!(sig.data, vec![Value::symbol("vm-alias-constant")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let unbind_err = builtin_makunbound(&mut eval, vec![Value::symbol("vm-alias-constant")])
        .expect_err("makunbound should reject nil aliases");
    match unbind_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "setting-constant");
            assert_eq!(sig.data, vec![Value::symbol("vm-alias-constant")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn set_allows_keyword_self_assignment_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let keyword = Value::keyword(":vm-set-keyword");

    let direct = builtin_set(&mut eval, vec![keyword, keyword])
        .expect("set should allow keyword self-assignment");
    assert_eq!(direct, keyword);

    builtin_defvaralias(
        &mut eval,
        vec![Value::symbol("vm-set-keyword-alias"), keyword, Value::NIL],
    )
    .expect("defvaralias should allow keyword targets");

    let aliased = builtin_set(
        &mut eval,
        vec![Value::symbol("vm-set-keyword-alias"), keyword],
    )
    .expect("set should allow alias-to-keyword self-assignment");
    assert_eq!(aliased, keyword);

    let default_direct = crate::emacs_core::data::set_default(&mut eval, vec![keyword, keyword])
        .expect("set-default should allow keyword self-assignment");
    assert_eq!(default_direct, keyword);

    let toplevel_direct =
        crate::emacs_core::eval::set_default_toplevel_value(&mut eval, vec![keyword, keyword])
            .expect("set-default-toplevel-value should allow keyword self-assignment");
    assert!(toplevel_direct.is_nil());

    let changed_default =
        crate::emacs_core::data::set_default(&mut eval, vec![keyword, Value::symbol("changed")])
            .expect_err("set-default should reject non-self keyword assignment");
    match changed_default {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "setting-constant");
            assert_eq!(sig.data, vec![keyword]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let changed_toplevel = crate::emacs_core::eval::set_default_toplevel_value(
        &mut eval,
        vec![keyword, Value::symbol("changed")],
    )
    .expect_err("set-default-toplevel-value should reject non-self keyword assignment");
    match changed_toplevel {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "setting-constant");
            assert_eq!(sig.data, vec![keyword]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn defvaralias_raises_plistp_errors_when_symbol_plist_is_non_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_setplist(
        &mut eval,
        vec![Value::symbol("vm-defvaralias-bad-plist"), Value::fixnum(1)],
    )
    .expect("setplist should seed malformed symbol plist value");

    let err = builtin_defvaralias(
        &mut eval,
        vec![
            Value::symbol("vm-defvaralias-bad-plist"),
            Value::symbol("vm-defvaralias-target"),
            Value::string("doc"),
        ],
    )
    .expect_err("defvaralias should preserve put-style plistp failures");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("plistp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let unresolved =
        builtin_indirect_variable(&mut eval, vec![Value::symbol("vm-defvaralias-bad-plist")])
            .expect("failed defvaralias should still install alias edges");
    assert_eq!(unresolved, Value::symbol("vm-defvaralias-target"));
}

#[test]
fn get_byte_string_semantics_match_oracle_edges() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(0), Value::string("abc")]).unwrap(),
        Value::fixnum(97)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(1), Value::string("abc")]).unwrap(),
        Value::fixnum(98)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::NIL, Value::string("abc")]).unwrap(),
        Value::fixnum(97)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::NIL, Value::string("")]).unwrap(),
        Value::fixnum(0)
    );

    let out_of_range =
        builtin_get_byte(&mut eval, vec![Value::fixnum(3), Value::string("abc")]).unwrap_err();
    match out_of_range {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::string("abc"), Value::fixnum(3)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let empty_explicit_position =
        builtin_get_byte(&mut eval, vec![Value::fixnum(0), Value::string("")]).unwrap_err();
    match empty_explicit_position {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::string(""), Value::fixnum(0)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let negative =
        builtin_get_byte(&mut eval, vec![Value::fixnum(-1), Value::string("abc")]).unwrap_err();
    match negative {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("wholenump"), Value::fixnum(-1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let non_ascii = builtin_get_byte(&mut eval, vec![Value::fixnum(0), Value::string("é")])
        .expect_err("multibyte non-byte8 should signal");
    match non_ascii {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Not an ASCII nor an 8-bit character: 233")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn get_byte_buffer_semantics_match_oracle_edges() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_erase_buffer(&mut eval, vec![]).unwrap();
    builtin_insert(&mut eval, vec![Value::string("abc")]).unwrap();

    assert_eq!(
        builtin_get_byte(&mut eval, vec![]).unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(97)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(98)
    );
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(2),
        false,
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![marker]).unwrap(),
        Value::fixnum(98)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(3)]).unwrap(),
        Value::fixnum(99)
    );

    builtin_goto_char(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::NIL]).unwrap(),
        Value::fixnum(98)
    );

    let zero = builtin_get_byte(&mut eval, vec![Value::fixnum(0)]).unwrap_err();
    match zero {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(0), Value::fixnum(1), Value::fixnum(4)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let end = builtin_get_byte(&mut eval, vec![Value::fixnum(4)]).unwrap_err();
    match end {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(4), Value::fixnum(1), Value::fixnum(4)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    builtin_erase_buffer(&mut eval, vec![]).unwrap();
    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_multibyte_flag(current_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    builtin_insert_byte(&mut eval, vec![Value::fixnum(200), Value::fixnum(1)]).unwrap();
    builtin_insert_byte(&mut eval, vec![Value::fixnum(65), Value::fixnum(1)]).unwrap();
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(200)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(2)]).unwrap(),
        Value::fixnum(65)
    );
}

#[test]
fn get_byte_unibyte_string_returns_raw_byte_values() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let s = builtin_unibyte_string(vec![Value::fixnum(255), Value::fixnum(65)]).unwrap();

    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(0), s]).unwrap(),
        Value::fixnum(255)
    );
    assert_eq!(
        builtin_get_byte(&mut eval, vec![Value::fixnum(1), s]).unwrap(),
        Value::fixnum(65)
    );
}

/// `buffer-text-pixel-size` must honor a `display (space :align-to N)` text
/// property the same way `window-text-pixel-size` does: the measured width is
/// the real aligned-to column count, not the single-column-per-space count.
/// (Regression: marginalia's right-aligned annotation was clipped because the
/// align-to space counted as one column.)
#[test]
fn buffer_text_pixel_size_honors_display_align_to() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("buf-pixel-align-to", 200, 24, buf_id);
    eval.frames.select_frame(frame_id);
    {
        let frame = eval.frames.selected_frame_mut().expect("selected frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("AB XY");
    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::symbol(":align-to"),
        Value::fixnum(80),
    ]);
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(4),
            Value::symbol("display"),
            spec,
        ],
    )
    .expect("put display align-to property");

    let result = crate::emacs_core::xdisp::builtin_buffer_text_pixel_size(&mut eval, vec![])
        .expect("buffer-text-pixel-size");
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());
    // char_width == 1.0 so pixels == columns; "XY" lands at columns 80,81 => 82.
    assert_eq!(
        result.cons_car(),
        Value::fixnum(82),
        "align-to 80 should yield ~82 columns, not ~5"
    );
}

/// `buffer-text-pixel-size` honors `display (space :width N)` too.
#[test]
fn buffer_text_pixel_size_honors_display_space_width() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("buf-pixel-space-width", 200, 24, buf_id);
    eval.frames.select_frame(frame_id);
    {
        let frame = eval.frames.selected_frame_mut().expect("selected frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("AB XY");
    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::symbol(":width"),
        Value::fixnum(20),
    ]);
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(4),
            Value::symbol("display"),
            spec,
        ],
    )
    .expect("put display :width property");

    let result = crate::emacs_core::xdisp::builtin_buffer_text_pixel_size(&mut eval, vec![])
        .expect("buffer-text-pixel-size");
    assert_eq!(
        result.cons_car(),
        Value::fixnum(24),
        ":width 20 should add 20 columns (2 + 20 + 2)"
    );
}

/// `window-text-pixel-size` must traverse overlay `after-string`s — including
/// their own `display (space :align-to N)` props and embedded newlines — when
/// computing the max line width, mirroring GNU's display iterator.
///
/// Regression: vertico's candidate list is an overlay after-string at point-max
/// whose widest candidate line carries a right-aligned marginalia annotation as
/// a `(space :align-to (+ left N))` space.  Because the measurement skipped
/// overlay strings, the M-x posframe measured only the short prompt and clipped
/// the annotation.  The fix folds the after-string's widest line into the width.
#[test]
fn window_text_pixel_size_includes_overlay_after_string_align_to() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("win-pixel-overlay-after", 200, 24, buf_id);
    eval.frames.select_frame(frame_id);
    {
        let frame = eval.frames.selected_frame_mut().expect("selected frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }

    // Short prompt buffer text (4 columns).
    eval.buffers.get_mut(buf_id).expect("buffer").insert("M-x ");

    // Overlay after-string at point-max: a multi-line candidate list whose
    // WIDEST line is `cand1` + `(space :align-to 80)` + `ann1` => 84 columns.
    // Other lines are short.  Built in elisp so the string carries real text
    // properties (the `display` space) on the alignment char.
    eval.eval_str(
        r#"
        (let* ((ov (make-overlay (point-max) (point-max)))
               (sp (propertize " "
                     'display '(space :align-to 80)))
               (after (concat "cand1" sp "ann1" "\n"
                              "c2" "\n"
                              "candidate3")))
          (overlay-put ov 'after-string after))"#,
    )
    .expect("install overlay after-string");

    let result =
        crate::emacs_core::xdisp::builtin_window_text_pixel_size_ctx(&mut eval, vec![Value::NIL])
            .expect("window-text-pixel-size");
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());

    // char_width == 1.0 so pixels == columns.  Widest line is the align-to line:
    // "cand1" -> col 5, space jumps to col 80, "ann1" -> col 84.  This dominates
    // the 4-column prompt; without the overlay traversal the width would be 4.
    assert_eq!(
        result.cons_car(),
        Value::fixnum(84),
        "overlay after-string's align-to line should drive the width to 84, not the 4-col prompt"
    );
}

/// Overlay `before-string`s anchored at a position contribute their columns too,
/// and embedded newlines split the string into lines for the max-width measure.
#[test]
fn buffer_text_pixel_size_includes_overlay_before_string_multiline() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("buf-pixel-overlay-before", 200, 24, buf_id);
    eval.frames.select_frame(frame_id);
    {
        let frame = eval.frames.selected_frame_mut().expect("selected frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }

    eval.buffers.get_mut(buf_id).expect("buffer").insert("abc");

    // Before-string at buffer start whose widest embedded line is 10 columns.
    eval.eval_str(
        r#"
        (let ((ov (make-overlay (point-min) (point-min))))
          (overlay-put ov 'before-string "short\nten-cols!!\nx"))"#,
    )
    .expect("install overlay before-string");

    let result = crate::emacs_core::xdisp::builtin_buffer_text_pixel_size(&mut eval, vec![])
        .expect("buffer-text-pixel-size");
    assert_eq!(
        result.cons_car(),
        Value::fixnum(10),
        "before-string widest line (ten-cols!! = 10) should drive width past the 3-col buffer text"
    );
}

/// Plain text without a width-affecting `display` property still measures by
/// per-char display width (wide chars count as 2), unchanged by the fix.
#[test]
fn buffer_text_pixel_size_plain_text_unchanged() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("buf-pixel-plain", 200, 24, buf_id);
    eval.frames.select_frame(frame_id);
    {
        let frame = eval.frames.selected_frame_mut().expect("selected frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("hello\nworld!");
    let result = crate::emacs_core::xdisp::builtin_buffer_text_pixel_size(&mut eval, vec![])
        .expect("buffer-text-pixel-size");
    assert_eq!(result.cons_car(), Value::fixnum(6));
    assert_eq!(result.cons_cdr(), Value::fixnum(2));
}

/// Adversarial-review fix: changing or moving an overlay alters layout (face /
/// before-string / hl-line position) with no buffer-text edit, so it MUST bump
/// the overlay modification tick — otherwise the cursor-only / scroll fast paths
/// reuse rows that don't reflect the overlay (e.g. hl-line stuck on the old line).
#[test]
fn overlay_put_and_move_bump_overlay_modified_tick() {
    let mut eval = super::super::eval::Context::new();
    let buffer_id = eval.buffers.create_buffer("*ov-tick*");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .insert_into_buffer(buffer_id, "hello world")
        .expect("insert");

    let overlay = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)])
        .expect("make-overlay");

    let before = eval.buffers.get(buffer_id).unwrap().overlay_modified_tick();
    crate::emacs_core::buffer::builtin_overlay_put(
        &mut eval,
        vec![overlay, Value::symbol("face"), Value::symbol("bold")],
    )
    .expect("overlay-put");
    let after_put = eval.buffers.get(buffer_id).unwrap().overlay_modified_tick();
    assert!(
        after_put > before,
        "overlay-put must bump the overlay tick ({before} -> {after_put})"
    );

    crate::emacs_core::buffer::builtin_move_overlay(
        &mut eval,
        vec![overlay, Value::fixnum(5), Value::fixnum(8)],
    )
    .expect("move-overlay");
    let after_move = eval.buffers.get(buffer_id).unwrap().overlay_modified_tick();
    assert!(
        after_move > after_put,
        "move-overlay must bump the overlay tick ({after_put} -> {after_move})"
    );
}

/// GNU registers a newly named bitmap in `fringe-bitmaps' next to setting its
/// `fringe' property, both under the same "not already known" test
/// (fringe.c:1620-1656), and unregisters it on destroy only when its index is
/// above the standard range (fringe.c:1409-1414) -- so redefining does not
/// list a bitmap twice, and destroying a standard bitmap leaves the listing
/// and the property alone.
#[test]
fn define_fringe_bitmap_registers_the_symbol_in_fringe_bitmaps() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(let ((standard (length fringe-bitmaps)))
             (define-fringe-bitmap 'p9-a [0 0 24 24 0 0] nil nil 'center)
             (let ((after-define (list (and (memq 'p9-a fringe-bitmaps) t)
                                       (- (length fringe-bitmaps) standard))))
               (define-fringe-bitmap 'p9-a [0 0 60 60 0 0] nil nil 'center)
               (let ((after-redefine (- (length fringe-bitmaps) standard)))
                 (destroy-fringe-bitmap 'p9-a)
                 (let ((after-destroy (list (and (memq 'p9-a fringe-bitmaps) t)
                                            (- (length fringe-bitmaps) standard)
                                            (get 'p9-a 'fringe))))
                   (destroy-fringe-bitmap 'question-mark)
                   (list after-define after-redefine after-destroy
                         (list (and (memq 'question-mark fringe-bitmaps) t)
                               (get 'question-mark 'fringe)))))))"#,
    )
    .into_iter()
    .next()
    .expect("at least one form");
    assert_eq!(result, "OK ((t 1) 1 (nil 0 nil) (t 1))");
}

/// GNU `Fintern` hands the argument straight to `intern_driver`, which calls
/// `Fmake_symbol (string)` (lread.c:4705-4708, 4773-4806) -- so a symbol the
/// caller created owns THE ARGUMENT STRING OBJECT as its name. `symbol-name`
/// returns that object, with its text properties, its multibyteness and any
/// later mutation, and is `eq` to what was interned. Measured on GNU 31.0.90:
/// `(let ((s (propertize "x" 'p 1))) (eq s (symbol-name (intern s))))` is `t`
/// for a unibyte AND a multibyte name, and
/// `(multibyte-string-p (symbol-name (intern (string-to-multibyte "/tmp/H"))))`
/// is `t` -- GNU does not canonicalize an ascii-only multibyte name.
#[test]
fn intern_stores_the_name_string_object_it_was_handed() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    for name in [
        Value::multibyte_string("p16-intern-mb"),
        Value::unibyte_string("p16-intern-ub"),
        Value::multibyte_string("p16-intern-é"),
    ] {
        let sym = dispatch_builtin(&mut eval, "intern", vec![name])
            .expect("builtin intern should resolve")
            .expect("builtin intern should evaluate");
        let roundtrip = dispatch_builtin(&mut eval, "symbol-name", vec![sym])
            .expect("builtin symbol-name should resolve")
            .expect("builtin symbol-name should evaluate");
        // `Value`'s `PartialEq` is `equal`, not `eq` -- assert on the tagged
        // word so this pins OBJECT identity the way GNU's `eq` does.
        assert_eq!(
            roundtrip.0, name.0,
            "symbol-name must be eq to the string intern was handed"
        );
    }
}

/// The property-visible half of the same rule, at Lisp level: interning a
/// propertized name makes those properties readable through `symbol-name`,
/// for a multibyte name as much as a unibyte one. `make-symbol` already did
/// both. Interning a name whose symbol ALREADY exists keeps the existing name
/// object, so the argument's properties stay invisible (GNU returns `nil`
/// there -- `intern_driver` only runs when `oblookup` found no symbol).
#[test]
fn intern_keeps_a_multibyte_name_strings_text_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(list (text-properties-at
                  0 (symbol-name (intern (propertize (string-to-multibyte "p16-mb-alpha") 'fontified nil))))
                 (text-properties-at
                  0 (symbol-name (intern (propertize "p16-ub-alpha" 'fontified nil))))
                 (text-properties-at
                  0 (symbol-name (make-symbol (propertize (string-to-multibyte "p16-mb-beta") 'fontified nil))))
                 (progn (intern "p16-existing")
                        (text-properties-at
                         0 (symbol-name (intern (propertize "p16-existing" 'p 1))))))"#,
    )
    .into_iter()
    .next()
    .expect("at least one form");
    assert_eq!(
        result,
        "OK ((fontified nil) (fontified nil) (fontified nil) nil)"
    );
}

/// A custom obarray interns through the same rule: GNU's `Fintern` has one
/// creation path for both obarrays. Measured on GNU 31.0.90,
/// `(multibyte-string-p (symbol-name (intern (string-to-multibyte "/tmp/HELLO") ob)))`
/// is `t`, and interning the unibyte spelling finds that same symbol -- name
/// IDENTITY is byte-wise (our normalized name atom), while the name OBJECT is
/// whatever created the symbol.
#[test]
fn custom_obarray_intern_stores_the_name_string_object_it_was_handed() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let obarray = dispatch_builtin(&mut eval, "obarray-make", vec![Value::fixnum(17)])
        .expect("builtin obarray-make should resolve")
        .expect("builtin obarray-make should evaluate");
    let ascii_multibyte = Value::multibyte_string("/tmp/HELLO");

    let first = dispatch_builtin(&mut eval, "intern", vec![ascii_multibyte, obarray])
        .expect("builtin intern should resolve")
        .expect("builtin intern should evaluate");
    let soft = dispatch_builtin(
        &mut eval,
        "intern-soft",
        vec![Value::unibyte_string("/tmp/HELLO"), obarray],
    )
    .expect("builtin intern-soft should resolve")
    .expect("builtin intern-soft should evaluate");
    assert_eq!(soft, first, "byte-equal spellings name the same symbol");

    let stored = dispatch_builtin(&mut eval, "symbol-name", vec![first])
        .expect("builtin symbol-name should resolve")
        .expect("builtin symbol-name should evaluate");
    assert_eq!(
        stored.0, ascii_multibyte.0,
        "symbol-name must be eq to the string intern was handed"
    );
}

/// GNU's `push_key_description` ends in `CHAR_STRING (c, p)` (keymap.c:2298),
/// and `char_string` maps the eight-bit raw-byte range through
/// `CHAR_TO_BYTE8` + `BYTE8_STRING` (character.c:133-136) -- so a raw byte
/// comes back as ONE character whose code is the byte's raw-byte code, not as
/// a replacement character. `Fsingle_key_description` then wraps the buffer
/// with `make_specified_string (..., multibyte=1)` (keymap.c:2339), so a key
/// description is ALWAYS multibyte, even for plain ASCII.
///
/// Measured on GNU 31.0.90: `(key-description [4194208])` is "\240" with
/// length 1 and `multibyte-string-p` t; `(key-description [4194208 4194303 ?a])`
/// is "\240 \377 a"; `(multibyte-string-p (key-description [?a]))` is t.
#[test]
fn key_description_renders_a_raw_byte_as_that_byte() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(list (append (key-description [4194208]) nil)
                 (append (key-description [4194303]) nil)
                 (append (key-description [4194208 4194303 ?a]) nil)
                 (append (single-key-description 4194208) nil)
                 (append (text-char-description 4194208) nil)
                 (length (key-description [4194208]))
                 (list (multibyte-string-p (key-description [?a]))
                       (multibyte-string-p (single-key-description ?a))
                       (multibyte-string-p (text-char-description ?a))
                       (multibyte-string-p (text-char-description 4194208)))
                 ;; a genuine multibyte character must be unaffected
                 (append (key-description [?é]) nil)
                 (append (key-description [?中]) nil)
                 (key-description [?\C-x ?a]))"#,
    )
    .into_iter()
    .next()
    .expect("at least one form");
    assert_eq!(
        result,
        concat!(
            "OK ((4194208) (4194303) (4194208 32 4194303 32 97) (4194208) (4194208) 1 ",
            "(t t nil t) (233) (20013) \"C-x a\")"
        )
    );
}

/// GNU `Fsingle_key_description` treats a cons of two fixnums as AN INTERVAL
/// FROM A MAP-CHAR-TABLE and renders it `FROM..TO` (keymap.c:2322-2329):
///
///     if (CONSP (key) && FIXNUMP (XCAR (key)) && FIXNUMP (XCDR (key)))
///       return concat3 (Fsingle_key_description (XCAR (key), no_angles),
///                       dot_dot, Fsingle_key_description (XCDR (key), ...));
///
/// It has no "(MOD . CHAR) modifier event" case at all -- measured on GNU
/// 31.0.90, `(single-key-description (cons 1 ?x))` is "C-a..x" and
/// `(single-key-description (cons ?a ?z))` is "a..z", both plain ranges. A
/// Lucid-style event LIST such as `(meta shift up)` is still converted first
/// and is unaffected.
///
/// This is what sets `describe-bindings`' column: the widest key in the global
/// section is a raw-byte range from the `self-insert-command` char-table, and
/// mis-rendering it as a single modified character narrows the section and
/// shifts `describe-map--align-section` for every row beneath it.
#[test]
fn single_key_description_renders_a_fixnum_cons_as_a_range() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(list (single-key-description (cons ?a ?z))
                 (single-key-description (cons 1 ?x))
                 (key-description (vector (cons ?a ?z)))
                 (string-width (single-key-description (cons 128 4194239)))
                 (single-key-description '(meta shift up)))"#,
    )
    .into_iter()
    .next()
    .expect("at least one form");
    assert_eq!(result, r#"OK ("a..z" "C-a..x" "a..z" 10 "M-S-<up>")"#);
}
