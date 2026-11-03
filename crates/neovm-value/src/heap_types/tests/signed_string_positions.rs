use super::{GnuDeclaredCharacterCount, GnuStringCharPos, GnuStringCharPosError, LispString};
use crate::value::positions::CharPos0;

fn declared_ascii(chars: i128, bytes: &[u8]) -> LispString {
    LispString::from_emacs_bytes_with_declared_chars(
        bytes.to_vec(),
        GnuDeclaredCharacterCount::try_from(chars).expect("bounded test metadata"),
    )
}

#[test]
fn initialized_ascii_suffix_has_a_signed_gnu_coordinate() {
    let string = declared_ascii(0, b"abcde");
    // GNU fns.c:1260-1294: nearest-end suffix count is subtracted from SCHARS.
    assert_eq!(string.byte_to_gnu_char_pos(1).get(), 1);
    assert_eq!(string.byte_to_gnu_char_pos(3).get(), -2);
    assert_eq!(string.byte_to_gnu_char_pos(4).get(), -1);
    assert_eq!(string.byte_to_gnu_char_pos(5).get(), 0);
    assert_eq!(string.byte_to_gnu_char_pos(usize::MAX).get(), 0);
}

#[test]
fn byte_midpoint_ties_use_signed_backward_declared_counts() {
    let string = declared_ascii(1, b"abcd");
    assert_eq!(string.byte_to_gnu_char_pos(2).get(), -1);
    assert_eq!(string.byte_to_gnu_char_pos(1).get(), 1);
}

#[test]
fn negative_slice_coordinates_are_rejected_by_the_option_boundary() {
    let string = declared_ascii(0, b"abcde");
    assert!(string.slice(3, 4).is_none());
    assert!(string.slice(0, 0).is_some());
    // No unsigned wrap, even if byte bounds themselves are valid.
    assert_eq!(
        CharPos0::try_from(GnuStringCharPos::new(-2)),
        Err(GnuStringCharPosError::Negative)
    );
}

#[test]
fn signed_string_coordinates_have_checked_unsigned_conversions() {
    let positive = GnuStringCharPos::try_from(CharPos0::new(7)).expect("bounded control");
    assert_eq!(positive.get(), 7);
    assert_eq!(CharPos0::try_from(positive), Ok(CharPos0::new(7)));
    assert_eq!(
        CharPos0::try_from(GnuStringCharPos::new(i64::MIN)),
        Err(GnuStringCharPosError::Negative)
    );
    if usize::BITS >= i64::BITS {
        assert_eq!(
            GnuStringCharPos::try_from(CharPos0::new(usize::MAX)),
            Err(GnuStringCharPosError::TooLarge)
        );
    }
}

#[test]
fn ordinary_unicode_and_unibyte_positions_remain_nonnegative() {
    let unicode = LispString::from_utf8("abécd");
    for (byte, character) in [(0, 0), (1, 1), (2, 2), (4, 3), (5, 4), (6, 5)] {
        assert_eq!(unicode.byte_to_gnu_char_pos(byte).get(), character);
    }
    let unibyte = LispString::from_unibyte(vec![97, 233, 98]);
    for byte in 0..=3 {
        assert_eq!(unibyte.byte_to_gnu_char_pos(byte).get(), byte as i64);
    }
}
