use super::*;
use crate::emacs_core::subr::SubrSpec;
use crate::emacs_core::{Context, format_eval_result};
use crate::test_utils::load_minimal_gnu_backquote_runtime;

fn bootstrap_eval(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    load_minimal_gnu_backquote_runtime(&mut ev);
    ev.eval_str_each(src)
        .iter()
        .map(format_eval_result)
        .collect()
}

// ----- copy-alist -----

#[test]
fn copy_alist_basic() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![
        Value::cons(Value::symbol("a"), Value::fixnum(1)),
        Value::cons(Value::symbol("b"), Value::fixnum(2)),
    ]);
    let result = builtin_copy_alist(vec![alist]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 2);
    // Original and copy should have equal structure (`equal`).
    assert!(equal_value(&alist, &result, 0));
    // But the cons cells should not be eq (different heap objects).
    // GNU `eq` is pointer-equal, so check via eq_value not the
    // Rust PartialEq impl (which is structural `equal`).
    assert!(items[0].is_cons());
    let orig_first = &list_to_vec(&alist).unwrap()[0];
    assert!(orig_first.is_cons());
    assert!(
        !crate::emacs_core::value::eq_value(&items[0], orig_first),
        "copy-alist must produce a fresh cons cell, not eq the original"
    );
}

#[test]
fn copy_alist_empty() {
    crate::test_utils::init_test_tracing();
    let result = builtin_copy_alist(vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn copy_alist_circular_top_level_signals_like_gnu() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![Value::cons(Value::symbol("a"), Value::fixnum(1))]);
    alist.set_cdr(alist);

    match builtin_copy_alist(vec![alist]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "circular-list"),
        other => panic!("expected circular-list signal, got {other:?}"),
    }
}

// ----- rassoc / rassq -----

#[test]
fn rassoc_found() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![
        Value::cons(Value::symbol("a"), Value::fixnum(1)),
        Value::cons(Value::symbol("b"), Value::fixnum(2)),
        Value::cons(Value::symbol("c"), Value::fixnum(3)),
    ]);
    let result = builtin_rassoc(vec![Value::fixnum(2), alist]).unwrap();
    // Should return (b . 2)
    if result.is_cons() {
        let pair_car = result.cons_car();
        let _pair_cdr = result.cons_cdr();
        assert!(eq_value(&pair_car, &Value::symbol("b")));
    } else {
        panic!("expected cons");
    }
}

#[test]
fn rassoc_not_found() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![Value::cons(Value::symbol("a"), Value::fixnum(1))]);
    let result = builtin_rassoc(vec![Value::fixnum(99), alist]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn rassq_found() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![
        Value::cons(Value::symbol("x"), Value::symbol("yes")),
        Value::cons(Value::symbol("y"), Value::symbol("no")),
    ]);
    let result = builtin_rassq(vec![Value::symbol("yes"), alist]).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let _pair_cdr = result.cons_cdr();
        assert!(eq_value(&pair_car, &Value::symbol("x")));
    } else {
        panic!("expected cons");
    }
}

#[test]
fn rassq_not_found() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![Value::cons(Value::symbol("a"), Value::fixnum(1))]);
    let result = builtin_rassq(vec![Value::fixnum(99), alist]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn rassq_improper_alist_signals_listp() {
    crate::test_utils::init_test_tracing();
    let alist = Value::cons(
        Value::cons(Value::symbol("a"), Value::fixnum(1)),
        Value::fixnum(4),
    );
    match builtin_rassq(vec![Value::fixnum(99), alist]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}

#[test]
fn rassq_circular_alist_signals_circular_list() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![Value::cons(Value::symbol("a"), Value::fixnum(1))]);
    alist.set_cdr(alist);
    match builtin_rassq(vec![Value::fixnum(99), alist]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "circular-list");
        }
        other => panic!("expected circular-list, got {other:?}"),
    }
}

// ----- assoc-default -----

