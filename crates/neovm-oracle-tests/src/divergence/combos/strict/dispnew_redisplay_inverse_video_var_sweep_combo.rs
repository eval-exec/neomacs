//! Strict combo oracle probes, batch 279: dispnew / redisplay CORE variable
//! sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_baud_rate_inverse_video_redisplay_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'baud-rate)
      (boundp 'inverse-video)
      (boundp 'no-redraw-on-reenter)
      (boundp 'redisplay-dont-pause)
      (boundp 'redisplay-skip-fontification-on-input)
      (boundp 'window-screen-lines)
      (boundp 'mode-line-default-help-echo)
      (boundp 'cursor-in-non-selected-windows)
      (boundp 'mode-line-in-non-selected-windows)
      (boundp 'glyph-table)
      (boundp 'face-remapping-alist)
      (boundp 'window-screen-lines))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil t nil t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_redisplay_force_mode_line_update_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'redisplay-end-trigger-functions)
      (boundp 'window-scroll-functions)
      (boundp 'window-size-change-functions)
      (boundp 'window-configuration-change-hook)
      (boundp 'redisplay-adhoc-scroll-in-dedicated-windows)
      (boundp 'fast-but-imprecise-scrolling)
      (boundp 'auto-window-vscroll)
      (boundp 'mouse-wheel-follow-mouse)
      (boundp 'mouse-wheel-scroll-amount)
      (boundp 'mouse-wheel-progressive-speed)
      (boundp 'mouse-wheel-tilt-scroll)
      (boundp 'wheel-up/down-event))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t t nil t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_terminal_coding_display_time_str_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'terminal-coding-system)
      (boundp 'keyboard-coding-system)
      (boundp 'default-terminal-coding-system)
      (boundp 'display-time-string)
      (boundp 'display-time-update-interval)
      (boundp 'mode-line-format)
      (boundp 'global-mode-string)
      (boundp 'mode-line-misc-info)
      (boundp 'mode-line-buffer-identification)
      (boundp 'minor-mode-alist)
      (boundp 'minor-mode-list)
      (boundp 'emacs-mode-string))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t nil nil t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
