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
