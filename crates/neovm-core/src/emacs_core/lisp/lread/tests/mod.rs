use super::*;
use crate::emacs_core::eval::Context;
use malachite::integer::Integer;

/// Test helper: create a fresh eval context for locate-file tests.
fn test_eval_ctx() -> Context {
    Context::new()
}

#[test]
fn eval_buffer_evaluates_current_buffer_forms() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-eb-a 11)\n(setq lread-eb-b (+ lread-eb-a 1))");
    }
    let result = builtin_eval_buffer(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-eb-a").cloned(),
        Some(Value::fixnum(11))
    );
    assert_eq!(
        ev.obarray.symbol_value("lread-eb-b").cloned(),
        Some(Value::fixnum(12))
    );
}

#[test]
fn eval_buffer_with_custom_reader_eagerly_expands_compiler_macros_in_lexical_functions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str("(setq load-read-function (symbol-function 'read))")
        .expect("install a custom Lisp form reader");
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert(
            ";;; -*- lexical-binding: t; -*-\n\
             (defun lread-eager-compiler-macro-probe ()\n\
               (let ((items nil))\n\
                 (add-to-list 'items \"value\" t)\n\
                 items))\n",
        );

    builtin_eval_buffer(&mut eval, vec![]).expect("evaluate lexical source buffer");

    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str("(lread-eager-compiler-macro-probe)"),),
        "OK (\"value\")",
        "source loading must expand add-to-list before its quoted target becomes lexical",
    );
}

#[test]
fn eval_buffer_incomplete_form_signals_source_buffer_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.current_buffer().expect("current buffer").id;
    ev.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("(defun incomplete (");

    let result = builtin_eval_buffer(&mut ev, vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "end-of-file"
                && sig.data == vec![Value::make_buffer(buffer_id)]
    ));
}

#[test]
fn eval_buffer_preserves_unibyte_string_literals() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            b'(', b's', b'e', b't', b'q', b' ', b'l', b'r', b'e', b'a', b'd', b'-', b'e', b'b',
            b'-', b'r', b'a', b'w', b' ', b'"', 0xFF, b'"', b')',
        ]));
    }

    let result = builtin_eval_buffer(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
    let value = ev
        .obarray
        .symbol_value("lread-eb-raw")
        .copied()
        .expect("setq should bind lread-eb-raw");
    let text = value
        .as_lisp_string()
        .expect("setq target should be a string");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn eval_buffer_accepts_shebang_reader_prefix() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("#!/usr/bin/env emacs --script\n(setq lread-eb-shebang 'ok)\n");
    }
    let result = builtin_eval_buffer(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-eb-shebang").cloned(),
        Some(Value::symbol("ok"))
    );
}

#[test]
fn eval_buffer_single_line_shebang_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("#!/usr/bin/env emacs --script");
    }
    let result = builtin_eval_buffer(&mut ev, vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty()
    ));
}

#[test]
fn eval_buffer_preserves_utf8_bom_reader_error_shape() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("\u{feff}(setq lread-eb-bom 'ok)\n");
    }
    let result = builtin_eval_buffer(&mut ev, vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "void-variable" && sig.data.len() == 1
    ));
}

#[test]
fn eval_buffer_uses_source_text_without_switching_current() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let target = ev.buffers.create_buffer("*lread-eval-buffer-target*");
    {
        let target_buf = ev.buffers.get_mut(target).expect("target buffer");
        target_buf.insert("(setq lread-eb-current-name (buffer-name))");
    }
    let caller = ev.buffers.create_buffer("*lread-eval-buffer-caller*");
    ev.buffers.set_current(caller);

    let result = builtin_eval_buffer(&mut ev, vec![Value::make_buffer(target)]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-eb-current-name").cloned(),
        Some(Value::string("*lread-eval-buffer-caller*"))
    );
}

#[test]
fn eval_buffer_restores_current_buffer_after_source_switches_buffer() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let target = ev
        .buffers
        .create_buffer("*lread-eval-buffer-switch-source*");
    {
        let target_buf = ev.buffers.get_mut(target).expect("target buffer");
        target_buf.insert("(set-buffer (get-buffer-create \"*lread-eval-buffer-switched*\"))");
    }
    let caller = ev.buffers.create_buffer("*lread-eval-buffer-caller*");
    ev.buffers.set_current(caller);

    let result = builtin_eval_buffer(&mut ev, vec![Value::make_buffer(target)]).unwrap();

    assert!(result.is_nil());
    assert_eq!(ev.buffers.current_buffer_id(), Some(caller));
}

#[test]
fn eval_buffer_preserves_unibyte_filename_in_load_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-eb-current-load-list current-load-list)");
    }

    let filename =
        crate::heap_types::LispString::from_unibyte(vec![b'/', b't', b'm', b'p', b'/', 0xFF]);
    let result = builtin_eval_buffer(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::heap_string(filename.clone())],
    )
    .unwrap();
    assert!(result.is_nil());

    let current_load_list = ev
        .obarray
        .symbol_value("lread-eb-current-load-list")
        .copied()
        .expect("eval-buffer should capture current-load-list");
    let current_load_items =
        list_to_vec(&current_load_list).expect("current-load-list should be a list");
    assert_eq!(current_load_items.len(), 1);
    let current_load_name = current_load_items[0]
        .as_lisp_string()
        .expect("current-load-list filename should stay a Lisp string");
    assert_eq!(current_load_name.as_bytes(), filename.as_bytes());
    assert!(!current_load_name.is_multibyte());

    let load_history = ev
        .obarray
        .symbol_value("load-history")
        .copied()
        .expect("eval-buffer should update load-history");
    let load_history_entries = list_to_vec(&load_history).expect("load-history should be a list");
    let first_entry = load_history_entries
        .first()
        .copied()
        .expect("load-history should contain one entry");
    let load_history_name_value = first_entry.cons_car();
    let load_history_name = load_history_name_value
        .as_lisp_string()
        .expect("load-history filename should stay a Lisp string");
    assert_eq!(load_history_name.as_bytes(), filename.as_bytes());
    assert!(!load_history_name.is_multibyte());
}

