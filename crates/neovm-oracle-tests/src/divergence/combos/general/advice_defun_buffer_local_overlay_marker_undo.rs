//! Deep combo: advice × buffer-local × overlay × marker × undo ×
//! text-prop × defun × fset × symbol-function × macroexpand.
//!
//! Stresses advice system interaction with buffer state: advising
//! functions that modify buffers, advice that captures buffer-local
//! state, and advice removal/restoration during undo. The advice
//! system is particularly tricky in a Rust rewrite because it involves
//! function cell manipulation, symbol indirection, and dynamic dispatch.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_advice_around_defun_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--test-func ()
    (insert "INSERTED"))
  (defun combo--advice (orig-fun &rest args)
    (let ((before (buffer-string)))
      (apply orig-fun args)
      (list before (buffer-string))))
  (advice-add 'combo--test-func :around 'combo--advice)
  (let ((buf (generate-new-buffer " combo-adv")))
    (with-current-buffer buf
      (make-local-variable 'my-local)
      (setq my-local 'advice-test)
      (insert "ORIGINAL-TEXT-HERE")
      (let ((m1 (copy-marker 9 nil))
            (m2 (copy-marker 13 t))
            (ov (make-overlay 1 18)))
        (overlay-put ov 'scope 'all)
        (put-text-property 1 9 'sect 'orig)
        (put-text-property 10 19 'sect 'text)
        (undo-boundary)
        (goto-char 9)
        (let ((result (combo--test-func)))
          (let ((after-advice (list result
                                    my-local
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'sect))))
            (primitive-undo 1 buffer-undo-list)
            (let ((after-undo (list (buffer-string)
                                    my-local
                                    (marker-position m1)
                                    (marker-position m2)
                                    (get-text-property 1 'sect))))
              (advice-remove 'combo--test-func 'combo--advice)
              (kill-buffer buf)
              (list after-advice after-undo)))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_before_defun_textprop_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--insert-func ()
    (insert "NEW"))
  (defun combo--before-advice (&rest _)
    (put-text-property (point-min) (point-max) 'advised t))
  (advice-add 'combo--insert-func :before 'combo--before-advice)
  (let ((buf (generate-new-buffer " combo-adbef")))
    (with-current-buffer buf
      (insert "HELLO")
      (let ((m (copy-marker 3 nil))
            (ov (make-overlay 1 6)))
        (overlay-put ov 'kind 'test)
        (undo-boundary)
        (goto-char 3)
        (combo--insert-func)
        (let ((after (list (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'advised)
                           (get-text-property 1 'kind))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'advised)
                                (get-text-property 1 'kind))))
            (advice-remove 'combo--insert-func 'combo--before-advice)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_override_defun_with_buffer_local_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--orig-func ()
    (insert "ORIGINAL"))
  (defun combo--override-func ()
    (insert "REPLACED"))
  (advice-add 'combo--orig-func :override 'combo--override-func)
  (let ((buf (generate-new-buffer " combo-adovr")))
    (with-current-buffer buf
      (make-local-variable 'override-local)
      (setq override-local 'active)
      (insert "PREFIX-SUFFIX")
      (let ((m (copy-marker 7 nil))
            (ov (make-overlay 1 13)))
        (overlay-put ov 'span 'full)
        (put-text-property 1 7 'part 'prefix)
        (put-text-property 8 14 'part 'suffix)
        (undo-boundary)
        (goto-char 7)
        (combo--orig-func)
        (let ((after (list (buffer-string)
                           override-local
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 7 'part)
                           (get-text-property 8 'part))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                override-local
                                (marker-position m)
                                (get-text-property 1 'part)
                                (get-text-property 8 'part))))
            (advice-remove 'combo--orig-func 'combo--override-func)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_defmacro_expansion_with_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro combo--def-advised (name body)
    `(progn
       (defun ,name () ,body)
       (defun ,(intern (concat (symbol-name name) "-advice")) (orig &rest args)
         (let ((pre (point)))
           (apply orig args)
           (list pre (point))))
       (advice-add ',name :around
                   ',(intern (concat (symbol-name name) "-advice")))))
  (combo--def-advised combo--macro-func (insert "MACRO-GEN"))
  (let ((buf (generate-new-buffer " combo-madv")))
    (with-current-buffer buf
      (insert "BEFORE-AFTER")
      (let ((m (copy-marker 7 nil))
            (ov (make-overlay 1 13)))
        (overlay-put ov 'range 'all)
        (undo-boundary)
        (goto-char 7)
        (let ((result (combo--macro-func)))
          (let ((after (list (buffer-string)
                             result
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'range))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m)
                                  (overlay-start ov) (overlay-end ov))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_chain_multiple_on_same_defun_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--chain-func ()
    (insert "-MIDDLE-"))
  (defun combo--advice1 (orig &rest args)
    (insert "<A1>")
    (apply orig args)
    (insert "</A1>"))
  (defun combo--advice2 (orig &rest args)
    (insert "<A2>")
    (apply orig args)
    (insert "</A2>"))
  (advice-add 'combo--chain-func :around 'combo--advice1)
  (advice-add 'combo--chain-func :around 'combo--advice2)
  (let ((buf (generate-new-buffer " combo-achain")))
    (with-current-buffer buf
      (insert "START-END")
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 6 t))
            (ov (make-overlay 1 10)))
        (overlay-put ov 'scope 'all)
        (put-text-property 1 6 'part 'start)
        (put-text-property 7 10 'part 'end)
        (undo-boundary)
        (goto-char 6)
        (combo--chain-func)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'part))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'part)
                                (get-text-property 7 'part))))
            (advice-remove 'combo--chain-func 'combo--advice1)
            (advice-remove 'combo--chain-func 'combo--advice2)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
