//! Strict combo oracle probes, batch 269: standard hook variable sweep.
//! boundp over init/startup/lifecycle/change hooks. Any nil-in-Neomacs/t-in-GNU
//! is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_init_startup_lifecycle_hook_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'before-init-hook)
      (boundp 'after-init-hook)
      (boundp 'emacs-startup-hook)
      (boundp 'term-setup-hook)
      (boundp 'window-setup-hook)
      (boundp 'before-pdump-load-hook)
      (boundp 'kill-emacs-hook)
      (boundp 'kill-emacs-query-functions)
      (boundp 'suspend-hook)
      (boundp 'suspend-resume-hook)
      (boundp 'save-place-alist)
      (boundp 'post-gc-hook))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t nil t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_change_post_command_hook_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'before-change-functions)
      (boundp 'after-change-functions)
      (boundp 'first-change-hook)
      (boundp 'pre-command-hook)
      (boundp 'post-command-hook)
      (boundp 'post-self-insert-hook)
      (boundp 'minibuffer-setup-hook)
      (boundp 'minibuffer-exit-hook)
      (boundp 'mouse-leave-buffer-hook)
      (boundp 'mouse-leave-frame-hook)
      (boundp 'menu-bar-update-hook)
      (boundp 'echo-area-clear-hook))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_mark_special_event_hook_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'activate-mark-hook)
      (boundp 'deactivate-mark-hook)
      (boundp 'special-event-map)
      (boundp 'special-event-function)
      (boundp 'interrupt-process-functions)
      (boundp 'suspend-tty-functions)
      (boundp 'resume-tty-functions)
      (boundp 'delete-frame-functions)
      (boundp 'after-make-frame-functions)
      (boundp 'before-make-frame-hook)
      (boundp 'focus-in-hook)
      (boundp 'focus-out-hook))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
