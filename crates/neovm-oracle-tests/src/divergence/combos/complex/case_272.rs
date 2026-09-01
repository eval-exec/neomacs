//! Complex combo batch 272 — `outline-minor-mode` / `imenu` actual
//! indexing / `semantic` parsing / `which-function` display / `page-
//! break-lines-mode` / `hi-lock` interactive highlighting.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx272_outline_minor_mode_with_custom_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-variable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'outline)
      (with-temp-buffer
        (insert "SECTION Alpha\nbody\nSUB Beta\nbody\nSECTION Gamma\nbody\n")
        (outline-minor-mode 1)
        (setq-local outline-regexp "^SECTION\\|SUB")
        (goto-char 1)
        (list (eq minor-mode 'outline-minor-mode)
              (outline-on-heading-p)
              (forward-line 1) (forward-line 1)
              (outline-on-heading-p))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_imenu_index_emacs_lisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun alpha () :a)\n(defun beta () :b)\n(defvar gamma 0)\n(defconst delta 1)\n")
      (emacs-lisp-mode)
      (let ((index (imenu--index-buffer)))
        (list (consp index)
              (assq 'Functions index)
              (assq 'Variables index))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_semantic_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'semantic)
          (fboundp 'semantic-mode)
          (boundp 'semantic-new-buffer-setup-functions))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_which_function_mode_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'which-func)
      (list (fboundp 'which-function-mode)
            (boundp 'which-func-modes)
            (boundp 'which-func-format)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_which_function_in_elisp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"neo-cx272-fn2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'which-func)
      (with-temp-buffer
        (insert "(defun neo-cx272-fn1 () :a)\n(defun neo-cx272-fn2 () :b)\n")
        (emacs-lisp-mode)
        (goto-char 30)
        (which-function)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_hi_lock_interactive_highlighting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'hi-lock)
      (list (fboundp 'highlight-regexp)
            (fboundp 'highlight-phrase)
            (fboundp 'highlight-lines-matching-regexp)
            (fboundp 'unhighlight-regexp)
            (boundp 'hi-lock-file-patterns)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_page_break_lines_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'page-break-lines)
          (fboundp 'page-break-lines-mode))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_outline_hide_show_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t \"* Heading 1\\nbody line one\\nbody line two\\n* Heading 2\\nbody three\\n\" \"* Heading 1\\nbody line one\\nbody line two\\n* Heading 2\\nbody three\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (outline-mode)
      (insert "* Heading 1\nbody line one\nbody line two\n* Heading 2\nbody three\n")
      (goto-char 1)
      (outline-hide-body)
      (let ((after-hide (buffer-string)))
        (outline-show-all)
        (let ((after-show (buffer-string)))
          (list (eq major-mode 'outline-mode)
                after-hide after-show))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_imenu_rescan_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun one () :one)\n")
      (emacs-lisp-mode)
      (let ((idx1 (imenu--index-buffer)))
        (insert "(defun two () :two)\n")
        (let ((idx2 (imenu--index-buffer)))
          (list (consp idx1)
                (consp idx2)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx272_outline_imenu_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'outline)
      (require 'imenu)
      (require 'hi-lock)
      (with-temp-buffer
        (buffer-enable-undo)
        (outline-mode)
        (insert "* Heading 1\nbody content\n* Heading 2\nmore content\n")
        (put-text-property 1 9 'face 'bold)
        (let ((m (set-marker (make-marker) 15))
              (ov (make-overlay 5 22)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 35)
          (goto-char 1)
          (let ((state (list (eq major-mode 'outline-mode)
                             (outline-on-heading-p)
                             (fboundp 'highlight-regexp)
                             (fboundp 'imenu)
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
    )
}
