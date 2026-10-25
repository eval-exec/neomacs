//! Full byte-domain checks for no-escape output encoding.
use super::super::princ_text::*;
use crate::emacs_core::emacs_char;
use crate::heap_types::LispString;

#[test]
fn every_unibyte_value_is_a_valid_character_in_each_stream_domain() {
    let source = LispString::from_unibyte((0..=255).collect());
    for mode in [
        PrincStringOutput::PreserveByte8,
        PrincStringOutput::CallbackCharacters,
    ] {
        let rendered = princ_string_bytes(&source, mode);
        let output = LispString::from_emacs_bytes(rendered.into_owned());
        assert_eq!(output.schars(), source.schars());
        let mut pos = 0;
        for byte in 0..=255u8 {
            let (code, len) = emacs_char::string_char(&output.as_bytes()[pos..]);
            let expected = match mode {
                PrincStringOutput::PreserveByte8 => emacs_char::unibyte_to_char(byte),
                PrincStringOutput::CallbackCharacters => u32::from(byte),
                PrincStringOutput::OctalByte8 => unreachable!("modes exclude escaping"),
            };
            assert_eq!(code.code(), expected);
            pos += len;
        }
        assert_eq!(pos, output.sbytes());
    }
}

#[test]
fn escaping_never_mistakes_unicode_or_nonunicode_characters_for_byte8() {
    let codes = [
        0x80,
        0xe0a0,
        0x200000,
        emacs_char::byte8_to_char(0x80),
        0x20,
        0xe9,
    ];
    let mut source_bytes = Vec::new();
    for code in codes {
        let mut encoded = [0; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(code, &mut encoded);
        source_bytes.extend_from_slice(&encoded[..len]);
    }
    let source = LispString::from_emacs_bytes(source_bytes);
    let out = princ_string_bytes(&source, PrincStringOutput::OctalByte8);
    let mut pos = 0;
    let mut decoded = Vec::new();
    while pos < out.len() {
        let (code, len) = emacs_char::string_char(&out[pos..]);
        decoded.push(code.code());
        pos += len;
    }
    assert_eq!(&decoded[..3], &codes[..3]);
    assert_eq!(&decoded[decoded.len() - 2..], &codes[codes.len() - 2..]);
    assert!(decoded.iter().all(|code| !emacs_char::char_byte8_p(*code)));
}