#[test]
fn eval_buffer_load_in_progress_preserves_unibyte_current_load_list() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("load-in-progress", Value::T);
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert(
            "(setq lread-eb-load-file-name load-file-name \
                   lread-eb-load-true-file-name load-true-file-name \
                   lread-eb-current-load-list current-load-list)",
        );
    }

    let filename = crate::heap_types::LispString::from_unibyte(vec![
        b'/', b't', b'm', b'p', b'/', 0xFF, b'.', b'e', b'l',
    ]);
    let result = builtin_eval_buffer(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::heap_string(filename.clone())],
    )
    .unwrap();
    assert!(result.is_truthy());

    for symbol_name in ["lread-eb-load-file-name", "lread-eb-load-true-file-name"] {
        let value = ev
            .obarray
            .symbol_value(symbol_name)
            .copied()
            .unwrap_or_else(|| panic!("eval-buffer should bind {symbol_name}"));
        assert!(
            value.is_nil(),
            "{symbol_name} should remain nil for GNU-shaped eval-buffer loads"
        );
    }

    let current_load_list = ev
        .obarray
        .symbol_value("lread-eb-current-load-list")
        .copied()
        .expect("eval-buffer should capture current-load-list");
    let current_load_items =
        list_to_vec(&current_load_list).expect("current-load-list should be a list");
    assert_eq!(current_load_items.len(), 1);
    let current_load_name = current_load_items[0]
        .as_lisp_string()
        .expect("current-load-list filename should stay a Lisp string");
    assert_eq!(current_load_name.as_bytes(), filename.as_bytes());
    assert!(!current_load_name.is_multibyte());
}

#[test]
fn eval_buffer_reports_designator_and_arity_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let missing = builtin_eval_buffer(&mut ev, vec![Value::string("*no-such-buffer*")]);
    assert!(matches!(
        missing,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error" && sig.data == vec![Value::string("No such buffer")]
    ));

    let bad_type = builtin_eval_buffer(&mut ev, vec![Value::fixnum(1)]);
    assert!(matches!(
        bad_type,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("stringp"), Value::fixnum(1)]
    ));

    let arity = builtin_eval_buffer(
        &mut ev,
        vec![
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        arity,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data == vec![Value::symbol("eval-buffer"), Value::fixnum(6)]
    ));
}

#[test]
fn eval_region_evaluates_forms_in_range() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-er-a 1)\n(setq lread-er-b (+ lread-er-a 2))");
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };

    let result = builtin_eval_region(&mut ev, vec![Value::fixnum(1), end]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-er-a").cloned(),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        ev.obarray.symbol_value("lread-er-b").cloned(),
        Some(Value::fixnum(3))
    );
}

#[test]
fn eval_region_uses_read_function_result_instead_of_source_text() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(fset 'lread-er-read-function
                 (lambda (stream)
                   (setq lread-er-read-function-got-buffer (bufferp stream))
                   (goto-char (point-max))
                   '(setq lread-er-read-function-result 'transformed)))"#,
    )
    .expect("define eval-region read function");
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-er-read-function-result 'source)");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };

    let result = builtin_eval_region(
        &mut ev,
        vec![
            Value::fixnum(1),
            end,
            Value::NIL,
            Value::symbol("lread-er-read-function"),
        ],
    )
    .expect("eval-region with read function");

    assert!(result.is_nil());
    assert_eq!(
        ev.obarray
            .symbol_value("lread-er-read-function-result")
            .copied(),
        Some(Value::symbol("transformed"))
    );
    assert_eq!(
        ev.obarray
            .symbol_value("lread-er-read-function-got-buffer")
            .copied(),
        Some(Value::T)
    );
    assert_eq!(
        ev.buffers
            .current_buffer()
            .expect("current buffer")
            .point_lisp_char_pos()
            .as_i64(),
        1,
        "eval-region preserves point"
    );
}

#[test]
fn eval_region_advances_from_reader_point_before_evaluating_returned_form() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq lread-er-point-reader-count 0)
             (fset 'lread-er-point-reader
                   (lambda (stream)
                     (setq lread-er-point-reader-count
                           (1+ lread-er-point-reader-count))
                     (let ((form (read stream)))
                       (list 'progn
                             '(goto-char (point-min))
                             form)))))"#,
    )
    .expect("define point-moving eval-region reader");
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-er-point-a 1)\n(setq lread-er-point-b 2)");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };

    let eval_result = builtin_eval_region(
        &mut ev,
        vec![
            Value::fixnum(1),
            end,
            Value::NIL,
            Value::symbol("lread-er-point-reader"),
        ],
    );
    if let Err(flow) = eval_result {
        match flow {
            Flow::Signal(signal) => panic!(
                "eval-region must keep the reader's stream point across evaluation: {} {:?}",
                signal.symbol_name(),
                signal.data
            ),
            other => panic!(
                "eval-region must keep the reader's stream point across evaluation: {other:?}"
            ),
        }
    }

    assert_eq!(
        ev.obarray.symbol_value("lread-er-point-a").copied(),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        ev.obarray.symbol_value("lread-er-point-b").copied(),
        Some(Value::fixnum(2))
    );
    assert_eq!(
        ev.obarray
            .symbol_value("lread-er-point-reader-count")
            .copied(),
        Some(Value::fixnum(2))
    );
    assert_eq!(
        ev.buffers
            .current_buffer()
            .expect("current buffer")
            .point_lisp_char_pos()
            .as_i64(),
        1,
        "eval-region preserves point even when returned forms move it"
    );
}

