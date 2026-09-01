//! Strict combo oracle probes, batch 267: display-property behavioral. (space
//! ...) width/align-to spec, raise/height display specs, and before-string/
//! after-string via overlay.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_display_space_width_align_to_spec_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBB")
  (put-text-property 1 2 'display '(space :width 5))
  (put-text-property 6 7 'display '(space :align-to 20))
  (list (get-text-property 1 'display)
        (get-text-property 2 'display)
        (get-text-property 6 'display)))
"##;
    let expect = expect_test::expect![[r#""OK ((space :width 5) nil (space :align-to 20))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_display_raise_height_spec_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "superscript subscript")
  (put-text-property 1 5 'display '(raise 0.5))
  (put-text-property 6 9 'display '(height 1.5))
  (list (get-text-property 1 'display)
        (get-text-property 6 'display)
        (get-text-property 11 'display)))
"##;
    let expect = expect_test::expect![[r#""OK ((raise 0.5) (height 1.5) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_before_after_string_overlay_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBB")
  (let ((o (make-overlay 3 5)))
    (overlay-put o 'before-string "<<<")
    (overlay-put o 'after-string ">>>")
    (overlay-put o 'display "REPLACED")
    (list (overlay-get o 'before-string)
          (overlay-get o 'after-string)
          (overlay-get o 'display)
          (overlay-start o)
          (overlay-end o))))
"##;
    let expect = expect_test::expect![[r#""OK (\"<<<\" \">>>\" \"REPLACED\" 3 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
