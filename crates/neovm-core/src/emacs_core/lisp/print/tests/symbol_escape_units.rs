//! GNU print.c:2513-2520: symbol whitespace is an Emacs character domain.
use super::super::*;
use crate::emacs_core::intern::{intern_uninterned, intern_uninterned_lisp_string};
#[test]
fn symbol_escape_distinguishes_nbsp_character_from_raw_a0_byte() {
    let multibyte = Value::symbol(intern_uninterned("a\u{a0}b"));
    assert_eq!(print_value_bytes(&multibyte), b"a\\\xc2\xa0b");
    let raw = crate::heap_types::LispString::from_unibyte(vec![b'a', 0xa0, b'b']);
    let unibyte = Value::symbol(intern_uninterned_lisp_string(&raw));
    let mut expected = vec![b'a'];
    let mut encoded = [0; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let width = crate::emacs_core::emacs_char::char_string(
        crate::emacs_core::emacs_char::unibyte_to_char(0xa0),
        &mut encoded,
    );
    expected.extend_from_slice(&encoded[..width]);
    expected.push(b'b');
    assert_eq!(print_value_bytes(&unibyte), expected);
}
