//! Strict combo oracle probes, batch 44: more boundp sweeps across standard
//! variable categories — completion/minibuffer, font-lock/jit-lock, display/
//! redisplay, dired/process, and window/frame — to find more variables
//! Neomacs fails to define.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_i1_completion_minibuffer_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'completion-styles)
      (boundp 'completion-category-overrides)
      (boundp 'completion-ignore-case)
      (boundp 'read-buffer-completion-ignore-case)
      (boundp 'read-file-name-completion-ignore-case)
      (boundp 'completion-auto-help)
      (boundp 'completion-cycle-threshold)
      (boundp 'minibuffer-eldef-shorten-default))
"##,
        expect,
    );
}

#[test]
fn div_i1_fontlock_jitlock_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'font-lock-maximum-decoration)
      (boundp 'font-lock-support-mode)
      (boundp 'font-lock-defaults)
      (boundp 'jit-lock-contextually)
      (boundp 'jit-lock-context-time)
      (boundp 'jit-lock-defer-contextually)
      (boundp 'jit-lock-stealth-time)
      (boundp 'font-lock-verbose)
      (boundp 'font-lock-fontified))
"##,
        expect,
    );
}

#[test]
fn div_i1_display_redisplay_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'redisplay-dont-pause)
      (boundp 'fast-but-imprecise-scrolling)
      (boundp 'mode-line-default-help-echo)
      (boundp 'auto-window-vscroll)
      (boundp 'scroll-conservatively)
      (boundp 'scroll-margin)
      (boundp 'scroll-step)
      (boundp 'mouse-yank-at-point))
"##,
        expect,
    );
}

#[test]
fn div_i1_dired_process_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'dired-listing-switches)
      (boundp 'dired-dwim-target)
      (boundp 'dired-recursive-deletes)
      (boundp 'compile-command)
      (boundp 'compilation-read-command)
      (boundp 'compilation-scroll-output)
      (boundp 'executable-prefix-env))
"##,
        expect,
    );
}

#[test]
fn div_i1_window_frame_display_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'display-buffer-mark-dedicated)
      (boundp 'same-window-regexps)
      (boundp 'special-display-regexps)
      (boundp 'pop-up-windows)
      (boundp 'pop-up-frames)
      (boundp 'split-height-threshold)
      (boundp 'split-width-threshold)
      (boundp 'window-resize-pixelwise))
"##,
        expect,
    );
}
