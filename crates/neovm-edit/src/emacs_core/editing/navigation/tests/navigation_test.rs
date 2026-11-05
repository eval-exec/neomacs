use neovm_value::error::FlowKind;

use malachite::integer::Integer;

use neovm_value::value::{Value, ValueKind};

use std::fs;

#[test]
fn skip_char_class_codes_match_gnu_re_wctype_t() {
    crate::test_utils::init_test_tracing();
    let expected = [
        (super::SkipCharClass::Alnum, "alnum", 1),
        (super::SkipCharClass::Alpha, "alpha", 2),
        (super::SkipCharClass::Word, "word", 3),
        (super::SkipCharClass::Graph, "graph", 4),
        (super::SkipCharClass::Print, "print", 5),
        (super::SkipCharClass::Lower, "lower", 6),
        (super::SkipCharClass::Upper, "upper", 7),
        (super::SkipCharClass::Punct, "punct", 8),
        (super::SkipCharClass::Cntrl, "cntrl", 9),
        (super::SkipCharClass::Digit, "digit", 10),
        (super::SkipCharClass::Xdigit, "xdigit", 11),
        (super::SkipCharClass::Blank, "blank", 12),
        (super::SkipCharClass::Space, "space", 13),
        (super::SkipCharClass::Multibyte, "multibyte", 14),
        (super::SkipCharClass::Nonascii, "nonascii", 15),
        (super::SkipCharClass::Ascii, "ascii", 16),
        (super::SkipCharClass::Unibyte, "unibyte", 17),
    ];

    for (class, name, gnu_code) in expected {
        assert_eq!(u8::from(class), gnu_code);
        assert_eq!(name.parse::<super::SkipCharClass>(), Ok(class));
    }
    assert!("Alpha".parse::<super::SkipCharClass>().is_err());
}
