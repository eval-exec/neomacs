//! Strict combo oracle probes, batch 204: newer property-search APIs.
//! text-property-search-forward with various PREDICATE values (nil, t, equal,
//! not equal), and the match-data / match positions it sets, plus
//! get-pos-property + char-property boundaries.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_search_forward_predicate_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCCDDDDD")
  (add-text-properties 1 6 '(face bold))
  (add-text-properties 6 11 '(face italic))
  (add-text-properties 11 16 '(face underline))
  (goto-char 1)
  (let ((m1 (text-property-search-forward 'face nil nil)))
    (list (if m1 (prop-match-beginning m1) nil)
          (if m1 (prop-match-end m1) nil)
          (if m1 (prop-match-value m1) nil)
          (point))))
"##;
    let expect = expect_test::expect![[r#""OK (1 6 bold 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_text_property_search_specific_value_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCCDDDDD")
  (add-text-properties 1 6 '(face bold))
  (add-text-properties 6 11 '(face italic))
  (add-text-properties 11 16 '(face bold))
  (goto-char 1)
  (let ((m1 (text-property-search-forward 'face 'italic))
        (p1 (point)))
    (goto-char 1)
    (let ((m2 (text-property-search-forward 'face 'bold t)))
      (list (and m1 (prop-match-value m1))
            p1
            (and m2 (prop-match-beginning m2))
            (and m2 (prop-match-value m2))))))
"##;
    let expect = expect_test::expect![[r#""OK (bold 6 1 bold)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_get_pos_property_text_property_search_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAAAAAAA")
  (add-text-properties 3 7 '(face bold))
  (add-text-properties 8 10 '(face italic))
  (list (get-pos-property 5 'face)
        (get-pos-property 1 'face)
        (get-pos-property 8 'face)
        (get-pos-property 10 'face)
        (progn (goto-char (point-max))
               (let ((mb (text-property-search-backward 'face nil nil)))
                 (and mb (prop-match-value mb))))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function text-property-search-backward)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
