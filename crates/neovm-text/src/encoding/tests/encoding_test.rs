use super::*;

use crate::emacs_core::coding::EolConversion;

use neovm_value::error::FlowKind;

use neovm_value::value::{Value, get_string_text_properties_for_value};

#[test]
fn ascii_width() {
    crate::test_utils::init_test_tracing();
    assert_eq!(char_width('a'), 1);
    assert_eq!(char_width(' '), 1);
    assert_eq!(char_width('Z'), 1);
}

#[test]
fn cjk_width() {
    crate::test_utils::init_test_tracing();
    assert_eq!(char_width('中'), 2);
    assert_eq!(char_width('日'), 2);
    assert_eq!(char_width('あ'), 2);
    assert_eq!(char_width('ア'), 2);
}

#[test]
fn gnu_default_emoji_symbol_widths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(char_width('\u{2603}'), 1);
    assert_eq!(char_width('\u{2615}'), 2);
    assert_eq!(char_width('\u{263A}'), 1);
}

#[test]
fn control_char_width() {
    crate::test_utils::init_test_tracing();
    assert_eq!(char_width('\0'), 2);
    assert_eq!(char_width('\x01'), 2); // ^A
    assert_eq!(char_width('\n'), 0);
    assert_eq!(char_width('\x7f'), 2); // ^?
    assert_eq!(char_width('\u{0080}'), 4);
    assert_eq!(char_width('\u{009f}'), 4);
}

#[test]
fn string_width_mixed() {
    crate::test_utils::init_test_tracing();
    assert_eq!(string_width("hello"), 5);
    assert_eq!(string_width("中文"), 4);
    assert_eq!(string_width("hi中"), 4);
}

#[test]
fn encode_coding_string_extracts_multibyte_byte8_chars_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"Hello ");
    let mut char_buf = [0u8; neovm_value::emacs_char::MAX_MULTIBYTE_LENGTH];
    for byte in [0xC3, 0xA9] {
        let len = neovm_value::emacs_char::char_string(
            neovm_value::emacs_char::byte8_to_char(byte),
            &mut char_buf,
        );
        bytes.extend_from_slice(&char_buf[..len]);
    }
    let source = Value::heap_string(neovm_value::heap_types::LispString::from_emacs_bytes(bytes));

    for coding in ["utf-8", "utf-8-emacs", "raw-text", "no-conversion"] {
        let encoded = builtin_encode_coding_string(vec![source, Value::symbol(coding)])
            .expect("encode-coding-string should evaluate");
        let ls = encoded
            .as_lisp_string()
            .expect("encode-coding-string should return a string");
        assert_eq!(ls.as_bytes(), b"Hello \xC3\xA9", "{coding}");
        assert!(
            !ls.is_multibyte(),
            "{coding} result should be a unibyte byte string"
        );
    }
}

#[test]
fn nil_coding_string_respects_nocopy_identity() {
    crate::test_utils::init_test_tracing();
    let source = Value::string("abc");

    let encoded_copy =
        builtin_encode_coding_string(vec![source, Value::NIL, Value::NIL]).expect("nil coding");
    assert_eq!(encoded_copy.as_utf8_str(), Some("abc"));
    assert!(!neovm_value::value::eq_value(&source, &encoded_copy));

    let encoded_nocopy =
        builtin_encode_coding_string(vec![source, Value::NIL, Value::T]).expect("nil coding");
    assert!(neovm_value::value::eq_value(&source, &encoded_nocopy));

    let decoded_copy =
        builtin_decode_coding_string(vec![source, Value::NIL, Value::NIL]).expect("nil coding");
    assert_eq!(decoded_copy.as_utf8_str(), Some("abc"));
    assert!(!neovm_value::value::eq_value(&source, &decoded_copy));

    let decoded_nocopy =
        builtin_decode_coding_string(vec![source, Value::NIL, Value::T]).expect("nil coding");
    assert!(neovm_value::value::eq_value(&source, &decoded_nocopy));
}

#[test]
fn ascii_coding_string_respects_nocopy_fast_path_identity() {
    crate::test_utils::init_test_tracing();
    let source = Value::string("abc");

    let encoded_copy =
        builtin_encode_coding_string(vec![source, Value::symbol("utf-8"), Value::NIL])
            .expect("utf-8 encode");
    assert_eq!(encoded_copy.as_utf8_str(), Some("abc"));
    assert!(!neovm_value::value::eq_value(&source, &encoded_copy));

    let encoded_nocopy =
        builtin_encode_coding_string(vec![source, Value::symbol("utf-8"), Value::T])
            .expect("utf-8 encode");
    assert!(neovm_value::value::eq_value(&source, &encoded_nocopy));

    let decoded_copy =
        builtin_decode_coding_string(vec![source, Value::symbol("utf-8"), Value::NIL])
            .expect("utf-8 decode");
    assert_eq!(decoded_copy.as_utf8_str(), Some("abc"));
    assert!(!neovm_value::value::eq_value(&source, &decoded_copy));

    let decoded_nocopy =
        builtin_decode_coding_string(vec![source, Value::symbol("utf-8"), Value::T])
            .expect("utf-8 decode");
    assert!(neovm_value::value::eq_value(&source, &decoded_nocopy));
}

#[test]
fn ascii_coding_string_nocopy_allocates_when_eol_conversion_needed() {
    crate::test_utils::init_test_tracing();
    let encode_source = Value::string("a\nb");
    let encoded =
        builtin_encode_coding_string(vec![encode_source, Value::symbol("utf-8-dos"), Value::T])
            .expect("utf-8-dos encode");
    assert_eq!(encoded.as_lisp_string().unwrap().as_bytes(), b"a\r\nb");
    assert!(!neovm_value::value::eq_value(&encode_source, &encoded));

    let decode_source = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(
        b"a\r\nb".to_vec(),
    ));
    let decoded =
        builtin_decode_coding_string(vec![decode_source, Value::symbol("utf-8-dos"), Value::T])
            .expect("utf-8-dos decode");
    assert_eq!(decoded.as_utf8_str(), Some("a\nb"));
    assert!(!neovm_value::value::eq_value(&decode_source, &decoded));
}

#[test]
fn encode_lisp_string_emacs_internal_uses_utf8_emacs_alias() {
    crate::test_utils::init_test_tracing();
    let text = neovm_value::heap_types::LispString::from_utf8("abc\n");

    assert_eq!(encode_lisp_string(&text, "emacs-internal"), b"abc\n");
    assert_eq!(encode_lisp_string(&text, "emacs-internal-dos"), b"abc\r\n");
}

