//! Strict combo oracle probes, batch 259: window / buffer-display variable
//! existence sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_pop_up_windows_frames_display_buffer_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'pop-up-windows)
      (boundp 'pop-up-frames)
      (boundp 'same-window-regexps)
      (boundp 'same-window-buffer-names)
      (boundp 'special-display-regexps)
      (boundp 'special-display-buffer-names)
      (boundp 'display-buffer-alist)
      (boundp 'display-buffer-base-action)
      (boundp 'display-buffer-alist)
      (boundp 'display-buffer-overriding-action)
      (boundp 'even-window-heights)
      (boundp 'switch-to-buffer-obey-display-actions))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_combination_split_threshold_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'window-combination-resize)
      (boundp 'window-combination-limit)
      (boundp 'split-window-preferred-function)
      (boundp 'split-width-threshold)
      (boundp 'split-height-threshold)
      (boundp 'window-sides-slots)
      (boundp 'window-sides-vertical)
      (boundp 'window-resize-pixelwise)
      (boundp 'frame-resize-pixelwise)
      (boundp 'fit-window-to-buffer-horizontally)
      (boundp 'fit-frame-to-buffer)
      (boundp 'window-text-pixel-size))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_dedicated_balance_vars_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'window-size-change-functions)
      (boundp 'window-scroll-functions)
      (boundp 'window-configuration-change-hook)
      (boundp 'window-buffer-change-functions)
      (boundp 'window-selection-change-functions)
      (boundp 'window-state-change-functions)
      (boundp 'window-state-change-hook)
      (boundp 'balance-windows)
      (boundp 'window-persistent-parameters)
      (boundp 'window-parameters)
      (boundp 'ignore-window-parameters))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
