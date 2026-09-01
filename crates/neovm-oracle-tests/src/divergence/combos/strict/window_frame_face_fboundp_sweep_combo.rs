//! Strict combo oracle probes, batch 282: window / frame / face fboundp sweep.
//! Any nil-in-Neomacs/t-in-GNU is a missing-function bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_config_state_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'current-window-configuration)
      (fboundp 'set-window-configuration)
      (fboundp 'window-configuration-p)
      (fboundp 'window-configuration-frame)
      (fboundp 'compare-window-configurations)
      (fboundp 'window-state-get)
      (fboundp 'window-state-put)
      (fboundp 'window-live-p)
      (fboundp 'window-valid-p)
      (fboundp 'window-at)
      (fboundp 'window-edges)
      (fboundp 'window-body-edges)
      (fboundp 'window-inside-edges)
      (fboundp 'window-pixel-edges)
      (fboundp 'window-absolute-pixel-edges)
      (fboundp 'window-tree)
      (fboundp 'window-parent)
      (fboundp 'window-child)
      (fboundp 'window-next-sibling)
      (fboundp 'window-prev-sibling)
      (fboundp 'walk-windows)
      (fboundp 'get-buffer-window))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_frame_terminal_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'make-frame)
      (fboundp 'select-frame)
      (fboundp 'selected-frame)
      (fboundp 'frame-live-p)
      (fboundp 'frame-visible-p)
      (fboundp 'frame-parameters)
      (fboundp 'frame-parameter)
      (fboundp 'modify-frame-parameters)
      (fboundp 'set-frame-parameter)
      (fboundp 'frame-root-window)
      (fboundp 'frame-selected-window)
      (fboundp 'next-frame)
      (fboundp 'previous-frame)
      (fboundp 'terminal-live-p)
      (fboundp 'terminal-name)
      (fboundp 'delete-terminal)
      (fboundp 'frame-terminal)
      (fboundp 'tty-top-level)
      (fboundp 'suspend-emacs)
      (fboundp 'suspend-tty))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_face_color_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'make-face)
      (fboundp 'facep)
      (fboundp 'copy-face)
      (fboundp 'set-face-attribute)
      (fboundp 'face-attribute)
      (fboundp 'face-attributes-as-vector)
      (fboundp 'face-all-attributes)
      (fboundp 'face-foreground)
      (fboundp 'face-background)
      (fboundp 'face-font)
      (fboundp 'face-underline-p)
      (fboundp 'face-inverse-video-p)
      (fboundp 'color-values)
      (fboundp 'color-defined-p)
      (fboundp 'color-gray-p)
      (fboundp 'color-supported-p)
      (fboundp 'defined-colors)
      (fboundp 'color-rgb-to-hex)
      (fboundp 'color-name-to-rgb)
      (fboundp 'color-complement)
      (fboundp 'color-distance))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
