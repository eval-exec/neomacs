use neovm_text::emacs_core::json::*;

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::heap_types::LispString;

use neovm_value::value::{ValueKind, VecLikeType};

#[test]
fn json_parse_buffer_advances_point_after_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
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
    let mut eval = Context::new();
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
    let mut eval = Context::new();
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
    let mut eval = Context::new();
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