#[test]
fn assoc_default_bootstrap_matches_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'assoc-default))
        (assoc-default "key" '(("key" . 42)))
        (assoc-default "missing" '(("key" . 42)) nil -1)
        (assoc-default 'foo '((foo . 10)) 'eq)
        (assoc-default 'foo '(foo) nil 'fallback)
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK 42");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK 10");
    assert_eq!(results[4], "OK fallback");
}

#[test]
fn assoc_default_bootstrap_error_shapes_match_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err
            (assoc-default 'foo 1)
          (wrong-type-argument (list (car err) (nth 1 err))))
        (condition-case err
            (assoc-default 'foo '((foo . 10)) 1)
          (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK (wrong-type-argument listp)");
    assert_eq!(results[1], "OK invalid-function");
}

// ----- make-list -----

#[test]
fn make_list_basic() {
    crate::test_utils::init_test_tracing();
    let result = builtin_make_list(vec![Value::fixnum(3), Value::symbol("x")]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 3);
    for item in &items {
        assert!(eq_value(item, &Value::symbol("x")));
    }
}

#[test]
fn make_list_zero() {
    crate::test_utils::init_test_tracing();
    let result = builtin_make_list(vec![Value::fixnum(0), Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn make_list_validates_wholenump_length() {
    crate::test_utils::init_test_tracing();
    let negative = builtin_make_list(vec![Value::fixnum(-1), Value::fixnum(1)]).unwrap_err();
    let float = builtin_make_list(vec![Value::make_float(3.2), Value::fixnum(1)]).unwrap_err();
    match negative {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("wholenump"), Value::fixnum(-1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match float {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("wholenump"), Value::make_float(3.2)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn string_repeat_basic() {
    crate::test_utils::init_test_tracing();
    let result = builtin_string_repeat(vec![Value::string("ab"), Value::fixnum(3)]).unwrap();
    assert_eq!(result.as_utf8_str().unwrap(), "ababab");
}

#[test]
fn string_repeat_zero() {
    crate::test_utils::init_test_tracing();
    let result = builtin_string_repeat(vec![Value::string("ab"), Value::fixnum(0)]).unwrap();
    assert_eq!(result.as_utf8_str().unwrap(), "");
}

#[test]
fn string_repeat_errors() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_string_repeat(vec![]).is_err());
    assert!(builtin_string_repeat(vec![Value::string("ab")]).is_err());
    assert!(builtin_string_repeat(vec![Value::string("ab"), Value::fixnum(-1)]).is_err());
    assert!(builtin_string_repeat(vec![Value::fixnum(1), Value::fixnum(2)]).is_err());
}

// ----- safe-length -----

#[test]
fn safe_length_proper_list() {
    crate::test_utils::init_test_tracing();
    let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    let result = builtin_safe_length(vec![list]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(3)));
}

#[test]
fn safe_length_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_safe_length(vec![Value::NIL]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(0)));
}

#[test]
fn safe_length_non_list() {
    crate::test_utils::init_test_tracing();
    let result = builtin_safe_length(vec![Value::fixnum(42)]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(0)));
}

#[test]
fn safe_length_circular_lists_match_gnu_detection_count() {
    crate::test_utils::init_test_tracing();

    let one = Value::cons(Value::symbol("a"), Value::NIL);
    one.set_cdr(one);
    let result = builtin_safe_length(vec![one]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(1)));

    let three = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    let tail = three.cons_cdr().cons_cdr();
    tail.set_cdr(three);
    let result = builtin_safe_length(vec![three]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(5)));

    let offset = Value::list(vec![
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
        Value::fixnum(4),
    ]);
    let cycle_start = offset.cons_cdr();
    let offset_tail = cycle_start.cons_cdr().cons_cdr();
    offset_tail.set_cdr(cycle_start);
    let result = builtin_safe_length(vec![offset]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(5)));
}

// ----- subst-char-in-string -----

#[test]
fn subst_char_in_string_preserves_nonunicode_character_codes() {
    crate::test_utils::init_test_tracing();
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let len = crate::emacs_core::emacs_char::char_string(0x3F_FFFF, &mut buf);
    let value = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        buf[..len].to_vec(),
    ));
    let result = builtin_subst_char_in_string(vec![
        Value::fixnum(0x3F_FFFF),
        Value::fixnum(0x3F_FFFE),
        value,
    ])
    .unwrap();
    let ls = result
        .as_lisp_string()
        .expect("subst-char-in-string result");
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(ls),
        vec![0x3F_FFFE]
    );
}

