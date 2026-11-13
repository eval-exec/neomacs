use super::*;

use neovm_edit::buffer::{EmacsByteLen, EmacsBytePos, EmacsByteRange, TextExtent};
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_edit::emacs_core::editfns::{
    signal_after_text_change, signal_before_text_change,
    text_change_for_empty_insertion_at_emacs_byte_pos,
};
use neovm_text::emacs_core::text_host::TextHost;
use neovm_value::error::LispCondition;
use neovm_value::error::expect_min_args;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::intern::{SymId, resolve_sym};
use neovm_value::tagged::value::SymbolMode;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use strum::{EnumString, IntoStaticStr};

use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::json::*;
use neovm_value::value::Value;

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::heap_types::LispString;

use neovm_value::value::{ValueKind, VecLikeType};

#[test]
fn json_control_escapes_use_uppercase_hex_digits() {
    crate::test_utils::init_test_tracing();
    // GNU `json_out_string` (src/json.c) formats \u00XX from "0123456789ABCDEF".
    let mut eval = crate::test_utils::new_context();
    let rendered = neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(
        r#"(let ((value ["\x0b\x1a\x1f"]))
             (erase-buffer)
             (json-insert value)
             (list (json-serialize value) (buffer-string)))"#,
    ));
    assert_eq!(
        rendered,
        r#"OK ("[\"\\u000B\\u001A\\u001F\"]" "[\"\\u000B\\u001A\\u001F\"]")"#
    );
}

#[test]
fn json_serialize_hash_table_keys_follow_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU `json_out_object_hash` (src/json.c) does `CHECK_STRING (k)`, whatever
    // the table's test is, then writes the key with `json_out_string`.
    let mut eval = crate::test_utils::new_context();
    let mut serialize_with_key = |test: &str, key: &str| {
        neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&format!(
            r#"(let ((table (make-hash-table :test '{test})))
                 (puthash {key} 2 table)
                 (condition-case err
                     (json-serialize table)
                   (error err)))"#
        )))
    };

    for (key, printed) in [
        ("1", "1"),
        ("'a", "a"),
        (":k", ":k"),
        ("nil", "nil"),
        ("t", "t"),
    ] {
        assert_eq!(
            serialize_with_key("eq", key),
            format!("OK (wrong-type-argument stringp {printed})")
        );
    }
    assert_eq!(
        serialize_with_key("equal", "1"),
        "OK (wrong-type-argument stringp 1)"
    );
    for test in ["eq", "eql", "equal"] {
        assert_eq!(serialize_with_key(test, "\"k\""), r#"OK "{\"k\":2}""#);
    }
    assert_eq!(
        serialize_with_key("equal", r#""\xCC""#),
        r#"OK (wrong-type-argument json-value-p "\314")"#
    );
}

#[test]
fn json_serialize_hash_table_keys_ignore_value_sentinels() {
    crate::test_utils::init_test_tracing();
    // GNU writes hash keys with `json_out_string` directly, so `:null-object`
    // and `:false-object` only ever apply to values.
    let mut eval = crate::test_utils::new_context();
    for option in [":null-object", ":false-object"] {
        let rendered = neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&format!(
            r#"(let ((key "k") (table (make-hash-table :test 'equal)))
                 (puthash key 2 table)
                 (json-serialize table {option} key))"#
        )));
        assert_eq!(rendered, r#"OK "{\"k\":2}""#);
    }
    let rendered = neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(
        r#"(let ((key "k") (table (make-hash-table :test 'equal)))
             (puthash "a" key table)
             (json-serialize table :null-object key))"#,
    ));
    assert_eq!(rendered, r#"OK "{\"a\":null}""#);
}

#[test]
fn json_parse_buffer_advances_point_after_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(" 42 trailing");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    }

    let value = builtin_json_parse_buffer(&mut eval, vec![]).expect("parse buffer");
    assert_eq!(value, Value::fixnum(42));
    assert_eq!(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .point_emacs_byte_pos()
            .get(),
        3
    );
}

