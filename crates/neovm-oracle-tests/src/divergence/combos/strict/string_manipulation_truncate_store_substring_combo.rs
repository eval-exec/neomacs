//! Strict combo oracle probes, batch 295: string manipulation deep.
//! truncate-string-to-width, store-substring, substring-no-properties,
//! format-propertize, split-string-and-unquote, and concat-with-separators.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_truncate_store_substring_format_propertize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (store-substring (copy-sequence "abcdef") 2 ?X)
      (substring-no-properties (propertize "abc" 'face 'bold) 0 2)
      (truncate-string-to-width "hello world" 5 0 nil t)
      (truncate-string-to-width "hello world" 5 0 "..." t)
      (truncate-string-to-width "abcdef" 10 0 nil nil)
      (format-propertize "hi" 'face 'bold)
      (format "%c" 65)
      (mapconcat #'identity '("a" "b" "c") "-"))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-propertize)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_split_string_unquote_combine_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (split-string-and-unquote "a, b, c")
      (split-string-and-unquote "single")
      (combine-and-quote-strings '("a" "b c" "d\"e"))
      (combine-and-quote-strings '("simple" "with space"))
      (split-string "a,b,,c" "," t)
      (mapconcat #'char-to-string "abc" "-"))
"##;
    let expect = expect_test::expect![[
        r#""OK ((\"a,\" \"b,\" \"c\") (\"single\") \"a \\\"b c\\\" \\\"d\\\\\\\"e\\\"\" \"simple \\\"with space\\\"\" (\"a\" \"b\" \"c\") \"a-b-c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_string_multibyte_unibyte_byte_funcs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-to-multibyte "abc")
      (string-make-unibyte "abc")
      (multibyte-string-p "日本")
      (multibyte-string-p (unibyte-string 200))
      (string-bytes "abc")
      (string-bytes "日本")
      (string-make-multibyte "abc")
      (byte-to-string 65)
      (char-to-string ?日)
      (string-as-unibyte "abc")
      (string-as-multibyte "abc"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"abc\" \"abc\" t nil 3 6 \"abc\" \"A\" \"日\" \"abc\" \"abc\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
