//! Strict combo oracle probes, batch 249: subword movement. forward/backward-
//! subword over camelCase and PascalCase identifiers, subword mark/transpose,
//! and capitalization.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_subword_forward_camel_pascal_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'subword)
(with-temp-buffer
  (insert "camelCaseWord PascalCase")
  (goto-char 1)
  (let ((p1 (progn (subword-forward 1) (point)))
        (p2 (progn (subword-forward 1) (point)))
        (p3 (progn (subword-forward 1) (point)))
        (p4 (progn (subword-forward 1) (point)))
        (p5 (progn (subword-forward 1) (point))))
    (list p1 p2 p3 p4 p5)))
"##;
    let expect = expect_test::expect![[r#""OK (6 10 14 21 25)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_subword_backward_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'subword)
(with-temp-buffer
  (insert "myVariableName")
  (goto-char (point-max))
  (let ((b1 (progn (subword-backward 1) (point)))
        (b2 (progn (subword-backward 1) (point)))
        (b3 (progn (subword-backward 1) (point))))
    (list b1 b2 b3)))
"##;
    let expect = expect_test::expect![[r#""OK (11 3 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_subword_capitalize_upcase_downcase_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'subword)
(list (with-temp-buffer
        (insert "myVarName")
        (goto-char 1)
        (subword-forward 1)
        (subword-capitalize 1)
        (buffer-string))
      (with-temp-buffer
        (insert "myVarName")
        (goto-char 1)
        (subword-upcase 1)
        (buffer-string))
      (with-temp-buffer
        (insert "MYVARName")
        (goto-char 1)
        (subword-downcase 1)
        (buffer-string)))
"##;
    let expect = expect_test::expect![[r#""OK (\"myVarName\" \"MYVarName\" \"myvarName\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
