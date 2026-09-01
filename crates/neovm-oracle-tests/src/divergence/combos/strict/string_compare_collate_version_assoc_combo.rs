//! Strict combo oracle probes, batch 153: string comparison and collation.
//! string-lessp/greaterp, string-version-lessp natural-order (numeric),
//! compare-strings with START/END slices + ignore-case, assoc-string with
//! case-fold variants, and string-collate-lessp under the C locale incl
//! diacritics.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_string_lessp_version_lessp_greatest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-lessp "abc" "abd")
      (string-lessp "abc" "abc")
      (string-lessp "abc" "ab")
      (string-greaterp "abc" "ab")
      (string-version-lessp "foo2.png" "foo10.png")
      (string-version-lessp "foo1.2" "foo1.10")
      (string-version-lessp "foo9" "foo10")
      (string-version-lessp "foo2bar" "foo10bar")
      (string-lessp "abc" "abd")
      (sort '("foo10" "foo2" "foo1" "foo20") #'string-version-lessp)
      (sort '("foo10" "foo2" "foo1" "foo20") #'string-lessp))
"##;
    let expect = expect_test::expect![[
        r#""OK (t nil nil t t t t t t (\"foo1\" \"foo2\" \"foo10\" \"foo20\") (\"foo1\" \"foo10\" \"foo2\" \"foo20\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_compare_strings_slices_ignore_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (compare-strings "abcdef" 1 nil "abcXYZ" 1 3)
      (compare-strings "abc" 1 nil "abc" 1 nil)
      (compare-strings "abc" 1 2 "abc" 1 2)
      (compare-strings "ABCdef" 1 nil "abcdef" 1 nil t)
      (compare-strings "ABCdef" 1 nil "abcdef" 1 nil nil)
      (compare-strings "xyz" 1 nil "abc" 1 nil)
      (compare-strings "abc" 1 nil "abx" 1 nil)
      (compare-strings "" 1 nil "" 1 nil))
"##;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range \"\" 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_assoc_string_case_fold_and_collate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (assoc-string "b" '(("a" . 1) ("b" . 2) ("B" . 3)))
      (assoc-string "B" '(("a" . 1) ("b" . 2) ("B" . 3)))
      (assoc-string "B" '(("a" . 1) ("b" . 2)) nil)
      (assoc-string "B" '(("a" . 1) ("b" . 2)) t)
      (assoc-string 'b '((a . 1) (b . 2)))
      (assoc-string "missing" '(("a" . 1)))
      (string-collate-lessp "abc" "abd" "C")
      (string-collate-lessp "abc" "abc" "C")
      (string-collate-lessp "cote" "côte" "C")
      (string-collate-equalp "abc" "abc" "C")
      (string-collate-equalp "ABC" "abc" "C" t))
"##;
    let expect = expect_test::expect![[
        r#""OK ((\"b\" . 2) (\"B\" . 3) nil (\"b\" . 2) (b . 2) nil t nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