#[test]
fn decode_latin1_attaches_charset_text_property() {
    crate::test_utils::init_test_tracing();
    let encoded = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        0xE9,
    ]));
    let decoded = builtin_decode_coding_string(vec![encoded, Value::symbol("latin-1")])
        .expect("latin-1 decode should succeed");
    if !decoded.is_string() {
        panic!("decode-coding-string should return a string");
    };
    let props = get_string_text_properties_for_value(decoded)
        .expect("decoded string should be propertized");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].start, 0);
    assert_eq!(props[0].end, 1);
    assert_eq!(
        props[0].plist,
        Value::list(vec![Value::symbol("charset"), Value::symbol("iso-8859-1")])
    );
}

#[test]
fn decode_latin1_charset_property_spans_ascii_like_gnu() {
    crate::test_utils::init_test_tracing();
    let encoded = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        b'A', 0xE9, b'B',
    ]));
    let decoded = builtin_decode_coding_string(vec![encoded, Value::symbol("latin-1")])
        .expect("latin-1 decode should succeed");

    assert_eq!(
        decoded.as_lisp_string().and_then(|s| s.as_utf8_str()),
        Some("AéB")
    );
    let props = get_string_text_properties_for_value(decoded)
        .expect("decoded Latin-1 string should be propertized");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].start, 0);
    assert_eq!(props[0].end, 3);
    assert_eq!(
        props[0].plist,
        Value::list(vec![Value::symbol("charset"), Value::symbol("iso-8859-1")])
    );
}

#[test]
fn encode_no_conversion_preserves_unibyte_storage_bytes() {
    crate::test_utils::init_test_tracing();
    let source = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        0xE9,
    ]));
    let encoded =
        builtin_encode_coding_string(vec![source, Value::symbol("no-conversion")]).unwrap();
    if !encoded.is_string() {
        panic!("encode-coding-string should return a string");
    };
    assert!(!encoded.string_is_multibyte());
    let ls = encoded.as_lisp_string().unwrap();
    assert_eq!(ls.as_bytes(), &[0xE9]);
}

#[test]
fn decode_no_conversion_returns_unibyte_bytes_for_non_ascii_input() {
    crate::test_utils::init_test_tracing();
    let encoded =
        builtin_encode_coding_string(vec![Value::string("é"), Value::symbol("no-conversion")])
            .expect("encoding should succeed");
    let decoded =
        builtin_decode_coding_string(vec![encoded, Value::symbol("no-conversion")]).unwrap();
    if !decoded.is_string() {
        panic!("decode-coding-string should return a string");
    };
    assert!(!decoded.string_is_multibyte());
    let ls = decoded.as_lisp_string().unwrap();
    assert_eq!(ls.as_bytes(), &[0xC3, 0xA9]);
}

#[test]
fn decode_byte_preserving_ascii_uses_multibyte_fast_path() {
    crate::test_utils::init_test_tracing();
    for coding in ["binary", "no-conversion", "raw-text"] {
        let source = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(
            b"Hello".to_vec(),
        ));
        let decoded =
            builtin_decode_coding_string(vec![source, Value::symbol(coding)]).expect("decode");
        assert!(
            decoded.string_is_multibyte(),
            "{coding} should decode ASCII as multibyte"
        );
        assert_eq!(decoded.as_lisp_string().unwrap().as_bytes(), b"Hello");
    }
}

#[test]
fn encoding_utf8() {
    crate::test_utils::init_test_tracing();
    let bytes = encode_string("hello", "utf-8");
    assert_eq!(bytes, b"hello");
    let decoded = decode_bytes(b"hello", "utf-8");
    assert_eq!(decoded, "hello");
}

#[test]
fn encoding_utf8_with_signature_consumes_leading_bom() {
    crate::test_utils::init_test_tracing();
    let decoded = decode_bytes(b"\xEF\xBB\xBF;;; fixture\n", "utf-8-with-signature-unix");
    assert_eq!(decoded, ";;; fixture\n");
}

#[test]
fn encoding_utf8_dos_applies_eol_conversion() {
    crate::test_utils::init_test_tracing();
    let bytes = encode_string("a\nb", "utf-8-dos");
    assert_eq!(bytes, b"a\r\nb");
    let decoded = decode_bytes(b"a\r\nb", "utf-8-dos");
    assert_eq!(decoded, "a\nb");
}

#[test]
fn raw_text_dos_preserves_bytes_but_converts_eol() {
    crate::test_utils::init_test_tracing();
    let encoded =
        builtin_encode_coding_string(vec![Value::string("a\nb"), Value::symbol("raw-text-dos")])
            .unwrap();
    if !encoded.is_string() {
        panic!("encode-coding-string should return a string");
    };
    let ls = encoded.as_lisp_string().unwrap();
    assert_eq!(ls.as_bytes(), b"a\r\nb");

    let decoded = builtin_decode_coding_string(vec![
        Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(
            b"a\r\nb".to_vec(),
        )),
        Value::symbol("raw-text-dos"),
    ])
    .unwrap();
    if !decoded.is_string() {
        panic!("decode-coding-string should return a string");
    };
    let ls = decoded.as_lisp_string().unwrap();
    assert_eq!(ls.as_bytes(), b"a\nb");
}

#[test]
fn encoding_latin1() {
    crate::test_utils::init_test_tracing();
    let bytes = encode_string("café", "latin-1");
    assert_eq!(bytes.len(), 4); // é maps to 0xe9
    let decoded = decode_bytes(&[0x63, 0x61, 0x66, 0xe9], "latin-1");
    assert_eq!(decoded, "café");
}

#[test]
fn encoding_big5_decodes_generated_leim_dictionary_bytes_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(decode_bytes(&[0xa4, 0x40], "big5"), "\u{4e00}");
    assert_eq!(decode_bytes(&[0xa4, 0x40], "chinese-big5-unix"), "\u{4e00}");
    assert_eq!(decode_bytes(&[0xa4, 0x40], "cp950"), "\u{4e00}");
    assert_eq!(
        decode_bytes(&[0xa4, 0x40, b'\r', b'\n'], "big5-dos"),
        "一\n"
    );
    assert_eq!(encode_string("一", "big5"), vec![0xa4, 0x40]);
    assert_eq!(encode_string("一", "chinese-big5-unix"), vec![0xa4, 0x40]);
}

#[test]
fn encoding_gb2312_decodes_generated_leim_dictionary_bytes_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(decode_bytes(&[0xd2, 0xbb], "cn-gb-2312"), "一");
    assert_eq!(decode_bytes(&[0xd2, 0xbb], "chinese-iso-8bit-unix"), "一");
    assert_eq!(
        decode_bytes(&[0xd2, 0xbb, b'\r', b'\n'], "gb2312-dos"),
        "一\n"
    );
    assert_eq!(encode_string("一", "cn-gb-2312"), vec![0xd2, 0xbb]);
    assert_eq!(
        encode_string("一", "chinese-iso-8bit-unix"),
        vec![0xd2, 0xbb]
    );
}

