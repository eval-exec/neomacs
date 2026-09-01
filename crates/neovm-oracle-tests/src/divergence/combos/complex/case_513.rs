/// Batch 513: string-lessp vs string-collate-lessp deeper, string version ordering.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx513_string_lessp_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-lessp "a" "b") (string-lessp "b" "a") (string-lessp "a" "a"))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_lessp_mixed_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-lessp "A" "a") (string-lessp "a" "A"))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_lessp_accent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (string-lessp "cafe" "café") (string-lessp "café" "cafe")))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_version_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1.1\" \"1.2\" \"1.10\" \"1.20\" \"2.0\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(sort '("1.10" "1.2" "1.1" "2.0" "1.20") #'string-version-lessp)
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_version_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-version-lessp "1.0" "2.0")
      (string-version-lessp "1.10" "1.2")
      (string-version-lessp "1.0a" "1.0b"))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_version_greater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-version-greaterp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-version-greaterp "2.0" "1.0")
      (string-version-greaterp "1.0" "2.0"))
"##,
        expect,
    );
}

#[test]
fn div_cx513_compare_strings_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (compare-strings "abc" nil nil "ABC" nil nil t)
      (compare-strings "ABC" nil nil "abc" nil nil t))
"##,
        expect,
    );
}

#[test]
fn div_cx513_compare_strings_accent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (compare-strings "cafe" nil nil "CAFE" nil nil t)))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_prefix_suffix_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-prefix-p "" "abc")
      (string-prefix-p "abc" "")
      (string-suffix-p "" "abc"))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_remove_prefix_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-remove-prefix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-remove-prefix "abc" "abcdef")
      (string-remove-prefix "xyz" "abcdef")
      (string-remove-suffix "def" "abcdef"))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_replace_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-replace "a" "x" "aaa aaa")
      (string-replace "" "x" "abc")
      (string-replace "a" "" "abcabc"))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_pad_trim_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abc       \" \"abc\" \"abc\" \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (list (string-pad "abc" 10)
        (string-trim "  abc  ")
        (string-trim-left "  abc")
        (string-trim-right "abc  ")))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_chop_newline_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"hello\\r\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (list (string-chop-newline "hello\n")
        (string-chop-newline "hello\r\n")
        (string-chop-newline "hello")))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_limit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\" \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (list (string-limit "hello world" 5)
        (string-limit "hello world" 5 t)
        (string-limit "abc" 10)))
"##,
        expect,
    );
}

#[test]
fn div_cx513_string_lines_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"hello\" \"world\" \"third\") (\"\") (\"single\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (list (string-lines "hello\nworld\nthird")
        (string-lines "")
        (string-lines "single")))
"##,
        expect,
    );
}
