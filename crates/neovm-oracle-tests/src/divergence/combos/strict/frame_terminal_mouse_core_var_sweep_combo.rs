//! Strict combo oracle probes, batch 272: frame / terminal / mouse CORE
//! variable sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_frame_alist_window_system_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'default-frame-alist)
      (boundp 'initial-frame-alist)
      (boundp 'minibuffer-frame-alist)
      (boundp 'window-system)
      (boundp 'window-system-default-frame-alist)
      (boundp 'initial-window-system)
      (boundp 'frame-creation-function)
      (boundp 'frame-inherited-parameters)
      (boundp 'frame-alpha-lower-limit)
      (boundp 'frame-title-format)
      (boundp 'icon-title-format)
      (boundp 'iconify-child-frame))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_mouse_yank_drag_autoselect_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'mouse-position-function)
      (boundp 'mouse-yank-at-point)
      (boundp 'mouse-autoselect-window)
      (boundp 'mouse-drag-copy-region)
      (boundp 'double-click-time)
      (boundp 'double-click-fuzz)
      (boundp 'mouse-1-click-in-selected-window)
      (boundp 'mouse-1-click-follows-link)
      (boundp 'track-mouse)
      (boundp 'make-pointer-invisible)
      (boundp 'mouse-avoidance-mode)
      (boundp 'mouse-avoidance-threshold))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_terminal_tool_bar_tab_bar_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'tool-bar-mode)
      (boundp 'tool-bar-map)
      (boundp 'tool-bar-style)
      (boundp 'tab-bar-mode)
      (boundp 'tab-bar-format)
      (boundp 'tab-bar-new-tab-choice)
      (boundp 'menu-bar-mode)
      (boundp 'menu-bar-final-items)
      (boundp 'scroll-bar-mode)
      (boundp 'scroll-bar-width)
      (boundp 'horizontal-scroll-bar-mode)
      (boundp 'horizontal-scroll-bar-height))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