#[test]
fn subst_char_basic() {
    crate::test_utils::init_test_tracing();
    let result = builtin_subst_char_in_string(vec![
        Value::char('.'),
        Value::char('/'),
        Value::string("a.b.c"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str().unwrap(), "a/b/c");
}

#[test]
fn subst_char_no_match() {
    crate::test_utils::init_test_tracing();
    let result = builtin_subst_char_in_string(vec![
        Value::char('z'),
        Value::char('!'),
        Value::string("hello"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str().unwrap(), "hello");
}

// ----- string encoding identity stubs -----

#[test]
fn string_to_multibyte_identity() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("hello");
    let result = builtin_string_to_multibyte(vec![s]).unwrap();
    assert!(equal_value(&s, &result, 0));
}

#[test]
fn string_to_multibyte_converts_unibyte_high_bytes_to_raw_byte_chars() {
    crate::test_utils::init_test_tracing();
    // Create a unibyte string with byte 0xFF
    let v = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = builtin_string_to_multibyte(vec![v]).unwrap();
    let ls = result.as_lisp_string().unwrap();
    assert!(ls.is_multibyte());
    assert_eq!(ls.sbytes(), 2); // raw byte 0xFF -> 2-byte overlong
    assert_eq!(ls.schars(), 1);
    // Decode: should be 0x3FFFFF (byte8_to_char(0xFF))
    let codes: Vec<u32> = crate::emacs_core::builtins::lisp_string_char_codes(ls);
    assert_eq!(codes, vec![0x3FFFFF]);
}

#[test]
fn string_to_unibyte_ascii_storage() {
    crate::test_utils::init_test_tracing();
    let result = builtin_string_to_unibyte(vec![Value::string("world")]).unwrap();
    let ls = result.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    assert_eq!(ls.sbytes(), 5);
    assert_eq!(ls.as_bytes(), b"world");
}

#[test]
fn string_to_unibyte_rejects_unicode_scalar() {
    crate::test_utils::init_test_tracing();
    let result = builtin_string_to_unibyte(vec![Value::string("é")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Cannot convert character at index 0 to unibyte"
                )]
            );
        }
        other => panic!("expected conversion error, got {other:?}"),
    }
}

#[test]
fn string_to_unibyte_preserves_existing_unibyte_storage() {
    crate::test_utils::init_test_tracing();
    // Create a unibyte string with byte 0xFF
    let v = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = builtin_string_to_unibyte(vec![v]).unwrap();
    let ls = result.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    assert_eq!(ls.sbytes(), 1);
    assert_eq!(ls.as_bytes(), &[0xFF]);
}

#[test]
fn string_as_unibyte_utf8_bytes_for_unicode() {
    crate::test_utils::init_test_tracing();
    let result = builtin_string_as_unibyte(vec![Value::string("é")]).unwrap();
    let ls = result.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    assert_eq!(ls.sbytes(), 2);
    assert_eq!(ls.as_bytes(), &[0xC3, 0xA9]); // UTF-8 encoding of é
}

#[test]
fn string_as_unibyte_ascii_passthrough_bytes() {
    crate::test_utils::init_test_tracing();
    let result = builtin_string_as_unibyte(vec![Value::string("test")]).unwrap();
    let ls = result.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    assert_eq!(ls.sbytes(), 4);
    assert_eq!(ls.as_bytes(), b"test");
}

#[test]
fn string_as_unibyte_preserves_unibyte_storage_bytes() {
    crate::test_utils::init_test_tracing();
    // Create a unibyte string with byte 0xFF
    let v = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = builtin_string_as_unibyte(vec![v]).unwrap();
    let ls = result.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    assert_eq!(ls.sbytes(), 1);
    assert_eq!(ls.as_bytes(), &[0xFF]);
}

#[test]
fn string_as_multibyte_identity_for_multibyte_input() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("test");
    let result = builtin_string_as_multibyte(vec![s]).unwrap();
    assert!(equal_value(&s, &result, 0));
}

#[test]
fn string_as_multibyte_converts_unibyte_high_bytes_to_raw_byte_chars() {
    crate::test_utils::init_test_tracing();
    // Create a unibyte string with byte 0xFF
    let v = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = builtin_string_as_multibyte(vec![v]).unwrap();
    let ls = result.as_lisp_string().unwrap();
    assert!(ls.is_multibyte());
    assert_eq!(ls.sbytes(), 2); // raw byte 0xFF -> 2-byte overlong
    let codes: Vec<u32> = crate::emacs_core::builtins::lisp_string_char_codes(ls);
    assert_eq!(codes, vec![0x3FFFFF]);
}

// ----- char encoding conversions -----

#[test]
fn unibyte_char_to_multibyte_ascii_identity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_unibyte_char_to_multibyte(vec![Value::fixnum(65)]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(65)));
}

#[test]
fn unibyte_char_to_multibyte_high_byte_maps_to_raw_range() {
    crate::test_utils::init_test_tracing();
    let result = builtin_unibyte_char_to_multibyte(vec![Value::fixnum(255)]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(0x3FFFFF)));
}

#[test]
fn unibyte_char_to_multibyte_rejects_non_unibyte_code() {
    crate::test_utils::init_test_tracing();
    let result = builtin_unibyte_char_to_multibyte(vec![Value::fixnum(256)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Not a unibyte character: 256")]
            );
        }
        other => panic!("expected conversion error, got {other:?}"),
    }
}