#[test]
fn eval_region_preserves_unibyte_string_literals() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            b'(', b's', b'e', b't', b'q', b' ', b'l', b'r', b'e', b'a', b'd', b'-', b'e', b'r',
            b'-', b'r', b'a', b'w', b' ', b'"', 0xFF, b'"', b')',
        ]));
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };

    let result = builtin_eval_region(&mut ev, vec![Value::fixnum(1), end]).unwrap();
    assert!(result.is_nil());
    let value = ev
        .obarray
        .symbol_value("lread-er-raw")
        .copied()
        .expect("setq should bind lread-er-raw");
    let text = value
        .as_lisp_string()
        .expect("setq target should be a string");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn eval_region_nil_or_reversed_bounds_are_noop() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-er-noop 9)");
    }
    ev.obarray
        .set_symbol_value("lread-er-noop", Value::fixnum(0));

    let nil_bounds = builtin_eval_region(&mut ev, vec![Value::NIL, Value::NIL]).unwrap();
    assert!(nil_bounds.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-er-noop").cloned(),
        Some(Value::fixnum(0))
    );

    let point_max = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        buf.z_lisp_char_pos().as_i64()
    };
    let reversed =
        builtin_eval_region(&mut ev, vec![Value::fixnum(point_max), Value::fixnum(1)]).unwrap();
    assert!(reversed.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-er-noop").cloned(),
        Some(Value::fixnum(0))
    );
}

#[test]
fn eval_region_accepts_marker_bounds_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-er-marker 17)");
    }
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    let start = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut ev.buffers,
        buffer_id,
        crate::buffer::LispCharPos1::new(1),
        false,
    );
    let end = {
        let point_max = ev
            .buffers
            .current_buffer()
            .expect("current buffer")
            .z_lisp_char_pos();
        crate::emacs_core::marker::make_registered_buffer_marker(
            &mut ev.buffers,
            buffer_id,
            point_max,
            false,
        )
    };

    let result = builtin_eval_region(&mut ev, vec![start, end]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-er-marker").cloned(),
        Some(Value::fixnum(17))
    );
}

#[test]
fn eval_region_reports_type_range_and_arity_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(+ 1 2)");
    }
    let point_max = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        buf.z_lisp_char_pos().as_i64()
    };

    let bad_start =
        builtin_eval_region(&mut ev, vec![Value::string("1"), Value::fixnum(point_max)]);
    assert!(matches!(
        bad_start,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data
                    == vec![Value::symbol("integer-or-marker-p"), Value::string("1")]
    ));

    let bad_end = builtin_eval_region(&mut ev, vec![Value::fixnum(1), Value::string("2")]);
    assert!(matches!(
        bad_end,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data
                    == vec![Value::symbol("integer-or-marker-p"), Value::string("2")]
    ));

    let big = Value::make_integer(Integer::from(1u64) << 100u32);
    let bad_big = builtin_eval_region(&mut ev, vec![big, Value::fixnum(point_max)]);
    assert!(matches!(
        bad_big,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data.first() == Some(&Value::symbol("integer-or-marker-p"))
    ));

    let range = builtin_eval_region(&mut ev, vec![Value::fixnum(1), Value::fixnum(999)]);
    assert!(matches!(
        range,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "args-out-of-range"
                && sig.data == vec![Value::fixnum(1), Value::fixnum(999)]
    ));

    let arity_low = builtin_eval_region(&mut ev, vec![]);
    assert!(matches!(
        arity_low,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data == vec![Value::symbol("eval-region"), Value::fixnum(0)]
    ));

    let arity_high = builtin_eval_region(
        &mut ev,
        vec![
            Value::fixnum(1),
            Value::fixnum(point_max),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        arity_high,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data == vec![Value::symbol("eval-region"), Value::fixnum(5)]
    ));
}

#[test]
fn eval_region_keeps_point_stable_without_side_effects() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("(setq lread-er-point 1)");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };
    let result = builtin_eval_region(&mut ev, vec![Value::fixnum(1), end]).unwrap();
    assert!(result.is_nil());
    let point = ev
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point, 1);
}

#[test]
fn eval_region_accepts_shebang_reader_prefix() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("#!/usr/bin/env emacs --script\n(setq lread-er-shebang 'ok)\n");
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };
    let result = builtin_eval_region(&mut ev, vec![Value::fixnum(1), end]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("lread-er-shebang").cloned(),
        Some(Value::symbol("ok"))
    );
}

#[test]
fn eval_region_single_line_shebang_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("#!/usr/bin/env emacs --script");
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };
    let result = builtin_eval_region(&mut ev, vec![Value::fixnum(1), end]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty()
    ));
}

#[test]
fn eval_region_preserves_utf8_bom_reader_error_shape() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("\u{feff}(setq lread-er-bom 'ok)\n");
    }
    let end = {
        let buf = ev.buffers.current_buffer().expect("current buffer");
        Value::fixnum(buf.z_lisp_char_pos().as_i64())
    };
    let result = builtin_eval_region(&mut ev, vec![Value::fixnum(1), end]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "void-variable" && sig.data.len() == 1
    ));
}

#[test]
fn read_event_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_event(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn read_event_without_inherited_input_method_preserves_raw_tty_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(
        "é".as_bytes().to_vec(),
        0,
    ))
    .expect("queue UTF-8 terminal bytes");

    let first = builtin_read_event(&mut ev, vec![]).expect("read first raw terminal event");
    let second = builtin_read_event(&mut ev, vec![]).expect("read second raw terminal event");

    assert_eq!([first, second], [Value::fixnum(0xc3), Value::fixnum(0xa9)]);
}

