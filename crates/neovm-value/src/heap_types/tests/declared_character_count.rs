use super::{GnuDeclaredCharacterCount, GnuDeclaredCharacterCountError, LispString};

fn declared(value: i128) -> GnuDeclaredCharacterCount {
    GnuDeclaredCharacterCount::try_from(value).expect("nonnegative test metadata")
}

#[test]
fn declared_count_rejects_negative_and_signed_overflow() {
    assert_eq!(
        GnuDeclaredCharacterCount::try_from(-1),
        Err(GnuDeclaredCharacterCountError::Negative)
    );
    assert_eq!(
        GnuDeclaredCharacterCount::try_from(isize::MAX as i128 + 1),
        Err(GnuDeclaredCharacterCountError::TooLarge)
    );
}

#[test]
fn declared_count_is_not_recovered_from_raw_suffix_bytes() {
    let name =
        LispString::from_emacs_bytes_with_declared_chars(vec![b'b', 233, 233, 233], declared(4));
    assert_eq!(name.schars(), 4);
    assert_eq!(name.sbytes(), 4);
    assert_eq!(name.as_bytes(), &[b'b', 233, 233, 233]);
    assert!(name.is_multibyte());
    assert!(name.has_trailing_nul());
    let copy = name.clone();
    assert_eq!(copy.schars(), 4);
    assert_eq!(copy.as_bytes(), name.as_bytes());
}

#[test]
fn declared_empty_bytes_select_gnu_empty_multibyte_shape() {
    let name = LispString::from_emacs_bytes_with_declared_chars(Vec::new(), declared(3));
    assert_eq!(name.schars(), 0);
    assert_eq!(name.sbytes(), 0);
    assert!(name.is_multibyte());
}

#[test]
fn zero_declared_characters_preserve_nonempty_bytes() {
    let name = LispString::from_emacs_bytes_with_declared_chars(b"a".to_vec(), declared(0));
    assert_eq!(name.schars(), 0);
    assert_eq!(name.sbytes(), 1);
    assert_eq!(name.as_bytes(), b"a");
    assert!(name.is_multibyte());
}

#[test]
fn valid_unicode_payload_preserves_codepoint_encoding() {
    let name =
        LispString::from_emacs_bytes_with_declared_chars("λé".as_bytes().to_vec(), declared(2));
    assert_eq!(name.as_bytes(), "λé".as_bytes());
    assert_eq!(name.schars(), 2);
    assert_eq!(name.sbytes(), 4);
    assert_eq!(name.as_utf8_str(), Some("λé"));
}
