use super::*;
use crate::emacs_core::print::PrintOptions;

// Test-only helpers that restore the default-options ergonomics of the
// deleted format_lisp_string / format_lisp_string_bytes shims.
fn format_lisp_string_with_options_default(s: &str) -> String {
    String::from_utf8_lossy(&format_lisp_string_bytes_inner(s, &PrintOptions::default()))
        .into_owned()
}

fn format_lisp_string_bytes_default(s: &str) -> Vec<u8> {
    format_lisp_string_bytes_inner(s, &PrintOptions::default())
}

#[test]
fn passes_through_control_chars_by_default() {
    crate::test_utils::init_test_tracing();
    // GNU Emacs default: only " and \ are escaped in prin1.
    // Control chars pass through literally (print-escape-newlines
    // and print-escape-control-characters are nil by default).
    assert_eq!(format_lisp_string_with_options_default("\n\t"), "\"\n\t\"");
    assert_eq!(
        format_lisp_string_with_options_default("\u{7f}"),
        "\"\u{7f}\""
    );
}

#[test]
fn keeps_non_bmp_visible() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        format_lisp_string_with_options_default("\u{10ffff}"),
        "\"\u{10ffff}\""
    );
}

#[test]
fn escapes_raw_byte_sentinel_as_octal() {
    crate::test_utils::init_test_tracing();
    let raw_377 = char::from_u32(0xE0FF).expect("valid sentinel scalar");
    assert_eq!(
        format_lisp_string_with_options_default(&raw_377.to_string()),
        "\"\\377\""
    );
}

#[test]
fn encode_nonunicode_char_uses_obsolete_utf8_bytes() {
    crate::test_utils::init_test_tracing();
    let encoded =
        encode_nonunicode_char_for_storage(0x110000).expect("non-unicode char should be encoded");
    assert_eq!(
        format_lisp_string_bytes_default(&encoded),
        vec![b'"', 0xF4, 0x90, 0x80, 0x80, b'"']
    );
}

#[test]
fn encode_nonunicode_char_uses_five_byte_sequence() {
    crate::test_utils::init_test_tracing();
    let encoded =
        encode_nonunicode_char_for_storage(0x200000).expect("non-unicode char should be encoded");
    assert_eq!(
        format_lisp_string_bytes_default(&encoded),
        vec![b'"', 0xF8, 0x88, 0x80, 0x80, 0x80, b'"']
    );
}

#[test]
fn decode_storage_char_codes_handles_nonunicode_and_raw_byte() {
    crate::test_utils::init_test_tracing();
    let encoded = format!(
        "{}{}",
        encode_nonunicode_char_for_storage(0x110000).expect("should encode"),
        encode_nonunicode_char_for_storage(0x3FFFFF).expect("raw byte should encode")
    );
    assert_eq!(
        decode_storage_char_codes(&encoded, true),
        vec![0x110000, 0x3FFFFF]
    );
}

#[test]
fn storage_char_len_for_nonunicode() {
    crate::test_utils::init_test_tracing();
    let ext = encode_nonunicode_char_for_storage(0x110000).expect("should encode");
    let raw = encode_nonunicode_char_for_storage(0x3FFFFF).expect("should encode");
    let s = format!("{ext}A{raw}");

    assert_eq!(storage_char_len(&s), 3);
}

#[test]
fn unibyte_storage_string_round_trips_emacs_mule_bytes() {
    crate::test_utils::init_test_tracing();
    let encoded =
        bytes_to_unibyte_storage_string(&[0x06, b'"', b'\\', b'\n', 0x7F, 0x80, 0xA9, 0xFF]);
    assert!(storage_string_contains_unibyte_bytes(&encoded));
    // With `print-escape-nonascii' nil (the default), GNU `print_object' prints a
    // unibyte string's high byte as `printchar (BYTE8_TO_CHAR (c))' -- the raw
    // byte in its 2-byte byte8 internal encoding (`BYTE8_STRING'), not an octal
    // escape.  Octal escaping is reserved for `print-escape-nonascii' (a
    // multibyte print target).  byte8 encodings: 0x80 -> C0 80, 0xA9 -> C0 A9,
    // 0xFF -> C1 BF.
    assert_eq!(
        format_lisp_string_bytes_default(&encoded),
        vec![
            b'"', 0x06, b'\\', b'"', b'\\', b'\\', b'\n', 0x7F, 0xC0, 0x80, 0xC0, 0xA9, 0xC1, 0xBF,
            b'"'
        ]
    );
    assert_eq!(
        decode_storage_char_codes(&encoded, false),
        vec![6, 34, 92, 10, 127, 128, 169, 255]
    );
    assert_eq!(storage_char_len(&encoded), 8);
    assert_eq!(storage_byte_len(&encoded), 8);
}

#[test]
fn multibyte_storage_sentinels_do_not_look_unibyte() {
    crate::test_utils::init_test_tracing();
    let raw_byte = encode_nonunicode_char_for_storage(0x3FFF80).expect("raw byte sentinel");
    let extended = encode_nonunicode_char_for_storage(0x110000).expect("extended sentinel");

    assert!(!storage_string_contains_unibyte_bytes("ascii"));
    assert!(!storage_string_contains_unibyte_bytes("é"));
    assert!(!storage_string_contains_unibyte_bytes(&raw_byte));
    assert!(!storage_string_contains_unibyte_bytes(&extended));
}

#[test]
fn private_use_chars_outside_sentinel_ranges_are_not_rewritten() {
    crate::test_utils::init_test_tracing();
    let plain_private_use = char::from_u32(0xE400).expect("valid private-use scalar");
    let s = plain_private_use.to_string();
    assert_eq!(decode_storage_char_codes(&s, true), vec![0xE400]);
    assert_eq!(storage_char_len(&s), 1);
    assert_eq!(storage_byte_len(&s), s.len());
}

/// Issue #131: real Private-Use-Area glyphs in U+E300..U+E37F (e.g. nerd-font
/// weather icons U+E322) are NOT unibyte byte-sentinels — those only occupy
/// U+E380..U+E3FF. They must not be detected as unibyte, and a multibyte
/// storage string must convert them to their real Emacs bytes (not a low byte).
#[test]
fn nerd_font_pua_glyphs_are_not_unibyte_sentinels_issue_131() {
    crate::test_utils::init_test_tracing();
    for cp in [0xE300u32, 0xE322, 0xE325, 0xE379, 0xE37F] {
        let s = char::from_u32(cp).unwrap().to_string();
        assert!(
            !storage_string_contains_unibyte_bytes(&s),
            "U+{cp:04X} must not look unibyte"
        );
        // Multibyte conversion preserves the real char (its own Emacs bytes).
        assert_eq!(
            storage_string_to_buffer_bytes(&s, true),
            s.as_bytes(),
            "U+{cp:04X} must convert to its real bytes"
        );
        assert_eq!(decode_storage_char_codes(&s, true), vec![cp]);
    }
    // The genuine unibyte sentinel range (U+E380..U+E3FF = bytes 0x80..0xFF)
    // still decodes to raw bytes for unibyte storage.
    let unibyte = bytes_to_unibyte_storage_string(&[0x80, 0xFF]);
    assert!(storage_string_contains_unibyte_bytes(&unibyte));
    assert_eq!(decode_storage_char_codes(&unibyte, false), vec![0x80, 0xFF]);
}
