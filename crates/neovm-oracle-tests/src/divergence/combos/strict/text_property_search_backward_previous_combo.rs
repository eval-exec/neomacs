//! Strict combo oracle probes, batch 363: text-property-search backward +
//! previous-single-char-property-change. text-property-search-backward,
//! previous-single-char-property-change, previous-char-property-change,
//! and get-pos-property at boundaries.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_search_backward_prop_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCCDDDDD")
  (add-text-properties 1 6 '(face bold))
  (add-text-properties 11 16 '(face italic))
  (goto-char (point-max))
  (let ((m (text-property-search-backward 'face 'italic)))
    (list (and m (prop-match-beginning m))
          (and m (prop-match-end m))
          (and m (prop-match-value m)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function text-property-search-backward)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_previous_single_char_property_change_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ")
  (add-text-properties 3 7 '(face bold))
  (add-text-properties 12 16 '(face italic))
  (list (previous-single-char-property-change 20 'face)
        (previous-single-char-property-change 14 'face)
        (previous-single-char-property-change 5 'face)
        (previous-char-property-change 20)
        (previous-char-property-change 2)))
"##;
    let expect = expect_test::expect![[r#""OK (16 12 3 16 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_text_property_search_predicate_not_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCC")
  (add-text-properties 1 6 '(face bold))
  (add-text-properties 11 16 '(face underline))
  (goto-char 1)
  (let ((m1 (text-property-search-forward 'face nil nil)))
    (goto-char 1)
    (let ((m2 (text-property-search-forward 'face 'bold t)))
      (goto-char (point-max))
      (let ((m3 (text-property-search-backward 'face)))
        (list (and m1 (prop-match-value m1))
              (and m2 (prop-match-value m2))
              (and m3 (prop-match-value m3)))))))
"##;
    let expect = expect_test::expect![[r#""OK (bold bold underline)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
