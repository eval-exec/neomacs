use super::*;

// -----------------------------------------------------------------------
// ASCII roundtrip
// -----------------------------------------------------------------------

#[test]
fn ascii_roundtrip_all_128() {
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    for byte in 0u8..128 {
        let c = byte as u32;
        assert_eq!(char_bytes(c), 1);
        let n = char_string(c, &mut buf);
        assert_eq!(n, 1);
        assert_eq!(buf[0], byte);
        let (decoded, len) = string_char(&buf[..n]);
        assert_eq!(decoded, c);
        assert_eq!(len, 1);
    }
}

// -----------------------------------------------------------------------
// 2-byte Unicode roundtrip (U+00E9 = e-acute)
// -----------------------------------------------------------------------

#[test]
fn two_byte_unicode_roundtrip() {
    let c: u32 = 0xE9; // e-acute
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    assert_eq!(char_bytes(c), 2);
    let n = char_string(c, &mut buf);
    assert_eq!(n, 2);
    // Standard UTF-8 for U+00E9: C3 A9
    assert_eq!(buf[0], 0xC3);
    assert_eq!(buf[1], 0xA9);
    let (decoded, len) = string_char(&buf[..n]);
    assert_eq!(decoded, c);
    assert_eq!(len, 2);
}

// -----------------------------------------------------------------------
// 3-byte Unicode roundtrip (U+2018 = left single quotation mark)
// -----------------------------------------------------------------------

#[test]
fn three_byte_unicode_roundtrip() {
    let c: u32 = 0x2018;
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    assert_eq!(char_bytes(c), 3);
    let n = char_string(c, &mut buf);
    assert_eq!(n, 3);
    // UTF-8 for U+2018: E2 80 98
    assert_eq!(buf[0], 0xE2);
    assert_eq!(buf[1], 0x80);
    assert_eq!(buf[2], 0x98);
    let (decoded, len) = string_char(&buf[..n]);
    assert_eq!(decoded, c);
    assert_eq!(len, 3);
}

// -----------------------------------------------------------------------
// 4-byte Unicode roundtrip (U+1F344 = mushroom emoji)
// -----------------------------------------------------------------------

#[test]
fn four_byte_unicode_roundtrip() {
    let c: u32 = 0x1F344;
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    assert_eq!(char_bytes(c), 4);
    let n = char_string(c, &mut buf);
    assert_eq!(n, 4);
    // UTF-8 for U+1F344: F0 9F 8D 84
    assert_eq!(buf[0], 0xF0);
    assert_eq!(buf[1], 0x9F);
    assert_eq!(buf[2], 0x8D);
    assert_eq!(buf[3], 0x84);
    let (decoded, len) = string_char(&buf[..n]);
    assert_eq!(decoded, c);
    assert_eq!(len, 4);
}

// -----------------------------------------------------------------------
// Raw byte 0x80 roundtrip
// -----------------------------------------------------------------------

#[test]
fn raw_byte_0x80_roundtrip() {
    let byte: u8 = 0x80;
    let c = byte8_to_char(byte);
    assert_eq!(c, 0x3FFF80);
    assert!(char_byte8_p(c));
    assert_eq!(char_to_byte8(c), byte);

    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    assert_eq!(char_bytes(c), 2);
    let n = char_string(c, &mut buf);
    assert_eq!(n, 2);
    // Overlong encoding: C0 80
    assert_eq!(buf[0], 0xC0);
    assert_eq!(buf[1], 0x80);

    let (decoded, len) = string_char(&buf[..n]);
    assert_eq!(decoded, c);
    assert_eq!(len, 2);
}

// -----------------------------------------------------------------------
// Raw byte 0xFF roundtrip
// -----------------------------------------------------------------------

#[test]
fn raw_byte_0xff_roundtrip() {
    let byte: u8 = 0xFF;
    let c = byte8_to_char(byte);
    assert_eq!(c, 0x3FFFFF);
    assert!(char_byte8_p(c));
    assert_eq!(char_to_byte8(c), byte);

    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    assert_eq!(char_bytes(c), 2);
    let n = char_string(c, &mut buf);
    assert_eq!(n, 2);
    // Overlong encoding: C1 BF
    assert_eq!(buf[0], 0xC1);
    assert_eq!(buf[1], 0xBF);

    let (decoded, len) = string_char(&buf[..n]);
    assert_eq!(decoded, c);
    assert_eq!(len, 2);
}

