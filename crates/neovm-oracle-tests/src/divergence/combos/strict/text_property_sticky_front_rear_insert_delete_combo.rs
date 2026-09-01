//! Strict combo oracle probes, batch 317: text-property stickiness edge cases.
//! front-sticky / rear-nonsticky behavior on insert between properties,
//! deletion merging, and set-text-properties bulk replacement.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_sticky_insert_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAABBBCCC")
  (add-text-properties 1 4 '(face bold front-sticky nil rear-nonsticky nil))
  (add-text-properties 4 7 '(face italic rear-sticky nil))
  (let ((before (list (text-properties-at 3) (text-properties-at 4))))
    (goto-char 4)
    (insert "X")
    (list before
          (text-properties-at 3)
          (text-properties-at 4)
          (text-properties-at 5)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[
        r#""OK (((rear-nonsticky nil front-sticky nil face bold) (rear-sticky nil face italic)) (rear-nonsticky nil front-sticky nil face bold) nil (rear-sticky nil face italic) #(\"AAAXBBBCCC\" 0 3 (rear-nonsticky nil front-sticky nil face bold) 4 7 (rear-sticky nil face italic)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_set_text_properties_bulk_replace_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789")
  (add-text-properties 1 6 '(face bold weight heavy))
  (set-text-properties 3 8 '(face italic color red))
  (list (text-properties-at 1)
        (text-properties-at 3)
        (text-properties-at 5)
        (text-properties-at 8)
        (text-properties-at 9)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((weight heavy face bold) (face italic color red) (face italic color red) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_text_property_delete_region_merge_adjacent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAABBBBCCCCDDDD")
  (add-text-properties 1 5 '(face bold))
  (add-text-properties 9 13 '(face italic))
  (let ((before (list (get-text-property 1 'face)
                      (get-text-property 9 'face))))
    (delete-region 5 9)
    (list before
          (buffer-string)
          (get-text-property 1 'face)
          (get-text-property 4 'face)
          (get-text-property 5 'face)
          (get-text-property 8 'face))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((bold italic) #(\"AAAACCCCDDDD\" 0 4 (face bold) 4 8 (face italic)) bold bold italic italic)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
