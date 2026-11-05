use super::*;

use crate::emacs_core::search::builtin_replace_regexp_in_string;

use neovm_edit::buffer::LispCharPos1;

use neovm_edit::emacs_core::marker;

use neovm_value::error::FlowResultExt as _;

use neovm_value::heap_types::LispString;

use neovm_value::value::{ValueKind, list_to_vec};

use std::cell::RefCell;

#[test]
fn regexp_quote_specials() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string("foo.bar*baz+qux")]);
    assert_str(result.unwrap(), "foo\\.bar\\*baz\\+qux");
}

#[test]
fn regexp_quote_no_specials() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string("hello")]);
    assert_str(result.unwrap(), "hello");
}

#[test]
fn regexp_quote_all_specials() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string(".*+?[]^$\\")]);
    // GNU regexp-quote does NOT escape ']' — only '[' is special.
    assert_str(result.unwrap(), "\\.\\*\\+\\?\\[]\\^\\$\\\\");
}

#[test]
fn regexp_quote_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let input = Value::heap_string(LispString::from_unibyte(vec![b'a', 0xFF, b'*', b'[', b'z']));
    let result = builtin_regexp_quote(vec![input]).unwrap();
    let string = result
        .as_lisp_string()
        .expect("regexp-quote should return string");
    assert!(!string.is_multibyte());
    assert_eq!(
        string.as_bytes(),
        &[b'a', 0xFF, b'\\', b'*', b'\\', b'[', b'z']
    );
    assert!(string.as_utf8_str().is_none());
}

#[test]
fn regexp_quote_parens_not_escaped() {
    crate::test_utils::init_test_tracing();
    // In Emacs regex, literal ( ) are NOT special, so regexp-quote
    // should NOT escape them.
    let result = builtin_regexp_quote(vec![Value::string("(foo)")]);
    assert_str(result.unwrap(), "(foo)");
}

#[test]
fn regexp_quote_right_bracket_not_escaped() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string("]")]);
    assert_str(result.unwrap(), "]");
}

#[test]
fn regexp_quote_preserves_raw_unibyte_bytes_compact_case() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF, b'[', b'.',
    ]));
    let result = builtin_regexp_quote(vec![raw]).unwrap();
    assert_lisp_string_bytes(result, &[b'a', 0xFF, b'\\', b'[', b'\\', b'.'], false);
}

fn assert_str(val: Value, expected: &str) {
    match val.kind() {
        ValueKind::String => {
            let s = val.as_utf8_str().unwrap().to_owned();
            assert_eq!(&*s, expected);
        }
        _other => panic!("Expected string {:?}, got {:?}", expected, val),
    }
}

fn assert_lisp_string_bytes(val: Value, expected: &[u8], multibyte: bool) {
    match val.kind() {
        ValueKind::String => {
            let s = val
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload");
            assert_eq!(s.as_bytes(), expected);
            assert_eq!(s.is_multibyte(), multibyte);
        }
        _other => panic!("Expected string bytes {:?}, got {:?}", expected, val),
    }
}
