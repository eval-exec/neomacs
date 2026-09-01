//! Deep combo: eval × eval-buffer × eval-region × eval-last-sexp ×
//! eval-print-last-sexp × marker × overlay × textprop × undo ×
//! buffer-local × narrow.
//!
//! Stresses eval operations with buffer state: evaluating Elisp in
//! various contexts while preserving markers, overlays, text properties,
//! and undo state. Eval is tricky because it executes arbitrary code
//! that can modify buffer state in complex ways.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eval_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable AAAA-BBBB-CCCC-DDDD)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-evb")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (eval-buffer)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp)
                           (get-text-property 12 'grp)
                           (get-text-property 18 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 6 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 16 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_eval_region_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-evr")))
    (with-current-buffer buf
      (insert "(+ 1 2)\n(* 3 4)\n(- 10 5)")
      (put-text-property 1 8 'code 'add)
      (put-text-property 9 16 'code 'mul)
      (put-text-property 17 25 'code 'sub)
      (let ((m1 (copy-marker 8 nil))
            (m2 (copy-marker 16 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (eval-region 1 8)
        (eval-region 9 16)
        (goto-char 20)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'code)
                           (get-text-property 9 'code)
                           (get-text-property 17 'code))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'code)
                                (get-text-property 9 'code)
                                (get-text-property 17 'code))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_eval_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 24 28)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-evn")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "AAAA\n(+ 1 2)\n(* 3 4)\nDDDD")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 14 'sect 'code1)
      (put-text-property 15 23 'sect 'code2)
      (put-text-property 24 28 'sect 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 14 t))
            (ov (make-overlay 6 23)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 23)
        (eval-region (point-min) (point-max))
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 24 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 15 'sect)
                                (get-text-property 24 'sect))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_eval_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-evbl")))
    (with-current-buffer buf
      (make-local-variable 'eval-local)
      (setq eval-local 'buffer-specific)
      (insert "(setq result (+ 1 2))")
      (put-text-property 1 21 'code 'expr)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 21)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (eval-buffer)
        (goto-char 10)
        (insert "XX")
        (let ((after (list (buffer-string)
                           eval-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'code))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                eval-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'code))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_eval_multi_sexpr_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function eval-sexp-add-defvar-result)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-evms")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(+ 1 2)\n(* 3 4)\n(- 10 5)\n(/ 20 4)")
      (put-text-property 1 8 'expr 'add)
      (put-text-property 9 16 'expr 'mul)
      (put-text-property 17 25 'expr 'sub)
      (put-text-property 26 34 'expr 'div)
      (let ((m1 (copy-marker 8 nil))
            (m2 (copy-marker 16 t))
            (m3 (copy-marker 25 nil))
            (ov (make-overlay 1 34)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        ;; Eval each sexp
        (goto-char 1)
        (eval-sexp-add-defvar-result (read (current-buffer)))
        (eval-sexp-add-defvar-result (read (current-buffer)))
        (eval-sexp-add-defvar-result (read (current-buffer)))
        (eval-sexp-add-defvar-result (read (current-buffer)))
        ;; Edit buffer
        (goto-char 20)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'expr)
                           (get-text-property 9 'expr)
                           (get-text-property 17 'expr)
                           (get-text-property 26 'expr))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'expr)
                                (get-text-property 9 'expr)
                                (get-text-property 17 'expr)
                                (get-text-property 26 'expr))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