#[test]
fn read_event_with_inherited_input_method_decodes_tty_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(
        "é".as_bytes().to_vec(),
        0,
    ))
    .expect("queue UTF-8 terminal bytes");

    let event = builtin_read_event(&mut ev, vec![Value::NIL, Value::T])
        .expect("read keyboard-decoded terminal event");

    assert_eq!(event, Value::fixnum('é' as i64));
}

#[test]
fn read_event_consumes_executing_keyboard_macro_event_without_input_receiver() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.begin_executing_kbd_macro_runtime(vec![Value::char('a')]);

    let result = builtin_read_event(&mut ev, vec![]).expect("read-event");

    assert_eq!(result, Value::fixnum('a' as i64));
    assert_eq!(ev.read_command_keys(), &[Value::char('a')]);
}

#[test]
fn read_event_rejects_non_string_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_event(&mut ev, vec![Value::fixnum(123)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_event_consumes_unread_command_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_event(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.recent_input_events(), &[Value::fixnum(97)]);
}

#[test]
fn read_event_sets_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let _ = builtin_read_event(&mut ev, vec![]).unwrap();
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_event_preserves_existing_command_keys_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::fixnum(97)]);
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::list(vec![Value::symbol("mouse-1")])]),
    );
    let result = builtin_read_event(&mut ev, vec![]).unwrap();
    assert!(result.is_cons());
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_event_echoes_invoking_keys_and_new_event_after_gnu_delay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.assign("noninteractive", Value::NIL);
    ev.assign("echo-keystrokes", Value::make_float(0.005));
    ev.assign("echo-keystrokes-help", Value::T);
    ev.assign("help-char", Value::fixnum(8));
    ev.assign("help-event-list", Value::list(vec![Value::symbol("help")]));
    ev.set_read_command_keys(vec![Value::fixnum(17)]); // C-q

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let notifier = ev.wait_notifier();
    let sender = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(30));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('1'),
        ))
        .expect("send event after the echo delay");
        if let Some(notifier) = notifier {
            notifier.notify().expect("wake read-event input wait");
        }
    });

    let event = builtin_read_event(&mut ev, vec![]).expect("read-event");
    sender.join().expect("input sender");

    assert_eq!(event, Value::fixnum('1' as i64));
    assert_eq!(
        ev.current_message_text().as_deref(),
        Some("C-q 1- (C-h for help)")
    );
}

#[test]
fn read_event_with_seconds_does_not_set_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let _ = builtin_read_event(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(0)]).unwrap();
    assert_eq!(ev.read_command_keys(), &[]);
}

#[test]
fn read_event_with_positive_seconds_does_not_set_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let _ = builtin_read_event(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(1)]).unwrap();
    assert_eq!(ev.read_command_keys(), &[]);
}

#[test]
fn read_event_with_float_seconds_does_not_set_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let _ = builtin_read_event(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::make_float(0.25)],
    )
    .unwrap();
    assert_eq!(ev.read_command_keys(), &[]);
}

#[test]
fn read_event_with_interactive_timeout_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    let start = std::time::Instant::now();
    let result = builtin_read_event(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::make_float(0.01)],
    )
    .unwrap();
    drop(tx);

    assert!(result.is_nil());
    assert!(start.elapsed() < std::time::Duration::from_millis(250));
}

#[test]
fn read_event_with_timeout_waits_without_input_source() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let start = std::time::Instant::now();
    let result = builtin_read_event(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::make_float(0.02)],
    )
    .unwrap();

    assert!(result.is_nil());
    assert!(start.elapsed() >= std::time::Duration::from_millis(10));
    assert!(start.elapsed() < std::time::Duration::from_millis(500));
}

#[test]
fn read_event_with_non_nil_seconds_preserves_existing_command_keys_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::fixnum(97)]);
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(98)]),
    );
    let _ = builtin_read_event(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::make_float(0.25)],
    )
    .unwrap();
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_event_with_nil_seconds_sets_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let _ = builtin_read_event(&mut ev, vec![Value::NIL, Value::NIL, Value::NIL]).unwrap();
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_event_consumes_non_character_event_and_preserves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo"), Value::fixnum(97)]),
    );
    let result = builtin_read_event(&mut ev, vec![]).unwrap();
    assert_eq!(result, Value::symbol("foo"));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(97)]))
    );
}

#[test]
fn read_event_consumes_character_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("unread-command-events", Value::list(vec![Value::char('a')]));
    let result = builtin_read_event(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::NIL)
    );
}

#[test]
fn read_event_preserves_trailing_events_after_non_character() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo"), Value::char('a')]),
    );
    let result = builtin_read_event(&mut ev, vec![]).unwrap();
    assert_eq!(result, Value::symbol("foo"));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::char('a')]))
    );
}

#[test]
fn read_event_rejects_more_than_three_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_event(
        &mut ev,
        vec![
            Value::string("key: "),
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_char_exclusive_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_char_exclusive(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn read_char_exclusive_consumes_executing_keyboard_macro_event_without_input_receiver() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.begin_executing_kbd_macro_runtime(vec![Value::char('a')]);

    let result = builtin_read_char_exclusive(&mut ev, vec![]).expect("read-char-exclusive");

    assert_eq!(result, Value::fixnum('a' as i64));
    assert_eq!(ev.read_command_keys(), &[Value::char('a')]);
}

#[test]
fn read_char_exclusive_rejects_non_string_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_char_exclusive(&mut ev, vec![Value::fixnum(123)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_char_exclusive_consumes_unread_command_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_char_exclusive(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_char_exclusive_with_seconds_does_not_set_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result =
        builtin_read_char_exclusive(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(0)])
            .unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.read_command_keys(), &[]);
}

#[test]
fn read_char_exclusive_with_timeout_waits_without_input_source() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let start = std::time::Instant::now();
    let result = builtin_read_char_exclusive(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::make_float(0.02)],
    )
    .unwrap();

    assert!(result.is_nil());
    assert!(start.elapsed() >= std::time::Duration::from_millis(10));
    assert!(start.elapsed() < std::time::Duration::from_millis(500));
}