#[test]
fn multibyte_char_to_unibyte_ascii_passthrough() {
    crate::test_utils::init_test_tracing();
    let result = builtin_multibyte_char_to_unibyte(vec![Value::fixnum(65)]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(65)));
}

#[test]
fn multibyte_char_to_unibyte_raw_range_maps_to_byte() {
    crate::test_utils::init_test_tracing();
    let result = builtin_multibyte_char_to_unibyte(vec![Value::fixnum(0x3FFFFF)]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(255)));
}

#[test]
fn multibyte_char_to_unibyte_returns_minus_one_for_non_unibyte_unicode() {
    crate::test_utils::init_test_tracing();
    let result = builtin_multibyte_char_to_unibyte(vec![Value::fixnum(256)]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(-1)));
}

// ----- locale-info -----

#[test]
fn locale_info_item_domain_matches_gnu_symbols() {
    assert_eq!(
        LocaleInfoItem::from_value(Value::symbol("codeset")),
        Some(LocaleInfoItem::Codeset)
    );
    assert_eq!(
        LocaleInfoItem::from_value(Value::symbol("days")),
        Some(LocaleInfoItem::Days)
    );
    assert_eq!(
        LocaleInfoItem::from_value(Value::symbol("months")),
        Some(LocaleInfoItem::Months)
    );
    assert_eq!(
        LocaleInfoItem::from_value(Value::symbol("paper")),
        Some(LocaleInfoItem::Paper)
    );
    assert_eq!(LocaleInfoItem::Codeset.symbol_name(), "codeset");
    assert_eq!(LocaleInfoItem::from_value(Value::symbol("time")), None);
    assert_eq!(LocaleInfoItem::from_value(Value::string("codeset")), None);
}

#[test]
fn locale_info_codeset_returns_utf8() {
    crate::test_utils::init_test_tracing();
    let result = builtin_locale_info(vec![Value::symbol("codeset")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("UTF-8"));
}

#[test]
fn locale_info_days_months_and_paper_return_oracle_shapes() {
    crate::test_utils::init_test_tracing();
    let days = builtin_locale_info(vec![Value::symbol("days")]).unwrap();
    let days_vec = match days.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => days.as_vector_data().unwrap().clone(),
        other => panic!("days should be a vector, got {other:?}"),
    };
    assert_eq!(days_vec.len(), 7);
    assert_eq!(days_vec[0], Value::string("Sunday"));
    assert_eq!(days_vec[6], Value::string("Saturday"));

    let months = builtin_locale_info(vec![Value::symbol("months")]).unwrap();
    let months_vec = match months.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => months.as_vector_data().unwrap().clone(),
        other => panic!("months should be a vector, got {other:?}"),
    };
    assert_eq!(months_vec.len(), 12);
    assert_eq!(months_vec[0], Value::string("January"));
    assert_eq!(months_vec[11], Value::string("December"));

    let paper = builtin_locale_info(vec![Value::symbol("paper")]).unwrap();
    assert_eq!(
        paper,
        Value::list(vec![Value::fixnum(210), Value::fixnum(297)])
    );
}

#[test]
fn locale_info_unknown_or_non_symbol_items_return_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_locale_info(vec![Value::symbol("time")]).unwrap();
    assert!(result.is_nil());
    let result = builtin_locale_info(vec![Value::string("codeset")]).unwrap();
    assert!(result.is_nil());
    let result = builtin_locale_info(vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn display_line_numbers_update_width_is_noop() {
    crate::test_utils::init_test_tracing();
    let result = builtin_display_line_numbers_update_width(vec![]).unwrap();
    assert_eq!(result, Value::NIL);
}

#[test]
fn display_line_numbers_update_width_arity() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_display_line_numbers_update_width(vec![Value::NIL]).is_err());
}

// ----- eval-dependent builtins (need Context) -----

#[test]
fn recursion_depth_zero() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_recursion_depth(&mut eval, vec![]).unwrap();
    // At top level, depth is 0
    assert!(eq_value(&result, &Value::fixnum(0)));
}

#[test]
fn recursion_depth_counts_recursive_command_loops_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    // Neomacs stores one raw command-loop level for GNU's visible top level.
    // A second active level is therefore one recursive edit.
    eval.command_loop.recursive_depth = 2;

    let result = builtin_recursion_depth(&mut eval, vec![]).unwrap();

    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn recursion_depth_includes_active_minibuffers_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.command_loop.recursive_depth = 2;
    eval.minibuffers
        .read_from_minibuffer(crate::buffer::BufferId(1), "Prompt: ", None, None)
        .unwrap();

    let result = builtin_recursion_depth(&mut eval, vec![]).unwrap();

    assert_eq!(result, Value::fixnum(2));
}