// -----------------------------------------------------------------------
// All raw bytes 0x80..0xFF roundtrip
// -----------------------------------------------------------------------

#[test]
fn raw_byte_all_roundtrip() {
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    for byte in 0x80u8..=0xFF {
        let c = byte8_to_char(byte);
        assert!(char_byte8_p(c));
        assert_eq!(char_to_byte8(c), byte);
        let n = char_string(c, &mut buf);
        assert_eq!(n, 2);
        let (decoded, len) = string_char(&buf[..n]);
        assert_eq!(decoded, c, "roundtrip failed for byte 0x{:02X}", byte);
        assert_eq!(len, 2);
    }
}

#[test]
fn unchecked_decoder_matches_checked_decoder_for_valid_internal_bytes() {
    let samples = [
        b'A' as u32,
        0xE9,
        0x2018,
        0x1F344,
        0x20_0000,
        MAX_5_BYTE_CHAR,
        byte8_to_char(0x80),
        byte8_to_char(0xFF),
    ];
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    for code in samples {
        let len = char_string(code, &mut buf);
        assert_eq!(
            string_char_unchecked(&buf[..len]),
            string_char(&buf[..len]),
            "fast decoder mismatch for U+{code:X}"
        );
    }
}

// -----------------------------------------------------------------------
// chars_in_multibyte
// -----------------------------------------------------------------------

#[test]
fn chars_in_multibyte_mixed() {
    // "Ae\u{0301}" (A, e-acute) + raw byte 0x80
    // A = 1 byte, e-acute = 2 bytes, raw 0x80 = 2 bytes → 5 bytes, 3 chars
    let mut data = Vec::new();
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];

    let n = char_string(b'A' as u32, &mut buf);
    data.extend_from_slice(&buf[..n]);

    let n = char_string(0xE9, &mut buf); // e-acute
    data.extend_from_slice(&buf[..n]);

    let n = char_string(byte8_to_char(0x80), &mut buf);
    data.extend_from_slice(&buf[..n]);

    assert_eq!(chars_in_multibyte(&data), 3);
    assert_eq!(data.len(), 5);
}

#[test]
fn chars_in_multibyte_empty() {
    assert_eq!(chars_in_multibyte(&[]), 0);
}

#[test]
fn chars_in_multibyte_ascii_only() {
    assert_eq!(chars_in_multibyte(b"hello"), 5);
}

// -----------------------------------------------------------------------
// char_to_byte_pos / byte_to_char_pos
// -----------------------------------------------------------------------

#[test]
fn char_byte_pos_conversion() {
    // Build: "A" (1 byte) + U+2018 (3 bytes) + raw 0xFF (2 bytes) + "B" (1 byte)
    // Char indices:  0       1                  2                    3
    // Byte offsets:  0       1                  4                    6
    let mut data = Vec::new();
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];

    let n = char_string(b'A' as u32, &mut buf);
    data.extend_from_slice(&buf[..n]);

    let n = char_string(0x2018, &mut buf);
    data.extend_from_slice(&buf[..n]);

    let n = char_string(byte8_to_char(0xFF), &mut buf);
    data.extend_from_slice(&buf[..n]);

    let n = char_string(b'B' as u32, &mut buf);
    data.extend_from_slice(&buf[..n]);

    assert_eq!(data.len(), 7); // 1 + 3 + 2 + 1

    // char_to_byte_pos
    assert_eq!(char_to_byte_pos(&data, 0), 0);
    assert_eq!(char_to_byte_pos(&data, 1), 1);
    assert_eq!(char_to_byte_pos(&data, 2), 4);
    assert_eq!(char_to_byte_pos(&data, 3), 6);
    assert_eq!(char_to_byte_pos(&data, 4), 7); // past end

    // byte_to_char_pos
    assert_eq!(byte_to_char_pos(&data, 0), 0);
    assert_eq!(byte_to_char_pos(&data, 1), 1);
    assert_eq!(byte_to_char_pos(&data, 4), 2);
    assert_eq!(byte_to_char_pos(&data, 6), 3);
    assert_eq!(byte_to_char_pos(&data, 7), 4);
}

// -----------------------------------------------------------------------
// try_as_utf8
// -----------------------------------------------------------------------

#[test]
fn try_as_utf8_valid() {
    let s = "hello world";
    let bytes = utf8_to_emacs(s);
    assert_eq!(try_as_utf8(&bytes), Some(s));
}

