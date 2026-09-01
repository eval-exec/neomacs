//! Divergence tests: string manipulation, substring, concat, case conversion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_substring_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello\" \"World!\" \"World!\" \"World\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "Hello, World!"))
  (list (substring s 0 5)
        (substring s 7)
        (substring s -6)
        (substring s -6 -1)))"#,
        expect,
    );
}

#[test]
fn divergence_substring_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((s (propertize "abcdef" 'face 'bold))
         (sub (substring s 2 4)))
  (list (get-text-property 0 'face sub)
        (get-text-property 0 'face s)
        (length sub)))"#,
        expect,
    );
}

#[test]
fn divergence_string_bytes_vs_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 13 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "Héllo 世界"))
  (list (length s)
        (string-bytes s)
        (string-equal s "Héllo 世界")
        (string< "abc" "abd")
        (string> "abd" "abc")))"#,
        expect,
    );
}

#[test]
fn divergence_case_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"HELLO WORLD\" \"hello world\" \"Hello World Foo\" \"Hello World Foo\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (upcase "Hello World")
  (downcase "Hello World")
  (capitalize "hello world foo")
  (upcase-initials "hello world foo"))"#,
        expect,
    );
}

#[test]
fn divergence_case_conversion_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"STRASSE\" \"i\u{307}stanbul\" \"Foo-Bar Baz\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (upcase "Straße")
              (downcase "İSTANBUL")
              (capitalize "foo-bar baz"))"#,
        expect,
    );
}

#[test]
fn divergence_string_multibyte_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"abc中文def\" 8 12 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s (concat "abc" "中文" "def")))
  (list s
        (length s)
        (string-bytes s)
        (multibyte-string-p s)
        (string= s "abc中文def")))"#,
        expect,
    );
}

#[test]
fn divergence_string_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable 0xc0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s (string ?a ?b 0xc0 ?d)))
  (list s
        (length s)
        (multibyte-string-p s)
        (string-to-multibyte s)
        (multibyte-string-p (string-to-multibyte s))))"#,
        expect,
    );
}

#[test]
fn divergence_string_make_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((ms "Héllo")
         (us (string-make-unibyte ms)))
  (list (multibyte-string-p ms)
        (multibyte-string-p us)
        (length us)))"#,
        expect,
    );
}

#[test]
fn divergence_string_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello Emacs\" \"f00 b00 m00\" \"a#b#c#\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(list\n  (string-replace \"world\" \"Emacs\" \"hello world\")\n  (string-replace \"o\" \"0\" \"foo boo moo\")\n  (replace-regexp-in-string \"[0-9]\" \"#\" \"a1b2c3\"))",
        expect,
    );
}

#[test]
fn divergence_split_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"foo\" \"bar\" \"baz\") (\"a\" \"b\" \"\" \"c\") (\"foo\" \"bar\" \"baz\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (split-string "  foo bar  baz  " " +" t)
  (split-string "a,b,,c" ",")
  (split-string "foo-bar-baz" "-"))"#,
        expect,
    );
}

#[test]
fn divergence_string_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hi        \" \"hi--------\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-pad "hi" 10)
  (string-pad "hi" 10 ?-)
  (string-pad "hello" 3))"#,
        expect,
    );
}