#[test]
fn read_char_exclusive_with_nil_seconds_sets_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result =
        builtin_read_char_exclusive(&mut ev, vec![Value::NIL, Value::NIL, Value::NIL]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_char_exclusive_preserves_existing_command_keys_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::fixnum(97)]);
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(98)]),
    );
    let result =
        builtin_read_char_exclusive(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(0)])
            .unwrap();
    assert_eq!(result.as_int(), Some(98));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_char_exclusive_rejects_more_than_three_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_char_exclusive(
        &mut ev,
        vec![
            Value::string("key: "),
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_char_exclusive_skips_non_character_events() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo"), Value::fixnum(97)]),
    );
    let result = builtin_read_char_exclusive(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(
        ev.recent_input_events(),
        &[Value::symbol("foo"), Value::fixnum(97)]
    );
}

#[test]
fn read_char_exclusive_skips_non_character_and_empty_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo"), Value::fixnum(97)]),
    );
    let result =
        builtin_read_char_exclusive(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(0)])
            .unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::NIL),
    );
}

#[test]
fn read_char_exclusive_skips_non_character_and_leaves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![
            Value::symbol("foo"),
            Value::fixnum(97),
            Value::fixnum(98),
        ]),
    );
    let result =
        builtin_read_char_exclusive(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(0)])
            .unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(98)])),
    );
}

#[test]
fn get_load_suffixes_returns_list() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    // GNU `Fget_load_suffixes` cross-products `load-suffixes` with
    // `load-file-rep-suffixes`.  The empty representation suffix extends each
    // load suffix; it is not returned as its own suffix.
    let result = builtin_get_load_suffixes(&ev.obarray, vec![]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].as_utf8_str(), Some(module_file_suffix()));
    assert_eq!(items[1].as_utf8_str(), Some(".elc"));
    assert_eq!(items[2].as_utf8_str(), Some(".el"));
}

#[test]
fn get_load_suffixes_cross_products_rep_suffixes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "load-file-rep-suffixes",
        Value::list(vec![Value::string(""), Value::string(".gz")]),
    );
    ev.obarray.set_symbol_value(
        "jka-compr-load-suffixes",
        Value::list(vec![Value::string(".gz")]),
    );

    let result = builtin_get_load_suffixes(&ev.obarray, vec![]).unwrap();
    let rendered = list_to_vec(&result)
        .unwrap()
        .into_iter()
        .map(|v| v.as_utf8_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![module_file_suffix(), ".elc", ".elc.gz", ".el", ".el.gz"]
    );
}

#[test]
fn get_load_suffixes_returns_nil_when_representation_suffixes_are_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("load-file-rep-suffixes", Value::NIL);

    let result = builtin_get_load_suffixes(&ev.obarray, vec![]).unwrap();

    assert!(result.is_nil());
}

#[test]
fn get_load_suffixes_ignores_irrelevant_non_string_jka_members() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("load-suffixes", Value::list(vec![Value::string(".el")]));
    ev.obarray.set_symbol_value(
        "load-file-rep-suffixes",
        Value::list(vec![Value::string("")]),
    );
    ev.obarray.set_symbol_value(
        "jka-compr-load-suffixes",
        Value::list(vec![Value::symbol("not-a-string")]),
    );

    let result = builtin_get_load_suffixes(&ev.obarray, vec![])
        .expect("GNU only consults jka members for represented module suffixes");

    assert_eq!(
        list_to_vec(&result)
            .expect("suffix list")
            .into_iter()
            .map(|value| value.as_utf8_str().expect("string suffix").to_owned())
            .collect::<Vec<_>>(),
        vec![".el"]
    );
}

