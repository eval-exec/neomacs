//! Charset property parity: charsetp across ascii/unicode/CJK/cp/koi8/eight-bit,
//! charset-dimension/chars/id-internal/plist/description/long-short-name,
//! decode-char/encode-char per charset, map-charset-chars, charset-after,
//! find-charset-string; plus default text-quoting-style (format-message /
//! substitute-command-keys curve quotes) and require error-message text.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn charset_after_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (ascii unicode-bmp unicode-bmp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "Aé日")
  (list (charset-after 1) (charset-after 2) (charset-after 3)))"##,
        expect,
    );
}

#[test]
fn charset_descriptions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument charsetp 'ascii)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (stringp (charset-description 'ascii))
        (charset-long-name 'ascii) (charset-short-name 'ascii))"##,
        expect,
    );
}

#[test]
fn charset_dimension_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 128 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (charset-dimension 'ascii) (charset-dimension 'japanese-jisx0208)
        (charset-chars 'ascii) (charset-dimension 'latin-iso8859-1))"##,
        expect,
    );
}

#[test]
fn charset_id_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t ascii ascii)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (integerp (charset-id-internal 'ascii))
        (plist-get (charset-plist 'ascii) :name)
        (get-charset-property 'ascii :name))"##,
        expect,
    );
}

#[test]
fn charsetp_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(mapcar #'charsetp '(ascii unicode latin-iso8859-1 big5 chinese-gb2312
        japanese-jisx0208 korean-ksc5601 windows-1252 cp932 koi8-r eight-bit))"##,
        expect,
    );
}

#[test]
fn decode_char_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (65 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (decode-char 'ascii 65)
        (integerp (decode-char 'japanese-jisx0208 (logior (ash 36 8) 36)))
        (decode-char 'latin-iso8859-1 233))"##,
        expect,
    );
}

#[test]
fn encode_char_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (65 65 105)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (encode-char ?A 'ascii) (encode-char ?A 'unicode)
        (encode-char ?é 'latin-iso8859-1))"##,
        expect,
    );
}

#[test]
fn map_charset_chars_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((n 0))
  (map-charset-chars (lambda (_range _arg) (setq n (1+ n))) 'ascii)
  (> n 0))"##,
        expect,
    );
}

#[test]
fn string_charsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"ascii\" \"unicode-bmp\") t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (sort (mapcar #'symbol-name (find-charset-string "Aé")) #'string<)
        (charsetp 'iso-8859-1) (charsetp 'nonexistent-charset-xyz))"##,
        expect,
    );
}

#[test]
fn require_error_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"Cannot open load file: No such file or directory, neo-no-such-feature-xyz\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (require 'neo-no-such-feature-xyz)
  (error (error-message-string e)))"##,
        expect,
    );
}

#[test]
fn text_quoting_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (nil \"use ‘foo’ here\" \"type \\\\‘C-c\\\\’ now\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list text-quoting-style
      (format-message "use `foo' here")
      (substitute-command-keys "type \\`C-c\\' now"))"##,
        expect,
    );
}