#[test]
fn decode_coding_string_big5_marks_charset_like_gnu() {
    crate::test_utils::init_test_tracing();
    let decoded = builtin_decode_coding_string(vec![
        Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
            0xa4, 0x40,
        ])),
        Value::symbol("chinese-big5-unix"),
    ])
    .expect("decode-coding-string chinese-big5-unix should succeed");

    assert_eq!(
        decoded.as_lisp_string().and_then(|s| s.as_utf8_str()),
        Some("一")
    );
    let props = get_string_text_properties_for_value(decoded)
        .expect("decoded Big5 string should be propertized");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].start, 0);
    assert_eq!(props[0].end, 1);
    assert_eq!(
        props[0].plist,
        Value::list(vec![Value::symbol("charset"), Value::symbol("big5")])
    );
}

#[test]
fn decode_coding_string_gb2312_marks_charset_like_gnu() {
    crate::test_utils::init_test_tracing();
    let decoded = builtin_decode_coding_string(vec![
        Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
            0xd2, 0xbb,
        ])),
        Value::symbol("cn-gb-2312-unix"),
    ])
    .expect("decode-coding-string cn-gb-2312-unix should succeed");

    assert_eq!(
        decoded.as_lisp_string().and_then(|s| s.as_utf8_str()),
        Some("一")
    );
    let props = get_string_text_properties_for_value(decoded)
        .expect("decoded GB2312 string should be propertized");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].start, 0);
    assert_eq!(props[0].end, 1);
    assert_eq!(
        props[0].plist,
        Value::list(vec![
            Value::symbol("charset"),
            Value::symbol("chinese-gb2312"),
        ])
    );
}

#[test]
fn decode_coding_string_gb2312_extends_charset_after_first_non_ascii_like_gnu() {
    crate::test_utils::init_test_tracing();
    let decoded = builtin_decode_coding_string(vec![
        Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
            b'A', 0xd2, 0xbb, b'B',
        ])),
        Value::symbol("cn-gb-2312-unix"),
    ])
    .expect("decode-coding-string cn-gb-2312-unix should succeed");

    assert_eq!(
        decoded.as_lisp_string().and_then(|s| s.as_utf8_str()),
        Some("A一B")
    );
    let props = get_string_text_properties_for_value(decoded)
        .expect("decoded GB2312 string should be propertized");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].start, 1);
    assert_eq!(props[0].end, 3);
    assert_eq!(
        props[0].plist,
        Value::list(vec![
            Value::symbol("charset"),
            Value::symbol("chinese-gb2312"),
        ])
    );
}

#[test]
fn glyphless_display() {
    crate::test_utils::init_test_tracing();
    assert_eq!(glyphless_char_display('\x01'), "^A");
    assert_eq!(glyphless_char_display('\x7f'), "^?");
    assert_eq!(glyphless_char_display('\u{FEFF}'), "\\uFEFF");
}

#[test]
fn multibyte_detection() {
    crate::test_utils::init_test_tracing();
    assert!(
        !neovm_value::value::Value::string("hello")
            .as_lisp_string()
            .expect("string")
            .is_multibyte()
    );
    assert!(
        neovm_value::value::Value::string("héllo")
            .as_lisp_string()
            .expect("string")
            .is_multibyte()
    );
    assert!(
        neovm_value::value::Value::string("中文")
            .as_lisp_string()
            .expect("string")
            .is_multibyte()
    );
}

#[test]
fn printable_check() {
    crate::test_utils::init_test_tracing();
    assert!(is_printable('a'));
    assert!(is_printable('中'));
    assert!(!is_printable('\x00'));
    assert!(!is_printable('\x7f'));
}

#[test]
fn decode_iso2022_attaches_charset_property_for_designated_charset() {
    crate::test_utils::init_test_tracing();
    use neovm_value::intern::intern;
    register_test_jis();
    let ascii = intern("ascii");
    // 7-bit ISO-2022 with escape designations (iso-2022-7bit profile).
    let spec = crate::emacs_core::coding::Iso2022Spec {
        initial: [Some(ascii), None, None, None],
        request: vec![],
        reg_usage: (0, 1),
        flags: enumflags2::BitFlags::from_flag(crate::emacs_core::coding::IsoFlag::SevenBits)
            | crate::emacs_core::coding::IsoFlag::Designation,
    };
    // "X" + ESC$B designation + two dim-2 chars + ESC(B (ascii) + "Y".
    let decoded = decode_via_iso2022(
        b"X\x1b$B$3$s\x1b(BY",
        &spec,
        &[intern("iso-2022")],
        &mut Iso2022DecodeState::default(),
        DosEolLookahead::NotRequired,
    );
    let (bytes, runs) = (decoded.bytes, decoded.runs);
    let text = neovm_value::emacs_char::to_utf8_lossy(&bytes);
    // X + 2 kanji + Y == 4 chars.  The run starts at the first kanji and runs
    // to the end of the text: re-designating G0 back to `ascii` does not close
    // it, because GNU's `decode_coding_iso_2022` only moves `last_id` for a
    // non-ASCII charset.  GNU Emacs 31 decoding the same bytes returns
    // `#("XこんY" 1 4 (charset japanese-jisx0208))`.
    assert_eq!(text.chars().count(), 4);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].start, 1);
    assert_eq!(runs[0].end, 4);
    assert_eq!(runs[0].plist, charset_prop("test-jis"));
}

#[test]
fn iso2022_encoding_boundary_controls_final_ascii_reset() {
    crate::test_utils::init_test_tracing();
    use neovm_value::intern::intern;

    let jis = register_test_jis();
    let spec = crate::emacs_core::coding::Iso2022Spec {
        initial: [Some(intern("ascii")), None, None, None],
        request: vec![],
        reg_usage: (0, 1),
        flags: enumflags2::BitFlags::from_flag(crate::emacs_core::coding::IsoFlag::SevenBits)
            | crate::emacs_core::coding::IsoFlag::Designation
            | crate::emacs_core::coding::IsoFlag::AsciiAtEol,
    };
    let source = neovm_value::heap_types::LispString::from_utf8("\u{10000}");

    let complete = encode_via_iso2022(
        &source,
        &spec,
        &[intern("ascii"), jis],
        EncodingBoundary::CompleteText,
        &mut CodingEncoderState::default(),
    );
    let file_region = encode_via_iso2022(
        &source,
        &spec,
        &[intern("ascii"), jis],
        EncodingBoundary::FileRegion,
        &mut CodingEncoderState::default(),
    );

    assert_eq!(complete, b"\x1b$B!!\x1b(B");
    assert_eq!(file_region, b"\x1b$B!!");
}

