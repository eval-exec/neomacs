//! Strict combo oracle probes, batch 255: font/face variable existence sweep.
//! boundp over standard font/face defcustoms. Any nil-in-Neomacs/t-in-GNU is a
//! missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_font_selection_order_registry_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'face-font-family-alternatives)
      (boundp 'face-font-registry-alternatives)
      (boundp 'face-font-selection-order)
      (boundp 'face-remapping-alist)
      (boundp 'scalable-fonts-allowed)
      (boundp 'font-list-limit)
      (boundp 'font-weight-table)
      (boundp 'font-slant-table)
      (boundp 'font-width-table)
      (boundp 'font-caching)
      (boundp 'underline-minimum-offset)
      (boundp 'x-use-underline-position-properties)
      (boundp 'line-spacing-default))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t nil t t t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_face_attribute_canonical_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'face-new-frame-defaults)
      (boundp 'face-default-stipple)
      (boundp 'face-ignored-fonts)
      (boundp 'face-near-same-color-threshold)
      (boundp 'face-distinct-worth)
      (boundp 'tty-defined-color-alist)
      (boundp 'w32-enable-synthesized-fonts)
      (boundp 'face-font-rescale-alist)
      (boundp 'face-default-family)
      (boundp 'default-text-properties)
      (boundp 'fontification-functions)
      (boundp 'jit-lock-context-time))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t t nil t nil t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_color_palette_x_resources_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'color-names-alist)
      (boundp 'color-distance-rgb)
      (boundp 'tty-color-mode)
      (boundp 'initial-frame-alist)
      (boundp 'default-frame-alist)
      (boundp 'minibuffer-frame-alist)
      (boundp 'window-system-default-frame-alist)
      (boundp 'cursor-in-non-selected-windows)
      (boundp 'mode-line-in-non-selected-windows))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
