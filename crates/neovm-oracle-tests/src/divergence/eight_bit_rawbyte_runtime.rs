//! Eight-bit / raw-byte char handling parity: char-charset of raw bytes
//! (eight-bit), string-to-multibyte/unibyte roundtrips, byte<->char
//! conversion, raw bytes in a buffer, max-char, decode keeping eight-bit,
//! decode-char 'eight-bit.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn byte_char_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (200 65 65 \"�\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (multibyte-char-to-unibyte (unibyte-char-to-multibyte 200))
        (unibyte-char-to-multibyte 65) (multibyte-char-to-unibyte ?A)
        (byte-to-string 200) (length (byte-to-string 200)))"##,
        expect,
    );
}

#[test]
fn char_to_byte_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (-1 unicode-bmp ascii unicode-bmp 65 4194248)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (multibyte-char-to-unibyte ?λ)
        (char-charset ?λ) (char-charset ?A) (char-charset ?あ)
        (encode-char ?A 'unicode) (decode-char 'eight-bit 200))"##,
        expect,
    );
}

#[test]
fn decode_eightbit_keep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t (ascii eight-bit ascii))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (decode-coding-string (unibyte-string 65 200 66) 'utf-8)))
  (list (length d) (multibyte-string-p d)
        (mapcar (lambda (c) (char-charset c)) (string-to-list d))))"##,
        expect,
    );
}

#[test]
fn eight_bit_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (eight-bit unicode-bmp unicode-bmp unicode-bmp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-charset (unibyte-char-to-multibyte 200))
        (char-charset 200) (char-charset 128) (char-charset 255)
        (charsetp 'eight-bit))"##,
        expect,
    );
}

#[test]
fn eight_bit_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 4194248 eight-bit 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert (unibyte-char-to-multibyte 200))
  (insert (unibyte-char-to-multibyte 255))
  (list (buffer-size) (char-after 1) (char-charset (char-after 1))
        (string-bytes (buffer-string))))"##,
        expect,
    );
}

#[test]
fn max_char_min_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4194303 1114111 t eight-bit unicode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (max-char) (max-char t) (characterp (max-char))
        (char-charset (max-char)) (char-charset #x10FFFF))"##,
        expect,
    );
}

#[test]
fn raw_byte_to_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t 3 (4194248 4194249 4194250) (eight-bit eight-bit eight-bit))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (string-to-multibyte (unibyte-string 200 201 202))))
  (list (multibyte-string-p s) (length s) (mapcar #'identity s)
        (mapcar (lambda (c) (char-charset c)) s)))"##,
        expect,
    );
}

#[test]
fn string_to_unibyte_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 (130 240) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((m (string-to-multibyte (unibyte-string 130 240)))
        (u (string-to-unibyte m)))
  (list (length u) (append u nil) (string= u (unibyte-string 130 240))))"##,
        expect,
    );
}
