//! Strict combo oracle probes, batch 356: string-trim-left/right/both
//! variants with custom regex patterns. subr-x string-trim family.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_string_trim_default_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"
(require 'subr-x)
(list (string-trim "  hello  ")
      (string-trim-left "  hello")
      (string-trim-right "hello  ")
      (string-trim "\n\thello\n\t")
      (string-trim "")
      (string-trim "no-trim-needed"))
"####;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello\" \"hello\" \"hello\" \"\" \"no-trim-needed\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_string_trim_custom_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"
(require 'subr-x)
(list (string-trim-left "...hello" "[.]+")
      (string-trim-right "hello!!!" "[!]+")
      (string-trim "###hello###" "#+")
      (string-trim-left "000123" "0+")
      (string-trim-right "abc123" "[0-9]+")
      (string-trim "  x  " nil))
"####;
    let expect =
        expect_test::expect![[r#""OK (\"hello\" \"hello\" \"hello###\" \"123\" \"abc\" \"x\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_string_pad_chop_truncate_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"
(require 'subr-x)
(list (string-pad "hi" 5)
      (string-pad "hi" 5 ?* t)
      (string-pad "hello" 3)
      (string-chop-newline "hello\n")
      (string-chop-newline "hello")
      (length (string-pad "x" 10 ?-)))
"####;
    let expect =
        expect_test::expect![[r#""OK (\"hi   \" \"***hi\" \"hello\" \"hello\" \"hello\" 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