#[test]
fn get_load_suffixes_rejects_over_arity() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    let result = builtin_get_load_suffixes(&ev.obarray, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn locate_file_finds_first_matching_suffix() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-locate-file-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(dir.join("probe.el"), "(setq vm-locate 1)\n").expect("write .el");
    fs::write(dir.join("probe.elc"), "compiled").expect("write .elc");

    let result = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el"), Value::string(".elc")]),
        ],
    )
    .expect("locate-file should succeed");
    let found = result
        .as_utf8_str()
        .expect("locate-file should return path");
    assert!(
        found.ends_with("probe.el"),
        "expected first matching suffix (.el), got {found}",
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn locate_file_respects_symbol_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-locate-file-predicate-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(dir.join("probe.el"), "(setq vm-locate 1)\n").expect("write .el");

    let regular = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el")]),
            Value::symbol("file-regular-p"),
        ],
    )
    .expect("locate-file with file-regular-p should evaluate");
    assert!(
        regular.as_utf8_str().is_some(),
        "regular-file predicate should accept candidate",
    );

    let directory = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el")]),
            Value::symbol("file-directory-p"),
        ],
    )
    .expect("locate-file with file-directory-p should evaluate");
    assert!(directory.is_nil(), "directory predicate should reject file");

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn locate_file_respects_integer_access_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-locate-file-access-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    let script = dir.join("script");
    let data = dir.join("data");
    fs::write(&script, "#!/bin/sh\nexit 0\n").expect("write script");
    fs::write(&data, "not executable\n").expect("write data");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o644)).unwrap();

    let executable = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("script"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::NIL,
            Value::fixnum(1),
        ],
    )
    .expect("locate-file with access predicate should evaluate");
    assert!(
        executable.as_utf8_str().is_some(),
        "X_OK predicate should accept executable files"
    );

    let non_executable = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("data"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::NIL,
            Value::fixnum(1),
        ],
    )
    .expect("locate-file with access predicate should evaluate");
    assert!(
        non_executable.is_nil(),
        "X_OK predicate should reject non-executable files"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn locate_file_treats_invalid_path_entries_as_no_match() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-locate-file-path-edge-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(dir.join("probe"), "x\n").expect("write probe");

    let non_list_path = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::fixnum(42),
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("locate-file should evaluate");
    assert!(non_list_path.is_nil());

    let invalid_entry_path = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::fixnum(42)]),
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("locate-file should evaluate");
    assert!(invalid_entry_path.is_nil());

    let mixed_path = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![
                Value::fixnum(42),
                Value::string(dir.to_string_lossy()),
            ]),
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("locate-file should evaluate");
    assert!(mixed_path.as_utf8_str().is_some());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn locate_file_unknown_predicate_defaults_to_truthy_match() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-locate-file-bad-predicate-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(dir.join("probe.el"), "(setq vm-locate 1)\n").expect("write .el");

    let result = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el")]),
            Value::symbol("definitely-not-a-real-predicate"),
        ],
    )
    .expect("locate-file should evaluate");
    let found = result
        .as_utf8_str()
        .expect("unknown predicate should not prevent match");
    assert!(found.ends_with("probe.el"), "unexpected result: {found}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn locate_file_internal_returns_nil_when_missing() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    let result = builtin_locate_file_internal(
        &mut ctx,
        vec![
            Value::string("definitely-missing-neovm-file"),
            Value::list(vec![Value::string(".")]),
            Value::list(vec![Value::string(".el")]),
        ],
    )
    .expect("locate-file-internal should evaluate");
    assert!(result.is_nil());
}

#[test]
fn locate_file_internal_finds_requested_suffix() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-locate-file-internal-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(dir.join("probe.elc"), "compiled").expect("write .elc");

    let result = builtin_locate_file_internal(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".elc")]),
        ],
    )
    .expect("locate-file-internal should succeed");
    let found = result
        .as_utf8_str()
        .expect("locate-file-internal should return path");
    assert!(
        found.ends_with("probe.elc"),
        "expected .elc resolution, got {found}",
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn locate_file_internal_rejects_tilde_prefixed_directory_without_dir_ok() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let home = std::env::var("HOME").expect("HOME must exist for locate-file tilde test");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::path::Path::new(&home).join(format!("neovm-locate-file-home-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir in HOME");

    let tilde_name = format!(
        "~/{}",
        dir.file_name()
            .expect("temp dir basename")
            .to_string_lossy()
    );

    let result = builtin_locate_file_internal(
        &mut ctx,
        vec![
            Value::string(&tilde_name),
            Value::list(vec![Value::string("./")]),
            Value::NIL,
            Value::symbol("file-directory-p"),
        ],
    )
    .expect("locate-file-internal tilde path should evaluate");

    assert!(
        result.is_nil(),
        "GNU openp rejects directories unless the predicate returns dir-ok"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn locate_file_accepts_raw_unibyte_path_and_suffix_lists() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    let raw_path = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let raw_suffix = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFE]));

    let result = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![raw_path]),
            Value::list(vec![raw_suffix]),
        ],
    )
    .unwrap();

    assert!(result.is_nil());
}

#[test]
fn locate_file_rejects_over_arity() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    let result = builtin_locate_file(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::string(".")]),
            Value::list(vec![Value::string(".el")]),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn locate_file_internal_rejects_over_arity() {
    crate::test_utils::init_test_tracing();
    let mut ctx = test_eval_ctx();
    let result = builtin_locate_file_internal(
        &mut ctx,
        vec![
            Value::string("probe"),
            Value::list(vec![Value::string(".")]),
            Value::list(vec![Value::string(".el")]),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_coding_system_signals_batch_eof() {
    crate::test_utils::init_test_tracing();
    // Mirrors GNU: `Fread_coding_system` -> `Fcompleting_read` ->
    // `read_minibuf_noninteractive`, which on empty stdin signals end-of-file
    // (the harness redirects stdin from /dev/null under test).
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_read_coding_system(&mut eval, vec![Value::string("")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "end-of-file"
                && sig.data == vec![Value::string("Error reading from stdin")]
    ));
}

#[test]
fn read_coding_system_validates_prompt_type_and_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let bad_prompt = builtin_read_coding_system(&mut eval, vec![Value::fixnum(1)]);
    assert!(matches!(
        bad_prompt,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("stringp"), Value::fixnum(1)]
    ));

    let arity =
        builtin_read_coding_system(&mut eval, vec![Value::string(""), Value::NIL, Value::NIL]);
    assert!(matches!(
        arity,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data == vec![Value::symbol("read-coding-system"), Value::fixnum(3)]
    ));
}

#[test]
fn read_non_nil_coding_system_signals_batch_eof() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_read_non_nil_coding_system(&mut eval, vec![Value::string("")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "end-of-file"
                && sig.data == vec![Value::string("Error reading from stdin")]
    ));
}

#[test]
fn read_non_nil_coding_system_validates_prompt_type_and_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let bad_prompt = builtin_read_non_nil_coding_system(&mut eval, vec![Value::fixnum(1)]);
    assert!(matches!(
        bad_prompt,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("stringp"), Value::fixnum(1)]
    ));

    let arity = builtin_read_non_nil_coding_system(&mut eval, vec![Value::string(""), Value::NIL]);
    assert!(matches!(
        arity,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data
                    == vec![Value::symbol("read-non-nil-coding-system"), Value::fixnum(2)]
    ));
}

