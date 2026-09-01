//! Regex (case-fold incl. multibyte, char classes, groups/backrefs, word
//! boundaries, replace-with-fn, split) and charset/char (char-charset,
//! charset-plist :code-space, char/string-width, decode/encode-char) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn regex_anchors_greedy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 4 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (string-match "a.*c" "axxcxxc")
  (list (match-end 0)
        (progn (string-match "a.*?c" "axxcxxc") (match-end 0))
        (string-match "^$" "")))"##,
        expect,
    );
}

#[test]
fn regex_case_fold_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 4 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (string-match "hello" "say HELLO world")
        (string-match "WORLD" "the world")
        (let ((case-fold-search nil)) (string-match "HELLO" "hello"))))"##,
        expect,
    );
}

#[test]
fn regex_case_fold_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (string-match "café" "the CAFÉ here")
        (string-match "ÀÉÎ" "àéî test")
        (string-match "ΑΒΓ" "αβγ greek")))"##,
        expect,
    );
}

#[test]
fn regex_char_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 3 1 \"_____\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-match "[[:digit:]]+" "abc123")
        (string-match "[[:alpha:]]+" "123abc")
        (string-match "[[:space:]]" "a b")
        (replace-regexp-in-string "[[:upper:]]" "_" "aBcDe"))"##,
        expect,
    );
}

#[test]
fn regex_groups_backrefs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"aaa\" \"bbb\" \"[a][b][c]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (string-match "\\(a+\\)\\(b+\\)" "xaaabbby")
  (list (match-string 1 "xaaabbby") (match-string 2 "xaaabbby")
        (replace-regexp-in-string "\\(.\\)\\1" "[\\1]" "aabbcc")))"##,
        expect,
    );
}

#[test]
fn regex_replace_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"a2b44c666\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(replace-regexp-in-string "[0-9]+"
  (lambda (m) (number-to-string (* 2 (string-to-number m)))) "a1b22c333")"##,
        expect,
    );
}

#[test]
fn regex_split_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"\" \"c\") (\"a\" \"b\") (\"1\" \"2\" \"3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (split-string "a,b,,c" ",")
        (split-string "  a  b  " )
        (split-string "1-2-3" "-" t))"##,
        expect,
    );
}

#[test]
fn regex_word_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 0 \"X X X\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-match "\\bword\\b" "a word here")
        (string-match "\\<the\\>" "the theory")
        (replace-regexp-in-string "\\w+" "X" "ab cd ef"))"##,
        expect,
    );
}

#[test]
fn category_syntax_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (119 40 32 119 \"\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-syntax ?a) (char-syntax ?\() (char-syntax ?\ )
        (char-syntax ?0) (string (char-syntax ?\")))"##,
        expect,
    );
}

#[test]
fn char_charset_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (ascii unicode-bmp unicode-bmp eight-bit unicode-bmp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-charset ?A) (char-charset ?あ) (char-charset ?€)
        (char-charset (max-char)) (char-charset 128))"##,
        expect,
    );
}

#[test]
fn char_equal_casefold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (char-equal ?a ?A) (char-equal ?z ?Z)
        (let ((case-fold-search nil)) (char-equal ?a ?A))))"##,
        expect,
    );
}

#[test]
fn char_width_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 8 5 4 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-width ?A) (char-width ?あ) (char-width ?\t)
        (string-width "héllo") (string-width "日本") (string-width "ab\tcd"))"##,
        expect,
    );
}

#[test]
fn charset_plist_codespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([0 127 0 0 0 0 0 0] t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (plist-get (charset-plist 'ascii) :code-space)
        (charsetp 'unicode) (charsetp 'ascii) (charsetp 'latin-iso8859-1))"##,
        expect,
    );
}

#[test]
fn decode_encode_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (65 65 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (decode-char 'ascii 65)
        (encode-char ?A 'ascii)
        (decode-char 'latin-iso8859-1 233))"##,
        expect,
    );
}

#[test]
fn multibyte_char_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (65 \"λ\" 955 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (multibyte-char-to-unibyte ?A)
        (char-to-string ?λ) (string-to-char "λ")
        (= (string-to-char "λ") ?λ))"##,
        expect,
    );
}
