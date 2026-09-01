//! Strict combo oracle probes, batch 351: text-property-default-nonsticky +
//! wrap-prefix / line-prefix. Default nonsticky property control,
//! wrap-prefix / line-prefix text properties, and their effect on
//! subsequent insertion.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_default_nonsticky_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCC")
  (add-text-properties 1 6 '(face bold))
  (let ((saved text-property-default-nonsticky))
    (unwind-protect
        (progn
          (setq text-property-default-nonsticky '((face . t)))
          (goto-char 6)
          (insert "X")
          (list (get-text-property 5 'face)
                (get-text-property 6 'face)
                (buffer-string)))
      (setq text-property-default-nonsticky saved))))
"####;
    let expect =
        expect_test::expect![[r#""OK (bold nil #(\"AAAAAXBBBBBCCCCC\" 0 5 (face bold)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_wrap_prefix_line_prefix_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"
(with-temp-buffer
  (insert "Line one\nLine two\nLine three")
  (add-text-properties 1 9 '(wrap-prefix ">>> " line-prefix "### "))
  (list (get-text-property 1 'wrap-prefix)
        (get-text-property 1 'line-prefix)
        (get-text-property 5 'wrap-prefix)
        (get-text-property 10 'wrap-prefix)))
"####;
    let expect = expect_test::expect![[r####""OK (\">>> \" \"### \" \">>> \" nil)""####]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_intangible_field_point_entered_left_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"
(with-temp-buffer
  (insert "AAAintangibleBBB")
  (add-text-properties 4 14 '(intangible t category cat-x))
  (list (get-text-property 4 'intangible)
        (get-text-property 4 'category)
        (get-text-property 3 'intangible)
        (text-properties-at 4)))
"####;
    let expect = expect_test::expect![[r#""OK (t cat-x nil (category cat-x intangible t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
