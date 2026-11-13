use super::{MAX_CHAR, byte8_to_char, str_as_unibyte, string_char_unchecked, string_escape_byte8};

#[test]
fn unibyte_conversion_preserves_the_tail_after_an_invalid_byte8_payload() {
    // Preserve existing in-domain arithmetic even for a noncontinuation tail.
    assert_eq!(str_as_unibyte(&[0xc0, 0xc0]), [0xc0]);
    // C1 FF has a two-byte shape but its arithmetic payload exceeds MAX_CHAR.
    // Recovery consumes C1 alone, leaving FF and the next character untouched.
    assert_eq!(str_as_unibyte(&[0xc1, 0xff, b'x']), [0xc1, 0xff, b'x']);
    assert_eq!(
        str_as_unibyte(&[b'a', 0xc1, 0xff, 0xc1, 0xbf, b'z']),
        [b'a', 0xc1, 0xff, 0xff, b'z'],
    );
}

#[test]
fn unibyte_conversion_preserves_an_incomplete_byte8_lead() {
    assert_eq!(str_as_unibyte(&[0xc1]), [0xc1]);
    assert_eq!(str_as_unibyte(&[b'a', 0xc1]), [b'a', 0xc1]);
}

#[test]
fn byte8_escaping_preserves_the_tail_after_an_invalid_payload() {
    assert_eq!(string_escape_byte8(&[0xc0, 0xc0], true), b"\\300");
    // Only the recovered C1 is escaped. The following FF is still a raw
    // non-C0/C1 byte, which this API passes through without reinterpretation.
    assert_eq!(
        string_escape_byte8(&[0xc1, 0xff, b'x'], true),
        b"\\301\xffx"
    );
    assert_eq!(
        string_escape_byte8(&[b'a', 0xc1, 0xbf, b'z'], true),
        b"a\\377z",
    );
}

#[test]
fn byte8_escaping_recovers_an_incomplete_lead_without_reading_past_it() {
    assert_eq!(string_escape_byte8(&[0xc1], true), b"\\301");
    assert_eq!(string_escape_byte8(&[b'a', 0xc1], true), b"a\\301");
}

#[test]
fn typed_arithmetic_decoder_preserves_in_domain_noncanonical_values() {
    // The file reader uses GNU arithmetic rather than the masked checked
    // decoder. Its established in-domain behavior remains distinct.
    let (character, width) = string_char_unchecked(&[0x80, 0x80]);
    assert_eq!((character.code(), width), (0x3fef80, 2));

    // MAX_CHAR remains representable even though canonical byte8 encoding
    // uses C0/C1 instead of this noncanonical five-byte spelling.
    let (character, width) = string_char_unchecked(&[0xf8, 0x8f, 0xbf, 0xbf, 0xbf]);
    assert_eq!((character.code(), width), (MAX_CHAR, 5));
}

#[test]
fn typed_arithmetic_decoder_recovers_payloads_above_the_character_domain() {
    for bytes in [
        &[0xc1, 0xff][..],
        &[0xf8, 0x90, 0x80, 0x80, 0x80][..],
        &[0xf8, 0xb8, 0x96, 0xb8, 0x96][..],
    ] {
        let (character, width) = string_char_unchecked(bytes);
        assert_eq!(
            (character.code(), width),
            (byte8_to_char(bytes[0]), 1),
            "out-of-domain payload: {bytes:x?}",
        );
    }
}