#[test]
fn try_as_utf8_with_unicode() {
    let s = "\u{2018}cafe\u{0301}\u{2019}";
    let bytes = utf8_to_emacs(s);
    assert_eq!(try_as_utf8(&bytes), Some(s));
}

#[test]
fn try_as_utf8_with_raw_bytes() {
    // Encode raw byte 0x80 — this produces overlong C0 80 which is not valid UTF-8.
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    let n = char_string(byte8_to_char(0x80), &mut buf);
    assert!(try_as_utf8(&buf[..n]).is_none());
}

// -----------------------------------------------------------------------
// to_utf8_lossy
// -----------------------------------------------------------------------

#[test]
fn to_utf8_lossy_clean() {
    let s = "hello";
    let bytes = utf8_to_emacs(s);
    assert_eq!(to_utf8_lossy(&bytes), "hello");
}

#[test]
fn to_utf8_lossy_with_raw_bytes() {
    // "A" + raw 0x80 + "B"
    let mut data = Vec::new();
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];

    let n = char_string(b'A' as u32, &mut buf);
    data.extend_from_slice(&buf[..n]);

    let n = char_string(byte8_to_char(0x80), &mut buf);
    data.extend_from_slice(&buf[..n]);

    let n = char_string(b'B' as u32, &mut buf);
    data.extend_from_slice(&buf[..n]);

    assert_eq!(to_utf8_lossy(&data), "A\u{FFFD}B");
}

// -----------------------------------------------------------------------
// utf8_to_emacs roundtrip
// -----------------------------------------------------------------------

#[test]
fn utf8_to_emacs_roundtrip() {
    let cases = [
        "",
        "hello",
        "\u{E9}",    // e-acute
        "\u{2018}",  // left single quote
        "\u{1F344}", // mushroom
        "mix\u{E9}d \u{2018}text\u{2019} with \u{1F344}",
    ];
    for s in cases {
        let emacs_bytes = utf8_to_emacs(s);
        // Since there are no raw bytes, try_as_utf8 should succeed and
        // return the original string.
        assert_eq!(
            try_as_utf8(&emacs_bytes),
            Some(s),
            "roundtrip failed for {:?}",
            s
        );
    }
}

// -----------------------------------------------------------------------
// 5-byte character (extended Emacs range)
// -----------------------------------------------------------------------

#[test]
fn five_byte_char_roundtrip() {
    // U+200000 is the start of the 5-byte range
    let c: u32 = 0x20_0000;
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    assert_eq!(char_bytes(c), 5);
    let n = char_string(c, &mut buf);
    assert_eq!(n, 5);
    assert_eq!(buf[0], 0xF8);
    let (decoded, len) = string_char(&buf[..n]);
    assert_eq!(decoded, c);
    assert_eq!(len, 5);
}

#[test]
fn five_byte_max_roundtrip() {
    let c: u32 = MAX_5_BYTE_CHAR; // 0x3FFF7F
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    assert_eq!(char_bytes(c), 5);
    let n = char_string(c, &mut buf);
    assert_eq!(n, 5);
    let (decoded, len) = string_char(&buf[..n]);
    assert_eq!(decoded, c);
    assert_eq!(len, 5);
}

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[test]
fn char_byte8_p_boundary() {
    assert!(!char_byte8_p(MAX_5_BYTE_CHAR));
    assert!(char_byte8_p(MAX_5_BYTE_CHAR + 1));
    assert!(char_byte8_p(MAX_CHAR));
}

#[test]
fn unibyte_to_char_ascii_passthrough() {
    for b in 0u8..0x80 {
        assert_eq!(unibyte_to_char(b), b as u32);
    }
}

#[test]
fn unibyte_to_char_high_bytes_are_byte8() {
    for b in 0x80u16..=0xFF {
        let b = b as u8;
        assert_eq!(unibyte_to_char(b), byte8_to_char(b));
        assert_eq!(unibyte_to_char(b), b as u32 + 0x3F_FF00);
    }
}

#[test]
fn byte8_to_char_is_strict() {
    // GNU `BYTE8_TO_CHAR` is unconditional: even ASCII bytes are mapped
    // into the eight-bit range.
    assert_eq!(byte8_to_char(0x00), 0x3F_FF00);
    assert_eq!(byte8_to_char(0x7F), 0x3F_FF7F);
    assert_eq!(byte8_to_char(0x80), 0x3F_FF80);
    assert_eq!(byte8_to_char(0xFF), 0x3F_FFFF);
}

