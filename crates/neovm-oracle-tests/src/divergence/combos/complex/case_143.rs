//! Complex combo batch 143 — `imenu` / `speedbar` / `cscope` / `gtag` /
//! `ebrowse` indexing of buffer symbols.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx143_imenu_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'imenu)
      (list (fboundp 'imenu)
            (fboundp 'imenu--index-buffer)
            (boundp 'imenu-create-index-function)
            (boundp 'imenu-sort-function)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_speedbar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'speedbar)
      (list (fboundp 'speedbar)
            (boundp 'speedbar-frame)
            (boundp 'speedbar-update-speed)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_cscope_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'xcscope)
      (list (fboundp 'cscope-find-global-definition)
            (boundp 'cscope-initial-directory)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_ebrowse_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ebrowse)
      (list (fboundp 'ebrowse-tree-buffer-p)
            (boundp 'ebrowse-view-file-hook)
            (boundp 'ebrowse-search-path)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_imenu_basic_emacs_lisp_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun foo () :foo)\n(defun bar () :bar)\n(defvar baz 0)\n")
      (emacs-lisp-mode)
      (let ((index (imenu--index-buffer)))
        (list (consp index)
              (assq 'Variables index)
              (assq 'Functions index))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_imenu_jump_to_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 21 \"(defun beta () :b)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun alpha () :a)\n(defun beta () :b)\n(defun gamma () :c)\n")
      (emacs-lisp-mode)
      (let ((pos (condition-case err
                     (imenu "beta")
                   (error :err))))
        (list pos (point) (buffer-substring (line-beginning-position) (line-end-position)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_which_func_mode_availability() {
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
    );
}

#[test]
fn div_cx143_imenu_add_to_menubar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'imenu-add-to-menubar)
          (fboundp 'imenu-add-menubar-index)
          (boundp 'imenu-menubar-modified-list))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_gtags_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'ggtags)
          (fboundp 'ggtags-find-tag)
          (boundp 'ggtags-executable-directory))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_imenu_rescan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun one () :one)\n")
      (emacs-lisp-mode)
      (let ((index-1 (imenu--index-buffer)))
        (insert "(defun two () :two)\n")
        (let ((index-2 (imenu--index-buffer)))
          (list (consp index-1)
                (consp index-2)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_which_function_in_emacs_lisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun neo-cx143-fn1 () :a)\n(defun neo-cx143-fn2 () :b)\n")
      (emacs-lisp-mode)
      (goto-char 30)
      (which-function))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx143_imenu_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "(defun alpha () :a)\n(defun beta () :b)\n")
      (emacs-lisp-mode)
      (put-text-property 1 8 'face 'bold)
      (let ((m (set-marker (make-marker) 12))
            (ov (make-overlay 4 18)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 30)
        (let ((index (imenu--index-buffer)))
          (let ((state (list index
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