// ---------------------------------------------------------------------------
// Dynamic-module suffixes (neomacs#193)
// ---------------------------------------------------------------------------

#[test]
fn module_suffixes_match_gnu_configure_per_operating_system() {
    // GNU configure.ac:
    //   case $opsys in
    //     cygwin|mingw32) DYNAMIC_LIB_SUFFIX=".dll" ;;
    //     darwin)         DYNAMIC_LIB_SUFFIX=".dylib" ;;
    //     *)              DYNAMIC_LIB_SUFFIX=".so" ;;
    //   esac
    //   case "${opsys}" in
    //     darwin) DYNAMIC_LIB_SECONDARY_SUFFIX='.so' ;;
    //     *)      DYNAMIC_LIB_SECONDARY_SUFFIX='' ;;
    //   esac
    //
    // macOS is the only platform with a SECONDARY suffix, and that is what
    // vterm needs: its build writes `vterm-module.so` even there (#193).
    assert_eq!(
        module_suffixes_for_os("macos"),
        ModuleSuffixes {
            primary: ".dylib",
            secondary: Some(".so"),
        }
    );
    assert_eq!(
        module_suffixes_for_os("windows"),
        ModuleSuffixes {
            primary: ".dll",
            secondary: None,
        }
    );
    for os in ["linux", "freebsd", "openbsd", "netbsd", "android"] {
        assert_eq!(
            module_suffixes_for_os(os),
            ModuleSuffixes {
                primary: ".so",
                secondary: None,
            },
            "{os} should use the default suffix"
        );
    }
}

#[test]
fn load_suffixes_put_the_secondary_module_suffix_first_like_gnu() {
    // GNU seeds `load-suffixes' as (".elc" ".el"), conses MODULES_SUFFIX, then
    // conses MODULES_SECONDARY_SUFFIX -- so on darwin the secondary lands FIRST.
    // Verified against GNU 31 on linux: (".so" ".elc" ".el").
    assert_eq!(
        load_suffixes_startup_values_for_os("macos"),
        vec![".so", ".dylib", ".elc", ".el"]
    );
    assert_eq!(
        load_suffixes_startup_values_for_os("linux"),
        vec![".so", ".elc", ".el"]
    );
    assert_eq!(
        load_suffixes_startup_values_for_os("windows"),
        vec![".dll", ".elc", ".el"]
    );
}

#[test]
fn dynamic_library_suffixes_always_carry_the_secondary_slot_like_gnu() {
    // GNU conses DYNAMIC_LIB_SECONDARY_SUFFIX unconditionally, so the empty
    // secondary shows up as "": verified against GNU 31 on linux, which reports
    // (".so" "").
    assert_eq!(
        dynamic_library_suffixes_for_os("macos"),
        vec![".dylib", ".so"]
    );
    assert_eq!(dynamic_library_suffixes_for_os("linux"), vec![".so", ""]);
    assert_eq!(dynamic_library_suffixes_for_os("windows"), vec![".dll", ""]);
}

#[test]
fn a_secondary_suffixed_file_is_still_a_module_on_darwin() {
    // GNU `load' decides between Lisp and a module with
    //   suffix_p (found, MODULES_SUFFIX) || suffix_p (found, MODULES_SECONDARY_SUFFIX)
    // (src/lread.c).  Testing only the primary suffix means a `vterm-module.so'
    // found on macOS would be read as Lisp instead of dlopen'd.
    assert!(path_has_module_suffix_for_os(
        "/tmp/vterm-module.dylib",
        "macos"
    ));
    assert!(path_has_module_suffix_for_os(
        "/tmp/vterm-module.so",
        "macos"
    ));
    assert!(!path_has_module_suffix_for_os("/tmp/vterm.el", "macos"));

    assert!(path_has_module_suffix_for_os("/tmp/mod.so", "linux"));
    // A `.dylib' is not a module on GNU/Linux: it is not in that platform's
    // suffix set at all.
    assert!(!path_has_module_suffix_for_os("/tmp/mod.dylib", "linux"));
    assert!(path_has_module_suffix_for_os("C:\\mod.dll", "windows"));
}

#[test]
fn the_running_platform_uses_its_own_suffixes() {
    // The compile-time answer must agree with the table above for THIS build,
    // which is what the macOS CI runner checks.
    let expected = module_suffixes_for_os(std::env::consts::OS);
    assert_eq!(module_file_suffix(), expected.primary);
    assert_eq!(module_suffixes().secondary, expected.secondary);

    #[cfg(target_os = "macos")]
    {
        assert_eq!(module_file_suffix(), ".dylib");
        assert_eq!(module_suffixes().secondary, Some(".so"));
    }
    #[cfg(target_os = "linux")]
    {
        assert_eq!(module_file_suffix(), ".so");
        assert_eq!(module_suffixes().secondary, None);
    }
}

// ---------------------------------------------------------------------------
// `eval-buffer' / `eval-region' must read through `load-read-function'
// ---------------------------------------------------------------------------

