//! Complex combo batch 119 — `font-lock` / `jit-lock` fontification,
//! `syntax-propertize`, font-lock-keywords/multiline, default-text-properties.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx119_font_lock_keywords_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t t ((\"alpha\" \"beta\" \"gamma\") quote word) (\"[0-9]+\" . font-lock-constant-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((kw '((("alpha" "beta" "gamma") . 'word)
                ("[0-9]+" . font-lock-constant-face)
                ("\\bfunction\\b" (0 font-lock-keyword-face))
                ("\\(\\w+\\)\\b" (1 font-lock-variable-name-face))
                ("\\<LOOP\\>" . font-lock-builtin-face))))
      (list (consp kw)
            (> (length kw) 0)
            (car kw)
            (cadr kw)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_font_lock_default_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (consp font-lock-keywords)
          (boundp 'font-lock-defaults)
          (boundp 'font-lock-maximum-decoration)
          (boundp 'font-lock-verbose))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_font_lock_fontify_buffer_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "def hello():\n    return 42\n")
      (let ((font-lock-defaults '((("def" "return") . font-lock-keyword-face))))
        (font-lock-mode 1)
        (font-lock-fontify-buffer)
        (list (get-text-property 1 'face)
              (get-text-property 5 'face)
              (get-text-property 15 'face)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_jit_lock_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'jit-lock)
      (list (fboundp 'jit-lock-register)
            (fboundp 'jit-lock-unregister)
            (boundp 'jit-lock-chunk-size)
            (boundp 'jit-lock-stealth-time)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_syntax_propertize_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'syntax-propertize)
          (fboundp 'syntax-propertize-extend-region)
          (boundp 'syntax-propertize-function)
          (boundp 'syntax-propertize-rules))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_default_text_properties_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (face bold) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((before (default-value 'default-text-properties)))
    (setq-local default-text-properties '(face bold))
    (insert "new text")
    (list before
          (buffer-local-value 'default-text-properties (current-buffer))
          (text-properties-at 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_font_lock_add_keywords_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored invalid-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "hello world foo bar baz")
      (font-lock-add-keywords nil
                              '((("hello") . font-lock-constant-face)
                                (("world") . font-lock-warning-face)))
      (font-lock-mode 1)
      (font-lock-fontify-buffer)
      (list (get-text-property 1 'face)
            (get-text-property 7 'face)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_font_lock_remove_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "hello world")
      (font-lock-add-keywords nil '((("hello") . font-lock-constant-face)))
      (font-lock-remove-keywords nil '((("hello") . font-lock-constant-face)))
      (list (consp font-lock-keywords)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_syntax_ppss_basic_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 1 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo ()\n  \"docstring\"\n  (body))")
  (let ((p1 (syntax-ppss 1))
        (p2 (syntax-ppss 15))
        (p3 (syntax-ppss 30)))
    (list (car p1)
          (car p2)
          (car p3)
          (nth 3 p2)
          (nth 8 p2))))
"##,
        expect,
    );
}

#[test]
fn div_cx119_pre_post_strings_overlay() {
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
    );
}

#[test]
fn div_cx119_default_text_preset_for_new_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (setq default-text-properties '(cat :default))
  (insert "new content here")
  (list (text-properties-at 1)
        (text-properties-at 5)
        (text-properties-at 10)))
"##,
        expect,
    );
}

#[test]
fn div_cx119_font_lock_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Font-lock mega test buffer content here")
      (put-text-property 1 6 'face 'bold)
      (font-lock-add-keywords nil '((("test") . font-lock-constant-face)))
      (let ((m (set-marker (make-marker) 12))
            (ov (make-overlay 4 18)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 25)
        (let ((state (list (consp font-lock-keywords)
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
    );
}
