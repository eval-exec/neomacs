//! Strict combo oracle probes, batch 264: keyboard / keyboard-macro CORE
//! variable existence sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable
//! bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_defining_executing_kbd_macro_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'defining-kbd-macro)
      (boundp 'last-kbd-macro)
      (boundp 'executing-kbd-macro)
      (boundp 'executing-kbd-macro-index)
      (boundp 'kbd-macro-termination-hook)
      (boundp 'prefix-command)
      (boundp 'prefix-command-echo-keystrokes)
      (boundp 'kbd-macro-timer-start)
      (boundp 'echo-keystrokes)
      (boundp 'keyboard-escape-quit-handler)
      (boundp 'overriding-arrow-text))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_input_event_deactivate_mark_core_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'deactivate-mark)
      (boundp 'mark-active)
      (boundp 'mark-ring)
      (boundp 'global-mark-ring)
      (boundp 'global-mark-ring-max)
      (boundp 'transient-mark-mode)
      (boundp 'handle-shift-selection)
      (boundp 'select-active-regions)
      (boundp 'delete-selection-mode)
      (boundp 'delete-active-region)
      (boundp 'use-empty-active-region))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_kill_ring_yank_register_core_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'kill-ring)
      (boundp 'kill-ring-yank-pointer)
      (boundp 'kill-ring-max)
      (boundp 'kill-whole-line)
      (boundp 'kill-read-only-ok)
      (boundp 'kill-do-not-save-duplicates)
      (boundp 'yank-window-start)
      (boundp 'yank-undo-function)
      (boundp 'register-alist)
      (boundp 'register-separator)
      (boundp 'undo-limit)
      (boundp 'undo-strong-limit)
      (boundp 'undo-outer-limit))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
