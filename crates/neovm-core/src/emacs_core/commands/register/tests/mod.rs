use super::*;
use crate::buffer::{BufferId, LispCharPos1};
use crate::heap_types::LispString;

// -----------------------------------------------------------------------
// RegisterManager unit tests
// -----------------------------------------------------------------------

#[test]
fn set_get_clear() {
    crate::test_utils::init_test_tracing();
    let mut mgr = RegisterManager::new();

    // Initially empty
    assert!(mgr.get('a').is_none());

    // Set text
    mgr.set(
        'a',
        RegisterContent::Text(LispString::from_unibyte(b"hello".to_vec())),
    );
    assert!(mgr.get('a').is_some());
    assert_eq!(mgr.get_text('a'), Some("hello"));

    // Overwrite
    mgr.set('a', RegisterContent::Number(42));
    assert!(mgr.get_text('a').is_none());
    match mgr.get('a') {
        Some(RegisterContent::Number(42)) => {}
        other => panic!("Expected Number(42), got {:?}", other),
    }

    // Clear
    mgr.clear('a');
    assert!(mgr.get('a').is_none());
}

#[test]
fn clear_all() {
    crate::test_utils::init_test_tracing();
    let mut mgr = RegisterManager::new();
    mgr.set(
        'a',
        RegisterContent::Text(LispString::from_unibyte(b"one".to_vec())),
    );
    mgr.set(
        'b',
        RegisterContent::Text(LispString::from_unibyte(b"two".to_vec())),
    );
    mgr.set('c', RegisterContent::Number(3));

    assert_eq!(mgr.list().len(), 3);
    mgr.clear_all();
    assert_eq!(mgr.list().len(), 0);
}

#[test]
fn text_append_and_prepend() {
    crate::test_utils::init_test_tracing();
    let mut mgr = RegisterManager::new();

    // Append to empty register creates text
    mgr.append_text('x', "hello", false);
    assert_eq!(mgr.get_text('x'), Some("hello"));

    // Append
    mgr.append_text('x', " world", false);
    assert_eq!(mgr.get_text('x'), Some("hello world"));

    // Prepend
    mgr.append_text('x', ">> ", true);
    assert_eq!(mgr.get_text('x'), Some(">> hello world"));
}

#[test]
fn append_to_non_text_replaces() {
    crate::test_utils::init_test_tracing();
    let mut mgr = RegisterManager::new();
    mgr.set('n', RegisterContent::Number(99));
    mgr.append_text('n', "new text", false);
    assert_eq!(mgr.get_text('n'), Some("new text"));
}

#[test]
fn marker_storage() {
    crate::test_utils::init_test_tracing();
    let mut mgr = RegisterManager::new();
    let buffer_id = BufferId(7);
    let marker = crate::emacs_core::marker::make_marker_value(
        Some(buffer_id),
        Some(LispCharPos1::new(42)),
        false,
    );
    mgr.set('p', RegisterContent::Marker(marker));
    match mgr.get('p') {
        Some(RegisterContent::Marker(stored)) => {
            let (buffer_id, point, insertion_type) =
                crate::emacs_core::marker::marker_logical_fields(stored).expect("marker");
            assert_eq!(buffer_id, Some(BufferId(7)));
            assert_eq!(point, Some(LispCharPos1::new(42)));
            assert!(!insertion_type);
        }
        other => panic!("Expected Marker, got {:?}", other),
    }
}

#[test]
fn list_registers_sorted() {
    crate::test_utils::init_test_tracing();
    let mut mgr = RegisterManager::new();
    mgr.set(
        'z',
        RegisterContent::Text(LispString::from_unibyte(b"z-text".to_vec())),
    );
    mgr.set('a', RegisterContent::Number(1));
    mgr.set(
        'm',
        RegisterContent::File(LispString::from_unibyte(b"/tmp/foo".to_vec())),
    );

    let list = mgr.list();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].0, 'a');
    assert_eq!(list[0].1, "number");
    assert_eq!(list[1].0, 'm');
    assert_eq!(list[1].1, "file");
    assert_eq!(list[2].0, 'z');
    assert_eq!(list[2].1, "text");
}

