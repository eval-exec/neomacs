//! Complex combo batch 361 — `font-lock`/`jit-lock`/`syntax-propertize`
//! ultimate: font-lock keywords with all forms (anchored/eval/override/
//! prepend/append/keep), jit-lock register/unregister, syntax-propertize.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx361_font_lock_keywords_all_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((kw '(("\\bdef\\b" . font-lock-keyword-face)
                ("\\b\\w+\\b" (0 font-lock-variable-name-face))
                ("\\b[0-9]+\\b" (0 font-lock-constant-face t))
                ("\\bfunc\\b" (0 (if (> (current-column) 10)
                                     'font-lock-warning-face
                                   'font-lock-keyword-face))))))
      (list (consp kw) (length kw)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx361_font_lock_mode_buffer_fontification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored invalid-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "def hello():\n    return 42\n")
      (font-lock-add-keywords nil '((("\\bdef\\b") . font-lock-keyword-face)
                                     (("\\breturn\\b") . font-lock-keyword-face)))
      (font-lock-mode 1)
      (font-lock-fontify-buffer)
      (list (eq font-lock-mode t)
            (get-text-property 1 'face)
            (get-text-property 5 'face)
            (consp font-lock-keywords)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx361_font_lock_remove_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "hello world foo bar baz")
      (font-lock-add-keywords nil '((("\\bhello\\b") . font-lock-constant-face)))
      (let ((before (length font-lock-keywords)))
        (font-lock-remove-keywords nil '((("\\bhello\\b") . font-lock-constant-face)))
        (list before (length font-lock-keywords))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx361_jit_lock_register_unregister() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'jit-lock)
      (let ((my-fn (lambda (beg end) nil)))
        (jit-lock-register my-fn)
        (let ((registered t))
          (jit-lock-unregister my-fn)
          (list (fboundp 'jit-lock-register)
                (fboundp 'jit-lock-unregister)
                (boundp 'jit-lock-chunk-size)
                (boundp 'jit-lock-defer-time)
                (boundp 'jit-lock-stealth-time)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx361_syntax_propertize_rules_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'syntax-propertize)
      (fboundp 'syntax-propertize-extend-region)
      (boundp 'syntax-propertize-function)
      (fboundp 'syntax-propertize-rules))
"##,
        expect,
    )
}

#[test]
fn div_cx361_font_lock_multiline_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line one\nline two\nline three\n")
  (put-text-property 1 25 'font-lock-multiline t)
  (list (get-text-property 1 'font-lock-multiline)
        (get-text-property 15 'font-lock-multiline)
        (get-text-property 26 'font-lock-multiline)))
"##,
        expect,
    )
}

#[test]
fn div_cx361_font_lock_defaults_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (consp font-lock-keywords)
      (boundp 'font-lock-defaults)
      (boundp 'font-lock-maximum-decoration)
      (boundp 'font-lock-verbose)
      (boundp 'font-lock-keywords-only))
"##,
        expect,
    )
}

#[test]
fn div_cx361_syntax_ppss_cached_with_font_lock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 nil nil nil nil nil 0 nil nil nil nil) (1 1 12 nil nil nil 0 nil nil (1) nil) (2 25 28 nil nil nil 0 nil nil (1 25) nil) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo ()\n  \"doc\"\n  (+ 1 2))")
  (list (syntax-ppss 1)
        (syntax-ppss 15)
        (syntax-ppss 30)
        (nth 3 (syntax-ppss 25))
        (nth 8 (syntax-ppss 25))))
"##,
        expect,
    )
}

#[test]
fn div_cx361_pre_post_strings_overlay_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"before content afte\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before content after")
  (let ((ov (make-overlay 7 14)))
    (overlay-put ov 'before-string (propertize "PRE" 'face 'bold))
    (overlay-put ov 'after-string (propertize "POST" 'face 'italic)))
  (list (buffer-substring 1 20)
        (length (overlays-in 1 20))))
"##,
        expect,
    )
}

#[test]
fn div_cx361_font_lock_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Font-lock mega test buffer content with keywords here")
      (put-text-property 1 10 'face 'bold)
      (font-lock-add-keywords nil '((("\\bmega\\b") . font-lock-warning-face)
                                     (("\\btest\\b") . font-lock-constant-face)))
      (let ((m (set-marker (make-marker) 12))
            (ov (make-overlay 5 25)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 30)
        (let ((state (list (consp font-lock-keywords)
                           (boundp 'font-lock-defaults)
                           (fboundp 'jit-lock-register)
                           (fboundp 'syntax-propertize)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen()
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