#[test]
fn json_parse_buffer_invalid_utf8_does_not_advance_point() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&LispString::from_unibyte(vec![
            b'"', 0xFF, b'"', b' ', b'x',
        ]));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    }

    match builtin_json_parse_buffer(&mut eval, vec![]).kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-utf8-decode-error");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(3)]
            );
        }
        other => panic!("expected json-utf8-decode-error, got {:?}", other),
    }
    assert_eq!(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .point_emacs_byte_pos()
            .get(),
        0
    );
}

#[test]
fn json_parse_buffer_end_of_file_uses_gnu_signal_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(" ");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    }

    match builtin_json_parse_buffer(&mut eval, vec![]).kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-end-of-file");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(1)]
            );
        }
        other => panic!("expected json-end-of-file signal, got {:?}", other),
    }
    assert_eq!(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .point_emacs_byte_pos()
            .get(),
        0
    );
}

#[test]
fn json_insert_writes_at_point_and_advances() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("ab");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(1));
    }

    builtin_json_insert(
        &mut eval,
        vec![Value::vector(vec![Value::fixnum(1), Value::T])],
    )
    .expect("json insert");

    let buf = eval.edit.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "a[1,true]b");
    assert_eq!(buf.point_emacs_byte_pos().get(), 9);
}

#[test]
fn json_insert_writes_utf8_bytes_into_unibyte_buffers() {
    crate::test_utils::init_test_tracing();
    // GNU `Fjson_insert` (src/json.c) inserts Unicode characters "as UTF-8 byte
    // sequences into unibyte buffers", so point and the change hook count bytes.
    let insert_between_a_and_z = |multibyte: &str| {
        let mut eval = crate::test_utils::new_context();
        neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&format!(
            r#"(progn
                 (erase-buffer)
                 (set-buffer-multibyte {multibyte})
                 (insert "az")
                 (goto-char 2)
                 (setq json-insert-test-change nil)
                 (setq after-change-functions
                       (list (lambda (beg end len)
                               (setq json-insert-test-change (list beg end len)))))
                 (json-insert ["\u00E9\uFFFF\U0001D538"])
                 (setq after-change-functions nil)
                 (list (append (buffer-string) nil)
                       (point)
                       enable-multibyte-characters
                       json-insert-test-change))"#
        )))
    };

    assert_eq!(
        insert_between_a_and_z("nil"),
        "OK ((97 91 34 195 169 239 191 191 240 157 148 184 34 93 122) 15 nil (2 15 0))"
    );
    assert_eq!(
        insert_between_a_and_z("t"),
        "OK ((97 91 34 233 65535 120120 34 93 122) 9 t (2 9 0))"
    );
}

#[test]
fn json_insert_uses_the_buffer_state_left_by_before_change_hooks() {
    crate::test_utils::init_test_tracing();
    // GNU `Fjson_insert` (src/json.c) reads point and the buffer's multibyte
    // flag after `prepare_to_modify_buffer`, so a hook can change where the
    // text goes and whether it lands as characters or UTF-8 bytes.
    let insert_after_hook = |multibyte: &str, hook_form: &str| {
        let mut eval = crate::test_utils::new_context();
        neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&format!(
            r#"(progn
                 (erase-buffer)
                 (set-buffer-multibyte {multibyte})
                 (insert "az")
                 (goto-char 2)
                 (setq json-insert-test-change nil)
                 (setq before-change-functions
                       (list (lambda (_beg _end) {hook_form})))
                 (setq after-change-functions
                       (list (lambda (beg end len)
                               (setq json-insert-test-change (list beg end len)))))
                 (json-insert ["é￿\U0001D538"])
                 (setq before-change-functions nil)
                 (setq after-change-functions nil)
                 (list (append (buffer-string) nil)
                       (point)
                       enable-multibyte-characters
                       json-insert-test-change))"#
        )))
    };

    assert_eq!(
        insert_after_hook("nil", "(set-buffer-multibyte t)"),
        "OK ((97 91 34 233 65535 120120 34 93 122) 9 t (2 9 0))"
    );
    assert_eq!(
        insert_after_hook("t", "(set-buffer-multibyte nil)"),
        "OK ((97 91 34 195 169 239 191 191 240 157 148 184 34 93 122) 15 nil (2 15 0))"
    );
    assert_eq!(
        insert_after_hook("t", "(goto-char 1)"),
        "OK ((91 34 233 65535 120120 34 93 97 122) 8 t (1 8 0))"
    );
    assert_eq!(
        insert_after_hook("nil", "(goto-char 1)"),
        "OK ((91 34 195 169 239 191 191 240 157 148 184 34 93 97 122) 14 nil (1 14 0))"
    );
}

