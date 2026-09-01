//! Strict combo oracle probes, batch 316: button-type inheritance. define-
//! button-type with :supertype, button-type-put/get, inherited properties,
//! and make-button/make-text-button with a custom type.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_button_type_supertype_inheritance_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'button)
(define-button-type 'probe-btn-base 'face 'bold 'help-echo "base-tooltip")
(define-button-type 'probe-btn-child 'supertype 'probe-btn-base 'face 'italic)
(button-type-put 'probe-btn-child 'extra 'extra-val)
(list (button-type-get 'probe-btn-base 'face)
      (button-type-get 'probe-btn-child 'face)
      (button-type-get 'probe-btn-child 'help-echo)
      (button-type-get 'probe-btn-child 'extra)))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 9 49)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_make_button_with_type_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'button)
(define-button-type 'probe-typed-btn 'face 'underline 'help-echo "typed-tip")
(with-temp-buffer
  (insert "Click here for more info")
  (let ((btn (make-button 7 11 'type 'probe-typed-btn 'action (lambda (b) 'clicked))))
    (list (buttonp btn)
          (button-get btn 'face)
          (button-get btn 'help-echo)
          (button-get btn 'type)
          (button-start btn)
          (button-end btn))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function buttonp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_insert_button_default_subtype_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'button)
(with-temp-buffer
  (insert "Before and After")
  (let ((b1 (insert-button "LINK1" 'type 'link))
        (b2 (insert-button "LINK2" 'action (lambda (&rest _) nil))))
    (list (buttonp b1)
          (buttonp b2)
          (button-label b1)
          (button-label b2)
          (button-get b1 'type))))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Unknown button type ‘link’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