#[test]
fn char_to_byte_pos_beyond_end() {
    let data = b"AB";
    assert_eq!(char_to_byte_pos(data, 5), 2); // clamps to len
}

#[test]
fn byte_to_char_pos_at_zero() {
    let data = b"hello";
    assert_eq!(byte_to_char_pos(data, 0), 0);
}

// -----------------------------------------------------------------------
// multibyte_length (strict validation)
// -----------------------------------------------------------------------

#[test]
fn multibyte_length_empty_returns_none() {
    assert_eq!(multibyte_length(&[], false), None);
    assert_eq!(multibyte_length(&[], true), None);
}

#[test]
fn multibyte_length_ascii_is_one() {
    for b in 0u8..0x80 {
        assert_eq!(multibyte_length(&[b], false), Some(1));
    }
}

#[test]
fn multibyte_length_round_trip_unicode() {
    // Encode every code point boundary, then validate length matches.
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    for c in [
        0x80u32, 0x7FF, 0x800, 0xFFFF, 0x1_0000, 0x1F_FFFF, 0x20_0000, 0x3F_FF7F,
    ] {
        let n = char_string(c, &mut buf);
        assert_eq!(
            multibyte_length(&buf[..n], false),
            Some(n),
            "round-trip failed for c=0x{:X}",
            c
        );
    }
}

#[test]
fn multibyte_length_eight_bit_only_with_allow_flag() {
    // Encode a raw byte; standard validation must reject (overlong),
    // `allow_8bit = true` must accept.
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    let n = char_string(byte8_to_char(0x80), &mut buf);
    assert_eq!(n, 2);
    assert_eq!(multibyte_length(&buf[..2], false), None);
    assert_eq!(multibyte_length(&buf[..2], true), Some(2));
}

#[test]
fn multibyte_length_truncated_returns_none() {
    // 3-byte sequence missing trailing byte.
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    let n = char_string(0x4E2D, &mut buf); // CJK char, 3 bytes
    assert_eq!(n, 3);
    assert_eq!(multibyte_length(&buf[..2], false), None);
}

#[test]
fn multibyte_length_stray_continuation_byte() {
    assert_eq!(multibyte_length(&[0x80], true), None);
    assert_eq!(multibyte_length(&[0xBF, 0xBF, 0xBF], true), None);
}

// -----------------------------------------------------------------------
// char_resolve_modifier_mask (GNU character.c:51)
// -----------------------------------------------------------------------

#[test]
fn char_resolve_shift_uppercase() {
    // S-A → A (shift cleared).
    let r = char_resolve_modifier_mask((CHAR_SHIFT as i64) | b'A' as i64);
    assert_eq!(r, b'A' as i64);
}

#[test]
fn char_resolve_shift_lowercase_to_upper() {
    // S-a → A (shift cleared, code shifted).
    let r = char_resolve_modifier_mask((CHAR_SHIFT as i64) | b'a' as i64);
    assert_eq!(r, b'A' as i64);
}

#[test]
fn char_resolve_shift_on_control_dropped() {
    // S-\t (base ≤ 0x20) → \t with shift cleared.
    let r = char_resolve_modifier_mask((CHAR_SHIFT as i64) | 0x09);
    assert_eq!(r, 0x09);
}

#[test]
fn char_resolve_ctl_space_to_nul() {
    // C-SPC → C-@ (NUL, ctl cleared).
    let r = char_resolve_modifier_mask((CHAR_CTL as i64) | b' ' as i64);
    assert_eq!(r, 0);
}

#[test]
fn char_resolve_ctl_question_to_del() {
    // C-? → DEL (0177, ctl cleared).
    let r = char_resolve_modifier_mask((CHAR_CTL as i64) | b'?' as i64);
    assert_eq!(r, 0o177);
}

#[test]
fn char_resolve_ctl_letter() {
    // C-a → 0x01.
    let r = char_resolve_modifier_mask((CHAR_CTL as i64) | b'a' as i64);
    assert_eq!(r, 0x01);
    // C-A → 0x01 too.
    let r = char_resolve_modifier_mask((CHAR_CTL as i64) | b'A' as i64);
    assert_eq!(r, 0x01);
}

