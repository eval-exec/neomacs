use neovm_eval::emacs_core::eval::Context;
use neovm_value::heap_types::GnuDeclaredCharacterCount;
use neovm_value::heap_types::GnuStringCharPos;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;

use neovm_value::string_pos_cache::*;

#[test]
fn cold_signed_cache_matches_gnu_initialized_ascii_arithmetic() {
    let _context = crate::test_utils::new_context();
    let value = ascii_value(0);
    let string = value.as_lisp_string().expect("test string");
    for (byte, expected) in [(0, 0), (1, 1), (2, 2), (3, -2), (4, -1), (5, 0)] {
        reset_string_pos_cache();
        assert_eq!(string_byte_to_gnu_char(value, string, byte).get(), expected);
    }
    reset_string_pos_cache();
}

#[test]
fn negative_cached_anchor_survives_backward_and_forward_conversions() {
    let _context = crate::test_utils::new_context();
    let value = ascii_value(0);
    let string = value.as_lisp_string().expect("test string");
    reset_string_pos_cache();
    assert_eq!(string_byte_to_gnu_char(value, string, 3).get(), -2);
    assert_eq!(
        cached_pair(value, string),
        Some((GnuStringCharPos::new(-2), 3))
    );
    assert_eq!(string_byte_to_gnu_char(value, string, 2).get(), -3);
    assert_eq!(string_byte_to_gnu_char(value, string, 3).get(), -2);
    assert_eq!(string_char_to_byte(value, string, 0), 5);
    assert_eq!(
        cached_pair(value, string),
        Some((GnuStringCharPos::new(0), 5))
    );
    reset_string_pos_cache();
}

#[test]
fn positive_cached_anchor_preserves_gnu_history_without_recounting() {
    let _context = crate::test_utils::new_context();
    let value = ascii_value(2);
    let string = value.as_lisp_string().expect("test string");
    reset_string_pos_cache();
    assert_eq!(string_byte_to_gnu_char(value, string, 2).get(), 2);
    // The nearby cached anchor beats a cold end-relative conversion (which is1).
    assert_eq!(string_byte_to_gnu_char(value, string, 3).get(), 3);
    assert_eq!(string_char_to_byte(value, string, 2), 2);
    reset_string_pos_cache();
}

#[test]
fn ordinary_signed_cache_controls_preserve_unicode_and_unibyte_domains() {
    let _context = crate::test_utils::new_context();
    for source in [
        LispString::from_utf8("abécd"),
        LispString::from_unibyte(vec![97, 233, 98]),
    ] {
        let value = Value::heap_string(source);
        let string = value.as_lisp_string().expect("test string");
        reset_string_pos_cache();
        for character in 0..=string.schars() {
            let byte = string_char_to_byte(value, string, character);
            assert_eq!(
                string_byte_to_gnu_char(value, string, byte).get(),
                character as i64
            );
        }
    }
    reset_string_pos_cache();
}

fn ascii_value(chars: i128) -> Value {
    Value::heap_string(LispString::from_emacs_bytes_with_declared_chars(
        b"abcde".to_vec(),
        GnuDeclaredCharacterCount::try_from(chars).expect("bounded metadata"),
    ))
}