#[test]
fn rectangle_and_kbd_macro() {
    crate::test_utils::init_test_tracing();
    let mut mgr = RegisterManager::new();

    let rect = vec![
        LispString::from_unibyte(b"line1".to_vec()),
        LispString::from_unibyte(b"line2".to_vec()),
        LispString::from_unibyte(b"line3".to_vec()),
    ];
    mgr.set('r', RegisterContent::Rectangle(rect));
    match mgr.get('r') {
        Some(RegisterContent::Rectangle(lines)) => assert_eq!(lines.len(), 3),
        other => panic!("Expected Rectangle, got {:?}", other),
    }

    let macro_keys = vec![Value::char('a'), Value::char('b')];
    mgr.set('k', RegisterContent::KbdMacro(macro_keys));
    match mgr.get('k') {
        Some(RegisterContent::KbdMacro(keys)) => assert_eq!(keys.len(), 2),
        other => panic!("Expected KbdMacro, got {:?}", other),
    }
}

// -----------------------------------------------------------------------
// Builtin-level tests
// -----------------------------------------------------------------------

#[test]
fn test_expect_register() {
    crate::test_utils::init_test_tracing();
    // Char
    assert_eq!(expect_register(&Value::char('a')).unwrap(), 'a');

    // Int (ASCII code)
    assert_eq!(expect_register(&Value::fixnum(65)).unwrap(), 'A');

    // Raw-byte Emacs character code maps back to its byte value.
    assert_eq!(
        expect_register(&Value::fixnum(0x3F_FFFF)).unwrap(),
        '\u{00FF}'
    );

    // Single-char string
    assert_eq!(expect_register(&Value::string("z")).unwrap(), 'z');

    // Multi-char string is an error
    assert!(expect_register(&Value::string("ab")).is_err());

    // Float is an error
    assert!(expect_register(&Value::make_float(1.0)).is_err());
}

#[test]
fn test_builtin_copy_and_insert() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // copy-to-register
    let result = builtin_copy_to_register(
        &mut eval,
        vec![Value::char('a'), Value::string("hello world")],
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());

    // insert-register -> returns the text
    let result = builtin_insert_register(&mut eval, vec![Value::char('a')]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("hello world"));

    // insert-register on empty register -> error
    let result = builtin_insert_register(&mut eval, vec![Value::char('z')]);
    assert!(result.is_err());
}

#[test]
fn test_builtin_number_and_increment() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // number-to-register
    let result = builtin_number_to_register(&mut eval, vec![Value::fixnum(10), Value::char('n')]);
    assert!(result.is_ok());

    // get-register -> returns 10
    let result = builtin_get_register(&mut eval, vec![Value::char('n')]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_fixnum());

    // increment-register by 5
    let result = builtin_increment_register(&mut eval, vec![Value::fixnum(5), Value::char('n')]);
    assert!(result.is_ok());

    // Now should be 15
    let result = builtin_get_register(&mut eval, vec![Value::char('n')]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_fixnum());
}

#[test]
fn test_builtin_increment_empty_register() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Incrementing empty register starts from 0
    let result = builtin_increment_register(&mut eval, vec![Value::fixnum(7), Value::char('e')]);
    assert!(result.is_ok());

    let result = builtin_get_register(&mut eval, vec![Value::char('e')]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_fixnum());
}

#[test]
fn test_builtin_set_and_get_register() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Set string
    let result = builtin_set_register(
        &mut eval,
        vec![Value::char('s'), Value::string("saved text")],
    );
    assert!(result.is_ok());

    let result = builtin_get_register(&mut eval, vec![Value::char('s')]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("saved text"));

    // Set nil clears
    let result = builtin_set_register(&mut eval, vec![Value::char('s'), Value::NIL]);
    assert!(result.is_ok());

    let result = builtin_get_register(&mut eval, vec![Value::char('s')]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn test_builtin_view_register() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Empty register
    let result = builtin_view_register(&mut eval, vec![Value::char('v')]);
    assert!(result.is_ok());
    let desc = result.unwrap();
    assert!(desc.as_utf8_str().unwrap().contains("empty"));

    // Text register
    eval.registers.set(
        'v',
        RegisterContent::Text(LispString::from_unibyte(b"some text".to_vec())),
    );
    let result = builtin_view_register(&mut eval, vec![Value::char('v')]);
    assert!(result.is_ok());
    let desc = result.unwrap();
    assert!(desc.as_utf8_str().unwrap().contains("text"));
    assert!(desc.as_utf8_str().unwrap().contains("some text"));

    // Number register
    eval.registers.set('v', RegisterContent::Number(99));
    let result = builtin_view_register(&mut eval, vec![Value::char('v')]);
    assert!(result.is_ok());
    let desc = result.unwrap();
    assert!(desc.as_utf8_str().unwrap().contains("99"));
}