#[test]
fn decode_euc_attaches_charset_property() {
    crate::test_utils::init_test_tracing();
    let jis = register_test_jis();
    // EUC profile: G1 = test-jis, decoded from GR (high-bit) bytes.
    let spec = crate::emacs_core::coding::Iso2022Spec {
        initial: [
            Some(neovm_value::intern::intern("ascii")),
            Some(jis),
            None,
            None,
        ],
        request: vec![],
        reg_usage: (0, 1),
        flags: enumflags2::BitFlags::empty(),
    };
    // ASCII 'A' then one GR character (0xA4 0xB3 -> GL 0x24 0x33).
    let decoded = decode_via_euc(b"A\xA4\xB3", &spec, DosEolLookahead::NotRequired);
    let (bytes, runs) = (decoded.bytes, decoded.runs);
    let text = neovm_value::emacs_char::to_utf8_lossy(&bytes);
    assert_eq!(text.chars().count(), 2);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].start, 1);
    assert_eq!(runs[0].end, 2);
    assert_eq!(runs[0].plist, charset_prop("test-jis"));
}

#[test]
fn decode_sjis_charset_runs_absorb_ascii_like_gnu() {
    crate::test_utils::init_test_tracing();
    use neovm_value::intern::intern;
    // GNU's `decode_coding_sjis` only moves `last_id`/`last_offset` for a
    // non-ASCII charset, so ASCII characters never close the run being
    // accumulated.  A run ends only where the next *different* non-ASCII
    // charset starts, or at the end of the decoded text - trailing ASCII
    // included.  Real GNU Emacs 31 decoding the equivalent Shift-JIS bytes
    // produces exactly three runs:
    //
    //   #("ABあCDｱｲEFいGH" 2 5 (charset japanese-jisx0208)
    //                      5 9 (charset katakana-jisx0201)
    //                      9 12 (charset japanese-jisx0208))
    let mut kana_args = vec![Value::NIL; 17];
    kana_args[0] = Value::symbol("test-katakana");
    kana_args[1] = Value::fixnum(1);
    kana_args[2] = Value::vector(vec![Value::fixnum(33), Value::fixnum(126)]);
    kana_args[11] = Value::fixnum(0x20000);
    crate::emacs_core::charset::builtin_define_charset_internal(kana_args).unwrap();
    let jis = register_test_jis();
    let charsets = vec![intern("ascii"), intern("test-katakana"), jis];

    let kanji_one = jis_to_sjis(0x2433);
    let kanji_two = jis_to_sjis(0x2434);
    let input = vec![
        b'A',
        b'B',
        kanji_one.0,
        kanji_one.1,
        b'C',
        b'D',
        0xA1,
        0xA2,
        b'E',
        b'F',
        kanji_two.0,
        kanji_two.1,
        b'G',
        b'H',
    ];
    let decoded = decode_via_sjis(&input, &charsets, DosEolLookahead::NotRequired);
    let (bytes, runs) = (decoded.bytes, decoded.runs);
    let text = neovm_value::emacs_char::to_utf8_lossy(&bytes);
    assert_eq!(text.chars().count(), 12);

    assert_eq!(runs.len(), 3);
    // The first kanji run swallows the following "CD" and stops where the
    // katakana charset takes over.
    assert_eq!((runs[0].start, runs[0].end), (2, 5));
    assert_eq!(runs[0].plist, charset_prop("test-jis"));
    // The katakana run swallows the following "EF".
    assert_eq!((runs[1].start, runs[1].end), (5, 9));
    assert_eq!(runs[1].plist, charset_prop("test-katakana"));
    // The final kanji run reaches the end of the text, trailing "GH" included.
    assert_eq!((runs[2].start, runs[2].end), (9, 12));
    assert_eq!(runs[2].plist, charset_prop("test-jis"));
}