#[test]
fn char_resolve_non_ascii_unchanged() {
    // CJK char with M- modifier: returned untouched (only ASCII bases resolve).
    let c = (CHAR_META as i64) | 0x4E2D;
    assert_eq!(char_resolve_modifier_mask(c), c);
}

#[test]
fn char_resolve_meta_preserved() {
    // Meta is intentionally NOT resolved (GNU bug#4751).
    let r = char_resolve_modifier_mask((CHAR_META as i64) | b'A' as i64);
    assert_eq!(r, (CHAR_META as i64) | b'A' as i64);
}

// -----------------------------------------------------------------------
// raw_prev_char_len, string_char_advance, chars_in_text
// -----------------------------------------------------------------------

#[test]
fn raw_prev_char_len_ascii() {
    let s = b"abc";
    assert_eq!(raw_prev_char_len(s, 3), 1);
    assert_eq!(raw_prev_char_len(s, 2), 1);
}

#[test]
fn raw_prev_char_len_multibyte() {
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    let n = char_string(0x4E2D, &mut buf); // 3-byte CJK char
    assert_eq!(raw_prev_char_len(&buf, n), n);
}

#[test]
fn string_char_advance_walks_through() {
    let mut buf = Vec::new();
    let mut tmp = [0u8; MAX_MULTIBYTE_LENGTH];
    for c in [b'A' as u32, 0x4E2D, byte8_to_char(0xFF), b'B' as u32] {
        let n = char_string(c, &mut tmp);
        buf.extend_from_slice(&tmp[..n]);
    }

    let mut pos = 0;
    assert_eq!(string_char_advance(&buf, &mut pos), b'A' as u32);
    assert_eq!(string_char_advance(&buf, &mut pos), 0x4E2D);
    assert_eq!(string_char_advance(&buf, &mut pos), byte8_to_char(0xFF));
    assert_eq!(string_char_advance(&buf, &mut pos), b'B' as u32);
    assert_eq!(pos, buf.len());
}

#[test]
fn chars_in_text_counts() {
    // Build "A中" + raw 0xFF + "B" → 4 chars, encoded with mixed widths.
    let mut buf = Vec::new();
    let mut tmp = [0u8; MAX_MULTIBYTE_LENGTH];
    for c in [b'A' as u32, 0x4E2D, byte8_to_char(0xFF), b'B' as u32] {
        let n = char_string(c, &mut tmp);
        buf.extend_from_slice(&tmp[..n]);
    }
    assert_eq!(chars_in_text(&buf, true), 4);
    assert_eq!(chars_in_text(&buf, false), buf.len());
}

// -----------------------------------------------------------------------
// Unibyte ↔ multibyte conversions (str_to_multibyte / str_as_multibyte / etc.)
// -----------------------------------------------------------------------

#[test]
fn count_size_as_multibyte_basic() {
    assert_eq!(count_size_as_multibyte(b"abc"), 3);
    assert_eq!(count_size_as_multibyte(b"a\xFFb"), 4);
    assert_eq!(count_size_as_multibyte(&[0x80, 0xFF]), 4);
}

#[test]
fn str_to_multibyte_round_trips_via_str_as_unibyte() {
    let src = b"a\x80\xC3\xFF";
    let mb = str_to_multibyte(src);
    // High bytes expand to 2-byte raw-byte form.
    assert_eq!(mb.len(), 1 + 2 + 2 + 2);
    let unib = str_as_unibyte(&mb);
    assert_eq!(unib, src);
}

#[test]
fn str_as_multibyte_preserves_valid_sequences() {
    // Build a valid multibyte buffer first.
    let mut input = Vec::new();
    let mut tmp = [0u8; MAX_MULTIBYTE_LENGTH];
    for c in [b'A' as u32, 0x4E2D, b'B' as u32] {
        let n = char_string(c, &mut tmp);
        input.extend_from_slice(&tmp[..n]);
    }
    let out = str_as_multibyte(&input);
    assert_eq!(out, input);
}

#[test]
fn str_as_multibyte_promotes_lone_high_bytes() {
    // A solitary 0xFF (not part of a valid sequence) becomes a raw-byte char.
    let out = str_as_multibyte(&[b'A', 0xFF, b'B']);
    assert_eq!(multibyte_chars_in_text(&out), 3);
}