#[test]
fn test_builtin_point_to_register_stores_marker() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let current_buffer_id = eval.buffers.current_buffer().expect("current buffer").id;

    builtin_point_to_register(&mut eval, vec![Value::char('p')]).expect("point-to-register");

    let stored = builtin_get_register(&mut eval, vec![Value::char('p')]).expect("get-register");
    assert!(stored.is_marker());
    assert_eq!(
        crate::emacs_core::marker::builtin_marker_buffer_in_buffers(&eval.buffers, vec![stored],)
            .expect("marker-buffer"),
        Value::make_buffer(current_buffer_id)
    );
    assert_eq!(
        crate::emacs_core::marker::builtin_marker_position_in_buffers(&eval.buffers, vec![stored],)
            .expect("marker-position"),
        Value::fixnum(1)
    );
}

#[test]
fn test_builtin_point_to_register_stores_lisp_char_position() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("\u{20AC}x");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new("\u{20AC}".len()));
        assert_eq!(buffer.point_lisp_char_pos().as_i64(), 2);
    }

    builtin_point_to_register(&mut eval, vec![Value::char('p')]).expect("point-to-register");

    let stored = builtin_get_register(&mut eval, vec![Value::char('p')]).expect("get-register");
    assert!(stored.is_marker());
    assert_eq!(
        crate::emacs_core::marker::builtin_marker_position_in_buffers(&eval.buffers, vec![stored],)
            .expect("marker-position"),
        Value::fixnum(2)
    );
}

#[test]
fn test_builtin_register_to_string() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Empty register => nil
    let empty = builtin_register_to_string(&mut eval, vec![Value::char('r')]).unwrap();
    assert!(empty.is_nil());

    // Text register => string
    builtin_set_register(&mut eval, vec![Value::char('r'), Value::string("abc")]).unwrap();
    let text = builtin_register_to_string(&mut eval, vec![Value::char('r')]).unwrap();
    assert_eq!(text.as_utf8_str(), Some("abc"));
}

#[test]
fn rectangle_and_file_registers_preserve_raw_unibyte_strings() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    eval.registers.set(
        'r',
        RegisterContent::Rectangle(vec![
            LispString::from_unibyte(vec![0xFF]),
            LispString::from_unibyte(b"ok".to_vec()),
        ]),
    );
    eval.registers.set(
        'f',
        RegisterContent::File(LispString::from_unibyte(vec![0xFF])),
    );

    let rect = builtin_get_register(&mut eval, vec![Value::char('r')]).expect("rectangle");
    let lines = crate::emacs_core::value::list_to_vec(&rect).expect("rectangle list");
    assert_eq!(
        lines[0]
            .as_lisp_string()
            .expect("raw rectangle line")
            .as_bytes(),
        &[0xFF]
    );

    let file = builtin_get_register(&mut eval, vec![Value::char('f')]).expect("file");
    let file = file.as_lisp_string().expect("file string");
    assert_eq!(file.as_bytes(), &[0xFF]);
    assert!(!file.is_multibyte());
}

#[test]
fn test_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // copy-to-register needs at least 2 args
    let result = builtin_copy_to_register(&mut eval, vec![Value::char('a')]);
    assert!(result.is_err());

    // point-to-register needs exactly 1 arg
    let result = builtin_point_to_register(&mut eval, vec![]);
    assert!(result.is_err());
}

#[test]
fn register_text_preserves_raw_unibyte_payload() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));

    builtin_set_register(&mut eval, vec![Value::char('r'), raw]).unwrap();

    let got = builtin_get_register(&mut eval, vec![Value::char('r')]).unwrap();
    let got = got.as_lisp_string().expect("register text");
    assert!(!got.is_multibyte());
    assert_eq!(got.as_bytes(), &[0xFF]);

    let rendered = builtin_register_to_string(&mut eval, vec![Value::char('r')]).unwrap();
    let rendered = rendered.as_lisp_string().expect("register string");
    assert!(!rendered.is_multibyte());
    assert_eq!(rendered.as_bytes(), &[0xFF]);
}
