//! Strict combo oracle probes, batch 213: the button library. make-button /
//! insert-button overlay creation, button-at, button-start/end, button-get/
//! button-put, buttonp, and forward-button navigation.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_button_make_at_start_end_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'button)
(with-temp-buffer
  (insert "Click here for more info")
  (make-button 7 11 'help-echo "tooltip text" 'category 'link)
  (let ((btn (button-at 9)))
    (list (buttonp btn)
          (button-start btn)
          (button-end btn)
          (button-get btn 'help-echo)
          (button-get btn 'category))))
"##;
    let expect = expect_test::expect![[
        r#""ERR (error \"Button ‘category’ property may not be set directly\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_button_insert_forward_navigation_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'button)
(with-temp-buffer
  (insert "Before and After")
  (let ((b1 (insert-button "LINK1"))
        (b2 (insert-button "LINK2")))
    (button-put b1 'action 'act1)
    (button-put b2 'action 'act2)
    (goto-char (point-min))
    (forward-button 1)
    (let ((p1 (point)))
      (forward-button 1)
      (list p1
            (point)
            (button-get b1 'action)
            (button-get b2 'action)
            (button-label b1)
            (button-label b2)))))
"##;
    let expect = expect_test::expect![[r#""OK (17 22 act1 act2 \"LINK1\" \"LINK2\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_button_overlay_properties_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'button)
(with-temp-buffer
  (insert "wrapped text here")
  (let ((btn (make-button 1 8 'face 'link 'mouse-face 'highlight)))
    (list (buttonp btn)
          (button-get btn 'face)
          (button-get btn 'mouse-face)
          (button-start btn)
          (button-end btn)
          (overlayp btn)
          (progn (button-put btn 'face 'bold) (button-get btn 'face)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function buttonp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