#[test]
fn parse_str_as_multibyte_counts() {
    let mut input = Vec::new();
    let mut tmp = [0u8; MAX_MULTIBYTE_LENGTH];
    for c in [b'A' as u32, 0x4E2D, b'B' as u32] {
        let n = char_string(c, &mut tmp);
        input.extend_from_slice(&tmp[..n]);
    }
    let metrics = parse_str_as_multibyte(&input);
    assert_eq!(metrics.chars, 3);
    assert_eq!(metrics.nbytes, input.len());

    // Lone high byte counts as 1 char / 2 bytes.
    let metrics = parse_str_as_multibyte(&[b'A', 0xFF]);
    assert_eq!(metrics.chars, 2);
    assert_eq!(metrics.nbytes, 1 + 2);
}

// -----------------------------------------------------------------------
// string_count_byte8 / string_escape_byte8 / strwidth
// -----------------------------------------------------------------------

#[test]
fn string_count_byte8_unibyte() {
    assert_eq!(string_count_byte8(b"abc", false), 0);
    assert_eq!(string_count_byte8(&[b'a', 0x80, 0xFF], false), 2);
}

#[test]
fn string_count_byte8_multibyte() {
    let mut input = Vec::new();
    let mut tmp = [0u8; MAX_MULTIBYTE_LENGTH];
    for c in [
        b'A' as u32,
        0x4E2D,
        byte8_to_char(0xFF),
        byte8_to_char(0x80),
        b'B' as u32,
    ] {
        let n = char_string(c, &mut tmp);
        input.extend_from_slice(&tmp[..n]);
    }
    assert_eq!(string_count_byte8(&input, true), 2);
}

#[test]
fn string_escape_byte8_unibyte() {
    let out = string_escape_byte8(&[b'a', 0xFF, b'b'], false);
    assert_eq!(out, b"a\\377b");
}

#[test]
fn string_escape_byte8_multibyte() {
    let mut input = Vec::new();
    let mut tmp = [0u8; MAX_MULTIBYTE_LENGTH];
    for c in [b'A' as u32, byte8_to_char(0xFF), b'B' as u32] {
        let n = char_string(c, &mut tmp);
        input.extend_from_slice(&tmp[..n]);
    }
    let out = string_escape_byte8(&input, true);
    assert_eq!(out, b"A\\377B");
}

#[test]
fn strwidth_basic() {
    assert_eq!(strwidth(b"abc", false), 3);
    // CJK char (3 bytes UTF-8) is double-width.
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    let n = char_string(0x4E2D, &mut buf);
    assert_eq!(strwidth(&buf[..n], true), 2);
}

// -----------------------------------------------------------------------
// Unicode general-category predicates
// -----------------------------------------------------------------------

#[test]
fn alphabeticp_letter_categories() {
    assert!(alphabeticp(UnicodeCategory::UppercaseLetter as i64));
    assert!(alphabeticp(UnicodeCategory::LowercaseLetter as i64));
    assert!(alphabeticp(UnicodeCategory::OtherLetter as i64));
    assert!(alphabeticp(UnicodeCategory::NonspacingMark as i64));
    assert!(alphabeticp(UnicodeCategory::LetterNumber as i64));
    assert!(!alphabeticp(UnicodeCategory::DecimalNumber as i64));
    assert!(!alphabeticp(UnicodeCategory::Control as i64));
}

#[test]
fn alphanumericp_includes_nd() {
    assert!(alphanumericp(UnicodeCategory::UppercaseLetter as i64));
    assert!(alphanumericp(UnicodeCategory::DecimalNumber as i64));
    assert!(!alphanumericp(UnicodeCategory::Control as i64));
}

#[test]
fn graphicp_excludes_separators_and_controls() {
    assert!(graphicp(UnicodeCategory::UppercaseLetter as i64));
    assert!(!graphicp(UnicodeCategory::SpaceSeparator as i64));
    assert!(!graphicp(UnicodeCategory::LineSeparator as i64));
    assert!(!graphicp(UnicodeCategory::Control as i64));
    assert!(!graphicp(UnicodeCategory::Surrogate as i64));
    assert!(!graphicp(UnicodeCategory::Unassigned as i64));
}

#[test]
fn printablep_excludes_only_cc_cs_cn() {
    assert!(printablep(UnicodeCategory::SpaceSeparator as i64));
    assert!(printablep(UnicodeCategory::UppercaseLetter as i64));
    assert!(!printablep(UnicodeCategory::Control as i64));
    assert!(!printablep(UnicodeCategory::Surrogate as i64));
    assert!(!printablep(UnicodeCategory::Unassigned as i64));
}