// Regression tests for `backtrace-frame--internal` — the real
// introspection primitive that walks the specpdl. GNU's
// `backtrace_frame_apply` (eval.c:3984) reads each SPECPDL_BACKTRACE
// entry and invokes the callback with (evald func args flags).
// Neomacs's `runtime_backtrace_frames_from_base` + `apply_backtrace_callback`
// do the same for `SpecBinding::Backtrace`. These tests exercise the
// live path including a frame pushed by `push_backtrace_frame` (which
// fires on every bytecode and interpreter call, see vm.rs:3443 and
// eval.rs:8858).

#[test]
fn backtrace_frame_internal_surfaces_live_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    // Push a backtrace frame ourselves so the walker has something to
    // surface without needing a full compile/call round-trip.
    eval.push_backtrace_frame(
        Value::symbol("my-func"),
        &[Value::fixnum(1), Value::fixnum(2)],
    );

    // Call backtrace-frame--internal with a callback that just
    // returns its four args as a list.  base=nil walks from the top
    // of the stack; nframes=0 grabs the newest frame.
    let callback = eval
        .eval_str("(lambda (evald func args flags) (list evald func args flags))")
        .expect("build callback");
    let result = super::builtin_backtrace_frame_internal(
        &mut eval,
        vec![callback, Value::fixnum(0), Value::NIL],
    )
    .expect("walk");

    let items = list_to_vec(&result).expect("four-element list");
    assert_eq!(items.len(), 4);
    assert_eq!(items[0], Value::T, "evald should be t");
    assert_eq!(items[1], Value::symbol("my-func"), "func symbol");
    assert_eq!(
        items[2],
        Value::list(vec![Value::fixnum(1), Value::fixnum(2)]),
        "args list"
    );
    assert!(items[3].is_nil(), "no flags on this frame");
}

/// Regression for GNU `nargs == UNEVALLED` parity (eval.c:2585, 3993).
/// `eval_sub_cons` must push an UNEVALLED backtrace frame around every
/// public special-form dispatch (`if`, `while`, `let`, etc.). The frame
/// exists only during dispatch; we observe it from inside the body via
/// a Rust probe subr that snapshots the live specpdl.
#[test]
fn eval_sub_cons_pushes_unevalled_frame_for_special_forms() {
    use std::cell::RefCell;

    thread_local! {
        static PROBE: RefCell<Vec<(Value, bool)>> = const { RefCell::new(Vec::new()) };
    }

    fn probe(eval: &mut super::super::eval::Context) -> EvalResult {
        let snap: Vec<(Value, bool)> = eval
            .specpdl
            .iter()
            .filter_map(|entry| {
                eval.backtrace_entry_values(entry)
                    .map(|(function, _, _, unevalled)| (function, unevalled))
            })
            .collect();
        PROBE.with(|p| *p.borrow_mut() = snap);
        Ok(Value::NIL)
    }

    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.register_subr(SubrSpec::fixed0("__unevalled_probe__", probe));

    PROBE.with(|p| p.borrow_mut().clear());
    eval.eval_str("(if t (__unevalled_probe__) nil)")
        .expect("eval if-body");
    let snap = PROBE.with(|p| p.borrow().clone());
    assert!(
        snap.iter().any(|(f, u)| *f == Value::symbol("if") && *u),
        "expected an UNEVALLED `if' frame while body runs, got {:?}",
        snap
    );
}

/// Regression for the Phase 2 architectural change: GNU `eval_sub`
/// pushes an UNEVALLED backtrace frame for EVERY cons-form call
/// (eval.c:2585), not just special forms. During arg evaluation of
/// `(foo (probe))`, walkers must see the outer `foo` frame on the
/// stack.
#[test]
fn eval_sub_cons_pushes_outer_unevalled_during_arg_eval() {
    use std::cell::RefCell;

    thread_local! {
        static ARG_PROBE: RefCell<Vec<(Value, bool)>> = const { RefCell::new(Vec::new()) };
    }

    fn probe(eval: &mut super::super::eval::Context) -> EvalResult {
        let snap: Vec<(Value, bool)> = eval
            .specpdl
            .iter()
            .filter_map(|entry| {
                eval.backtrace_entry_values(entry)
                    .map(|(function, _, _, unevalled)| (function, unevalled))
            })
            .collect();
        ARG_PROBE.with(|p| *p.borrow_mut() = snap);
        Ok(Value::NIL)
    }

    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.register_subr(SubrSpec::fixed0("__arg_eval_probe__", probe));

    // Wrap in a user lambda so there's an "outer" non-builtin frame we
    // can look for. `defun` is an elisp macro not available in a bare
    // Context; `fset` with a literal lambda is primitive and works.
    eval.eval_str("(fset 'my-outer-fn (lambda (x) x))")
        .expect("fset outer");

    ARG_PROBE.with(|p| p.borrow_mut().clear());
    eval.eval_str("(my-outer-fn (__arg_eval_probe__))")
        .expect("eval outer call");
    let snap = ARG_PROBE.with(|p| p.borrow().clone());
    assert!(
        snap.iter()
            .any(|(f, u)| *f == Value::symbol("my-outer-fn") && *u),
        "expected UNEVALLED my-outer-fn frame visible during arg eval, \
         got {:?}",
        snap
    );
}

