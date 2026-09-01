//! Strict combo oracle probes, batch 235: structural navigation. forward/back-
//! ward-sexp, down-list, up-list, backward-up-list, beginning/end-of-defun, and
//! forward-list matching-pair traversal.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_forward_backward_sexp_list_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b c) d) (e f)")
  (goto-char 1)
  (let ((after-sexp (progn (forward-sexp) (point)))
        (back (progn (backward-sexp) (point)))
        (fwd-list (progn (forward-list) (point)))
        (back-list (progn (backward-list) (point))))
    (list after-sexp back fwd-list back-list)))
"##;
    let expect = expect_test::expect![[r#""OK (12 1 12 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_down_up_list_backward_up_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b (c d) e) f)")
  (goto-char 1)
  (let ((down1 (progn (down-list) (point)))
        (down2 (progn (down-list) (point)))
        (up1 (progn (up-list) (point)))
        (bup (progn (backward-up-list) (point))))
    (list down1 down2 up1 bup)))
"##;
    let expect = expect_test::expect![[r#""OK (2 5 15 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_beginning_end_of_defun_top_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun first () (body))\n\n(defun second (x)\n  (let ((y (* x 2)))\n    (list x y)))\n")
  (goto-char 30)
  (let ((beg-defun (progn (beginning-of-defun) (point)))
        (end-defun (progn (end-of-defun) (point))))
    (goto-char (point-min))
    (let ((next-defun (progn (beginning-of-defun -1) (point))))
      (list beg-defun end-defun next-defun))))
"##;
    let expect = expect_test::expect![[r#""OK (26 82 26)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
