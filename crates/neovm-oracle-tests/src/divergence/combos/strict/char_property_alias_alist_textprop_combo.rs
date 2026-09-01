//! Strict combo oracle probes, batch 336: char-property-alias-alist. Aliasing
//! text properties so one property delegates to another, and the interaction
//! with get-text-property / get-char-property.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_property_alias_alist_delegate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "Hello World")
  (let ((saved char-property-alias-alist))
    (unwind-protect
        (progn
          (setq char-property-alias-alist '((fontified . face)))
          (add-text-properties 1 6 '(face bold))
          (list (get-text-property 1 'face)
                (get-text-property 1 'fontified)
                (get-char-property 1 'fontified)
                char-property-alias-alist))
      (setq char-property-alias-alist saved))))
"##;
    let expect = expect_test::expect![[r#""OK (bold nil nil ((fontified . face)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_property_alias_chain_multi_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "PropTest")
  (let ((saved char-property-alias-alist))
    (unwind-protect
        (progn
          (setq char-property-alias-alist '((c . b) (b . a)))
          (add-text-properties 1 5 '(a original-val))
          (list (get-text-property 1 'a)
                (get-text-property 1 'b)
                (get-text-property 1 'c)
                (get-char-property 1 'c)))
      (setq char-property-alias-alist saved))))
"##;
    let expect = expect_test::expect![[r#""OK (original-val nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_property_alias_real_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "OverrideTest")
  (let ((saved char-property-alias-alist))
    (unwind-protect
        (progn
          (setq char-property-alias-alist '((alias . real)))
          (add-text-properties 1 7 '(real real-val))
          (add-text-properties 3 7 '(alias alias-val))
          (list (get-text-property 1 'real)
                (get-text-property 1 'alias)
                (get-text-property 3 'real)
                (get-text-property 3 'alias)))
      (setq char-property-alias-alist saved))))
"##;
    let expect = expect_test::expect![[r#""OK (real-val nil real-val alias-val)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
