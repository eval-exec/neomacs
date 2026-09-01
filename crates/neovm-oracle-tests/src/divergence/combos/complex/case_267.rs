//! Complex combo batch 267 — `font-lock` keywords deep: anchored matcher,
//! `eval` form, `prepend`/`append`/`keep` override, multiline, and
//! `jit-lock-contextually`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx267_font_lock_keywords_anchored_matcher() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (\"\\\\bdef\\\\b\" (0 font-lock-keyword-face) (\"\\\\b\\\\(\\\\w+\\\\)\\\\b\" (1 font-lock-function-name-face))) \"\\\\bdef\\\\b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((kw '(("\\bdef\\b"
                 (0 font-lock-keyword-face)
                 ("\\b\\(\\w+\\)\\b"
                  (1 font-lock-function-name-face))))))
      (list (consp kw)
            (car kw)
            (caar kw)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_font_lock_keywords_eval_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((kw '(("\\bvar\\b"
                 (0 (if (> (current-column) 10)
                        'font-lock-warning-face
                      'font-lock-keyword-face))))))
      (list (consp kw)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_font_lock_keywords_override_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"pattern\" 0 'face prepend) (\"pattern\" 0 'face append) (\"pattern\" 0 'face keep) (\"pattern\" 0 'face override))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((kw-prepend '(("pattern" 0 'face prepend)))
          (kw-append '(("pattern" 0 'face append)))
          (kw-keep '(("pattern" 0 'face keep)))
          (kw-override '(("pattern" 0 'face override))))
      (list (car kw-prepend)
            (car kw-append)
            (car kw-keep)
            (car kw-override)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_font_lock_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "line one\nline two\nline three\n")
      (put-text-property 1 25 'font-lock-multiline t)
      (list (get-text-property 1 'font-lock-multiline)
            (get-text-property 15 'font-lock-multiline)
            (get-text-property 26 'font-lock-multiline)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_jit_lock_contextually() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'jit-lock)
      (list (fboundp 'jit-lock-register)
            (fboundp 'jit-lock-unregister)
            (boundp 'jit-lock-chunk-size)
            (boundp 'jit-lock-defer-time)
            (boundp 'jit-lock-stealth-time)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_syntax_propertize_with_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'syntax-propertize)
          (fboundp 'syntax-propertize-extend-region)
          (boundp 'syntax-propertize-function)
          (fboundp 'syntax-propertize-rules))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_font_lock_default_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (consp font-lock-keywords)
      (boundp 'font-lock-defaults)
      (boundp 'font-lock-maximum-decoration)
      (boundp 'font-lock-verbose))
"##,
        expect,
    )
}

#[test]
fn div_cx267_font_lock_add_keywords_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored invalid-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "hello world foo bar baz")
      (font-lock-add-keywords nil '((("\\bhello\\b") . font-lock-constant-face)
                                     (("\\bworld\\b") . font-lock-warning-face)))
      (font-lock-mode 1)
      (font-lock-fontify-buffer)
      (list (get-text-property 1 'face)
            (get-text-property 7 'face)
            (consp font-lock-keywords)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_font_lock_remove_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "hello world")
      (font-lock-add-keywords nil '((("\\bhello\\b") . font-lock-constant-face)))
      (font-lock-remove-keywords nil '((("\\bhello\\b") . font-lock-constant-face)))
      (list (consp font-lock-keywords)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx267_font_lock_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Font-lock mega test buffer content with keywords")
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
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
