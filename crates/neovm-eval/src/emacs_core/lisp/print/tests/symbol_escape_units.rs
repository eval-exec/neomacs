//! GNU print.c:2513-2520: symbol whitespace is an Emacs character domain.
use super::super::*;
use neovm_value::intern::{make_uninterned_symbol, make_uninterned_symbol_lisp_string};
#[test]
fn symbol_escape_distinguishes_nbsp_character_from_raw_a0_byte() {
    let _harness = crate::test_utils::unit_harness();
    let multibyte = Value::symbol(make_uninterned_symbol("a\u{a0}b"));
    assert_eq!(print_value_bytes(&multibyte), b"a\\\xc2\xa0b");
    let raw = neovm_value::heap_types::LispString::from_unibyte(vec![b'a', 0xa0, b'b']);
    let unibyte = Value::symbol(make_uninterned_symbol_lisp_string(&raw));
    let mut expected = vec![b'a'];
    let mut encoded = [0; neovm_value::emacs_char::MAX_MULTIBYTE_LENGTH];
    let width = neovm_value::emacs_char::char_string(
        neovm_value::emacs_char::unibyte_to_char(0xa0),
        &mut encoded,
    );
    expected.extend_from_slice(&encoded[..width]);
    expected.push(b'b');
    assert_eq!(print_value_bytes(&unibyte), expected);
}
