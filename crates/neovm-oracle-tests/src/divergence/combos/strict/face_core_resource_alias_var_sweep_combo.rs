//! Strict combo oracle probes, batch 265: face CORE variable existence sweep.
//! Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_face_new_frame_defaults_remapping_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'face-id)
      (boundp 'face-new-frame-defaults)
      (boundp 'face-remapping-alist)
      (boundp 'face-font-rescale-alist)
      (boundp 'face-ignored-fonts)
      (boundp 'face-default-stipple)
      (boundp 'face-near-same-color-threshold)
      (boundp 'face-distinct-worth)
      (boundp 'face-font-selection-order)
      (boundp 'face-font-family-alternatives)
      (boundp 'face-font-registry-alternatives)
      (boundp 'face-alias-alist))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t t t t t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_face_definition_face_attribute_resolved_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'face-documentation)
      (boundp 'face-override-user)
      (boundp 'face-modification-hook)
      (boundp 'face-x-resources)
      (boundp 'face-default-family)
      (boundp 'font-weight-table)
      (boundp 'font-slant-table)
      (boundp 'font-width-table)
      (boundp 'scalable-fonts-allowed)
      (boundp 'underline-minimum-offset)
      (boundp 'x-use-underline-position-properties)
      (boundp 'x-underline-at-descent-line))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil nil t nil t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_face_attr_constants_support_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'face-attribute-name-alist)
      (boundp 'face-attributes-as-vector)
      (boundp 'face-attribute-name-p)
      (boundp 'face-x-resource-types)
      (boundp 'tty-mode-alist)
      (boundp 'tty-built-in-mode-alist)
      (boundp 'frame-creation-function)
      (boundp 'window-system)
      (boundp 'initial-window-system)
      (boundp 'window-setup-hook))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
