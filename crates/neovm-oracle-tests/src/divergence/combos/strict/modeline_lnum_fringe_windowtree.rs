//! Strict combo oracle probes, batch 19: mode-line/header/tab/frame-title
//! format defaults, display-line-numbers effect on body width, fringe
//! indicator settings, complex window tree shape after nested splits,
//! balance-windows result geometry, and selective-display line motion.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_f4_mode_line_format_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"%e\" mode-line-front-space (:propertize (\"\" mode-line-mule-info mode-line-client mode-line-modified mode-line-remote mode-line-window-dedicated) display (min-width (6.0))) mode-line-frame-identification mode-line-buffer-identification \"   \" mode-line-position (project-mode-line project-mode-line-format) (vc-mode vc-mode) \"  \" mode-line-modes mode-line-misc-info mode-line-end-spaces) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (default-value 'mode-line-format)
      (default-value 'header-line-format)
      (default-value 'tab-line-format))
"##,
        expect,
    );
}

#[test]
fn div_f4_frame_title_format_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((multiple-frames \"%b\" (\"\" \"%b - [EMACS-PRODUCT] at \" system-name)) (multiple-frames \"%b\" (\"\" \"%b - [EMACS-PRODUCT] at \" system-name)))""#
    ]];
    // INTENTIONAL product-branding divergence (Neomacs is not GNU Emacs):
    //   GNU Emacs: "%b - GNU Emacs at " system-name
    //   Neomacs:   "%b - NEO Emacs at " system-name   (see frame_vars.rs)
    // The title-bar literal must advertise "NEO Emacs", never "GNU Emacs". The
    // STRUCTURE (both frame-title-format and icon-title-format are the same
    // `multiple-frames' form embedding `system-name') is still locked to GNU:
    // the shared oracle normalizer canonicalizes the product name to
    // "[EMACS-PRODUCT]" on both engines, so the intentional brand difference is
    // ignored while every other part remains a parity assertion.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (default-value 'frame-title-format)
      (default-value 'icon-title-format))
"##,
        expect,
    );
}

#[test]
fn div_f4_display_line_numbers_body_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 t 80)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-dln*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b
          (insert "a\nb\nc\nd\n")
          (display-line-numbers-mode 1))
        (list (line-number-at-pos (point-max))
              (with-current-buffer b display-line-numbers)
              (window-body-width)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f4_fringe_indicator_settings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list fringe-mode
      (default-value 'fringe-mode)
      (default-value 'indicate-empty-lines)
      (default-value 'indicate-buffer-boundaries)
      (default-value 'overflow-newline-into-fringe)
      (default-value 'fringes-outside-margins))
"##,
        expect,
    );
}

#[test]
fn div_f4_window_tree_complex_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (3 nil \" *probe-wtc2*\" 40 12 (\" *probe-wtc2*\" \" *probe-wtc3*\" \" *probe-wtc1*\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-wtc1*"))
      (b2 (get-buffer-create " *probe-wtc2*"))
      (b3 (get-buffer-create " *probe-wtc3*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (let ((w2 (split-window nil nil 'right))
              (w3 nil))
          (set-window-buffer w2 b2)
          (select-window w2)
          (setq w3 (split-window nil nil 'below))
          (set-window-buffer w3 b3)
          (list (count-windows)
                (eq (window-parent w3) w2)
                (buffer-name (window-buffer (window-parent w2)))
                (window-total-width w2)
                (window-total-height w3)
                (mapcar (lambda (w) (buffer-name (window-buffer w)))
                        (window-list nil 'nomini)))))
    (mapc (lambda (x) (when (buffer-live-p x) (kill-buffer x))) (list b1 b2 b3))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f4_balance_windows_geometry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 17 12 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-bw1*"))
      (b2 (get-buffer-create " *probe-bw2*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (let ((w2 (split-window nil nil 'below)))
          (set-window-buffer w2 b2)
          (condition-case err
              (window-resize w2 5 nil nil nil)
            (error nil))
          (let ((before (list (window-total-height) (window-total-height w2))))
            (condition-case err (balance-windows) (error nil))
            (append before (list (window-total-height) (window-total-height w2))))))
    (mapc (lambda (x) (when (buffer-live-p x) (kill-buffer x))) (list b1 b2))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f4_selective_display_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 7 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\ralpha\nline3\n")
  (let ((c1 (count-lines (point-min) (point-max))))
    (setq-local selective-display t)
    (list c1
          (count-lines (point-min) (point-max))
          (progn (goto-char 1) (forward-line 1) (point))
          (progn (forward-line 1) (point)))))
"##,
        expect,
    );
}

#[test]
fn div_f4_display_table_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-display-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (buffer-display-table)
      (window-display-table)
      (default-value 'buffer-display-table)
      (window-parameter nil 'no-other-window))
"##,
        expect,
    );
}
