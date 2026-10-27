use super::{LispString, decode_declared_multibyte_char};
use crate::emacs_core::builtins;
use crate::emacs_core::error::FlowRef;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// Decode a declared character and expose its code, so the assertions below
/// stay written in the code domain while the decoder returns `EmacsChar`.
fn decode_code(bytes: &[u8]) -> (u32, usize) {
    let (character, width) = decode_declared_multibyte_char(bytes);
    (character.code(), width)
}

#[test]
fn declared_string_character_access_is_bounded_and_count_driven() {
    let _ctx = Context::new();
    // Preserve GNU lead arithmetic within the character domain.
    // The two extra zeros below are the bounded emulator tail policy,
    // not a claim that GNU initialized bytes past its terminator.
    let string = LispString::from_dump(vec![0xe9, 0xe9, 0xe9], 3, 3);
    assert_eq!(
        builtins::lisp_string_char_codes(&string),
        vec![0xaaa9, 0, 0]
    );
    let mut streamed = Vec::new();
    builtins::for_each_lisp_string_char(&string, |code| streamed.push(code));
    assert_eq!(streamed, vec![0xaaa9, 0, 0]);
    // Indexed access preserves GNU's SCHARS==SBYTES byte positioning,
    // unlike sequential append. A short high-byte tail must not panic.
    assert_eq!(builtins::lisp_string_char_at(&string, 0), Some(0xaaa9));
    assert!(builtins::lisp_string_char_at(&string, 2).is_some());
    let value = Value::heap_string(string);
    assert!(builtins::lisp_string_value_char_at(value, 2).is_some());
    assert_eq!(builtins::lisp_string_value_char_at(value, 3), None);
}

#[test]
fn declared_string_iteration_honors_character_limit_and_virtual_zero_tail() {
    let _ctx = Context::new();
    let longer = LispString::from_dump(b"ab".to_vec(), 4, 2);
    assert_eq!(
        builtins::lisp_string_char_codes(&longer),
        vec![97, 98, 0, 0]
    );
    let shorter = LispString::from_dump(b"ab".to_vec(), 1, 2);
    assert_eq!(builtins::lisp_string_char_codes(&shorter), vec![97]);
    let copy = longer.without_properties();
    assert_eq!(copy.schars(), 4);
    assert_eq!(copy.sbytes(), 2);
    assert_eq!(copy.as_bytes(), b"ab");
    assert!(copy.has_trailing_nul());
    assert_eq!(decode_code(&[]), (0, 1));
    assert_eq!(decode_code("é".as_bytes()), (233, 2));
}

#[test]
fn declared_string_aset_rejects_a_character_without_physical_storage() {
    let _ctx = Context::new();
    let value = Value::heap_string(LispString::from_dump(b"ab".to_vec(), 5, 2));
    let error = builtins::aset_string_replacement(&value, &Value::fixnum(2), &Value::fixnum(65))
        .expect_err("logical index cannot authorize a physical out-of-bounds write");
    match error.kind() {
        FlowRef::Signal(signal) => assert_eq!(signal.symbol_name(), "args-out-of-range"),
        other => panic!("expected args-out-of-range, got {other:?}"),
    }
    assert_eq!(value.as_lisp_string().unwrap().as_bytes(), b"ab");
}

#[test]
fn declared_string_invalid_heads_and_short_tails_stay_in_character_domain() {
    use crate::emacs_core::emacs_char::MAX_CHAR;
    for bytes in [
        &[0x80][..],
        &[0xbf][..],
        &[0xff][..],
        &[0xc0][..],
        &[0xc1][..],
        &[0xe9][..],
        &[0xe9, 0xe9][..],
        &[0xf8][..],
        &[0xf8, 0xbf, 0xbf, 0xbf, 0xbf][..],
    ] {
        let (code, width) = decode_code(bytes);
        assert!(code <= MAX_CHAR, "{bytes:x?}: {code:#x}");
        assert!((1..=5).contains(&width));
    }
    assert_eq!(decode_code(&[0x80]), (0x3fff80, 1));
    assert_eq!(decode_code(&[0xff]), (MAX_CHAR, 1));
    // A full F8 tail can decode outside the 22-bit domain; it must become
    // a raw byte rather than reaching downstream character assertions.
    assert_eq!(decode_code(&[0xf8, 0xbf, 0xbf, 0xbf, 0xbf]), (0x3ffff8, 1));
    assert_eq!(decode_code(&[0xe9, 0xe9, 0xe9]), (0xaaa9, 3));
}
