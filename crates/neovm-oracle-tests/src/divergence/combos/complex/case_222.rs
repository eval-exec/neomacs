//! Complex combo batch 222 — `uniquify` / `reveal-mode` / `hl-line` /
//! `hl-todo` / `rainbow-delimiters` / `rainbow-mode` / `diff-hl` /
//! `delsel` / `cua-mode` availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx222_uniquify_buffer_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'uniquify)
      (list (fboundp 'uniquify-rationalize-file-buffer-names)
            (boundp 'uniquify-buffer-name-style)
            (boundp 'uniquify-separator)
            (boundp 'uniquify-after-kill-buffer-p)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_reveal_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'reveal)
      (list (fboundp 'reveal-mode)
            (boundp 'reveal-auto-hide)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_hl_line_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'hl-line-mode)
      (fboundp 'global-hl-line-mode)
      (boundp 'hl-line-sticky-flag)
      (boundp 'hl-line-overlay-priority))
"##,
        expect,
    );
}

#[test]
fn div_cx222_hl_todo_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'hl-todo)
      (list (fboundp 'hl-todo-mode)
            (boundp 'hl-todo-keyword-faces)
            (boundp 'hl-todo-color-priorities)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_rainbow_delimiters_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'rainbow-delimiters)
          (fboundp 'rainbow-delimiters-mode)
          (boundp 'rainbow-delimiters-max-face-count))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_rainbow_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'rainbow-mode)
      (list (fboundp 'rainbow-mode)
            (boundp 'rainbow-hexadecimal-colors-font-lock-keywords)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_delsel_delete_selection_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'delsel)
      (list (fboundp 'delete-selection-mode)
            (boundp 'delete-selection-save-to-register)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_cua_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cua-base)
      (list (fboundp 'cua-mode)
            (boundp 'cua-enable-cua-keys)
            (boundp 'cua-rectangle-mark-key)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_diff_hl_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'diff-hl)
          (fboundp 'diff-hl-mode)
          (boundp 'diff-hl-side))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx222_highlight_modes_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'hl-line)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Highlight modes mega test buffer content")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (fboundp 'hl-line-mode)
                             (boundp 'hl-line-sticky-flag)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