#[test]
fn json_insert_targets_the_buffer_selected_by_before_change_hooks() {
    crate::test_utils::init_test_tracing();
    // GNU lets a before-change hook's `set-buffer` persist, and `Fjson_insert`
    // then inserts into, and reports the change for, that current buffer.
    let insert_after_switch = |other_multibyte: &str| {
        let mut eval = crate::test_utils::new_context();
        neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&format!(
            r#"(let ((origin (current-buffer))
                     (other (get-buffer-create " *json-insert-other*")))
                 (set-buffer other)
                 (erase-buffer)
                 (set-buffer-multibyte {other_multibyte})
                 (set-buffer origin)
                 (erase-buffer)
                 (insert "az")
                 (goto-char 2)
                 (setq json-insert-test-change nil)
                 (setq before-change-functions
                       (list (lambda (_beg _end) (set-buffer other))))
                 (setq after-change-functions
                       (list (lambda (beg end len)
                               (setq json-insert-test-change
                                     (list (buffer-name) beg end len)))))
                 (json-insert ["é￿\U0001D538"])
                 (setq before-change-functions nil)
                 (setq after-change-functions nil)
                 (let ((result (list (eq (current-buffer) other)
                                     (append (buffer-string) nil)
                                     (point)
                                     json-insert-test-change)))
                   (set-buffer origin)
                   (list result (buffer-string) (point))))"#
        )))
    };

    assert_eq!(
        insert_after_switch("t"),
        "OK ((t (91 34 233 65535 120120 34 93) 8 (\" *json-insert-other*\" 1 8 0)) \"az\" 2)"
    );
    assert_eq!(
        insert_after_switch("nil"),
        "OK ((t (91 34 195 169 239 191 191 240 157 148 184 34 93) 14 (\" *json-insert-other*\" 1 14 0)) \"az\" 2)"
    );
}

#[test]
fn json_serialize_cons_objects_report_list_errors_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU `json_out_object_cons` (src/json.c) checks each element while it
    // walks with FOR_EACH_TAIL, then does CHECK_LIST_END on the final tail.
    let mut eval = crate::test_utils::new_context();
    let mut outcome = |form: &str| {
        neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&format!(
            r#"(condition-case err (json-serialize {form}) (error err))"#
        )))
    };

    assert_eq!(
        outcome("'((a . 1) . b)"),
        "OK (wrong-type-argument listp ((a . 1) . b))"
    );
    assert_eq!(
        outcome("'((1 . 2) . b)"),
        "OK (wrong-type-argument symbolp 1)"
    );
    assert_eq!(outcome("'((a . 1) 2)"), "OK (wrong-type-argument consp 2)");
    assert_eq!(outcome("'(:a . 1)"), "OK (wrong-type-argument consp 1)");
    assert_eq!(outcome("'(:a 1 :b)"), "OK (wrong-type-argument consp nil)");
    assert_eq!(
        outcome("'((a . 1) (a . 2) (b . 3))"),
        r#"OK "{\"a\":1,\"b\":3}""#
    );
    assert_eq!(outcome("'(:a 1 :a 2 :b 3)"), r#"OK "{\"a\":1,\"b\":3}""#);

    let mut signal_of = |form: &str| {
        neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&format!(
            r#"(let ((items {form}))
                 (setcdr (nthcdr (1- (length items)) items) items)
                 (condition-case err (json-serialize items) (error (car err))))"#
        )))
    };
    assert_eq!(signal_of("(list (cons 'a 1))"), "OK circular-list");
    assert_eq!(
        signal_of("(list (cons 'a 1) (cons 'b 2))"),
        "OK circular-list"
    );
    assert_eq!(signal_of("(list :a 1)"), "OK circular-list");
}
