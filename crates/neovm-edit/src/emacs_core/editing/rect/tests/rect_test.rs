use super::*;

use std::fs;

#[test]
fn rectangle_state_default() {
    crate::test_utils::init_test_tracing();
    let state = RectangleState::new();
    assert!(state.killed.is_empty());
}

#[test]
fn rectangle_state_default_trait() {
    crate::test_utils::init_test_tracing();
    let state = RectangleState::default();
    assert!(state.killed.is_empty());
}

#[test]
fn extract_rectangle_line_returns_string() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(result.as_utf8_str(), Some(""));
}

#[test]
fn extract_rectangle_line_with_line_argument() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![
        Value::fixnum(1),
        Value::fixnum(3),
        Value::string("abcdef"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("bc"));
}

#[test]
fn extract_rectangle_line_swapped_columns() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![
        Value::fixnum(3),
        Value::fixnum(1),
        Value::string("abcdef"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("bc"));
}

#[test]
fn extract_rectangle_line_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![
        Value::fixnum(0),
        Value::fixnum(2),
        Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
            0xFF, b'A', b'B',
        ])),
    ])
    .unwrap();
    let string = result
        .as_lisp_string()
        .expect("extract-rectangle-line should return a LispString");
    assert!(!string.is_multibyte());
    assert_eq!(string.as_bytes(), &[0xFF, b'A']);
}

#[test]
fn extract_rectangle_line_negative_column_errors() {
    crate::test_utils::init_test_tracing();
    assert!(
        builtin_extract_rectangle_line(vec![
            Value::fixnum(-1),
            Value::fixnum(1),
            Value::string("abc"),
        ])
        .is_err()
    );
}

#[test]
fn extract_rectangle_line_validates_args() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_extract_rectangle_line(vec![]).is_err());
    assert!(builtin_extract_rectangle_line(vec![Value::fixnum(1)]).is_err());
    assert!(
        builtin_extract_rectangle_line(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)])
            .is_err()
    );
}

#[test]
fn delete_extract_rectangle_from_text_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let source =
        neovm_value::heap_types::LispString::from_unibyte(vec![0xFF, b'A', b'\n', 0xFE, b'B']);
    let (extracted, rewritten) = delete_extract_rectangle_from_text(&source, 0, 1, 0, 1);

    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted[0].as_bytes(), &[0xFF]);
    assert_eq!(extracted[1].as_bytes(), &[0xFE]);
    assert!(!extracted[0].is_multibyte());
    assert!(!rewritten.is_multibyte());
    assert_eq!(rewritten.as_bytes(), b"A\nB");
}
