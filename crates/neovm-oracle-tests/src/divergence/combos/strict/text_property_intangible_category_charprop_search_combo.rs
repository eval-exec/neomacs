//! Strict combo oracle probes, batch 158: advanced text-property semantics.
//! font-lock-face + intangible + category property stacking, char-property
//! search (next-single-char-property-change, next-char-property-change,
//! previous-single-char-property-change), text-property-any / not-all over
//! ranges, and property stickiness under deletion.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_intangible_category_stacked() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAAAAAAA")
  (add-text-properties 2 5 '(font-lock-face bold intangible t category cat-x))
  (put-text-property 6 8 'font-lock-face 'italic)
  (list (get-char-property 1 'font-lock-face)
        (get-char-property 3 'font-lock-face)
        (get-char-property 3 'category)
        (get-char-property 3 'intangible)
        (get-char-property 7 'font-lock-face)
        (get-char-property 9 'font-lock-face)))
"##;
    let expect = expect_test::expect![[r#""OK (nil bold cat-x t italic nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_property_change_search_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ")
  (add-text-properties 3 7 '(face bold))
  (add-text-properties 11 14 '(face italic))
  (list (next-single-char-property-change 1 'face)
        (next-single-char-property-change 3 'face)
        (next-single-char-property-change 7 'face)
        (next-single-char-property-change 1 'category)
        (previous-single-char-property-change 20 'face)
        (previous-single-char-property-change 12 'face)
        (next-char-property-change 1)
        (next-char-property-change 13)
        (text-property-any 1 21 'face 'italic)
        (text-property-not-all 1 21 'face nil)
        (text-property-any 1 21 'face 'underline)))
"##;
    let expect = expect_test::expect![[r#""OK (3 7 11 21 14 11 3 14 11 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_property_stickiness_deletion_rear_front() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAABBBCCC")
  (add-text-properties 1 4 '(face bold rear-nonsticky nil))
  (add-text-properties 4 7 '(face italic front-sticky nil))
  (add-text-properties 7 9 '(face underline))
  (let ((before (list (text-properties-at 3)
                      (text-properties-at 4)
                      (text-properties-at 6)
                      (text-properties-at 7)))
        (gap-str (delete-and-extract-region 4 7)))
    (list before
          gap-str
          (buffer-string)
          (text-properties-at 3)
          (text-properties-at 4)
          (text-properties-at 5)
          (length (buffer-string)))))
"##;
    let expect = expect_test::expect![[
        r#""OK (((rear-nonsticky nil face bold) (front-sticky nil face italic) (front-sticky nil face italic) (face underline)) #(\"BBB\" 0 3 (front-sticky nil face italic)) #(\"AAACCC\" 0 3 (rear-nonsticky nil face bold) 3 5 (face underline)) (rear-nonsticky nil face bold) (face underline) (face underline) 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
