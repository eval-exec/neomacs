use super::super::lisp_string_advance_byte_to_boundary;
use crate::emacs_core::emacs_char;
use crate::heap_types::LispString;

#[test]
fn conversion_boundary_rounds_forward_across_every_emacs_encoding_width_and_raw_bytes() {
    let codes = [
        0, 0x7f, 0x80, 0x7ff, 0x800, 0xd800, 0xffff, 0x10000, 0x10ffff, 0x110000, 0x1fffff,
        0x200000, 0x3fff7f, 0x3fff80, 0x3fffc0, 0x3fffff,
    ];
    let mut bytes = Vec::new();
    let mut boundaries = vec![0];
    for code in codes {
        let mut encoded = [0; emacs_char::MAX_MULTIBYTE_LENGTH];
        let length = emacs_char::char_string(code, &mut encoded);
        // C0/C1 raw-byte leads and F8 five-byte leads are valid internal
        // Emacs forms even though Rust's UTF8 parser rejects those strings.
        assert_eq!(
            emacs_char::multibyte_length(&encoded[..length], true),
            Some(length)
        );
        bytes.extend_from_slice(&encoded[..length]);
        boundaries.push(bytes.len());
    }
    let string = LispString::from_emacs_bytes(bytes);
    for byte_pos in 0..=string.sbytes() + 2 {
        let clamped = byte_pos.min(string.sbytes());
        let expected = boundaries
            .iter()
            .copied()
            .find(|boundary| *boundary >= clamped)
            .unwrap();
        assert_eq!(
            lisp_string_advance_byte_to_boundary(&string, byte_pos),
            expected,
            "Emacs byte offset {byte_pos}"
        );
    }
    assert!(
        string.as_utf8_str().is_none(),
        "the fixture must cover Emacs-only internal forms"
    );
}

#[test]
fn conversion_boundary_keeps_unibyte_positions_and_clamps_past_end() {
    let string = LispString::from_unibyte(vec![0, 0x7f, 0x80, 0xc0, 0xc1, 0xf8, 0xff]);
    for byte_pos in 0..=string.sbytes() + 2 {
        assert_eq!(
            lisp_string_advance_byte_to_boundary(&string, byte_pos),
            byte_pos.min(string.sbytes())
        );
    }
}

#[test]
fn conversion_boundary_at_the_end_of_a_long_ascii_prefix_advances_only_to_the_next_character() {
    let mut bytes = vec![b'a'; 32768];
    let mut encoded = [0; emacs_char::MAX_MULTIBYTE_LENGTH];
    let length = emacs_char::char_string(0x200000, &mut encoded);
    bytes.extend_from_slice(&encoded[..length]);
    let string = LispString::from_emacs_bytes(bytes);
    assert_eq!(lisp_string_advance_byte_to_boundary(&string, 32768), 32768);
    for byte_pos in 32769..string.sbytes() {
        assert_eq!(
            lisp_string_advance_byte_to_boundary(&string, byte_pos),
            string.sbytes()
        );
    }
    let ascii = LispString::from_utf8("ascii");
    for byte_pos in 0..=ascii.sbytes() {
        assert_eq!(
            lisp_string_advance_byte_to_boundary(&ascii, byte_pos),
            byte_pos
        );
    }
}
