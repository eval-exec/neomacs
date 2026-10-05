//! GNU Emacs 31.1: `encode-coding-string` / `encode-coding-region` of a
//! unibyte source through `utf-8`, `utf-8-emacs`, `emacs-internal`, or an
//! EOL variant consumes `C0`/`C1` pairs.
//!
//! `consume_chars` reads a unibyte source whose encoder is not raw-text or
//! CCL with `multibyte_length(src, src_end, true, true)` (src/coding.c:7666).
//! `encode_coding_utf_8` then writes a `CHAR_BYTE8_P` character as one raw
//! byte (src/coding.c:1456). `prefer-utf-8` and `undecided` use
//! `encode_coding_raw_text` and keep the bytes.
//!
//! Expectations measured under GNU Emacs 31.1 (`tmp/gdh-f123-gnu.out`).

use super::encode_lisp_string;

fn unibyte(bytes: &[u8]) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_unibyte(bytes.to_vec())
}

#[test]
fn utf8_family_consumes_unibyte_c0_c1_pairs_like_gnu() {
    crate::test_utils::init_test_tracing();
    let pair = unibyte(&[0xC0, 0x80]);
    let c1 = unibyte(&[0xC1, 0xBF]);
    let mixed = unibyte(&[0xC0, 0x80, 0xC3, 0xA9]);
    let around = unibyte(&[0x41, 0xC0, 0x80, 0x42]);

    for coding in ["utf-8", "utf-8-emacs", "emacs-internal", "utf-8-unix"] {
        assert_eq!(encode_lisp_string(&pair, coding), vec![128], "{coding}");
        assert_eq!(encode_lisp_string(&c1, coding), vec![255], "{coding}");
        assert_eq!(
            encode_lisp_string(&mixed, coding),
            vec![128, 195, 169],
            "{coding}"
        );
        assert_eq!(
            encode_lisp_string(&around, coding),
            vec![65, 128, 66],
            "{coding}"
        );
    }

    // A lone high byte is not a sequence, so it round-trips as itself.
    assert_eq!(encode_lisp_string(&unibyte(&[0xE9]), "utf-8"), vec![0xE9]);
    // Valid UTF-8 inside the unibyte string is preserved.
    assert_eq!(
        encode_lisp_string(&unibyte(&[0xC3, 0xA9]), "utf-8"),
        vec![0xC3, 0xA9]
    );
    // Signature systems collapse the pair and prepend one BOM.
    assert_eq!(
        encode_lisp_string(&pair, "utf-8-with-signature"),
        vec![0xEF, 0xBB, 0xBF, 128]
    );

    // Raw-text encoders do not consume the pair (coding.c:7667).
    for coding in ["prefer-utf-8", "undecided", "raw-text", "no-conversion"] {
        assert_eq!(
            encode_lisp_string(&pair, coding),
            vec![0xC0, 0x80],
            "{coding}"
        );
    }
}

#[test]
fn utf8_family_still_encodes_multibyte_eight_bit_as_one_byte() {
    crate::test_utils::init_test_tracing();
    let mut buf = [0u8; 5];
    let n = crate::emacs_core::emacs_char::char_string(0x3FFF80, &mut buf);
    let eight = crate::heap_types::LispString::from_emacs_bytes(buf[..n].to_vec());
    assert_eq!(encode_lisp_string(&eight, "utf-8"), vec![0x80]);
    let n = crate::emacs_core::emacs_char::char_string(0xE9, &mut buf);
    let eacute = crate::heap_types::LispString::from_emacs_bytes(buf[..n].to_vec());
    assert_eq!(encode_lisp_string(&eacute, "utf-8"), vec![0xC3, 0xA9]);
}
