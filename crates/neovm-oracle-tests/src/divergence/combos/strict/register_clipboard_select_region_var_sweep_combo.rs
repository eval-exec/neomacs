//! Strict combo oracle probes, batch 290: register / clipboard / select-region
//! CORE variable sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_register_clipboard_interprogram_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'register-alist)
      (boundp 'register-separator)
      (boundp 'delete-selection-mode)
      (boundp 'delete-active-region)
      (boundp 'use-empty-active-region)
      (boundp 'select-active-regions)
      (boundp 'x-select-enable-clipboard)
      (boundp 'x-select-enable-primary)
      (boundp 'gui-select-selection-function)
      (boundp 'save-interprogram-paste-before-kill)
      (boundp 'interprogram-cut-function)
      (boundp 'interprogram-paste-function))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_kill_ring_yank_handler_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'kill-ring)
      (boundp 'kill-ring-yank-pointer)
      (boundp 'kill-ring-max)
      (boundp 'kill-whole-line)
      (boundp 'kill-read-only-ok)
      (boundp 'kill-do-not-save-duplicates)
      (boundp 'yank-handler)
      (boundp 'yank-excluded-properties)
      (boundp 'yank-undo-function)
      (boundp 'filter-buffer-substring-functions)
      (boundp 'buffer-substring-filters)
      (boundp 'rectangle-mark-mode))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_narrow_region_transient_mark_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'transient-mark-mode)
      (boundp 'default-transient-mark-mode)
      (boundp 'handle-shift-selection)
      (boundp 'mark-even-if-inactive)
      (boundp 'deactivate-mark)
      (boundp 'mark-active)
      (boundp 'activate-mark-hook)
      (boundp 'deactivate-mark-hook)
      (boundp 'region-extract-function)
      (boundp 'narrow-to-region)
      (boundp 'point-before-scroll)
      (boundp 'cache-long-line-scans))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil t t t t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
