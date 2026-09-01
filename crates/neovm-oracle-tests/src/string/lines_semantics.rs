//! Oracle parity tests for GNU `subr.el` `string-lines`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_subr_string_lines_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:string-lines special-cases the empty string, can preserve
    // newline characters, suppresses empty newline-only results when both
    // OMIT-EMPTY and KEEP-NEWLINES are non-nil, and returns the original string
    // object when there are no newlines.
    let form = r#"(let* ((plain "abc")
       (props (propertize "a\nb\n" 'face 'bold)))
  (list
   (string-lines "")
   (string-lines "" t)
   (string-lines "a\nb\n")
   (string-lines "a\nb\n" t)
   (string-lines "a\n\nb" t)
   (string-lines "a\n\nb" t t)
   (string-lines "a\n\nb" nil t)
   (eq (car (string-lines plain)) plain)
   (mapcar (lambda (s)
             (list s (text-properties-at 0 s)))
           (string-lines props nil t))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\") nil (\"a\" \"b\") (\"a\" \"b\") (\"a\" \"b\") (\"a\\n\" \"b\") (\"a\\n\" \"\\n\" \"b\") t ((#(\"a\\n\" 0 2 (face bold)) (face bold)) (#(\"b\\n\" 0 2 (face bold)) (face bold))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