/// Unit test for `set_backtrace_args_evalled` — mirrors GNU
/// `set_backtrace_args` (eval.c:144-156) called at eval.c:2638, 2660,
/// 3299 to promote a UNEVALLED frame to EVALD in place.
#[test]
fn set_backtrace_args_evalled_mutates_in_place() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let bt_count = eval.specpdl.len();
    let original_args = Value::list(vec![
        Value::symbol("x"),
        Value::list(vec![Value::symbol("+"), Value::fixnum(1), Value::fixnum(2)]),
    ]);
    eval.push_unevalled_backtrace_frame(Value::symbol("my-func"), original_args);

    // Promote to EVALD with evaluated values.
    let evaluated = [Value::fixnum(42), Value::fixnum(3)];
    eval.set_backtrace_args_evalled(bt_count, &evaluated);

    // Inspect slot — should now be EVALD with the evaluated values.
    let (function, args, _, unevalled) = eval
        .backtrace_entry_values(eval.specpdl.last().expect("frame present"))
        .expect("backtrace frame");
    assert!(!unevalled, "flag cleared after promotion");
    assert_eq!(function, Value::symbol("my-func"), "function preserved");
    assert_eq!(
        args.as_slice(),
        evaluated,
        "args replaced with evaluated values"
    );
}

/// Regression for `backtrace-frame--internal` UNEVALLED dispatch. Set
/// up an artificial UNEVALLED frame via `push_unevalled_backtrace_frame`
/// and assert the callback receives `(nil FUNC FORMS nil)`.
#[test]
fn backtrace_frame_internal_surfaces_unevalled_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let original_args = Value::list(vec![
        Value::symbol("x"),
        Value::list(vec![Value::symbol("+"), Value::fixnum(1), Value::fixnum(2)]),
    ]);
    eval.push_unevalled_backtrace_frame(Value::symbol("if"), original_args);

    let callback = eval
        .eval_str("(lambda (evald func args flags) (list evald func args flags))")
        .expect("build callback");
    let result = super::builtin_backtrace_frame_internal(
        &mut eval,
        vec![callback, Value::fixnum(0), Value::NIL],
    )
    .expect("walk");

    let items = list_to_vec(&result).expect("four-element list");
    assert_eq!(items.len(), 4);
    assert!(items[0].is_nil(), "UNEVALLED → evald=nil");
    assert_eq!(items[1], Value::symbol("if"));
    // args should be the cons list of un-evaluated forms, not a wrapping list.
    assert_eq!(
        items[2], original_args,
        "forms list passed through verbatim"
    );
    assert!(items[3].is_nil());
}

#[test]
fn mapbacktrace_base_t_does_not_recurse() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval.eval_str(
        r#"(let (bt-frames)
             (condition-case nil
                 (signal 'wrong-type-argument '(integerp "x"))
               (error
                (mapbacktrace
                 (lambda (_evald func args _flags)
                   (push (list (if (symbolp func) func :lambda)
                               (length args))
                         bt-frames))
                 t)))
             (list (consp bt-frames) (> (length bt-frames) 0)))"#,
    );

    assert!(
        result.is_ok(),
        "mapbacktrace base t probe failed: {}",
        format_eval_result(&result)
    );
    assert_eq!(
        super::super::print::print_value(&result.unwrap()),
        "(nil nil)"
    );
}