/// GNU `Fdefine_charset_alias` stores the target's attribute vector
/// (charset.c:1296). Rebinding that name replaces only its hash slot.
/// Redefining it overwrites `charset_table[id]` and installs a new vector for
/// that name; the alias keeps the old vector and still decodes through the id.
#[test]
fn charset_alias_keeps_captured_vector_and_redefine_splits_plist() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{
        builtin_charset_id_internal, builtin_charset_plist, builtin_define_charset_alias,
        builtin_define_charset_internal, builtin_iso_charset, charset_decode_char,
        charset_encode_char, reset_charset_registry, set_charset_plist_value,
    };
    use neovm_value::plist::plist_get;
    use neovm_value::value::eq_value;

    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            reset_charset_registry();
        }
    }
    let _reset = Reset;
    reset_charset_registry();

    let id_of = |name: &'static str| {
        builtin_charset_id_internal(vec![Value::symbol(name)])
            .unwrap()
            .as_int()
            .unwrap()
    };
    let plist_of = |name: &'static str| builtin_charset_plist(vec![Value::symbol(name)]).unwrap();
    let prop = |plist: &Value, key: &'static str| plist_get(*plist, &Value::symbol(key));

    builtin_define_charset_alias(vec![
        Value::symbol("probe"),
        Value::symbol("latin-iso8859-1"),
    ])
    .unwrap();
    builtin_define_charset_alias(vec![
        Value::symbol("latin-iso8859-1"),
        Value::symbol("ascii"),
    ])
    .unwrap();
    assert_eq!(id_of("probe"), 5);
    assert_eq!(id_of("latin-iso8859-1"), 0);
    assert!(
        prop(&plist_of("probe"), ":name")
            .unwrap()
            .is_bare_symbol_named("latin-iso8859-1")
    );
    assert!(
        prop(&plist_of("latin-iso8859-1"), ":name")
            .unwrap()
            .is_bare_symbol_named("ascii")
    );

    let define = |offset: i64, final_char: i64, mark: Option<i64>| {
        let mut elems = vec![
            Value::symbol(":name"),
            Value::symbol("redef-x"),
            Value::symbol(":code-offset"),
            Value::fixnum(offset),
            Value::symbol(":iso-final-char"),
            Value::fixnum(final_char),
        ];
        if let Some(mark) = mark {
            elems.push(Value::symbol(":probe-mark"));
            elems.push(Value::fixnum(mark));
        }
        let plist = Value::list(elems);
        let mut args = vec![Value::NIL; 17];
        args[0] = Value::symbol("redef-x");
        args[1] = Value::fixnum(1);
        args[2] = Value::vector(vec![Value::fixnum(33), Value::fixnum(126)]);
        args[5] = Value::fixnum(final_char);
        args[11] = Value::fixnum(offset);
        args[16] = plist;
        builtin_define_charset_internal(args).unwrap();
    };
    define(500, i64::from(b'9'), None);
    builtin_define_charset_alias(vec![Value::symbol("redef-probe"), Value::symbol("redef-x")])
        .unwrap();
    let shared = Value::list(vec![
        Value::symbol(":name"),
        Value::symbol("redef-x"),
        Value::symbol(":code-offset"),
        Value::fixnum(500),
        Value::symbol(":iso-final-char"),
        Value::fixnum(i64::from(b'9')),
        Value::symbol(":probe-mark"),
        Value::fixnum(1),
    ]);
    let pairs = neovm_value::value::list_to_vec(&shared).unwrap();
    let mut stored = Vec::new();
    let mut i = 0;
    while i + 1 < pairs.len() {
        stored.push((
            pairs[i]
                .as_symbol_id(neovm_value::tagged::value::SymbolMode::Bare)
                .unwrap(),
            pairs[i + 1],
        ));
        i += 2;
    }
    set_charset_plist_value(neovm_value::intern::intern("redef-x"), stored, shared);
    let probe_plist = plist_of("redef-probe");
    let base_plist = plist_of("redef-x");
    assert!(eq_value(&probe_plist, &base_plist));
    assert!(eq_value(&probe_plist, &shared));
    assert_eq!(prop(&probe_plist, ":probe-mark").unwrap().as_int(), Some(1));
    assert_eq!(id_of("redef-probe"), id_of("redef-x"));
    let probe = neovm_value::intern::intern("redef-probe");
    let base = neovm_value::intern::intern("redef-x");
    assert_eq!(charset_decode_char(probe, 65), Some(532));
    assert_eq!(charset_decode_char(base, 65), Some(532));

    define(800, i64::from(b'8'), None);
    let probe_plist = plist_of("redef-probe");
    let base_plist = plist_of("redef-x");
    assert!(!eq_value(&probe_plist, &base_plist));
    assert_eq!(id_of("redef-probe"), id_of("redef-x"));
    assert_eq!(
        prop(&probe_plist, ":code-offset").unwrap().as_int(),
        Some(500)
    );
    assert_eq!(
        prop(&base_plist, ":code-offset").unwrap().as_int(),
        Some(800)
    );
    assert_eq!(
        prop(&probe_plist, ":iso-final-char").unwrap().as_int(),
        Some(i64::from(b'9'))
    );
    assert_eq!(
        prop(&base_plist, ":iso-final-char").unwrap().as_int(),
        Some(i64::from(b'8'))
    );
    assert_eq!(prop(&probe_plist, ":probe-mark").unwrap().as_int(), Some(1));
    assert!(prop(&base_plist, ":probe-mark").is_none());
    assert_eq!(charset_decode_char(probe, 65), Some(832));
    assert_eq!(charset_decode_char(base, 65), Some(832));
    assert_eq!(charset_encode_char(probe, 532), None);
    assert_eq!(charset_encode_char(base, 532), None);
    assert_eq!(charset_encode_char(probe, 832), Some(65));
    assert_eq!(charset_encode_char(base, 832), Some(65));
    let iso = |final_char: u8| {
        builtin_iso_charset(vec![
            Value::fixnum(1),
            Value::fixnum(94),
            Value::fixnum(i64::from(final_char)),
        ])
        .unwrap()
    };
    assert!(iso(b'9').is_bare_symbol_named("redef-x"));
    assert!(iso(b'8').is_bare_symbol_named("redef-x"));

    let after = Value::list(vec![
        Value::symbol(":name"),
        Value::symbol("redef-x"),
        Value::symbol(":code-offset"),
        Value::fixnum(800),
        Value::symbol(":after"),
        Value::fixnum(2),
    ]);
    let pairs = neovm_value::value::list_to_vec(&after).unwrap();
    let stored: Vec<_> = pairs
        .chunks(2)
        .map(|pair| {
            (
                pair[0]
                    .as_symbol_id(neovm_value::tagged::value::SymbolMode::Bare)
                    .unwrap(),
                pair[1],
            )
        })
        .collect();
    set_charset_plist_value(base, stored, after);
    assert!(prop(&plist_of("redef-probe"), ":after").is_none());
    assert_eq!(
        prop(&plist_of("redef-x"), ":after").unwrap().as_int(),
        Some(2)
    );
    assert_eq!(
        prop(&plist_of("redef-probe"), ":code-offset")
            .unwrap()
            .as_int(),
        Some(500)
    );
    assert_eq!(charset_decode_char(probe, 65), Some(832));
}

/// ISO-2022 keeps the charset that was in the designation table when the
/// final was declared. An alias of that name does not retarget the bytes.
/// An unknown final, or a charset with no ISO final, is replayed one byte at
/// a time (`invalid_code`), and the table write still stands.
#[test]
fn iso2022_designation_uses_table_identity_and_replays_unsafe() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{
        builtin_declare_equiv_charset, builtin_define_charset_alias,
        builtin_define_charset_internal, builtin_iso_charset, reset_charset_registry,
    };
    use crate::emacs_core::coding::{Iso2022Spec, IsoFlag};
    use neovm_value::intern::intern;

    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            reset_charset_registry();
        }
    }
    let _reset = Reset;
    reset_charset_registry();

    let spec = Iso2022Spec {
        initial: [Some(intern("ascii")), None, None, None],
        request: vec![],
        reg_usage: (0, 0),
        flags: enumflags2::BitFlags::from_flag(IsoFlag::SevenBits)
            | IsoFlag::Designation
            | IsoFlag::AsciiAtEol,
    };
    let decode = |bytes: &[u8]| {
        decode_via_iso2022(
            bytes,
            &spec,
            &[intern("iso-2022")],
            &mut Iso2022DecodeState::default(),
            DosEolLookahead::NotRequired,
        )
    };

    builtin_declare_equiv_charset(vec![
        Value::fixnum(1),
        Value::fixnum(94),
        Value::fixnum(i64::from(b'Z')),
        Value::symbol("latin-iso8859-1"),
    ])
    .unwrap();
    builtin_define_charset_alias(vec![
        Value::symbol("latin-iso8859-1"),
        Value::symbol("ascii"),
    ])
    .unwrap();
    let decoded = decode(b"\x1b(ZA");
    let text = neovm_value::emacs_char::to_utf8_lossy(&decoded.bytes);
    assert_eq!(text, "\u{00C1}");
    assert_eq!(decoded.runs.len(), 1);
    assert_eq!(decoded.runs[0].plist, charset_prop("latin-iso8859-1"));
    assert!(
        builtin_iso_charset(vec![
            Value::fixnum(1),
            Value::fixnum(94),
            Value::fixnum(i64::from(b'Z')),
        ])
        .unwrap()
        .is_bare_symbol_named("latin-iso8859-1")
    );

    let encoded = encode_via_iso2022(
        &neovm_value::heap_types::LispString::from_utf8("\u{00C1}"),
        &spec,
        &[intern("iso-2022")],
        EncodingBoundary::CompleteText,
        &mut CodingEncoderState::default(),
    );
    assert_eq!(encoded, b"\x1b,AA\x1b(B");

    reset_charset_registry();
    let replayed = decode(b"\x1b(Z\\");
    assert_eq!(replayed.bytes, b"\x1b(Z\\");
    assert!(replayed.runs.is_empty());

    builtin_declare_equiv_charset(vec![
        Value::fixnum(1),
        Value::fixnum(94),
        Value::fixnum(i64::from(b'Z')),
        Value::symbol("unicode"),
    ])
    .unwrap();
    let no_final = decode(b"\x1b(ZA");
    assert_eq!(no_final.bytes, b"\x1b(ZA");
    assert!(no_final.runs.is_empty());
    assert!(
        builtin_iso_charset(vec![
            Value::fixnum(1),
            Value::fixnum(94),
            Value::fixnum(i64::from(b'Z')),
        ])
        .unwrap()
        .is_bare_symbol_named("unicode")
    );

    let mut args = vec![Value::NIL; 17];
    args[0] = Value::symbol("withfinal-probe");
    args[1] = Value::fixnum(1);
    args[2] = Value::vector(vec![Value::fixnum(33), Value::fixnum(126)]);
    args[5] = Value::fixnum(i64::from(b'9'));
    args[11] = Value::fixnum(400);
    builtin_define_charset_internal(args).unwrap();
    builtin_declare_equiv_charset(vec![
        Value::fixnum(1),
        Value::fixnum(94),
        Value::fixnum(i64::from(b'Z')),
        Value::symbol("withfinal-probe"),
    ])
    .unwrap();
    let with_final = decode(b"\x1b(ZA");
    let text = neovm_value::emacs_char::to_utf8_lossy(&with_final.bytes);
    assert_eq!(text, "\u{01B0}");
    assert_eq!(with_final.runs.len(), 1);
    assert_eq!(with_final.runs[0].plist, charset_prop("withfinal-probe"));
}

