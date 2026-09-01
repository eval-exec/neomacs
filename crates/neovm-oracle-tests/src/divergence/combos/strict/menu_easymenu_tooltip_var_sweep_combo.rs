//! Strict combo oracle probes, batch 283: menu / easymenu / tooltip CORE
//! variable sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_menu_bar_final_items_update_hook_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'menu-bar-mode)
      (boundp 'menu-bar-final-items)
      (boundp 'menu-bar-update-hook)
      (boundp 'global-menu-bar-update-hook)
      (boundp 'tmm-table-mismatch-list)
      (boundp 'tmm-km-list)
      (boundp 'tmm-completion-prompt)
      (boundp 'tmm-mid-prompt)
      (boundp 'easy-menu-always-add)
      (boundp 'tooltip-mode)
      (boundp 'tooltip-functions)
      (boundp 'tooltip-hide-delay))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil nil nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_menu_item_shortcut_checkbox_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'menu-item)
      (boundp 'easy-menu-define)
      (boundp 'easy-menu-change)
      (boundp 'easy-menu-item-present-p)
      (boundp 'easy-menu-do-add-item)
      (boundp 'menu-bar-separator-alist)
      (boundp 'menu-bar-help-menu)
      (boundp 'tooltip-frame-parameters)
      (boundp 'tooltip-delay)
      (boundp 'tooltip-short-delay)
      (boundp 'tooltip-x-offset)
      (boundp 'tooltip-y-offset))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_popup_menu_dialog_box_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'popup-menu)
      (boundp 'x-popup-dialog)
      (boundp 'x-popup-menu)
      (boundp 'popup-menu-loop)
      (boundp 'dialog-box)
      (boundp 'help-event-list)
      (boundp 'mouse-avoidance-mode)
      (boundp 'mouse-avoidance-threshold)
      (boundp 'mouse-avoidance-pointer-shape)
      (boundp 'mode-line-default-help-echo)
      (boundp 'tooltip-use-effort-mode)
      (boundp 'lazy-unfontify))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil t t nil nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