#[test]
fn handler_bind_sees_signaling_eval_frames_before_unwind() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval.eval_str(
        r#"(let (seen-eval seen-progn)
                 (condition-case nil
                     (handler-bind-1
                       (lambda () (eval '(progn hb-missing-symbol) t))
                       '(error)
                       (lambda (_err)
                         (mapbacktrace
                          (lambda (evald func args _flags)
                            (if (and evald
                                     (eq func 'eval)
                                     (equal args '((progn hb-missing-symbol) t)))
                                (setq seen-eval t))
                            (if (and (null evald)
                                     (eq func 'progn)
                                     (equal args '(hb-missing-symbol)))
                                (setq seen-progn t))))))
                   (error nil))
                 (list seen-eval seen-progn))"#,
    );

    assert!(
        result.is_ok(),
        "handler-bind probe failed: {}",
        format_eval_result(&result)
    );
    let result = result.unwrap();

    assert_eq!(
        super::super::print::print_value(&result),
        "(t t)",
        "GNU keeps eval and inner UNEVALLED form frames visible while handler-bind runs"
    );
}

#[test]
fn handler_bind_sees_eval_symbol_frame_before_feval_unwind() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval.eval_str(
        r#"(let (seen-eval)
                 (condition-case nil
                     (handler-bind-1
                       (lambda () (eval 'hb-missing-symbol t))
                       '(error)
                       (lambda (_err)
                         (mapbacktrace
                          (lambda (evald func args _flags)
                            (if (and evald
                                     (eq func 'eval)
                                     (equal args '(hb-missing-symbol t)))
                                (setq seen-eval t))))))
                   (error nil))
                 seen-eval)"#,
    );

    assert!(
        result.is_ok(),
        "handler-bind eval-symbol probe failed: {}",
        format_eval_result(&result)
    );
    let result = result.unwrap();

    assert_eq!(
        super::super::print::print_value(&result),
        "t",
        "GNU keeps the active eval frame visible while a bare-symbol eval signal is handled"
    );
}

#[test]
fn backtrace_helper_stubs_shape_and_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let thread = super::super::threads::builtin_current_thread(&mut eval, vec![]).unwrap();
    let frames = builtin_backtrace_frames_from_thread(&mut eval, vec![thread]).unwrap();
    assert!(frames.is_list());
    assert!(matches!(
        builtin_backtrace_frames_from_thread(&mut eval, vec![Value::NIL]),
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("threadp"), Value::NIL]
    ));

    assert!(matches!(
        builtin_backtrace_locals(&mut eval, vec![Value::NIL]),
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("wholenump"), Value::NIL]
    ));
    assert!(matches!(
        builtin_backtrace_locals(&mut eval, vec![Value::fixnum(0)]),
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("wholenump"), Value::fixnum(-1)]
    ));
    assert!(matches!(
        builtin_backtrace_eval(&mut eval, vec![Value::fixnum(0), Value::NIL]),
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("wholenump"), Value::NIL]
    ));

    eval.push_backtrace_frame(Value::symbol("outer-frame"), &[]);
    eval.push_backtrace_frame(Value::symbol("base-frame"), &[]);
    assert_eq!(
        builtin_backtrace_locals(
            &mut eval,
            vec![Value::fixnum(1), Value::symbol("base-frame")]
        )
        .expect("base function should be accepted"),
        Value::NIL
    );
    assert_eq!(
        builtin_backtrace_debug(
            &mut eval,
            vec![Value::fixnum(1), Value::T, Value::symbol("base-frame")]
        )
        .expect("backtrace-debug should accept a non-numeric flag and function base"),
        Value::T
    );
    assert_eq!(
        builtin_backtrace_eval(
            &mut eval,
            vec![
                Value::fixnum(42),
                Value::fixnum(1),
                Value::symbol("base-frame")
            ]
        )
        .expect("backtrace-eval should accept expression, frame number, and function base"),
        Value::fixnum(42)
    );
}

#[test]
fn backtrace_helper_stubs_arity_checks() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    assert!(matches!(
        builtin_backtrace_debug(&mut eval, vec![]),
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data == vec![Value::symbol("backtrace-debug"), Value::fixnum(0)]
    ));
    assert!(matches!(
        builtin_backtrace_debug(&mut eval, vec![Value::fixnum(0)]),
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data == vec![Value::symbol("backtrace-debug"), Value::fixnum(1)]
    ));
    assert!(matches!(
        builtin_backtrace_frame_internal(&mut eval, vec![]),
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data == vec![Value::symbol("backtrace-frame--internal"), Value::fixnum(0)]
    ));
}

#[test]
fn backtrace_frame_internal_tracks_runtime_funcall_interactively_marker() {
    crate::test_utils::init_test_tracing();
    let mut ev = super::super::eval::Context::new();
    let results = ev.eval_str_each(
        r#"
        (progn
          (fset 'neovm--misc-bt-target
                (lambda ()
                  (interactive)
                  (let (frame)
                    (backtrace-frame--internal
                     (lambda (evald func args flags)
                       (setq frame (list evald func args flags)))
                     1
                     'neovm--misc-bt-target)
                    (nth 1 frame))))
          (unwind-protect
              (list
               (funcall-interactively 'neovm--misc-bt-target)
               (call-interactively 'neovm--misc-bt-target))
            (fmakunbound 'neovm--misc-bt-target)))
        "#,
    );
    assert_eq!(
        results.iter().map(format_eval_result).collect::<Vec<_>>(),
        vec!["OK (funcall-interactively funcall-interactively)"]
    );
}

// ----- special form: save-current-buffer -----

#[test]
fn sf_save_current_buffer_restores() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // Create a buffer and make it current
    let buf_id = ev.buffers.create_buffer("*test*");
    ev.buffers.set_current(buf_id);

    // save-current-buffer with body that just returns 42
    let result = ev.eval_str("(save-current-buffer 42)").unwrap();
    assert!(eq_value(&result, &Value::fixnum(42)));
    // Current buffer should still be *test*
    assert_eq!(ev.buffers.current_buffer().unwrap().id, buf_id);
}

// ----- with-syntax-table (Elisp macro in GNU subr.el:6394) -----

#[test]
fn sf_with_syntax_table_evaluates_body() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    load_minimal_gnu_backquote_runtime(&mut ev);
    let result = ev
        .eval_str("(with-syntax-table (make-syntax-table) 30)")
        .expect("eval");
    assert!(eq_value(&result, &Value::fixnum(30)));
}

#[test]
fn sf_with_syntax_table_restores_original_table_on_success() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    load_minimal_gnu_backquote_runtime(&mut ev);
    let original = crate::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    ev.eval_str("(with-syntax-table (make-syntax-table) 1)")
        .expect("eval");
    let restored = crate::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    assert!(eq_value(&restored, &original));
}

#[test]
fn with_syntax_table_restores_original_table_on_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    load_minimal_gnu_backquote_runtime(&mut ev);
    let original = crate::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    let _ = ev.eval_str("(ignore-errors (with-syntax-table (make-syntax-table) missing-var))");
    let restored = crate::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    assert!(eq_value(&restored, &original));
}

// ---------------------------------------------------------------------------
// Ledger 183: GNU's BASE resolution for the backtrace primitives, row by row.
//
// `get_backtrace_starting_at` (`src/eval.c:3958-3980`) is shared by
// `backtrace-frame`, `backtrace-debug`, `mapbacktrace` and
// `backtrace--frames-from-thread`, so probing it through `backtrace-frame`
// probes all four.  Ledger 172 audited it "only at its edges"; this is the
// frame-by-frame comparison it asked for -- 20 rows measured against GNU Emacs
// 31.0.90 `-Q --batch` (`tmp/l183-p13.el`), of which 19 already agreed and one
// did not.
// ---------------------------------------------------------------------------

/// The one divergent row of the 20.
///
/// GNU takes the offset out of `(OFFSET . FUNCTION)` with a bare `XFIXNUM`
/// (`src/eval.c:3966`) -- there is no `CHECK_FIXNAT` there, unlike the LEVEL
/// argument twenty lines further down (`:3986`) -- and spends it in
/// `while (backtrace_p (pdl) && offset-- > 0)` (`:3977`).  So a negative
/// offset is not an error, it is a loop that does not run.  Measured
/// (`tmp/l183-p14.el`), each row projected to the frame's function:
///
/// ```text
/// GNU                (l183-b2 l183-b2 l183-b1 nil wrong-type-argument)
/// this port, before  (wrong-type-argument l183-b2 l183-b1 nil wrong-type-argument)
/// ```
///
/// The asymmetry is the point: the same primitive checks one of its two
/// numbers and not the other, and copying the check onto both is how it got
/// wrong here.  The last row is the LEVEL argument, which GNU does check.
#[test]
fn a_negative_base_offset_is_not_an_error_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `defun` is a macro from `byte-run.el`/`subr.el`, so a bare `Context`
    // cannot run this probe at all -- the first version of this pin failed
    // with `void-function l183-b1` rather than with the divergence.
    let out = bootstrap_eval(
        "(defun l183-b3 (probe) (funcall probe))
         (defun l183-b2 (probe) (l183-b3 probe))
         (defun l183-b1 (probe) (l183-b2 probe))
         (mapcar (lambda (x) (if (consp x) (nth 1 x) x))
           (l183-b1 (lambda ()
             (list (condition-case e (backtrace-frame 0 '(-1 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame 0 '(0 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame 0 '(1 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame 0 '(99 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame -1 'l183-b2) (error (car e)))))))",
    );
    assert_eq!(
        out.last().expect("the probe form must have run"),
        "OK (l183-b2 l183-b2 l183-b1 nil wrong-type-argument)"
    );
}
