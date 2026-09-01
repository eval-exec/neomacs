//! Deep combo: advice × add-advice × remove-advice × advice-mapc ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses advice system with buffer state: adding/removing advice
//! on functions that modify buffers, advice that captures buffer-local
//! state, and advice interaction with markers, overlays, text properties,
//! and undo. The advice system is complex because it involves function
//! cell manipulation, symbol indirection, and dynamic dispatch that must
//! interact correctly with the buffer's edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_advice_add_remove_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--adv-target () "target")
  (defun combo--adv-before (&rest _) (insert "<BEFORE>"))
  (defun combo--adv-after (&rest _) (insert "<AFTER>"))
  (let ((buf (generate-new-buffer " combo-adv")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (advice-add 'combo--adv-target :before 'combo--adv-before)
        (advice-add 'combo--adv-target :after 'combo--adv-after)
        (undo-boundary)
        (goto-char 5)
        (combo--adv-target)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (advice--p (advice--symbol-function 'combo--adv-target)))))
          (advice-remove 'combo--adv-target 'combo--adv-before)
          (advice-remove 'combo--adv-target 'combo--adv-after)
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (fmakunbound 'combo--adv-target)
            (fmakunbound 'combo--adv-before)
            (fmakunbound 'combo--adv-after)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_capture_buflocal_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--adv-bl-target () "target")
  (defun combo--adv-bl-before (&rest _)
    (insert (format "<%s>" combo--adv-bl-local)))
  (let ((buf (generate-new-buffer " combo-advbl")))
    (with-current-buffer buf
      (make-local-variable 'combo--adv-bl-local)
      (setq combo--adv-bl-local 'buf-local-val)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (advice-add 'combo--adv-bl-target :before 'combo--adv-bl-before)
        (undo-boundary)
        (goto-char 5)
        (combo--adv-bl-target)
        (let ((after (list (buffer-string)
                           combo--adv-bl-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (advice-remove 'combo--adv-bl-target 'combo--adv-bl-before)
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                combo--adv-bl-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (fmakunbound 'combo--adv-bl-target)
            (fmakunbound 'combo--adv-bl-before)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--adv-nar-target () "target")
  (defun combo--adv-nar-before (&rest _) (insert "XX-"))
  (let ((buf (generate-new-buffer " combo-advnar")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (advice-add 'combo--adv-nar-target :before 'combo--adv-nar-before)
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (combo--adv-nar-target)
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (advice-remove 'combo--adv-nar-target 'combo--adv-nar-before)
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (fmakunbound 'combo--adv-nar-target)
            (fmakunbound 'combo--adv-nar-before)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_chain_multiple_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--adv-chain-target () "target")
  (defun combo--adv-a (&rest _) (insert "<A>"))
  (defun combo--adv-b (&rest _) (insert "<B>"))
  (let ((buf (generate-new-buffer " combo-advch")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (advice-add 'combo--adv-chain-target :before 'combo--adv-a)
        (advice-add 'combo--adv-chain-target :before 'combo--adv-b)
        (undo-boundary)
        (goto-char 5)
        (combo--adv-chain-target)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (advice-remove 'combo--adv-chain-target 'combo--adv-a)
          (advice-remove 'combo--adv-chain-target 'combo--adv-b)
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (fmakunbound 'combo--adv-chain-target)
            (fmakunbound 'combo--adv-a)
            (fmakunbound 'combo--adv-b)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_advice_replace_symbol_function_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun combo--adv-rep-target () (insert "ORIGINAL"))
  (let ((buf (generate-new-buffer " combo-advrep")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (combo--adv-rep-target)
        (let ((after1 (list (buffer-string)
                            (marker-position m1)
                            (marker-position m2)
                            (overlay-start ov) (overlay-end ov)
                            (get-text-property 1 'zone)
                            (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (fmakunbound 'combo--adv-rep-target)
            (kill-buffer buf)
            (list after1 restored))))))) "#,
        expect,
    );
}
