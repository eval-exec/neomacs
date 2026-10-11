//! GNU `string-as-multibyte` rejects `C0`/`C1` raw-byte sequences.
//!
//! Expectations are the character codes GNU Emacs 31.1 produced for these
//! bytes (`string-as-multibyte` of a unibyte string). `C0 80` is the two
//! eight-bit characters 4194240 and 4194176, four bytes, not one raw byte.

use super::super::{
    MAX_MULTIBYTE_LENGTH, byte8_to_char, char_string, multibyte_length, parse_str_as_multibyte,
    str_as_multibyte, str_as_multibyte_span, str_to_multibyte,
};
use crate::emacs_core::builtins::lisp_string_char_codes;
use crate::emacs_core::format_eval_result;
use crate::emacs_core::misc::builtin_string_as_multibyte;
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

fn promoted(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() * 2);
    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
    let mut i = 0;
    while i < src.len() {
        let (input_bytes, output_bytes) = str_as_multibyte_span(&src[i..]);
        assert!(input_bytes > 0);
        if output_bytes == input_bytes {
            out.extend_from_slice(&src[i..i + input_bytes]);
        } else {
            let n = char_string(byte8_to_char(src[i]), &mut buf);
            assert_eq!(n, output_bytes);
            out.extend_from_slice(&buf[..n]);
        }
        i += input_bytes;
    }
    out
}

#[test]
fn c0_c1_pairs_split_into_eight_bit_characters() {
    // GNU Emacs 31.1: (string-as-multibyte (unibyte-string 192 128))
    // -> chars (4194240 4194176), 4 bytes.
    // (string-as-multibyte (unibyte-string 193 191))
    // -> chars (4194241 4194239), 4 bytes.
    let cases: &[(&[u8], &[u32])] = &[
        (&[0xC0, 0x80], &[4194240, 4194176]),
        (&[0xC1, 0xBF], &[4194241, 4194239]),
        (&[0xC0, 0xBF], &[4194240, 4194239]),
        (&[0xC1, 0x80], &[4194241, 4194176]),
        (&[b'A', 0xC0, 0x80, b'B'], &[65, 4194240, 4194176, 66]),
    ];
    for (src, chars) in cases {
        let out = str_as_multibyte(src);
        assert_eq!(out, promoted(src), "bytes {src:02X?}");
        assert_ne!(
            multibyte_length(src, false),
            Some(src.len()),
            "the whole C0/C1 buffer is not one character"
        );
        let metrics = parse_str_as_multibyte(src);
        assert_eq!(metrics.chars, chars.len(), "chars {src:02X?}");
        assert_eq!(metrics.nbytes, out.len(), "nbytes {src:02X?}");
        let value = Value::heap_string(LispString::from_unibyte(src.to_vec()));
        let converted = builtin_string_as_multibyte(vec![value]).expect("string-as-multibyte");
        let ls = converted.as_lisp_string().expect("string");
        assert!(ls.is_multibyte());
        assert_eq!(ls.sbytes(), out.len());
        assert_eq!(lisp_string_char_codes(ls), *chars);
    }
}

#[test]
fn valid_non_eight_bit_sequences_are_preserved() {
    // C2 80 is U+0080. The allow_8bit=false window still accepts w == 0x2C2.
    // C3 A9 is U+00E9. E0 A0 80 is U+0800. F0 90 80 80 is U+10000.
    let cases: &[(&[u8], &[u32])] = &[
        (&[0xC2, 0x80], &[0x80]),
        (&[0xC3, 0xA9], &[0xE9]),
        (&[0xE0, 0xA0, 0x80], &[0x800]),
        (&[0xF0, 0x90, 0x80, 0x80], &[0x10000]),
        (b"ABC", &[65, 66, 67]),
    ];
    for (src, chars) in cases {
        let out = str_as_multibyte(src);
        assert_eq!(out, *src, "preserved {src:02X?}");
        let metrics = parse_str_as_multibyte(src);
        assert_eq!(metrics.chars, chars.len());
        assert_eq!(metrics.nbytes, src.len());
        let value = Value::heap_string(LispString::from_unibyte(src.to_vec()));
        let converted = builtin_string_as_multibyte(vec![value]).expect("string-as-multibyte");
        assert_eq!(
            lisp_string_char_codes(converted.as_lisp_string().expect("string")),
            *chars
        );
    }
}

#[test]
fn lone_high_byte_and_c0_before_ascii_still_promote_one_byte() {
    let lone = str_as_multibyte(&[b'A', 0xFF, b'B']);
    assert_eq!(parse_str_as_multibyte(&[b'A', 0xFF, b'B']).chars, 3);
    assert_eq!(lone.len(), 1 + 2 + 1);

    // C0 followed by a non-continuation is not a character, and the
    // following ASCII byte stays one byte.
    let split = str_as_multibyte(&[0xC0, 0x00]);
    assert_eq!(parse_str_as_multibyte(&[0xC0, 0x00]).chars, 2);
    assert_eq!(parse_str_as_multibyte(&[0xC0, 0x00]).nbytes, 3);
    assert_eq!(split.len(), 3);

    // string-to-multibyte promotes every high byte, including bytes that
    // form a valid UTF-8 sequence. That is a different function.
    let as_multi = str_as_multibyte(&[0xC3, 0xA9]);
    let to_multi = str_to_multibyte(&[0xC3, 0xA9]);
    assert_eq!(as_multi, &[0xC3, 0xA9]);
    assert_ne!(to_multi, as_multi);
    assert_eq!(to_multi.len(), 4);
}

#[test]
fn compiled_and_interpreted_callers_see_the_split() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let rendered = format_eval_result(&eval.eval_str(
        r#"(let ((f (lambda (s) (string-as-multibyte s)))
                 (s (unibyte-string 192 128))
                 (i 0)
                 (r nil))
             (while (< i 5)
               (setq r (funcall f s))
               (setq i (1+ i)))
             (list (multibyte-string-p r) (length r) (string-bytes r) (append r nil)))"#,
    ));
    assert_eq!(rendered, "OK (t 2 4 (4194240 4194176))");
}