#[test]
fn graphic_base_p_excludes_marks() {
    assert!(graphic_base_p(UnicodeCategory::UppercaseLetter as i64));
    assert!(!graphic_base_p(UnicodeCategory::NonspacingMark as i64));
    assert!(!graphic_base_p(UnicodeCategory::SpacingMark as i64));
    assert!(!graphic_base_p(UnicodeCategory::Format as i64));
}

#[test]
fn blankp_only_zs() {
    assert!(blankp(UnicodeCategory::SpaceSeparator as i64));
    assert!(!blankp(UnicodeCategory::LineSeparator as i64));
    assert!(!blankp(UnicodeCategory::Control as i64));
}

#[test]
fn char_general_category_matches_gnu_unicode_data() {
    // Values cross-checked against GNU `(get-char-code-property C 'general-category)`.
    assert_eq!(
        char_general_category(b'A' as u32),
        Some(UnicodeCategory::UppercaseLetter as i64)
    );
    assert_eq!(
        char_general_category(b'a' as u32),
        Some(UnicodeCategory::LowercaseLetter as i64)
    );
    assert_eq!(
        char_general_category(b';' as u32),
        Some(UnicodeCategory::OtherPunctuation as i64)
    );
    assert_eq!(
        char_general_category(0x00),
        Some(UnicodeCategory::Control as i64)
    );
    assert_eq!(
        char_general_category(0x20),
        Some(UnicodeCategory::SpaceSeparator as i64)
    );
    assert_eq!(
        char_general_category(0x3BB), // GREEK SMALL LETTER LAMBDA (λ)
        Some(UnicodeCategory::LowercaseLetter as i64)
    );
    assert_eq!(
        char_general_category(0x200B), // ZERO WIDTH SPACE
        Some(UnicodeCategory::Format as i64)
    );
    // graphic_base_p, fed by char_general_category, matches GNU's print logic:
    // letters/punctuation are graphic bases, control/format/space are not.
    assert!(char_general_category(b'A' as u32).is_some_and(graphic_base_p));
    assert!(char_general_category(0x3BB).is_some_and(graphic_base_p));
    assert!(!char_general_category(0x00).is_some_and(graphic_base_p));
    assert!(!char_general_category(0x20).is_some_and(graphic_base_p));
    assert!(!char_general_category(0x200B).is_some_and(graphic_base_p));
    // Out-of-range (eight-bit / non-scalar) yields None, like GNU's nil entry.
    assert_eq!(char_general_category(0x110000), None);
}

// -----------------------------------------------------------------------
// EmacsChar newtype (issue #131): make the "char code as Rust char / PUA
// sentinel" mistake un-writable.
// -----------------------------------------------------------------------
#[test]
fn emacs_char_classifies_unicode_eight_bit_and_nonunicode() {
    // A real Private-Use glyph (nerd-font icon) is a Unicode scalar value.
    let glyph = EmacsChar::from_code(0xE0A0).unwrap();
    assert!(glyph.is_unicode_scalar());
    assert!(!glyph.is_byte8());
    assert_eq!(glyph.as_rust_char(), Some('\u{E0A0}'));
    assert_eq!(glyph.to_byte8(), None);
    // Its canonical bytes are real extended UTF-8, never an in-Unicode sentinel.
    assert_eq!(glyph.to_emacs_bytes(), vec![0xEE, 0x82, 0xA0]);

    // An eight-bit raw byte is disjoint from Unicode.
    let raw = EmacsChar::from_byte8(0xA0);
    assert_eq!(raw.code(), 0x3FFFA0);
    assert!(!raw.is_unicode_scalar());
    assert!(raw.is_byte8());
    assert_eq!(raw.as_rust_char(), None);
    assert_eq!(raw.to_byte8(), Some(0xA0));

    // The glyph and the raw byte are distinct EmacsChars (the old storage
    // string collapsed both onto U+E0A0).
    assert_ne!(glyph, raw);

    // A non-Unicode code (> 0x10FFFF, below the eight-bit range) is neither.
    let nonuni = EmacsChar::from_code(0x110000).unwrap();
    assert!(!nonuni.is_unicode_scalar());
    assert!(!nonuni.is_byte8());
    assert_eq!(nonuni.to_byte8(), None);
}