fn encode_string(s: &str, coding_system: &str) -> Vec<u8> {
    super::encode_string(s, coding_system, EolConversion::Enabled)
}

fn decode_bytes(bytes: &[u8], coding_system: &str) -> String {
    super::decode_bytes(bytes, coding_system, EolConversion::Enabled)
}

fn encode_lisp_string(s: &neovm_value::heap_types::LispString, coding_system: &str) -> Vec<u8> {
    super::encode_lisp_string(s, coding_system, EolConversion::Enabled)
}

/// Register a deterministic dim-2 ISO-2022 charset (`test-jis`, ISO final 'B',
/// OFFSET method) so `charset_decode_char` maps a 2-byte code to a non-ASCII
/// char and `charset_iso2022_designation` recognizes the `$B` designation.
fn register_test_jis() -> neovm_value::intern::SymId {
    use neovm_value::value::Value;
    let mut args = vec![Value::NIL; 17];
    args[0] = Value::symbol("test-jis");
    args[1] = Value::fixnum(2);
    args[2] = Value::vector(vec![
        Value::fixnum(33),
        Value::fixnum(126),
        Value::fixnum(33),
        Value::fixnum(126),
    ]);
    args[5] = Value::fixnum(66); // ISO final 'B'
    args[11] = Value::fixnum(0x10000); // code-offset -> decoded chars are non-ASCII
    crate::emacs_core::charset::builtin_define_charset_internal(args).unwrap();
    neovm_value::intern::intern("test-jis")
}

fn charset_prop(charset: &str) -> Value {
    Value::list(vec![
        Value::symbol("charset"),
        Value::from_sym_id(neovm_value::intern::intern(charset)),
    ])
}

fn emacs_char_codes(bytes: &[u8]) -> Vec<u32> {
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < bytes.len() {
        let (ch, n) = neovm_value::emacs_char::string_char(&bytes[pos..]);
        if n == 0 {
            break;
        }
        out.push(ch.code());
        pos += n;
    }
    out
}

struct CharsetReset;
impl Drop for CharsetReset {
    fn drop(&mut self) {
        crate::emacs_core::charset::reset_charset_registry();
    }
}

fn define_offset_charset(
    name: &'static str,
    dimension: i64,
    code_space: &[i64],
    final_char: u8,
    offset: i64,
    mark: Option<i64>,
) {
    use crate::emacs_core::charset::builtin_define_charset_internal;
    let mut elems = vec![
        Value::symbol(":name"),
        Value::symbol(name),
        Value::symbol(":code-offset"),
        Value::fixnum(offset),
    ];
    if let Some(mark) = mark {
        elems.push(Value::symbol(":mark"));
        elems.push(Value::fixnum(mark));
    }
    let mut args = vec![Value::NIL; 17];
    args[0] = Value::symbol(name);
    args[1] = Value::fixnum(dimension);
    args[2] = Value::vector(code_space.iter().copied().map(Value::fixnum).collect());
    args[5] = Value::fixnum(i64::from(final_char));
    args[11] = Value::fixnum(offset);
    args[16] = Value::list(elems);
    builtin_define_charset_internal(args).unwrap();
}

fn iso_charset_name(dimension: i64, chars: i64, final_char: u8) -> Value {
    crate::emacs_core::charset::builtin_iso_charset(vec![
        Value::fixnum(dimension),
        Value::fixnum(chars),
        Value::fixnum(i64::from(final_char)),
    ])
    .unwrap()
}

/// GNU `Fdefine_charset_internal` keeps the id and publishes the name used
/// for the redefinition (`charset.c:1114-1154`). Both finals then answer
/// that name from `iso-charset`.
#[test]
fn redefine_through_alias_publishes_the_defining_name() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{
        builtin_charset_id_internal, builtin_charset_plist, builtin_define_charset_alias,
        reset_charset_registry,
    };
    use neovm_value::plist::plist_get;

    let _reset = CharsetReset;
    reset_charset_registry();
    define_offset_charset("cr-base", 1, &[33, 126], b'9', 100, None);
    builtin_define_charset_alias(vec![Value::symbol("cr-probe"), Value::symbol("cr-base")])
        .unwrap();
    define_offset_charset("cr-probe", 1, &[33, 126], b'8', 200, None);
    assert!(iso_charset_name(1, 94, b'9').is_bare_symbol_named("cr-probe"));
    assert!(iso_charset_name(1, 94, b'8').is_bare_symbol_named("cr-probe"));
    let id = |name: &'static str| {
        builtin_charset_id_internal(vec![Value::symbol(name)])
            .unwrap()
            .as_int()
            .unwrap()
    };
    assert_eq!(id("cr-probe"), id("cr-base"));
    let plist = |name: &'static str| builtin_charset_plist(vec![Value::symbol(name)]).unwrap();
    assert!(
        plist_get(plist("cr-probe"), &Value::symbol(":name"))
            .unwrap()
            .is_bare_symbol_named("cr-probe")
    );
    assert!(
        plist_get(plist("cr-base"), &Value::symbol(":name"))
            .unwrap()
            .is_bare_symbol_named("cr-base")
    );
}