/// Define a counting reader that records, per call, whether it was handed the
/// buffer that is current at the time of the call.  Edebug's `edebug--read'
/// makes exactly that test (`(eq stream (current-buffer))',
/// lisp/emacs-lisp/edebug.el:457) before it instruments anything, so a fixture
/// that only counted calls would not pin what Edebug actually needs.
///
/// `load-read-function' is a dynamic binding, so nested file loads triggered
/// from inside the region under test legitimately go through it too -- GNU does
/// the same, and an unfiltered counter reads 124 instead of 2 there.  Loading a
/// source file goes through `load-source-file-function', i.e. `eval-buffer' on
/// a work buffer, in both editors, so filtering on "is a buffer" is not enough
/// either; the buffer under test carries a name only this fixture uses.  All
/// expectations below were taken by running this same fixture under GNU.
fn define_stream_recording_reader(eval: &mut Context) {
    eval.eval_str(
        "(progn (defvar lrf-eb-calls 0)
                (defvar lrf-eb-stream-is-current-buffer nil)
                (defun lrf-eb-read (&optional stream)
                  (when (and (bufferp stream)
                             (string-prefix-p \" lrf-eb-target\"
                                              (buffer-name stream)))
                    (setq lrf-eb-calls (1+ lrf-eb-calls))
                    (push (and (eq stream (current-buffer)) t)
                          lrf-eb-stream-is-current-buffer))
                  (read stream)))",
    )
    .expect("define the counting reader");
}

#[test]
fn eval_buffer_reads_each_form_through_load_read_function() {
    // GNU `Feval_buffer' calls `readevalloop' with the BUFFER as readcharfun
    // (src/lread.c:2417), and `readevalloop' reads every top-level form through
    // `load-read-function' when it is non-nil:
    //     else if (! NILP (Vload_read_function))
    //       val = calln (Vload_read_function, readcharfun);   (src/lread.c:2317-2318)
    //
    // Edebug installs itself exactly there --
    //     (add-function :around load-read-function #'edebug--read)
    // (lisp/emacs-lisp/edebug.el:556, run unconditionally at edebug.el:4632) --
    // and instruments only when the stream it is handed is the current buffer
    // (edebug.el:457).  An `eval-buffer' that reads the buffer text internally
    // never calls the hook, so `edebug-all-defs' silently does nothing.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    define_stream_recording_reader(&mut eval);

    eval.eval_str(
        "(with-temp-buffer
           (rename-buffer \" lrf-eb-target\" t)
           (insert \"(setq lrf-eb-one 1)\\n(setq lrf-eb-two 2)\\n\")
           (let ((load-read-function #'lrf-eb-read)) (eval-buffer)))",
    )
    .expect("eval-buffer with the hook installed");

    assert_eq!(
        eval.eval_str("lrf-eb-calls")
            .expect("call count")
            .to_string(),
        "2",
        "`eval-buffer' must read each top-level form through `load-read-function'"
    );
    assert_eq!(
        eval.eval_str("lrf-eb-stream-is-current-buffer")
            .expect("recorded streams")
            .to_string(),
        "(t t)",
        "the hook must be handed the buffer being evaluated, and that buffer \
         must be current -- Edebug instruments on exactly that identity"
    );
    for (var, want) in [("lrf-eb-one", "1"), ("lrf-eb-two", "2")] {
        assert_eq!(
            eval.eval_str(var).expect("evaluated variable").to_string(),
            want,
            "the forms the hook returned must still be evaluated"
        );
    }
}

#[test]
fn eval_buffer_of_a_named_file_during_a_load_still_uses_load_read_function() {
    // Undercover's file handler evaluates the visited source file with
    //     (let ((edebug-all-defs t) (load-file-name ...) (load-in-progress t))
    //       (save-excursion (eval-buffer (find-file load-file-name))))
    // (undercover.el `undercover--load-file-handler'), so the buffer has a file
    // name and `load-in-progress' is t.  GNU takes no special branch for that:
    // `Feval_buffer' is one `readevalloop' over the buffer either way
    // (src/lread.c:2417).  Pin the hook on that shape too.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    define_stream_recording_reader(&mut eval);

    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("visited.el");
    std::fs::write(&file, "(setq lrf-eb-three 3)\n").expect("write file");

    let form = format!(
        "(let ((load-in-progress t) (load-file-name {path:?})
               (load-read-function #'lrf-eb-read))
           (with-temp-buffer
             (rename-buffer \" lrf-eb-target\" t)
             (insert-file-contents {path:?})
             (setq buffer-file-name {path:?})
             (unwind-protect (eval-buffer)
               (setq buffer-file-name nil))))",
        path = file.to_str().expect("utf8 path")
    );
    eval.eval_str(&form).expect("eval-buffer of a visited file");

    assert_eq!(
        eval.eval_str("lrf-eb-calls")
            .expect("call count")
            .to_string(),
        "1",
        "a file-visiting `eval-buffer' during a load must still consult the hook"
    );
    assert_eq!(
        eval.eval_str("lrf-eb-three")
            .expect("evaluated variable")
            .to_string(),
        "3"
    );
}

#[test]
fn eval_region_reads_through_load_read_function_when_no_read_function_is_passed() {
    // GNU `readevalloop' prefers an explicit READ-FUNCTION argument and falls
    // back to `load-read-function' (src/lread.c:2302-2318); `Feval_region'
    // passes the region's buffer as readcharfun (src/lread.c:2451).
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    define_stream_recording_reader(&mut eval);

    eval.eval_str(
        "(with-temp-buffer
           (rename-buffer \" lrf-eb-target\" t)
           (insert \"(setq lrf-er-one 1)\\n(setq lrf-er-two 2)\\n\")
           (let ((load-read-function #'lrf-eb-read))
             (eval-region (point-min) (point-max))))",
    )
    .expect("eval-region with the hook installed");

    assert_eq!(
        eval.eval_str("lrf-eb-calls")
            .expect("call count")
            .to_string(),
        "2",
        "`eval-region' must fall back to `load-read-function'"
    );
    for (var, want) in [("lrf-er-one", "1"), ("lrf-er-two", "2")] {
        assert_eq!(
            eval.eval_str(var).expect("evaluated variable").to_string(),
            want
        );
    }
}