#[test]
fn emacs_char_range_and_unibyte_promotion() {
    assert!(EmacsChar::from_code(MAX_CHAR).is_some());
    assert!(EmacsChar::from_code(MAX_CHAR + 1).is_none());
    // ASCII unibyte byte stays itself; a high byte promotes to eight-bit.
    assert_eq!(EmacsChar::from_unibyte_byte(0x41).code(), 0x41);
    assert_eq!(EmacsChar::from_unibyte_byte(0xFF).code(), 0x3FFFFF);
    // char_string round-trips through string_char for every kind.
    for code in [0x41u32, 0xE0A0, 0x10FFFF, 0x110000, 0x3FFFA0, MAX_CHAR] {
        let ec = EmacsChar::from_code(code).unwrap();
        let bytes = ec.to_emacs_bytes();
        let (decoded, len) = string_char(&bytes);
        assert_eq!(len, bytes.len());
        assert_eq!(decoded, code, "round-trip {code:#x}");
    }
}

#[test]
fn str_to_unibyte_narrows_each_char_to_low_byte() {
    // Mirrors GNU `copy_text` multibyte->unibyte (insdel.c:632-648): each
    // character collapses to its low byte `c & 0xFF`, one byte per char.

    // ASCII passes through unchanged.
    assert_eq!(str_to_unibyte(b"abc"), b"abc".to_vec());

    // A regular multibyte char é (U+00E9) is stored as the 2 internal bytes
    // C3 A9 but narrows to the single byte 0xE9 (= 233), NOT the raw byte
    // pair. This is the behavior `search_buffer_non_re` relies on so a true
    // multibyte char fails to match its UTF-8 bytes in a unibyte buffer.
    let e_acute_internal = {
        let mut buf = [0u8; 8];
        let n = char_string(0x00E9, &mut buf);
        buf[..n].to_vec()
    };
    assert_eq!(e_acute_internal, vec![0xC3, 0xA9]);
    assert_eq!(str_to_unibyte(&e_acute_internal), vec![0xE9]);

    // An eight-bit raw-byte char (byte + 0x3FFF00) narrows back to its byte.
    let raw = {
        let mut buf = [0u8; 8];
        let n = char_string(byte8_to_char(0xA9), &mut buf);
        buf[..n].to_vec()
    };
    assert_eq!(str_to_unibyte(&raw), vec![0xA9]);

    // A 3-byte CJK char 中 (U+4E2D) narrows to its low byte 0x2D.
    let zhong = {
        let mut buf = [0u8; 8];
        let n = char_string(0x4E2D, &mut buf);
        buf[..n].to_vec()
    };
    assert_eq!(str_to_unibyte(&zhong), vec![0x2D]);
}

#[test]
fn unibyte_case_fold_match_len_advances_byte_by_byte() {
    // Unibyte text C3 A9 41 A9: a search for byte 0xA9 must find the byte
    // EMBEDDED in the would-be C3 A9 multibyte lead (at offset 1), not skip to
    // the standalone trailing 0xA9 (offset 3). This is GNU `simple_search`'s
    // unibyte per-byte advance (search.c:1622-1633).
    let hay = [0xC3u8, 0xA9, 0x41, 0xA9];
    assert_eq!(unibyte_case_fold_match_len(&hay, 1, &[0xA9]), Some(2));
    assert_eq!(unibyte_case_fold_match_len(&hay, 0, &[0xA9]), None);
    assert_eq!(unibyte_case_fold_match_len(&hay, 3, &[0xA9]), Some(4));

    // Latin-1 case folding on raw bytes: É (0xC9) folds to é (0xE9).
    let hay2 = [0xE9u8];
    assert_eq!(unibyte_case_fold_match_len(&hay2, 0, &[0xC9]), Some(1));
    assert_eq!(unibyte_case_fold_match_len(&hay2, 0, &[0xE9]), Some(1));
    // A byte with no case partner compares verbatim.
    assert_eq!(unibyte_case_fold_match_len(&[0xA9u8], 0, &[0xA9]), Some(1));
    assert_eq!(unibyte_case_fold_match_len(&[0xA9u8], 0, &[0xAA]), None);

    // ASCII folding still works.
    assert_eq!(unibyte_case_fold_match_len(b"ABC", 0, b"abc"), Some(3));
}