/// A 96-position code space is a 96-set for both registration and the
/// designation escape. `[33 128]` does not start at 0x20.
#[test]
fn iso2022_emits_96_set_from_code_space_width() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{charset_decode_char, reset_charset_registry};
    use crate::emacs_core::coding::{Iso2022Spec, IsoFlag};
    use neovm_value::intern::intern;

    let _reset = CharsetReset;
    reset_charset_registry();
    define_offset_charset("cr-wide", 1, &[33, 128], b'W', 0xE000, None);
    assert!(iso_charset_name(1, 96, b'W').is_bare_symbol_named("cr-wide"));
    assert!(iso_charset_name(1, 94, b'W').is_nil());
    let ch = charset_decode_char(intern("cr-wide"), 33).unwrap();
    let spec = Iso2022Spec {
        initial: [Some(intern("ascii")), None, None, None],
        request: vec![],
        reg_usage: (0, 0),
        flags: enumflags2::BitFlags::from_flag(IsoFlag::SevenBits)
            | IsoFlag::Designation
            | IsoFlag::AsciiAtEol,
    };
    let encoded = encode_via_iso2022(
        &neovm_value::heap_types::LispString::from_emacs_bytes(
            neovm_value::emacs_char::EmacsChar::from_code(ch as u32)
                .unwrap()
                .to_emacs_bytes(),
        ),
        &spec,
        &[intern("ascii"), intern("cr-wide")],
        EncodingBoundary::CompleteText,
        &mut CodingEncoderState::default(),
    );
    assert_eq!(encoded, b"\x1b,W!\x1b(B");
}

/// `iso-2022-jp` does not list `latin-iso8859-1`, so `ESC ( Z` is replayed.
/// A full-support list still decodes it.
#[test]
fn iso2022_rejects_a_charset_the_coding_system_does_not_list() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{builtin_declare_equiv_charset, reset_charset_registry};
    use crate::emacs_core::coding::{Iso2022Spec, IsoFlag};
    use neovm_value::intern::intern;

    let _reset = CharsetReset;
    reset_charset_registry();
    builtin_declare_equiv_charset(vec![
        Value::fixnum(1),
        Value::fixnum(94),
        Value::fixnum(i64::from(b'Z')),
        Value::symbol("latin-iso8859-1"),
    ])
    .unwrap();
    let spec = Iso2022Spec {
        initial: [Some(intern("ascii")), None, None, None],
        request: vec![],
        reg_usage: (0, 0),
        flags: enumflags2::BitFlags::from_flag(IsoFlag::SevenBits) | IsoFlag::Designation,
    };
    let decode = |list: &[neovm_value::intern::SymId]| {
        decode_via_iso2022(
            b"\x1b(ZA",
            &spec,
            list,
            &mut Iso2022DecodeState::default(),
            DosEolLookahead::NotRequired,
        )
    };
    let rejected = decode(&[intern("ascii")]);
    assert_eq!(rejected.bytes, b"\x1b(ZA");
    let accepted = decode(&[intern("iso-2022")]);
    assert_eq!(
        neovm_value::emacs_char::to_utf8_lossy(&accepted.bytes),
        "\u{00C1}"
    );
}

/// Dimension 3 reads three positions. GNU's formula validates the third
/// byte and then assembles `c2` twice (coding.c:3911).
#[test]
fn iso2022_reads_three_positions_of_a_dimension_three_charset() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{builtin_declare_equiv_charset, reset_charset_registry};
    use crate::emacs_core::coding::{Iso2022Spec, IsoFlag};
    use neovm_value::intern::intern;

    let _reset = CharsetReset;
    reset_charset_registry();
    define_offset_charset(
        "cr-3d",
        3,
        &[33, 126, 33, 126, 33, 126],
        b'Y',
        0x300000,
        None,
    );
    builtin_declare_equiv_charset(vec![
        Value::fixnum(1),
        Value::fixnum(94),
        Value::fixnum(i64::from(b'Y')),
        Value::symbol("cr-3d"),
    ])
    .unwrap();
    let spec = Iso2022Spec {
        initial: [Some(intern("ascii")), None, None, None],
        request: vec![],
        reg_usage: (0, 0),
        flags: enumflags2::BitFlags::from_flag(IsoFlag::SevenBits) | IsoFlag::Designation,
    };
    let decode = |bytes: &[u8]| {
        emacs_char_codes(
            &decode_via_iso2022(
                bytes,
                &spec,
                &[intern("iso-2022")],
                &mut Iso2022DecodeState::default(),
                DosEolLookahead::NotRequired,
            )
            .bytes,
        )
    };
    assert_eq!(decode(b"\x1b(Y!!!"), vec![0x300000]);
    assert_eq!(decode(b"\x1b(Y!!A"), vec![0x300000]);
}

/// A control byte is not a continuation. The first graphic byte is replayed
/// and the following escape is decoded on its own.
#[test]
fn iso2022_rejects_a_control_continuation_before_assembly() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{builtin_declare_equiv_charset, reset_charset_registry};
    use crate::emacs_core::coding::{Iso2022Spec, IsoFlag};
    use neovm_value::intern::intern;

    let _reset = CharsetReset;
    reset_charset_registry();
    define_offset_charset("cr-open", 2, &[0, 255, 0, 255], b'X', 0, None);
    builtin_declare_equiv_charset(vec![
        Value::fixnum(1),
        Value::fixnum(94),
        Value::fixnum(i64::from(b'X')),
        Value::symbol("cr-open"),
    ])
    .unwrap();
    let spec = Iso2022Spec {
        initial: [Some(intern("ascii")), None, None, None],
        request: vec![],
        reg_usage: (0, 0),
        flags: enumflags2::BitFlags::from_flag(IsoFlag::SevenBits) | IsoFlag::Designation,
    };
    let decode = |bytes: &[u8]| {
        emacs_char_codes(
            &decode_via_iso2022(
                bytes,
                &spec,
                &[intern("ascii"), intern("cr-open")],
                &mut Iso2022DecodeState::default(),
                DosEolLookahead::NotRequired,
            )
            .bytes,
        )
    };
    assert_eq!(decode(b"\x1b(XA\x1b(B"), vec![b'A' as u32]);
    assert_eq!(decode(b"\x1b(X!!"), vec![0x2121]);
}

