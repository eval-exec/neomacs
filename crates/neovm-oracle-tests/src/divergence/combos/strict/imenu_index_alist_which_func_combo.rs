//! Strict combo oracle probes, batch 225: imenu indexing. imenu--index-alist
//! over elisp defuns/defvars, imenu default-goto-function, and which-function
//! mode hooks.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_imenu_index_alist_elisp_defuns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'imenu)
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun probe-foo () \"doc\" (message \"foo\"))\n(defun probe-bar (x) (* x 2))\n(defvar probe-var 5)\n")
  (let ((idx (imenu--index-alist)))
    (list (consp idx)
          (assoc "*Rescan*" idx)
          (length idx)
          (assoc "probe-foo" idx)
          (assoc "probe-bar" idx))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function imenu--index-alist)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_imenu_rescan_and_subindex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'imenu)
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun probe-a ())\n(defun probe-b ())\n(defvar probe-c 1)\n(defvar probe-d 2)\n")
  (let ((idx (imenu--index-alist nil t)))
    (list (assq '*Rescan* idx)
          (sort (delq nil (mapcar (lambda (e) (car e)) idx)) #'string<))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function imenu--index-alist)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_which_function_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'imenu)
(with-current-buffer (get-buffer-create " *probe-wfm*")
  (emacs-lisp-mode)
  (insert "(defun probe-wfm-fn ())\n")
  (goto-char (point-min))
  (let ((result (list (imenu--in-alist "probe-wfm-fn" (imenu--index-alist))
                      (stringp (format-mode-line mode-name)))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function imenu--index-alist)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
