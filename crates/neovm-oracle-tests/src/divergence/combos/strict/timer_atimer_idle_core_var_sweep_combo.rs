//! Strict combo oracle probes, batch 273: timer / atimer / idle CORE variable
//! sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_timer_list_idle_list_max_repeats_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'timer-list)
      (boundp 'timer-idle-list)
      (boundp 'timer-max-repeats)
      (boundp 'timer-debug)
      (boundp 'timer-precision)
      (boundp 'blink-cursor-mode)
      (boundp 'blink-cursor-interval)
      (boundp 'blink-cursor-blinks)
      (boundp 'blink-cursor-delay)
      (boundp 'blink-paren-function)
      (boundp 'show-paren-style)
      (boundp 'show-paren-delay))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil nil t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_idle_delay_repeat_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'idle-update-delay)
      (boundp 'idle-delay)
      (boundp 'auto-save-timeout)
      (boundp 'auto-save-visited-interval)
      (boundp 'display-time-interval)
      (boundp 'display-time-default-load-average)
      (boundp 'display-time-mail-face)
      (boundp 'display-time-mail-function)
      (boundp 'display-time-string-forms)
      (boundp 'display-time-24hr-format)
      (boundp 'line-number-display-limit)
      (boundp 'line-number-display-limit-width))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t t nil nil nil nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_revert_autorevert_tail_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'auto-revert-interval)
      (boundp 'auto-revert-stop-on-polling-error)
      (boundp 'auto-revert-remote-files)
      (boundp 'auto-revert-check-vc-info)
      (boundp 'auto-revert-mode)
      (boundp 'global-auto-revert-mode)
      (boundp 'tail-mode)
      (boundp 'tail-volatile-max-size)
      (boundp 'revert-without-query)
      (boundp 'revert-buffer-function)
      (boundp 'revert-buffer-preserve-modes)
      (boundp 'before-revert-hook))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil t nil nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
