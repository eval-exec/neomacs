use super::*;

#[test]
fn gdn_delete_retains_input_string_encoding() {
    let source = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(vec![
        b'a', b'b', 0xc3, 0xa9,
    ]));
    let result = builtin_delete_with_symbols(vec![Value::fixnum(233), source], false).unwrap();
    assert!(result.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(result.as_lisp_string().unwrap().as_bytes(), b"ab");
    let raw = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(vec![
        0xc3, 0xa9, 0xc1, 0xaa,
    ]));
    let result = builtin_delete_with_symbols(vec![Value::fixnum(233), raw], false).unwrap();
    assert!(result.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(
        super::super::lisp_string_char_codes(result.as_lisp_string().unwrap()),
        vec![0x3fffea]
    );
}

#[test]
fn gdn_reverse_ascii_multibyte_becomes_unibyte() {
    let source = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        b"ab".to_vec(),
    ));
    let result = builtin_reverse(vec![source]).unwrap();
    assert!(!result.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(result.as_lisp_string().unwrap().as_bytes(), b"ba");
}