fn plist_mark(name: &'static str) -> Option<i64> {
    use crate::emacs_core::charset::builtin_charset_plist;
    use neovm_value::plist::plist_get;
    plist_get(
        builtin_charset_plist(vec![Value::symbol(name)]).unwrap(),
        &Value::symbol(":mark"),
    )
    .and_then(|value| value.as_int())
}

/// Alias plists survive a snapshot: a physical name that captured another
/// charset's vector, an in-place `setcar`, and a shared slot that must not
/// be split or joined by comparing pair contents.
#[test]
fn charset_snapshot_keeps_captured_plist_slots() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{
        builtin_charset_plist, builtin_define_charset_alias, reset_charset_registry,
        restore_charset_registry, set_charset_plist_value, snapshot_charset_registry,
    };
    use neovm_value::value::eq_value;

    let _reset = CharsetReset;
    reset_charset_registry();

    define_offset_charset("cr-phys", 1, &[33, 126], b'7', 10, Some(9));
    define_offset_charset("cr-target", 1, &[33, 126], b'6', 20, Some(1));
    builtin_define_charset_alias(vec![Value::symbol("cr-phys"), Value::symbol("cr-target")])
        .unwrap();
    define_offset_charset("cr-target", 1, &[33, 126], b'5', 30, Some(2));
    assert_eq!(plist_mark("cr-phys"), Some(1));
    assert_eq!(plist_mark("cr-target"), Some(2));

    define_offset_charset("cr-live", 1, &[33, 126], b'4', 40, Some(1));
    let live = builtin_charset_plist(vec![Value::symbol("cr-live")]).unwrap();
    // (:name NAME :code-offset N :mark 1 ...) — the mark's value cell.
    let mut cell = live;
    for _ in 0..5 {
        cell = cell.cons_cdr();
    }
    cell.set_car(Value::fixnum(2));
    assert_eq!(plist_mark("cr-live"), Some(2));

    define_offset_charset("cr-shared", 1, &[33, 126], b'3', 50, Some(1));
    builtin_define_charset_alias(vec![
        Value::symbol("cr-share-p"),
        Value::symbol("cr-shared"),
    ])
    .unwrap();
    builtin_define_charset_alias(vec![
        Value::symbol("cr-share-q"),
        Value::symbol("cr-shared"),
    ])
    .unwrap();
    define_offset_charset("cr-shared", 1, &[33, 126], b'2', 60, Some(8));
    assert_eq!(plist_mark("cr-share-p"), Some(1));
    assert_eq!(plist_mark("cr-share-q"), Some(1));
    let shared = builtin_charset_plist(vec![Value::symbol("cr-share-p")]).unwrap();
    let shared_q = builtin_charset_plist(vec![Value::symbol("cr-share-q")]).unwrap();
    assert!(eq_value(&shared, &shared_q));

    define_offset_charset("cr-equal", 1, &[33, 126], b'1', 70, Some(4));
    builtin_define_charset_alias(vec![Value::symbol("cr-equal-p"), Value::symbol("cr-equal")])
        .unwrap();
    define_offset_charset("cr-equal", 1, &[33, 126], b'0', 80, Some(4));
    let before = builtin_charset_plist(vec![Value::symbol("cr-equal-p")]).unwrap();
    let after = builtin_charset_plist(vec![Value::symbol("cr-equal")]).unwrap();
    assert!(!eq_value(&before, &after));

    let snapshot = snapshot_charset_registry();
    reset_charset_registry();
    restore_charset_registry(snapshot);

    assert_eq!(plist_mark("cr-phys"), Some(1));
    assert_eq!(plist_mark("cr-target"), Some(2));
    assert_eq!(plist_mark("cr-live"), Some(2));
    assert_eq!(plist_mark("cr-share-p"), Some(1));
    assert_eq!(plist_mark("cr-share-q"), Some(1));
    assert!(eq_value(
        &builtin_charset_plist(vec![Value::symbol("cr-share-p")]).unwrap(),
        &builtin_charset_plist(vec![Value::symbol("cr-share-q")]).unwrap(),
    ));
    assert!(!eq_value(
        &builtin_charset_plist(vec![Value::symbol("cr-equal-p")]).unwrap(),
        &builtin_charset_plist(vec![Value::symbol("cr-equal")]).unwrap(),
    ));

    let updated = Value::list(vec![
        Value::symbol(":name"),
        Value::symbol("cr-shared"),
        Value::symbol(":mark"),
        Value::fixnum(3),
    ]);
    let pairs = neovm_value::value::list_to_vec(&updated).unwrap();
    let stored: Vec<_> = pairs
        .chunks(2)
        .map(|pair| {
            (
                pair[0]
                    .as_symbol_id(neovm_value::tagged::value::SymbolMode::Bare)
                    .unwrap(),
                pair[1],
            )
        })
        .collect();
    set_charset_plist_value(neovm_value::intern::intern("cr-share-p"), stored, updated);
    assert_eq!(plist_mark("cr-share-q"), Some(3));
    assert_eq!(plist_mark("cr-shared"), Some(8));
}

/// A plist slot nobody still names is not a GC root. A slot an alias still
/// holds is.
#[test]
fn abandoned_charset_plist_slot_is_not_a_gc_root() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::charset::{
        builtin_define_charset_alias, collect_charset_registry_gc_roots,
        current_charset_registry_handle, reset_charset_registry,
    };

    let _reset = CharsetReset;
    reset_charset_registry();
    const ABANDONED: i64 = 0x504C_1501;
    const KEPT: i64 = 0x504C_1502;
    define_offset_charset("cr-gc", 1, &[33, 126], b'P', 1, Some(ABANDONED));
    define_offset_charset("cr-gc", 1, &[33, 126], b'Q', 2, Some(2));
    let handle = current_charset_registry_handle();
    let mut roots = Vec::new();
    collect_charset_registry_gc_roots(&handle, &mut roots);
    assert!(roots.iter().all(|value| value.as_int() != Some(ABANDONED)));

    define_offset_charset("cr-gc-keep", 1, &[33, 126], b'R', 3, Some(KEPT));
    builtin_define_charset_alias(vec![
        Value::symbol("cr-gc-alias"),
        Value::symbol("cr-gc-keep"),
    ])
    .unwrap();
    define_offset_charset("cr-gc-keep", 1, &[33, 126], b'S', 4, Some(5));
    roots.clear();
    collect_charset_registry_gc_roots(&handle, &mut roots);
    assert!(roots.iter().any(|value| value.as_int() == Some(KEPT)));
}

#[cfg(test)]
#[path = "gdh_utf8_unibyte.rs"]
mod gdh_utf8_unibyte;
