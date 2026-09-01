//! Strict combo oracle probes, batch 384: text-property interaction with
//! copy/deletion/narrowing. set-text-properties after narrow, copy buffer
//! region with props, delete-region merging adjacent props, and narrowing
//! interaction with text-property search.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_set_after_narrow_copy_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCCDDDDD")
  (add-text-properties 1 6 '(face bold))
  (add-text-properties 6 11 '(face italic))
  (narrow-to-region 3 14)
  (list (point-min)
        (point-max)
        (get-text-property (point-min) 'face)
        (get-text-property 6 'face)
        (get-text-property 11 'face)
        (set-text-properties 5 10 '(face underline))
        (get-text-property 5 'face)
        (get-text-property 11 'face)))
"##;
    let expect = expect_test::expect![[r#""OK (3 14 bold italic nil t underline nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_delete_region_merge_adjacent_face_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCC")
  (add-text-properties 1 6 '(face bold))
  (add-text-properties 11 16 '(face bold))
  (let ((before (list (get-text-property 1 'face)
                      (get-text-property 11 'face)
                      (get-text-property 6 'face))))
    (delete-region 6 11)
    (list before
          (buffer-string)
          (get-text-property 1 'face)
          (get-text-property 5 'face)
          (get-text-property 6 'face))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((bold bold nil) #(\"AAAAACCCCC\" 0 5 (face bold) 5 10 (face bold)) bold bold bold)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_copy_region_as_kill_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "Hello World")
  (add-text-properties 1 6 '(face bold weight heavy))
  (add-text-properties 7 12 '(face italic))
  (copy-region-as-kill 1 12)
  (with-temp-buffer
    (yank)
    (list (buffer-string)
          (get-text-property 1 'face)
          (get-text-property 7 'face)
          (get-text-property 1 'weight)
          (text-properties-at 1)
          (text-properties-at 7))))
"##;
    let expect = expect_test::expect![[
        r#""OK (#(\"Hello World\" 0 5 (weight heavy face bold) 6 10 (face italic) 10 11 (rear-nonsticky t face italic)) bold italic heavy (weight heavy face bold) (face italic))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
